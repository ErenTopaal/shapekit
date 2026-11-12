//! Runs recipes as chains, in parallel, and installs what they produce;
//! and regenerates them to check reproducibility.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::{Context, Result, anyhow, bail, ensure};
use shapekit_core_driver::chain::ChainDriver;
use shapekit_core_driver::config::{CoreImage, NodeConfig};
use shapekit_core_driver::contracts::ECHO_WASM;
use shapekit_core_driver::recorder::{CANONICAL_STREAM_FILE, CHAIN_FILE};
use shapekit_core_driver::settings::NetworkSettings;
use shapekit_manifest::build::ENTRY_FILE;
use shapekit_manifest::{GeneratedWith, MANIFEST_SCHEMA_VERSION, Manifest, sha256_hex, validate};
use shapekit_registry::{Family, Registry};

use super::ctx::{Captured, ChainCtx};
use super::output::{
    CHAINS_DIR, ChainCheck, chain_entries, check_entry, generator_entry_json, generator_files,
    install_chain, install_entries, merge, reproducible_chain_json, write_entry,
};
use super::{Network, Recipe, chain_id};

pub struct GenOptions<'a> {
    /// The corpus directory entries and chains are installed into.
    pub corpus: PathBuf,
    pub registry: &'a Registry,
    pub image: CoreImage,
    pub networks: Vec<Network>,
    /// Chains recorded at once, each on its own node.
    pub jobs: usize,
    /// The stack the corpus header names; the produced entries are run
    /// through the manifest validator under it before they are installed.
    pub generated_with: GeneratedWith,
}

/// One recorded chain under one passphrase.
#[derive(Debug, Clone)]
pub struct ChainRun {
    pub chain_id: String,
    pub network: Network,
    pub seconds: f64,
    pub first_ledger: u32,
    pub last_ledger: u32,
    pub captures: usize,
    pub canonical_stream_sha256: String,
}

#[derive(Debug, Clone)]
pub struct GenReport {
    pub runs: Vec<ChainRun>,
    pub chains: Vec<ChainCheck>,
    pub seconds: f64,
}

/// The node every recipe chain runs on. Diagnostic events stay off: core
/// would add wall-clock timings to them (see the smoke chain).
fn node_config(recipe: &dyn Recipe, network: Network, image: &CoreImage) -> NodeConfig {
    NodeConfig {
        passphrase: network.passphrase().to_owned(),
        image: image.clone(),
        protocol: recipe.protocol(),
        soroban_high_limits: recipe.soroban_settings(),
        emit_classic_events: true,
        backfill_stellar_asset_events: true,
        diagnostic_events: false,
    }
}

type JobResult = Result<(ChainRun, Vec<Captured>)>;

fn run_job(recipe: &dyn Recipe, network: Network, image: &CoreImage, target: &Path) -> JobResult {
    let started = Instant::now();
    let work = tempfile::Builder::new()
        .prefix("shapekit-gen-")
        .tempdir()
        .context("creating the node workdir")?;
    let driver = ChainDriver::start(node_config(recipe, network, image), work.path())?;
    let mut ctx = ChainCtx::new(driver, recipe.id(), network, recipe.protocol());
    if recipe.soroban_settings() {
        let root = ctx.root().clone();
        ctx.driver()
            .apply_settings(&NetworkSettings::testnet()?, &root, ECHO_WASM)?;
    }
    recipe.run(&mut ctx)?;
    let (summary, captured) = ctx.finish(target)?;
    Ok((
        ChainRun {
            chain_id: chain_id(recipe),
            network,
            seconds: started.elapsed().as_secs_f64(),
            first_ledger: summary.ledger_range.first,
            last_ledger: summary.ledger_range.last,
            captures: captured.len(),
            canonical_stream_sha256: summary.canonical_stream_sha256,
        },
        captured,
    ))
}

fn family_of<'r>(registry: &'r Registry, recipe: &dyn Recipe) -> Result<&'r Family> {
    let family = registry
        .family(recipe.family())
        .with_context(|| format!("recipe {}: unknown family {}", recipe.id(), recipe.family()))?;
    let p = recipe.protocol();
    ensure!(
        p >= family.protocol_valid.min && family.protocol_valid.max.is_none_or(|m| p <= m),
        "recipe {} runs protocol {p}, outside {}'s valid range",
        recipe.id(),
        family.id
    );
    Ok(family)
}

