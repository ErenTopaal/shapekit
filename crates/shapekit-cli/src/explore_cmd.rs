//! `shapekit explore`: the recipe explorer.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use shapekit_core_driver::config::CoreImage;
use shapekit_explore::arms::{self, is_contract_arm};
use shapekit_explore::entries;
use shapekit_explore::episode::Env;
use shapekit_explore::explore::{self, Options, RECIPES, REPORT};
use shapekit_explore::feedback::Seen;
use shapekit_explore::verify;
use shapekit_gen::recipe::ir_run::IrRecipe;
use shapekit_schema::Schema;

use crate::error_text::chain;

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
        /// Seed of the search; the same seed and settings repeat the same episodes.
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Directory to write the recipes and the report into; must not exist.
        #[arg(long)]
        out: PathBuf,
        /// Arms counted as targets: the core-witnessed arms of this coverage report.
        #[arg(long, default_value = "corpus/coverage/coverage.json")]
        coverage_report: PathBuf,
        /// The registry whose `uncovered.toml` names the arms only network history holds,
        /// which are left out of the targets.
        #[arg(long, default_value = "registry")]
        registry: PathBuf,
        /// The pins file, read for the core image and the pinned sources.
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
        /// Pins naming the coverage twin image.
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
        /// Candidates run at once, each on its own node.
        #[arg(long, default_value_t = 3)]
        jobs: usize,
        /// Directory to write the kept pairs into; must not exist.
        #[arg(long)]
        out: PathBuf,
        /// The pins file, read for the core image and the pinned sources.
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
    },
    /// Witness sides of core's protocol gates: every recipe on the coverage twin at every
    /// protocol, beside empty reference chains. Writes `explore-gate-sides.json` (each
    /// witnessed side with its runs, counts and reference counts, and the output difference
    /// each gate's version makes), an input to the gate list; the gate list is not rewritten.
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
        /// Runs at once, each on its own node.
        #[arg(long, default_value_t = 3)]
        jobs: usize,
        /// Where runs are kept (reused on a second call) and the results are written.
        #[arg(long)]
        out: PathBuf,
        /// The pins file, read for the core image and the pinned sources.
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
        /// Pins naming the coverage twin image.
        #[arg(long, default_value = "twin/pins.toml")]
        twin_pins: PathBuf,
    },
    /// Record one recipe under both passphrases into a directory in the committed chain form
    /// (`<out>/testnet`, `<out>/mainnet`).
    Record {
        /// The recipe IR file to record.
        #[arg(long)]
        recipe: PathBuf,
        /// Directory to write the chain into; must not exist.
        #[arg(long)]
        out: PathBuf,
        /// The pins file, read for the core image and the pinned sources.
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
    },
    /// Record a pair of recipes under both passphrases into `<out>/a` and `<out>/b`, and
    /// compare the entries the last ledger of each changed (all of them, or only those of
    /// one ledger entry type) into `<out>/pair.json`.
    Pair {
        /// The recipe IR file of side `a`.
        #[arg(long)]
        a: PathBuf,
        /// The recipe IR file of side `b`.
        #[arg(long)]
        b: PathBuf,
        /// Compare only entries of this type (`trustline`, `account`, ...).
        #[arg(long)]
        only: Option<String>,
        /// Directory to write both chains and pair.json into.
        #[arg(long)]
        out: PathBuf,
        /// The pins file, read for the core image and the pinned sources.
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
    },
    /// Entries for a recorded explorer chain in a candidate lane: the fewest ledgers that
    /// together hold every union arm the chain holds, one `LedgerCloseMeta` entry each with
    /// both passphrases' bytes. Entries this command wrote for the chain before are replaced.
    Entries {
        /// The committed chain directory (holding `testnet/` and `mainnet/`); its name is the
        /// chain id.
        #[arg(long)]
        chain: PathBuf,
        /// The chain id the entries name, when it is not the directory's name (a pair side's
        /// directory is `a` or `b`).
        #[arg(long)]
        chain_id: Option<String>,
        /// The recipe the chain was recorded from, relative to the repository root.
        #[arg(long)]
        recipe: String,
        /// The family the entry ids start with.
        #[arg(long, default_value = "explore.witnesses")]
        family: String,
        /// What the entry ids start with after the family; `-l<ledger>` follows.
        #[arg(long)]
        slug: String,
        /// Where the recipe came from, in one sentence: the start of every entry's notes.
        #[arg(long)]
        about: String,
        /// Also an entry for the chain's last ledger, for a pair's compared ledger.
        #[arg(long)]
        last: bool,
        /// The run.toml beside the recipe: cover only the arms it credits to the recipe
        /// (`arms_first_witnessed` or `contract_arms_first_witnessed` of `[recipes.<file
        /// stem>]`), or every arm the chain holds when it credits none. Without it, every arm
        /// the chain holds.
        #[arg(long, conflicts_with = "arm")]
        run: Option<PathBuf>,
        /// Cover only these arms; repeatable.
        #[arg(long)]
        arm: Vec<String>,
        /// The candidate lane the entries are written into.
        #[arg(long, default_value = "candidate")]
        out: PathBuf,
    },
    /// The core-witnessed arms of the coverage report that committed chains hold: the union
    /// over every chain directory given, with the arms none of them holds.
    Witnessed {
        /// Committed chain directories (each holding `testnet/` and `mainnet/`); repeatable.
        #[arg(long = "chain", required = true)]
        chains: Vec<PathBuf>,
        /// The coverage report whose core-witnessed arms are counted.
        #[arg(long, default_value = "corpus/coverage/coverage.json")]
        coverage_report: PathBuf,
        /// The registry whose `uncovered.toml` names the arms only network history holds.
        #[arg(long, default_value = "registry")]
        registry: PathBuf,
    },
    /// Record every recipe of a run again under both passphrases, --repeat times each, and
    /// check the recorded ledgers are the same every time and the credited arms are there.
    Verify {
        /// The directory `explore run` wrote.
        #[arg(long)]
        dir: PathBuf,
        /// How many times each recipe is recorded under each passphrase.
        #[arg(long, default_value_t = 2)]
        repeat: usize,
        /// Recipes recorded at once, each on its own node.
        #[arg(long, default_value_t = 3)]
        jobs: usize,
        /// The pins file, read for the core image and the pinned sources.
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
            registry,
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
                crate::twin_cmd::twin_image(&twin_pins)?
            } else {
                release.clone()
            };
            let reachable = arms::reachable()?;
            let witnessed = arms::core_witnessed(&coverage_report)?;
            let uncovered = shapekit_coverage::uncovered::UncoveredFile::load(&registry)
                .with_context(|| format!("reading the uncovered arms of {}", registry.display()))?;
            let history_only: BTreeSet<String> = uncovered
                .history_only_arms
                .iter()
                .map(|r| r.arm.clone())
                .collect();
            let targets: BTreeSet<String> = witnessed
                .into_iter()
                .filter(|a| contracts || !is_contract_arm(a))
                .filter(|a| !history_only.contains(a))
                .collect();
            let schema = Schema::pinned();
            let root = std::env::current_dir().context("reading the working directory")?;
            let wasms = shapekit_explore::contract::load_all(&root)?;
            let scratch = scratch()?;
            let work = scratch.path();
            let seen = Seen::default();
            let env = Env {
                schema: &schema,
                image: &image,
                coverage,
                release: &release,
                root: &root,
                reachable: &reachable,
                work,
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
                history_only: history_only.into_iter().collect(),
            };
            let r = explore::run(&env, &opts)?;
            close(scratch)?;
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
            for h in &r.history_only {
                println!("left out, network history only: {h}");
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
        ExploreCommand::Pair {
            a,
            b,
            only,
            out,
            pins,
        } => pair(&a, &b, only.as_deref(), &out, &pins),
        ExploreCommand::Record { recipe, out, pins } => {
            if out.exists() {
                bail!("{} already exists", out.display());
            }
            let image = CoreImage::from_pins(&pins)
                .with_context(|| format!("reading the core image pin from {}", pins.display()))?;
            let root = std::env::current_dir()?;
            let ir = IrRecipe::load(&recipe, &root)?;
            let scratch = scratch()?;
            let work = scratch.path();
            for n in shapekit_gen::recipe::Network::BOTH {
                let target = out.join(n.name());
                let (hash, _) = verify::record_into(&ir, n, &image, work, &target)?;
                println!(
                    "{} ({}): canonical stream {hash}",
                    recipe.display(),
                    n.name()
                );
            }
            close(scratch)?;
            Ok(true)
        }
        ExploreCommand::Verify {
            dir,
            repeat,
            jobs,
            pins,
        } => verify_dir(&dir, repeat, jobs, &pins),
        ExploreCommand::Entries {
            chain,
            chain_id: id,
            recipe,
            family,
            slug,
            about,
            last,
            run,
            arm,
            out,
        } => {
            let id = match id {
                Some(id) => id,
                None => chain_id(&chain)?,
            };
            let credited = match &run {
                Some(run) => credited(run, &recipe)?,
                None if !arm.is_empty() => Some(arm.into_iter().collect()),
                None => None,
            };
            chain_entries(
                &chain,
                id,
                recipe,
                family,
                slug,
                about,
                credited.as_ref(),
                last,
                &out,
            )
        }
        ExploreCommand::Witnessed {
            chains,
            coverage_report,
            registry,
        } => witnessed(&chains, &coverage_report, &registry),
    }
}

