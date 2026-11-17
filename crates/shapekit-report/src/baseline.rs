//! New, changed, fixed and unchanged failures against a baseline: an earlier run of the same
//! kind, a harness expectations file, or a conformance run's known-failures directory.
//!
//! A harness failure is one entry on one network; it is the same failure in both runs when
//! its verdict and reason are. A conformance failure is one check of one entry on one
//! testee; it is the same when its result is. Testees are matched by `<sdk>@<version>`,
//! except that an SDK with a single version on each side is matched by the SDK alone, since
//! a pull request build and the run before it report different version strings.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use shapekit_conformance::compare::ResultClass;
use shapekit_conformance::known;
use shapekit_findings::collect::harness_failure;
use shapekit_findings::inputs::{self, MatrixView};
use shapekit_harness::report::Report as HarnessReport;

use crate::{Change, Comparison, RunKind, resolve_input};

/// (entry, network or testee key, check).
pub type Key = (String, String, Option<String>);

/// One run's results as far as a comparison needs them.
#[derive(Debug, Clone, Default)]
pub struct Units {
    /// Failures and their result.
    pub failing: BTreeMap<Key, String>,
    /// Results that are neither a pass nor a failure (`out-of-scope`, `unsupported`).
    pub other: BTreeMap<Key, String>,
    /// Every (entry, network or testee) the run covered.
    pub covered: BTreeSet<(String, String)>,
    /// Per testee key, its sdk and version.
    pub testees: BTreeMap<String, (String, String)>,
}

fn verdict_label(verdict: &str, reason: Option<&str>) -> String {
    match reason {
        Some(r) => format!("{verdict} ({r})"),
        None => verdict.to_owned(),
    }
}

pub fn harness_units(r: &HarnessReport) -> Units {
    let mut u = Units::default();
    for row in &r.entries {
        let key = (row.entry.clone(), row.network.clone(), None);
        let v = row.result.verdict.name();
        let label = verdict_label(v, row.result.reason.map(|x| x.name()));
        u.covered.insert((row.entry.clone(), row.network.clone()));
        if harness_failure(v) {
            u.failing.insert(key, label);
        } else if v != "pass" {
            u.other.insert(key, label);
        }
    }
    u
}

pub fn matrix_units(m: &MatrixView) -> Units {
    let mut u = Units::default();
    for t in &m.testees {
        u.testees
            .insert(t.key.clone(), (t.sdk.clone(), t.version.clone()));
    }
    for e in &m.entries {
        for (key, cell) in &e.cells {
            u.covered.insert((e.id.clone(), key.clone()));
            for c in &cell.checks {
                let k = (e.id.clone(), key.clone(), Some(c.check.clone()));
                match ResultClass::parse(&c.result) {
                    Some(r) if r.is_failure() => {
                        u.failing.insert(k, c.result.clone());
                    }
                    Some(ResultClass::Pass) => {}
                    _ => {
                        u.other.insert(k, c.result.clone());
                    }
                }
            }
        }
    }
    u
}

fn expectation_units(path: &Path) -> Result<Units> {
    let e = shapekit_harness::expect::load(path)?;
    let mut u = Units::default();
    for x in &e.entries {
        let key = (x.entry.clone(), x.network.clone(), None);
        let label = verdict_label(&x.verdict, x.reason.as_deref());
        u.covered.insert((x.entry.clone(), x.network.clone()));
        if harness_failure(&x.verdict) {
            u.failing.insert(key, label);
        } else if x.verdict != "pass" {
            u.other.insert(key, label);
        }
    }
    Ok(u)
}

/// Rewrites testee keys to the bare sdk for every sdk with exactly one version on both sides.
fn normalise(a: &Units, b: &Units) -> (Units, Units, BTreeMap<String, String>) {
    let per_sdk = |u: &Units| {
        let mut m: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (key, (sdk, _)) in &u.testees {
            m.entry(sdk.clone()).or_default().insert(key.clone());
        }
        m
    };
    let (sa, sb) = (per_sdk(a), per_sdk(b));
    let single: BTreeSet<&String> = sa
        .iter()
        .filter(|(sdk, keys)| keys.len() == 1 && sb.get(*sdk).is_some_and(|k| k.len() == 1))
        .map(|(sdk, _)| sdk)
        .collect();
    let rename = |u: &Units| {
        let map: BTreeMap<String, String> = u
            .testees
            .iter()
            .filter(|(_, (sdk, _))| single.contains(sdk))
            .map(|(key, (sdk, _))| (key.clone(), sdk.clone()))
            .collect();
        let r = |at: &String| map.get(at).cloned().unwrap_or_else(|| at.clone());
        let keyed = |m: &BTreeMap<Key, String>| {
            m.iter()
                .map(|((e, at, c), v)| ((e.clone(), r(at), c.clone()), v.clone()))
                .collect()
        };
        let out = Units {
            failing: keyed(&u.failing),
            other: keyed(&u.other),
            covered: u.covered.iter().map(|(e, at)| (e.clone(), r(at))).collect(),
            testees: BTreeMap::new(),
        };
        (out, map)
    };
    let (na, map_a) = rename(a);
    let (nb, _) = rename(b);
    // normalised name back to the current run's own key
    let back = map_a.into_iter().map(|(k, sdk)| (sdk, k)).collect();
    (na, nb, back)
}

