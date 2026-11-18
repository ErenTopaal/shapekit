//! Turning run outputs into findings: every failure becomes an occurrence, every occurrence
//! is matched against the triage, and each finding is summarised from what it matched.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shapekit_harness::compare::Problem;
use shapekit_harness::run::EntryRow;
use shapekit_manifest::Manifest;

use crate::inputs::{self, LintFinding, MatrixView};
use crate::triage::{
    AnswerMatch, ConformanceMatch, FindingDef, HarnessMatch, LintMatch, Severity, Status, Triage,
    glob,
};

pub const FINDINGS_SCHEMA: u32 = 1;

/// One failure that needs a finding.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Occurrence {
    /// `harness`, `conformance` or `lint` for a failure; `answer` for an answer recorded on
    /// an implementation-defined entry, which is evidence but not a failure.
    pub source: String,
    /// The target or SDK it was observed on, with its version: `wallet-backend@e967ce8b...`,
    /// `go@v0.7.1`.
    pub on: String,
    /// The corpus entry, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    /// Network variant of a harness entry, or the source location of a linter finding.
    pub at: String,
    /// What failed, in one line.
    pub what: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputRef {
    pub kind: String,
    pub path: String,
    pub sha256: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FindingRecord {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short: Option<String>,
    pub component: String,
    pub severity: Severity,
    pub caps: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimal_entry: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reproduce: Option<String>,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
    pub cause: String,
    pub siblings: Vec<String>,
    pub related: Vec<String>,
    /// Distinct corpus entries the finding was observed on.
    pub entries: Vec<String>,
    /// How to produce the minimal entry's input again on a local core: `recipe:<file>` for an
    /// entry recorded from a recipe IR file, `chain:<chain id>` for one recorded by a recipe
    /// program (`shapekit gen chains --family <family>`). Absent when the input is
    /// hand-built, a core golden or a capture from a public network.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe: Option<String>,
    /// Per SDK or target, the versions the inputs ran and the ones that fail.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affected: Vec<Affected>,
    /// SDK versions of other SDKs that also fail one of the finding's entries, whichever
    /// finding takes them, `<sdk>@<version>`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub other_sdks: Vec<String>,
    pub occurrences: Vec<Occurrence>,
}

/// The versions of one SDK or target a finding's inputs ran, in the order the inputs list
/// them (oldest first for SDKs), and which of them fail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Affected {
    /// The SDK (`go`) or target (`wallet-backend`).
    pub on: String,
    pub tested: Vec<String>,
    pub failing: Vec<String>,
    /// The first tested version after the last failing one, when there is one: every later
    /// tested version passes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passes_from: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub findings: usize,
    pub failures: usize,
    pub unmapped: usize,
    /// Failures no finding takes but a candidate finding does (see [`Collection::held`]).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub held: usize,
    /// component, then severity, then the number of findings.
    pub by_component: BTreeMap<String, BTreeMap<String, usize>>,
    pub by_severity: BTreeMap<String, usize>,
    pub by_status: BTreeMap<String, usize>,
}

/// `findings/findings.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub schema: u32,
    pub inputs: Vec<InputRef>,
    pub summary: Summary,
    pub findings: Vec<FindingRecord>,
    /// Failures no finding matched.
    pub unmapped: Vec<Occurrence>,
    /// Failures no finding matched that a finding of the candidate triage
    /// ([`Triage::candidates`]) takes, by candidate finding id. They are neither unmapped
    /// nor findings of this record: the candidate finding is triaged and drafted in its own
    /// lane and counts here once it is promoted.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub held: BTreeMap<String, Vec<Occurrence>>,
    /// Problems the inputs themselves report and that hide failures: chains that did not run,
    /// known-failures lists out of date.
    pub input_problems: Vec<String>,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// The findings of the candidate triage `triage` names, which hold failures none of its own
/// findings take.
fn candidate_findings(root: &Path, triage: &Triage) -> Result<Vec<FindingDef>> {
    let Some(rel) = &triage.candidates else {
        return Ok(Vec::new());
    };
    let c = crate::triage::load(&root.join(rel))?;
    if c.candidates.is_some() {
        bail!("{rel} names candidates of its own; only one triage can hold failures for another");
    }
    if let Some(f) = c
        .findings
        .iter()
        .find(|f| triage.findings.iter().any(|m| m.id == f.id))
    {
        bail!(
            "candidate finding {} in {rel} has the id of a finding of the triage",
            f.id
        );
    }
    Ok(c.findings)
}

/// Puts an occurrence no finding took with the candidate finding that takes it, if one does,
/// or with the unmapped failures.
fn hold_or_unmap(
    candidates: &[FindingDef],
    takes: impl Fn(&FindingDef) -> bool,
    occ: Occurrence,
    held: &mut BTreeMap<String, Vec<Occurrence>>,
    unmapped: &mut Vec<Occurrence>,
) {
    match candidates.iter().find(|d| takes(d)) {
        Some(d) => held.entry(d.id.clone()).or_default().push(occ),
        None => unmapped.push(occ),
    }
}

/// The harness verdicts that are failures. `out-of-scope` is a statement about the claims,
/// `pass` is a pass.
pub fn harness_failure(verdict: &str) -> bool {
    matches!(verdict, "dropped" | "mishandled" | "not-evaluated")
}

fn any_or(list: &[String], v: &str) -> bool {
    list.is_empty() || list.iter().any(|x| x == v)
}

