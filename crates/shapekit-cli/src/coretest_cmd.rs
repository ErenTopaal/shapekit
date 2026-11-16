//! `shapekit coretest`: stellar-core's own `[tx]` tests at every protocol version, and the
//! transaction metas they apply.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, ensure};
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
        /// run.json of an earlier run: start the cases that took longest there first.
        #[arg(long)]
        order_from: Option<PathBuf>,
        /// Run this case once per protocol version, each in its own container (repeatable).
        /// Defaults to the cases one process cannot hold in 2 GB and the longest ones.
        #[arg(long = "split-by-version", default_values = shapekit_coretest::run::SPLIT_BY_DEFAULT)]
        split: Vec<String>,
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
    },
    /// Convert the meta dump of a run back to XDR and check every meta against the short hash
    /// in core's committed baseline. Writes each distinct meta once under --out/meta and one
    /// index line per dumped meta.
    Convert {
        /// Directory written by `coretest run`.
        #[arg(long)]
        run: PathBuf,
        /// A git checkout of stellar-core at the pinned commit, holding
        /// test-tx-meta-baseline-current.
        #[arg(long)]
        core_src: PathBuf,
        /// Output directory, absent or empty.
        #[arg(long)]
        out: PathBuf,
        /// Dumps converted at once.
        #[arg(long, default_value_t = 2)]
        jobs: usize,
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
            order_from,
            split,
            pins,
        } => {
            // with a chosen set of cases, split only those of them the list names
            let split = if tests.is_empty() {
                split
            } else {
                split.into_iter().filter(|s| tests.contains(s)).collect()
            };
            let opts = Options {
                image: Image::from_pins(&pins, shapekit_coretest::BUILDTESTS_IMAGE_PIN)?,
                shards,
                limits: Limits { cpus, memory },
                out,
                tests,
                order_from,
                split,
            };
            run_tests(&opts)
        }
        CoretestCommand::Convert {
            run,
            core_src,
            out,
            jobs,
            pins,
        } => convert(&run, &core_src, &out, jobs, &pins),
    }
}

/// The baseline directory of a core checkout, required to be the pinned commit with no local
/// change to the baseline.
fn pinned_baseline(core_src: &Path, pins_path: &Path) -> Result<PathBuf> {
    let text = std::fs::read_to_string(pins_path)
        .with_context(|| format!("reading {}", pins_path.display()))?;
    let pins = crate::pins::parse(&text)?;
    let want = pins
        .git
        .iter()
        .find(|g| g.name == "stellar-core")
        .and_then(|g| g.sha.clone())
        .context("pins.toml has no stellar-core commit")?;
    let git = |args: &[&str]| -> Result<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(core_src)
            .args(args)
            .output()
            .context("running git")?;
        ensure!(
            out.status.success(),
            "git {} in {} failed: {}",
            args.join(" "),
            core_src.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
        Ok(String::from_utf8(out.stdout)?.trim().to_owned())
    };
    let head = git(&["rev-parse", "--verify", "HEAD^{commit}"])?;
    ensure!(
        head == want,
        "{} is checked out at {head}, pins.toml pins stellar-core {want}",
        core_src.display()
    );
    ensure!(
        git(&["status", "--porcelain", "--", BASELINE_DIR])?.is_empty(),
        "{BASELINE_DIR} in {} has local changes",
        core_src.display()
    );
    Ok(core_src.join(BASELINE_DIR))
}

const BASELINE_DIR: &str = "test-tx-meta-baseline-current";

fn convert(run: &Path, core_src: &Path, out: &Path, jobs: usize, pins: &Path) -> Result<bool> {
    let baseline = pinned_baseline(core_src, pins)?;
    let progress = |name: &str, done: usize, total: usize| {
        eprintln!("[{done}/{total}] {name}");
    };
    let report = shapekit_coretest::convert::convert(run, &baseline, out, jobs, &progress)
        .context("converting the dump")?;
    let t = &report.totals;
    println!("dump of {}", report.run_image);
    println!(
        "{} metas, {} converted, {} conversion errors",
        t.metas, t.converted, t.conversion_errors
    );
    println!(
        "short hashes: {} of {} match core's baseline, {} differ, {} have no baseline entry",
        t.matched, t.metas, t.mismatched, t.not_in_baseline
    );
    println!(
        "baseline: {} hashes, {} not reached by the dump",
        t.baseline_hashes, t.baseline_unreached
    );
    println!(
        "{} distinct metas, {:.1} MB of XDR",
        t.distinct_metas,
        t.distinct_bytes as f64 / 1e6
    );
    let arms: Vec<String> = t
        .meta_versions
        .iter()
        .map(|(v, n)| format!("v{v} {n}"))
        .collect();
    println!("TransactionMeta arms: {}", arms.join(", "));
    println!(
        "{} protocol versions; address hex read as claimable balance {}, as pool {}",
        t.protocols.len(),
        t.address_claimable_balance,
        t.address_liquidity_pool
    );
    for p in &report.problems {
        eprintln!(
            "{}:{} {} | {} #{}: {}",
            p.dump, p.line, p.file, p.section, p.ordinal, p.message
        );
    }
    println!("report {}", out.join("convert.json").display());
    Ok(report.passed())
}

fn run_tests(opts: &Options) -> Result<bool> {
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
    let report = run::run(opts, &progress).context("running core's tests")?;
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
    println!("report {}", opts.out.join("run.json").display());
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
