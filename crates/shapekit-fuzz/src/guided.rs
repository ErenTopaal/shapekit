//! `shapekit guided-fuzz`: the three sdks searched together, steered by what each one's
//! decoder covers. Every input goes through the coverage builds of the rust, go and javascript
//! testees (`conformance::versions::coverage_target`) and through stellar-xdr in process, which
//! is the oracle as in the differential run. After every input each testee says whether its
//! coverage grew (`coverage_new`, read in the testee process from its live counters); an input
//! that grew it in any sdk joins one shared pool that later inputs are mutated from, so an input
//! that opened a new path in one sdk is tried, and mutated further, on all of them. Each run
//! starts and ends with `coverage_snapshot`, whose ranges are what the report counts.
//!
//! With the feedback, the corpus seeds and the pool form one list per type, and an entry is
//! picked with a weight that grows with the inputs mutated from it that grew coverage and shrinks
//! with the times it was picked. Without it, seeds are picked uniformly and nothing joins the
//! pool. The inputs follow from the seed and from what the testees answer, nothing else. The
//! command's `--compare` runs the same seed and budget both ways and reports the ranges each sdk
//! reached in each, along with every disagreement class with stellar-xdr.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroUsize;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use arbitrary::Unstructured;
use serde::Serialize;
use serde_json::{Map, Value};
use shapekit_conformance::testee::{Reply, Testee};
use shapekit_conformance::versions::{self, VersionsFile};

use shapekit_process::error_text::chain;

use crate::mutate::{Rng, mutate};
use crate::oracle::{classify, go_side, known_filter, rust_side, signature};
use crate::types::FuzzType;

pub const SDKS: [&str; 3] = ["rust", "go", "js"];
const POOL_CAP: usize = 4096;
const POOL_INPUT_MAX: usize = 64 * 1024;
// how much one find adds to an entry's chance of being picked again
const ENERGY: u64 = 8;

