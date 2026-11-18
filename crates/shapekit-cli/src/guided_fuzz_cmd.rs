//! `shapekit guided-fuzz`: the rust, go and javascript testees fuzzed together, steered by
//! their coverage; with --compare the same seed and budget once without the steering.

use std::path::PathBuf;

use anyhow::{Context, Result};
use shapekit_fuzz::guided::{self, GuidedConfig, GuidedRun};

#[derive(clap::Args)]
pub struct GuidedFuzzArgs {
    /// Seed of the input generator; the same seed and budget give the same run.
    #[arg(long, default_value_t = 1)]
    seed: u64,
    /// Inputs per run; every input goes to all three testees.
    #[arg(long, default_value_t = 100_000)]
    max_inputs: u64,
    /// Also run the same seed and budget without the feedback and write both side by side.
    #[arg(long)]
    compare: bool,
    /// Use the coverage builds already in target/testees-cov/.
    #[arg(long)]
    no_build: bool,
    /// Corpus files larger than this are not used as seeds.
    #[arg(long, default_value_t = 64 * 1024)]
    max_seed_bytes: u64,
    /// Most raw bytes handed to `arbitrary` for one generated value; at least 1.
    #[arg(long, default_value_t = 4096, value_parser = clap::value_parser!(u32).range(1..))]
    max_arbitrary_bytes: u32,
    /// report.json (and REPORT.md with --compare).
    #[arg(long, default_value = "target/guided-fuzz")]
    out: PathBuf,
}

fn print(run: &GuidedRun) {
    println!(
        "{} feedback: {} inputs, pool {}",
        if run.feedback { "with" } else { "without" },
        run.inputs,
        run.pool
    );
    for s in &run.sdks {
        println!(
            "  {} ({}): {} ranges ({} at start), {} units, {} inputs grew it, {} pool entries credited",
            s.key,
            s.tool,
            s.ranges,
            s.start_ranges,
            s.units,
            s.inputs_with_new_units,
            s.pool_credit
        );
    }
    for c in &run.classes {
        println!("  {} {}: {} inputs", c.sdk, c.signature, c.count);
    }
}

pub fn run(args: GuidedFuzzArgs) -> Result<bool> {
    let root = std::env::current_dir().context("current directory")?;
    let cfg = GuidedConfig {
        root: root.clone(),
        seed: args.seed,
        max_inputs: args.max_inputs,
        build: !args.no_build,
        max_seed_bytes: args.max_seed_bytes,
        max_arbitrary_bytes: std::num::NonZeroUsize::new(args.max_arbitrary_bytes as usize)
            .context("--max-arbitrary-bytes must be at least 1")?,
    };
    let out = root.join(&args.out);
    std::fs::create_dir_all(&out).with_context(|| out.display().to_string())?;
    let with = guided::run(&cfg, true)?;
    print(&with);
    let mut runs = vec![with];
    if args.compare {
        let without = guided::run(
            &GuidedConfig {
                build: false,
                ..cfg
            },
            false,
        )?;
        print(&without);
        std::fs::write(out.join("REPORT.md"), guided::render(&runs[0], &without))?;
        runs.push(without);
    }
    std::fs::write(
        out.join("report.json"),
        serde_json::to_string_pretty(&runs)? + "\n",
    )?;
    println!("report in {}", args.out.join("report.json").display());
    Ok(true)
}
