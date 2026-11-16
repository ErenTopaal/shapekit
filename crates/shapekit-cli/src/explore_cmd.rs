//! `shapekit explore`: the recipe explorer.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use shapekit_core_driver::config::CoreImage;
use shapekit_explore::arms::{self, is_contract_arm};
use shapekit_explore::episode::Env;
use shapekit_explore::explore::{self, Options, RECIPES, REPORT};
use shapekit_explore::feedback::Seen;
use shapekit_explore::verify;
use shapekit_gen::recipe::ir_run::IrRecipe;
use shapekit_schema::Schema;

#[derive(Subcommand)]
pub enum ExploreCommand {
    /// Search for recipes for a time budget and write them, with a report of the union arms
    /// they witness, into a new directory.
    Run {
        /// Minutes to search.
        #[arg(long)]
        budget: u64,
        /// Protocols episodes start at: one number, a comma list or a range like 9-30.
        #[arg(long, default_value = "30")]
        protocol: String,
        /// Highest protocol a ledger upgrade may move an episode to.
        #[arg(long, default_value_t = 30)]
        max_protocol: u32,
        /// Smart contract operations as well as classic ones.
        #[arg(long)]
        contracts: bool,
        /// Run on the coverage twin and feed core's code regions back into the search.
        #[arg(long)]
        coverage: bool,
        /// Episodes at once, each on its own node.
        #[arg(long, default_value_t = 3)]
        jobs: usize,
        /// New steps per episode.
        #[arg(long, default_value_t = 40)]
        actions: usize,
        #[arg(long, default_value_t = 1)]
        seed: u64,
        #[arg(long)]
        out: PathBuf,
        /// Arms counted as targets: the core-witnessed arms of this coverage report.
        #[arg(long, default_value = "corpus/coverage/coverage.json")]
        coverage_report: PathBuf,
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
        #[arg(long, default_value = "twin/pins.toml")]
        twin_pins: PathBuf,
    },
    /// Record every recipe of a run again under both passphrases, twice each, and check the
    /// streams repeat and the credited arms are there.
    Verify {
        /// The directory `explore run` wrote.
        #[arg(long)]
        dir: PathBuf,
        #[arg(long, default_value_t = 2)]
        repeat: usize,
        #[arg(long, default_value_t = 3)]
        jobs: usize,
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
    },
}

pub fn run(command: ExploreCommand) -> Result<bool> {
    match command {
        ExploreCommand::Run {
            budget,
            protocol,
            max_protocol,
            contracts,
            coverage,
            jobs,
            actions,
            seed,
            out,
            coverage_report,
            pins,
            twin_pins,
        } => {
            if out.exists() {
                bail!("{} already exists", out.display());
            }
            let protocols = parse_protocols(&protocol)?;
            let image_pins = if coverage { &twin_pins } else { &pins };
            let image = CoreImage::from_pins(image_pins).with_context(|| {
                format!("reading the core image pin from {}", image_pins.display())
            })?;
            let reachable = arms::reachable()?;
            let witnessed = arms::core_witnessed(&coverage_report)?;
            let targets: BTreeSet<String> = witnessed
                .into_iter()
                .filter(|a| contracts || !is_contract_arm(a))
                .collect();
            let schema = Schema::pinned();
            let root = std::env::current_dir().context("reading the working directory")?;
            let work = scratch()?;
            let seen = Seen::default();
            let env = Env {
                schema: &schema,
                image: &image,
                coverage,
                root: &root,
                reachable: &reachable,
                work: &work,
                seen: &seen,
            };
            let opts = Options {
                budget: Duration::from_secs(budget * 60),
                protocols,
                max_protocol,
                jobs,
                actions,
                seed,
                contracts,
                out: out.clone(),
                targets,
            };
            let r = explore::run(&env, &opts)?;
            println!(
                "{} episodes ({} replays) in {:.0}s: {} recipes, {} of {} target arms witnessed ({:.1}%), {} more arms beyond the targets",
                r.episodes,
                r.replays,
                r.seconds,
                r.recipes,
                r.targets_witnessed,
                r.targets,
                100.0 * r.targets_witnessed as f64 / r.targets.max(1) as f64,
                r.beyond_targets.len()
            );
            println!(
                "transactions: {} applied successfully, {} applied and failed, {} refused at submission, {} invalid fills; {} upgrades refused",
                r.applied, r.failed, r.rejected, r.invalid_fills, r.upgrades_refused
            );
            println!(
                "feedback: {} signature paths, {} core regions; shrinking kept {} and gave up on {}",
                r.signature_paths, r.core_regions, r.shrunk, r.not_shrunk
            );
            for m in &r.missing {
                println!("missing: {m}");
            }
            for e in &r.errors {
                println!("error: {e}");
            }
            println!("report: {}", out.join(REPORT).display());
            Ok(r.errors.is_empty())
        }
        ExploreCommand::Verify {
            dir,
            repeat,
            jobs,
            pins,
        } => verify_dir(&dir, repeat, jobs, &pins),
    }
}

