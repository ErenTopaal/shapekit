//! Corpus-seeded differential fuzzing of XDR decode and re-encode: stellar-xdr in process
//! against go-stellar-sdk through its conformance testee. docs/differential-fuzzing.md has the
//! design, the known-divergence filter and how a class becomes a corpus entry.
//!
//! A run with `max_inputs` that finishes inside its time budget is reproducible from its seed
//! for any number of workers: worker `w` evaluates a fixed share of the inputs from its own
//! generator, keeps its own classes, and the classes are merged afterwards by a total order
//! (the representative is the smallest input, then the lowest worker, then the earliest
//! iteration), never by which worker reached a lock first. A run cut by the clock is not,
//! and its report says so.

pub mod minimize;
pub mod mutate;
pub mod native;
pub mod oracle;
pub mod report;
pub mod types;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use arbitrary::Unstructured;
use shapekit_conformance::testee::Testee;
use shapekit_conformance::versions::{self, Target, VersionsFile};

use crate::mutate::{Rng, mutate, worker_seed};
use crate::oracle::{Kind, Side, classify, go_side, known_filter, rust_side, signature};
use crate::types::FuzzType;

// deep generated values are built, encoded, decoded and dropped recursively
const WORKER_STACK: usize = 256 * 1024 * 1024;
// grown inputs past this are not kept for further mutation; corpus ledgers stay well below
const POOL_INPUT_MAX: usize = 256 * 1024;
const POOL_CAP: usize = 256;
// predicate calls per class when shrinking
const MINIMIZE_BUDGET: usize = 6000;

pub struct Config {
    pub root: PathBuf,
    pub corpus: PathBuf,
    pub versions: PathBuf,
    pub go_version: Option<String>,
    pub build: bool,
    pub seconds: u64,
    pub max_inputs: Option<u64>,
    pub seed: u64,
    pub jobs: usize,
    pub types: Vec<FuzzType>,
    /// Corpus files above this size are not used as seeds.
    pub max_seed_bytes: u64,
    /// Upper bound of the raw bytes handed to `arbitrary` per generated value.
    pub max_arbitrary_bytes: usize,
}

/// Where an input came from, for the stats and for replaying a first sighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Origin {
    SeedMutation,
    Arbitrary,
    ArbitraryMutation,
    PoolMutation,
}

