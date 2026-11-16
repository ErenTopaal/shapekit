//! Witnesses for both sides of core's protocol gates (F6.5). Every recipe is run on the
//! coverage twin at each protocol, beside two reference chains per protocol: a node that only
//! closed ledger 2 (the upgrade from protocol 0) and the same node after the empty padding
//! ledgers every recipe chain starts with. A gate call ran during a recipe's own steps at
//! protocol p when its count in the recipe's run exceeds its count in the padded reference
//! at p; it ran in the padding ledgers when the padded reference exceeds the genesis one. A
//! side is met by such a run at a protocol where the comparison takes that side.
//!
//! Per gate the runs at N-1 and N of the same recipe are also compared on what core answered
//! each transaction: the output difference the gate makes, when a recipe shows one.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;
use shapekit_contracts::RecipeIr;
use shapekit_contracts::gates::{Branch, Gate, GateList};
use shapekit_contracts::obligations::ObligationStatus;
use shapekit_contracts::recipe_ir::Step;
use shapekit_core_driver::chain::ChainDriver;
use shapekit_gen::recipe::ir_run::IrRecipe;
use shapekit_gen::recipe::{Network, node_config};
use shapekit_twin::coverage::{Counts, merge_and_export};

use crate::coverage::{count_at, read_export_all};
use crate::episode::{self, Env, Spec};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunKind {
    /// Only ledger 2, which upgrades from protocol 0.
    Genesis,
    /// Through the padding every recipe chain starts with.
    Padded,
    Recipe,
}

#[derive(Debug, Clone)]
pub struct GateRun {
    pub kind: RunKind,
    /// The recipe's name, for a recipe run.
    pub recipe: Option<String>,
    pub protocol: u32,
    pub counts: Counts,
    /// What core answered each transaction, in order.
    pub answers: Vec<String>,
}

impl GateRun {
    pub fn name(&self) -> String {
        match (&self.kind, &self.recipe) {
            (RunKind::Recipe, Some(r)) => format!("{r}@p{}", self.protocol),
            (RunKind::Genesis, _) => format!("genesis@p{}", self.protocol),
            _ => format!("empty-ledgers@p{}", self.protocol),
        }
    }
}

/// A recipe at another protocol: the chain is upgraded to `protocol` instead, and its own
/// protocol upgrades are left out so every ledger after the padding runs at `protocol`.
pub fn at_protocol(ir: &RecipeIr, protocol: u32) -> RecipeIr {
    let mut out = ir.clone();
    out.protocol = protocol;
    if out.node.soroban_settings && protocol != 30 {
        // the captured settings only validate at 30; the limit override stands in
        out.node.soroban_settings = false;
        out.node.soroban_limits = true;
    }
    out.steps = ir
        .steps
        .iter()
        .map(|s| match s {
            Step::Upgrade {
                protocol_version: Some(_),
                base_fee: None,
                base_reserve: None,
                max_tx_set_size: None,
                flags: None,
                config,
                ..
            } if config.is_empty() => Step::Close {
                label: None,
                order: Vec::new(),
            },
            Step::Upgrade { .. } => {
                let mut s = s.clone();
                if let Step::Upgrade {
                    protocol_version, ..
                } = &mut s
                {
                    *protocol_version = None;
                }
                s
            }
            other => other.clone(),
        })
        .collect();
    out
}

/// The lowest protocol `ir` can run at: the highest interface protocol of the code its
/// upload steps carry (a host refuses code declaring a newer protocol), 1 without uploads.
pub fn lowest_protocol(ir: &RecipeIr, root: &Path) -> Result<u32> {
    let mut lowest = 1;
    for s in &ir.steps {
        if let Step::Upload { wasm, .. } = s {
            let bytes =
                std::fs::read(root.join(wasm)).with_context(|| format!("reading {wasm}"))?;
            let p = shapekit_core_driver::contracts::interface_protocol(&bytes)
                .with_context(|| format!("{wasm} declares no interface protocol"))?;
            lowest = lowest.max(p);
        }
        if let Step::Invoke { .. }
        | Step::HostFunction { .. }
        | Step::ExtendTtl { .. }
        | Step::Restore { .. } = s
        {
            // Soroban arrived with protocol 20, and the first host core links is 21's
            lowest = lowest.max(21);
        }
    }
    Ok(lowest)
}

