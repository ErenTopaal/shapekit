//! `shapekit report`: one harness or conformance run as a self-contained HTML page and a
//! short markdown summary for a pull request comment.
//!
//! The report invents no grouping of its own where a triage exists. It hands the run to
//! `shapekit findings collect` ([`shapekit_findings::collect`]) as the only input of a copy of
//! the triage, so every failure lands on the finding the committed triage gives it, or on a
//! candidate finding that holds it, exactly as in `findings/findings.json`. Failures no
//! finding takes are grouped by corpus family and by what failed, and say so.
//!
//! Each group links:
//!
//! - its finding: the upstream issue or recorded decision, and the finding on the site;
//! - the guide: the field table rows of the fields its corpus entries exercise (the
//!   manifest's `fields_exercised`), the checklist items that name those fields
//!   (`registry/guide-checklist.toml`), and the guide's list of corpus entries by field;
//! - a reproduce command for its smallest entry.
//!
//! With a baseline (an earlier run, a harness expectations file, or for a conformance run its
//! known-failures directory) every failure is also classed as new, changed or unchanged, and
//! every baseline failure the run no longer shows as fixed.

pub mod baseline;
pub mod html;
pub mod summary;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use shapekit_findings::collect::{self, Collection, FindingRecord, Occurrence};
use shapekit_findings::inputs::{self, MatrixView};
use shapekit_findings::triage::{self, Triage};
use shapekit_harness::report::{MARKDOWN_FILE as HARNESS_MD, REPORT_FILE as HARNESS_REPORT};
use shapekit_manifest::{Entry, Manifest};
use shapekit_registry::{Registry, ValidatedByCore};

pub const REPORT_SCHEMA: u32 = 1;
/// Where `divergence-matrix.json` sits in a conformance run directory.
pub const MATRIX_FILE: &str = "divergence-matrix.json";
/// The site the report links findings and guide sections on.
pub const DEFAULT_SITE: &str = "https://shapekit.org";
/// The guide page on the site, and its section anchors (the `{#id}` headings of
/// `docs/guide/indexer-hardening.mdx`).
pub const GUIDE_PATH: &str = "/docs/guide/indexer-hardening";
pub const GUIDE_FIELDS_ANCHOR: &str = "validated-and-unvalidated-fields";
pub const GUIDE_CHECKLIST_ANCHOR: &str = "self-audit-checklist";
/// The first line of every summary is this prefix, the run kind and ` -->`
/// (`<!-- shapekit-report harness -->`), so a workflow finds and updates its own comment and a
/// harness and a conformance comment on one pull request stay apart.
pub const SUMMARY_MARKER: &str = "<!-- shapekit-report";

/// The first line of a summary of a run of this kind.
pub fn summary_marker(kind: RunKind) -> String {
    format!("{SUMMARY_MARKER} {} -->", kind.name())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunKind {
    Harness,
    Conformance,
}

impl RunKind {
    pub fn name(self) -> &'static str {
        match self {
            RunKind::Harness => "harness",
            RunKind::Conformance => "conformance",
        }
    }
}