/// Records every recipe under every requested network, checks the entries
/// and installs chains and entries into the corpus. Nothing in the corpus
/// changes unless every chain and every check succeeded.
pub fn generate(recipes: &[&dyn Recipe], opts: &GenOptions<'_>) -> Result<GenReport> {
    let started = Instant::now();
    ensure!(!recipes.is_empty(), "no recipes selected");
    ensure!(!opts.networks.is_empty(), "no networks selected");
    let mut ids = BTreeSet::new();
    for r in recipes {
        family_of(opts.registry, *r)?;
        ensure!(ids.insert(chain_id(*r)), "chain {} twice", chain_id(*r));
    }
    std::fs::create_dir_all(&opts.corpus)
        .with_context(|| format!("creating {}", opts.corpus.display()))?;
    // staged inside the corpus so installing is a rename on one filesystem
    let staging = tempfile::Builder::new()
        .prefix(".gen-staging-")
        .tempdir_in(&opts.corpus)
        .context("creating the staging directory")?;

    let jobs: Vec<(usize, Network)> = (0..recipes.len())
        .flat_map(|i| opts.networks.iter().map(move |n| (i, *n)))
        .collect();
    let next = AtomicUsize::new(0);
    let results: Mutex<BTreeMap<usize, JobResult>> = Mutex::new(BTreeMap::new());
    std::thread::scope(|s| {
        for _ in 0..opts.jobs.clamp(1, jobs.len()) {
            s.spawn(|| {
                loop {
                    let j = next.fetch_add(1, Ordering::SeqCst);
                    let Some((i, n)) = jobs.get(j).copied() else {
                        break;
                    };
                    let r = recipes[i];
                    let target = staging
                        .path()
                        .join(CHAINS_DIR)
                        .join(chain_id(r))
                        .join(n.name());
                    let out = run_job(r, n, &opts.image, &target)
                        .with_context(|| format!("chain {} ({})", chain_id(r), n.name()));
                    if let Ok((run, _)) = &out {
                        eprintln!(
                            "recorded {} ({}): ledgers {} to {}, {} captures, {:.1}s",
                            run.chain_id,
                            n.name(),
                            run.first_ledger,
                            run.last_ledger,
                            run.captures,
                            run.seconds
                        );
                    }
                    results
                        .lock()
                        .expect("a job panicked while holding the results")
                        .insert(j, out);
                }
            });
        }
    });
    let mut results = results.into_inner().expect("no job holds the lock");
    let mut failures = Vec::new();
    let mut runs = Vec::new();
    let mut by_chain: BTreeMap<usize, BTreeMap<Network, Vec<Captured>>> = BTreeMap::new();
    for (j, (i, n)) in jobs.iter().enumerate() {
        match results.remove(&j).expect("every job reports") {
            Ok((run, caps)) => {
                runs.push(run);
                by_chain.entry(*i).or_default().insert(*n, caps);
            }
            Err(e) => failures.push(format!("{e:#}")),
        }
    }
    if !failures.is_empty() {
        bail!("{} chains failed:\n{}", failures.len(), failures.join("\n"));
    }

    let mut checks = Vec::new();
    let mut installs: Vec<(String, String, Vec<String>)> = Vec::new();
    let mut all_entries = Vec::new();
    for (i, caps) in &by_chain {
        let r = recipes[*i];
        let family = family_of(opts.registry, r)?;
        let cid = chain_id(r);
        let pending = merge(family, r.id(), &cid, r.protocol(), caps, &opts.corpus)?;
        let mut entries = Vec::new();
        for p in &pending {
            entries.push(check_entry(family, p)?);
            all_entries.push(write_entry(staging.path(), p)?);
        }
        installs.push((
            family.id.clone(),
            cid.clone(),
            pending.iter().map(|p| p.entry.id.clone()).collect(),
        ));
        checks.push(ChainCheck {
            chain_id: cid,
            entries,
        });
    }

    all_entries.sort_by(|a, b| a.id.cmp(&b.id));
    let tiers: BTreeSet<u32> = all_entries
        .iter()
        .filter_map(|e| e.protocol.generated_at)
        .collect();
    let check_manifest = Manifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        corpus_version: "generated-entries".into(),
        generated_with: opts.generated_with.clone(),
        protocol_tiers: tiers.into_iter().collect(),
        entries: all_entries,
    };
    let problems = validate(&check_manifest, opts.registry, staging.path());
    if !problems.is_empty() {
        let lines: Vec<String> = problems.iter().map(|p| p.to_string()).collect();
        bail!(
            "the generated entries fail manifest validation:\n{}",
            lines.join("\n")
        );
    }

    for run in &runs {
        let staged = staging
            .path()
            .join(CHAINS_DIR)
            .join(&run.chain_id)
            .join(run.network.name());
        install_chain(&staged, &opts.corpus, &run.chain_id, run.network)?;
    }
    for (family, cid, ids) in &installs {
        install_entries(staging.path(), &opts.corpus, family, cid, ids)?;
    }
    staging.close().context("removing the staging directory")?;
    Ok(GenReport {
        runs,
        chains: checks,
        seconds: started.elapsed().as_secs_f64(),
    })
}