fn chain_id(chain: &Path) -> Result<String> {
    Ok(chain
        .file_name()
        .and_then(|n| n.to_str())
        .with_context(|| format!("{} has no directory name", chain.display()))?
        .to_owned())
}

#[allow(clippy::too_many_arguments)]
fn chain_entries(
    chain: &Path,
    id: String,
    recipe: String,
    family: String,
    slug: String,
    about: String,
    credited: Option<&BTreeSet<String>>,
    last: bool,
    out: &Path,
) -> Result<bool> {
    let reachable = arms::reachable()?;
    let ledgers = entries::read_chain(chain, &reachable)?;
    let src = entries::ChainSource {
        chain_id: id,
        recipe,
        family,
        slug,
        about,
    };
    // the cover first: a chain that cannot be covered keeps the entries it has
    let picks = entries::cover(&ledgers, credited, last)?;
    let removed = entries::remove_chain_entries(out, &src.family, &src.chain_id)?;
    let held: BTreeSet<&String> = ledgers.iter().flat_map(|l| &l.arms).collect();
    for p in &picks {
        let e = entries::entry(&src, p);
        let id = e.id.clone();
        entries::write(out, e, p)?;
        println!("{id}: {} arms", p.credited.len());
    }
    println!(
        "{}: {} ledgers hold {} arms; {} entries written, {} replaced",
        src.chain_id,
        ledgers.len(),
        held.len(),
        picks.len(),
        removed
    );
    Ok(true)
}