/// Records a node that closes only ledger 2 on the twin and reads its counts.
pub fn genesis(env: &Env<'_>, protocol: u32) -> Result<Counts> {
    let work = tempfile::Builder::new()
        .prefix("explore-genesis-")
        .tempdir_in(env.work)?;
    let profiles = work.path().join("profiles");
    std::fs::create_dir_all(&profiles)?;
    let mut image = env.image.clone();
    image.profile_dir = Some(profiles.canonicalize()?);
    let node = work.path().join("node");
    std::fs::create_dir_all(&node)?;
    let shell = IrRecipe::new(
        RecipeIr {
            schema_version: 1,
            id: "explore.genesis".into(),
            family: episode::FAMILY.into(),
            protocol,
            node: Default::default(),
            below_protocol_valid: false,
            steps: Vec::new(),
        },
        env.root,
    );
    let driver = ChainDriver::start(node_config(&shell, Network::Testnet, &image), &node)?;
    driver.stop()?;
    read_export_all(&merge_and_export(env.image, &profiles)?)
}

/// One run of `ir` (or the padded empty chain when `ir` is `None`) at `protocol` on the twin.
pub fn record(env: &Env<'_>, name: Option<(&str, &RecipeIr)>, protocol: u32) -> Result<GateRun> {
    let (kind, recipe, ir) = match name {
        Some((n, ir)) => (
            RunKind::Recipe,
            Some(n.to_owned()),
            at_protocol(ir, protocol),
        ),
        None => (
            RunKind::Padded,
            None,
            RecipeIr {
                schema_version: 1,
                id: "explore.padded".into(),
                family: episode::FAMILY.into(),
                protocol,
                node: Default::default(),
                below_protocol_valid: false,
                steps: Vec::new(),
            },
        ),
    };
    let spec = Spec {
        id: ir.id.clone(),
        protocol,
        max_protocol: protocol,
        node: ir.node.clone(),
        prefix: ir.steps.clone(),
        actions: 0,
        contracts: false,
        seed: 0,
        setup: false,
    };
    let mut menu = crate::action::OpMenu {
        arms: Vec::new(),
        novelty: Vec::new(),
    };
    let ep = episode::run(env, &spec, &mut menu)?;
    Ok(GateRun {
        kind,
        recipe,
        protocol,
        counts: ep.counts.context("the twin wrote no counts")?,
        answers: ep.answers,
    })
}

/// Whether a comparison takes `branch` at `protocol`.
pub fn takes(gate: &Gate, branch: Branch, protocol: u32) -> bool {
    gate.protocols(branch)
        .iter()
        .any(|(lo, hi)| protocol >= *lo && hi.is_none_or(|h| protocol <= h))
}

/// How often a gate call ran during what is particular to `run` (see the module docs).
fn own_count(gate: &Gate, run: &GateRun, refs: &BTreeMap<(RunKind, u32), &GateRun>) -> u64 {
    let at = |r: &GateRun| count_at(&r.counts, &gate.file, gate.line, gate.column).unwrap_or(0);
    let base = match run.kind {
        RunKind::Recipe => refs.get(&(RunKind::Padded, run.protocol)),
        RunKind::Padded => refs.get(&(RunKind::Genesis, run.protocol)),
        RunKind::Genesis => None,
    };
    at(run).saturating_sub(base.map_or(0, |b| at(b)))
}

