//! Expected verdicts for a fixed harness selection. `harness run` exits 0 whatever the
//! verdicts are, because verdicts are findings about the target; a regression in the adapter's
//! mapping, the oracle or the comparison would then pass unnoticed. An expectations file pins
//! every entry's verdict and reason at one target build, and `harness run --expect` fails on
//! any difference: a changed verdict or reason, an entry missing from the run, an entry the
//! file does not list, or another target build.
//!
//! ```toml
//! schema = 1
//! target = "wallet-backend"
//! target_version = "<full commit>"
//!
//! [[entry]]
//! entry = "cap73.sac-trust.p30-..."
//! network = "testnet"
//! verdict = "mishandled"
//! reason = "wrong-value"   # absent when the verdict has none
//! ```
//!
//! `shapekit harness expect --report <harness-report.json> --write <file>` writes one from a
//! real run.
//!
//! [`Mode::Regressions`] reads the same file as a floor instead: it ignores the target build,
//! so a pull request (a new commit) can be checked against the expectations committed on its
//! default branch, and fails only when a verdict gets worse. A failure (`mishandled`,
//! `dropped`, `not-evaluated`) the file does not expect, or one whose verdict or reason
//! changed, fails; a failure that went away is reported as fixed and passes.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::compare::{Reason, Verdict};
use crate::report::Report;
use crate::run::EntryRow;

pub const EXPECT_SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expectations {
    pub schema: u32,
    pub target: String,
    pub target_version: String,
    #[serde(rename = "entry", default)]
    pub entries: Vec<ExpectedEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedEntry {
    pub entry: String,
    pub network: String,
    pub verdict: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

fn label(verdict: &str, reason: Option<&str>) -> String {
    match reason {
        Some(r) => format!("{verdict} ({r})"),
        None => verdict.to_owned(),
    }
}

pub fn load(path: &Path) -> Result<Expectations> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let e: Expectations =
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    if e.schema != EXPECT_SCHEMA {
        bail!(
            "{}: schema {} is not {EXPECT_SCHEMA}",
            path.display(),
            e.schema
        );
    }
    let mut seen = std::collections::BTreeSet::new();
    for x in &e.entries {
        if Verdict::parse(&x.verdict).is_none() {
            bail!(
                "{}: {}: unknown verdict {:?}",
                path.display(),
                x.entry,
                x.verdict
            );
        }
        if let Some(r) = &x.reason
            && Reason::parse(r).is_none()
        {
            bail!("{}: {}: unknown reason {r:?}", path.display(), x.entry);
        }
        if !seen.insert((&x.entry, &x.network)) {
            bail!(
                "{}: {} ({}) is listed twice",
                path.display(),
                x.entry,
                x.network
            );
        }
    }
    if e.entries.is_empty() {
        bail!("{}: no entry, so it would expect nothing", path.display());
    }
    Ok(e)
}

/// The expectations a report meets, for committing after a reviewed run.
pub fn from_report(report: &Report) -> Result<Expectations> {
    let d = report
        .describe
        .as_ref()
        .context("the report has no describe, so the target build is unknown")?;
    if let Some(c) = report.chains.iter().find(|c| c.error.is_some()) {
        bail!(
            "chain {} ({}) did not run, so its verdicts cannot be expected: {}",
            c.chain,
            c.network,
            c.error.as_deref().unwrap_or("")
        );
    }
    let mut entries: Vec<ExpectedEntry> = report
        .entries
        .iter()
        .map(|r| ExpectedEntry {
            entry: r.entry.clone(),
            network: r.network.clone(),
            verdict: r.result.verdict.name().to_owned(),
            reason: r.result.reason.map(|x| x.name().to_owned()),
        })
        .collect();
    entries.sort();
    Ok(Expectations {
        schema: EXPECT_SCHEMA,
        target: d.target.clone(),
        target_version: d.target_version.clone(),
        entries,
    })
}

pub fn render(e: &Expectations, header: &str) -> Result<String> {
    let mut out = String::new();
    for line in header.lines() {
        let _ = writeln!(out, "# {line}");
    }
    out.push_str(&toml::to_string(e).context("rendering expectations")?);
    Ok(out)
}

/// How a run is held to an expectations file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Every verdict and reason as listed, at the listed target build.
    #[default]
    Exact,
    /// No verdict worse than listed, at any build of the listed target.
    Regressions,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Exact => "exact",
            Mode::Regressions => "regressions",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Mode::Exact, Mode::Regressions]
            .into_iter()
            .find(|m| m.name() == s)
    }
}

