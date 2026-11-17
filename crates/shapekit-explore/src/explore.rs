//! The search: workers run episodes until the time budget is spent. An episode whose
//! ledgers hold an arm no recipe of the run witnessed yet is shrunk to the steps that
//! showed something new, replayed to confirm the arms survive, and written out as a recipe.
//! Episodes that showed new signature paths or, on the twin, new core regions are kept as
//! seeds: later episodes replay one and continue from its end state.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::Serialize;
use shapekit_contracts::RecipeIr;
use shapekit_contracts::recipe_ir::NodeOptions;

use crate::action::OpMenu;
use crate::episode::{self, Env, Episode, Spec};
use crate::feedback::RegionKey;
use crate::rng::Rng;

pub struct Options {
    pub budget: Duration,
    /// Protocols episodes start at, drawn uniformly.
    pub protocols: Vec<u32>,
    pub max_protocol: u32,
    pub jobs: usize,
    /// New steps per episode.
    pub actions: usize,
    pub seed: u64,
    pub contracts: bool,
    pub out: PathBuf,
    /// The arms the run is measured against.
    pub targets: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Witness {
    pub recipe: String,
    pub ledger: u32,
}

struct Seed {
    ir: RecipeIr,
}

struct State {
    menu: OpMenu,
    witnesses: BTreeMap<String, Witness>,
    seeds: Vec<Seed>,
    recipes: usize,
    episodes: usize,
    replays: usize,
    shrink_kept: usize,
    shrink_lost: usize,
    errors: Vec<String>,
    stats: episode::Stats,
    new_regions: usize,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub seed: u64,
    pub budget_seconds: u64,
    pub seconds: f64,
    pub protocols: Vec<u32>,
    pub episodes: usize,
    pub replays: usize,
    pub recipes: usize,
    pub shrunk: usize,
    pub not_shrunk: usize,
    pub applied: usize,
    pub failed: usize,
    pub rejected: usize,
    pub invalid_fills: usize,
    pub upgrades_refused: usize,
    pub left_out: usize,
    pub coverage_seconds: f64,
    pub refusals: BTreeMap<String, usize>,
    pub fill_errors: BTreeMap<String, usize>,
    pub signature_paths: usize,
    pub core_regions: usize,
    pub targets: usize,
    pub targets_witnessed: usize,
    pub missing: Vec<String>,
    /// Arms outside the targets that the run witnessed.
    pub beyond_targets: Vec<String>,
    pub witnesses: BTreeMap<String, Witness>,
    pub errors: Vec<String>,
}

pub const RECIPES: &str = "recipes";

/// Seeds longer than this are not extended.
const MAX_SEED_STEPS: usize = 160;
pub const REPORT: &str = "report.json";

fn write_recipe(dir: &Path, name: &str, ir: &RecipeIr) -> Result<()> {
    let path = dir.join(format!("{name}.json"));
    let text = serde_json::to_string_pretty(ir)? + "\n";
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
}

fn regions(counts: &shapekit_twin::coverage::Counts) -> Vec<RegionKey> {
    counts
        .iter()
        .flat_map(|(f, spans)| {
            spans.iter().filter(|(_, c)| **c > 0).map(move |(s, _)| {
                (
                    f.clone(),
                    s.line_start,
                    s.column_start,
                    s.line_end,
                    s.column_end,
                )
            })
        })
        .collect()
}

/// One worker's episode and what follows from it.
fn one(env: &Env<'_>, opts: &Options, state: &Mutex<State>, rng: &mut Rng, n: usize) -> Result<()> {
    let (mut spec, mut menu) = {
        let g = state.lock().expect("no worker panics holding the lock");
        let mut menu = g.menu.clone();
        let missing: std::collections::BTreeSet<String> = opts
            .targets
            .iter()
            .filter(|a| !g.witnesses.contains_key(*a))
            .cloned()
            .collect();
        menu.aim(&missing);
        // seeds short enough to replay cheaply; longer ones still count as witnesses
        let seeds: Vec<&Seed> = g
            .seeds
            .iter()
            .filter(|s| s.ir.steps.len() <= MAX_SEED_STEPS)
            .collect();
        let spec = if !seeds.is_empty() && rng.chance(1, 2) {
            let s = seeds[rng.below(seeds.len())];
            Spec {
                id: s.ir.id.clone(),
                protocol: s.ir.protocol,
                max_protocol: opts.max_protocol,
                node: s.ir.node.clone(),
                prefix: s.ir.steps.clone(),
                actions: opts.actions,
                contracts: opts.contracts,
                seed: rng.next_u64(),
                setup: true,
            }
        } else {
            let protocol = *rng.pick(&opts.protocols);
            let events = protocol >= 23 || rng.chance(1, 2);
            Spec {
                id: format!("explore.s{:x}.e{n}", opts.seed),
                protocol,
                max_protocol: opts.max_protocol,
                node: NodeOptions {
                    emit_classic_events: events,
                    backfill_stellar_asset_events: events,
                    // testnet's settings only validate at protocol 30
                    soroban_settings: opts.contracts && protocol == 30 && rng.chance(1, 2),
                    // contracts need them, and so does a frozen keys upgrade's writer contract
                    soroban_limits: opts.contracts || protocol >= episode::FROZEN_KEYS_PROTOCOL,
                    // a short persistent lifetime lets entries archive within an episode
                    min_persistent_ttl: (opts.contracts && rng.chance(1, 3)).then_some(10),
                    // the close meta extension exists from protocol 20
                    emit_ledger_close_meta_ext_v1: protocol >= 20 && rng.chance(1, 4),
                },
                prefix: Vec::new(),
                actions: opts.actions,
                contracts: opts.contracts,
                seed: rng.next_u64(),
                setup: true,
            }
        };
        (spec, menu)
    };
    let ep = episode::run(env, &spec, &mut menu)?;
    let new_regions = ep
        .counts
        .as_ref()
        .map_or(0, |c| env.seen.regions(regions(c)));
    let new_arms: BTreeSet<String> = {
        let mut g = state.lock().expect("no worker panics holding the lock");
        g.menu = menu;
        g.episodes += 1;
        g.new_regions += new_regions;
        g.stats.add(&ep.stats);
        ep.arms
            .keys()
            .filter(|a| !g.witnesses.contains_key(*a))
            .cloned()
            .collect()
    };
    let mut kept: Episode = ep;
    if !new_arms.is_empty() {
        let steps = episode::shrunk_steps(&kept);
        // a replay costs a whole chain, so only a shrink that drops a quarter is tried
        if steps.len() * 4 <= kept.ir.steps.len() * 3 {
            spec.prefix = steps;
            spec.actions = 0;
            let mut scratch = OpMenu::empty();
            let ledgers_only = Env {
                image: env.release,
                coverage: false,
                ..*env
            };
            let replay = episode::run(&ledgers_only, &spec, &mut scratch)?;
            let mut g = state.lock().expect("no worker panics holding the lock");
            g.replays += 1;
            if new_arms.iter().all(|a| replay.arms.contains_key(a)) {
                g.shrink_kept += 1;
                kept = Episode {
                    novel_steps: kept.novel_steps,
                    ..replay
                };
            } else {
                g.shrink_lost += 1;
            }
        }
        let mut g = state.lock().expect("no worker panics holding the lock");
        let name = format!("r{:04}", g.recipes);
        let mut credited = 0;
        for (arm, ledger) in &kept.arms {
            if !g.witnesses.contains_key(arm) {
                g.witnesses.insert(
                    arm.clone(),
                    Witness {
                        recipe: name.clone(),
                        ledger: *ledger,
                    },
                );
                credited += 1;
            }
        }
        if credited > 0 {
            g.recipes += 1;
            write_recipe(&opts.out.join(RECIPES), &name, &kept.ir)?;
            eprintln!(
                "{name}: {} at protocol {}, {} steps, {credited} new arms, {:.1}s",
                kept.ir.id,
                kept.ir.protocol,
                kept.ir.steps.len(),
                kept.seconds
            );
        }
        g.seeds.push(Seed { ir: kept.ir });
    } else if kept.new_paths > 0 || new_regions > 0 {
        let mut g = state.lock().expect("no worker panics holding the lock");
        if g.seeds.len() < 256 {
            g.seeds.push(Seed { ir: kept.ir });
        }
    }
    Ok(())
}

pub fn run(env: &Env<'_>, opts: &Options) -> Result<Report> {
    let started = Instant::now();
    let deadline = started + opts.budget;
    std::fs::create_dir_all(opts.out.join(RECIPES))
        .with_context(|| format!("creating {}", opts.out.display()))?;
    let state = Mutex::new(State {
        menu: OpMenu::new(env.schema, opts.contracts),
        witnesses: BTreeMap::new(),
        seeds: Vec::new(),
        recipes: 0,
        episodes: 0,
        replays: 0,
        shrink_kept: 0,
        shrink_lost: 0,
        errors: Vec::new(),
        stats: episode::Stats::default(),
        new_regions: 0,
    });
    let counter = std::sync::atomic::AtomicUsize::new(0);
    let mut root = Rng::new(opts.seed);
    let rngs: Vec<Rng> = (0..opts.jobs.max(1)).map(|_| root.split()).collect();
    std::thread::scope(|s| {
        for mut rng in rngs {
            let state = &state;
            let counter = &counter;
            s.spawn(move || {
                while Instant::now() < deadline {
                    let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if let Err(e) = one(env, opts, state, &mut rng, n) {
                        let msg = format!("episode {n}: {e:#}");
                        eprintln!("{msg}");
                        state
                            .lock()
                            .expect("no worker panics holding the lock")
                            .errors
                            .push(msg);
                    }
                }
            });
        }
    });
    let g = state
        .into_inner()
        .expect("no worker panics holding the lock");
    let (paths, regions) = env.seen.counts();
    let missing: Vec<String> = opts
        .targets
        .iter()
        .filter(|a| !g.witnesses.contains_key(*a))
        .cloned()
        .collect();
    let beyond: Vec<String> = g
        .witnesses
        .keys()
        .filter(|a| !opts.targets.contains(*a))
        .cloned()
        .collect();
    let report = Report {
        seed: opts.seed,
        budget_seconds: opts.budget.as_secs(),
        seconds: started.elapsed().as_secs_f64(),
        protocols: opts.protocols.clone(),
        episodes: g.episodes,
        replays: g.replays,
        recipes: g.recipes,
        shrunk: g.shrink_kept,
        not_shrunk: g.shrink_lost,
        applied: g.stats.applied,
        failed: g.stats.failed,
        rejected: g.stats.rejected,
        invalid_fills: g.stats.invalid_fills,
        upgrades_refused: g.stats.upgrades_refused,
        left_out: g.stats.left_out,
        coverage_seconds: g.stats.coverage_seconds,
        refusals: g.stats.refusals.clone(),
        fill_errors: g.stats.fill_errors.clone(),
        signature_paths: paths,
        core_regions: regions,
        targets: opts.targets.len(),
        targets_witnessed: opts.targets.len() - missing.len(),
        missing,
        beyond_targets: beyond,
        witnesses: g.witnesses,
        errors: g.errors,
    };
    let path = opts.out.join(REPORT);
    std::fs::write(&path, serde_json::to_string_pretty(&report)? + "\n")
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(report)
}
