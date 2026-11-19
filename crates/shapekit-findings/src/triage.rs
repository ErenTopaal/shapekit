//! `findings/triage.toml`: the triaged root causes, how each failure is recognised as one of
//! them, and what was decided upstream. The collector never invents a grouping; it applies
//! these matchers and reports whatever no matcher takes.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

pub const TRIAGE_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    /// Ingestion stops: the target retries the same ledger or exits.
    Halt,
    /// Facts the ledger shows are not stored at all.
    DataLoss,
    /// Facts are stored with other values than the ledger shows, or invented.
    WrongData,
    /// An SDK answers differently from the network (accepts what core refuses or the
    /// reverse, orders or encodes differently).
    Divergence,
    /// A missing limit: input the network never emits can exhaust the reader, or a cap that
    /// is wider than core's.
    Robustness,
}

impl Severity {
    pub fn name(self) -> &'static str {
        match self {
            Severity::Halt => "halt",
            Severity::DataLoss => "data-loss",
            Severity::WrongData => "wrong-data",
            Severity::Divergence => "divergence",
            Severity::Robustness => "robustness",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// An issue body is written but nothing is filed yet. Counts as lacking a link.
    Draft,
    /// Filed upstream; `link` is the issue.
    Filed,
    /// An upstream issue or fix already covers it; `link` is that issue or pull request.
    DuplicateOf,
    /// Reported through the upstream's private security channel; `reference` is the report id.
    PrivateReport,
    /// Deliberately not filed; `reason` says why.
    WontFix,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Draft => "draft",
            Status::Filed => "filed",
            Status::DuplicateOf => "duplicate-of",
            Status::PrivateReport => "private-report",
            Status::WontFix => "wont-fix",
        }
    }
}

/// Recognises harness problems (or whole entries, for stops) as this finding. Every given
/// member must match; an absent member matches anything.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HarnessMatch {
    /// `describe.target` of the report.
    pub target: Option<String>,
    /// Entry id patterns, `*` matching any run of characters.
    #[serde(default)]
    pub entries: Vec<String>,
    #[serde(default)]
    pub networks: Vec<String>,
    /// Entry verdicts (`dropped`, `mishandled`, `not-evaluated`).
    #[serde(default)]
    pub verdicts: Vec<String>,
    /// Entry reasons (`halted`, `wrong-value`, ...).
    #[serde(default)]
    pub reasons: Vec<String>,
    /// Problem kinds (`dropped`, `wrong-value`, `extra`, `undecodable-address`). With this
    /// set the matcher only takes problems, never a whole entry without problems.
    #[serde(default)]
    pub problems: Vec<String>,
    /// Problem facts: `participant` or a vocabulary kind.
    #[serde(default)]
    pub facts: Vec<String>,
    /// The `event` field of a token-event problem, on whichever side it is present.
    #[serde(default)]
    pub events: Vec<String>,
    /// Field value prefixes, on any side of the problem that has the field.
    #[serde(default)]
    pub prefix: BTreeMap<String, Vec<String>>,
    /// Fields that are null or absent on every side of the problem that is present.
    #[serde(default)]
    pub null: Vec<String>,
    /// Substring of the problem detail, or of the entry detail for an entry without problems.
    pub detail_contains: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConformanceMatch {
    /// `rust`, `go` or `js`.
    pub sdk: String,
    /// Entry id patterns.
    pub entries: Vec<String>,
    /// Exact versions; every version when empty.
    #[serde(default)]
    pub versions: Vec<String>,
    /// Check name patterns (`*/tx_hash *`); every failing check of a matched cell when empty.
    /// Two findings can split one cell's failing checks: each takes the checks it names.
    #[serde(default)]
    pub checks: Vec<String>,
}