pub struct Options {
    /// The shapekit checkout: the triage, its candidate triage and the registry are read
    /// relative to it.
    pub root: PathBuf,
    /// A run directory (`harness run --out`, `conformance run --out`) or the json in it.
    pub input: PathBuf,
    /// The triage, relative to `root` unless absolute.
    pub triage: PathBuf,
    /// The manifest the run used; the triage's own when `None`.
    pub manifest: Option<PathBuf>,
    /// An earlier run of the same kind, or a harness expectations file.
    pub baseline: Option<PathBuf>,
    /// A conformance run's known-failures directory, used as its baseline.
    pub known_failures: Option<PathBuf>,
    /// The site findings and guide sections are linked on, without a trailing slash.
    pub site: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputInfo {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GroupKind {
    /// A finding of the triage.
    Finding,
    /// A finding of the candidate triage, not yet in the record.
    Candidate,
    /// No finding takes these failures.
    Untriaged,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldRef {
    pub id: String,
    pub validated_by_core: String,
    pub risk: String,
    pub handling: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuideRef {
    /// The guide section.
    pub section: String,
    /// What in that section applies.
    pub title: String,
    pub url: String,
}

/// Failures with one root cause.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub kind: GroupKind,
    /// The finding id, or for untriaged failures `untriaged-<n>`.
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The upstream issue, pull request or recorded decision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// The finding on the site.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
    pub caps: Vec<String>,
    pub entries: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimal_entry: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reproduce: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe: Option<String>,
    pub fields: Vec<FieldRef>,
    pub guide: Vec<GuideRef>,
    pub occurrences: Vec<Occurrence>,
}

/// One failure, or one baseline failure, compared across the two runs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Change {
    pub entry: String,
    /// The network of a harness entry, or the testee of a conformance check.
    pub at: String,
    /// The conformance check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub now: Option<String>,
    /// The group the failure is in, for one this run shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    /// What the run was compared with: `harness report`, `expectations`, `conformance
    /// matrix` or `known-failures`.
    pub kind: String,
    pub path: String,
    /// Failing now, not before.
    pub new: Vec<Change>,
    /// Failing in both, with another result.
    pub changed: Vec<Change>,
    /// Failing before, not now.
    pub fixed: Vec<Change>,
    /// Failing in both with the same result.
    pub unchanged: usize,
    /// Baseline failures this run did not cover, so nothing is known about them.
    pub not_rerun: usize,
}

/// Everything the HTML page and the summary show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub schema: u32,
    pub kind: RunKind,
    pub input: InputInfo,
    /// The target and version, or the testees.
    pub subject: String,
    pub corpus: String,
    /// Entries or cells by result, in the run's own words.
    pub totals: BTreeMap<String, BTreeMap<String, usize>>,
    /// Failures as `findings collect` counts them.
    pub failures: usize,
    pub groups: Vec<Group>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comparison: Option<Comparison>,
    pub input_problems: Vec<String>,
    pub site: String,
}

/// Finds the run json: `path` itself, or the harness report or divergence matrix in the
/// directory `path`.
pub fn resolve_input(path: &Path) -> Result<(RunKind, PathBuf)> {
    if path.is_dir() {
        let h = path.join(HARNESS_REPORT);
        let m = path.join(MATRIX_FILE);
        return match (h.is_file(), m.is_file()) {
            (true, false) => Ok((RunKind::Harness, h)),
            (false, true) => Ok((RunKind::Conformance, m)),
            (true, true) => bail!(
                "{} holds both {HARNESS_REPORT} and {MATRIX_FILE}; name the file",
                path.display()
            ),
            (false, false) => bail!(
                "{} holds neither {HARNESS_REPORT} ({HARNESS_MD}'s run) nor {MATRIX_FILE}",
                path.display()
            ),
        };
    }
    Ok((sniff(path)?, path.to_path_buf()))
}

/// Tells a harness report from a divergence matrix by the keys only each one has.
fn sniff(path: &Path) -> Result<RunKind> {
    #[derive(Deserialize)]
    struct Keys {
        adapter_protocol: Option<serde::de::IgnoredAny>,
        testees: Option<serde::de::IgnoredAny>,
    }
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let path2 = path.to_path_buf();
    let keys = shapekit_findings::parse_on_big_stack(move || {
        let mut de = serde_json::Deserializer::from_slice(&bytes);
        de.disable_recursion_limit();
        Keys::deserialize(&mut de).with_context(|| format!("parsing {}", path2.display()))
    })?;
    match (keys.adapter_protocol.is_some(), keys.testees.is_some()) {
        (true, false) => Ok(RunKind::Harness),
        (false, true) => Ok(RunKind::Conformance),
        _ => bail!(
            "{} is neither a harness report nor a divergence matrix",
            path.display()
        ),
    }
}

fn rooted(root: &Path, p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    }
}

