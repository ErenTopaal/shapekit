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
}

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
        if f.harness.is_empty() && f.conformance.is_empty() && f.lint.is_empty() {
            bail!("finding {} has no matcher, so nothing can map to it", f.id);
        }
    }
    for f in &t.findings {
        for s in &f.siblings {
            if !seen.contains(s) {
                bail!("finding {} names sibling {s}, which is not a finding", f.id);
            }
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
    use super::glob;

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
