//! Gate sides witnessed by chains on the coverage twin.
//!
//! clang's coverage mapping keeps a branch region for every leaf condition,
//! with how often it was true and how often false. A protocol gate used as a
//! condition (`if (protocolVersionIsBefore(v, V_19))`, or one operand of
//! `&&` / `||`) therefore has its own true and false counts, at the gate
//! call's line and column. A chain whose run gives the true count above zero
//! witnesses the gate's true side, likewise for false. A negated gate
//! (`!protocolVersion...`) has its branch region on the `!`, with the counts
//! of the negation, so its sides swap.
//!
//! A gate side no chain takes stays open, with the reason the data gives:
//! no branch region (the result is stored in a variable rather than tested,
//! so coverage cannot tell its value), the condition never evaluated by any
//! chain, or evaluated but never with this outcome. Each reason names the
//! protocols the side runs at and the protocols the chains ran.
//!
//! A second source joins in: the explorer's gate runs (`shapekit explore
//! gates`), which rerun recipes on the twin at other protocols and count how
//! often each gate call ran during the recipe's own steps. Such a run says
//! the call ran at a protocol where the comparison takes the side; it does
//! not see the outcome itself. Its witnesses are written
//! `recipe:<IR path>@p<protocol>` and added to the chains' (the union; a side
//! met before is never set back to open). When a rerun at a recipe's own
//! protocol claims a side that the same recipe's committed chain evaluated
//! and never took, the two disagree: that is reported as a conflict, not
//! resolved.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use shapekit_contracts::gates::{Branch, Gate, GateList, GateSide};
use shapekit_contracts::obligations::ObligationStatus;

use crate::coverage::{Branches, Span};

/// What one chain's run says about a gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Outcome {
    taken_true: bool,
    taken_false: bool,
}

/// The branch region of a gate call, with whether it is negated.
fn branch_of<'a>(
    branches: &'a Branches,
    g: &Gate,
    line_text: Option<&str>,
) -> Option<(&'a (u64, u64), bool)> {
    let file = branches.get(&g.file)?;
    let at = |col: u32| {
        file.iter()
            .find(|(s, _): &(&Span, _)| s.line_start == g.line && s.column_start == col)
            .map(|(_, c)| c)
    };
    if let Some(c) = at(g.column) {
        return Some((c, false));
    }
    // `!protocolVersion...`: the region starts on the `!`
    let col = g.column.checked_sub(1)?;
    let bang = line_text
        .and_then(|l| l.chars().nth((col - 1) as usize))
        .is_some_and(|c| c == '!');
    if bang && let Some(c) = at(col) {
        return Some((c, true));
    }
    None
}

