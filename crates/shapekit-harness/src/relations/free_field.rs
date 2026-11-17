//! Free field inertness: a field core does not validate can take any value, and core then
//! treats the transaction the same way whatever the value is. An indexer must too. The
//! witnesses are attempts that differ in one field, each with core's outcome; this relation
//! groups them by field and outcome and compares, per attempt, how the indexer did against
//! core's own meta, which is the harness verdict of the attempt's entry.
//!
//! The comparison is per attempt against core, not between the attempts' stored rows: the
//! attempts of one field run on their own funded accounts, so their rows differ in keys,
//! sequence numbers and ledgers as well as in the field, and a row-to-row comparison would
//! need all of that normalised first. Two attempts core treated alike must get the same
//! verdict, the same reason and the same kinds of problems; when they do not, the value of a
//! field core ignores changed what the indexer stored.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::run::EntryRow;

/// One attempt: a witness entry and core's outcome for it, with the carrier transaction the
/// field sat in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    pub label: String,
    /// The entries that carry the attempt; the harness judges those with an oracle file.
    pub entries: Vec<String>,
    /// Core's outcome, compared as text: submission, application and result code.
    pub outcome: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    pub field: String,
    #[serde(default)]
    pub attempt: Vec<Attempt>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Groups {
    #[serde(default)]
    pub group: Vec<Group>,
}

impl Groups {
    /// Hand-written groups over corpus entries, for fields whose attempts are already in the
    /// corpus as entries of one chain.
    pub fn load(path: &Path) -> Result<Groups> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    /// The fields `validation.json` records as `not-validated`, each attempt with its
    /// witness entries (`candidate:` or `corpus:` ids) and core's outcome.
    pub fn from_validation(path: &Path) -> Result<Groups> {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let v: Value = serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing {}", path.display()))?;
        let fields = v
            .get("fields")
            .and_then(Value::as_array)
            .with_context(|| format!("{} has no fields", path.display()))?;
        let text = |a: &Value, k: &str| a.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
        let mut out = Groups::default();
        for f in fields {
            if f.get("verdict").and_then(Value::as_str) != Some("not-validated") {
                continue;
            }
            let mut g = Group {
                field: text(f, "path"),
                attempt: Vec::new(),
            };
            for a in f
                .get("attempts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let entries = a
                    .get("witnesses")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(|w| w.split_once(':').map_or(w, |(_, id)| id).to_owned())
                    .collect();
                g.attempt.push(Attempt {
                    label: text(a, "variant"),
                    entries,
                    // attempts in different carrier transactions are different transactions,
                    // so the carrier is part of what core treated alike
                    outcome: format!(
                        "carrier {}: {} {} {}",
                        text(a, "carrier"),
                        text(a, "submission"),
                        text(a, "applied"),
                        text(a, "result_code")
                    ),
                });
            }
            out.group.push(g);
        }
        Ok(out)
    }
}

/// What the harness made of one attempt on one network.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Judged {
    pub verdict: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// `kind fact` of every problem, without repeats.
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Difference {
    pub field: String,
    pub network: String,
    pub outcome: String,
    /// Attempt label to what the harness made of it; at least two different values.
    pub attempts: BTreeMap<String, Judged>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FreeFieldReport {
    pub groups: u64,
    /// Attempts the run judged, on any network.
    pub judged: u64,
    /// Attempts none of whose entries the run judged.
    pub not_run: u64,
    /// Sets of two or more attempts core treated alike, compared per network.
    pub compared: u64,
    pub differences: Vec<Difference>,
}

/// Compares the attempts of every group as the run's `entries` judged them.
pub fn check(entries: &[EntryRow], groups: &Groups) -> FreeFieldReport {
    let mut by_entry: BTreeMap<(&str, &str), Judged> = BTreeMap::new();
    for e in entries {
        let problems: BTreeSet<String> = e
            .result
            .problems
            .iter()
            .map(|p| {
                format!(
                    "{} {}",
                    serde_json::to_value(p.kind)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_owned))
                        .unwrap_or_default(),
                    p.fact
                )
            })
            .collect();
        by_entry.insert(
            (e.entry.as_str(), e.network.as_str()),
            Judged {
                verdict: e.result.verdict.name().to_owned(),
                reason: e.result.reason.map(|r| r.name().to_owned()),
                problems: problems.into_iter().collect(),
            },
        );
    }
    let networks: BTreeSet<&str> = entries.iter().map(|e| e.network.as_str()).collect();
    let mut r = FreeFieldReport {
        groups: groups.group.len() as u64,
        ..FreeFieldReport::default()
    };
    for g in &groups.group {
        for a in &g.attempt {
            let seen = networks.iter().any(|n| {
                a.entries
                    .iter()
                    .any(|e| by_entry.contains_key(&(e.as_str(), *n)))
            });
            if seen {
                r.judged += 1;
            } else {
                r.not_run += 1;
            }
        }
        for n in &networks {
            let mut by_outcome: BTreeMap<&str, BTreeMap<String, Judged>> = BTreeMap::new();
            for a in &g.attempt {
                // an attempt's verdict is the most severe of its entries' (envelope and ledger)
                let judged = a
                    .entries
                    .iter()
                    .filter_map(|e| by_entry.get(&(e.as_str(), *n)))
                    .max_by_key(|j| severity(&j.verdict));
                if let Some(j) = judged {
                    by_outcome
                        .entry(a.outcome.as_str())
                        .or_default()
                        .insert(a.label.clone(), j.clone());
                }
            }
            for (outcome, attempts) in by_outcome {
                if attempts.len() < 2 {
                    continue;
                }
                r.compared += 1;
                let distinct: BTreeSet<&Judged> = attempts.values().collect();
                if distinct.len() > 1 {
                    r.differences.push(Difference {
                        field: g.field.clone(),
                        network: (*n).to_owned(),
                        outcome: outcome.to_owned(),
                        attempts,
                    });
                }
            }
        }
    }
    r
}

