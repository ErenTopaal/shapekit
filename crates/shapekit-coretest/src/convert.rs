//! The dump of a [`crate::run`] back to XDR, checked meta by meta against core's committed
//! short hashes.
//!
//! core's baseline (`test-tx-meta-baseline-current/<file>.json` in the core repository) maps
//! each test section to the short hashes of the metas it applied, in order. The short hash is
//! SipHash-2-4 of the meta's XDR (`shortHash::xdrComputeHash`, `src/crypto/ShortHash.h`, the
//! same as hashing `xdr_to_opaque`) under a key derived from the test seed
//! (`shortHash::seed`, `src/crypto/ShortHash.cpp`), stored as the eight big-endian bytes of the
//! 64-bit value in base64 (`saveTestTxMeta`, `src/test/test.cpp`). The meta is hashed after
//! `normalizeMeta`, the same value the dump holds.
//!
//! Output layout under `out`:
//!
//! ```text
//! meta/<aa>/<sha256>.xdr   each distinct TransactionMeta once, raw XDR
//! index.jsonl              one line per dumped meta, in dump order
//! convert.json             the report
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use base64::Engine;
use sha2::{Digest, Sha256};
use shapekit_schema::Schema;
use siphasher::sip::SipHasher24;
use stellar_xdr::{Limits, ReadXdr, TransactionMeta, WriteXdr};

use crate::cereal::{Converter, Resolutions};
use crate::dump::{Reader, Record};
use crate::run::Report as RunReport;
use crate::{Error, RNG_SEED, Result};

/// The short hash key core derives from a test seed: byte `i` is `seed >> (i % 4)`, a shift by
/// bits, not bytes (`shortHash::seed`).
pub fn short_hash_key(seed: u32) -> [u8; 16] {
    let mut key = [0u8; 16];
    for (i, k) in key.iter_mut().enumerate() {
        *k = (seed >> (i % 4)) as u8;
    }
    key
}

/// core's short hash of an XDR value.
pub fn short_hash(key: &[u8; 16], xdr: &[u8]) -> u64 {
    SipHasher24::new_with_key(key).hash(xdr)
}

/// file stem, then section, then the hashes in recording order.
pub type Baseline = BTreeMap<String, BTreeMap<String, Vec<u64>>>;

/// Reads every `<file>.json` of a baseline directory. Header keys (`!...`) are checked for
/// the seed and the all-versions flag the dump was run with.
pub fn load_baseline(dir: &Path) -> Result<Baseline> {
    let mut out = Baseline::new();
    let entries = std::fs::read_dir(dir).map_err(|source| Error::Read {
        path: dir.display().to_string(),
        source,
    })?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    if paths.is_empty() {
        return Err(Error::Read {
            path: dir.display().to_string(),
            source: std::io::Error::other("no baseline json files"),
        });
    }
    for path in paths {
        let shown = path.display().to_string();
        let text = std::fs::read_to_string(&path).map_err(|source| Error::Read {
            path: shown.clone(),
            source,
        })?;
        let root: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text)?;
        let bad = |message: String| Error::Dump {
            path: shown.clone(),
            line: 0,
            message,
        };
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| bad("file name is not utf-8".into()))?
            .to_owned();
        // header keys as core writes them (TESTKEY_*, src/test/test.cpp)
        if root.get("!rng seed").and_then(|v| v.as_u64()) != Some(u64::from(RNG_SEED)) {
            return Err(bad(format!("not recorded with --rng-seed {RNG_SEED}")));
        }
        if root.get("!test all versions").and_then(|v| v.as_bool()) != Some(true) {
            return Err(bad("not recorded with --all-versions".into()));
        }
        let mut sections = BTreeMap::new();
        for (key, value) in root {
            if key.starts_with('!') {
                continue;
            }
            let list = value
                .as_array()
                .ok_or_else(|| bad(format!("section {key:?} is not a list")))?;
            let mut hashes = Vec::with_capacity(list.len());
            for h in list {
                let b64 = h
                    .as_str()
                    .ok_or_else(|| bad(format!("section {key:?} holds a non-string hash")))?;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(b64)
                    .map_err(|e| bad(format!("hash {b64:?}: {e}")))?;
                let arr: [u8; 8] = bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| bad(format!("hash {b64:?} is not eight bytes")))?;
                hashes.push(u64::from_be_bytes(arr));
            }
            sections.insert(key, hashes);
        }
        out.insert(stem, sections);
    }
    Ok(out)
}