fn scratch() -> Result<PathBuf> {
    let dir = std::env::temp_dir().join("shapekit-explore");
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    Ok(dir)
}

/// `30`, `9,12,30` or `9-30`.
pub fn parse_protocols(s: &str) -> Result<Vec<u32>> {
    let mut out = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if let Some((a, b)) = part.split_once('-') {
            let (a, b): (u32, u32) = (a.parse()?, b.parse()?);
            if a > b {
                bail!("protocol range {part} runs backwards");
            }
            out.extend(a..=b);
        } else {
            out.push(part.parse().with_context(|| format!("protocol {part:?}"))?);
        }
    }
    if out.is_empty() || out.iter().any(|p| *p == 0 || *p > 30) {
        bail!("protocols must be 1 to 30, got {s:?}");
    }
    Ok(out)
}

fn verify_dir(dir: &Path, repeat: usize, jobs: usize, pins: &Path) -> Result<bool> {
    let image = CoreImage::from_pins(pins)
        .with_context(|| format!("reading the core image pin from {}", pins.display()))?;
    let report: serde_json::Value = serde_json::from_slice(
        &std::fs::read(dir.join(REPORT))
            .with_context(|| format!("reading {}", dir.join(REPORT).display()))?,
    )?;
    let witnesses = report["witnesses"]
        .as_object()
        .with_context(|| format!("{} has no witnesses", dir.join(REPORT).display()))?;
    let root = std::env::current_dir()?;
    let reachable = arms::reachable()?;
    let work = scratch()?;
    let mut names: Vec<String> = Vec::new();
    for e in std::fs::read_dir(dir.join(RECIPES))? {
        let p = e?.path();
        if p.extension().is_some_and(|x| x == "json") {
            names.push(
                p.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    names.sort();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|s| -> Result<()> {
        for _ in 0..jobs.clamp(1, names.len().max(1)) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let Some(name) = names.get(i) else { break };
                    let credited: BTreeSet<String> = witnesses
                        .iter()
                        .filter(|(_, w)| w["recipe"].as_str() == Some(name.as_str()))
                        .map(|(a, _)| a.clone())
                        .collect();
                    let checked = match IrRecipe::load(
                        &dir.join(RECIPES).join(format!("{name}.json")),
                        &root,
                    ) {
                        Ok(recipe) => verify::check(
                            name, &recipe, &credited, &image, &reachable, &work, repeat,
                        ),
                        Err(e) => verify::Checked {
                            recipe: name.clone(),
                            runs: Vec::new(),
                            problems: vec![format!("{e:#}")],
                        },
                    };
                    eprintln!(
                        "{}: {} recordings, {} credited arms, {}",
                        name,
                        checked.runs.len(),
                        credited.len(),
                        if checked.problems.is_empty() {
                            "ok".to_owned()
                        } else {
                            checked.problems.join("; ")
                        }
                    );
                    results
                        .lock()
                        .expect("no thread panics holding the lock")
                        .push(checked);
                }
            });
        }
        Ok(())
    })?;
    let mut results = results
        .into_inner()
        .expect("no thread panics holding the lock");
    results.sort_by(|a, b| a.recipe.cmp(&b.recipe));
    let bad: Vec<&verify::Checked> = results.iter().filter(|c| !c.problems.is_empty()).collect();
    let out = dir.join("verify.json");
    std::fs::write(&out, serde_json::to_string_pretty(&results)? + "\n")?;
    println!(
        "{} recipes, {} recordings each ({} per passphrase): {} reproduce byte for byte under both passphrases with their credited arms, {} do not",
        results.len(),
        2 * repeat,
        repeat,
        results.len() - bad.len(),
        bad.len()
    );
    for c in &bad {
        for p in &c.problems {
            println!("{}: {p}", c.recipe);
        }
    }
    println!("details: {}", out.display());
    Ok(bad.is_empty())
}
