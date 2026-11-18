//! `shapekit gen`: record chains on a private standalone stellar-core.

use std::path::Path;

use anyhow::{Context, Result, bail};
use shapekit_core_driver::config::CoreImage;
use shapekit_core_driver::keys::contract_strkey;
use shapekit_core_driver::smoke::{self, MAINNET_PASSPHRASE, TESTNET_PASSPHRASE};
use shapekit_gen::recipe::{self, GenOptions, GenReport, Network, Recipe};
use shapekit_registry::Registry;

use crate::{manifest_cmd, pins};

/// Records the smoke chain under one network's passphrase into
/// `out`, which must not exist yet.
pub fn smoke(network: &str, out: &Path, pins: &Path) -> Result<bool> {
    let passphrase = match network {
        "testnet" => TESTNET_PASSPHRASE,
        "mainnet" => MAINNET_PASSPHRASE,
        other => bail!("unknown network {other:?}, expected testnet or mainnet"),
    };
    if out.exists() {
        bail!("{} already exists", out.display());
    }
    let image = CoreImage::from_pins(pins)
        .with_context(|| format!("reading the core image pin from {}", pins.display()))?;
    let work = shapekit_pack::scratch::scratch_dir("shapekit-smoke-")
        .context("creating the node workdir")?;
    let chain = smoke::record(smoke::node_config(passphrase, image), work.path(), out)
        .context("recording the smoke chain")?;
    let s = &chain.summary;
    println!(
        "smoke chain ({network}): ledgers {} to {}, protocol {}",
        s.ledger_range.first, s.ledger_range.last, s.protocol
    );
    println!(
        "testnet settings applied in ledger {}, echo contract {}",
        chain.settings.upgrade_ledger,
        contract_strkey(&chain.echo)
    );
    println!(
        "canonical stream {} sha256 {} ({})",
        s.canonical_stream_file, s.canonical_stream_sha256, s.normalization
    );
    println!("raw stream {} sha256 {}", s.stream_file, s.stream_sha256);
    println!("written to {}", out.display());
    Ok(true)
}

/// Every recipe family, one per line, so scripts read the list from the binary instead of
/// the source.
pub fn families(corpus: &Path) -> Result<bool> {
    let set: std::collections::BTreeSet<String> = registered(corpus)?
        .iter()
        .map(|r| r.family().to_owned())
        .collect();
    for f in set {
        println!("{f}");
    }
    Ok(true)
}

/// Where `gen chains` and `gen verify-repro` read and write.
pub struct GenPaths<'a> {
    pub corpus: &'a Path,
    pub registry: &'a Path,
    pub pins: &'a Path,
}

/// The recipes of this build and the chains `<corpus>/recipes/chains.toml` lists, recorded
/// from their recipe IR files.
fn registered(corpus: &Path) -> Result<Vec<Box<dyn recipe::Recipe>>> {
    let mut all = recipe::all();
    // upload paths in a recipe are relative to the repository root, the working directory
    let root = std::env::current_dir().context("reading the working directory")?;
    let index = recipe::chains_index::ChainsIndex::load(corpus)?;
    for r in index.recipes(corpus, &root)? {
        let id = r.chain_id();
        if all.iter().any(|x| x.chain_id() == id) {
            bail!(
                "{} lists chain {id}, which a recipe of this build records",
                recipe::chains_index::ChainsIndex::path(corpus).display()
            );
        }
        all.push(Box::new(r));
    }
    Ok(all)
}

fn selected(corpus: &Path, families: &[String]) -> Result<Vec<Box<dyn recipe::Recipe>>> {
    let all = registered(corpus)?;
    for f in families {
        if !all.iter().any(|r| r.family() == f) {
            bail!("no recipe records family {f}");
        }
    }
    Ok(all
        .into_iter()
        .filter(|r| families.is_empty() || families.iter().any(|f| f == r.family()))
        .collect())
}

fn options<'a>(
    paths: &GenPaths<'_>,
    registry: &'a Registry,
    networks: Vec<Network>,
    jobs: u16,
) -> Result<GenOptions<'a>> {
    let image = CoreImage::from_pins(paths.pins)
        .with_context(|| format!("reading the core image pin from {}", paths.pins.display()))?;
    let generated_with = manifest_cmd::generated_with(&pins::load(paths.pins)?)?;
    Ok(GenOptions {
        corpus: paths.corpus.to_path_buf(),
        registry,
        image,
        networks,
        jobs: usize::from(jobs),
        generated_with,
    })
}

fn print_report(report: &GenReport) {
    for run in &report.runs {
        println!(
            "chain {} ({}): ledgers {} to {}, {} captures, {:.1}s, canonical stream {}",
            run.chain_id,
            run.network.name(),
            run.first_ledger,
            run.last_ledger,
            run.captures,
            run.seconds,
            run.canonical_stream_sha256
        );
    }
    for chain in &report.chains {
        println!("entries of {}:", chain.chain_id);
        for e in &chain.entries {
            println!(
                "  {} ({}): {}",
                e.id,
                if e.passphrase_dependent {
                    "passphrase dependent"
                } else {
                    "same bytes under both passphrases"
                },
                e.matched_fragments.join(", ")
            );
        }
    }
    println!("total {:.1}s", report.seconds);
}