fn severity(verdict: &str) -> u8 {
    match verdict {
        "pass" => 0,
        "out-of-scope" => 1,
        "not-evaluated" => 2,
        "dropped" => 3,
        "mishandled" => 4,
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn judged(v: &str) -> Judged {
        Judged {
            verdict: v.into(),
            reason: None,
            problems: Vec::new(),
        }
    }

    #[test]
    fn validation_groups_take_only_not_validated_fields() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("validation.json");
        std::fs::write(
            &p,
            r#"{"fields":[
              {"path":"A.x","verdict":"validated","attempts":[]},
              {"path":"B.y","verdict":"not-validated","attempts":[
                {"variant":"base","carrier":"pay","submission":"pending","applied":"succeeded","result_code":"tx_success","witnesses":["candidate:b-0","candidate:b-0-ledger"]},
                {"variant":"zero","carrier":"pay","submission":"pending","applied":"succeeded","result_code":"tx_success","witnesses":["candidate:b-1"]}]}]}"#,
        )
        .unwrap();
        let g = Groups::from_validation(&p).unwrap();
        assert_eq!(g.group.len(), 1);
        assert_eq!(g.group[0].attempt[0].entries, ["b-0", "b-0-ledger"]);
        assert_eq!(
            g.group[0].attempt[1].outcome,
            "carrier pay: pending succeeded tx_success"
        );
    }

    fn row(entry: &str, v: crate::compare::Verdict) -> EntryRow {
        EntryRow {
            entry: entry.into(),
            family: "f".into(),
            chain: "c".into(),
            network: "testnet".into(),
            ledger: 1,
            result: crate::compare::EntryVerdict::simple(v, None, String::new()),
        }
    }

    #[test]
    fn alike_attempts_with_different_verdicts_are_a_difference() {
        use crate::compare::Verdict;
        let att = |label: &str, entry: &str, outcome: &str| Attempt {
            label: label.into(),
            entries: vec![entry.into()],
            outcome: outcome.into(),
        };
        let groups = Groups {
            group: vec![Group {
                field: "Memo.id".into(),
                attempt: vec![
                    att("a", "e-a", "applied succeeded"),
                    att("b", "e-b", "applied succeeded"),
                    att("c", "e-c", "applied failed"),
                    att("d", "e-d", "applied succeeded"),
                ],
            }],
        };
        let same = [
            row("e-a", Verdict::Pass),
            row("e-b", Verdict::Pass),
            row("e-c", Verdict::Mishandled),
        ];
        let r = check(&same, &groups);
        assert_eq!((r.judged, r.not_run, r.compared), (3, 1, 1));
        assert!(r.differences.is_empty(), "{r:?}");
        let differ = [row("e-a", Verdict::Pass), row("e-b", Verdict::Dropped)];
        let r = check(&differ, &groups);
        assert_eq!(r.differences.len(), 1);
        assert_eq!(r.differences[0].attempts.len(), 2);
    }

    #[test]
    fn severity_orders_verdicts() {
        let mut v = [judged("pass"), judged("mishandled"), judged("dropped")];
        v.sort_by_key(|j| severity(&j.verdict));
        assert_eq!(v.last().unwrap().verdict, "mishandled");
    }
}
