//! One coverage snapshot of a testee and the question it answers: did this range run?
//!
//! Rust regions and Go blocks are a fixed set, so a range ran exactly when the snapshot lists
//! it. V8 lists a nested block only while its count differs from the block around it, and its
//! snapshot carries the count-0 ranges as `uncovered` (docs/testee-protocol.md): a range in
//! neither list ran as often as the smallest listed range that contains it. Every testee's
//! snapshot covers the whole process since it started: llvm and go counters only grow, and the
//! JavaScript testee folds V8's reads together, since `Profiler.takePreciseCoverage` resets
//! V8's own counters on every read. A later snapshot therefore lists everything an earlier one
//! did. The comparison does not depend on that: the baseline is the union of its snapshots,
//! and a range is credited to the first ledger whose snapshot shows it.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};

/// A range string split into its file and its span. V8 ranges are `<path>:<start>-<end>`
/// byte offsets; ranges of other forms are only ever compared whole.
fn span(range: &str) -> Option<(&str, u64, u64)> {
    let (path, s) = range.rsplit_once(':')?;
    let (a, b) = s.split_once('-')?;
    Some((path, a.parse().ok()?, b.parse().ok()?))
}

/// Listed ranges of one file as (start, end, ran).
type Listed = Vec<(u64, u64, bool)>;

pub struct Snapshot {
    pub tool: String,
    /// Ranges that ran at least once.
    pub ran: BTreeSet<String>,
    /// For V8: every listed range by file, as (start, end, ran), so the smallest listed range
    /// around an unlisted one can be found.
    listed: Option<BTreeMap<String, Listed>>,
}

impl Snapshot {
    /// Reads a `coverage_snapshot` result.
    pub fn parse(result: &serde_json::Value) -> Result<Snapshot> {
        let tool = result["tool"]
            .as_str()
            .context("coverage_snapshot has no tool")?
            .to_string();
        let ran: BTreeSet<String> = serde_json::from_value(result["ranges"].clone())
            .context("the ranges of coverage_snapshot")?;
        let listed = match result.get("uncovered") {
            None | Some(serde_json::Value::Null) => None,
            Some(u) => {
                let never: BTreeSet<String> = serde_json::from_value(u.clone())
                    .context("the uncovered ranges of coverage_snapshot")?;
                let mut by_file: BTreeMap<String, Listed> = BTreeMap::new();
                for (r, did) in ran
                    .iter()
                    .map(|r| (r, true))
                    .chain(never.iter().map(|r| (r, false)))
                {
                    let (path, a, b) = span(r)
                        .with_context(|| format!("the range {r} is not <path>:<start>-<end>"))?;
                    by_file
                        .entry(path.to_string())
                        .or_default()
                        .push((a, b, did));
                }
                Some(by_file)
            }
        };
        if listed.is_none() && tool.starts_with("v8") {
            anyhow::bail!(
                "{tool:?} sent no uncovered ranges: V8 leaves out a nested range while its count equals the one around it, so without the count-0 ranges a later snapshot can list a block the earlier one ran"
            );
        }
        Ok(Snapshot { tool, ran, listed })
    }

    /// Whether `range` ran by the time of this snapshot.
    pub fn ran(&self, range: &str) -> bool {
        if self.ran.contains(range) {
            return true;
        }
        let Some(listed) = &self.listed else {
            return false;
        };
        let Some((path, a, b)) = span(range) else {
            return false;
        };
        // the smallest listed range containing it decides; a range itself listed as uncovered
        // is its own smallest container
        listed
            .get(path)
            .into_iter()
            .flatten()
            .filter(|(s, e, _)| *s <= a && b <= *e)
            .min_by_key(|(s, e, _)| e - s)
            .is_some_and(|(_, _, did)| *did)
    }
}

/// Credits each range `now` shows as run, and no baseline snapshot does, to `ledger`, unless an
/// earlier ledger already reached it. Snapshots are cumulative, so a range a ledger reaches
/// stays in every later snapshot; the first ledger keeps the credit.
pub fn credit(new: &mut BTreeMap<String, u32>, baseline: &[Snapshot], now: &Snapshot, ledger: u32) {
    for r in &now.ran {
        if !new.contains_key(r) && !baseline.iter().any(|b| b.ran(r)) {
            new.insert(r.clone(), ledger);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unlisted_v8_ranges_take_the_smallest_listed_container() {
        let base = Snapshot::parse(&json!({
            "tool": "v8 precise block coverage",
            "ranges": ["a.js:0-100", "a.js:10-20"],
            "uncovered": ["a.js:30-60", "a.js:40-50"],
        }))
        .unwrap();
        assert!(base.ran("a.js:10-20"));
        // inside 0-100 only: ran as often as it
        assert!(base.ran("a.js:70-80"));
        // inside 30-60, which never ran
        assert!(!base.ran("a.js:32-38"));
        assert!(!base.ran("a.js:40-50"));
        // a file the snapshot does not know never ran
        assert!(!base.ran("b.js:0-5"));
    }

    fn v8(ran: &[&str], uncovered: &[&str]) -> Snapshot {
        Snapshot::parse(&json!({"tool": "v8 x", "ranges": ran, "uncovered": uncovered})).unwrap()
    }

    #[test]
    fn cumulative_snapshots_credit_the_first_ledger_once() {
        // before the corpus, then after it: the second lists everything the first did
        let before = v8(&["a.js:0-100"], &["a.js:30-60", "a.js:70-80"]);
        let after = v8(&["a.js:0-100", "a.js:70-80"], &["a.js:30-60"]);
        assert!(before.ran.is_subset(&after.ran));
        let baseline = [before, after];
        let mut new = BTreeMap::new();
        // ledger 7 reaches 30-60 (listed) and 40-50 (inside it, now listed as run)
        let l7 = v8(
            &["a.js:0-100", "a.js:70-80", "a.js:30-60", "a.js:40-50"],
            &[],
        );
        credit(&mut new, &baseline, &l7, 7);
        // ledger 9 still lists both, cumulative, and adds 72-75 inside 70-80, which the
        // baseline already ran: neither credit moves and 72-75 is not new
        let l9 = v8(
            &[
                "a.js:0-100",
                "a.js:70-80",
                "a.js:30-60",
                "a.js:40-50",
                "a.js:72-75",
            ],
            &[],
        );
        assert!(l7.ran.is_subset(&l9.ran));
        credit(&mut new, &baseline, &l9, 9);
        assert_eq!(
            new,
            BTreeMap::from([("a.js:30-60".to_string(), 7), ("a.js:40-50".to_string(), 7)])
        );
        // the union of the baseline equals its last snapshot when snapshots are cumulative
        let union: BTreeSet<&String> = baseline.iter().flat_map(|b| b.ran.iter()).collect();
        assert_eq!(union, baseline[1].ran.iter().collect());
    }

    #[test]
    fn fixed_sets_are_plain_membership_and_v8_needs_uncovered() {
        let go = Snapshot::parse(
            &json!({"tool": "go -cover statement blocks", "ranges": ["p/f.go:1.1,2.2"]}),
        )
        .unwrap();
        assert!(go.ran("p/f.go:1.1,2.2"));
        assert!(!go.ran("p/f.go:3.1,4.2"));
        assert!(Snapshot::parse(&json!({"tool": "v8 x", "ranges": []})).is_err());
    }
}