fn change(
    key: &Key,
    back: &BTreeMap<String, String>,
    before: Option<&String>,
    now: Option<&String>,
) -> Change {
    Change {
        entry: key.0.clone(),
        at: back.get(&key.1).cloned().unwrap_or_else(|| key.1.clone()),
        check: key.2.clone(),
        before: before.cloned(),
        now: now.cloned(),
        group: None,
    }
}

/// Compares the run with an earlier run of the same kind, or a harness run with an
/// expectations file (`.toml`).
pub fn compare_with_run(kind: RunKind, current: &Units, path: &Path) -> Result<Comparison> {
    let (label, base) = if path.extension().is_some_and(|x| x == "toml") {
        if kind != RunKind::Harness {
            bail!(
                "{} is a harness expectations file; a conformance run takes an earlier matrix or --known-failures",
                path.display()
            );
        }
        ("expectations", expectation_units(path)?)
    } else {
        let (bk, file) = resolve_input(path)?;
        if bk != kind {
            bail!(
                "the baseline {} is a {} run, and this is a {} run",
                path.display(),
                bk.name(),
                kind.name()
            );
        }
        let (bytes, _) = inputs::read(&file)?;
        match kind {
            RunKind::Harness => (
                "harness report",
                harness_units(&inputs::harness_report(&bytes, &file)?),
            ),
            RunKind::Conformance => {
                let f = file.clone();
                let m = shapekit_findings::parse_on_big_stack(move || inputs::matrix(&bytes, &f))?;
                ("conformance matrix", matrix_units(&m))
            }
        }
    };
    let (now, before, back) = normalise(current, &base);
    let mut c = Comparison {
        kind: label.into(),
        path: path.to_string_lossy().into_owned(),
        new: Vec::new(),
        changed: Vec::new(),
        fixed: Vec::new(),
        unchanged: 0,
        not_rerun: 0,
    };
    for (key, result) in &now.failing {
        match before.failing.get(key) {
            Some(b) if b == result => c.unchanged += 1,
            Some(b) => c.changed.push(change(key, &back, Some(b), Some(result))),
            None => c
                .new
                .push(change(key, &back, before.other.get(key), Some(result))),
        }
    }
    for (key, result) in &before.failing {
        if now.failing.contains_key(key) {
            continue;
        }
        if now.covered.contains(&(key.0.clone(), key.1.clone())) {
            let passes = "pass".to_owned();
            let label = now.other.get(key).unwrap_or(&passes);
            c.fixed.push(change(key, &back, Some(result), Some(label)));
        } else {
            c.not_rerun += 1;
        }
    }
    Ok(c)
}

/// Compares a conformance run with the known-failures lists in `dir`, the way the runner
/// does: a failing check without a line is new, one listed with another result changed, and
/// a listed check that no longer fails fixed.
pub fn compare_with_known(m: &MatrixView, current: &Units, dir: &Path) -> Result<Comparison> {
    let sdks: BTreeSet<&str> = m.testees.iter().map(|t| t.sdk.as_str()).collect();
    let mut lines: BTreeMap<&str, Vec<known::KnownLine>> = BTreeMap::new();
    for sdk in &sdks {
        lines.insert(
            sdk,
            known::load(dir, sdk)
                .with_context(|| format!("reading the known failures of {sdk}"))?,
        );
    }
    let mut c = Comparison {
        kind: "known-failures list".into(),
        path: dir.to_string_lossy().into_owned(),
        new: Vec::new(),
        changed: Vec::new(),
        fixed: Vec::new(),
        unchanged: 0,
        not_rerun: 0,
    };
    let none = BTreeMap::new();
    for (key, result) in &current.failing {
        let Some((sdk, version)) = current.testees.get(&key.1) else {
            continue;
        };
        let check = key.2.as_deref().unwrap_or_default();
        let line = lines
            .get(sdk.as_str())
            .and_then(|ls| ls.iter().find(|l| l.covers_check(&key.0, version, check)));
        match line {
            Some(l) if l.result.as_str() == result => c.unchanged += 1,
            Some(l) => c.changed.push(change(
                key,
                &none,
                Some(&l.result.as_str().to_owned()),
                Some(result),
            )),
            None => c.new.push(change(key, &none, None, Some(result))),
        }
    }
    let several = sdks.len() > 1;
    for (sdk, ls) in &lines {
        for l in ls {
            let keys: Vec<&String> = current
                .testees
                .iter()
                .filter(|(_, (s, v))| s == sdk && l.version.as_deref().is_none_or(|lv| lv == v))
                .map(|(k, _)| k)
                .collect();
            let ran: Vec<&&String> = keys
                .iter()
                .filter(|k| current.covered.contains(&(l.entry.clone(), (**k).clone())))
                .collect();
            if ran.is_empty() {
                c.not_rerun += l.checks.len();
                continue;
            }
            for k in ran {
                for check in &l.checks {
                    let key = (l.entry.clone(), (*k).clone(), Some(check.clone()));
                    if current.failing.contains_key(&key) {
                        continue;
                    }
                    // without other sdks there were no peers to diverge from
                    if l.result == ResultClass::DivergesFromPeers && !several {
                        continue;
                    }
                    let passes = "pass".to_owned();
                    let now = current.other.get(&key).unwrap_or(&passes);
                    c.fixed.push(change(
                        &key,
                        &none,
                        Some(&l.result.as_str().to_owned()),
                        Some(now),
                    ));
                }
            }
        }
    }
    c.fixed.sort();
    Ok(c)
}
