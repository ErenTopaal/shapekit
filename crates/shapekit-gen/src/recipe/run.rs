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
use shapekit_core_driver::recorder::CHAIN_FILE;
use shapekit_core_driver::settings::NetworkSettings;
use shapekit_manifest::build::{ENTRIES_DIR, ENTRY_FILE};
use shapekit_manifest::{GeneratedWith, MANIFEST_SCHEMA_VERSION, Manifest, validate};
use shapekit_pack::chain::LEDGERS_DIR;
use shapekit_pack::{Layout, lake_digest, pack_chain};
use shapekit_registry::{Family, Registry};

use super::ctx::{Captured, ChainCtx};
use super::output::{
    CHAINS_DIR, ChainCheck, Journal, chain_entries, check_entry, generator_entry_json,
    generator_files, install_chain, install_entries, merge, oracle_files, reproducible_chain_json,
    write_entry,
};
use super::state_check::{StateCheck, check_against_core};
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
    /// The oracle's state at the head, compared with core's query server.
    pub state_check: StateCheck,
}

#[derive(Debug, Clone)]
pub struct GenReport {
    pub runs: Vec<ChainRun>,
    pub chains: Vec<ChainCheck>,
    pub seconds: f64,
}

/// Flag on every entry of a chain recorded below its family's valid range.
pub use shapekit_manifest::BELOW_PROTOCOL_VALID;