/// The result of holding a run to its expectations: what fails it, and what is worth a line
/// without failing it (a fixed failure, another build in regressions mode).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    pub failures: Vec<String>,
    pub notes: Vec<String>,
}

impl Outcome {
    pub fn met(&self) -> bool {
        self.failures.is_empty()
    }
}

/// The verdicts a pull request summary counts as failures, the same set `shapekit report`
/// classes as new, changed or fixed.
pub fn is_failure(verdict: &str) -> bool {
    matches!(verdict, "dropped" | "mishandled" | "not-evaluated")
}

/// Every difference between `report` and `want`, one line each. Empty means the run met
/// its expectations.
pub fn compare(report: &Report, want: &Expectations) -> Vec<String> {
    check(report, want, Mode::Exact).failures
}

/// Holds `report` to `want` in `mode`.
pub fn check(report: &Report, want: &Expectations, mode: Mode) -> Outcome {
    let target = report
        .describe
        .as_ref()
        .map(|d| (d.target.as_str(), d.target_version.as_str()));
    match mode {
        Mode::Exact => Outcome {
            failures: compare_rows(target, &report.entries, want),
            notes: Vec::new(),
        },
        Mode::Regressions => regressions(target, &report.entries, want),
    }
}

fn regressions(target: Option<(&str, &str)>, rows: &[EntryRow], want: &Expectations) -> Outcome {
    let mut o = Outcome::default();
    match target {
        None => o
            .failures
            .push("the report has no describe, so the target is unknown".into()),
        Some((t, _)) if t != want.target => o.failures.push(format!(
            "the run is of {t}, the expectations are of {}",
            want.target
        )),
        Some((_, v)) if v != want.target_version => o.notes.push(format!(
            "the run is of {} {v}, the expectations were written at {}; regressions mode compares the verdicts only",
            want.target, want.target_version
        )),
        Some(_) => {}
    }
    let got: BTreeMap<(&str, &str), (&str, Option<&str>)> = rows
        .iter()
        .map(|r| {
            (
                (r.entry.as_str(), r.network.as_str()),
                (r.result.verdict.name(), r.result.reason.map(|x| x.name())),
            )
        })
        .collect();
    let expected: BTreeMap<(&str, &str), (&str, Option<&str>)> = want
        .entries
        .iter()
        .map(|x| {
            (
                (x.entry.as_str(), x.network.as_str()),
                (x.verdict.as_str(), x.reason.as_deref()),
            )
        })
        .collect();
    for ((entry, network), (v, r)) in &expected {
        let was = label(v, *r);
        let Some((gv, gr)) = got.get(&(*entry, *network)) else {
            o.failures.push(format!(
                "{entry} ({network}): expected {was}, not in the run"
            ));
            continue;
        };
        if gv == v && gr == r {
            continue;
        }
        let now = label(gv, *gr);
        match (is_failure(v), is_failure(gv)) {
            (false, true) => o
                .failures
                .push(format!("{entry} ({network}): new failure, expected {was}, got {now}")),
            (true, true) => o.failures.push(format!(
                "{entry} ({network}): changed failure, expected {was}, got {now}"
            )),
            (true, false) => o.notes.push(format!(
                "{entry} ({network}): fixed, expected {was}, got {now}; regenerate the expectations to keep it fixed"
            )),
            (false, false) => o
                .notes
                .push(format!("{entry} ({network}): expected {was}, got {now}")),
        }
    }
    for ((entry, network), (gv, gr)) in &got {
        if expected.contains_key(&(*entry, *network)) {
            continue;
        }
        let now = label(gv, *gr);
        if is_failure(gv) {
            o.failures.push(format!(
                "{entry} ({network}): new failure, got {now}, and the expectations do not list it"
            ));
        } else {
            o.notes.push(format!(
                "{entry} ({network}): got {now}, and the expectations do not list it"
            ));
        }
    }
    o
}

