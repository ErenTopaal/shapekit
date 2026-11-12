//! `shapekit gen`: record chains on a private standalone stellar-core.

use std::path::Path;

use anyhow::{Context, Result, bail};
use shapekit_core_driver::config::CoreImage;
use shapekit_core_driver::keys::contract_strkey;
use shapekit_core_driver::smoke::{self, MAINNET_PASSPHRASE, TESTNET_PASSPHRASE};
use shapekit_gen::recipe::{self, GenOptions, GenReport, Network};
use shapekit_registry::Registry;

use crate::{manifest_cmd, pins};

/// Records the phase 3 smoke chain under one network's passphrase into
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
    let work = tempfile::Builder::new()
        .prefix("shapekit-smoke-")
        .tempdir()
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

/// Where `gen chains` and `gen verify-repro` read and write.
pub struct GenPaths<'a> {
    pub corpus: &'a Path,
    pub registry: &'a Path,
    pub pins: &'a Path,
}

fn selected(families: &[String]) -> Result<Vec<Box<dyn recipe::Recipe>>> {
    let all = recipe::all();
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
    let recipes = selected(families)?;
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
    let recipes = selected(families)?;
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
    let work = tempfile::Builder::new()
        .prefix("shapekit-checkpoints-")
        .tempdir()
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