/// The arms `run` (a run.toml) credits to the recipe file `recipe`, `None` when it credits
/// none.
fn credited(run: &Path, recipe: &str) -> Result<Option<BTreeSet<String>>> {
    let text =
        std::fs::read_to_string(run).with_context(|| format!("reading {}", run.display()))?;
    let doc: toml::Value =
        toml::from_str(&text).with_context(|| format!("parsing {}", run.display()))?;
    let key = Path::new(recipe)
        .file_stem()
        .and_then(|s| s.to_str())
        .with_context(|| format!("{recipe} has no file name"))?;
    let table = doc
        .get("recipes")
        .and_then(|r| r.get(key))
        .with_context(|| format!("{} has no [recipes.{key}]", run.display()))?;
    let mut arms = BTreeSet::new();
    for field in ["arms_first_witnessed", "contract_arms_first_witnessed"] {
        if let Some(list) = table.get(field) {
            let list = list.as_array().with_context(|| {
                format!("{}: recipes.{key}.{field} is not a list", run.display())
            })?;
            for a in list {
                arms.insert(
                    a.as_str()
                        .with_context(|| format!("{}: recipes.{key}.{field}", run.display()))?
                        .to_owned(),
                );
            }
        }
    }
    Ok((!arms.is_empty()).then_some(arms))
}