/// Quotes one argument for a POSIX shell when it needs it.
pub fn shell_quote(s: &str) -> String {
    let plain = !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:@,+%".contains(c));
    if plain {
        s.to_owned()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

/// Size of an entry's first file, as `findings collect` picks the minimal entry.
fn entry_size(e: &Entry, corpus: &Path) -> Option<u64> {
    let first = e
        .variants
        .testnet
        .iter()
        .chain(e.variants.mainnet.iter())
        .chain(e.variants.any.iter())
        .map(|v| v.xdr_file.as_str())
        .chain(e.vector.iter().map(|v| v.path.as_str()))
        .next()?;
    if !shapekit_manifest::is_safe_relative(first) {
        return None;
    }
    std::fs::metadata(corpus.join(first)).ok().map(|m| m.len())
}

fn validated_name(v: ValidatedByCore) -> &'static str {
    match v {
        ValidatedByCore::Yes => "yes",
        ValidatedByCore::No => "no",
        ValidatedByCore::Partial => "partial",
    }
}

/// What the report reads besides the run.
struct Ctx<'a> {
    kind: RunKind,
    manifest: BTreeMap<&'a str, &'a Entry>,
    corpus: PathBuf,
    registry: Registry,
    checklist: shapekit_guide::ChecklistFile,
    site: String,
    /// Per harness entry, the chain and network to rerun it with (testnet first).
    harness_runs: BTreeMap<String, (String, String)>,
    /// The adapter command of the harness run, quoted.
    adapter: String,
}

impl Ctx<'_> {
    fn fields_and_guide(&self, entries: &[String]) -> (Vec<FieldRef>, Vec<GuideRef>) {
        let names: BTreeSet<&str> = entries
            .iter()
            .filter_map(|e| self.manifest.get(e.as_str()))
            .flat_map(|e| e.fields_exercised.iter().map(String::as_str))
            .collect();
        let guide = format!("{}{GUIDE_PATH}", self.site);
        let fields: Vec<FieldRef> = names
            .iter()
            .filter_map(|n| self.registry.fields.fields.iter().find(|f| f.id == *n))
            .map(|f| FieldRef {
                id: f.id.clone(),
                validated_by_core: validated_name(f.validated_by_core).into(),
                risk: f.risk.clone(),
                handling: f.handling.clone(),
            })
            .collect();
        let mut refs = Vec::new();
        if !fields.is_empty() {
            refs.push(GuideRef {
                section: "Validated and unvalidated fields".into(),
                title: format!(
                    "field table rows {}",
                    fields
                        .iter()
                        .map(|f| format!("`{}`", f.id))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                url: format!("{guide}#{GUIDE_FIELDS_ANCHOR}"),
            });
            refs.push(GuideRef {
                section: "Corpus entries by field".into(),
                title: "the entries the guide lists for those fields".into(),
                url: format!("{guide}#{}", shapekit_guide::CORPUS_ANCHOR),
            });
        }
        for item in &self.checklist.items {
            if item.fields.iter().any(|f| names.contains(f.as_str())) {
                refs.push(GuideRef {
                    section: "Self-audit checklist".into(),
                    title: item.title.clone(),
                    url: format!("{guide}#{GUIDE_CHECKLIST_ANCHOR}"),
                });
            }
        }
        (fields, refs)
    }

    fn smallest(&self, entries: &[String]) -> Option<String> {
        entries
            .iter()
            .filter_map(|e| {
                let me = self.manifest.get(e.as_str())?;
                Some((entry_size(me, &self.corpus)?, e))
            })
            .min()
            .map(|(_, e)| e.clone())
            .or_else(|| entries.first().cloned())
    }

    /// How to rerun one entry of the run.
    fn reproduce(&self, entry: &str, occurrences: &[Occurrence]) -> Option<String> {
        match self.kind {
            RunKind::Harness => {
                let (chain, network) = self.harness_runs.get(entry)?;
                Some(format!(
                    "shapekit check indexer --chain {} --network {network} --adapter {}",
                    shell_quote(chain),
                    shell_quote(&self.adapter)
                ))
            }
            RunKind::Conformance => {
                // the newest failing version, the last in the matrix's order
                let on = occurrences
                    .iter()
                    .rfind(|o| o.entry.as_deref() == Some(entry))?
                    .on
                    .clone();
                let (sdk, version) = on.split_once('@')?;
                Some(format!(
                    "shapekit check sdk --sdk {sdk} --version {} --entry {}",
                    shell_quote(version),
                    shell_quote(entry)
                ))
            }
        }
    }

    fn recipe(&self, entry: &str) -> Option<String> {
        let e = self.manifest.get(entry)?;
        if let Some(ir) = &e.source.recipe_ir {
            return Some(format!("recipe:{ir}"));
        }
        if e.source.class.is_local() {
            return e.source.chain.as_ref().map(|c| format!("chain:{c}"));
        }
        None
    }
}