#[derive(Debug, Clone, Serialize)]
pub struct GateDifference {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub version: u32,
    /// Recipes that ran the call at both N-1 and N and whose answers differ between them.
    pub differing: Vec<String>,
    /// Recipes that ran it at both and answered the same.
    pub same: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GateSummary {
    pub gates: usize,
    pub sides: usize,
    pub met: usize,
    pub unmeetable: usize,
    pub open: usize,
    pub open_unmeasured: usize,
    pub gates_with_difference: usize,
}

/// Fills every side of every gate from the runs. Sides no run meets stay open, with what is
/// known about them.
pub fn statuses(list: &mut GateList, runs: &[GateRun]) -> (GateSummary, Vec<GateDifference>) {
    let refs: BTreeMap<(RunKind, u32), &GateRun> = runs
        .iter()
        .filter(|r| r.kind != RunKind::Recipe)
        .map(|r| ((r.kind, r.protocol), r))
        .collect();
    let protocols: BTreeSet<u32> = runs.iter().map(|r| r.protocol).collect();
    let measured: BTreeSet<&String> = runs.iter().flat_map(|r| r.counts.keys()).collect();
    let mut s = GateSummary {
        gates: list.gates.len(),
        ..Default::default()
    };
    let mut diffs = Vec::new();
    for gate in &mut list.gates {
        let in_binary = measured.contains(&gate.file)
            && runs
                .first()
                .is_some_and(|r| count_at(&r.counts, &gate.file, gate.line, gate.column).is_some());
        for side in &mut gate.sides {
            s.sides += 1;
            let branch = side.branch;
            let g: &Gate = &Gate {
                sides: Vec::new(),
                ..gate_clone(
                    &gate.file,
                    gate.line,
                    gate.column,
                    &gate.function,
                    gate.comparison,
                    gate.version,
                )
            };
            if g.protocols(branch).is_empty() {
                side.status = ObligationStatus::Unmeetable;
                side.witnesses.clear();
                side.evidence = Some(format!(
                    "no protocol version takes this branch of the comparison with V_{}",
                    g.version
                ));
                s.unmeetable += 1;
                continue;
            }
            let mut witnesses: Vec<String> = runs
                .iter()
                .filter(|r| r.kind != RunKind::Genesis && takes(g, branch, r.protocol))
                .filter(|r| own_count(g, r, &refs) > 0)
                .map(GateRun::name)
                .collect();
            witnesses.sort();
            if witnesses.is_empty() {
                side.status = ObligationStatus::Open;
                side.witnesses.clear();
                let tried: Vec<String> = protocols
                    .iter()
                    .filter(|p| takes(g, branch, **p))
                    .map(|p| p.to_string())
                    .collect();
                side.evidence = Some(if !in_binary {
                    s.open_unmeasured += 1;
                    "the twin's coverage mapping has no region at this call, so no run can show it"
                        .to_owned()
                } else if tried.is_empty() {
                    "no run was recorded at a protocol that takes this branch".to_owned()
                } else {
                    format!(
                        "no recipe or empty-ledger run ran this call at protocols {}",
                        tried.join(", ")
                    )
                });
                s.open += 1;
            } else {
                side.status = ObligationStatus::Met;
                side.witnesses = witnesses;
                side.evidence = None;
                s.met += 1;
            }
        }
        // the output difference across the gate's version
        let n = gate.version;
        if n >= 1 {
            let g = gate_clone(
                &gate.file,
                gate.line,
                gate.column,
                &gate.function,
                gate.comparison,
                n,
            );
            let mut by_recipe: BTreeMap<&str, (Option<&GateRun>, Option<&GateRun>)> =
                BTreeMap::new();
            for r in runs.iter().filter(|r| r.kind == RunKind::Recipe) {
                let e = by_recipe
                    .entry(r.recipe.as_deref().unwrap_or(""))
                    .or_default();
                if r.protocol + 1 == n {
                    e.0 = Some(r);
                } else if r.protocol == n {
                    e.1 = Some(r);
                }
            }
            let mut d = GateDifference {
                file: gate.file.clone(),
                line: gate.line,
                column: gate.column,
                version: n,
                differing: Vec::new(),
                same: Vec::new(),
            };
            for (name, pair) in by_recipe {
                let (Some(a), Some(b)) = pair else { continue };
                if own_count(&g, a, &refs) == 0 || own_count(&g, b, &refs) == 0 {
                    continue;
                }
                if a.answers != b.answers {
                    d.differing.push(name.to_owned());
                } else {
                    d.same.push(name.to_owned());
                }
            }
            if !d.differing.is_empty() {
                s.gates_with_difference += 1;
            }
            if !d.differing.is_empty() || !d.same.is_empty() {
                diffs.push(d);
            }
        }
    }
    (s, diffs)
}

fn gate_clone(
    file: &str,
    line: u32,
    column: u32,
    function: &str,
    comparison: shapekit_contracts::gates::Comparison,
    version: u32,
) -> Gate {
    Gate {
        file: file.to_owned(),
        line,
        column,
        function: function.to_owned(),
        comparison,
        version,
        sides: Vec::new(),
    }
}

/// Writes counts compactly so a run can be read back without recording it again.
pub fn save(run: &GateRun, dir: &Path) -> Result<()> {
    let path = dir.join(format!("{}.json", run.name()));
    let v = serde_json::json!({
        "kind": run.kind,
        "recipe": run.recipe,
        "protocol": run.protocol,
        "answers": run.answers,
    });
    std::fs::write(&path, serde_json::to_vec(&v)?)
        .with_context(|| format!("writing {}", path.display()))?;
    shapekit_twin::coverage::write_counts(
        &dir.join(format!("{}.counts.json", run.name())),
        &run.counts,
    )
}

/// Reads back every run [`save`] wrote into `dir`.
pub fn load_all(dir: &Path) -> Result<Vec<GateRun>> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let p = e?.path();
        let name = p
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let Some(stem) = name.strip_suffix(".json") else {
            continue;
        };
        if stem.ends_with(".counts") {
            continue;
        }
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&p)?)?;
        let kind = match v["kind"].as_str() {
            Some("genesis") => RunKind::Genesis,
            Some("padded") => RunKind::Padded,
            _ => RunKind::Recipe,
        };
        out.push(GateRun {
            kind,
            recipe: v["recipe"].as_str().map(str::to_owned),
            protocol: v["protocol"].as_u64().unwrap_or(0) as u32,
            counts: shapekit_twin::coverage::read_counts(&dir.join(format!("{stem}.counts.json")))?,
            answers: serde_json::from_value(v["answers"].clone()).unwrap_or_default(),
        });
    }
    out.sort_by_key(|r| (r.kind, r.protocol, r.recipe.clone()));
    Ok(out)
}