fn witnessed(chains: &[PathBuf], coverage_report: &Path, registry: &Path) -> Result<bool> {
    let reachable = arms::reachable()?;
    let targets = arms::core_witnessed(coverage_report)?;
    let uncovered = shapekit_coverage::uncovered::UncoveredFile::load(registry)
        .with_context(|| format!("reading the uncovered arms of {}", registry.display()))?;
    let history_only: BTreeSet<String> = uncovered
        .history_only_arms
        .iter()
        .map(|r| r.arm.clone())
        .collect();
    let mut held = BTreeSet::new();
    for c in chains {
        for l in entries::read_chain(c, &reachable)? {
            held.extend(l.arms);
        }
    }
    let hit = targets.intersection(&held).count();
    let classic = targets.iter().filter(|a| !is_contract_arm(a)).count();
    let classic_hit = targets
        .iter()
        .filter(|a| !is_contract_arm(a) && held.contains(*a))
        .count();
    println!(
        "{} chains hold {hit} of the {} core-witnessed arms of {} ({classic_hit} of {classic} classic, {} of {} contract)",
        chains.len(),
        targets.len(),
        coverage_report.display(),
        hit - classic_hit,
        targets.len() - classic
    );
    for a in targets.difference(&held) {
        let why = if history_only.contains(a) {
            " (network history only)"
        } else {
            ""
        };
        println!("not held: {a}{why}");
    }
    let beyond: Vec<&String> = held.difference(&targets).collect();
    println!(
        "{} arms beyond the core-witnessed ones: {}",
        beyond.len(),
        beyond
            .iter()
            .map(|a| a.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(true)
}

/// A scratch directory for this call alone, under the temporary directory: concurrent calls
/// never share one, and it is removed when the returned value is dropped or closed.
fn scratch() -> Result<tempfile::TempDir> {
    tempfile::Builder::new()
        .prefix("shapekit-explore-")
        .tempdir()
        .context("creating a scratch directory")
}

/// Removes a scratch directory, reporting a failure rather than leaving it behind unseen.
fn close(scratch: tempfile::TempDir) -> Result<()> {
    let path = scratch.path().display().to_string();
    scratch
        .close()
        .with_context(|| format!("removing the scratch directory {path}"))
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
    let checks = verify::recipes_to_check(dir)?;
    let names: Vec<&String> = checks.keys().collect();
    let root = std::env::current_dir()?;
    let reachable = arms::reachable()?;
    let scratch = scratch()?;
    let work = scratch.path();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|s| -> Result<()> {
        for _ in 0..jobs.clamp(1, names.len().max(1)) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let Some(name) = names.get(i) else { break };
                    let credited = &checks[name.as_str()];
                    let checked = match IrRecipe::load(
                        &dir.join(RECIPES).join(format!("{name}.json")),
                        &root,
                    ) {
                        Ok(recipe) => {
                            verify::check(name, &recipe, credited, &image, &reachable, work, repeat)
                        }
                        Err(e) => verify::Checked {
                            recipe: name.to_string(),
                            runs: Vec::new(),
                            problems: vec![chain(e.as_ref())],
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
    close(scratch)?;
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
    let mut files: Vec<PathBuf> = Vec::new();
    for e in std::fs::read_dir(recipes).with_context(|| format!("reading {}", recipes.display()))? {
        let p = e
            .with_context(|| format!("reading {}", recipes.display()))?
            .path();
        if p.extension().is_some_and(|x| x == "json") {
            files.push(p);
        }
    }
    files.sort();
    for f in files {
        let name = f
            .file_stem()
            .and_then(|n| n.to_str())
            .with_context(|| format!("{} has no UTF-8 file name", f.display()))?
            .to_owned();
        irs.push((name, shapekit_gen::recipe::ir::load(&f)?));
    }
    let cands = pairs::candidates(&irs);
    let total = cands.len();
    // one candidate per recipe and rewrite first, so a small budget still sees every kind
    let mut cands = pairs::firsts_first(cands, |c| (c.from.clone(), c.rewrite as u8));
    cands.truncate(max);
    let schema = Schema::pinned();
    let reachable = arms::reachable()?;
    let scratch = scratch()?;
    let work = scratch.path();
    let seenf = Seen::default();
    let env = Env {
        schema: &schema,
        image: &image,
        coverage: false,
        release: &image,
        root: &root,
        reachable: &reachable,
        work,
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
                        Err(e) => eprintln!("{name}: {}", chain(e.as_ref())),
                    }
                    results
                        .lock()
                        .expect("no thread panics holding the lock")
                        .push(r.map_err(|e| format!("{name}: {}", chain(e.as_ref()))));
                }
            });
        }
    });
    close(scratch)?;
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