impl Origin {
    pub fn slug(self) -> &'static str {
        match self {
            Origin::SeedMutation => "seed-mutation",
            Origin::Arbitrary => "arbitrary",
            Origin::ArbitraryMutation => "arbitrary-mutation",
            Origin::PoolMutation => "pool-mutation",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TypeStats {
    pub inputs: u64,
    pub bytes: u64,
    pub both_accept: u64,
    pub both_reject: u64,
    pub filtered: BTreeMap<&'static str, u64>,
    pub disagreements: u64,
    /// Generated values `arbitrary` could not build from any of its buffers; counted as
    /// inputs of the share but never evaluated.
    pub generation_failures: u64,
}

#[derive(Debug, Clone)]
pub struct Sighting {
    pub worker: usize,
    pub iteration: u64,
    pub origin: Origin,
}

/// One disagreement class: every input with the same [`oracle::signature`].
#[derive(Debug, Clone)]
pub struct Class {
    pub signature: String,
    pub ty: FuzzType,
    pub kind: Kind,
    pub count: u64,
    /// The smallest input seen in the run, before minimisation.
    pub input: Vec<u8>,
    /// The worker that saw `input` (the lowest one among equally small inputs).
    pub input_worker: usize,
    pub first: Sighting,
    pub rust: Side,
    pub go: Side,
}

pub struct Outcome {
    pub elapsed: Duration,
    /// Whether some worker stopped at the time budget before its share of `max_inputs`
    /// (always, without `max_inputs`). Such a run is not reproducible from its seed.
    pub time_cut: bool,
    /// The protocol both sides know, the newest the reproducers can claim.
    pub protocol: u32,
    pub go: Target,
    pub seeds: BTreeMap<FuzzType, usize>,
    pub stats: BTreeMap<FuzzType, TypeStats>,
    pub origins: BTreeMap<Origin, u64>,
    pub classes: Vec<Minimized>,
}

/// A class after shrinking, with the verdicts on the reproducer itself.
#[derive(Debug, Clone)]
pub struct Minimized {
    pub class: Class,
    pub reproducer: Vec<u8>,
    pub minimize_calls: usize,
    pub rust: Side,
    pub go: Side,
}

impl Outcome {
    pub fn inputs(&self) -> u64 {
        self.stats.values().map(|s| s.inputs).sum()
    }
}

/// The go testee named in versions.toml (built first when asked), after checking the
/// in-process rust side is the version the matrix calls current, with the newest protocol
/// both of them know.
pub fn go_target(cfg: &Config) -> Result<(Target, u32)> {
    let file = VersionsFile::load(&cfg.versions)?;
    let rust = file.sdk("rust")?;
    ensure!(
        rust.current == stellar_xdr::VERSION.pkg,
        "versions.toml's current rust testee is stellar-xdr {}, this build links {}",
        rust.current,
        stellar_xdr::VERSION.pkg
    );
    let version = match &cfg.go_version {
        Some(v) => v.clone(),
        None => file.sdk("go")?.current.clone(),
    };
    let target = file.target("go", &version)?;
    let rust_target = file.target("rust", &rust.current)?;
    let protocol = match (rust_target.max_protocol, target.max_protocol) {
        (Some(r), Some(g)) => r.min(g),
        _ => bail!(
            "versions.toml gives no max_protocol for rust {} or go {version}, so reproducers cannot name a protocol",
            rust.current
        ),
    };
    if cfg.build {
        versions::build(&cfg.root, &target)?;
    }
    Ok((target, protocol))
}

/// Seeds per type: every corpus xdr entry of a fuzzed type, plus the values nested in them.
pub fn load_seeds(cfg: &Config) -> Result<BTreeMap<FuzzType, Vec<Vec<u8>>>> {
    // read loosely: only kind, type and file paths matter here, so a manifest written by a
    // newer or older model still seeds the run
    let manifest_path = cfg.corpus.join("manifest.json");
    let text = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let manifest: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("parsing {}", manifest_path.display()))?;
    let entries = manifest["entries"]
        .as_array()
        .with_context(|| format!("{} has no entries array", manifest_path.display()))?;
    let wanted: BTreeSet<FuzzType> = cfg.types.iter().copied().collect();
    let mut seen = BTreeSet::new();
    let mut seeds: BTreeMap<FuzzType, Vec<Vec<u8>>> = BTreeMap::new();
    for entry in entries {
        if entry["kind"] != "xdr" {
            continue;
        }
        let Some(ty) = entry["xdr_type"].as_str().and_then(FuzzType::from_name) else {
            continue;
        };
        let id = entry["id"].as_str().unwrap_or("?");
        for variant in ["testnet", "mainnet", "any"] {
            let Some(file) = entry["variants"][variant]["xdr_file"].as_str() else {
                continue;
            };
            // an edited manifest must not point outside the corpus or at stale bytes
            ensure!(
                shapekit_manifest::is_safe_relative(file),
                "{id}: {variant} xdr_file {file:?} is not a path inside the corpus"
            );
            let sha = entry["variants"][variant]["xdr_sha256"]
                .as_str()
                .with_context(|| format!("{id}: {variant} has no xdr_sha256"))?;
            let path = cfg.corpus.join(file);
            let size = std::fs::metadata(&path)
                .with_context(|| format!("{id}: {}", path.display()))?
                .len();
            if size > cfg.max_seed_bytes {
                continue;
            }
            let bytes = shapekit_manifest::read_verified(&cfg.corpus, file, Some(sha))
                .map_err(|e| anyhow::anyhow!("{id}: {e}"))?;
            let mut found = vec![(ty, bytes.clone())];
            found.extend(types::nested_seeds(ty, &bytes));
            for (t, b) in found {
                if wanted.contains(&t) && seen.insert((t, b.clone())) {
                    seeds.entry(t).or_default().push(b);
                }
            }
        }
    }
    Ok(seeds)
}

/// What one worker counted and saw.
struct WorkerTotals {
    stats: BTreeMap<FuzzType, TypeStats>,
    origins: BTreeMap<Origin, u64>,
    classes: BTreeMap<String, Class>,
    /// Stopped at the time budget before finishing its share.
    time_cut: bool,
}

// a failed `arbitrary` call gets fresh buffers this many times before the input is given up
const GENERATE_TRIES: usize = 8;

/// Worker `index`'s fixed share of `max` inputs over `jobs` workers.
pub fn share(max: u64, jobs: usize, index: usize) -> u64 {
    let jobs = jobs as u64;
    max / jobs + u64::from((index as u64) < max % jobs)
}

/// Folds `other` into `into` by a total order, so the result does not depend on which
/// worker saw a class first in wall-clock time: the representative is the smallest input,
/// then the lowest worker, then the earliest iteration; the first sighting is the earliest
/// iteration, then the lowest worker.
fn merge_class(into: &mut Class, other: Class) {
    into.count += other.count;
    // within one worker a tie already kept the earlier iteration
    let rank = |c: &Class| (c.input.len(), c.input_worker);
    let first_rank = |s: &Sighting| (s.iteration, s.worker);
    let other_first = other.first.clone();
    if rank(&other) < rank(into) {
        into.input = other.input;
        into.input_worker = other.input_worker;
        into.rust = other.rust;
        into.go = other.go;
    }
    if first_rank(&other_first) < first_rank(&into.first) {
        into.first = other_first;
    }
}

struct Worker<'a> {
    cfg: &'a Config,
    index: usize,
    rng: Rng,
    go: Testee,
    seeds: &'a BTreeMap<FuzzType, Vec<Vec<u8>>>,
    pool: BTreeMap<FuzzType, Vec<Vec<u8>>>,
    stats: BTreeMap<FuzzType, TypeStats>,
    origins: BTreeMap<Origin, u64>,
}

