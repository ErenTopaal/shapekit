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
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, serde::Deserialize)]
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
    /// [`recipe_hash`] of the recipe, for a recipe run: a saved run is reused only for the
    /// recipe it was recorded from, not for another file that has its name.
    pub recipe_sha256: Option<String>,
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

    /// The name its files are saved under: the run's name, and for a recipe run the recipe's
    /// hash as well.
    pub fn key(&self) -> String {
        run_key(&self.name(), self.recipe_sha256.as_deref())
    }
}

/// [`GateRun::key`] of a run named `name` of a recipe hashing to `recipe_sha256`.
pub fn run_key(name: &str, recipe_sha256: Option<&str>) -> String {
    match recipe_sha256 {
        Some(h) => format!("{name}.{h}"),
        None => name.to_owned(),
    }
}

/// Hex sha256 of a recipe's IR as JSON: what a saved recipe run was recorded from. The IR
/// names its code by hash, so the hash covers the code as well.
pub fn recipe_hash(ir: &RecipeIr) -> Result<String> {
    use sha2::{Digest, Sha256};
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(ir)?)))
}

/// Where the runs recorded on `twin` under `out` are kept: a directory per twin image, so a
/// rebuilt twin never reuses another build's counts.
pub fn runs_dir(out: &Path, twin: &shapekit_core_driver::config::CoreImage) -> Result<PathBuf> {
    let id = twin
        .digest
        .strip_prefix("sha256:")
        .with_context(|| format!("twin image digest {} is not sha256:<hex>", twin.digest))?;
    Ok(out.join("runs").join(id))
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
            // an upgrade of the protocol alone becomes a plain close, keeping its label
            Step::Upgrade {
                label,
                protocol_version: Some(_),
                base_fee: None,
                base_reserve: None,
                max_tx_set_size: None,
                max_soroban_tx_set_size: None,
                flags: None,
                config,
            } if config.is_empty() => Step::Close {
                label: label.clone(),
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
    let work = shapekit_process::scratch::scratch_in(env.work, "explore-genesis-")?;
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
    let mut menu = crate::action::OpMenu::empty();
    let ep = episode::run(env, &spec, &mut menu)?;
    let recipe_sha256 = match name {
        Some((_, ir)) => Some(recipe_hash(ir)?),
        None => None,
    };
    Ok(GateRun {
        kind,
        recipe,
        recipe_sha256,
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

/// How often a gate call ran during what is particular to `run` (see the module docs), or
/// `None` when the run it is measured against is missing: without it the setup and padding
/// ledgers' calls would count as the run's own.
fn own_count(gate: &Gate, run: &GateRun, refs: &BTreeMap<(RunKind, u32), &GateRun>) -> Option<u64> {
    let at = |r: &GateRun| count_at(&r.counts, &gate.file, gate.line, gate.column).unwrap_or(0);
    let base = match run.kind {
        RunKind::Recipe => *refs.get(&(RunKind::Padded, run.protocol))?,
        RunKind::Padded => *refs.get(&(RunKind::Genesis, run.protocol))?,
        RunKind::Genesis => return Some(at(run)),
    };
    Some(at(run).saturating_sub(at(base)))
}

/// The genesis and padded runs of `runs` by kind and protocol, once every protocol a run in
/// `runs` is at, and every one of `protocols`, has both. A side counted against a missing
/// reference would take the reference's own calls as the recipe's, so nothing is counted
/// until they are all there.
pub fn references<'a>(
    runs: &'a [GateRun],
    protocols: &[u32],
) -> Result<BTreeMap<(RunKind, u32), &'a GateRun>> {
    let mut refs = BTreeMap::new();
    for r in runs.iter().filter(|r| r.kind != RunKind::Recipe) {
        if refs.insert((r.kind, r.protocol), r).is_some() {
            bail!("two {} runs", r.name());
        }
    }
    let wanted: BTreeSet<u32> = runs
        .iter()
        .map(|r| r.protocol)
        .chain(protocols.iter().copied())
        .collect();
    let missing: Vec<String> = wanted
        .iter()
        .flat_map(|p| {
            [
                (RunKind::Genesis, "genesis"),
                (RunKind::Padded, "empty-ledgers"),
            ]
            .into_iter()
            .filter(|(k, _)| !refs.contains_key(&(*k, *p)))
            .map(move |(_, n)| format!("{n}@p{p}"))
        })
        .collect();
    if !missing.is_empty() {
        bail!(
            "reference runs missing: {}; no side is counted without them",
            missing.join(", ")
        );
    }
    Ok(refs)
}

/// [`own_count`] for a run whose references [`references`] checked.
fn checked_own(
    gate: &Gate,
    run: &GateRun,
    refs: &BTreeMap<(RunKind, u32), &GateRun>,
) -> Result<u64> {
    own_count(gate, run, refs).with_context(|| format!("{} has no reference run", run.name()))
}

/// One run that executed a gate call where the comparison takes the side.
#[derive(Debug, Clone, Serialize)]
pub struct SideRun {
    /// `<recipe>@p<N>` or `empty-ledgers@p<N>`.
    pub run: String,
    pub kind: RunKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipe: Option<String>,
    pub protocol: u32,
    /// Executions of the call's innermost region in this run.
    pub count: u64,
    /// The same in the run it is measured against: the padded empty chain for a recipe, the
    /// genesis-only node for the padded chain.
    pub reference_count: u64,
}

/// The runs that witness one side of one gate.
#[derive(Debug, Clone, Serialize)]
pub struct SideWitnesses {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub branch: Branch,
    pub runs: Vec<SideRun>,
}

/// A recipe the runs name: `name` is what a run's `recipe` field holds.
#[derive(Debug, Clone, Serialize)]
pub struct RecipeRef {
    /// `<directory>.<file stem>`, unique among the recipes of one call.
    pub name: String,
    /// The IR's `id`; `<id>.p<protocol>` is its corpus chain id.
    pub id: String,
    /// The protocol the recipe itself names; its runs here are at every protocol.
    pub protocol: u32,
    /// The IR file, as given on the command line.
    pub path: String,
}

/// `explore-gate-sides.json`: the gate sides recipe runs on the twin witness. An input for
/// the gate list, which a side here never downgrades.
#[derive(Debug, Clone, Serialize)]
pub struct GateSides {
    pub schema_version: u32,
    pub core: shapekit_contracts::CoreRef,
    /// Twin image the runs were recorded on, as docker names it.
    pub twin_image: String,
    pub protocols: Vec<u32>,
    pub recipes: Vec<RecipeRef>,
    /// Every side with at least one witnessing run, in gate list order.
    pub sides: Vec<SideWitnesses>,
    /// Per gate version N, recipes whose answers differ between their runs at N-1 and N.
    pub differences: Vec<GateDifference>,
}

/// The witnessing runs of every gate side. Fails when a reference run is missing.
pub fn side_witnesses(list: &GateList, runs: &[GateRun]) -> Result<Vec<SideWitnesses>> {
    let refs = references(runs, &[])?;
    let mut out = Vec::new();
    for gate in &list.gates {
        for branch in [Branch::True, Branch::False] {
            let mut side_runs = Vec::new();
            for r in runs
                .iter()
                .filter(|r| r.kind != RunKind::Genesis && takes(gate, branch, r.protocol))
            {
                let own = checked_own(gate, r, &refs)?;
                if own > 0 {
                    let count =
                        count_at(&r.counts, &gate.file, gate.line, gate.column).unwrap_or(0);
                    side_runs.push(SideRun {
                        run: r.name(),
                        kind: r.kind,
                        recipe: r.recipe.clone(),
                        protocol: r.protocol,
                        count,
                        reference_count: count - own,
                    });
                }
            }
            side_runs.sort_by(|a, b| a.run.cmp(&b.run));
            if !side_runs.is_empty() {
                out.push(SideWitnesses {
                    file: gate.file.clone(),
                    line: gate.line,
                    column: gate.column,
                    branch,
                    runs: side_runs,
                });
            }
        }
    }
    Ok(out)
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
/// known about them. Fails when a reference run is missing.
pub fn statuses(
    list: &mut GateList,
    runs: &[GateRun],
) -> Result<(GateSummary, Vec<GateDifference>)> {
    let refs = references(runs, &[])?;
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
            let mut witnesses = Vec::new();
            for r in runs
                .iter()
                .filter(|r| r.kind != RunKind::Genesis && takes(g, branch, r.protocol))
            {
                if checked_own(g, r, &refs)? > 0 {
                    witnesses.push(r.name());
                }
            }
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
                if checked_own(&g, a, &refs)? == 0 || checked_own(&g, b, &refs)? == 0 {
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
    Ok((s, diffs))
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

/// Writes `bytes` to `path` through a temporary file in the same directory, renamed into
/// place: a run cut short leaves no partial file that a later call would read as recorded.
fn write_atomic(path: &Path, write: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
    let dir = path
        .parent()
        .with_context(|| format!("{} has no directory", path.display()))?;
    let tmp = tempfile::Builder::new()
        .prefix(".partial-")
        .tempfile_in(dir)
        .with_context(|| format!("creating a temporary file in {}", dir.display()))?;
    write(tmp.path())?;
    tmp.persist(path)
        .with_context(|| format!("renaming into {}", path.display()))?;
    Ok(())
}

/// Writes a run compactly so it can be read back without recording it again: the counts
/// first, then the run file, whose presence is what marks the run recorded.
pub fn save(run: &GateRun, dir: &Path) -> Result<()> {
    let key = run.key();
    write_atomic(&dir.join(format!("{key}.counts.json")), |p| {
        shapekit_twin::coverage::write_counts(p, &run.counts)
    })?;
    let v = serde_json::json!({
        "kind": run.kind,
        "recipe": run.recipe,
        "recipe_sha256": run.recipe_sha256,
        "protocol": run.protocol,
        "answers": run.answers,
    });
    let path = dir.join(format!("{key}.json"));
    write_atomic(&path, |p| {
        std::fs::write(p, serde_json::to_vec(&v)?)
            .with_context(|| format!("writing {}", p.display()))
    })
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedRun {
    kind: RunKind,
    recipe: Option<String>,
    recipe_sha256: Option<String>,
    protocol: u32,
    answers: Vec<String>,
}

/// Reads back every run [`save`] wrote into `dir`, each under the key it was saved as.
pub fn load_all(dir: &Path) -> Result<Vec<GateRun>> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let p = e
            .with_context(|| format!("reading {}", dir.display()))?
            .path();
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .with_context(|| format!("{} has no UTF-8 name", p.display()))?;
        if name.starts_with(".partial-") {
            // a save cut short before its rename; the run was never recorded
            continue;
        }
        let Some(stem) = name.strip_suffix(".json") else {
            bail!("{} is not a saved run", p.display());
        };
        if stem.ends_with(".counts") {
            continue;
        }
        let saved: SavedRun = serde_json::from_slice(
            &std::fs::read(&p).with_context(|| format!("reading {}", p.display()))?,
        )
        .with_context(|| format!("parsing {}", p.display()))?;
        let is_recipe = saved.kind == RunKind::Recipe;
        if is_recipe != saved.recipe.is_some() || is_recipe != saved.recipe_sha256.is_some() {
            bail!(
                "{}: a recipe run, and only one, names its recipe and the recipe's hash",
                p.display()
            );
        }
        let run = GateRun {
            kind: saved.kind,
            recipe: saved.recipe,
            recipe_sha256: saved.recipe_sha256,
            protocol: saved.protocol,
            counts: shapekit_twin::coverage::read_counts(&dir.join(format!("{stem}.counts.json")))?,
            answers: saved.answers,
        };
        if run.key() != stem {
            bail!("{} holds run {}", p.display(), run.key());
        }
        out.push(run);
    }
    out.sort_by_key(|r| (r.kind, r.protocol, r.recipe.clone()));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shapekit_contracts::gates::Comparison;

    fn upgrade(label: &str, protocol: Option<u32>, soroban_set: Option<u32>) -> Step {
        Step::Upgrade {
            label: Some(label.into()),
            protocol_version: protocol,
            base_fee: None,
            base_reserve: None,
            max_tx_set_size: None,
            max_soroban_tx_set_size: soroban_set,
            flags: None,
            config: Vec::new(),
        }
    }

    fn ir(steps: Vec<Step>) -> RecipeIr {
        RecipeIr {
            schema_version: 1,
            id: "t.at".into(),
            family: "t".into(),
            protocol: 30,
            node: Default::default(),
            below_protocol_valid: false,
            steps,
        }
    }

    #[test]
    fn another_protocol_keeps_labels_and_the_soroban_set_size() {
        let out = at_protocol(
            &ir(vec![
                upgrade("to-21", Some(21), None),
                upgrade("to-22", Some(22), Some(5)),
            ]),
            25,
        );
        assert_eq!(out.protocol, 25);
        assert_eq!(
            out.steps[0],
            Step::Close {
                label: Some("to-21".into()),
                order: Vec::new(),
            }
        );
        assert_eq!(out.steps[1], upgrade("to-22", None, Some(5)));
    }

    fn gate() -> Gate {
        Gate {
            file: "src/a.cpp".into(),
            line: 10,
            column: 5,
            function: "f".into(),
            comparison: Comparison::IsBefore,
            version: 19,
            sides: vec![],
        }
    }

    fn run(kind: RunKind, recipe: Option<&str>, protocol: u32, count: u64) -> GateRun {
        let mut counts = Counts::new();
        counts.entry("src/a.cpp".into()).or_default().insert(
            shapekit_twin::coverage::Span {
                line_start: 10,
                column_start: 5,
                line_end: 10,
                column_end: 30,
            },
            count,
        );
        GateRun {
            kind,
            recipe: recipe.map(str::to_owned),
            recipe_sha256: recipe.map(|_| "ab".repeat(32)),
            protocol,
            counts,
            answers: Vec::new(),
        }
    }

    fn list() -> GateList {
        GateList {
            schema_version: 1,
            core: shapekit_contracts::CoreRef {
                version: "v30.0.0".into(),
                commit: "0e49751aa9ddf892302cca02cea7f3f5d97df9f1".into(),
                image: None,
            },
            gates: vec![gate()],
        }
    }

    #[test]
    fn a_recipe_run_without_its_padded_reference_counts_nothing() {
        let runs = vec![
            run(RunKind::Genesis, None, 18, 2),
            run(RunKind::Recipe, Some("d.r"), 18, 7),
        ];
        let refs: BTreeMap<(RunKind, u32), &GateRun> = runs
            .iter()
            .filter(|r| r.kind != RunKind::Recipe)
            .map(|r| ((r.kind, r.protocol), r))
            .collect();
        assert_eq!(own_count(&gate(), &runs[1], &refs), None);
        let e = side_witnesses(&list(), &runs).unwrap_err();
        assert!(format!("{e:#}").contains("empty-ledgers@p18"), "{e:#}");
        assert!(statuses(&mut list(), &runs).is_err());
    }

    #[test]
    fn a_requested_protocol_without_references_is_refused() {
        let runs = vec![
            run(RunKind::Genesis, None, 18, 2),
            run(RunKind::Padded, None, 18, 4),
        ];
        assert!(references(&runs, &[18]).is_ok());
        let e = references(&runs, &[18, 19]).unwrap_err();
        let e = format!("{e:#}");
        assert!(
            e.contains("genesis@p19") && e.contains("empty-ledgers@p19"),
            "{e}"
        );
    }

    #[test]
    fn calls_beyond_the_padded_reference_witness_the_side() {
        let runs = vec![
            run(RunKind::Genesis, None, 18, 2),
            run(RunKind::Padded, None, 18, 4),
            run(RunKind::Recipe, Some("d.r"), 18, 4),
            run(RunKind::Genesis, None, 17, 2),
            run(RunKind::Padded, None, 17, 2),
            run(RunKind::Recipe, Some("d.r"), 17, 6),
        ];
        let sides = side_witnesses(&list(), &runs).unwrap();
        // true side (before 19): the p18 recipe ran nothing of its own, the p18 padding did
        let names: Vec<&str> = sides[0].runs.iter().map(|r| r.run.as_str()).collect();
        assert_eq!(names, vec!["d.r@p17", "empty-ledgers@p18"]);
        assert_eq!(sides[0].runs[0].reference_count, 2);
    }

    #[test]
    fn saved_runs_read_back_under_their_recipe_hash() {
        let dir = tempfile::tempdir().unwrap();
        let runs = vec![
            run(RunKind::Genesis, None, 18, 2),
            run(RunKind::Padded, None, 18, 4),
            run(RunKind::Recipe, Some("d.r"), 18, 7),
        ];
        for r in &runs {
            save(r, dir.path()).unwrap();
        }
        // a save cut short leaves only its temporary file
        std::fs::write(dir.path().join(".partial-x"), "{").unwrap();
        let back = load_all(dir.path()).unwrap();
        let keys: Vec<String> = back.iter().map(GateRun::key).collect();
        assert_eq!(
            keys,
            vec![
                "genesis@p18".to_owned(),
                "empty-ledgers@p18".to_owned(),
                format!("d.r@p18.{}", "ab".repeat(32)),
            ]
        );
        assert_eq!(back[2].counts, runs[2].counts);
        // a recipe run file without the recipe's hash is refused, not taken as any recipe's
        std::fs::write(
            dir.path().join("x.r@p18.json"),
            r#"{"kind":"recipe","recipe":"x.r","recipe_sha256":null,"protocol":18,"answers":[]}"#,
        )
        .unwrap();
        assert!(load_all(dir.path()).is_err());
    }

    #[test]
    fn a_changed_recipe_has_another_hash_and_each_twin_its_own_runs() {
        let a = ir(vec![upgrade("to-21", Some(21), None)]);
        let b = ir(vec![upgrade("to-22", Some(22), None)]);
        assert_ne!(recipe_hash(&a).unwrap(), recipe_hash(&b).unwrap());
        assert_eq!(recipe_hash(&a).unwrap(), recipe_hash(&a.clone()).unwrap());
        let twin = |d: &str| shapekit_core_driver::config::CoreImage {
            repository: "shapekit/core-twin".into(),
            tag: "t".into(),
            digest: format!("sha256:{d}"),
            local: true,
            profile_dir: None,
        };
        let out = Path::new("out");
        assert_ne!(
            runs_dir(out, &twin(&"1".repeat(64))).unwrap(),
            runs_dir(out, &twin(&"2".repeat(64))).unwrap()
        );
        assert_eq!(
            runs_dir(out, &twin(&"1".repeat(64))).unwrap(),
            Path::new("out/runs").join("1".repeat(64))
        );
    }
}
