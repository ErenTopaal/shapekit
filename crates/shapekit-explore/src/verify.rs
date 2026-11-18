//! Checks the recipes a run wrote: each is recorded again, in the interpreter's checking
//! mode (every step held to the expectation the search learned), under both network
//! passphrases and twice under each. Both recordings under one passphrase must give the same
//! canonical ledger stream, and each passphrase's chain must hold the arms the run credited
//! to the recipe.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use shapekit_core_driver::config::CoreImage;
use shapekit_core_driver::recorder::canonical_frame;
use shapekit_gen::recipe::ir_run::IrRecipe;
use shapekit_gen::recipe::{Network, Recipe, start_chain};

use crate::arms::{arms_in, ledger_signature};

#[derive(Debug, Clone, Serialize)]
pub struct Recorded {
    pub network: &'static str,
    pub canonical_stream_sha256: String,
    pub arms: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Checked {
    pub recipe: String,
    pub runs: Vec<Recorded>,
    pub problems: Vec<String>,
}

/// Records `recipe` once under `network` into a scratch directory under `work`.
pub fn record(
    recipe: &IrRecipe,
    network: Network,
    image: &CoreImage,
    reachable: &BTreeSet<String>,
    work: &Path,
) -> Result<Recorded> {
    let dir = tempfile::Builder::new()
        .prefix("explore-verify-")
        .tempdir_in(work)?;
    let node = dir.path().join("node");
    std::fs::create_dir_all(&node)?;
    let mut ctx = start_chain(recipe, network, image, &node)?;
    let ran = recipe.run(&mut ctx);
    let mut arms = BTreeSet::new();
    if ran.is_ok() {
        for f in ctx.driver().recorder().frames() {
            let c = canonical_frame(f)?;
            arms.extend(arms_in(&ledger_signature(&c.meta)?, reachable));
        }
    }
    let target = dir.path().join("chain");
    let summary = match ran {
        Ok(()) => ctx.finish(&target)?.0,
        Err(e) => return Err(stopped(e, ctx.stop())),
    };
    Ok(Recorded {
        network: network.name(),
        canonical_stream_sha256: summary.canonical_stream_sha256,
        arms,
    })
}

/// The recipes of an `explore run` output to verify: the file stems of `<dir>/recipes/*.json`,
/// each with the arms `report.json` credits to it. A run with no recipe, a witness without a
/// recipe name, and a witness naming a recipe file that is not there are errors: each would
/// let a verification pass that checked nothing for the arms involved.
pub fn recipes_to_check(dir: &Path) -> Result<BTreeMap<String, BTreeSet<String>>> {
    let report_path = dir.join(crate::explore::REPORT);
    let report: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&report_path)
            .with_context(|| format!("reading {}", report_path.display()))?,
    )
    .with_context(|| format!("parsing {}", report_path.display()))?;
    let witnesses = report["witnesses"]
        .as_object()
        .with_context(|| format!("{} has no witnesses", report_path.display()))?;
    let recipes = dir.join(crate::explore::RECIPES);
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for e in
        std::fs::read_dir(&recipes).with_context(|| format!("reading {}", recipes.display()))?
    {
        let p = e
            .with_context(|| format!("reading {}", recipes.display()))?
            .path();
        if p.extension().is_some_and(|x| x == "json") {
            let name = p
                .file_stem()
                .and_then(|n| n.to_str())
                .with_context(|| format!("{} has no UTF-8 file name", p.display()))?;
            out.insert(name.to_owned(), BTreeSet::new());
        }
    }
    if out.is_empty() {
        bail!(
            "{} holds no recipe, so there is nothing to verify",
            recipes.display()
        );
    }
    for (arm, w) in witnesses {
        let name = w["recipe"].as_str().with_context(|| {
            format!(
                "{}: the witness of {arm} names no recipe",
                report_path.display()
            )
        })?;
        out.get_mut(name)
            .with_context(|| {
                format!(
                    "{}: {arm} is credited to recipe {name}, which {} does not hold",
                    report_path.display(),
                    recipes.display()
                )
            })?
            .insert(arm.clone());
    }
    Ok(out)
}

/// The recipe's error `e` after its node was stopped: the recipe's failure is what the run
/// reports, and a failed stop is added to it rather than taking its place.
fn stopped(e: anyhow::Error, stop: Result<()>) -> anyhow::Error {
    match stop {
        Ok(()) => e,
        Err(s) => e.context(format!("stopping the node afterwards also failed: {s:#}")),
    }
}

/// Checks one recipe; `credited` are the arms the run credited to it.
pub fn check(
    name: &str,
    recipe: &IrRecipe,
    credited: &BTreeSet<String>,
    image: &CoreImage,
    reachable: &BTreeSet<String>,
    work: &Path,
    repeat: usize,
) -> Checked {
    let mut runs = Vec::new();
    let mut problems = Vec::new();
    for network in Network::BOTH {
        for _ in 0..repeat.max(1) {
            match record(recipe, network, image, reachable, work)
                .with_context(|| format!("{name} ({})", network.name()))
            {
                Ok(r) => {
                    for a in credited.difference(&r.arms) {
                        problems.push(format!("{} lacks credited arm {a}", network.name()));
                    }
                    runs.push(r);
                }
                Err(e) => problems.push(format!("{e:#}")),
            }
        }
    }
    problems.extend(disagreements(&runs));
    Checked {
        recipe: name.to_owned(),
        runs,
        problems,
    }
}