fn sides(p: &Problem) -> Vec<&Value> {
    [p.expected.as_ref(), p.got.as_ref()]
        .into_iter()
        .flatten()
        .collect()
}

pub fn harness_matches(
    m: &HarnessMatch,
    target: &str,
    row: &EntryRow,
    p: Option<&Problem>,
) -> bool {
    if m.target.as_deref().is_some_and(|t| t != target) {
        return false;
    }
    if !m.entries.is_empty() && !m.entries.iter().any(|g| glob(g, &row.entry)) {
        return false;
    }
    if !any_or(&m.networks, &row.network)
        || !any_or(&m.verdicts, row.result.verdict.name())
        || !m.reasons.is_empty()
            && !row
                .result
                .reason
                .is_some_and(|r| m.reasons.iter().any(|x| x == r.name()))
    {
        return false;
    }
    let Some(p) = p else {
        // a whole entry: only matchers that say nothing about problems
        return m.problems.is_empty()
            && m.facts.is_empty()
            && m.events.is_empty()
            && m.prefix.is_empty()
            && m.null.is_empty()
            && m.detail_contains
                .as_ref()
                .is_none_or(|d| row.result.detail.contains(d));
    };
    let kind = serde_json::to_value(p.kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    if !any_or(&m.problems, &kind) || !any_or(&m.facts, &p.fact) {
        return false;
    }
    if m.detail_contains
        .as_ref()
        .is_some_and(|d| !p.detail.contains(d))
    {
        return false;
    }
    let s = sides(p);
    if !m.events.is_empty()
        && !s
            .iter()
            .filter_map(|v| v.get("event").and_then(Value::as_str))
            .any(|e| m.events.iter().any(|x| x == e))
    {
        return false;
    }
    for (field, prefixes) in &m.prefix {
        let hit = s
            .iter()
            .filter_map(|v| v.get(field).and_then(Value::as_str))
            .any(|v| prefixes.iter().any(|pre| v.starts_with(pre.as_str())));
        if !hit {
            return false;
        }
    }
    for field in &m.null {
        if s.is_empty() || !s.iter().all(|v| v.get(field).is_none_or(Value::is_null)) {
            return false;
        }
    }
    true
}

pub fn conformance_matches(m: &ConformanceMatch, sdk: &str, version: &str, entry: &str) -> bool {
    m.sdk == sdk && any_or(&m.versions, version) && m.entries.iter().any(|g| glob(g, entry))
}

pub fn lint_matches(m: &LintMatch, target: &str, f: &LintFinding) -> bool {
    m.target == target
        && m.rule == f.rule
        && m.function == f.function
        && m.package.as_ref().is_none_or(|p| *p == f.package)
}

fn problem_line(p: &Problem) -> String {
    let kind = serde_json::to_value(p.kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    format!("{kind} {}: {}", p.fact, p.detail)
}

/// The conformance command that reruns `entry` on the first sdk (by name) it failed on,
/// at the newest failing version of that same sdk in release order (by name, `v0.9.0` would
/// come after `v0.10.0`).
fn sdk_reproduce(runs: &BTreeMap<(String, String), Vec<String>>, entry: &str) -> Option<String> {
    let ((_, sdk), versions) = runs
        .range((entry.to_owned(), String::new())..)
        .next()
        .filter(|((e, _), _)| e == entry)?;
    let version = crate::version::newest(versions.iter().map(String::as_str))?;
    Some(format!(
        "shapekit conformance run --sdk {sdk} --version {version} --entry {entry}"
    ))
}

/// What one collection matched, before records are built.
struct Matched<'a> {
    def: &'a FindingDef,
    occurrences: Vec<Occurrence>,
    /// Per entry, how to rerun it: the chain and network of a harness entry, or, per entry
    /// and sdk, the failing versions of a conformance entry. Keyed by sdk as well, since one
    /// finding can match an entry on two sdks, and a version belongs to its own sdk only.
    harness_runs: BTreeMap<String, (String, String, String)>,
    sdk_runs: BTreeMap<(String, String), Vec<String>>,
    /// Entries whose answers split across the sdks, rerun with every sdk at once.
    peer_runs: BTreeSet<String>,
    lint_runs: Vec<String>,
}

/// The `on` of an occurrence that stands for the answers of every sdk of a matrix.
pub const ALL_SDKS: &str = "all sdks";

/// An answer short enough for one line: a long one is cut and named by its hash, so two
/// different long answers still read as different.
fn short_answer(a: &str) -> String {
    match a.char_indices().nth(48) {
        None => a.to_owned(),
        Some((i, _)) => format!(
            "{}... (sha256 {})",
            &a[..i],
            &shapekit_manifest::sha256_hex(a.as_bytes())[..12]
        ),
    }
}

pub fn answer_matches(m: &AnswerMatch, entry: &str, check: &str) -> bool {
    m.entries.iter().any(|g| glob(g, entry)) && any_or(&m.checks, check)
}

/// Cites the implementation-defined answers of `entries` that `matched`'s answer matchers
/// ask for. Answers are recorded, not judged, so an answer no matcher asks for is neither a
/// failure nor unmapped.
fn cite_answers(
    matched: &mut [Matched<'_>],
    entries: &[crate::inputs::MatrixEntry],
    testees: &BTreeMap<&str, (&str, &str)>,
) {
    for e in entries {
        // check, then answer, then the testees that gave it
        let mut by_check: BTreeMap<&str, BTreeMap<String, Vec<&str>>> = BTreeMap::new();
        for (key, cell) in &e.cells {
            for c in cell.checks.iter().filter(|c| c.is_implementation_defined()) {
                by_check
                    .entry(c.check.as_str())
                    .or_default()
                    .entry(c.answer())
                    .or_default()
                    .push(key.as_str());
            }
        }
        for (check, answers) in &by_check {
            for mm in matched.iter_mut() {
                let mut found = Vec::new();
                for am in mm
                    .def
                    .answer
                    .iter()
                    .filter(|am| answer_matches(am, &e.id, check))
                {
                    if let Some(sdk) = &am.sdk {
                        for (answer, keys) in answers.iter().filter(|(a, _)| am.answers.contains(a))
                        {
                            for key in keys {
                                let Some(&(s, v)) = testees.get(key) else {
                                    continue;
                                };
                                if s != sdk || !any_or(&am.versions, v) {
                                    continue;
                                }
                                let run =
                                    mm.sdk_runs.entry((e.id.clone(), s.to_owned())).or_default();
                                if !run.iter().any(|x| x == v) {
                                    run.push(v.to_owned());
                                }
                                found.push(Occurrence {
                                    source: "answer".into(),
                                    on: (*key).to_owned(),
                                    entry: Some(e.id.clone()),
                                    at: (*check).to_owned(),
                                    what: format!(
                                        "implementation-defined, answered {}",
                                        short_answer(answer)
                                    ),
                                });
                            }
                        }
                    } else if let Some(n) = am.split
                        && answers.len() >= n
                    {
                        mm.peer_runs.insert(e.id.clone());
                        let total: usize = answers.values().map(Vec::len).sum();
                        found.push(Occurrence {
                            source: "answer".into(),
                            on: ALL_SDKS.into(),
                            entry: Some(e.id.clone()),
                            at: (*check).to_owned(),
                            what: format!(
                                "implementation-defined, {} different answers from {total} sdk versions: {}",
                                answers.len(),
                                answers
                                    .iter()
                                    .map(|(a, keys)| format!(
                                        "{} from {}",
                                        short_answer(a),
                                        keys.join(", ")
                                    ))
                                    .collect::<Vec<_>>()
                                    .join("; ")
                            ),
                        });
                    }
                }
                for o in found {
                    if !mm.occurrences.contains(&o) {
                        mm.occurrences.push(o);
                    }
                }
            }
        }
    }
}

/// Splits an occurrence's `on`, `go@v0.7.1` or `wallet-backend@3e22...`, at its first `@`.
fn split_on(on: &str) -> (&str, &str) {
    on.split_once('@').unwrap_or((on, ""))
}

fn push_tested(tested: &mut BTreeMap<String, Vec<String>>, on: &str, version: &str) {
    let list = tested.entry(on.to_owned()).or_default();
    if !list.iter().any(|v| v == version) {
        list.push(version.to_owned());
    }
}

pub fn collect(root: &Path, triage: &Triage) -> Result<Collection> {
    let mut input_refs = Vec::new();
    let mut input_problems = Vec::new();
    let manifest_path = root.join(&triage.inputs.manifest);
    let (bytes, manifest_sha) = inputs::read(&manifest_path)?;
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing {}", manifest_path.display()))?;
    input_refs.push(InputRef {
        kind: "manifest".into(),
        path: triage.inputs.manifest.clone(),
        sha256: manifest_sha.clone(),
        detail: format!("corpus {}", manifest.corpus_version),
    });

    let mut matched: Vec<Matched> = triage
        .findings
        .iter()
        .map(|def| Matched {
            def,
            occurrences: Vec::new(),
            harness_runs: BTreeMap::new(),
            sdk_runs: BTreeMap::new(),
            peer_runs: BTreeSet::new(),
            lint_runs: Vec::new(),
        })
        .collect();
    let candidates = candidate_findings(root, triage)?;
    let mut held: BTreeMap<String, Vec<Occurrence>> = BTreeMap::new();
    let mut unmapped = Vec::new();
    let mut failures = 0usize;
    // every version each sdk or target ran, failing or not
    let mut tested: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for rel in &triage.inputs.harness {
        let path = root.join(rel);
        let (bytes, sha) = inputs::read(&path)?;
        let report = inputs::harness_report(&bytes, &path)?;
        let (target, version) = match report.describe.as_ref() {
            Some(d) => (d.target.clone(), d.target_version.clone()),
            None => {
                // target matchers cannot apply, so every failure would go unmapped or to a
                // matcher without a target
                input_problems.push(format!(
                    "{rel}: the report has no describe, so its target and version are unknown"
                ));
                ("unknown".into(), "unknown".into())
            }
        };
        if report.manifest_sha256 != manifest_sha {
            input_problems.push(format!(
                "{rel}: the harness ran against manifest {}, but {} is {manifest_sha}; rerun the harness and import its report",
                report.manifest_sha256, triage.inputs.manifest
            ));
        }
        input_refs.push(InputRef {
            kind: "harness".into(),
            path: rel.clone(),
            sha256: sha,
            detail: format!(
                "{target} {version}, {} entries, manifest {}",
                report.totals.entries, report.manifest_sha256
            ),
        });
        for c in &report.chains {
            if let Some(e) = &c.error {
                input_problems.push(format!(
                    "{rel}: chain {} ({}) did not run: {e}",
                    c.chain, c.network
                ));
            }
        }
        push_tested(&mut tested, &target, &version);
        let on = format!("{target}@{version}");
        for row in &report.entries {
            if !harness_failure(row.result.verdict.name()) {
                continue;
            }
            let units: Vec<Option<&Problem>> = if row.result.problems.is_empty() {
                vec![None]
            } else {
                row.result.problems.iter().map(Some).collect()
            };
            for p in units {
                failures += 1;
                let occ = Occurrence {
                    source: "harness".into(),
                    on: on.clone(),
                    entry: Some(row.entry.clone()),
                    at: row.network.clone(),
                    what: match p {
                        Some(p) => problem_line(p),
                        None => format!(
                            "{}{}: {}",
                            row.result.verdict.name(),
                            row.result
                                .reason
                                .map(|r| format!(" ({})", r.name()))
                                .unwrap_or_default(),
                            row.result.detail
                        ),
                    },
                };
                let hit = matched.iter_mut().find(|m| {
                    m.def
                        .harness
                        .iter()
                        .any(|hm| harness_matches(hm, &target, row, p))
                });
                match hit {
                    Some(m) => {
                        // testnet is the variant the drafts quote, so its run is the one offered
                        let run = (
                            row.chain.clone(),
                            row.network.clone(),
                            target.clone() + "\u{0}" + &version,
                        );
                        let slot = m
                            .harness_runs
                            .entry(row.entry.clone())
                            .or_insert(run.clone());
                        if row.network == "testnet" {
                            *slot = run;
                        }
                        m.occurrences.push(occ);
                    }
                    None => hold_or_unmap(
                        &candidates,
                        |d| {
                            d.harness
                                .iter()
                                .any(|hm| harness_matches(hm, &target, row, p))
                        },
                        occ,
                        &mut held,
                        &mut unmapped,
                    ),
                }
            }
        }
    }

    for rel in &triage.inputs.matrices {
        let path = root.join(rel);
        let (bytes, sha) = inputs::read(&path)?;
        let m: MatrixView = crate::parse_on_big_stack(move || inputs::matrix(&bytes, &path))?;
        input_refs.push(InputRef {
            kind: "conformance".into(),
            path: rel.clone(),
            sha256: sha,
            detail: format!(
                "corpus {}, {}",
                m.corpus_version,
                m.testees
                    .iter()
                    .map(|t| t.key.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        });
        if m.corpus_version != manifest.corpus_version {
            input_problems.push(format!(
                "{rel}: the matrix is of corpus {}, but {} is corpus {}; rerun conformance and import the matrix",
                m.corpus_version, triage.inputs.manifest, manifest.corpus_version
            ));
        }
        for p in &m.known_failure_problems {
            input_problems.push(format!("{rel}: known-failures: {p}"));
        }
        for t in &m.testees {
            push_tested(&mut tested, &t.sdk, &t.version);
        }
        let testees: BTreeMap<&str, (&str, &str)> = m
            .testees
            .iter()
            .map(|t| (t.key.as_str(), (t.sdk.as_str(), t.version.as_str())))
            .collect();
        for e in &m.entries {
            for (key, cell) in &e.cells {
                if !cell.is_failure() {
                    continue;
                }
                failures += 1;
                let (sdk, version) = match testees.get(key.as_str()) {
                    Some(t) => *t,
                    None => {
                        input_problems.push(format!(
                            "{rel}: {} has a cell for {key}, which is not one of the matrix's testees",
                            e.id
                        ));
                        ("?", "?")
                    }
                };
                if cell.known_issue.is_none() {
                    input_problems.push(format!(
                        "{rel}: {} on {key} is {} and not in the known-failures list",
                        e.id, cell.result
                    ));
                }
                let failed: Vec<&str> = cell
                    .checks
                    .iter()
                    .filter(|c| c.result != "pass")
                    .map(|c| c.check.as_str())
                    .collect();
                let occ = Occurrence {
                    source: "conformance".into(),
                    on: key.clone(),
                    entry: Some(e.id.clone()),
                    at: key.clone(),
                    what: format!(
                        "{} ({} check(s): {})",
                        cell.result,
                        failed.len(),
                        failed
                            .iter()
                            .take(4)
                            .copied()
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                };
                let hit = matched.iter_mut().find(|mm| {
                    mm.def
                        .conformance
                        .iter()
                        .any(|cm| conformance_matches(cm, sdk, version, &e.id))
                });
                match hit {
                    Some(mm) => {
                        mm.sdk_runs
                            .entry((e.id.clone(), sdk.to_owned()))
                            .or_default()
                            .push(version.to_owned());
                        mm.occurrences.push(occ);
                    }
                    None => hold_or_unmap(
                        &candidates,
                        |d| {
                            d.conformance
                                .iter()
                                .any(|cm| conformance_matches(cm, sdk, version, &e.id))
                        },
                        occ,
                        &mut held,
                        &mut unmapped,
                    ),
                }
            }
        }
        cite_answers(&mut matched, &m.entries, &testees);
    }

    for li in &triage.inputs.lint {
        let path = root.join(&li.result);
        let (bytes, sha) = inputs::read(&path)?;
        let r = inputs::lint(&bytes, &path)?;
        input_refs.push(InputRef {
            kind: "lint".into(),
            path: li.result.clone(),
            sha256: sha,
            detail: format!(
                "{} {}, {} finding(s), {} baselined, {} suppressed",
                li.target,
                li.version,
                r.findings.len(),
                r.baselined.len(),
                r.suppressed.len()
            ),
        });
        push_tested(&mut tested, &li.target, &li.version);
        for f in &r.findings {
            failures += 1;
            let occ = Occurrence {
                source: "lint".into(),
                on: format!("{}@{}", li.target, li.version),
                entry: None,
                at: format!("{}:{}:{}", f.file, f.line, f.column),
                what: format!("{}: {} [{}]", f.rule, f.message, f.function),
            };
            let hit = matched
                .iter_mut()
                .find(|m| m.def.lint.iter().any(|lm| lint_matches(lm, &li.target, f)));
            match hit {
                Some(m) => {
                    m.lint_runs.push(format!(
                        "{} (in a checkout of {} at {})",
                        li.command, li.target, li.version
                    ));
                    m.occurrences.push(occ);
                }
                None => hold_or_unmap(
                    &candidates,
                    |d| d.lint.iter().any(|lm| lint_matches(lm, &li.target, f)),
                    occ,
                    &mut held,
                    &mut unmapped,
                ),
            }
        }
    }

    let by_id: BTreeMap<&str, &shapekit_manifest::Entry> = manifest
        .entries
        .iter()
        .map(|e| (e.id.as_str(), e))
        .collect();
    let corpus_dir = manifest_path.parent().unwrap_or(root);
    let mut records: Vec<FindingRecord> = matched
        .iter()
        .map(|m| record(m, triage, &by_id, corpus_dir, &mut input_problems))
        .collect();
    link_siblings(&mut records, triage);
    for r in &mut records {
        r.affected = affected(&r.occurrences, &tested);
    }
    let sdk_failures: Vec<Occurrence> = records
        .iter()
        .flat_map(|r| &r.occurrences)
        .chain(&unmapped)
        .chain(held.values().flatten())
        .filter(|o| o.source == "conformance")
        .cloned()
        .collect();
    for r in &mut records {
        r.other_sdks = other_sdks(r, &sdk_failures);
    }

    let mut summary = Summary {
        findings: records.len(),
        failures,
        unmapped: unmapped.len(),
        held: held.values().map(Vec::len).sum(),
        ..Summary::default()
    };
    for r in &records {
        *summary
            .by_component
            .entry(r.component.clone())
            .or_default()
            .entry(r.severity.name().into())
            .or_default() += 1;
        *summary
            .by_severity
            .entry(r.severity.name().into())
            .or_default() += 1;
        *summary.by_status.entry(r.status.name().into()).or_default() += 1;
    }
    unmapped.sort();
    for v in held.values_mut() {
        v.sort();
    }
    Ok(Collection {
        schema: FINDINGS_SCHEMA,
        inputs: input_refs,
        summary,
        findings: records,
        unmapped,
        held,
        input_problems,
    })
}

/// Size of the first file of an entry, so the smallest entry can be offered as the minimal
/// reproduction. A file that cannot be read is an input problem, not an infinitely large
/// entry, so it cannot silently drop out of the choice.
fn entry_size(e: &shapekit_manifest::Entry, corpus: &Path) -> Result<u64, String> {
    let first = e
        .variants
        .testnet
        .iter()
        .chain(e.variants.mainnet.iter())
        .chain(e.variants.any.iter())
        .map(|v| v.xdr_file.as_str())
        .chain(e.vector.iter().map(|v| v.path.as_str()))
        .next()
        .ok_or_else(|| format!("manifest entry {} names no file", e.id))?;
    if !shapekit_manifest::is_safe_relative(first) {
        return Err(format!(
            "manifest entry {} names {first:?}, which is not a path inside the corpus",
            e.id
        ));
    }
    let path = corpus.join(first);
    std::fs::metadata(&path)
        .map(|m| m.len())
        .map_err(|err| format!("entry {}: {}: {err}", e.id, path.display()))
}

fn record(
    m: &Matched<'_>,
    triage: &Triage,
    manifest: &BTreeMap<&str, &shapekit_manifest::Entry>,
    corpus: &Path,
    problems: &mut Vec<String>,
) -> FindingRecord {
    let def = m.def;
    let entries: BTreeSet<String> = m
        .occurrences
        .iter()
        .filter_map(|o| o.entry.clone())
        .collect();
    let mut caps: BTreeSet<String> = def.caps.iter().cloned().collect();
    for e in &entries {
        if let Some(me) = manifest.get(e.as_str()) {
            caps.extend(me.caps.iter().cloned());
        }
    }
    let mut sized: Vec<(u64, &String)> = Vec::new();
    for e in &entries {
        match manifest.get(e.as_str()) {
            None => problems.push(format!(
                "{}: entry {e} is not in the manifest, so the input is from another corpus",
                def.id
            )),
            Some(me) => match entry_size(me, corpus) {
                Ok(n) => sized.push((n, e)),
                Err(why) => problems.push(format!("{}: {why}", def.id)),
            },
        }
    }
    let minimal = def
        .minimal_entry
        .clone()
        .or_else(|| sized.iter().min().map(|(_, e)| (*e).clone()));
    let reproduce = minimal
        .as_ref()
        .and_then(|e| {
            if let Some((chain, network, tv)) = m.harness_runs.get(e) {
                let (target, version) = tv.split_once('\u{0}').unwrap_or((tv, ""));
                let adapter = triage
                    .targets
                    .get(target)
                    .and_then(|t| t.adapter.as_ref())?
                    .replace("{version}", version);
                return Some(format!(
                    "shapekit harness run --chain {chain} --network {network} --adapter \"{adapter}\""
                ));
            }
            if let Some(line) = sdk_reproduce(&m.sdk_runs, e) {
                return Some(line);
            }
            m.peer_runs
                .contains(e)
                .then(|| format!("shapekit conformance run --entry {e}"))
        })
        .or_else(|| m.lint_runs.first().cloned());
    let mut occurrences = m.occurrences.clone();
    occurrences.sort();
    FindingRecord {
        id: def.id.clone(),
        title: def.title.clone(),
        short: def.short.clone(),
        component: def.component.clone(),
        severity: def.severity,
        caps: caps.into_iter().collect(),
        recipe: minimal
            .as_deref()
            .and_then(|e| manifest.get(e))
            .and_then(|e| recipe_of(e)),
        minimal_entry: minimal,
        reproduce,
        status: def.status,
        link: def.link.clone(),
        reference: def.reference.clone(),
        reason: def.reason.clone(),
        draft: def.draft.clone(),
        cause: def.cause.clone(),
        siblings: Vec::new(),
        related: def.related.clone(),
        entries: entries.into_iter().collect(),
        affected: Vec::new(),
        other_sdks: Vec::new(),
        occurrences,
    }
}

/// The recipe that produces an entry on a local core, if one does.
fn recipe_of(e: &shapekit_manifest::Entry) -> Option<String> {
    if let Some(ir) = &e.source.recipe_ir {
        return Some(format!("recipe:{ir}"));
    }
    if e.source.class.is_local() {
        return e.source.chain.as_ref().map(|c| format!("chain:{c}"));
    }
    None
}

fn affected(occurrences: &[Occurrence], tested: &BTreeMap<String, Vec<String>>) -> Vec<Affected> {
    let mut failing: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    // the answers of every sdk at once name no one sdk
    for o in occurrences.iter().filter(|o| o.on != ALL_SDKS) {
        let (on, version) = split_on(&o.on);
        failing.entry(on).or_default().insert(version);
    }
    failing
        .into_iter()
        .map(|(on, versions)| {
            let tried = tested.get(on).cloned().unwrap_or_default();
            let failing: Vec<String> = tried
                .iter()
                .filter(|v| versions.contains(v.as_str()))
                .cloned()
                .collect();
            let last = tried.iter().rposition(|v| versions.contains(v.as_str()));
            let passes_from = last.and_then(|i| tried.get(i + 1)).cloned();
            Affected {
                on: on.to_owned(),
                tested: tried,
                failing,
                passes_from,
            }
        })
        .collect()
}

fn other_sdks(r: &FindingRecord, sdk_failures: &[Occurrence]) -> Vec<String> {
    let own: BTreeSet<&str> = r
        .occurrences
        .iter()
        .filter(|o| o.source == "conformance")
        .map(|o| split_on(&o.on).0)
        .collect();
    let others: BTreeSet<String> = sdk_failures
        .iter()
        .filter(|o| o.entry.as_ref().is_some_and(|e| r.entries.contains(e)))
        .filter(|o| !own.contains(split_on(&o.on).0))
        .map(|o| o.on.clone())
        .collect();
    others.into_iter().collect()
}

/// Findings in different components that fail on the same corpus entry are siblings, as
/// are those the triage names. The relation is symmetric.
fn link_siblings(records: &mut [FindingRecord], triage: &Triage) {
    let mut pairs: BTreeSet<(String, String)> = BTreeSet::new();
    for (i, a) in records.iter().enumerate() {
        for b in &records[i + 1..] {
            if a.component != b.component && a.entries.iter().any(|e| b.entries.contains(e)) {
                pairs.insert((a.id.clone(), b.id.clone()));
                pairs.insert((b.id.clone(), a.id.clone()));
            }
        }
    }
    for f in &triage.findings {
        for s in &f.siblings {
            pairs.insert((f.id.clone(), s.clone()));
            pairs.insert((s.clone(), f.id.clone()));
        }
    }
    for r in records.iter_mut() {
        r.siblings = pairs
            .iter()
            .filter(|(a, _)| *a == r.id)
            .map(|(_, b)| b.clone())
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use shapekit_harness::compare::{EntryVerdict, ProblemKind, Reason, Verdict};

    fn row(
        entry: &str,
        verdict: Verdict,
        reason: Option<Reason>,
        problems: Vec<Problem>,
    ) -> EntryRow {
        EntryRow {
            entry: entry.into(),
            family: "f".into(),
            chain: "c".into(),
            network: "testnet".into(),
            ledger: 70,
            result: EntryVerdict {
                verdict,
                reason,
                detail: "ingestion stopped at ledger 70 (halted): boom".into(),
                problems,
                unmapped: Vec::new(),
                notes: Vec::new(),
            },
        }
    }

    fn problem(
        kind: ProblemKind,
        fact: &str,
        expected: Option<Value>,
        got: Option<Value>,
    ) -> Problem {
        Problem {
            kind,
            tx: "aa".into(),
            fact: fact.into(),
            detail: format!("{fact} differs in to_muxed_id"),
            expected,
            got,
        }
    }

    #[test]
    fn problem_matchers_check_every_given_member() {
        let p = problem(
            ProblemKind::Dropped,
            "token-event",
            Some(json!({"event": "transfer", "from": "GA", "to": "BAAA"})),
            None,
        );
        let r = row("cap23.x", Verdict::Dropped, None, vec![p.clone()]);
        let mut m = HarnessMatch {
            target: Some("wallet-backend".into()),
            facts: vec!["token-event".into()],
            problems: vec!["dropped".into()],
            events: vec!["transfer".into()],
            ..HarnessMatch::default()
        };
        m.prefix.insert("to".into(), vec!["B".into(), "L".into()]);
        assert!(harness_matches(&m, "wallet-backend", &r, Some(&p)));
        assert!(!harness_matches(&m, "stellar-etl", &r, Some(&p)));
        m.prefix.insert("from".into(), vec!["C".into()]);
        assert!(!harness_matches(&m, "wallet-backend", &r, Some(&p)));
        // a problem matcher never takes a whole entry
        assert!(!harness_matches(&m, "wallet-backend", &r, None));
    }

    #[test]
    fn null_needs_the_field_absent_on_every_present_side() {
        let extra = problem(
            ProblemKind::Extra,
            "token-event",
            None,
            Some(json!({"event": "transfer", "to": null})),
        );
        let paired = problem(
            ProblemKind::WrongValue,
            "token-event",
            Some(json!({"event": "transfer", "to": "GB"})),
            Some(json!({"event": "transfer", "to": null})),
        );
        let r = row("e", Verdict::Mishandled, Some(Reason::WrongValue), vec![]);
        let m = HarnessMatch {
            null: vec!["to".into()],
            ..HarnessMatch::default()
        };
        assert!(harness_matches(&m, "t", &r, Some(&extra)));
        assert!(!harness_matches(&m, "t", &r, Some(&paired)));
    }

    #[test]
    fn entry_matchers_take_stops() {
        let r = row(
            "cap66.archival.p30-x",
            Verdict::Mishandled,
            Some(Reason::Halted),
            vec![],
        );
        let m = HarnessMatch {
            entries: vec!["cap66.*".into()],
            reasons: vec!["halted".into()],
            ..HarnessMatch::default()
        };
        assert!(harness_matches(&m, "t", &r, None));
        let other = HarnessMatch {
            reasons: vec!["crashed".into()],
            ..m.clone()
        };
        assert!(!harness_matches(&other, "t", &r, None));
    }

    #[test]
    fn only_dropped_mishandled_and_not_evaluated_are_failures() {
        for (v, f) in [
            ("pass", false),
            ("out-of-scope", false),
            ("dropped", true),
            ("mishandled", true),
            ("not-evaluated", true),
        ] {
            assert_eq!(harness_failure(v), f, "{v}");
        }
    }

    #[test]
    fn conformance_versions_default_to_all() {
        let m = ConformanceMatch {
            sdk: "go".into(),
            entries: vec!["*.synth-strkey-*-invalid".into()],
            versions: vec![],
        };
        assert!(conformance_matches(
            &m,
            "go",
            "v0.8.0",
            "cap23.claimable-balances.synth-strkey-b-invalid"
        ));
        assert!(!conformance_matches(
            &m,
            "js",
            "v0.8.0",
            "cap23.claimable-balances.synth-strkey-b-invalid"
        ));
        assert!(!conformance_matches(
            &m,
            "go",
            "v0.8.0",
            "cap23.claimable-balances.synth-strkey-b-valid"
        ));
    }

    fn occ(on: &str, entry: &str) -> Occurrence {
        Occurrence {
            source: "conformance".into(),
            on: on.into(),
            entry: Some(entry.into()),
            at: on.into(),
            what: "diverges-from-network".into(),
        }
    }

    #[test]
    fn affected_versions_follow_the_tested_order() {
        let tested = BTreeMap::from([(
            "go".to_string(),
            vec![
                "v0.7.1".into(),
                "v0.7.2".into(),
                "v0.8.0".into(),
                "v0.9.0".into(),
            ],
        )]);
        let a = affected(&[occ("go@v0.7.2", "e"), occ("go@v0.7.1", "e")], &tested);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].failing, vec!["v0.7.1", "v0.7.2"]);
        assert_eq!(a[0].passes_from.as_deref(), Some("v0.8.0"));
        let a = affected(&[occ("go@v0.9.0", "e")], &tested);
        assert_eq!(a[0].passes_from, None);
    }

    #[test]
    fn other_sdks_are_failures_of_other_sdks_on_the_same_entries() {
        let failures = vec![
            occ("go@v0.9.0", "e"),
            occ("js@17.4.0", "e"),
            occ("rust@30.0.0", "other"),
        ];
        let r = FindingRecord {
            id: "GO-X".into(),
            title: "t".into(),
            short: None,
            component: "go-stellar-sdk".into(),
            severity: Severity::Divergence,
            caps: vec![],
            minimal_entry: None,
            reproduce: None,
            status: Status::Draft,
            link: None,
            reference: None,
            reason: None,
            draft: None,
            cause: "c".into(),
            siblings: vec![],
            related: vec![],
            entries: vec!["e".into()],
            recipe: None,
            affected: vec![],
            other_sdks: vec![],
            occurrences: vec![occ("go@v0.9.0", "e")],
        };
        assert_eq!(other_sdks(&r, &failures), vec!["js@17.4.0"]);
    }

    #[test]
    fn the_rerun_takes_the_newest_failing_version_in_release_order() {
        // versions arrive in the order of "sdk@version" keys, where v0.9.0 follows v0.10.0
        let runs = BTreeMap::from([
            (
                ("e".to_owned(), "go".to_owned()),
                vec!["v0.10.0".to_owned(), "v0.9.0".to_owned()],
            ),
            (("e".to_owned(), "js".to_owned()), vec!["17.4.0".to_owned()]),
        ]);
        assert_eq!(
            sdk_reproduce(&runs, "e").unwrap(),
            "shapekit conformance run --sdk go --version v0.10.0 --entry e"
        );
    }

    #[test]
    fn a_finding_on_two_sdks_reruns_each_at_its_own_version() {
        use crate::inputs::{CellView, CheckView, MatrixEntry};
        let def: FindingDef = toml::from_str(
            r#"
id = "X-1"
title = "t"
component = "rs-stellar-xdr"
severity = "divergence"
status = "filed"
cause = "c"
[[answer]]
sdk = "rust"
entries = ["e"]
checks = ["any/decode_json"]
answers = ['a']
[[answer]]
sdk = "python"
entries = ["e"]
checks = ["any/decode_json"]
answers = ['b']
"#,
        )
        .unwrap();
        let answer = |v: &str| CellView {
            result: "pass".into(),
            known_issue: None,
            checks: vec![CheckView {
                check: "any/decode_json".into(),
                op: "decode_json".into(),
                result: "pass".into(),
                failure: None,
                detail: Some("implementation-defined: no answer is required".into()),
                actual: Some(serde_json::json!(v)),
            }],
        };
        let entries = vec![MatrixEntry {
            id: "e".into(),
            cells: BTreeMap::from([
                ("rust@1.0.0".into(), answer("a")),
                ("python@7.0.0".into(), answer("b")),
            ]),
        }];
        let testees: BTreeMap<&str, (&str, &str)> = BTreeMap::from([
            ("rust@1.0.0", ("rust", "1.0.0")),
            ("python@7.0.0", ("python", "7.0.0")),
        ]);
        let mut matched = vec![Matched {
            def: &def,
            occurrences: Vec::new(),
            harness_runs: BTreeMap::new(),
            sdk_runs: BTreeMap::new(),
            peer_runs: BTreeSet::new(),
            lint_runs: Vec::new(),
        }];
        cite_answers(&mut matched, &entries, &testees);
        assert_eq!(matched[0].sdk_runs.len(), 2);
        assert_eq!(
            sdk_reproduce(&matched[0].sdk_runs, "e").unwrap(),
            "shapekit conformance run --sdk python --version 7.0.0 --entry e"
        );
        assert_eq!(sdk_reproduce(&matched[0].sdk_runs, "other"), None);
    }

    #[test]
    fn answer_matchers_cite_recorded_answers_and_splits() {
        use crate::inputs::{CellView, CheckView, MatrixEntry};
        let def: FindingDef = toml::from_str(
            r#"
id = "RS-3"
title = "t"
component = "rs-stellar-xdr"
severity = "divergence"
status = "filed"
cause = "c"
[[answer]]
sdk = "rust"
entries = ["e-bool"]
checks = ["any/decode_json"]
answers = ['{"json":{"bool":false}}']
"#,
        )
        .unwrap();
        let split_def: FindingDef = toml::from_str(
            r#"
id = "CORE-1"
title = "t"
component = "stellar-core"
severity = "robustness"
status = "filed"
cause = "c"
[[answer]]
entries = ["e-bool"]
checks = ["any/decode_json"]
split = 3
"#,
        )
        .unwrap();
        let answer = |v: serde_json::Value| CellView {
            result: "pass".into(),
            known_issue: None,
            checks: vec![CheckView {
                check: "any/decode_json".into(),
                op: "decode_json".into(),
                result: "pass".into(),
                failure: None,
                detail: Some("implementation-defined: no answer is required".into()),
                actual: Some(v),
            }],
        };
        let entries = vec![MatrixEntry {
            id: "e-bool".into(),
            cells: BTreeMap::from([
                (
                    "rust@1".into(),
                    answer(serde_json::json!({"json": {"bool": false}})),
                ),
                (
                    "rust@2".into(),
                    answer(serde_json::json!({"json": {"bool": false}})),
                ),
                (
                    "python@1".into(),
                    answer(serde_json::json!({"json": {"bool": true}})),
                ),
                ("go@1".into(), answer(serde_json::json!("rejected"))),
            ]),
        }];
        let testees: BTreeMap<&str, (&str, &str)> = BTreeMap::from([
            ("rust@1", ("rust", "1")),
            ("rust@2", ("rust", "2")),
            ("python@1", ("python", "1")),
            ("go@1", ("go", "1")),
        ]);
        let new = |def| Matched {
            def,
            occurrences: Vec::new(),
            harness_runs: BTreeMap::new(),
            sdk_runs: BTreeMap::new(),
            peer_runs: BTreeSet::new(),
            lint_runs: Vec::new(),
        };
        let mut matched = vec![new(&def), new(&split_def)];
        cite_answers(&mut matched, &entries, &testees);
        let on: Vec<&str> = matched[0]
            .occurrences
            .iter()
            .map(|o| o.on.as_str())
            .collect();
        assert_eq!(on, ["rust@1", "rust@2"]);
        assert_eq!(
            matched[0].sdk_runs[&("e-bool".to_owned(), "rust".to_owned())],
            ["1", "2"]
        );
        assert_eq!(matched[1].occurrences.len(), 1);
        let split = &matched[1].occurrences[0];
        assert_eq!(split.on, ALL_SDKS);
        assert!(
            split
                .what
                .contains("3 different answers from 4 sdk versions")
                && split.what.contains("rejected from go@1"),
            "{}",
            split.what
        );
        // a split of three needs three answers
        let two = vec![MatrixEntry {
            id: "e-bool".into(),
            cells: entries[0]
                .cells
                .iter()
                .filter(|(k, _)| k.as_str() != "go@1")
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        }];
        let mut matched = vec![new(&def), new(&split_def)];
        cite_answers(&mut matched, &two, &testees);
        assert!(matched[1].occurrences.is_empty());
        assert!(
            affected(std::slice::from_ref(split), &BTreeMap::new()).is_empty(),
            "the answers of every sdk name no one sdk"
        );
    }
}
