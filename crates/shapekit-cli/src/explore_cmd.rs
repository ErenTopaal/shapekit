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
    /// Pairs of recipes from one start state whose transactions differ only in how they name
    /// the same account, kept when core leaves every changed ledger entry byte for byte the
    /// same under both.
    Pairs {
        /// A directory of recipe IR files to start from (an `explore run` output's recipes).
        #[arg(long)]
        recipes: PathBuf,
        /// At most this many candidates are run.
        #[arg(long, default_value_t = 40)]
        max: usize,
        #[arg(long, default_value_t = 3)]
        jobs: usize,
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
    },
    /// Witness both sides of core's protocol gates: every recipe on the coverage twin at
    /// every protocol, beside empty reference chains, and write the gate list with each
    /// side's status and the output difference each gate's version makes.
    Gates {
        /// The gate list `shapekit twin gates` wrote, all sides open.
        #[arg(long)]
        gates: PathBuf,
        /// Directories of recipe IR files, searched recursively; repeatable.
        #[arg(long)]
        recipes: Vec<PathBuf>,
        /// Protocols to run at: one number, a comma list or a range.
        #[arg(long, default_value = "1-30")]
        protocol: String,
        #[arg(long, default_value_t = 3)]
        jobs: usize,
        /// Where runs are kept (reused on a second call) and the results are written.
        #[arg(long)]
        out: PathBuf,
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
            let release = CoreImage::from_pins(&pins)
                .with_context(|| format!("reading the core image pin from {}", pins.display()))?;
            let image = if coverage {
                CoreImage::from_pins(&twin_pins).with_context(|| {
                    format!("reading the twin image pin from {}", twin_pins.display())
                })?
            } else {
                release.clone()
            };
            let reachable = arms::reachable()?;
            let witnessed = arms::core_witnessed(&coverage_report)?;
            let targets: BTreeSet<String> = witnessed
                .into_iter()
                .filter(|a| contracts || !is_contract_arm(a))
                .collect();
            let schema = Schema::pinned();
            let root = std::env::current_dir().context("reading the working directory")?;
            let wasms = shapekit_explore::contract::load_all(&root)?;
            let work = scratch()?;
            let seen = Seen::default();
            let env = Env {
                schema: &schema,
                image: &image,
                coverage,
                release: &release,
                root: &root,
                reachable: &reachable,
                work: &work,
                seen: &seen,
                contracts: &wasms,
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
        ExploreCommand::Pairs {
            recipes,
            max,
            jobs,
            out,
            pins,
        } => pairs(&recipes, max, jobs, &out, &pins),
        ExploreCommand::Gates {
            gates,
            recipes,
            protocol,
            jobs,
            out,
            pins,
            twin_pins,
        } => gates_cmd(&gates, &recipes, &protocol, jobs, &out, &pins, &twin_pins),
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

fn pairs(recipes: &Path, max: usize, jobs: usize, out: &Path, pins: &Path) -> Result<bool> {
    use shapekit_explore::pairs;
    if out.exists() {
        bail!("{} already exists", out.display());
    }
    let image = CoreImage::from_pins(pins)
        .with_context(|| format!("reading the core image pin from {}", pins.display()))?;
    let root = std::env::current_dir()?;
    let wasms = shapekit_explore::contract::load_all(&root)?;
    let mut irs = Vec::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(recipes)
        .with_context(|| format!("reading {}", recipes.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    for f in files {
        let name = f
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        irs.push((name, shapekit_gen::recipe::ir::load(&f)?));
    }
    let mut cands = pairs::candidates(&irs);
    let total = cands.len();
    // one candidate per recipe and rewrite first, so a small budget still sees every kind
    let mut seen = BTreeSet::new();
    cands.sort_by_key(|c| !seen.insert((c.from.clone(), c.rewrite as u8)));
    cands.truncate(max);
    let schema = Schema::pinned();
    let reachable = arms::reachable()?;
    let work = scratch()?;
    let seenf = Seen::default();
    let env = Env {
        schema: &schema,
        image: &image,
        coverage: false,
        release: &image,
        root: &root,
        reachable: &reachable,
        work: &work,
        seen: &seenf,
        contracts: &wasms,
    };
    std::fs::create_dir_all(out.join("pairs"))?;
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..jobs.clamp(1, cands.len().max(1)) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let Some(c) = cands.get(i) else { break };
                    let name = format!("p{i:04}");
                    let r = pairs::run_candidate(&env, c, &name).and_then(|(rep, a, b)| {
                        if rep.records_equal {
                            let dir = out.join("pairs").join(&name);
                            std::fs::create_dir_all(&dir)?;
                            std::fs::write(
                                dir.join("a.json"),
                                serde_json::to_string_pretty(&a)? + "\n",
                            )?;
                            std::fs::write(
                                dir.join("b.json"),
                                serde_json::to_string_pretty(&b)? + "\n",
                            )?;
                        }
                        Ok(rep)
                    });
                    match &r {
                        Ok(rep) => eprintln!(
                            "{name}: {} step {} {:?}: {}",
                            rep.from,
                            rep.step,
                            rep.rewrite,
                            if rep.records_equal {
                                "same records"
                            } else if rep.both_applied {
                                "records differ"
                            } else {
                                "not applied under both"
                            }
                        ),
                        Err(e) => eprintln!("{name}: {e:#}"),
                    }
                    results
                        .lock()
                        .expect("no thread panics holding the lock")
                        .push(r.map_err(|e| format!("{name}: {e:#}")));
                }
            });
        }
    });
    let results = results
        .into_inner()
        .expect("no thread panics holding the lock");
    let mut reports: Vec<pairs::PairReport> = Vec::new();
    let mut errors = Vec::new();
    for r in results {
        match r {
            Ok(rep) => reports.push(rep),
            Err(e) => errors.push(e),
        }
    }
    reports.sort_by(|a, b| a.pair.cmp(&b.pair));
    std::fs::write(
        out.join("pairs.json"),
        serde_json::to_string_pretty(&reports)? + "\n",
    )?;
    let equal = reports.iter().filter(|r| r.records_equal).count();
    let differ = reports
        .iter()
        .filter(|r| r.both_applied && !r.records_equal)
        .count();
    println!(
        "{total} candidates, {} run: {equal} pairs with the same records, {differ} whose records differ, {} not applied under both, {} errors",
        cands.len(),
        reports.len() - equal - differ,
        errors.len()
    );
    for e in &errors {
        println!("error: {e}");
    }
    println!("pairs: {}", out.join("pairs.json").display());
    Ok(errors.is_empty())
}