/// Differences between a fresh regeneration and the committed corpus.
#[derive(Debug, Clone)]
pub struct Repro {
    pub report: GenReport,
    pub differences: Vec<String>,
    /// Chain directories and entries compared.
    pub chains_compared: usize,
    pub entries_compared: usize,
}

/// Regenerates `recipes` under both passphrases into a temporary corpus and
/// compares it with `opts.corpus`: every chain's canonical ledger hashes and
/// canonical stream, the set of entries each chain owns, and every file the
/// generator writes for them, byte for byte.
pub fn verify_repro(recipes: &[&dyn Recipe], opts: &GenOptions<'_>) -> Result<Repro> {
    let fresh = tempfile::Builder::new()
        .prefix("shapekit-repro-")
        .tempdir()
        .context("creating the regeneration directory")?;
    let fresh_opts = GenOptions {
        corpus: fresh.path().to_path_buf(),
        registry: opts.registry,
        image: opts.image.clone(),
        networks: Network::BOTH.to_vec(),
        jobs: opts.jobs,
        generated_with: opts.generated_with.clone(),
    };
    let report = generate(recipes, &fresh_opts)?;
    let mut diffs = Vec::new();
    let mut chains_compared = 0;
    let mut entries_compared = 0;
    for r in recipes {
        let cid = chain_id(*r);
        for n in Network::BOTH {
            chains_compared += 1;
            let rel = Path::new(CHAINS_DIR).join(&cid).join(n.name());
            let committed = opts.corpus.join(&rel);
            let regenerated = fresh.path().join(&rel);
            if !committed.join(CHAIN_FILE).exists() {
                diffs.push(format!("{}: no committed chain", rel.display()));
                continue;
            }
            if reproducible_chain_json(&committed.join(CHAIN_FILE))?
                != reproducible_chain_json(&regenerated.join(CHAIN_FILE))?
            {
                diffs.push(format!(
                    "{}: chain.json differs in canonical hashes or ledger range",
                    rel.display()
                ));
            }
            let a = std::fs::read(committed.join(CANONICAL_STREAM_FILE))
                .map_err(|e| anyhow!("{}: {e}", rel.display()))?;
            let b = std::fs::read(regenerated.join(CANONICAL_STREAM_FILE))?;
            if sha256_hex(&a) != sha256_hex(&b) {
                diffs.push(format!("{}: canonical stream differs", rel.display()));
            }
        }
        let family = r.family();
        let ours = chain_entries(&opts.corpus, family, &cid)?;
        let theirs = chain_entries(fresh.path(), family, &cid)?;
        let ids: BTreeSet<&String> = ours.keys().chain(theirs.keys()).collect();
        for id in ids {
            entries_compared += 1;
            let (Some(a), Some(b)) = (ours.get(id), theirs.get(id)) else {
                diffs.push(format!(
                    "{id}: {} only",
                    if ours.contains_key(id) {
                        "committed"
                    } else {
                        "regenerated"
                    }
                ));
                continue;
            };
            if generator_entry_json(&a.join(ENTRY_FILE))?
                != generator_entry_json(&b.join(ENTRY_FILE))?
            {
                diffs.push(format!("{id}: entry.json differs"));
            }
            for f in generator_files(&b.join(ENTRY_FILE))? {
                let x = std::fs::read(opts.corpus.join(&f));
                let y = std::fs::read(fresh.path().join(&f))?;
                match x {
                    Ok(x) if x == y => {}
                    Ok(_) => diffs.push(format!("{f}: bytes differ")),
                    Err(e) => diffs.push(format!("{f}: committed file unreadable: {e}")),
                }
            }
        }
    }
    Ok(Repro {
        report,
        differences: diffs,
        chains_compared,
        entries_compared,
    })
}