/// `shapekit gen chains`: records every selected recipe's chain under the
/// selected passphrases and installs chains and entries into the corpus.
pub fn chains(families: &[String], network: &str, jobs: u16, paths: &GenPaths<'_>) -> Result<bool> {
    let networks = match network {
        "testnet" => vec![Network::Testnet],
        "mainnet" => vec![Network::Mainnet],
        "both" => Network::BOTH.to_vec(),
        other => bail!("unknown network {other:?}, expected testnet, mainnet or both"),
    };
    let registry = Registry::load(paths.registry)
        .with_context(|| format!("loading the registry in {}", paths.registry.display()))?;
    let recipes = selected(paths.corpus, families)?;
    let refs: Vec<&dyn recipe::Recipe> = recipes.iter().map(|r| r.as_ref()).collect();
    let opts = options(paths, &registry, networks, jobs)?;
    let report = recipe::generate(&refs, &opts)?;
    print_report(&report);
    println!(
        "installed into {}; run `shapekit manifest build` and `shapekit manifest validate`",
        paths.corpus.display()
    );
    Ok(true)
}

/// `shapekit gen verify-repro`: regenerates the selected chains under both
/// passphrases in a temporary directory and compares them with the corpus.
pub fn verify_repro(families: &[String], jobs: u16, paths: &GenPaths<'_>) -> Result<bool> {
    let registry = Registry::load(paths.registry)
        .with_context(|| format!("loading the registry in {}", paths.registry.display()))?;
    let recipes = selected(paths.corpus, families)?;
    let refs: Vec<&dyn recipe::Recipe> = recipes.iter().map(|r| r.as_ref()).collect();
    let opts = options(paths, &registry, Network::BOTH.to_vec(), jobs)?;
    let repro = recipe::verify_repro(&refs, &opts)?;
    print_report(&repro.report);
    for d in &repro.differences {
        println!("difference: {d}");
    }
    println!(
        "{} differences over {} chains and {} entries",
        repro.differences.len(),
        repro.chains_compared,
        repro.entries_compared
    );
    Ok(repro.differences.is_empty())
}

/// `shapekit gen run-ir`: records the chain of one recipe IR file under the chosen
/// passphrases in a temporary corpus and compares it with the committed corpus: the canonical
/// ledger hashes and lake of each chain and, when both passphrases run, every entry the chain
/// owns.
pub fn run_ir(file: &Path, network: &str, jobs: u16, paths: &GenPaths<'_>) -> Result<bool> {
    let networks = match network {
        "testnet" => vec![Network::Testnet],
        "mainnet" => vec![Network::Mainnet],
        "both" => Network::BOTH.to_vec(),
        other => bail!("unknown network {other:?}, expected testnet, mainnet or both"),
    };
    let registry = Registry::load(paths.registry)
        .with_context(|| format!("loading the registry in {}", paths.registry.display()))?;
    // upload paths in a recipe are relative to the repository root, the working directory
    let root = std::env::current_dir().context("reading the working directory")?;
    let ir = recipe::ir_run::IrRecipe::load(file, &root)?;
    let opts = options(paths, &registry, networks.clone(), jobs)?;
    let repro = recipe::verify_repro_on(&[&ir], &opts, &networks)?;
    print_report(&repro.report);
    for d in &repro.differences {
        println!("difference: {d}");
    }
    println!(
        "{} differences over {} chains and {} entries",
        repro.differences.len(),
        repro.chains_compared,
        repro.entries_compared
    );
    Ok(repro.differences.is_empty())
}

/// `shapekit gen checkpoints`: a chain with no transactions that runs through `last`, so
/// its archive holds real checkpoints. Used to test history archive delivery, which the
/// short recipe chains (no checkpoint before ledger 63) cannot exercise. Written into `out`
/// in the committed form.
pub fn checkpoints(
    network: &str,
    protocol: u32,
    last: u32,
    out: &Path,
    pins: &Path,
) -> Result<bool> {
    use shapekit_core_driver::config::close_time;
    use shapekit_core_driver::node::CoreNode;
    use shapekit_core_driver::recorder::{ChainRecorder, FIRST_LEDGER};

    let passphrase = match network {
        "testnet" => TESTNET_PASSPHRASE,
        "mainnet" => MAINNET_PASSPHRASE,
        other => bail!("unknown network {other:?}, expected testnet or mainnet"),
    };
    if last <= FIRST_LEDGER {
        bail!("--last must be above {FIRST_LEDGER}");
    }
    if out.exists() {
        bail!("{} already exists", out.display());
    }
    let image = CoreImage::from_pins(pins)
        .with_context(|| format!("reading the core image pin from {}", pins.display()))?;
    let mut cfg = smoke::node_config(passphrase, image);
    cfg.protocol = protocol;
    cfg.soroban_high_limits = false;
    let work = shapekit_pack::scratch::scratch_dir("shapekit-checkpoints-")
        .context("creating the node workdir")?;
    let node = CoreNode::start(cfg, work.path()).context("starting core")?;
    let mut rec = ChainRecorder::new(&node);
    node.upgrade_to(protocol, FIRST_LEDGER, close_time(FIRST_LEDGER))?;
    for seq in FIRST_LEDGER + 1..=last {
        node.close(seq, close_time(seq))?;
    }
    rec.collect_through(last)?;
    let summary = rec.finish(&node, out)?;
    node.stop()?;
    shapekit_pack::pack_chain(out, shapekit_pack::Layout::default())?;
    println!(
        "{}: ledgers {} to {}, checkpoints {:?}",
        out.display(),
        summary.ledger_range.first,
        summary.ledger_range.last,
        summary.checkpoints
    );
    Ok(true)
}

