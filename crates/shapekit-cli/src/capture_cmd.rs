//! `shapekit capture history`: online, run by a maintainer, never in per-PR CI.

use std::path::Path;

use anyhow::{Context, Result};
use shapekit_gen::history::{self, CaptureRequest, Network, Slice};
use shapekit_registry::Registry;

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum NetworkArg {
    Testnet,
    Mainnet,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum Via {
    /// The public SEP-54 data lake (aws-public-blockchain, us-east-2).
    Lake,
    /// stellar-rpc getLedgers, inside the endpoint's retention window only.
    Rpc,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum SliceArg {
    /// The whole LedgerCloseMeta.
    Ledger,
    /// The --tx transaction's envelope and result meta, as two entries.
    Tx,
}

pub struct Args<'a> {
    pub network: NetworkArg,
    pub ledger: u32,
    pub tx: Option<&'a str>,
    pub slice: SliceArg,
    pub family: &'a str,
    pub slug: &'a str,
    pub via: Via,
    pub lake_prefix: Option<&'a str>,
    pub rpc_url: Option<&'a str>,
    pub linked_issues: Vec<String>,
    pub note: Option<&'a str>,
    pub corpus: &'a Path,
    pub registry: &'a Path,
}

pub fn run(args: Args<'_>) -> Result<bool> {
    let registry = Registry::load(args.registry)
        .with_context(|| format!("loading the registry in {}", args.registry.display()))?;
    let network = match args.network {
        NetworkArg::Testnet => Network::Testnet,
        NetworkArg::Mainnet => Network::Mainnet,
    };
    let client = history::client()?;
    let fetched = match args.via {
        Via::Lake => history::fetch_lake(&client, network, args.lake_prefix, args.ledger)?,
        Via::Rpc => {
            let url = match (args.rpc_url, network) {
                (Some(u), _) => u,
                (None, Network::Testnet) => history::TESTNET_RPC,
                (None, Network::Mainnet) => {
                    anyhow::bail!("--via rpc on mainnet needs --rpc-url; sdf runs no public one")
                }
            };
            history::fetch_rpc(&client, url, args.ledger)?
        }
    };
    let date = history::today_utc()?;
    let req = CaptureRequest {
        network,
        ledger: args.ledger,
        tx: args.tx,
        slice: match args.slice {
            SliceArg::Ledger => Slice::Ledger,
            SliceArg::Tx => Slice::Tx,
        },
        family: args.family,
        slug: args.slug,
        linked_issues: args.linked_issues,
        note: args.note,
    };
    let report = history::write_capture(args.corpus, &registry, &req, &fetched, &date)?;
    let ids: Vec<&str> = report.entries.iter().map(|e| e.id.as_str()).collect();
    println!(
        "wrote {} (ledger of {} bytes, {} transactions) via the {} from {}",
        ids.join(" and "),
        fetched.bytes.len(),
        report.tx_count,
        fetched.path,
        fetched.url
    );
    if let Some(i) = report.tx_index {
        println!(
            "transaction {} is at apply index {i}",
            args.tx.unwrap_or_default()
        );
    }
    println!(
        "close time {} ({}{}), lcm v{}, protocol {}",
        report.close_time,
        report.stellar_value_ext,
        report
            .close_time_ms
            .map(|ms| format!(", close_time_ms {ms}"))
            .unwrap_or_default(),
        report.lcm_version,
        report.protocol
    );
    println!("matches {}", report.matched_fragments.join(", "));
    Ok(true)
}
