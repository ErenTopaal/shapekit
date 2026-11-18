//! `shapekit fuzz`: differential decode and re-encode fuzzing, stellar-xdr against the go
//! testee. See docs/differential-fuzzing.md.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use shapekit_fuzz::types::FuzzType;

#[derive(clap::Args)]
pub struct FuzzArgs {
    /// Wall-clock budget of the generation phase; minimisation runs after it.
    #[arg(long, default_value_t = 60)]
    seconds: u64,
    /// Seeds every worker's generator. With --max-inputs, a run that finishes inside --seconds
    /// writes the same report.json for the same seed whatever --jobs is set to.
    #[arg(long, default_value_t = 1)]
    seed: u64,
    /// Workers, each with its own go testee process.
    #[arg(long, default_value_t = 4)]
    jobs: usize,
    /// Inputs in total, split into a fixed share per worker. Without it the clock ends the
    /// run and the run is not reproducible.
    #[arg(long)]
    max_inputs: Option<u64>,
    /// Types to fuzz, comma separated (default: every type both testees decode).
    #[arg(long, value_delimiter = ',')]
    types: Vec<String>,
    /// The go testee version from conformance/versions.toml (default: its current one).
    #[arg(long)]
    go_version: Option<String>,
    /// Use the go testee binary already built instead of running its build steps.
    #[arg(long)]
    no_build: bool,
    /// Corpus files larger than this are not used as seeds.
    #[arg(long, default_value_t = 64 * 1024)]
    max_seed_bytes: u64,
    /// Most raw bytes handed to `arbitrary` for one generated value.
    #[arg(long, default_value_t = 4096)]
    max_arbitrary_bytes: usize,
    /// The corpus directory, holding manifest.json.
    #[arg(long, default_value = "corpus")]
    corpus: PathBuf,
    /// The SDK versions file naming the go testee build and command.
    #[arg(long, default_value = "conformance/versions.toml")]
    versions: PathBuf,
    /// Run report and a reproducer for every class seen.
    #[arg(long, default_value = "target/fuzz")]
    out: PathBuf,
    /// Where reproducers of classes not in the known list are written.
    #[arg(long, default_value = "findings/fuzz")]
    findings: PathBuf,
    /// Classes that are triaged already; seeing them does not fail the run.
    #[arg(long, default_value = "findings/fuzz/known-classes.txt")]
    known: PathBuf,
    /// Instead of fuzzing, run every stored reproducer under this directory again and fail
    /// if one no longer lands in the class it was filed under.
    #[arg(long)]
    replay: Option<PathBuf>,
}

pub fn run(args: FuzzArgs) -> Result<bool> {
    let types = if args.types.is_empty() {
        FuzzType::ALL.to_vec()
    } else {
        let mut out = Vec::new();
        for name in &args.types {
            match FuzzType::from_name(name) {
                Some(t) => out.push(t),
                None => bail!(
                    "{name} is not a fuzzed type (have: {})",
                    FuzzType::ALL
                        .iter()
                        .map(|t| t.name())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            }
        }
        out
    };
    let cfg = shapekit_fuzz::Config {
        root: std::env::current_dir().context("current directory")?,
        corpus: args.corpus,
        versions: args.versions,
        go_version: args.go_version,
        build: !args.no_build,
        seconds: args.seconds,
        max_inputs: args.max_inputs,
        seed: args.seed,
        jobs: args.jobs,
        types,
        max_seed_bytes: args.max_seed_bytes,
        max_arbitrary_bytes: args.max_arbitrary_bytes.max(1),
    };

    if let Some(dir) = args.replay {
        let replayed = shapekit_fuzz::replay(&cfg, &dir)?;
        let mut ok = true;
        for r in &replayed {
            match &r.now {
                Some(now) if *now == r.id => println!("{}: still reproduces", r.id),
                Some(now) => {
                    ok = false;
                    println!("{}: now lands in {now}", r.id);
                }
                None => {
                    ok = false;
                    println!(
                        "{}: no longer disagrees (rust {}, go {})",
                        r.id,
                        r.rust.describe(),
                        r.go.describe()
                    );
                }
            }
        }
        println!("{} reproducers replayed", replayed.len());
        return Ok(ok);
    }

    let known = shapekit_fuzz::load_known(&args.known)?;
    let outcome = shapekit_fuzz::run(&cfg)?;
    let written =
        shapekit_fuzz::report::write(&cfg, &outcome, &known, &args.out, Some(&args.findings))?;
    let secs = outcome.elapsed.as_secs_f64();
    println!(
        "{} inputs in {secs:.0} s ({:.0}/s) against {}; {} classes ({} known, {} new)",
        outcome.inputs(),
        outcome.inputs() as f64 / secs.max(1e-9),
        outcome.go.key(),
        outcome.classes.len(),
        written.known.len(),
        written.new.len()
    );
    for id in &written.new {
        println!(
            "new class {id}: reproducer under {}",
            args.findings.join(id).display()
        );
    }
    if outcome.time_cut {
        println!(
            "the time budget ended the run before every worker finished its share, so it is not reproducible from --seed {}",
            cfg.seed
        );
    }
    println!("report: {}", args.out.join("FUZZ.md").display());
    Ok(written.new.is_empty())
}
