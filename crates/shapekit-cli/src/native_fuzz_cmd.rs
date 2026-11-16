//! `shapekit native-fuzz`: an sdk's own coverage-guided fuzzer on its decoder, checked by the
//! encoding laws; new classes go to the candidate lane.

use std::path::PathBuf;

use anyhow::{Context, Result};
use shapekit_fuzz::native::{self, NativeConfig, Target};

#[derive(clap::Args)]
pub struct NativeFuzzArgs {
    /// rust (cargo-fuzz on stellar-xdr), go (go test -fuzz on go-stellar-sdk) or js
    /// (jazzer.js on @stellar/stellar-sdk).
    #[arg(long)]
    target: String,
    /// Wall-clock budget of the fuzzing, restarts included; minimisation runs inside it.
    #[arg(long, default_value_t = 600)]
    seconds: u64,
    /// Work directory and report.json; emptied first.
    #[arg(long)]
    out: Option<PathBuf>,
    #[arg(long, default_value = "candidate")]
    candidate: PathBuf,
    #[arg(long, default_value = "conformance/native-fuzz/known-classes.txt")]
    known: PathBuf,
    /// Skip the javascript testee's build steps (the sdk install must exist).
    #[arg(long)]
    no_build: bool,
    /// Corpus files larger than this are not used as seeds.
    #[arg(long, default_value_t = 64 * 1024)]
    max_seed_bytes: u64,
}

pub fn run(args: NativeFuzzArgs) -> Result<bool> {
    let root = std::env::current_dir().context("current directory")?;
    let target = Target::parse(&args.target)?;
    let out = args
        .out
        .unwrap_or_else(|| PathBuf::from(format!("target/native-fuzz/{}", target.name())));
    let report = native::run(&NativeConfig {
        root: root.clone(),
        target,
        seconds: args.seconds,
        out: root.join(&out),
        candidate: root.join(&args.candidate),
        known: root.join(&args.known),
        build: !args.no_build,
        max_seed_bytes: args.max_seed_bytes,
    })?;
    println!(
        "{} {} with {}: {} s, {} seeds, {} restarts, {} known classes skipped",
        report.sdk,
        report.sdk_version,
        report.tool,
        report.seconds,
        report.seeds,
        report.restarts,
        report.known_skipped.len()
    );
    for f in &report.found {
        println!(
            "  {} ({} bytes): {} {}",
            f.class,
            f.input_len.saturating_sub(1),
            f.entry,
            if f.new_entry {
                "written to the candidate lane"
            } else if f.entry.contains(' ') {
                ""
            } else {
                "already in a lane"
            }
        );
    }
    println!(
        "{} new classes; report in {}",
        report.found.len(),
        out.join("report.json").display()
    );
    Ok(true)
}