/// `shapekit gen verify-raw-order`: compares core's raw stream of every chain present in both
/// corpora, ledger by ledger, and fails on any per-key order difference or any difference that
/// is not a reordering.
pub fn verify_raw_order(corpus: &Path, other: &Path) -> Result<bool> {
    use shapekit_core_driver::canonical::{RawOrderStats, compare_raw_order};
    use shapekit_core_driver::meta::read_all;

    let chains = corpus.join("chains");
    let mut names: Vec<String> = std::fs::read_dir(&chains)
        .with_context(|| format!("reading {}", chains.display()))?
        .map(|e| Ok(e?.file_name().to_string_lossy().into_owned()))
        .collect::<std::io::Result<_>>()?;
    names.sort();
    let mut total = RawOrderStats::default();
    let (mut compared, mut missing, mut ledgers, mut ok) = (0usize, Vec::new(), 0usize, true);
    let mut identical = Vec::new();
    for name in &names {
        for network in ["testnet", "mainnet"] {
            let rel = Path::new("chains")
                .join(name)
                .join(network)
                .join("ledgers.xdr");
            let (a, b) = (corpus.join(&rel), other.join(&rel));
            if !a.exists() {
                continue;
            }
            if !b.exists() {
                missing.push(rel.display().to_string());
                continue;
            }
            // byte-identical streams are left out of the comparison: a copied corpus gives
            // them too, and they say nothing about two runs
            let same = std::fs::read(&a).with_context(|| format!("reading {}", a.display()))?
                == std::fs::read(&b).with_context(|| format!("reading {}", b.display()))?;
            if same {
                identical.push(format!("{name} {network}"));
                continue;
            }
            let fa = read_all(&a).with_context(|| format!("reading {}", a.display()))?;
            let fb = read_all(&b).with_context(|| format!("reading {}", b.display()))?;
            let mut chain = RawOrderStats::default();
            if fa.len() != fb.len() {
                println!(
                    "{name} {network}: {} ledgers against {}",
                    fa.len(),
                    fb.len()
                );
                ok = false;
                continue;
            }
            for (x, y) in fa.iter().zip(fb.iter()) {
                if x.ledger_seq != y.ledger_seq {
                    bail!(
                        "{name} {network}: ledger {} paired with {}",
                        x.ledger_seq,
                        y.ledger_seq
                    );
                }
                let s = compare_raw_order(&x.meta, &y.meta)?;
                if s.per_key_mismatches > 0 || s.content_mismatches > 0 {
                    println!(
                        "{name} {network} ledger {}: {} per-key order differences, {} content differences",
                        x.ledger_seq, s.per_key_mismatches, s.content_mismatches
                    );
                }
                chain.add(s);
            }
            ledgers += fa.len();
            compared += 1;
            println!(
                "{name} {network}: {} ledgers, {} lists, {} reordered between keys, {} keys with 2+ changes, {} per-key differences, {} content differences",
                fa.len(),
                chain.lists,
                chain.lists_reordered,
                chain.multi_change_keys,
                chain.per_key_mismatches,
                chain.content_mismatches
            );
            ok &= chain.per_key_mismatches == 0 && chain.content_mismatches == 0;
            total.add(chain);
        }
    }
    // a chain the second recording lacks is a chain whose order was never checked
    for m in &missing {
        println!("not in {}: {m}", other.display());
        ok = false;
    }
    if !identical.is_empty() {
        println!(
            "{} recordings are byte-identical in both corpora, so they are not counted: {}",
            identical.len(),
            identical.join(", ")
        );
    }
    println!(
        "{compared} independent chain recordings, {ledgers} ledgers, {} change lists: {} reordered between keys, {} keys with 2+ changes, {} per-key order differences, {} content differences",
        total.lists,
        total.lists_reordered,
        total.multi_change_keys,
        total.per_key_mismatches,
        total.content_mismatches
    );
    if compared == 0 {
        bail!(
            "no chain is recorded independently in both {} and {}; byte-identical recordings do not count",
            corpus.display(),
            other.display()
        );
    }
    Ok(ok)
}