/// The node every recipe chain runs on. Diagnostic events stay off: core
/// would add wall-clock timings to them (see the smoke chain).
fn node_config(recipe: &dyn Recipe, network: Network, image: &CoreImage) -> NodeConfig {
    NodeConfig {
        passphrase: network.passphrase().to_owned(),
        image: image.clone(),
        protocol: recipe.protocol(),
        soroban_high_limits: recipe.soroban_limits(),
        emit_classic_events: recipe.emit_classic_events(),
        backfill_stellar_asset_events: recipe.backfill_stellar_asset_events(),
        diagnostic_events: false,
        min_persistent_ttl: recipe.min_persistent_ttl(),
        invariant_checks: false,
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
    pad_to_first_checkpoint(&mut ctx)?;
    if recipe.soroban_settings() {
        let root = ctx.root().clone();
        ctx.driver()
            .apply_settings(&NetworkSettings::testnet()?, &root, ECHO_WASM)?;
    }
    recipe.run(&mut ctx)?;
    // the node is still up: core's own answer for every touched key is the reference
    let state_check = {
        let d = ctx.driver();
        check_against_core(d.node(), d.recorder().frames(), network.passphrase())
            .context("checking the oracle state against core")?
    };
    let (summary, captured) = ctx.finish(target)?;
    // the committed form: canonical ledgers as a sep-54 lake, the archive as a tar
    pack_chain(target, Layout::default())
        .with_context(|| format!("packing the chain in {}", target.display()))?;
    Ok((
        ChainRun {
            chain_id: chain_id(recipe),
            network,
            seconds: started.elapsed().as_secs_f64(),
            first_ledger: summary.ledger_range.first,
            last_ledger: summary.ledger_range.last,
            captures: captured.len(),
            canonical_stream_sha256: summary.canonical_stream_sha256,
            state_check,
        },
        captured,
    ))
}

/// The ledger closing core's first history checkpoint at the default frequency of 64.
pub const FIRST_CHECKPOINT: u32 = 63;

/// Closes empty ledgers through the first checkpoint before settings and the recipe, so
/// the archive holds a real checkpoint that precedes every shape ledger. Indexers that
/// bootstrap from the archive (wallet-backend live mode on an empty database starts at the
/// latest checkpoint and refuses an archive at ledger 0) can then start before the first
/// shape. Done before the settings so no entry with a ttl ages during the padding.
fn pad_to_first_checkpoint(ctx: &mut ChainCtx) -> Result<()> {
    while ctx.driver().last_closed() < FIRST_CHECKPOINT {
        ctx.driver().close_empty()?;
    }
    Ok(())
}

fn family_of<'r>(registry: &'r Registry, recipe: &dyn Recipe) -> Result<&'r Family> {
    let family = registry
        .family(recipe.family())
        .with_context(|| format!("recipe {}: unknown family {}", recipe.id(), recipe.family()))?;
    let p = recipe.protocol();
    if recipe.below_protocol_valid() {
        ensure!(
            p < family.protocol_valid.min,
            "recipe {} records contrast entries below {}'s valid range, but runs protocol {p}",
            recipe.id(),
            family.id
        );
        return Ok(family);
    }
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
/// changes unless every chain and every check succeeded, and an install that
/// fails part way is rolled back.
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
                            "recorded {} ({}): ledgers {} to {}, {} captures, {:.1}s, \
                             state matches core on {} keys ({} ttls by owner, {} ttls not \
                             live, {} by presence only)",
                            run.chain_id,
                            n.name(),
                            run.first_ledger,
                            run.last_ledger,
                            run.captures,
                            run.seconds,
                            run.state_check.keys_compared,
                            run.state_check.ttls_compared,
                            run.state_check.ttls_not_live,
                            run.state_check.presence_only
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
    for (i, caps) in &mut by_chain {
        let r = recipes[*i];
        if r.below_protocol_valid() {
            for c in caps.values_mut().flatten() {
                c.spec.flags.push(BELOW_PROTOCOL_VALID.to_owned());
            }
        }
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

    // every install goes through one journal, so a failure part way puts back what the
    // earlier installs replaced and the corpus is as it was
    let mut journal = Journal::new(staging.path());
    let installed = (|| -> Result<()> {
        for run in &runs {
            let staged = staging
                .path()
                .join(CHAINS_DIR)
                .join(&run.chain_id)
                .join(run.network.name());
            install_chain(
                &mut journal,
                &staged,
                &opts.corpus,
                &run.chain_id,
                run.network,
            )?;
        }
        for (family, cid, ids) in &installs {
            install_entries(&mut journal, staging.path(), &opts.corpus, family, cid, ids)?;
        }
        Ok(())
    })();
    if let Err(e) = installed {
        let failed = journal.roll_back();
        if failed.is_empty() {
            return Err(e.context("installing; the corpus was restored"));
        }
        // the staging directory still holds what could not be put back
        let kept = staging.keep();
        bail!(
            "installing failed ({e:#}) and the corpus could not be fully restored; the replaced directories are under {}:\n{}",
            kept.display(),
            failed.join("\n")
        );
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
/// SEP-54 lake, the set of entries each chain owns, every file the generator
/// writes for them, and the oracle files `shapekit oracle run` derives from
/// the regenerated chains, byte for byte.
pub fn verify_repro(recipes: &[&dyn Recipe], opts: &GenOptions<'_>) -> Result<Repro> {
    verify_repro_on(recipes, opts, &Network::BOTH)
}

/// [`verify_repro`] under the given passphrases only. Chains are compared for each of
/// them; entries hold a variant per passphrase, so they are compared only when both run.
pub fn verify_repro_on(
    recipes: &[&dyn Recipe],
    opts: &GenOptions<'_>,
    networks: &[Network],
) -> Result<Repro> {
    let fresh = tempfile::Builder::new()
        .prefix("shapekit-repro-")
        .tempdir()
        .context("creating the regeneration directory")?;
    let fresh_opts = GenOptions {
        corpus: fresh.path().to_path_buf(),
        registry: opts.registry,
        image: opts.image.clone(),
        networks: networks.to_vec(),
        jobs: opts.jobs,
        generated_with: opts.generated_with.clone(),
    };
    let both = Network::BOTH.iter().all(|n| networks.contains(n));
    if !both {
        // an entry holds a variant per passphrase; the one not regenerated is kept from the
        // committed entry, so the committed entries of each chain are the starting point
        for r in recipes {
            for (id, dir) in chain_entries(&opts.corpus, r.family(), &chain_id(*r))? {
                let to = fresh.path().join(ENTRIES_DIR).join(r.family()).join(&id);
                copy_dir(&dir, &to)?;
            }
        }
    }
    let report = generate(recipes, &fresh_opts)?;
    // the oracle is rerun with the code under test, so a change to the interpreter that the
    // committed oracle files were not regenerated for shows up here as a byte difference. it
    // reads every variant's chain, so it runs only when both were regenerated, which is also
    // the only case the entries and their oracle files are compared in
    if both {
        shapekit_oracle::corpus::run(fresh.path())
            .map_err(|e| anyhow!("oracle run over the regenerated chains: {e}"))?;
    }
    let mut diffs = Vec::new();
    let mut chains_compared = 0;
    let mut entries_compared = 0;
    for r in recipes {
        let cid = chain_id(*r);
        for &n in networks {
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
            let a = lake_digest(&committed.join(LEDGERS_DIR))
                .map_err(|e| anyhow!("{}: {e}", rel.display()))?;
            let b = lake_digest(&regenerated.join(LEDGERS_DIR))?;
            if a != b {
                diffs.push(format!("{}: ledger lake differs", rel.display()));
            }
        }
        if !both {
            continue;
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
            let committed = oracle_files(&a.join(ENTRY_FILE))?;
            let regenerated = oracle_files(&b.join(ENTRY_FILE))?;
            for f in committed.union(&regenerated) {
                match (committed.contains(f), regenerated.contains(f)) {
                    (true, true) => {
                        let x = std::fs::read(opts.corpus.join(f));
                        let y = std::fs::read(fresh.path().join(f))?;
                        match x {
                            Ok(x) if x == y => {}
                            Ok(_) => diffs.push(format!("{f}: oracle file differs")),
                            Err(e) => diffs.push(format!("{f}: committed file unreadable: {e}")),
                        }
                    }
                    (true, false) => diffs.push(format!("{f}: committed oracle file only")),
                    _ => diffs.push(format!("{f}: regenerated oracle file only")),
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

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    for e in walkdir::WalkDir::new(from) {
        let e = e.with_context(|| format!("walking {}", from.display()))?;
        let rel = e
            .path()
            .strip_prefix(from)
            .expect("walkdir stays below its root");
        let target = to.join(rel);
        if e.file_type().is_dir() {
            std::fs::create_dir_all(&target)
                .with_context(|| format!("creating {}", target.display()))?;
        } else {
            std::fs::copy(e.path(), &target).with_context(|| {
                format!("copying {} to {}", e.path().display(), target.display())
            })?;
        }
    }
    Ok(())
}