/// Evidence from the answers SDKs give on implementation-defined entries. Those answers are
/// recorded, not judged, so they are never failures; a finding cites them when the answer
/// itself is the defect. Two forms:
/// - `sdk` and `answers`: the SDK gave one of `answers` on an entry, for example rust reading
///   the bool word 2 as `{"json":{"bool":false}}`;
/// - `split`: the SDKs of one matrix gave at least `split` different answers on an entry,
///   the disagreement itself being the evidence.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerMatch {
    pub sdk: Option<String>,
    /// Entry id patterns.
    pub entries: Vec<String>,
    /// Check names, such as `any/decode_json`; every check when empty.
    #[serde(default)]
    pub checks: Vec<String>,
    /// Exact versions of `sdk`; every version when empty.
    #[serde(default)]
    pub versions: Vec<String>,
    /// Answers as the matrix records them: a string answer (`rejected`) as itself, anything
    /// else as compact JSON.
    #[serde(default)]
    pub answers: Vec<String>,
    /// The least number of different answers on one check.
    pub split: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LintMatch {
    pub target: String,
    pub rule: String,
    pub function: String,
    pub package: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingDef {
    pub id: String,
    pub title: String,
    /// The defect in one plain sentence, for listings where the issue title is too long or
    /// carries an upstream prefix; `findings check` refuses it empty.
    pub short: Option<String>,
    /// The upstream project, for example `wallet-backend` or `go-stellar-sdk`.
    pub component: String,
    pub severity: Severity,
    pub status: Status,
    /// Issue or pull request for `filed` and `duplicate-of`.
    pub link: Option<String>,
    /// Private report reference for `private-report`; never a link to the report's content.
    pub reference: Option<String>,
    /// Why, for `wont-fix`.
    pub reason: Option<String>,
    /// The drafted issue body, a path relative to the repository root.
    pub draft: Option<String>,
    /// CAPs to add to those of the matched entries (for a linter finding, which has none).
    #[serde(default)]
    pub caps: Vec<String>,
    /// Overrides the smallest matched entry.
    pub minimal_entry: Option<String>,
    /// Upstream issues or pull requests that touch the same code but are not this finding.
    #[serde(default)]
    pub related: Vec<String>,
    /// Other finding ids filed as siblings, beyond those that share a corpus entry.
    #[serde(default)]
    pub siblings: Vec<String>,
    /// Free text: the root cause in one or two sentences.
    pub cause: String,
    #[serde(default)]
    pub harness: Vec<HarnessMatch>,
    #[serde(default)]
    pub conformance: Vec<ConformanceMatch>,
    #[serde(default)]
    pub answer: Vec<AnswerMatch>,
    #[serde(default)]
    pub lint: Vec<LintMatch>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LintInput {
    pub target: String,
    /// The exact build the linter ran on.
    pub version: String,
    /// The `indexerlint -format json` output.
    pub result: String,
    /// How to reproduce it, run from a checkout of the target at `version`.
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inputs {
    pub manifest: String,
    #[serde(default)]
    pub harness: Vec<String>,
    #[serde(default)]
    pub matrices: Vec<String>,
    #[serde(default)]
    pub lint: Vec<LintInput>,
    /// Where each committed input under `findings/inputs/` came from;
    /// `shapekit findings import-inputs` reads `from`, reduces it and writes `to`.
    #[serde(default)]
    pub import: Vec<ImportDef>,
    /// The directory the committed inputs and their provenance file live in, relative to the
    /// repository root; `findings/inputs` when absent. A second triage, such as the candidate
    /// lane's, keeps its inputs apart with its own directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
}

impl Inputs {
    pub fn dir(&self) -> &str {
        self.dir.as_deref().unwrap_or(INPUTS_DIR)
    }

    /// The provenance file of the committed inputs.
    pub fn provenance_file(&self) -> String {
        format!("{}/provenance.json", self.dir())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InputKind {
    Harness,
    Conformance,
    Lint,
}

impl InputKind {
    pub fn name(self) -> &'static str {
        match self {
            InputKind::Harness => "harness",
            InputKind::Conformance => "conformance",
            InputKind::Lint => "lint",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportDef {
    pub kind: InputKind,
    /// The raw run output, relative to the repository root (usually under the gitignored
    /// `target/`).
    pub from: String,
    /// The committed, reduced copy that `[inputs]` names.
    pub to: String,
    /// The command that wrote `from`.
    pub command: String,
}

/// The directory committed inputs live in, so a clean checkout can collect.
pub const INPUTS_DIR: &str = "findings/inputs";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetDef {
    /// `owner/repo` on GitHub.
    pub repo: String,
    /// The adapter command line for a reproduction; `{version}` is the target build.
    pub adapter: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Triage {
    pub schema: u32,
    /// A second triage, relative to the repository root, whose findings are triaged but not
    /// yet promoted (the candidate lane's `candidate/findings/triage.toml`). A failure no
    /// finding of this triage takes, but one of those does, is held by that candidate
    /// finding: counted and listed on its own, neither unmapped nor dropped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidates: Option<String>,
    pub inputs: Inputs,
    #[serde(default)]
    pub targets: BTreeMap<String, TargetDef>,
    #[serde(rename = "finding", default)]
    pub findings: Vec<FindingDef>,
}

pub fn load(path: &Path) -> Result<Triage> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let t: Triage = toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    validate(&t).with_context(|| format!("checking {}", path.display()))?;
    Ok(t)
}

pub fn validate(t: &Triage) -> Result<()> {
    if t.schema != TRIAGE_SCHEMA {
        bail!("schema {} is not {TRIAGE_SCHEMA}", t.schema);
    }
    let mut seen = std::collections::BTreeSet::new();
    for f in &t.findings {
        if !seen.insert(&f.id) {
            bail!("finding {} is listed twice", f.id);
        }
        if f.harness.is_empty()
            && f.conformance.is_empty()
            && f.answer.is_empty()
            && f.lint.is_empty()
        {
            bail!("finding {} has no matcher, so nothing can map to it", f.id);
        }
        // the first matcher that takes a failure keeps it, so one without a constraint would
        // absorb every later finding's failures
        for (i, m) in f.harness.iter().enumerate() {
            if let Some(why) = harness_unconstrained(m) {
                bail!("finding {} harness matcher {}: {why}", f.id, i + 1);
            }
        }
        for (i, m) in f.conformance.iter().enumerate() {
            if let Some(why) = conformance_unconstrained(m) {
                bail!("finding {} conformance matcher {}: {why}", f.id, i + 1);
            }
        }
        for (i, m) in f.answer.iter().enumerate() {
            if let Some(why) = answer_unconstrained(m) {
                bail!("finding {} answer matcher {}: {why}", f.id, i + 1);
            }
        }
    }
    validate_imports(&t.inputs)?;
    for f in &t.findings {
        for s in &f.siblings {
            if !seen.contains(s) {
                bail!("finding {} names sibling {s}, which is not a finding", f.id);
            }
        }
    }
    Ok(())
}

fn wildcard_only(pattern: &str) -> bool {
    pattern.chars().all(|c| c == '*')
}

/// Why `m` would take every failure of its target, if it would.
pub fn harness_unconstrained(m: &HarnessMatch) -> Option<String> {
    if let Some(p) = m.entries.iter().find(|p| wildcard_only(p)) {
        return Some(format!("entry pattern {p:?} matches every entry"));
    }
    let constrained = !m.entries.is_empty()
        || !m.networks.is_empty()
        || !m.verdicts.is_empty()
        || !m.reasons.is_empty()
        || !m.problems.is_empty()
        || !m.facts.is_empty()
        || !m.events.is_empty()
        || !m.prefix.is_empty()
        || !m.null.is_empty()
        || m.detail_contains.as_deref().is_some_and(|d| !d.is_empty());
    (!constrained).then(|| "no constraint beyond the target, so it takes every failure".into())
}

/// Why `m` would take every failure of its SDK, or none, if so.
pub fn conformance_unconstrained(m: &ConformanceMatch) -> Option<String> {
    if m.entries.is_empty() {
        return Some("no entry pattern, so it can never match".into());
    }
    m.entries
        .iter()
        .find(|p| wildcard_only(p))
        .map(|p| format!("entry pattern {p:?} matches every entry"))
}

/// Why `m` is not one of the two forms [`AnswerMatch`] allows, or could match every entry.
pub fn answer_unconstrained(m: &AnswerMatch) -> Option<String> {
    if m.entries.is_empty() {
        return Some("no entry pattern, so it can never match".into());
    }
    if let Some(p) = m.entries.iter().find(|p| wildcard_only(p)) {
        return Some(format!("entry pattern {p:?} matches every entry"));
    }
    match (&m.sdk, m.split) {
        (Some(_), None) if m.answers.is_empty() => {
            Some("an sdk without answers would cite every answer it gives".into())
        }
        (Some(_), None) => None,
        (None, Some(n)) if n < 2 => Some(format!(
            "split {n} is met by any answer; it needs at least 2"
        )),
        (None, Some(_)) if !m.answers.is_empty() || !m.versions.is_empty() => {
            Some("a split matcher compares every sdk, so it takes no answers or versions".into())
        }
        (None, Some(_)) => None,
        (Some(_), Some(_)) => Some("give either sdk and answers, or split, not both".into()),
        (None, None) => Some("give either sdk and answers, or split".into()),
    }
}

fn validate_imports(inputs: &Inputs) -> Result<()> {
    let named: std::collections::BTreeSet<&str> = inputs
        .harness
        .iter()
        .chain(&inputs.matrices)
        .map(String::as_str)
        .chain(inputs.lint.iter().map(|l| l.result.as_str()))
        .collect();
    let mut tos = std::collections::BTreeSet::new();
    for i in &inputs.import {
        if !tos.insert(i.to.as_str()) {
            bail!("import target {} is listed twice", i.to);
        }
        let dir = inputs.dir();
        let under = Path::new(&i.to)
            .strip_prefix(dir)
            .is_ok_and(|rest| shapekit_manifest::is_safe_relative(&rest.to_string_lossy()));
        if !under {
            bail!("import target {} is not a plain path under {dir}/", i.to);
        }
        if !shapekit_manifest::is_safe_relative(&i.from) {
            bail!(
                "import source {} is not a relative path inside the repository",
                i.from
            );
        }
        if !named.contains(i.to.as_str()) {
            bail!("import target {} is not named in [inputs]", i.to);
        }
        let listed = match i.kind {
            InputKind::Harness => inputs.harness.contains(&i.to),
            InputKind::Conformance => inputs.matrices.contains(&i.to),
            InputKind::Lint => inputs.lint.iter().any(|l| l.result == i.to),
        };
        if !listed {
            bail!("import target {} is not a {} input", i.to, i.kind.name());
        }
    }
    Ok(())
}

/// `*` matches any run of characters, everything else itself.
pub fn glob(pattern: &str, s: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == s;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !s.starts_with(first) || s.len() < first.len() + last.len() || !s.ends_with(last) {
        return false;
    }
    let mut rest = &s[first.len()..s.len() - last.len()];
    for mid in &parts[1..parts.len() - 1] {
        match rest.find(mid) {
            Some(i) => rest = &rest[i + mid.len()..],
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matchers_without_a_constraint_are_refused() {
        let empty = HarnessMatch {
            target: Some("t".into()),
            ..HarnessMatch::default()
        };
        assert!(harness_unconstrained(&empty).is_some());
        let star = HarnessMatch {
            entries: vec!["*".into()],
            problems: vec!["dropped".into()],
            ..HarnessMatch::default()
        };
        assert!(harness_unconstrained(&star).is_some());
        let narrow = HarnessMatch {
            entries: vec!["*clawback*".into()],
            ..HarnessMatch::default()
        };
        assert!(harness_unconstrained(&narrow).is_none());
        let all = ConformanceMatch {
            sdk: "go".into(),
            entries: vec!["**".into()],
            ..ConformanceMatch::default()
        };
        assert!(conformance_unconstrained(&all).is_some());
        let none = ConformanceMatch {
            sdk: "go".into(),
            ..ConformanceMatch::default()
        };
        assert!(conformance_unconstrained(&none).is_some());
    }

    #[test]
    fn answer_matchers_take_one_of_two_forms() {
        let e = || vec!["soroban.scval-shapes.synth-bool-value-2".to_owned()];
        let by_sdk = AnswerMatch {
            sdk: Some("rust".into()),
            entries: e(),
            answers: vec!["false".into()],
            ..AnswerMatch::default()
        };
        assert_eq!(answer_unconstrained(&by_sdk), None);
        let split = AnswerMatch {
            entries: e(),
            split: Some(3),
            ..AnswerMatch::default()
        };
        assert_eq!(answer_unconstrained(&split), None);
        for bad in [
            AnswerMatch {
                answers: vec![],
                ..by_sdk.clone()
            },
            AnswerMatch {
                split: Some(2),
                ..by_sdk.clone()
            },
            AnswerMatch {
                split: Some(1),
                ..split.clone()
            },
            AnswerMatch {
                answers: vec!["false".into()],
                ..split.clone()
            },
            AnswerMatch {
                entries: vec!["*".into()],
                ..split.clone()
            },
            AnswerMatch {
                entries: vec![],
                ..by_sdk.clone()
            },
            AnswerMatch {
                entries: e(),
                ..AnswerMatch::default()
            },
        ] {
            assert!(answer_unconstrained(&bad).is_some(), "{bad:?}");
        }
    }

    #[test]
    fn globs() {
        assert!(glob("cap66.*", "cap66.archival.p30-x"));
        assert!(glob("*-envelope", "a-envelope"));
        assert!(glob("a*b*c", "aXXbYYc"));
        assert!(!glob("a*b*c", "aXXcYYb"));
        assert!(glob("exact", "exact"));
        assert!(!glob("exact", "exactly"));
        assert!(!glob("ab*ba", "aba"));
    }
}