/// One line of `index.jsonl`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IndexRow {
    /// the test case (the first section component)
    pub test: String,
    pub file: String,
    pub section: String,
    /// position of this meta among its section's metas, from 0
    pub ordinal: usize,
    pub protocol: Option<u32>,
    pub meta_version: u32,
    pub sha256: String,
    /// core's short hash, hex of the big-endian bytes
    pub short_hash: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Problem {
    pub test: String,
    pub file: String,
    pub section: String,
    pub ordinal: usize,
    pub dump: String,
    pub line: u64,
    pub message: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Totals {
    pub metas: u64,
    pub converted: u64,
    pub matched: u64,
    pub mismatched: u64,
    pub conversion_errors: u64,
    /// dumped metas the baseline holds no hash for
    pub not_in_baseline: u64,
    pub baseline_hashes: u64,
    /// baseline hashes no dumped meta reached
    pub baseline_unreached: u64,
    pub distinct_metas: u64,
    pub distinct_bytes: u64,
    pub meta_versions: BTreeMap<u32, u64>,
    pub protocols: BTreeMap<u32, u64>,
    /// `SCAddress` hex values read as a claimable balance id
    pub address_claimable_balance: u64,
    /// `SCAddress` hex values read as a pool id
    pub address_liquidity_pool: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Report {
    pub run_image: String,
    pub totals: Totals,
    /// the first problems found, at most [`MAX_PROBLEMS`]
    pub problems: Vec<Problem>,
}

impl Report {
    pub fn passed(&self) -> bool {
        let t = &self.totals;
        t.metas > 0
            && t.matched == t.metas
            && t.baseline_unreached == 0
            && t.matched == t.baseline_hashes
    }
}

pub const MAX_PROBLEMS: usize = 200;

#[derive(Default)]
struct DumpResult {
    rows: Vec<IndexRow>,
    problems: Vec<Problem>,
    totals: Totals,
    /// (file, section) with the number of metas the dump gave it
    reached: BTreeMap<(String, String), usize>,
}

struct Ctx<'a> {
    converter: Converter<'a>,
    baseline: &'a Baseline,
    key: [u8; 16],
    meta_dir: PathBuf,
    written: Mutex<BTreeSet<String>>,
}

fn store(ctx: &Ctx, sha: &str, bytes: &[u8]) -> Result<bool> {
    {
        let mut w = ctx.written.lock().unwrap_or_else(|p| p.into_inner());
        if !w.insert(sha.to_owned()) {
            return Ok(false);
        }
    }
    let dir = ctx.meta_dir.join(&sha[..2]);
    std::fs::create_dir_all(&dir).map_err(|source| Error::Write {
        path: dir.display().to_string(),
        source,
    })?;
    let path = dir.join(format!("{sha}.xdr"));
    std::fs::write(&path, bytes).map_err(|source| Error::Write {
        path: path.display().to_string(),
        source,
    })?;
    Ok(true)
}

fn convert_record(
    ctx: &Ctx,
    record: &Record,
) -> std::result::Result<(Vec<u8>, Resolutions), String> {
    let doc: serde_json::Value =
        serde_json::from_slice(&record.body).map_err(|e| format!("json: {e}"))?;
    let (bytes, res) = ctx
        .converter
        .document(&doc, "TransactionMeta")
        .map_err(|e| e.to_string())?;
    let meta = TransactionMeta::from_xdr(&bytes, Limits::none())
        .map_err(|e| format!("stellar-xdr refuses the converted bytes: {e}"))?;
    let again = meta
        .to_xdr(Limits::none())
        .map_err(|e| format!("stellar-xdr cannot encode the value back: {e}"))?;
    if again != bytes {
        return Err("stellar-xdr encodes the decoded value to different bytes".into());
    }
    Ok((bytes, res))
}

fn convert_dump(ctx: &Ctx, path: &Path, test: &str) -> Result<DumpResult> {
    let file = File::open(path).map_err(|source| Error::Read {
        path: path.display().to_string(),
        source,
    })?;
    let shown = path.display().to_string();
    let reader = Reader::new(BufReader::with_capacity(1 << 20, file), shown.clone());
    let mut r = DumpResult::default();
    for record in reader {
        let record = record?;
        let ordinal = {
            let n = r
                .reached
                .entry((record.file.clone(), record.section.clone()))
                .or_default();
            *n += 1;
            *n - 1
        };
        r.totals.metas += 1;
        let meta_version = record.meta_version().unwrap_or(u32::MAX);
        *r.totals.meta_versions.entry(meta_version).or_default() += 1;
        if let Some(p) = record.protocol() {
            *r.totals.protocols.entry(p).or_default() += 1;
        }
        let problem = |message: String, r: &mut DumpResult| {
            if r.problems.len() < MAX_PROBLEMS {
                r.problems.push(Problem {
                    test: test.to_owned(),
                    file: record.file.clone(),
                    section: record.section.clone(),
                    ordinal,
                    dump: shown.clone(),
                    line: record.line,
                    message,
                });
            }
        };
        let (bytes, res) = match convert_record(ctx, &record) {
            Ok(x) => x,
            Err(message) => {
                r.totals.conversion_errors += 1;
                problem(message, &mut r);
                continue;
            }
        };
        r.totals.converted += 1;
        r.totals.address_claimable_balance += res.claimable_balance;
        r.totals.address_liquidity_pool += res.liquidity_pool;
        let got = short_hash(&ctx.key, &bytes);
        let expected = ctx
            .baseline
            .get(&record.file)
            .and_then(|s| s.get(&record.section))
            .and_then(|h| h.get(ordinal));
        match expected {
            None => {
                r.totals.not_in_baseline += 1;
                problem("core's baseline holds no hash for this meta".into(), &mut r);
            }
            Some(&want) if want != got => {
                r.totals.mismatched += 1;
                problem(
                    format!("short hash {got:016x}, core's baseline has {want:016x}"),
                    &mut r,
                );
            }
            Some(_) => r.totals.matched += 1,
        }
        let sha = hex::encode(Sha256::digest(&bytes));
        if store(ctx, &sha, &bytes)? {
            r.totals.distinct_metas += 1;
            r.totals.distinct_bytes += bytes.len() as u64;
        }
        r.rows.push(IndexRow {
            test: test.to_owned(),
            file: record.file.clone(),
            section: record.section.clone(),
            ordinal,
            protocol: record.protocol(),
            meta_version,
            sha256: sha,
            short_hash: format!("{got:016x}"),
        });
    }
    Ok(r)
}

fn add(into: &mut Totals, from: &Totals) {
    into.metas += from.metas;
    into.converted += from.converted;
    into.matched += from.matched;
    into.mismatched += from.mismatched;
    into.conversion_errors += from.conversion_errors;
    into.not_in_baseline += from.not_in_baseline;
    into.distinct_metas += from.distinct_metas;
    into.distinct_bytes += from.distinct_bytes;
    into.address_claimable_balance += from.address_claimable_balance;
    into.address_liquidity_pool += from.address_liquidity_pool;
    for (k, v) in &from.meta_versions {
        *into.meta_versions.entry(*k).or_default() += v;
    }
    for (k, v) in &from.protocols {
        *into.protocols.entry(*k).or_default() += v;
    }
}

/// Converts every dump of a run directory.
pub fn convert(
    run_dir: &Path,
    baseline_dir: &Path,
    out: &Path,
    jobs: usize,
    progress: &(dyn Fn(&str, usize, usize) + Sync),
) -> Result<Report> {
    let run = RunReport::load(&run_dir.join("run.json"))?;
    if !run.passed() {
        return Err(Error::Dump {
            path: run_dir.join("run.json").display().to_string(),
            line: 0,
            message: "the run has failed test cases; convert a complete run".into(),
        });
    }
    if out.exists()
        && std::fs::read_dir(out)
            .map_err(|source| Error::Read {
                path: out.display().to_string(),
                source,
            })?
            .next()
            .is_some()
    {
        return Err(Error::Write {
            path: out.display().to_string(),
            source: std::io::Error::other("output directory is not empty"),
        });
    }
    let meta_dir = out.join("meta");
    std::fs::create_dir_all(&meta_dir).map_err(|source| Error::Write {
        path: meta_dir.display().to_string(),
        source,
    })?;
    let baseline = load_baseline(baseline_dir)?;
    let schema = Schema::pinned();
    let ctx = Ctx {
        converter: Converter::new(&schema),
        baseline: &baseline,
        key: short_hash_key(RNG_SEED),
        meta_dir,
        written: Mutex::new(BTreeSet::new()),
    };
    let tests: Vec<_> = run.tests.iter().filter(|t| t.metas > 0).collect();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let results: Mutex<Vec<(usize, Result<DumpResult>)>> = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..jobs.max(1).min(tests.len().max(1)) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(t) = tests.get(i) else { break };
                    let path = run_dir.join("dump").join(format!("{:03}.txt", t.index));
                    let r = convert_dump(&ctx, &path, &t.name);
                    let n = done.fetch_add(1, Ordering::SeqCst) + 1;
                    progress(&t.name, n, tests.len());
                    results
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .push((i, r));
                }
            });
        }
    });
    let mut results = results.into_inner().unwrap_or_else(|p| p.into_inner());
    results.sort_by_key(|(i, _)| *i);

    let mut totals = Totals::default();
    let mut problems = Vec::new();
    let mut reached: BTreeMap<(String, String), usize> = BTreeMap::new();
    let index_path = out.join("index.jsonl");
    let index_file = File::create(&index_path).map_err(|source| Error::Write {
        path: index_path.display().to_string(),
        source,
    })?;
    let mut index = BufWriter::new(index_file);
    let write_err = |source| Error::Write {
        path: index_path.display().to_string(),
        source,
    };
    for (_, r) in results {
        let r = r?;
        add(&mut totals, &r.totals);
        for p in r.problems {
            if problems.len() < MAX_PROBLEMS {
                problems.push(p);
            }
        }
        for (key, n) in r.reached {
            if reached.insert(key.clone(), n).is_some() {
                return Err(Error::Dump {
                    path: run_dir.display().to_string(),
                    line: 0,
                    message: format!("section {key:?} appears in two test cases"),
                });
            }
        }
        for row in r.rows {
            serde_json::to_writer(&mut index, &row)?;
            index.write_all(b"\n").map_err(write_err)?;
        }
    }
    index.flush().map_err(write_err)?;

    for (file, sections) in &baseline {
        for (section, hashes) in sections {
            totals.baseline_hashes += hashes.len() as u64;
            let got = reached
                .get(&(file.clone(), section.clone()))
                .copied()
                .unwrap_or(0);
            totals.baseline_unreached += hashes.len().saturating_sub(got) as u64;
        }
    }
    let report = Report {
        run_image: run.image,
        totals,
        problems,
    };
    let mut text = serde_json::to_string_pretty(&report)?;
    text.push('\n');
    let path = out.join("convert.json");
    std::fs::write(&path, text).map_err(|source| Error::Write {
        path: path.display().to_string(),
        source,
    })?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_shifts_the_seed_by_bits() {
        let k = short_hash_key(12345);
        assert_eq!(&k[..4], &[0x39, 0x1c, 0x0e, 0x07]);
        assert_eq!(&k[4..8], &k[..4]);
        assert_eq!(&k[12..], &k[..4]);
    }

    #[test]
    fn siphash_matches_the_reference_vector() {
        // SipHash-2-4 paper, appendix a: key 00..0f, message 00..0e
        let key: [u8; 16] = std::array::from_fn(|i| i as u8);
        let msg: Vec<u8> = (0..15).collect();
        assert_eq!(short_hash(&key, &msg), 0xa129ca6149be45e5);
    }

    #[test]
    fn baseline_hashes_are_big_endian_base64() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("X.json"),
            r#"{"!rng seed": 12345, "!test all versions": true, "!cfg protocol version": 30, "a|b": ["AQIDBAUGBwg="]}"#,
        )
        .unwrap();
        let b = load_baseline(dir.path()).unwrap();
        assert_eq!(b["X"]["a|b"], vec![0x0102030405060708]);
    }

    #[test]
    fn baseline_with_another_seed_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("X.json"),
            r#"{"!rng seed": 1, "!test all versions": true, "a": []}"#,
        )
        .unwrap();
        assert!(load_baseline(dir.path()).is_err());
    }
}
