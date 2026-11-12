//! `shapekit serve`: the replay server for one committed chain.

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;
use shapekit_serve::{DEFAULT_CACHE_BYTES, ServeConfig, start};

#[derive(Args)]
pub struct ServeArgs {
    /// Chain id (a directory under <corpus>/chains), or a chain's network directory path.
    #[arg(long)]
    chain: String,
    /// Network variant of the chain; ignored when --chain is a path to a network directory.
    #[arg(long, value_parser = ["testnet", "mainnet"])]
    network: Option<String>,
    #[arg(long, default_value = "127.0.0.1:8000")]
    listen: SocketAddr,
    /// Visible head ledger at start; the chain's last ledger when absent.
    #[arg(long)]
    head: Option<u32>,
    /// Byte budget of the decompressed batch cache.
    #[arg(long, default_value_t = DEFAULT_CACHE_BYTES)]
    cache_bytes: usize,
    /// Print every JSON-RPC method called, to stderr.
    #[arg(long)]
    log_requests: bool,
    #[arg(long, default_value = "corpus")]
    corpus: PathBuf,
}

pub fn run(args: ServeArgs) -> Result<bool> {
    let as_path = PathBuf::from(&args.chain);
    let dir = if as_path.join("chain.json").is_file() {
        as_path
    } else {
        let network = args
            .network
            .as_deref()
            .context("--network is needed when --chain is a chain id")?;
        args.corpus.join("chains").join(&args.chain).join(network)
    };
    let server = start(ServeConfig {
        chain_dir: dir.clone(),
        listen: args.listen,
        head: args.head,
        cache_bytes: args.cache_bytes,
        log_requests: args.log_requests,
    })
    .with_context(|| format!("serving {}", dir.display()))?;
    // the harness reads this line to learn the bound address
    println!("serving {} at {}", dir.display(), server.url());
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("starting the signal runtime")?;
    rt.block_on(tokio::signal::ctrl_c())
        .context("waiting for ctrl-c")?;
    server.stop()?;
    Ok(true)
}
