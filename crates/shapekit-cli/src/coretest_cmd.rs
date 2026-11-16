//! `shapekit coretest`: stellar-core's own `[tx]` tests at every protocol version, and the
//! transaction metas they apply.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Subcommand;
use shapekit_coretest::pins::Image;
use shapekit_coretest::run::{self, Limits, Options, TestRun};

#[derive(Subcommand)]
pub enum CoretestCommand {
    /// Run every [tx] test of the pinned test image with --all-versions, one container per
    /// test case, and keep each case's meta dump and short-hash baseline under --out.
    Run {
        /// Containers running at once.
        #[arg(long, default_value_t = 6)]
        shards: usize,
        /// docker --cpus for each container.
        #[arg(long, default_value = "1")]
        cpus: String,
        /// docker --memory for each container (no swap on top).
        #[arg(long, default_value = "1536m")]
        memory: String,
        /// Output directory, absent or empty. The full dump is several gigabytes.
        #[arg(long)]
        out: PathBuf,
        /// Run only this test case (repeatable); every [tx] case when omitted.
        #[arg(long = "test")]
        tests: Vec<String>,
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
    },
}

pub fn run(command: CoretestCommand) -> Result<bool> {
    match command {
        CoretestCommand::Run {
            shards,
            cpus,
            memory,
            out,
            tests,
            pins,
        } => run_tests(shards, cpus, memory, out, tests, &pins),
    }
}

fn run_tests(
    shards: usize,
    cpus: String,
    memory: String,
    out: PathBuf,
    tests: Vec<String>,
    pins: &std::path::Path,
) -> Result<bool> {
    let image = Image::from_pins(pins, shapekit_coretest::BUILDTESTS_IMAGE_PIN)?;
    let opts = Options {
        image,
        shards,
        limits: Limits { cpus, memory },
        out: out.clone(),
        tests,
    };
    let progress = |t: &TestRun, done: usize, total: usize| {
        let state = match (&t.error, t.exit_code) {
            (Some(e), _) => format!("error: {e}"),
            (None, Some(0)) => format!("{} metas", t.metas),
            (None, Some(c)) => {
                format!("exit {c}{}", if t.oom_killed { ", oom killed" } else { "" })
            }
            (None, None) => "no exit code".to_owned(),
        };
        eprintln!("[{done}/{total}] {:.0}s {}: {state}", t.seconds, t.name);
    };
    let report = run::run(&opts, &progress).context("running core's tests")?;
    let t = &report.totals;
    println!("image {}", report.image);
    println!(
        "{} test cases, {} exit 0, {} dumped at least one meta",
        t.tests, t.passed, t.tests_with_metas
    );
    println!(
        "{} transaction metas, {:.2} GB of dump, {:.0} s wall, {:.0} test seconds summed",
        t.metas,
        t.dump_bytes as f64 / 1e9,
        t.wall_seconds,
        t.test_seconds
    );
    let versions: Vec<String> = t.protocols.keys().map(u32::to_string).collect();
    println!(
        "{} protocol versions: {} ({} metas outside the version wrapper)",
        t.protocols.len(),
        versions.join(" "),
        t.unlabelled
    );
    let arms: Vec<String> = t
        .meta_versions
        .iter()
        .map(|(v, n)| format!("v{v} {n}"))
        .collect();
    println!("TransactionMeta arms: {}", arms.join(", "));
    println!("report {}", out.join("run.json").display());
    for failed in report.tests.iter().filter(|t| !t.passed()) {
        eprintln!(
            "failed: {} (exit {:?}, oom killed {}, {})",
            failed.name,
            failed.exit_code,
            failed.oom_killed,
            failed.error.as_deref().unwrap_or("see its log")
        );
    }
    Ok(report.passed())
}