impl Worker<'_> {
    /// The next input of `ty`, or `None` (with its origin) when `arbitrary` could not build
    /// a value.
    fn next_input(&mut self, ty: FuzzType) -> (Option<Vec<u8>>, Origin) {
        let has_seeds = self.seeds.get(&ty).is_some_and(|s| !s.is_empty());
        let has_pool = self.pool.get(&ty).is_some_and(|p| !p.is_empty());
        let roll = self.rng.below(100);
        let origin = match (has_seeds, has_pool) {
            (true, _) if roll < 45 => Origin::SeedMutation,
            (_, true) if roll >= 75 => Origin::PoolMutation,
            _ if roll.is_multiple_of(3) => Origin::ArbitraryMutation,
            _ => Origin::Arbitrary,
        };
        let donor = self.donor(ty);
        let data = match origin {
            Origin::SeedMutation => Some(pick(&mut self.rng, &self.seeds[&ty]).clone()),
            Origin::PoolMutation => Some(pick(&mut self.rng, &self.pool[&ty]).clone()),
            Origin::Arbitrary | Origin::ArbitraryMutation => self.generate(ty),
        };
        let Some(mut data) = data else {
            return (None, origin);
        };
        if origin != Origin::Arbitrary {
            mutate(&mut self.rng, &mut data, donor.as_deref());
        }
        (Some(data), origin)
    }

    fn donor(&mut self, ty: FuzzType) -> Option<Vec<u8>> {
        let from_seeds = self.rng.chance(50);
        let list = if from_seeds {
            self.seeds.get(&ty)
        } else {
            self.pool.get(&ty)
        }?;
        (!list.is_empty()).then(|| pick(&mut self.rng, list).clone())
    }

    fn generate(&mut self, ty: FuzzType) -> Option<Vec<u8>> {
        // an empty input in place of a failed value would test nothing while counting as
        // coverage, so a failure gets fresh buffers and is then reported
        for _ in 0..GENERATE_TRIES {
            // mostly small buffers, now and then a large one, so both shallow values with
            // every arm and long arrays or deep nesting come up
            let cap = 1usize << (2 + self.rng.below(11));
            let len = 1 + self.rng.below(cap.min(self.cfg.max_arbitrary_bytes));
            let mut raw = vec![0u8; len];
            self.rng.fill(&mut raw);
            let mut u = Unstructured::new(&raw);
            if let Some(v) = ty.arbitrary(&mut u) {
                return Some(v);
            }
        }
        None
    }

    fn remember(&mut self, ty: FuzzType, data: Vec<u8>) {
        if data.len() > POOL_INPUT_MAX {
            return;
        }
        let pool = self.pool.entry(ty).or_default();
        if pool.len() < POOL_CAP {
            pool.push(data);
        } else {
            let i = self.rng.below(POOL_CAP);
            pool[i] = data;
        }
    }
}

