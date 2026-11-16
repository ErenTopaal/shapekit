//! Checks the recipes a run wrote: each is recorded again, in the interpreter's checking
//! mode (every step held to the expectation the search learned), under both network
//! passphrases and twice under each. Both recordings under one passphrase must give the same
//! canonical ledger stream, and each passphrase's chain must hold the arms the run credited
//! to the recipe.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};
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
        Err(e) => {
            ctx.stop()?;
            return Err(e);
        }
    };
    Ok(Recorded {
        network: network.name(),
        canonical_stream_sha256: summary.canonical_stream_sha256,
        arms,
    })
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
        let mut hashes = BTreeSet::new();
        for _ in 0..repeat.max(1) {
            match record(recipe, network, image, reachable, work)
                .with_context(|| format!("{name} ({})", network.name()))
            {
                Ok(r) => {
                    for a in credited.difference(&r.arms) {
                        problems.push(format!("{} lacks credited arm {a}", network.name()));
                    }
                    hashes.insert(r.canonical_stream_sha256.clone());
                    runs.push(r);
                }
                Err(e) => problems.push(format!("{e:#}")),
            }
        }
        if hashes.len() > 1 {
            problems.push(format!(
                "{}: {} different canonical streams over {} recordings",
                network.name(),
                hashes.len(),
                repeat
            ));
        }
    }
    let arm_sets: BTreeMap<&str, &BTreeSet<String>> =
        runs.iter().map(|r| (r.network, &r.arms)).collect();
    if let (Some(t), Some(m)) = (arm_sets.get("testnet"), arm_sets.get("mainnet"))
        && t != m
    {
        problems.push(format!(
            "the two passphrases' chains hold different arms: testnet only {:?}, mainnet only {:?}",
            t.difference(m).collect::<Vec<_>>(),
            m.difference(t).collect::<Vec<_>>()
        ));
    }
    Checked {
        recipe: name.to_owned(),
        runs,
        problems,
    }
}

/// Records `recipe` under `network` into `target` in the corpus's committed form: the chain
/// summary, the canonical ledgers as a SEP-54 lake and the history archive.
pub fn record_into(
    recipe: &IrRecipe,
    network: Network,
    image: &CoreImage,
    work: &Path,
    target: &Path,
) -> Result<String> {
    let dir = tempfile::Builder::new()
        .prefix("explore-record-")
        .tempdir_in(work)?;
    let mut ctx = start_chain(recipe, network, image, dir.path())?;
    if let Err(e) = recipe.run(&mut ctx) {
        ctx.stop()?;
        return Err(e);
    }
    let (summary, _) = ctx.finish(target)?;
    shapekit_pack::pack_chain(target, shapekit_pack::Layout::default())
        .map_err(|e| anyhow::anyhow!("packing {}: {e}", target.display()))?;
    Ok(summary.canonical_stream_sha256)
}