fn decision(f: &FindingRecord) -> Option<String> {
    match (&f.link, &f.reference, &f.reason) {
        (Some(_), _, _) => None,
        (_, Some(r), _) => Some(format!("private report {r}")),
        (_, _, Some(r)) => Some(r.clone()),
        _ => Some("not filed yet".into()),
    }
}

/// Builds the report of one run.
pub fn build(opts: &Options) -> Result<Report> {
    let (kind, input) = resolve_input(&opts.input)?;
    let triage_path = rooted(&opts.root, &opts.triage);
    let mut triage: Triage = triage::load(&triage_path)?;
    let manifest_path = match &opts.manifest {
        Some(m) => m.clone(),
        None => opts.root.join(&triage.inputs.manifest),
    };
    let input_abs =
        std::path::absolute(&input).with_context(|| format!("resolving {}", input.display()))?;
    let manifest_abs = std::path::absolute(&manifest_path)
        .with_context(|| format!("resolving {}", manifest_path.display()))?;
    triage.inputs.manifest = manifest_abs.to_string_lossy().into_owned();
    triage.inputs.lint.clear();
    let input_str = input_abs.to_string_lossy().into_owned();
    match kind {
        RunKind::Harness => {
            triage.inputs.harness = vec![input_str];
            triage.inputs.matrices.clear();
        }
        RunKind::Conformance => {
            triage.inputs.harness.clear();
            triage.inputs.matrices = vec![input_str];
        }
    }
    let collection: Collection = collect::collect(&opts.root, &triage)?;

    let (bytes, sha256) = inputs::read(&input_abs)?;
    let mbytes = std::fs::read(&manifest_abs)
        .with_context(|| format!("reading {}", manifest_abs.display()))?;
    let manifest: Manifest = serde_json::from_slice(&mbytes)
        .with_context(|| format!("parsing {}", manifest_abs.display()))?;
    let registry = Registry::load(&opts.root.join("registry"))
        .with_context(|| format!("loading {}", opts.root.join("registry").display()))?;
    let checklist = shapekit_guide::ChecklistFile::load(
        &opts
            .root
            .join("registry")
            .join(shapekit_guide::CHECKLIST_FILE),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut totals: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut harness_runs: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut adapter = String::new();
    let subject;
    let current: baseline::Units;
    let mut matrix_view: Option<MatrixView> = None;
    match kind {
        RunKind::Harness => {
            let r = inputs::harness_report(&bytes, &input_abs)?;
            subject = match &r.describe {
                Some(d) => format!("{} {}", d.target, d.target_version),
                None => "a target that did not describe itself".into(),
            };
            adapter = r
                .adapter_command
                .iter()
                .map(|a| shell_quote(a))
                .collect::<Vec<_>>()
                .join(" ");
            totals.insert("entries".into(), r.totals.by_verdict.clone());
            if !r.totals.by_reason.is_empty() {
                totals.insert("reasons".into(), r.totals.by_reason.clone());
            }
            for row in &r.entries {
                let slot = harness_runs
                    .entry(row.entry.clone())
                    .or_insert((row.chain.clone(), row.network.clone()));
                if row.network == "testnet" {
                    *slot = (row.chain.clone(), row.network.clone());
                }
            }
            current = baseline::harness_units(&r);
        }
        RunKind::Conformance => {
            let p = input_abs.clone();
            let m: MatrixView =
                shapekit_findings::parse_on_big_stack(move || inputs::matrix(&bytes, &p))?;
            subject = m
                .testees
                .iter()
                .map(|t| t.key.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            for e in &m.entries {
                for (key, cell) in &e.cells {
                    *totals
                        .entry(key.clone())
                        .or_default()
                        .entry(cell.result.clone())
                        .or_default() += 1;
                }
            }
            current = baseline::matrix_units(&m);
            matrix_view = Some(m);
        }
    }

    let ctx = Ctx {
        kind,
        manifest: manifest
            .entries
            .iter()
            .map(|e| (e.id.as_str(), e))
            .collect(),
        corpus: manifest_abs
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default(),
        registry,
        checklist,
        site: opts.site.trim_end_matches('/').to_owned(),
        harness_runs,
        adapter,
    };

    let mut groups = Vec::new();
    for f in collection
        .findings
        .iter()
        .filter(|f| !f.occurrences.is_empty())
    {
        let (fields, guide) = ctx.fields_and_guide(&f.entries);
        let minimal = f
            .minimal_entry
            .clone()
            .filter(|e| f.entries.contains(e))
            .or_else(|| ctx.smallest(&f.entries));
        groups.push(Group {
            kind: GroupKind::Finding,
            id: f.id.clone(),
            title: f.title.clone(),
            severity: Some(f.severity.name().into()),
            status: Some(f.status.name().into()),
            decision: decision(f),
            link: f.link.clone(),
            page: Some(format!("{}/dashboard/findings#{}", ctx.site, f.id)),
            cause: Some(f.cause.clone()),
            caps: f.caps.clone(),
            entries: f.entries.clone(),
            reproduce: minimal
                .as_deref()
                .and_then(|e| ctx.reproduce(e, &f.occurrences)),
            recipe: minimal.as_deref().and_then(|e| ctx.recipe(e)),
            minimal_entry: minimal,
            fields,
            guide,
            occurrences: f.occurrences.clone(),
        });
    }

    if !collection.held.is_empty() {
        let candidates = match &triage.candidates {
            Some(rel) => triage::load(&opts.root.join(rel))?.findings,
            None => Vec::new(),
        };
        for (id, occs) in &collection.held {
            let def = candidates.iter().find(|d| &d.id == id);
            groups.push(group_of(
                &ctx,
                GroupKind::Candidate,
                id.clone(),
                def.map(|d| d.title.clone())
                    .unwrap_or_else(|| format!("candidate finding {id}")),
                def,
                occs.clone(),
            ));
        }
    }

    // failures no finding takes: one group per corpus family and kind of failure
    let mut untriaged: BTreeMap<(String, String), Vec<Occurrence>> = BTreeMap::new();
    for o in &collection.unmapped {
        let family = o
            .entry
            .as_deref()
            .and_then(|e| ctx.manifest.get(e))
            .map(|e| e.family.clone())
            .unwrap_or_else(|| "no corpus entry".into());
        untriaged
            .entry((family, failure_kind(kind, &o.what)))
            .or_default()
            .push(o.clone());
    }
    for (n, ((family, what), occs)) in untriaged.into_iter().enumerate() {
        groups.push(group_of(
            &ctx,
            GroupKind::Untriaged,
            format!("untriaged-{}", n + 1),
            format!("{what} in {family}"),
            None,
            occs,
        ));
    }

    let comparison = match (&opts.baseline, &opts.known_failures) {
        (Some(_), Some(_)) => {
            bail!("give either a baseline or a known-failures directory, not both")
        }
        (Some(b), None) => Some(baseline::compare_with_run(kind, &current, b)?),
        (None, Some(dir)) => {
            let Some(m) = &matrix_view else {
                bail!("--known-failures applies to a conformance run, and this is a harness run");
            };
            Some(baseline::compare_with_known(m, &current, dir)?)
        }
        (None, None) => None,
    }
    .map(|mut c| {
        let of = group_index(&groups);
        for ch in c.new.iter_mut().chain(c.changed.iter_mut()) {
            ch.group = of.get(&(ch.entry.clone(), ch.at.clone())).cloned();
        }
        c
    });

    Ok(Report {
        schema: REPORT_SCHEMA,
        kind,
        input: InputInfo {
            path: input.to_string_lossy().into_owned(),
            sha256,
        },
        subject,
        corpus: manifest.corpus_version.clone(),
        totals,
        failures: collection.summary.failures,
        groups,
        comparison,
        input_problems: collection.input_problems,
        site: ctx.site.clone(),
    })
}

/// The kind of failure an untriaged occurrence is: the problem and fact of a harness problem,
/// the verdict of a whole harness entry, or a conformance cell's result.
fn failure_kind(kind: RunKind, what: &str) -> String {
    match kind {
        RunKind::Harness => what.split(':').next().unwrap_or(what).trim().to_owned(),
        RunKind::Conformance => what.split(" (").next().unwrap_or(what).trim().to_owned(),
    }
}

fn group_of(
    ctx: &Ctx<'_>,
    kind: GroupKind,
    id: String,
    title: String,
    def: Option<&triage::FindingDef>,
    mut occurrences: Vec<Occurrence>,
) -> Group {
    occurrences.sort();
    let entries: Vec<String> = occurrences
        .iter()
        .filter_map(|o| o.entry.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let caps: Vec<String> = entries
        .iter()
        .filter_map(|e| ctx.manifest.get(e.as_str()))
        .flat_map(|e| e.caps.iter().cloned())
        .chain(def.into_iter().flat_map(|d| d.caps.iter().cloned()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let (fields, guide) = ctx.fields_and_guide(&entries);
    let minimal = def
        .and_then(|d| d.minimal_entry.clone())
        .filter(|e| entries.contains(e))
        .or_else(|| ctx.smallest(&entries));
    Group {
        kind,
        id,
        title,
        severity: def.map(|d| d.severity.name().into()),
        status: def.map(|d| d.status.name().into()),
        decision: None,
        link: def.and_then(|d| d.link.clone()),
        page: None,
        cause: def.map(|d| d.cause.clone()),
        caps,
        reproduce: minimal
            .as_deref()
            .and_then(|e| ctx.reproduce(e, &occurrences)),
        recipe: minimal.as_deref().and_then(|e| ctx.recipe(e)),
        minimal_entry: minimal,
        entries,
        fields,
        guide,
        occurrences,
    }
}

/// Per (entry, network or testee), the group whose occurrences hold it.
fn group_index(groups: &[Group]) -> BTreeMap<(String, String), String> {
    let mut out = BTreeMap::new();
    for g in groups {
        for o in &g.occurrences {
            if let Some(e) = &o.entry {
                out.entry((e.clone(), o.at.clone()))
                    .or_insert_with(|| g.id.clone());
            }
        }
    }
    out
}

/// Writes the HTML page.
pub fn write_html(r: &Report, path: &Path) -> Result<()> {
    std::fs::write(path, html::render(r)).with_context(|| format!("writing {}", path.display()))
}

/// Writes the pull request summary.
pub fn write_summary(r: &Report, path: &Path, html_name: Option<&str>) -> Result<()> {
    std::fs::write(path, summary::render(r, html_name))
        .with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(
            shell_quote("adapters/bin/wb-adapter"),
            "adapters/bin/wb-adapter"
        );
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn untriaged_kinds() {
        assert_eq!(
            failure_kind(RunKind::Harness, "dropped token-event: transfer from B..."),
            "dropped token-event"
        );
        assert_eq!(
            failure_kind(RunKind::Conformance, "error (2 check(s): a, b)"),
            "error"
        );
    }
}
