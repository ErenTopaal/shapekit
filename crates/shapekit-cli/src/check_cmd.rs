//! `shapekit check sdk|indexer`: the user entry points, thin fronts over `conformance run` and
//! `harness run` with the options an sdk or indexer developer needs and the same defaults.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Subcommand};

use crate::conformance_cmd::{self, ConformanceCommand};
use crate::harness_cmd::{self, HarnessCommand};

#[derive(Subcommand)]
pub enum CheckCommand {
    /// Run the corpus through SDK testees and write the divergence matrix. Runs every SDK in
    /// the versions file at its current version unless told otherwise; the full option set is
    /// `shapekit conformance run --help`.
    Sdk(SdkArgs),
    /// Replay the corpus chains into an indexer behind an adapter and judge what it stored.
    /// Runs every chain under the testnet passphrase unless told otherwise; the full option set
    /// is `shapekit harness run --help`.
    Indexer(IndexerArgs),
}

#[derive(Args)]
pub struct SdkArgs {
    /// SDK to run, a key of the versions file such as rust, go or js; repeatable. Every SDK in
    /// the versions file when absent.
    #[arg(long = "sdk")]
    sdks: Vec<String>,
    /// Exact SDK version to run; repeatable, needs exactly one --sdk.
    #[arg(long = "version")]
    versions: Vec<String>,
    /// Run every version in the versions file instead of the current one.
    #[arg(long)]
    all_versions: bool,
    /// Entry id to run; repeatable. Every entry when absent.
    #[arg(long = "entry")]
    entries: Vec<String>,
    /// Skip the build steps in the versions file (the testees must already be built).
    #[arg(long)]
    no_build: bool,
    /// The corpus directory, holding manifest.json.
    #[arg(long, default_value = "corpus")]
    corpus: PathBuf,
    /// The SDK versions and how to build and start each testee.
    #[arg(long, default_value = "conformance/versions.toml")]
    versions_file: PathBuf,
    /// Directory of <sdk>.txt files listing the known failures.
    #[arg(long, default_value = "conformance/known-failures")]
    known_failures: PathBuf,
    /// Directory for divergence-matrix.json and MATRIX.md.
    #[arg(long, default_value = "target/conformance")]
    out: PathBuf,
}

#[derive(Args)]
pub struct IndexerArgs {
    /// The adapter command line, split on whitespace; quote an argument with ' or ".
    /// docs/adapter-interface.md describes what an adapter does.
    #[arg(long)]
    adapter: String,
    /// Only entries of this family; repeatable.
    #[arg(long)]
    family: Vec<String>,
    /// Only this chain id; repeatable.
    #[arg(long)]
    chain: Vec<String>,
    /// Which passphrase's recording of each chain to replay.
    #[arg(long, default_value = "testnet", value_parser = ["testnet", "mainnet", "both"])]
    network: String,
    /// Expected verdicts (see `shapekit harness expect`): fail when any entry's verdict or
    /// reason differs from the file.
    #[arg(long)]
    expect: Option<PathBuf>,
    /// Directory for harness-report.json, HARNESS.md and the logs.
    #[arg(long, default_value = "harness-out")]
    out: PathBuf,
    #[command(flatten)]
    common: harness_cmd::Common,
}

pub fn run(cmd: CheckCommand) -> Result<bool> {
    match cmd {
        CheckCommand::Sdk(a) => conformance_cmd::run(ConformanceCommand::Run {
            sdks: a.sdks,
            versions: a.versions,
            all_versions: a.all_versions,
            entries: a.entries,
            kinds: Vec::new(),
            flags: Vec::new(),
            linked_issues: Vec::new(),
            no_build: a.no_build,
            dataset: None,
            candidate: PathBuf::from("candidate"),
            coverage: false,
            obligations: false,
            passes: 1,
            manifest: a.corpus.join("manifest.json"),
            versions_file: a.versions_file,
            known_failures: a.known_failures,
            out: a.out,
        }),
        CheckCommand::Indexer(a) => harness_cmd::run(HarnessCommand::Run(harness_cmd::RunArgs {
            adapter: a.adapter,
            family: a.family,
            chain: a.chain,
            chain_dir: Vec::new(),
            network: a.network,
            out: a.out,
            repeat: 1,
            memory_slack: 0.1,
            memory_budget: None,
            expect: a.expect,
            common: a.common,
        })),
    }
}