fn pick<'a>(rng: &mut Rng, list: &'a [Vec<u8>]) -> &'a Vec<u8> {
    &list[rng.below(list.len())]
}

/// What one input is, all filters applied: `None` when the two agree or the disagreement is a
/// known one (its name is returned in the second slot).
pub struct Evaluation {
    pub rust: Side,
    pub go: Side,
    pub kind: Option<Kind>,
    pub filtered: Option<&'static str>,
}

pub fn evaluate(go: &mut Testee, ty: FuzzType, bytes: &[u8]) -> Result<Evaluation> {
    let rust = rust_side(ty, bytes, ty.limits(bytes.len()));
    let go_now = go_side(go, ty, bytes, ty.depth())?;
    let mut kind = classify(&rust, &go_now);
    let mut filtered = None;
    if kind.is_some() {
        filtered = known_filter(go, ty, bytes, &rust, &go_now)?;
        if filtered.is_some() {
            kind = None;
        }
    }
    Ok(Evaluation {
        rust,
        go: go_now,
        kind,
        filtered,
    })
}

pub fn run(cfg: &Config) -> Result<Outcome> {
    let (target, protocol) = go_target(cfg)?;
    run_against(cfg, target, protocol)
}

/// [`run`] against any testee speaking the conformance protocol in place of the go one.
pub fn run_against(cfg: &Config, target: Target, protocol: u32) -> Result<Outcome> {
    ensure!(cfg.jobs > 0, "--jobs must be at least 1");
    ensure!(!cfg.types.is_empty(), "no types to fuzz");
    let seeds = load_seeds(cfg)?;
    let seed_counts: BTreeMap<FuzzType, usize> = cfg
        .types
        .iter()
        .map(|t| (*t, seeds.get(t).map_or(0, Vec::len)))
        .collect();
    let deadline = Instant::now() + Duration::from_secs(cfg.seconds);
    let started = Instant::now();
    let stop = AtomicBool::new(false);

    let results: Vec<Result<WorkerTotals>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..cfg.jobs)
            .map(|w| {
                let (target, seeds, stop) = (&target, &seeds, &stop);
                std::thread::Builder::new()
                    .name(format!("fuzz-{w}"))
                    .stack_size(WORKER_STACK)
                    .spawn_scoped(scope, move || {
                        let r = work(cfg, w, target, seeds, deadline, stop);
                        if r.is_err() {
                            stop.store(true, Ordering::Relaxed);
                        }
                        r
                    })
                    .expect("spawning a fuzz worker")
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_else(|_| bail!("a fuzz worker panicked")))
            .collect()
    });
    let elapsed = started.elapsed();

    let mut stats: BTreeMap<FuzzType, TypeStats> = BTreeMap::new();
    let mut origins: BTreeMap<Origin, u64> = BTreeMap::new();
    let mut classes: BTreeMap<String, Class> = BTreeMap::new();
    let mut time_cut = cfg.max_inputs.is_none();
    // results are in worker order, which the merge's tie-breaks rely on only through the
    // worker index recorded in each class
    for r in results {
        let WorkerTotals {
            stats: s,
            origins: o,
            classes: c,
            time_cut: cut,
        } = r?;
        time_cut |= cut;
        for (sig, class) in c {
            match classes.get_mut(&sig) {
                Some(into) => merge_class(into, class),
                None => {
                    classes.insert(sig, class);
                }
            }
        }
        for (t, ts) in s {
            let into = stats.entry(t).or_default();
            into.inputs += ts.inputs;
            into.bytes += ts.bytes;
            into.both_accept += ts.both_accept;
            into.both_reject += ts.both_reject;
            into.disagreements += ts.disagreements;
            into.generation_failures += ts.generation_failures;
            for (k, n) in ts.filtered {
                *into.filtered.entry(k).or_default() += n;
            }
        }
        for (k, n) in o {
            *origins.entry(k).or_default() += n;
        }
    }

    let classes = minimize_all(cfg, &target, classes.into_values().collect())?;
    Ok(Outcome {
        elapsed,
        time_cut,
        protocol,
        go: target,
        seeds: seed_counts,
        stats,
        origins,
        classes,
    })
}