/// `<directory>.<file stem>`: how `explore gates` names a recipe file.
fn recipe_name(f: &Path) -> Result<String> {
    let dir = f
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .with_context(|| format!("{} has no UTF-8 directory name", f.display()))?;
    let stem = f
        .file_stem()
        .and_then(|n| n.to_str())
        .with_context(|| format!("{} has no UTF-8 file name", f.display()))?;
    Ok(format!("{dir}.{stem}"))
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
    let twin = crate::twin_cmd::twin_image(twin_pins)?;
    let mut files = Vec::new();
    for d in dirs {
        recipe_files(d, &mut files)?;
    }
    files.sort();
    let mut recipes: Vec<(String, shapekit_contracts::RecipeIr, String)> = Vec::new();
    let mut named: std::collections::BTreeMap<String, &PathBuf> = Default::default();
    for f in &files {
        let ir = shapekit_gen::recipe::ir::load(f)?;
        let name = recipe_name(f)?;
        // runs, witnesses and the sides file all name a recipe by this alone
        if let Some(other) = named.insert(name.clone(), f) {
            bail!(
                "{} and {} are both named {name}; recipes of one call need distinct <directory>.<file stem> names",
                other.display(),
                f.display()
            );
        }
        let hash = gates::recipe_hash(&ir)?;
        recipes.push((name, ir, hash));
    }
    let current: BTreeSet<(&str, &str)> = recipes
        .iter()
        .map(|(n, _, h)| (n.as_str(), h.as_str()))
        .collect();
    let root = std::env::current_dir()?;
    let runs_dir = gates::runs_dir(out, &twin)?;
    std::fs::create_dir_all(&runs_dir)
        .with_context(|| format!("creating {}", runs_dir.display()))?;
    let done: BTreeSet<String> = gates::load_all(&runs_dir)?
        .iter()
        .map(|r| r.key())
        .collect();
    // genesis and padded references at every protocol, then each recipe at every protocol
    let mut todo: Vec<(RunKind, Option<usize>, u32)> = Vec::new();
    for &p in &protocols {
        todo.push((RunKind::Genesis, None, p));
        todo.push((RunKind::Padded, None, p));
        for (i, (_, ir, _)) in recipes.iter().enumerate() {
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
    let key_of = |t: &(RunKind, Option<usize>, u32)| {
        gates::run_key(&name_of(t), t.1.map(|i| recipes[i].2.as_str()))
    };
    todo.retain(|t| !done.contains(&key_of(t)));
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
    let scratch = scratch()?;
    let work = scratch.path();
    let seen = Seen::default();
    let env = Env {
        schema: &schema,
        image: &twin,
        coverage: true,
        release: &release,
        root: &root,
        reachable: &reachable,
        work,
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
                                recipe_sha256: None,
                                protocol: *p,
                                counts,
                                answers: Vec::new(),
                            })
                        }
                        (RunKind::Padded, _, p) => gates::record(&env, None, *p),
                        (_, Some(r), p) => {
                            let (n, ir, _) = &recipes[*r];
                            gates::record(&env, Some((n, ir)), *p)
                        }
                        _ => unreachable!("a recipe run names its recipe"),
                    };
                    match run.and_then(|r| gates::save(&r, &runs_dir)) {
                        Ok(()) => eprintln!("{name}: {:.1}s", started.elapsed().as_secs_f64()),
                        Err(e) => {
                            eprintln!("{name}: {}", chain(e.as_ref()));
                            failures
                                .lock()
                                .expect("no thread panics holding the lock")
                                .push(format!("{name}: {}", chain(e.as_ref())));
                        }
                    }
                }
            });
        }
    });
    close(scratch)?;
    // this call's runs: the references and exactly these recipes at exactly these protocols.
    // runs of other recipes, of an earlier version of one or at other protocols stay saved
    // for the call that asks for them, but are not what this call's output describes
    let runs: Vec<gates::GateRun> = gates::load_all(&runs_dir)?
        .into_iter()
        .filter(|r| protocols.contains(&r.protocol))
        .filter(|r| match (&r.recipe, &r.recipe_sha256) {
            (Some(n), Some(h)) => current.contains(&(n.as_str(), h.as_str())),
            _ => r.kind != RunKind::Recipe,
        })
        .collect();
    // a reference run that failed or was never recorded leaves its protocol's recipe runs
    // with nothing to subtract, so nothing is written until every reference is there
    gates::references(&runs, &protocols).with_context(|| {
        format!(
            "not writing {}; rerun to record the missing references",
            out.join("explore-gate-sides.json").display()
        )
    })?;
    let sides = gates::side_witnesses(&list, &runs)?;
    // the counts below describe what these runs alone show; the gate list itself is written
    // by `twin gates` and its owner, which take this file as one input
    let (summary, diffs) = gates::statuses(&mut list, &runs)?;
    let file = gates::GateSides {
        schema_version: 1,
        core: list.core.clone(),
        twin_image: twin.reference(),
        protocols: protocols.clone(),
        recipes: recipes
            .iter()
            .zip(&files)
            .map(|((n, ir, _), f)| gates::RecipeRef {
                name: n.clone(),
                id: ir.id.clone(),
                protocol: ir.protocol,
                path: f.display().to_string(),
            })
            .collect(),
        sides,
        differences: diffs,
    };
    std::fs::write(
        out.join("explore-gate-sides.json"),
        serde_json::to_string_pretty(&file)? + "\n",
    )?;
    std::fs::write(
        out.join("summary.json"),
        serde_json::to_string_pretty(&summary)? + "\n",
    )?;
    println!(
        "{} gates, {} sides; these runs alone: {} sides witnessed, {} no protocol takes, {} not shown ({} of them at calls the coverage mapping has no region for); {} gates show an output difference between N-1 and N; sides written to {}",
        summary.gates,
        summary.sides,
        summary.met,
        summary.unmeetable,
        summary.open,
        summary.open_unmeasured,
        summary.gates_with_difference,
        out.join("explore-gate-sides.json").display()
    );
    let failures = failures
        .into_inner()
        .expect("no thread panics holding the lock");
    for f in &failures {
        println!("run failed: {f}");
    }
    Ok(failures.is_empty())
}

