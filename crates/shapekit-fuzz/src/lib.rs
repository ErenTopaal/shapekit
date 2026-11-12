//! Corpus-seeded differential fuzzing of XDR decode and re-encode: stellar-xdr in process
//! against go-stellar-sdk through its conformance testee. docs/differential-fuzzing.md has the
//! design, the known-divergence filter and how a class becomes a corpus entry.

pub mod minimize;
pub mod mutate;
pub mod oracle;
pub mod report;
pub mod types;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    pub first: Sighting,
    pub rust: Side,
    pub go: Side,
}

pub struct Outcome {
    pub elapsed: Duration,
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

/// Starts the go testee named in versions.toml (building it first when asked) and checks the
/// in-process rust side is the version the matrix calls current.
pub fn go_target(cfg: &Config) -> Result<Target> {
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
    if cfg.build {
        versions::build(&cfg.root, &target)?;
    }
    Ok(target)
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
            let path = cfg.corpus.join(file);
            let size = std::fs::metadata(&path)
                .with_context(|| format!("{id}: {}", path.display()))?
                .len();
            if size > cfg.max_seed_bytes {
                continue;
            }
            let bytes =
                std::fs::read(&path).with_context(|| format!("{id}: {}", path.display()))?;
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

/// What one worker counted: per type, and per input origin.
type WorkerTotals = (BTreeMap<FuzzType, TypeStats>, BTreeMap<Origin, u64>);

struct Shared {
    classes: BTreeMap<String, Class>,
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
    fn next_input(&mut self, ty: FuzzType) -> (Vec<u8>, Origin) {
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
        let mut data = match origin {
            Origin::SeedMutation => pick(&mut self.rng, &self.seeds[&ty]).clone(),
            Origin::PoolMutation => pick(&mut self.rng, &self.pool[&ty]).clone(),
            Origin::Arbitrary | Origin::ArbitraryMutation => self.generate(ty),
        };
        if origin != Origin::Arbitrary {
            mutate(&mut self.rng, &mut data, donor.as_deref());
        }
        (data, origin)
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

    fn generate(&mut self, ty: FuzzType) -> Vec<u8> {
        // mostly small buffers, now and then a large one, so both shallow values with every
        // arm and long arrays or deep nesting come up
        let cap = 1usize << (2 + self.rng.below(11));
        let len = 1 + self.rng.below(cap.min(self.cfg.max_arbitrary_bytes));
        let mut raw = vec![0u8; len];
        self.rng.fill(&mut raw);
        let mut u = Unstructured::new(&raw);
        ty.arbitrary(&mut u).unwrap_or_default()
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
    ensure!(cfg.jobs > 0, "--jobs must be at least 1");
    ensure!(!cfg.types.is_empty(), "no types to fuzz");
    let target = go_target(cfg)?;
    let seeds = load_seeds(cfg)?;
    let seed_counts: BTreeMap<FuzzType, usize> = cfg
        .types
        .iter()
        .map(|t| (*t, seeds.get(t).map_or(0, Vec::len)))
        .collect();
    let shared = Mutex::new(Shared {
        classes: BTreeMap::new(),
    });
    let deadline = Instant::now() + Duration::from_secs(cfg.seconds);
    let started = Instant::now();
    let done = AtomicU64::new(0);
    let stop = AtomicBool::new(false);

    let results: Vec<Result<WorkerTotals>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..cfg.jobs)
            .map(|w| {
                let (target, seeds, shared, done, stop) = (&target, &seeds, &shared, &done, &stop);
                std::thread::Builder::new()
                    .name(format!("fuzz-{w}"))
                    .stack_size(WORKER_STACK)
                    .spawn_scoped(scope, move || {
                        let r = work(cfg, w, target, seeds, shared, deadline, done, stop);
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
    for r in results {
        let (s, o) = r?;
        for (t, ts) in s {
            let into = stats.entry(t).or_default();
            into.inputs += ts.inputs;
            into.bytes += ts.bytes;
            into.both_accept += ts.both_accept;
            into.both_reject += ts.both_reject;
            into.disagreements += ts.disagreements;
            for (k, n) in ts.filtered {
                *into.filtered.entry(k).or_default() += n;
            }
        }
        for (k, n) in o {
            *origins.entry(k).or_default() += n;
        }
    }

    let classes = shared
        .into_inner()
        .unwrap_or_else(|p| p.into_inner())
        .classes;
    let classes = minimize_all(cfg, &target, classes.into_values().collect())?;
    Ok(Outcome {
        elapsed,
        go: target,
        seeds: seed_counts,
        stats,
        origins,
        classes,
    })
}

#[allow(clippy::too_many_arguments)]
fn work(
    cfg: &Config,
    index: usize,
    target: &Target,
    seeds: &BTreeMap<FuzzType, Vec<Vec<u8>>>,
    shared: &Mutex<Shared>,
    deadline: Instant,
    done: &AtomicU64,
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
    let mut iteration = 0u64;
    loop {
        if stop.load(Ordering::Relaxed) || Instant::now() >= deadline {
            break;
        }
        if let Some(max) = cfg.max_inputs
            && done.fetch_add(1, Ordering::Relaxed) >= max
        {
            break;
        }
        let ty = cfg.types[w.rng.below(cfg.types.len())];
        let (data, origin) = w.next_input(ty);
        iteration += 1;
        *w.origins.entry(origin).or_default() += 1;
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
                let mut guard = shared.lock().unwrap_or_else(|p| p.into_inner());
                let class = guard.classes.entry(sig.clone()).or_insert_with(|| Class {
                    signature: sig,
                    ty,
                    kind,
                    count: 0,
                    input: data.clone(),
                    first: Sighting {
                        worker: w.index,
                        iteration,
                        origin,
                    },
                    rust: ev.rust.clone(),
                    go: ev.go.clone(),
                });
                class.count += 1;
                if data.len() < class.input.len() {
                    class.input = data;
                    class.rust = ev.rust;
                    class.go = ev.go;
                }
            }
        }
    }
    Ok((w.stats, w.origins))
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

fn kebab(name: &str) -> String {
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
    let target = go_target(cfg)?;
    let mut ids: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("class.json").is_file())
        .collect();
    ids.sort();
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