fn protocol_list(sides: &[(u32, Option<u32>)]) -> String {
    sides
        .iter()
        .map(|(lo, hi)| match hi {
            Some(h) if h == lo => format!("{lo}"),
            Some(h) => format!("{lo} to {h}"),
            None => format!("{lo} and later"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The explorer's gate runs, `explore-gate-sides.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct ExploreSides {
    pub schema_version: u32,
    pub twin_image: String,
    /// The protocols the reruns ran at.
    pub protocols: Vec<u32>,
    pub recipes: Vec<ExploreRecipe>,
    pub sides: Vec<ExploreSide>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExploreRecipe {
    pub name: String,
    /// The IR's id; its committed chain is `<id>.p<protocol>`.
    pub id: String,
    pub protocol: u32,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExploreSide {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub branch: Branch,
    pub runs: Vec<ExploreRun>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExploreRun {
    pub run: String,
    pub kind: String,
    #[serde(default)]
    pub recipe: Option<String>,
    pub protocol: u32,
    pub count: u64,
    pub reference_count: u64,
}

/// Two explore outputs over the same twin as one: recipes by name (a name
/// must keep its path), sides by position with their runs joined, protocols
/// joined.
pub fn merge_explore(mut a: ExploreSides, b: ExploreSides) -> anyhow::Result<ExploreSides> {
    for r in b.recipes {
        match a.recipes.iter().find(|x| x.name == r.name) {
            Some(x) if x.path != r.path => {
                anyhow::bail!(
                    "recipe {} is {} in one input and {} in another",
                    r.name,
                    x.path,
                    r.path
                )
            }
            Some(_) => {}
            None => a.recipes.push(r),
        }
    }
    for s in b.sides {
        match a.sides.iter_mut().find(|x| {
            x.file == s.file && x.line == s.line && x.column == s.column && x.branch == s.branch
        }) {
            Some(x) => {
                for run in s.runs {
                    if !x.runs.iter().any(|r| r.run == run.run) {
                        x.runs.push(run);
                    }
                }
            }
            None => a.sides.push(s),
        }
    }
    a.protocols.extend(b.protocols);
    a.protocols.sort_unstable();
    a.protocols.dedup();
    Ok(a)
}

/// A side the two sources disagree on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub branch: Branch,
    /// The rerun claiming the side.
    pub run: String,
    /// The committed chain of the same recipe and protocol, which evaluated
    /// the condition and never took the side.
    pub chain: String,
}

/// Consecutive protocols folded into inclusive ranges.
fn ranges_of(ps: &[u32]) -> Vec<(u32, Option<u32>)> {
    let mut v: Vec<u32> = ps.to_vec();
    v.sort_unstable();
    v.dedup();
    let mut out: Vec<(u32, Option<u32>)> = Vec::new();
    for p in v {
        match out.last_mut() {
            Some((_, Some(hi))) if *hi + 1 == p => *hi = p,
            _ => out.push((p, Some(p))),
        }
    }
    out
}

/// Per gate side, the chains that take it.
pub struct Witnesses {
    pub list: GateList,
    pub met: usize,
    pub open: usize,
    /// Gates with no branch region in any run.
    pub no_branch: usize,
    /// Sides met only by explorer reruns.
    pub met_by_reruns_only: usize,
    pub conflicts: Vec<Conflict>,
}

/// Sets every side of `list` from the chains' branch counts. `chains` pairs
/// a `chain:<id>` reference with its protocol and branch counts; `line` gives
/// a source line of core (for the `!` check).
pub fn witness(
    list: &GateList,
    chains: &[(String, u32, Branches)],
    explore: Option<&ExploreSides>,
    line: impl Fn(&str, u32) -> Option<String>,
) -> Witnesses {
    let recipes: BTreeMap<&str, &ExploreRecipe> = explore
        .map(|e| e.recipes.iter().map(|r| (r.name.as_str(), r)).collect())
        .unwrap_or_default();
    let reruns: BTreeMap<(&str, u32, u32, Branch), &ExploreSide> = explore
        .map(|e| {
            e.sides
                .iter()
                .map(|s| ((s.file.as_str(), s.line, s.column, s.branch), s))
                .collect()
        })
        .unwrap_or_default();
    let mut conflicts = Vec::new();
    let mut met_by_reruns_only = 0;
    let protocols: BTreeSet<u32> = chains.iter().map(|(_, p, _)| *p).collect();
    let ran = protocols
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let mut out = list.clone();
    let (mut met, mut open, mut no_branch) = (0, 0, 0);
    for g in &mut out.gates {
        let text = line(&g.file, g.line);
        let mut outcomes: BTreeMap<&str, Outcome> = BTreeMap::new();
        let mut any_region = false;
        for (chain, _, branches) in chains {
            if let Some(((t, f), negated)) = branch_of(branches, g, text.as_deref()) {
                any_region = true;
                let (t, f) = if negated { (*f, *t) } else { (*t, *f) };
                outcomes.insert(
                    chain.as_str(),
                    Outcome {
                        taken_true: t > 0,
                        taken_false: f > 0,
                    },
                );
            }
        }
        if !any_region {
            no_branch += 1;
        }
        let evaluated = outcomes.values().any(|o| o.taken_true || o.taken_false);
        let mut sides = Vec::new();
        let before: BTreeMap<Branch, &GateSide> = list
            .gates
            .iter()
            .find(|x| x.file == g.file && x.line == g.line && x.column == g.column)
            .map(|x| x.sides.iter().map(|s| (s.branch, s)).collect())
            .unwrap_or_default();
        for branch in [Branch::True, Branch::False] {
            let chain_witnesses: Vec<String> = outcomes
                .iter()
                .filter(|(_, o)| match branch {
                    Branch::True => o.taken_true,
                    Branch::False => o.taken_false,
                })
                .map(|(c, _)| c.to_string())
                .collect();
            let mut rerun_witnesses = Vec::new();
            if let Some(side) = reruns.get(&(g.file.as_str(), g.line, g.column, branch)) {
                for run in side.runs.iter().filter(|r| r.count > r.reference_count) {
                    let recipe = run.recipe.as_deref().and_then(|n| recipes.get(n));
                    // empty-ledger runs are setup every chain shares, not a witness
                    if let ("recipe", Some(r)) = (run.kind.as_str(), recipe) {
                        rerun_witnesses.push(format!("recipe:{}@p{}", r.path, run.protocol));
                        // the rerun at the recipe's own protocol is the
                        // committed chain's run; compare outcomes
                        if run.protocol == r.protocol {
                            let chain = format!("chain:{}.p{}", r.id, r.protocol);
                            if let Some(o) = outcomes.get(chain.as_str()) {
                                let took = match branch {
                                    Branch::True => o.taken_true,
                                    Branch::False => o.taken_false,
                                };
                                if (o.taken_true || o.taken_false) && !took {
                                    conflicts.push(Conflict {
                                        file: g.file.clone(),
                                        line: g.line,
                                        column: g.column,
                                        branch,
                                        run: run.run.clone(),
                                        chain,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            let mut witnesses: BTreeSet<String> = chain_witnesses.iter().cloned().collect();
            witnesses.extend(rerun_witnesses.iter().cloned());
            // a side met before stays met, with its witnesses kept
            if let Some(prev) = before.get(&branch)
                && prev.status == ObligationStatus::Met
            {
                witnesses.extend(prev.witnesses.iter().cloned());
            }
            let witnesses: Vec<String> = witnesses.into_iter().collect();
            let runs_at = g.protocols(branch);
            let on_chain_protocol = protocols.iter().any(|p| {
                runs_at
                    .iter()
                    .any(|(lo, hi)| p >= lo && hi.is_none_or(|h| *p <= h))
            });
            if witnesses.is_empty() {
                open += 1;
                let needs = if runs_at.is_empty() {
                    "no protocol".to_owned()
                } else {
                    format!("protocol {}", protocol_list(&runs_at))
                };
                let evidence = if !any_region {
                    format!(
                        "the gate's result is stored or passed on, not tested as a condition, so coverage records no side; this side runs at {needs}"
                    )
                } else if !on_chain_protocol {
                    format!("needs a chain at {needs}; the chains run protocols {ran}")
                } else if !evaluated {
                    format!(
                        "no chain evaluates this condition; this side runs at {needs} (chains run {ran}) and needs a transaction that reaches {}",
                        g.function
                    )
                } else {
                    format!(
                        "chains evaluate the condition but never with this outcome; this side runs at {needs} (chains run {ran}) and needs a setup in {} that makes it {}",
                        g.function,
                        branch.as_str()
                    )
                };
                let evidence = match explore {
                    Some(e) if !e.protocols.is_empty() => format!(
                        "{evidence}; no recipe rerun at protocols {} ran the call at a protocol taking this side",
                        protocol_list(&ranges_of(&e.protocols))
                    ),
                    _ => evidence,
                };
                sides.push(GateSide {
                    branch,
                    status: ObligationStatus::Open,
                    witnesses,
                    evidence: Some(evidence),
                });
            } else {
                met += 1;
                if chain_witnesses.is_empty() && !rerun_witnesses.is_empty() {
                    met_by_reruns_only += 1;
                }
                let mut kinds = Vec::new();
                if !chain_witnesses.is_empty() {
                    kinds.push(format!(
                        "{} chain(s) take this side by the twin's branch counts",
                        chain_witnesses.len()
                    ));
                }
                if !rerun_witnesses.is_empty() {
                    kinds.push(format!(
                        "{} recipe rerun(s) ran the call during their own steps at a protocol that takes this side (execution count, outcome inferred from the protocol{})",
                        rerun_witnesses.len(),
                        if any_region { "" } else { "; the only evidence, as the gate has no branch region" }
                    ));
                }
                if kinds.is_empty() {
                    kinds.push("met by an earlier run, kept".into());
                }
                sides.push(GateSide {
                    branch,
                    status: ObligationStatus::Met,
                    witnesses,
                    evidence: Some(kinds.join("; ")),
                });
            }
        }
        g.sides = sides;
    }
    Witnesses {
        list: out,
        met,
        open,
        no_branch,
        met_by_reruns_only,
        conflicts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shapekit_contracts::CoreRef;
    use shapekit_contracts::gates::Comparison;

    fn gate(column: u32) -> Gate {
        Gate {
            file: "src/transactions/SetOptionsOpFrame.cpp".into(),
            line: 310,
            column,
            function: "SetOptionsOpFrame::doCheckValid".into(),
            comparison: Comparison::IsBefore,
            version: 19,
            sides: vec![],
        }
    }

    fn branches(col: u32, t: u64, f: u64) -> Branches {
        let mut b = Branches::new();
        b.entry("src/transactions/SetOptionsOpFrame.cpp".into())
            .or_default()
            .insert(
                Span {
                    line_start: 310,
                    column_start: col,
                    line_end: 310,
                    column_end: 75,
                },
                (t, f),
            );
        b
    }

    fn list(g: Gate) -> GateList {
        GateList {
            schema_version: 1,
            core: CoreRef {
                version: "v30.0.0".into(),
                commit: "0e49751aa9ddf892302cca02cea7f3f5d97df9f1".into(),
                image: None,
            },
            gates: vec![g],
        }
    }

    #[test]
    fn a_taken_side_is_met_and_the_other_says_what_it_needs() {
        let chains = vec![(
            "chain:cap40.signed-payload-signer.p30".to_owned(),
            30,
            branches(14, 0, 32),
        )];
        let w = witness(&list(gate(14)), &chains, None, |_, _| None);
        let s = &w.list.gates[0].sides;
        assert_eq!(s[0].status, ObligationStatus::Open);
        assert!(
            s[0].evidence
                .as_deref()
                .unwrap()
                .contains("needs a chain at protocol 0 to 18")
        );
        assert_eq!(s[1].status, ObligationStatus::Met);
        assert_eq!(
            s[1].witnesses,
            vec!["chain:cap40.signed-payload-signer.p30"]
        );
        assert_eq!((w.met, w.open), (1, 1));
        assert!(shapekit_contracts::Contract::problems(&w.list).is_empty());
    }

    #[test]
    fn a_negated_gate_swaps_its_counts() {
        let chains = vec![("chain:x.p30".to_owned(), 30, branches(13, 5, 0))];
        let w = witness(&list(gate(14)), &chains, None, |_, _| {
            Some("        if (!protocolVersionIsBefore(v, V_19))".into())
        });
        let s = &w.list.gates[0].sides;
        // the negation was true 5 times, so the gate itself was false
        assert_eq!(s[1].status, ObligationStatus::Met);
        assert_eq!(s[0].status, ObligationStatus::Open);
    }

    #[test]
    fn reruns_join_the_chains_and_a_disagreement_is_a_conflict() {
        let explore: ExploreSides = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "twin_image": "sha256:x",
            "protocols": [18, 30],
            "recipes": [{"name": "cap40.r", "id": "cap40.signed-payload-signer", "protocol": 30, "path": "corpus/recipes/cap40/r.json"}],
            "sides": [
                {"file": "src/transactions/SetOptionsOpFrame.cpp", "line": 310, "column": 14, "branch": "true",
                 "runs": [{"run": "cap40.r@p18", "kind": "recipe", "recipe": "cap40.r", "protocol": 18, "count": 3, "reference_count": 0},
                          {"run": "cap40.r@p30", "kind": "recipe", "recipe": "cap40.r", "protocol": 30, "count": 3, "reference_count": 0}]}
            ]
        }))
        .unwrap();
        let chains = vec![(
            "chain:cap40.signed-payload-signer.p30".to_owned(),
            30,
            branches(14, 0, 32),
        )];
        let w = witness(&list(gate(14)), &chains, Some(&explore), |_, _| None);
        let s = &w.list.gates[0].sides;
        assert_eq!(s[0].status, ObligationStatus::Met);
        assert!(
            s[0].witnesses
                .contains(&"recipe:corpus/recipes/cap40/r.json@p18".to_owned())
        );
        assert_eq!(w.met_by_reruns_only, 1);
        // the p30 rerun claims the true side the p30 chain never took
        assert_eq!(w.conflicts.len(), 1);
        assert_eq!(
            w.conflicts[0].chain,
            "chain:cap40.signed-payload-signer.p30"
        );
        assert!(shapekit_contracts::Contract::problems(&w.list).is_empty());
    }

    #[test]
    fn no_branch_region_is_said_so() {
        let chains = vec![("chain:x.p30".to_owned(), 30, Branches::new())];
        let w = witness(&list(gate(14)), &chains, None, |_, _| None);
        assert_eq!(w.no_branch, 1);
        assert!(
            w.list.gates[0].sides[0]
                .evidence
                .as_deref()
                .unwrap()
                .contains("not tested as a condition")
        );
    }
}