fn pair(a: &Path, b: &Path, only: Option<&str>, out: &Path, pins: &Path) -> Result<bool> {
    use shapekit_explore::pairs::{compare, post_state};
    use shapekit_gen::recipe::Network;
    if out.exists() {
        bail!("{} already exists", out.display());
    }
    let image = CoreImage::from_pins(pins)
        .with_context(|| format!("reading the core image pin from {}", pins.display()))?;
    let root = std::env::current_dir()?;
    let scratch = scratch()?;
    let work = scratch.path();
    let mut states = Vec::new();
    for (name, file) in [("a", a), ("b", b)] {
        let ir = IrRecipe::load(file, &root)?;
        std::fs::create_dir_all(out.join(name))?;
        std::fs::copy(file, out.join(format!("{name}.json")))?;
        let mut last = None;
        for n in Network::BOTH {
            let (hash, lcm) =
                verify::record_into(&ir, n, &image, work, &out.join(name).join(n.name()))?;
            println!("{name} ({}): canonical stream {hash}", n.name());
            if n == Network::Testnet {
                last = Some(lcm);
            }
        }
        let mut state = post_state(&last.expect("testnet ran"))?;
        if let Some(kind) = only {
            state.retain(|k, _| {
                use stellar_xdr::{Limits, ReadXdr};
                stellar_xdr::LedgerKey::from_xdr(k, Limits::none())
                    .ok()
                    .and_then(|k| serde_json::to_value(k).ok())
                    .is_some_and(|v| v.get(kind).is_some())
            });
        }
        states.push(state);
    }
    close(scratch)?;
    let rows = compare(&states[0], &states[1])?;
    let equal = !rows.is_empty() && rows.iter().all(|r| r.equal);
    let v = serde_json::json!({
        "a": "a.json",
        "b": "b.json",
        "compared": only.unwrap_or("every entry the last ledger changed"),
        "records_equal": equal,
        "entries": rows,
    });
    std::fs::write(
        out.join("pair.json"),
        serde_json::to_string_pretty(&v)? + "\n",
    )?;
    println!(
        "{} entries compared on the testnet chains' last ledgers: {}",
        rows.len(),
        if equal {
            "all byte for byte equal"
        } else {
            "they differ"
        }
    );
    Ok(equal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rerun_that_cannot_cover_keeps_the_chains_entries() {
        let chain = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/chains/cap01.bump-sequence.p30");
        let lane = tempfile::tempdir().unwrap();
        let run = |credited: Option<&BTreeSet<String>>| {
            chain_entries(
                &chain,
                "cap01.bump-sequence.p30".into(),
                "corpus/recipes/cap01.bump-sequence/bump-sequence.json".into(),
                "explore.witnesses".into(),
                "t".into(),
                "A test.".into(),
                credited,
                false,
                lane.path(),
            )
        };
        assert!(run(None).unwrap());
        let family = lane.path().join("entries/explore.witnesses");
        let before = std::fs::read_dir(&family).unwrap().count();
        assert!(before > 0);
        // an arm no ledger of the chain holds: the cover fails, and nothing is removed
        let missing = BTreeSet::from(["NoSuchType::NoSuchArm".to_owned()]);
        assert!(run(Some(&missing)).is_err());
        assert_eq!(std::fs::read_dir(&family).unwrap().count(), before);
    }

    #[test]
    fn recipe_names_are_directory_and_stem() {
        assert_eq!(
            recipe_name(Path::new("candidate/recipes/g2/r0003.json")).unwrap(),
            "g2.r0003"
        );
        assert!(recipe_name(Path::new("r0003.json")).is_err());
    }
}