pub struct GuidedConfig {
    pub root: PathBuf,
    pub seed: u64,
    pub max_inputs: u64,
    pub build: bool,
    pub max_seed_bytes: u64,
    /// Upper bound of the raw bytes handed to `arbitrary` per generated value.
    pub max_arbitrary_bytes: NonZeroUsize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SdkRun {
    pub key: String,
    pub tool: String,
    pub ranges: usize,
    /// Inputs after which this sdk's `coverage_new` reported new units.
    pub inputs_with_new_units: u64,
    /// Units (`coverage_new`'s total) at the end of the run.
    pub units: u64,
    /// Pool entries this sdk's coverage put there (an entry can be credited to several).
    pub pool_credit: u64,
    /// Disagreements that are the known depth-accounting divergence, per its name.
    pub known: BTreeMap<String, u64>,
    /// Ranges the testee had reached before the first input, at startup.
    pub start_ranges: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClassSeen {
    pub sdk: String,
    pub signature: String,
    pub count: u64,
    /// The first input of the class, base64, and its type.
    pub xdr_type: String,
    pub first_input: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuidedRun {
    pub feedback: bool,
    pub seed: u64,
    pub inputs: u64,
    pub pool: usize,
    pub sdks: Vec<SdkRun>,
    pub sources: BTreeMap<Source, SourceStats>,
    pub classes: Vec<ClassSeen>,
}

struct Side3 {
    testee: Testee,
    sdk: String,
    covered: BTreeSet<String>,
    tool: String,
    new_inputs: u64,
    units: u64,
    credit: u64,
    known: BTreeMap<String, u64>,
    start: usize,
}

fn snapshot(t: &mut Testee) -> Result<(String, BTreeSet<String>)> {
    let key = t.target().key();
    let reply = t
        .request(Map::from_iter([(
            "op".to_string(),
            Value::from("coverage_snapshot"),
        )]))?
        .map_err(|f| anyhow::anyhow!("{key}: coverage_snapshot failed: {f}"))?;
    let Reply::Ok(r) = reply else {
        bail!("{key}: coverage_snapshot was answered with {reply:?}");
    };
    let tool = r["tool"].as_str().unwrap_or_default().to_string();
    let ranges: Vec<String> =
        serde_json::from_value(r["ranges"].clone()).context("coverage_snapshot ranges")?;
    Ok((tool, ranges.into_iter().collect()))
}

/// `coverage_new`: units run for the first time since the previous call, and units run so far.
fn fresh(t: &mut Testee) -> Result<(u64, u64)> {
    let key = t.target().key();
    let reply = t
        .request(Map::from_iter([(
            "op".to_string(),
            Value::from("coverage_new"),
        )]))?
        .map_err(|f| anyhow::anyhow!("{key}: coverage_new failed: {f}"))?;
    let Reply::Ok(r) = reply else {
        bail!("{key}: coverage_new was answered with {reply:?}");
    };
    match (r["new"].as_u64(), r["total"].as_u64()) {
        (Some(n), Some(t)) => Ok((n, t)),
        _ => bail!("{key}: coverage_new answered {r}"),
    }
}

fn start(cfg: &GuidedConfig) -> Result<Vec<Side3>> {
    let file = VersionsFile::load(&cfg.root.join("conformance/versions.toml"))?;
    let mut out = Vec::new();
    for sdk in SDKS {
        let s = file.sdk(sdk)?;
        let target = versions::coverage_target(&file.target(sdk, &s.current)?)?;
        if cfg.build {
            versions::build(&cfg.root, &target)?;
        }
        let testee = Testee::start(&cfg.root, &target)?;
        for op in ["coverage_snapshot", "coverage_new"] {
            if !testee.supports(op) {
                bail!("{}: the coverage build does not answer {op}", target.key());
            }
        }
        out.push(Side3 {
            testee,
            sdk: sdk.to_string(),
            covered: BTreeSet::new(),
            tool: String::new(),
            new_inputs: 0,
            units: 0,
            credit: 0,
            known: BTreeMap::new(),
            start: 0,
        });
    }
    Ok(out)
}

/// A pool entry: the xdr bytes of an input, or for a generated input the raw bytes `arbitrary`
/// built it from, so mutating the entry yields another well-formed value near it rather than
/// bytes that stop decoding at the first changed word.
#[derive(Clone)]
enum Entry {
    Xdr(Vec<u8>),
    Raw(Vec<u8>),
}

struct Item {
    entry: Entry,
    /// Inputs mutated from this entry that grew coverage.
    finds: u64,
    /// Inputs mutated from this entry.
    picks: u64,
}

/// A generated value and the raw bytes it came from.
fn generate(rng: &mut Rng, ty: FuzzType, max: NonZeroUsize) -> Option<(Vec<u8>, Vec<u8>)> {
    for _ in 0..8 {
        let cap = 1usize << (2 + rng.below(11));
        let len = 1 + rng.below(cap.min(max.get()));
        let mut raw = vec![0u8; len];
        rng.fill(&mut raw);
        if let Some(v) = ty.arbitrary(&mut Unstructured::new(&raw)) {
            return Some((raw, v));
        }
    }
    None
}

/// Where an input came from, for the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    SeedMutation,
    Generated,
    PoolXdrMutation,
    PoolRawMutation,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SourceStats {
    pub inputs: u64,
    /// Inputs that grew the coverage of at least one sdk.
    pub grew: u64,
}

/// One run, with or without the coverage feedback.
pub fn run(cfg: &GuidedConfig, feedback: bool) -> Result<GuidedRun> {
    let seeds = crate::load_seeds(&cfg.root.join("corpus"), FuzzType::ALL, cfg.max_seed_bytes)?;
    let mut sides = start(cfg)?;
    let mut rng = Rng::new(cfg.seed);
    // per type the corpus seeds first, then (with the feedback) the inputs that grew coverage
    let mut corpus: BTreeMap<FuzzType, Vec<Item>> = seeds
        .into_iter()
        .map(|(ty, list)| {
            let items = list
                .into_iter()
                .map(|d| Item {
                    entry: Entry::Xdr(d),
                    finds: 0,
                    picks: 0,
                })
                .collect();
            (ty, items)
        })
        .collect();
    let seed_counts: BTreeMap<FuzzType, usize> =
        corpus.iter().map(|(ty, v)| (*ty, v.len())).collect();
    let mut sources: BTreeMap<Source, SourceStats> = BTreeMap::new();
    let mut classes: BTreeMap<(String, String), ClassSeen> = BTreeMap::new();
    // what the testees reach starting up is the starting point of both runs
    for s in &mut sides {
        let (tool, covered) = snapshot(&mut s.testee)?;
        s.tool = tool;
        s.start = covered.len();
        s.covered = covered;
        // what starting up ran is not credited to the first input
        fresh(&mut s.testee)?;
    }
    for _ in 0..cfg.max_inputs {
        let ty = FuzzType::ALL[rng.below(FuzzType::ALL.len())];
        let items = corpus.get(&ty).map(Vec::as_slice).unwrap_or_default();
        let generate_percent = 25;
        let mut parent = None;
        let (data, raw, source) = if items.is_empty() || rng.below(100) < generate_percent {
            match generate(&mut rng, ty, cfg.max_arbitrary_bytes) {
                Some((r, v)) => (Some(v), Some(r), Source::Generated),
                None => (None, None, Source::Generated),
            }
        } else {
            let k = if feedback {
                // an entry picked less often so far, or whose mutations grew coverage before, is
                // picked more often: fresh entries are tried soon, worn-out ones rarely
                let weight = |it: &Item| ((1 + ENERGY * it.finds) * 1024 / (1 + it.picks)).max(1);
                let total: u64 = items.iter().map(weight).sum();
                let mut at = rng.next_u64() % total;
                items
                    .iter()
                    .position(|it| {
                        let w = weight(it);
                        if at < w {
                            true
                        } else {
                            at -= w;
                            false
                        }
                    })
                    .expect("the weights add up to the total")
            } else {
                rng.below(items.len())
            };
            parent = Some(k);
            if let Some(it) = corpus.get_mut(&ty).and_then(|v| v.get_mut(k)) {
                it.picks += 1;
            }
            let items = &corpus[&ty];
            let from_seed = k < seed_counts[&ty];
            match items[k].entry.clone() {
                Entry::Xdr(mut d) => {
                    let donor = match &items[rng.below(items.len())].entry {
                        Entry::Xdr(x) => Some(x.as_slice()),
                        Entry::Raw(_) => None,
                    };
                    mutate(&mut rng, &mut d, donor);
                    let source = if from_seed {
                        Source::SeedMutation
                    } else {
                        Source::PoolXdrMutation
                    };
                    (Some(d), None, source)
                }
                Entry::Raw(mut r) => {
                    mutate(&mut rng, &mut r, None);
                    let v = ty.arbitrary(&mut Unstructured::new(&r));
                    (v, Some(r), Source::PoolRawMutation)
                }
            }
        };
        let Some(data) = data else { continue };
        sources.entry(source).or_default().inputs += 1;
        let rust = rust_side(ty, &data, ty.limits(data.len()));
        for s in &mut sides {
            let side = match go_side(&mut s.testee, ty, &data, ty.depth()) {
                Ok(side) => side,
                // a type the testee has no table entry for tells nothing about this input
                Err(e) if chain(e.as_ref()).contains("does not support") => continue,
                Err(e) => return Err(e),
            };
            if let Some(kind) = classify(&rust, &side) {
                if let Some(known) = known_filter(&mut s.testee, ty, &data, &rust, &side)? {
                    *s.known.entry(known.to_string()).or_default() += 1;
                    continue;
                }
                let sig = signature(ty, kind, &rust, &side);
                let e = classes
                    .entry((s.sdk.clone(), sig.clone()))
                    .or_insert_with(|| ClassSeen {
                        sdk: s.sdk.clone(),
                        signature: sig,
                        count: 0,
                        xdr_type: ty.name().to_string(),
                        first_input: base64_of(&data),
                    });
                e.count += 1;
            }
        }
        let mut grew = false;
        for s in &mut sides {
            let (n, total) = fresh(&mut s.testee)?;
            s.units = total;
            if n > 0 {
                s.new_inputs += 1;
                if feedback && data.len() <= POOL_INPUT_MAX {
                    s.credit += 1;
                }
                grew = true;
            }
        }
        if grew {
            sources.entry(source).or_default().grew += 1;
        }
        if feedback && grew {
            let items = corpus.entry(ty).or_default();
            if let Some(k) = parent {
                items[k].finds += 1;
            }
            if data.len() <= POOL_INPUT_MAX {
                let entry = match raw {
                    Some(r) => Entry::Raw(r),
                    None => Entry::Xdr(data),
                };
                let item = Item {
                    entry,
                    finds: 0,
                    picks: 0,
                };
                let seeded = seed_counts.get(&ty).copied().unwrap_or(0);
                if items.len() - seeded < POOL_CAP {
                    items.push(item);
                } else {
                    // seeds stay; a full pool replaces one of its own entries
                    let k = seeded + rng.below(POOL_CAP);
                    items[k] = item;
                }
            }
        }
    }
    for s in &mut sides {
        let (_, now) = snapshot(&mut s.testee)?;
        s.covered.extend(now);
    }
    Ok(GuidedRun {
        feedback,
        seed: cfg.seed,
        inputs: cfg.max_inputs,
        pool: corpus
            .iter()
            .map(|(ty, v)| v.len() - seed_counts.get(ty).copied().unwrap_or(0))
            .sum(),
        sdks: sides
            .iter()
            .map(|s| SdkRun {
                key: s.testee.target().key(),
                tool: s.tool.clone(),
                ranges: s.covered.len(),
                inputs_with_new_units: s.new_inputs,
                units: s.units,
                pool_credit: s.credit,
                known: s.known.clone(),
                start_ranges: s.start,
            })
            .collect(),
        sources,
        classes: classes.into_values().collect(),
    })
}

fn base64_of(b: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(b)
}

/// Markdown for the two runs of `compare`.
pub fn render(with: &GuidedRun, without: &GuidedRun) -> String {
    let mut out = format!(
        "# Coverage-guided fuzzing across the three sdks\n\nSeed {}, {} inputs per run, the same seed and budget with and without the feedback.\n\n\
         | testee | tool | ranges with feedback | ranges without | ranges at start | inputs that grew it (with / without) | pool entries it credited |\n|---|---|---|---|---|---|---|\n",
        with.seed, with.inputs
    );
    for (a, b) in with.sdks.iter().zip(&without.sdks) {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} / {} | {} |\n",
            a.key,
            a.tool,
            a.ranges,
            b.ranges,
            a.start_ranges,
            a.inputs_with_new_units,
            b.inputs_with_new_units,
            a.pool_credit
        ));
    }
    out.push_str(
        "\n## Where the inputs came from\n\n| run | source | inputs | grew some sdk's coverage |\n|---|---|---|---|\n",
    );
    for run in [with, without] {
        for (source, st) in &run.sources {
            out.push_str(&format!(
                "| {} feedback | {} | {} | {} |\n",
                if run.feedback { "with" } else { "without" },
                serde_json::to_value(source)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default(),
                st.inputs,
                st.grew
            ));
        }
    }
    for run in [with, without] {
        out.push_str(&format!(
            "\n## Disagreements with stellar-xdr, {} feedback\n\n| sdk | class | inputs |\n|---|---|---|\n",
            if run.feedback { "with" } else { "without" }
        ));
        for c in &run.classes {
            out.push_str(&format!(
                "| {} | `{}` | {} |\n",
                c.sdk,
                c.signature.replace('|', "\\|"),
                c.count
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // the bound is drawn below, so the smallest one allowed must still give a length; a zero
    // bound is not a value GuidedConfig or Config can hold
    #[test]
    fn the_smallest_bound_generates_one_byte_inputs() {
        let mut rng = Rng::new(1);
        for &ty in FuzzType::ALL {
            for _ in 0..16 {
                if let Some((raw, _)) = generate(&mut rng, ty, NonZeroUsize::MIN) {
                    assert_eq!(raw.len(), 1);
                }
            }
        }
    }
}