fn recipe_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for e in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let p = e?.path();
        if p.is_dir() {
            recipe_files(&p, out)?;
        } else if p.extension().is_some_and(|x| x == "json") {
            out.push(p);
        }
    }
    Ok(())
}

fn gates_cmd(
    gates_path: &Path,
    dirs: &[PathBuf],
    protocol: &str,
    jobs: usize,
    out: &Path,
    pins: &Path,
    twin_pins: &Path,
) -> Result<bool> {
    use shapekit_contracts::gates::GateList;
    use shapekit_explore::gates::{self, RunKind};

    let mut list: GateList =
        shapekit_contracts::load(gates_path).map_err(|e| anyhow::anyhow!("{e}"))?;
    let protocols = parse_protocols(protocol)?;
    let release = CoreImage::from_pins(pins)
        .with_context(|| format!("reading the core image pin from {}", pins.display()))?;
    let twin = CoreImage::from_pins(twin_pins)
        .with_context(|| format!("reading the twin image pin from {}", twin_pins.display()))?;
    let mut files = Vec::new();
    for d in dirs {
        recipe_files(d, &mut files)?;
    }
    files.sort();
    let mut recipes = Vec::new();
    for f in &files {
        let ir = shapekit_gen::recipe::ir::load(f)?;
        let name = format!(
            "{}.{}",
            f.parent()
                .and_then(|p| p.file_name())
                .unwrap_or_default()
                .to_string_lossy(),
            f.file_stem().unwrap_or_default().to_string_lossy()
        );
        recipes.push((name, ir));
    }
    let root = std::env::current_dir()?;
    let runs_dir = out.join("runs");
    std::fs::create_dir_all(&runs_dir)?;
    let done: BTreeSet<String> = gates::load_all(&runs_dir)?
        .iter()
        .map(|r| r.name())
        .collect();
    // genesis and padded references at every protocol, then each recipe at every protocol
    let mut todo: Vec<(RunKind, Option<usize>, u32)> = Vec::new();
    for &p in &protocols {
        todo.push((RunKind::Genesis, None, p));
        todo.push((RunKind::Padded, None, p));
        for (i, (_, ir)) in recipes.iter().enumerate() {
            // a recipe that uploads code runs only where the host accepts that code
            if p >= gates::lowest_protocol(ir, &root)? {
                todo.push((RunKind::Recipe, Some(i), p));
            }
        }
    }
    let name_of = |(k, i, p): &(RunKind, Option<usize>, u32)| match (k, i) {
        (RunKind::Genesis, _) => format!("genesis@p{p}"),
        (RunKind::Padded, _) => format!("empty-ledgers@p{p}"),
        (_, Some(i)) => format!("{}@p{p}", recipes[*i].0),
        _ => unreachable!("a recipe run names its recipe"),
    };
    todo.retain(|t| !done.contains(&name_of(t)));
    println!(
        "{} recipes at {} protocols: {} runs recorded before, {} to record",
        recipes.len(),
        protocols.len(),
        done.len(),
        todo.len()
    );
    let schema = Schema::pinned();
    let reachable = arms::reachable()?;
    let wasms = shapekit_explore::contract::load_all(&root)?;
    let work = scratch()?;
    let seen = Seen::default();
    let env = Env {
        schema: &schema,
        image: &twin,
        coverage: true,
        release: &release,
        root: &root,
        reachable: &reachable,
        work: &work,
        seen: &seen,
        contracts: &wasms,
    };
    let next = std::sync::atomic::AtomicUsize::new(0);
    let failures = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..jobs.clamp(1, todo.len().max(1)) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let Some(t) = todo.get(i) else { break };
                    let name = name_of(t);
                    let started = std::time::Instant::now();
                    let run = match t {
                        (RunKind::Genesis, _, p) => {
                            gates::genesis(&env, *p).map(|counts| gates::GateRun {
                                kind: RunKind::Genesis,
                                recipe: None,
                                protocol: *p,
                                counts,
                                answers: Vec::new(),
                            })
                        }
                        (RunKind::Padded, _, p) => gates::record(&env, None, *p),
                        (_, Some(r), p) => {
                            let (n, ir) = &recipes[*r];
                            gates::record(&env, Some((n, ir)), *p)
                        }
                        _ => unreachable!("a recipe run names its recipe"),
                    };
                    match run.and_then(|r| gates::save(&r, &runs_dir)) {
                        Ok(()) => eprintln!("{name}: {:.1}s", started.elapsed().as_secs_f64()),
                        Err(e) => {
                            eprintln!("{name}: {e:#}");
                            failures
                                .lock()
                                .expect("no thread panics holding the lock")
                                .push(format!("{name}: {e:#}"));
                        }
                    }
                }
            });
        }
    });
    let runs = gates::load_all(&runs_dir)?;
    let (summary, diffs) = gates::statuses(&mut list, &runs);
    std::fs::write(
        out.join("gates.json"),
        serde_json::to_string_pretty(&list)? + "\n",
    )?;
    std::fs::write(
        out.join("differences.json"),
        serde_json::to_string_pretty(&diffs)? + "\n",
    )?;
    std::fs::write(
        out.join("summary.json"),
        serde_json::to_string_pretty(&summary)? + "\n",
    )?;
    println!(
        "{} gates, {} sides: {} met, {} unmeetable, {} open ({} of them at calls the coverage mapping has no region for); {} gates show an output difference between N-1 and N",
        summary.gates,
        summary.sides,
        summary.met,
        summary.unmeetable,
        summary.open,
        summary.open_unmeasured,
        summary.gates_with_difference
    );
    let failures = failures
        .into_inner()
        .expect("no thread panics holding the lock");
    for f in &failures {
        println!("run failed: {f}");
    }
    Ok(failures.is_empty())
}