fn work(
    cfg: &Config,
    index: usize,
    target: &Target,
    seeds: &BTreeMap<FuzzType, Vec<Vec<u8>>>,
    deadline: Instant,
    stop: &AtomicBool,
) -> Result<WorkerTotals> {
    let go = Testee::start(&cfg.root, target)?;
    let mut w = Worker {
        cfg,
        index,
        rng: Rng::new(worker_seed(cfg.seed, index)),
        go,
        seeds,
        pool: BTreeMap::new(),
        stats: BTreeMap::new(),
        origins: BTreeMap::new(),
    };
    let share = cfg.max_inputs.map(|max| share(max, cfg.jobs, index));
    let mut classes: BTreeMap<String, Class> = BTreeMap::new();
    let mut iteration = 0u64;
    let mut time_cut = false;
    loop {
        if share.is_some_and(|s| iteration >= s) {
            break;
        }
        if stop.load(Ordering::Relaxed) || Instant::now() >= deadline {
            time_cut = true;
            break;
        }
        let ty = cfg.types[w.rng.below(cfg.types.len())];
        let (data, origin) = w.next_input(ty);
        iteration += 1;
        *w.origins.entry(origin).or_default() += 1;
        let Some(data) = data else {
            w.stats.entry(ty).or_default().generation_failures += 1;
            continue;
        };
        let ev = evaluate(&mut w.go, ty, &data)?;
        let st = w.stats.entry(ty).or_default();
        st.inputs += 1;
        st.bytes += data.len() as u64;
        if let Some(f) = ev.filtered {
            *st.filtered.entry(f).or_default() += 1;
            continue;
        }
        match ev.kind {
            None => {
                if ev.rust.accepted().is_some() {
                    st.both_accept += 1;
                    if origin != Origin::SeedMutation || w.rng.chance(25) {
                        w.remember(ty, data);
                    }
                } else {
                    st.both_reject += 1;
                }
            }
            Some(kind) => {
                st.disagreements += 1;
                let sig = signature(ty, kind, &ev.rust, &ev.go);
                let class = classes.entry(sig.clone()).or_insert_with(|| Class {
                    signature: sig,
                    ty,
                    kind,
                    count: 0,
                    input: data.clone(),
                    input_worker: w.index,
                    first: Sighting {
                        worker: w.index,
                        iteration,
                        origin,
                    },
                    rust: ev.rust.clone(),
                    go: ev.go.clone(),
                });
                class.count += 1;
                // strictly smaller, so a tie keeps the earlier iteration
                if data.len() < class.input.len() {
                    class.input = data;
                    class.rust = ev.rust;
                    class.go = ev.go;
                }
            }
        }
    }
    Ok(WorkerTotals {
        stats: w.stats,
        origins: w.origins,
        classes,
        time_cut,
    })
}

