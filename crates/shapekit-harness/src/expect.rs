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

/// Every difference between `report` and `want`, one line each. Empty means the run met
/// its expectations.
pub fn compare(report: &Report, want: &Expectations) -> Vec<String> {
    let target = report
        .describe
        .as_ref()
        .map(|d| (d.target.as_str(), d.target_version.as_str()));
    compare_rows(target, &report.entries, want)
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