fn compare_rows(
    target: Option<(&str, &str)>,
    rows: &[EntryRow],
    want: &Expectations,
) -> Vec<String> {
    let mut out = Vec::new();
    match target {
        None => out.push("the report has no describe, so the target build is unknown".into()),
        Some((t, v)) => {
            if t != want.target || v != want.target_version {
                out.push(format!(
                    "the run is of {t} {v}, the expectations are of {} {}; rerun at that build or regenerate the expectations with `shapekit harness expect`",
                    want.target, want.target_version
                ));
            }
        }
    }
    let got: BTreeMap<(&str, &str), (&str, Option<&str>)> = rows
        .iter()
        .map(|r| {
            (
                (r.entry.as_str(), r.network.as_str()),
                (r.result.verdict.name(), r.result.reason.map(|x| x.name())),
            )
        })
        .collect();
    let expected: BTreeMap<(&str, &str), (&str, Option<&str>)> = want
        .entries
        .iter()
        .map(|x| {
            (
                (x.entry.as_str(), x.network.as_str()),
                (x.verdict.as_str(), x.reason.as_deref()),
            )
        })
        .collect();
    for ((entry, network), (v, r)) in &expected {
        match got.get(&(*entry, *network)) {
            None => out.push(format!(
                "{entry} ({network}): expected {}, not in the run",
                label(v, *r)
            )),
            Some((gv, gr)) if gv != v || gr != r => out.push(format!(
                "{entry} ({network}): expected {}, got {}",
                label(v, *r),
                label(gv, *gr)
            )),
            Some(_) => {}
        }
    }
    for ((entry, network), (gv, gr)) in &got {
        if !expected.contains_key(&(*entry, *network)) {
            out.push(format!(
                "{entry} ({network}): got {}, but the expectations do not list it",
                label(gv, *gr)
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn want() -> Expectations {
        Expectations {
            schema: EXPECT_SCHEMA,
            target: "t".into(),
            target_version: "v1".into(),
            entries: vec![
                ExpectedEntry {
                    entry: "a".into(),
                    network: "testnet".into(),
                    verdict: "pass".into(),
                    reason: None,
                },
                ExpectedEntry {
                    entry: "b".into(),
                    network: "testnet".into(),
                    verdict: "mishandled".into(),
                    reason: Some("halted".into()),
                },
            ],
        }
    }

    fn row(entry: &str, verdict: Verdict, reason: Option<Reason>) -> EntryRow {
        EntryRow {
            entry: entry.into(),
            family: "f".into(),
            chain: "c".into(),
            network: "testnet".into(),
            ledger: 70,
            result: crate::compare::EntryVerdict {
                verdict,
                reason,
                detail: String::new(),
                problems: Vec::new(),
                unmapped: Vec::new(),
                notes: Vec::new(),
            },
        }
    }

    #[test]
    fn every_difference_is_reported() {
        let w = want();
        let ok = [
            row("a", Verdict::Pass, None),
            row("b", Verdict::Mishandled, Some(Reason::Halted)),
        ];
        assert!(compare_rows(Some(("t", "v1")), &ok, &w).is_empty());
        assert_eq!(compare_rows(Some(("t", "v2")), &ok, &w).len(), 1);
        assert_eq!(compare_rows(None, &ok, &w).len(), 1);
        // a pass turned into dropped, a reason that changed, an entry gone and one new
        let bad = [
            row("a", Verdict::Dropped, None),
            row("c", Verdict::Pass, None),
        ];
        let d = compare_rows(Some(("t", "v1")), &bad, &w);
        assert_eq!(d.len(), 3, "{d:?}");
        assert!(d[0].contains("expected pass, got dropped"), "{d:?}");
        assert!(d[1].contains("not in the run"), "{d:?}");
        assert!(d[2].contains("do not list it"), "{d:?}");
        let other_reason = [
            row("a", Verdict::Pass, None),
            row("b", Verdict::Mishandled, Some(Reason::WrongValue)),
        ];
        assert_eq!(compare_rows(Some(("t", "v1")), &other_reason, &w).len(), 1);
    }

    fn regressions_of(rows: &[EntryRow], version: &str) -> Outcome {
        regressions(Some(("t", version)), rows, &want())
    }

    #[test]
    fn regressions_ignore_the_build() {
        let same = [
            row("a", Verdict::Pass, None),
            row("b", Verdict::Mishandled, Some(Reason::Halted)),
        ];
        let o = regressions_of(&same, "v2");
        assert!(o.met(), "{o:?}");
        assert_eq!(o.notes.len(), 1, "{o:?}");
        assert!(o.notes[0].contains("compares the verdicts only"), "{o:?}");
        assert_eq!(regressions_of(&same, "v1"), Outcome::default());
        let o = regressions(Some(("other", "v1")), &same, &want());
        assert!(
            !o.met() && o.failures[0].contains("expectations are of t"),
            "{o:?}"
        );
        assert!(!regressions(None, &same, &want()).met());
    }

    #[test]
    fn regressions_fail_on_a_new_mishandled_verdict() {
        let rows = [
            row("a", Verdict::Mishandled, Some(Reason::WrongValue)),
            row("b", Verdict::Mishandled, Some(Reason::Halted)),
        ];
        let o = regressions_of(&rows, "v2");
        assert_eq!(o.failures.len(), 1, "{o:?}");
        assert!(
            o.failures[0].starts_with(
                "a (testnet): new failure, expected pass, got mishandled (wrong-value)"
            ),
            "{o:?}"
        );
    }

    #[test]
    fn regressions_fail_on_a_new_dropped_verdict() {
        let rows = [
            row("a", Verdict::Dropped, None),
            row("b", Verdict::Mishandled, Some(Reason::Halted)),
        ];
        let o = regressions_of(&rows, "v2");
        assert_eq!(o.failures.len(), 1, "{o:?}");
        assert!(
            o.failures[0].contains("new failure, expected pass, got dropped"),
            "{o:?}"
        );
    }

    #[test]
    fn regressions_fail_on_a_changed_failure() {
        for b in [
            row("b", Verdict::Mishandled, Some(Reason::WrongValue)),
            row("b", Verdict::Dropped, None),
            row("b", Verdict::NotEvaluated, None),
        ] {
            let rows = [row("a", Verdict::Pass, None), b];
            let o = regressions_of(&rows, "v2");
            assert_eq!(o.failures.len(), 1, "{o:?}");
            assert!(
                o.failures[0]
                    .starts_with("b (testnet): changed failure, expected mishandled (halted)"),
                "{o:?}"
            );
        }
    }

    #[test]
    fn regressions_report_a_fix_and_pass() {
        let rows = [row("a", Verdict::Pass, None), row("b", Verdict::Pass, None)];
        let o = regressions_of(&rows, "v2");
        assert!(o.met(), "{o:?}");
        assert_eq!(o.notes.len(), 2, "{o:?}");
        assert!(
            o.notes[1].starts_with("b (testnet): fixed, expected mishandled (halted), got pass"),
            "{o:?}"
        );
        // out of scope is not a failure either way
        let rows = [
            row("a", Verdict::OutOfScope, None),
            row("b", Verdict::OutOfScope, None),
        ];
        let o = regressions_of(&rows, "v1");
        assert!(o.met(), "{o:?}");
        assert_eq!(o.notes.len(), 2, "{o:?}");
    }

    #[test]
    fn regressions_hold_the_selection() {
        // an expected entry gone fails; an unlisted entry fails only as a failure
        let o = regressions_of(&[row("a", Verdict::Pass, None)], "v1");
        assert_eq!(o.failures.len(), 1, "{o:?}");
        assert!(o.failures[0].contains("not in the run"), "{o:?}");
        let rows = [
            row("a", Verdict::Pass, None),
            row("b", Verdict::Mishandled, Some(Reason::Halted)),
            row("c", Verdict::Pass, None),
            row("d", Verdict::Dropped, None),
        ];
        let o = regressions_of(&rows, "v1");
        assert_eq!(o.failures.len(), 1, "{o:?}");
        assert!(
            o.failures[0].starts_with("d (testnet): new failure, got dropped"),
            "{o:?}"
        );
        assert_eq!(o.notes.len(), 1, "{o:?}");
        assert!(o.notes[0].starts_with("c (testnet): got pass"), "{o:?}");
    }

    #[test]
    fn modes_parse_by_name() {
        for m in [Mode::Exact, Mode::Regressions] {
            assert_eq!(Mode::parse(m.name()), Some(m));
        }
        assert_eq!(Mode::parse("loose"), None);
        assert_eq!(Mode::default(), Mode::Exact);
    }

    #[test]
    fn renders_and_loads_back() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("e.toml");
        let text = render(&want(), "line one\nline two").unwrap();
        assert!(text.starts_with("# line one\n# line two\n"), "{text}");
        std::fs::write(&p, text).unwrap();
        assert_eq!(load(&p).unwrap(), want());
        std::fs::write(&p, "schema = 1\ntarget = \"t\"\ntarget_version = \"v\"\n").unwrap();
        assert!(load(&p).is_err(), "an empty list expects nothing");
    }
}