/// Shrinks every class on one testee; a class whose input no longer reproduces (a go
/// timeout under load, say) keeps its original input and says so through its verdicts.
fn minimize_all(cfg: &Config, target: &Target, classes: Vec<Class>) -> Result<Vec<Minimized>> {
    if classes.is_empty() {
        return Ok(Vec::new());
    }
    let cfg_root = cfg.root.clone();
    let target = target.clone();
    std::thread::Builder::new()
        .name("fuzz-minimize".into())
        .stack_size(WORKER_STACK)
        .spawn(move || -> Result<Vec<Minimized>> {
            let mut go = Testee::start(&cfg_root, &target)?;
            let mut out = Vec::new();
            for class in classes {
                let (reproducer, calls) = minimize::minimize(&class.input, MINIMIZE_BUDGET, |b| {
                    let ev = evaluate(&mut go, class.ty, b)?;
                    Ok(ev.kind.is_some_and(|k| {
                        signature(class.ty, k, &ev.rust, &ev.go) == class.signature
                    }))
                })?;
                let ev = evaluate(&mut go, class.ty, &reproducer)?;
                out.push(Minimized {
                    class,
                    reproducer,
                    minimize_calls: calls,
                    rust: ev.rust,
                    go: ev.go,
                });
            }
            Ok(out)
        })
        .context("spawning the minimiser")?
        .join()
        .unwrap_or_else(|_| bail!("the minimiser panicked"))
}

/// Stable id of a class: its type, its kind and a hash of its signature.
pub fn class_id(ty: FuzzType, kind: Kind, signature: &str) -> String {
    let h = shapekit_manifest::sha256_hex(signature.as_bytes());
    format!("fuzz.{}-{}-{}", kebab(ty.name()), kind.slug(), &h[..8])
}

pub(crate) fn kebab(name: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = name.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() {
            // a new word starts at an upper case letter after a lower case one or a digit, or
            // before a lower case one inside a run (ScSpecFunctionV0 -> sc-spec-function-v0)
            let prev_lower = i > 0 && !chars[i - 1].is_ascii_uppercase();
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_ascii_lowercase());
            let prev_upper = i > 0 && chars[i - 1].is_ascii_uppercase();
            if i > 0 && (prev_lower || (prev_upper && next_lower)) {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(*c);
        }
    }
    out
}

/// `findings/fuzz/known-classes.txt`: one `<class-id> <issue or reason>` per line, `#` for
/// comments. A listed class is counted and reported but does not fail the run.
pub fn load_known(path: &Path) -> Result<BTreeMap<String, String>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut out = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (id, why) = line.split_once(char::is_whitespace).with_context(|| {
            format!(
                "{}:{}: expected `<class-id> <issue or reason>`",
                path.display(),
                n + 1
            )
        })?;
        ensure!(
            out.insert(id.to_string(), why.trim().to_string()).is_none(),
            "{}:{}: {id} is listed twice",
            path.display(),
            n + 1
        );
    }
    Ok(out)
}

/// One stored reproducer run again: the class it was filed under and the class it is in now
/// (`None` when the two sides agree on it today).
pub struct Replayed {
    pub id: String,
    pub now: Option<String>,
    pub rust: Side,
    pub go: Side,
}