/// Where the recordings that succeeded disagree: different canonical streams under one
/// passphrase, or different arms in any two recordings (each passphrase's chain must hold
/// the same arms, and every recording of it too).
fn disagreements(runs: &[Recorded]) -> Vec<String> {
    let mut out = Vec::new();
    for network in Network::BOTH {
        let of: Vec<&Recorded> = runs
            .iter()
            .filter(|r| r.network == network.name())
            .collect();
        let hashes: BTreeSet<&str> = of
            .iter()
            .map(|r| r.canonical_stream_sha256.as_str())
            .collect();
        if hashes.len() > 1 {
            out.push(format!(
                "{}: {} different canonical streams over {} recordings",
                network.name(),
                hashes.len(),
                of.len()
            ));
        }
    }
    if let Some(first) = runs.first() {
        for r in &runs[1..] {
            if r.arms != first.arms {
                out.push(format!(
                    "recordings hold different arms: {} only {:?}, {} only {:?}",
                    first.network,
                    first.arms.difference(&r.arms).collect::<Vec<_>>(),
                    r.network,
                    r.arms.difference(&first.arms).collect::<Vec<_>>()
                ));
            }
        }
    }
    out
}

/// Records `recipe` under `network` into `target` in the corpus's committed form: the chain
/// summary, the canonical ledgers as a SEP-54 lake and the history archive.
pub fn record_into(
    recipe: &IrRecipe,
    network: Network,
    image: &CoreImage,
    work: &Path,
    target: &Path,
) -> Result<(String, stellar_xdr::LedgerCloseMeta)> {
    let dir = tempfile::Builder::new()
        .prefix("explore-record-")
        .tempdir_in(work)?;
    let mut ctx = start_chain(recipe, network, image, dir.path())?;
    if let Err(e) = recipe.run(&mut ctx) {
        return Err(stopped(e, ctx.stop()));
    }
    let last = ctx.driver().last_meta().clone();
    let (summary, _) = ctx.finish(target)?;
    shapekit_pack::pack_chain(target, shapekit_pack::Layout::default())
        .map_err(|e| anyhow::anyhow!("packing {}: {e}", target.display()))?;
    Ok((summary.canonical_stream_sha256, last))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_stop_keeps_the_recipe_error() {
        let e = stopped(
            anyhow::anyhow!("step 3 failed"),
            Err(anyhow::anyhow!("docker stop timed out")),
        );
        let text = format!("{e:#}");
        assert!(text.contains("step 3 failed"), "{text}");
        assert!(text.contains("docker stop timed out"), "{text}");
        assert_eq!(e.root_cause().to_string(), "step 3 failed");
        let e = stopped(anyhow::anyhow!("step 3 failed"), Ok(()));
        assert_eq!(format!("{e:#}"), "step 3 failed");
    }

    fn recorded(network: &'static str, hash: &str, arms: &[&str]) -> Recorded {
        Recorded {
            network,
            canonical_stream_sha256: hash.into(),
            arms: arms.iter().map(|a| (*a).to_owned()).collect(),
        }
    }

    #[test]
    fn disagreements_count_the_recordings_made_and_compare_every_arm_set() {
        // two testnet recordings where the earlier one lacks an arm the rest hold
        let runs = vec![
            recorded("testnet", "h1", &["A"]),
            recorded("testnet", "h1", &["A", "B"]),
            recorded("mainnet", "h2", &["A", "B"]),
        ];
        let d = disagreements(&runs);
        assert_eq!(d.len(), 2, "{d:?}");
        // one mainnet recording failed: the stream count says one, not the repeat asked
        let runs = vec![
            recorded("testnet", "h1", &["A"]),
            recorded("testnet", "h3", &["A"]),
            recorded("mainnet", "h2", &["A"]),
        ];
        assert_eq!(
            disagreements(&runs),
            vec!["testnet: 2 different canonical streams over 2 recordings".to_owned()]
        );
        assert!(disagreements(&[recorded("testnet", "h1", &["A"])]).is_empty());
    }

    fn run_dir(recipes: &[&str], witnesses: serde_json::Value) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join(crate::explore::RECIPES)).unwrap();
        for r in recipes {
            std::fs::write(
                d.path()
                    .join(crate::explore::RECIPES)
                    .join(format!("{r}.json")),
                "{}",
            )
            .unwrap();
        }
        std::fs::write(
            d.path().join(crate::explore::REPORT),
            serde_json::to_vec(&serde_json::json!({ "witnesses": witnesses })).unwrap(),
        )
        .unwrap();
        d
    }

    #[test]
    fn verification_needs_recipes_and_every_witness_recipe() {
        let d = run_dir(
            &["r0001", "r0002"],
            serde_json::json!({ "A": {"recipe": "r0001", "ledger": 3}, "B": {"recipe": "r0001", "ledger": 4} }),
        );
        let got = recipes_to_check(d.path()).unwrap();
        assert_eq!(
            got["r0001"],
            BTreeSet::from(["A".to_owned(), "B".to_owned()])
        );
        assert!(got["r0002"].is_empty());

        let empty = run_dir(&[], serde_json::json!({}));
        let e = recipes_to_check(empty.path()).unwrap_err();
        assert!(format!("{e:#}").contains("holds no recipe"), "{e:#}");

        let missing = run_dir(
            &["r0001"],
            serde_json::json!({ "A": {"recipe": "r0009", "ledger": 3} }),
        );
        let e = recipes_to_check(missing.path()).unwrap_err();
        assert!(format!("{e:#}").contains("r0009"), "{e:#}");
    }
}