/// Re-runs every `<dir>/<id>/any.xdr` that has a `class.json`, so a fixed or changed
/// disagreement shows up.
pub fn replay(cfg: &Config, dir: &Path) -> Result<Vec<Replayed>> {
    let (target, _) = go_target(cfg)?;
    let mut ids: Vec<PathBuf> = Vec::new();
    for e in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let p = e
            .with_context(|| format!("reading {}", dir.display()))?
            .path();
        if p.join("class.json").is_file() {
            ids.push(p);
        }
    }
    ids.sort();
    // a replay of nothing would pass without having checked anything
    ensure!(
        !ids.is_empty(),
        "{} holds no reproducer (no <id>/class.json), so there is nothing to replay",
        dir.display()
    );
    let target2 = target.clone();
    let root = cfg.root.clone();
    std::thread::Builder::new()
        .name("fuzz-replay".into())
        .stack_size(WORKER_STACK)
        .spawn(move || -> Result<Vec<Replayed>> {
            let mut go = Testee::start(&root, &target2)?;
            let mut out = Vec::new();
            for at in ids {
                let text = std::fs::read_to_string(at.join("class.json"))?;
                let class: serde_json::Value = serde_json::from_str(&text)
                    .with_context(|| format!("parsing {}", at.join("class.json").display()))?;
                let id = class["id"]
                    .as_str()
                    .context("class.json has no id")?
                    .to_string();
                let name = class["xdr_type"]
                    .as_str()
                    .context("class.json has no xdr_type")?;
                let ty = FuzzType::from_name(name)
                    .with_context(|| format!("{id}: {name} is not a fuzzed type"))?;
                let bytes = std::fs::read(at.join("any.xdr"))?;
                let ev = evaluate(&mut go, ty, &bytes)?;
                let now = ev
                    .kind
                    .map(|k| class_id(ty, k, &signature(ty, k, &ev.rust, &ev.go)));
                out.push(Replayed {
                    id,
                    now,
                    rust: ev.rust,
                    go: ev.go,
                });
            }
            Ok(out)
        })
        .context("spawning the replay")?
        .join()
        .unwrap_or_else(|_| bail!("the replay panicked"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares_cover_the_budget_exactly() {
        for (max, jobs) in [(30000u64, 3usize), (10, 4), (3, 4), (0, 2), (7, 1)] {
            let total: u64 = (0..jobs).map(|w| share(max, jobs, w)).sum();
            assert_eq!(total, max, "{max} over {jobs}");
        }
        assert_eq!(share(10, 4, 0), 3);
        assert_eq!(share(10, 4, 3), 2);
    }

    fn class(worker: usize, iteration: u64, input: &[u8]) -> Class {
        Class {
            signature: "s".into(),
            ty: FuzzType::ScVal,
            kind: Kind::RustAcceptsGoRejects,
            count: 1,
            input: input.to_vec(),
            input_worker: worker,
            first: Sighting {
                worker,
                iteration,
                origin: Origin::Arbitrary,
            },
            rust: Side::Accepted(input.to_vec()),
            go: Side::Rejected(format!("w{worker}")),
        }
    }

    #[test]
    fn class_merge_does_not_depend_on_arrival_order() {
        let parts = [
            class(0, 50, &[1, 2, 3]),
            class(1, 7, &[9, 9]),
            class(2, 3, &[8, 8]),
        ];
        let merged = |order: &[usize]| {
            let mut it = order.iter().map(|&i| parts[i].clone());
            let mut into = it.next().unwrap();
            for c in it {
                merge_class(&mut into, c);
            }
            into
        };
        let a = merged(&[0, 1, 2]);
        let b = merged(&[2, 1, 0]);
        let c = merged(&[1, 0, 2]);
        for m in [&a, &b, &c] {
            assert_eq!(m.count, 3);
            // two inputs of length 2: the lower worker wins
            assert_eq!(m.input, vec![9, 9]);
            assert_eq!(m.go, Side::Rejected("w1".into()));
            // earliest iteration is worker 2's third input
            assert_eq!((m.first.worker, m.first.iteration), (2, 3));
        }
    }

    #[test]
    fn kebab_names() {
        assert_eq!(kebab("TransactionEnvelope"), "transaction-envelope");
        assert_eq!(kebab("ScSpecFunctionV0"), "sc-spec-function-v0");
        assert_eq!(
            kebab("TransactionResultMetaV1"),
            "transaction-result-meta-v1"
        );
        assert_eq!(kebab("ScVal"), "sc-val");
    }

    #[test]
    fn every_type_generates() {
        let raw: Vec<u8> = (0..512u32).map(|i| (i * 37 % 251) as u8).collect();
        for t in FuzzType::ALL {
            let mut u = Unstructured::new(&raw);
            let bytes = t.arbitrary(&mut u).expect("arbitrary value");
            // what arbitrary builds is a value, so stellar-xdr reads its encoding back
            // unless it is nested past the decode depth
            let back = rust_side(*t, &bytes, stellar_xdr::Limits::none());
            assert_eq!(back, Side::Accepted(bytes), "{}", t.name());
        }
    }
}
