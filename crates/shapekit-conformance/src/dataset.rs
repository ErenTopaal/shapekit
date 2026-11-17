//! `shapekit conformance run --dataset <dir>`: a large set of `TransactionMeta` that is not
//! in the manifest, the converted dump of core's own transaction tests (`shapekit coretest
//! convert`), through each sdk's `decode_encode`. The directory holds `index.jsonl` (one line
//! per meta core wrote: test, file, section, protocol, sha256) and `meta/<first two hex>/<sha256>.xdr`
//! (each distinct meta once).
//!
//! A meta passes when the sdk decodes it and writes the same bytes back. A failure on a meta
//! whose every occurrence is from a protocol newer than the sdk version's `max_protocol` is
//! counted apart, as the corpus runner does. A dropped meta is shrunk while the sdk still
//! drops it the same way (operations, changes and events removed one at a time) and written to
//! the candidate lane; every test section gets a `core-test-section` obligation.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use shapekit_contracts::obligations::{
    Obligation, ObligationKind, ObligationLedger, ObligationStatus,
};
use stellar_xdr::{Limits, ReadXdr, TransactionMeta, WriteXdr};

use crate::compare::{self, Judged, ResultClass};
use crate::plan::Expect;
use crate::testee::Testee;
use crate::versions::{self, Target, VersionsFile};

const TYPE: &str = "TransactionMeta";
// requests per dropped meta when shrinking it
const SHRINK_BUDGET: usize = 2000;

#[derive(Debug, Clone, Deserialize)]
pub struct IndexRecord {
    pub test: String,
    pub file: String,
    pub section: String,
    /// From the section name; `None` for a test that runs at core's current protocol only and
    /// names none.
    pub protocol: Option<u32>,
    pub meta_version: u32,
    pub sha256: String,
}

pub struct Dataset {
    pub dir: PathBuf,
    pub records: Vec<IndexRecord>,
    /// Distinct metas: sha256 to the indices of its records.
    pub metas: BTreeMap<String, Vec<usize>>,
}

impl Dataset {
    pub fn load(dir: &Path) -> Result<Dataset> {
        let path = dir.join("index.jsonl");
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let mut records = Vec::new();
        let mut metas: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (n, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let r: IndexRecord = serde_json::from_str(line)
                .with_context(|| format!("{}:{}", path.display(), n + 1))?;
            ensure!(
                r.sha256.len() == 64 && r.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
                "{}:{}: {:?} is not a sha256",
                path.display(),
                n + 1,
                r.sha256
            );
            metas
                .entry(r.sha256.clone())
                .or_default()
                .push(records.len());
            records.push(r);
        }
        ensure!(!records.is_empty(), "{} lists no meta", path.display());
        Ok(Dataset {
            dir: dir.to_path_buf(),
            records,
            metas,
        })
    }

    /// The bytes of a distinct meta, checked against its name.
    pub fn bytes(&self, sha: &str) -> Result<Vec<u8>> {
        let path = self
            .dir
            .join("meta")
            .join(&sha[..2])
            .join(format!("{sha}.xdr"));
        let b = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        let got = shapekit_manifest::sha256_hex(&b);
        ensure!(got == sha, "{} hashes to {got}", path.display());
        Ok(b)
    }

    pub fn sections(&self) -> BTreeMap<(String, String), BTreeSet<String>> {
        let mut out: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
        for r in &self.records {
            out.entry((r.file.clone(), r.section.clone()))
                .or_default()
                .insert(r.sha256.clone());
        }
        out
    }

    /// The newest protocol any section names: the protocol of the core that wrote the dump,
    /// which a section naming none runs at.
    pub fn current_protocol(&self) -> u32 {
        self.records
            .iter()
            .filter_map(|r| r.protocol)
            .max()
            .unwrap_or(0)
    }

    /// The oldest protocol a meta was written under, where its sections name one.
    fn protocol(&self, sha: &str) -> Option<u32> {
        self.metas[sha]
            .iter()
            .filter_map(|&i| self.records[i].protocol)
            .min()
    }
}

fn request(bytes: &[u8]) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("op".into(), Value::from("decode_encode"));
    m.insert("type".into(), Value::from(TYPE));
    m.insert("xdr".into(), Value::from(B64.encode(bytes)));
    m.insert(
        "depth_limit".into(),
        Value::from(shapekit_manifest::decode_depth(TYPE)),
    );
    m.insert("len_limit".into(), Value::from(bytes.len()));
    m
}

/// What one testee made of one meta: the result class and the testee's answer in short.
fn judge(testee: &mut Testee, bytes: &[u8]) -> Result<(ResultClass, Option<String>, Value)> {
    let answer = testee.request(request(bytes))?;
    match compare::judge(&Expect::Bytes(B64.encode(bytes)), &answer) {
        Judged::Done(v) => Ok((v.result, v.detail, v.actual)),
        Judged::Peer(_) => unreachable!("a byte expectation is never judged by peers"),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Dropped {
    pub sha256: String,
    pub protocol: Option<u32>,
    pub meta_version: u32,
    pub sections: usize,
    pub first_section: String,
    pub detail: Option<String>,
    pub bytes: usize,
    pub shrunk_bytes: usize,
    /// The candidate entry it was written as.
    pub entry: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TargetResult {
    pub key: String,
    pub distinct: usize,
    /// Distinct metas by result class.
    pub results: BTreeMap<ResultClass, usize>,
    /// Failures on metas from protocols the version does not know.
    pub beyond_protocol: usize,
    pub sections_passing: usize,
    pub dropped: Vec<Dropped>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DatasetReport {
    /// The core image whose tests wrote the dump, from the dataset's `convert.json`. The
    /// dataset's location is left out: it is a local directory, not part of the result.
    pub core_image: Option<String>,
    pub metas: usize,
    pub distinct: usize,
    pub sections: usize,
    pub targets: Vec<TargetResult>,
}

pub struct DatasetOptions {
    pub root: PathBuf,
    pub dataset: PathBuf,
    pub versions: PathBuf,
    pub sdks: Vec<String>,
    pub only_versions: Vec<String>,
    pub all_versions: bool,
    pub build: bool,
    pub out: PathBuf,
    /// The candidate lane dropped metas are written to.
    pub candidate: PathBuf,
}

fn targets(opts: &DatasetOptions) -> Result<Vec<Target>> {
    let file = VersionsFile::load(&opts.versions)?;
    if opts.only_versions.is_empty() {
        return file.select(&opts.sdks, opts.all_versions);
    }
    ensure!(
        opts.sdks.len() == 1,
        "--version needs exactly one --sdk, since versions differ per sdk"
    );
    opts.only_versions
        .iter()
        .map(|v| file.target(&opts.sdks[0], v))
        .collect()
}

/// Shrinks a dropped meta while the testee still gives it `class` and the same answer shape:
/// whole operations, then single changes and events, are removed while the meta still encodes.
fn shrink(testee: &mut Testee, bytes: &[u8], class: ResultClass) -> Result<Vec<u8>> {
    let Ok(mut meta) = TransactionMeta::from_xdr(bytes, Limits::none()) else {
        // stellar-xdr itself cannot read it, so there is no structure to remove from
        return Ok(bytes.to_vec());
    };
    let mut best = bytes.to_vec();
    let mut calls = 0;
    loop {
        let mut improved = false;
        for cand in candidates(&meta) {
            if calls >= SHRINK_BUDGET {
                return Ok(best);
            }
            let Ok(b) = cand.to_xdr(Limits::none()) else {
                continue;
            };
            if b.len() >= best.len() {
                continue;
            }
            calls += 1;
            if judge(testee, &b)?.0 == class {
                best = b;
                meta = cand;
                improved = true;
                break;
            }
        }
        if !improved {
            return Ok(best);
        }
    }
}

macro_rules! drop_each {
    ($out:ident, $meta:ident, $variant:ident, $field:ident) => {
        if let TransactionMeta::$variant(m) = &$meta {
            for i in 0..m.$field.len() {
                let mut v = m.$field.to_vec();
                v.remove(i);
                if let Ok(vm) = v.try_into() {
                    let mut c = m.clone();
                    c.$field = vm;
                    $out.push(TransactionMeta::$variant(c));
                }
            }
        }
    };
}

/// Every meta one element smaller: an operation, a change before or after, or an event left out.
fn candidates(meta: &TransactionMeta) -> Vec<TransactionMeta> {
    let mut out = Vec::new();
    drop_each!(out, meta, V1, operations);
    drop_each!(out, meta, V1, tx_changes);
    drop_each!(out, meta, V2, operations);
    drop_each!(out, meta, V2, tx_changes_before);
    drop_each!(out, meta, V2, tx_changes_after);
    drop_each!(out, meta, V3, operations);
    drop_each!(out, meta, V3, tx_changes_before);
    drop_each!(out, meta, V3, tx_changes_after);
    drop_each!(out, meta, V4, operations);
    drop_each!(out, meta, V4, tx_changes_before);
    drop_each!(out, meta, V4, tx_changes_after);
    drop_each!(out, meta, V4, events);
    drop_each!(out, meta, V4, diagnostic_events);
    // single changes inside each operation
    let ops: Vec<stellar_xdr::LedgerEntryChanges> = match meta {
        TransactionMeta::V0(o) => o.iter().map(|x| x.changes.clone()).collect(),
        TransactionMeta::V1(m) => m.operations.iter().map(|x| x.changes.clone()).collect(),
        TransactionMeta::V2(m) => m.operations.iter().map(|x| x.changes.clone()).collect(),
        TransactionMeta::V3(m) => m.operations.iter().map(|x| x.changes.clone()).collect(),
        TransactionMeta::V4(m) => m.operations.iter().map(|x| x.changes.clone()).collect(),
    };
    for (k, changes) in ops.iter().enumerate() {
        for i in 0..changes.len() {
            let mut v = changes.to_vec();
            v.remove(i);
            let Ok(smaller) = v.try_into() else { continue };
            let smaller: stellar_xdr::LedgerEntryChanges = smaller;
            let mut c = meta.clone();
            match &mut c {
                TransactionMeta::V0(o) => {
                    let mut ops = o.to_vec();
                    ops[k].changes = smaller;
                    if let Ok(vm) = ops.try_into() {
                        *o = vm;
                    }
                }
                TransactionMeta::V1(m) => {
                    let mut ops = m.operations.to_vec();
                    ops[k].changes = smaller;
                    if let Ok(vm) = ops.try_into() {
                        m.operations = vm;
                    }
                }
                TransactionMeta::V2(m) => {
                    let mut ops = m.operations.to_vec();
                    ops[k].changes = smaller;
                    if let Ok(vm) = ops.try_into() {
                        m.operations = vm;
                    }
                }
                TransactionMeta::V3(m) => {
                    let mut ops = m.operations.to_vec();
                    ops[k].changes = smaller;
                    if let Ok(vm) = ops.try_into() {
                        m.operations = vm;
                    }
                }
                TransactionMeta::V4(m) => {
                    let mut ops = m.operations.to_vec();
                    ops[k].changes = smaller;
                    if let Ok(vm) = ops.try_into() {
                        m.operations = vm;
                    }
                }
            }
            out.push(c);
        }
    }
    out
}

pub fn run(opts: &DatasetOptions) -> Result<DatasetReport> {
    let data = Dataset::load(&opts.dataset)?;
    let sections = data.sections();
    let targets = targets(opts)?;
    let mut results = Vec::new();
    for target in &targets {
        if opts.build {
            eprintln!("building {}", target.key());
            versions::build(&opts.root, target)?;
        }
        let mut testee = Testee::start(&opts.root, target)?;
        let mut counts: BTreeMap<ResultClass, usize> = BTreeMap::new();
        let mut failed: BTreeMap<String, (ResultClass, Option<String>)> = BTreeMap::new();
        let mut beyond = 0;
        for (i, sha) in data.metas.keys().enumerate() {
            if i % 5000 == 0 {
                eprintln!("{}: {i} of {} metas", target.key(), data.metas.len());
            }
            let bytes = data.bytes(sha)?;
            let (class, detail, _) = judge(&mut testee, &bytes)?;
            // a meta from a section that names no protocol is from core's current one
            let newer = match (target.max_protocol, data.protocol(sha)) {
                (Some(max), Some(p)) => p > max,
                (Some(max), None) => max < data.current_protocol(),
                (None, _) => false,
            };
            if class != ResultClass::Pass && newer {
                beyond += 1;
                continue;
            }
            *counts.entry(class).or_default() += 1;
            // a testee without the type or op answers unsupported: a gap in the testee, not
            // the sdk dropping the meta
            if class != ResultClass::Pass && class != ResultClass::Unsupported {
                failed.insert(sha.clone(), (class, detail));
            }
        }
        let unsupported = counts.get(&ResultClass::Unsupported).copied().unwrap_or(0);
        let sections_passing = if unsupported > 0 {
            0
        } else {
            sections
                .values()
                .filter(|shas| shas.iter().all(|s| !failed.contains_key(s)))
                .count()
        };
        let mut dropped = Vec::new();
        for (sha, (class, detail)) in &failed {
            let bytes = data.bytes(sha)?;
            let small = shrink(&mut testee, &bytes, *class)?;
            let recs: Vec<&IndexRecord> =
                data.metas[sha].iter().map(|&i| &data.records[i]).collect();
            let entry = write_candidate(
                opts,
                target,
                sha,
                &small,
                (*class, detail.as_deref()),
                recs[0],
                data.current_protocol(),
            )?;
            dropped.push(Dropped {
                sha256: sha.clone(),
                protocol: data.protocol(sha),
                meta_version: recs[0].meta_version,
                sections: recs
                    .iter()
                    .map(|r| (&r.file, &r.section))
                    .collect::<BTreeSet<_>>()
                    .len(),
                first_section: format!("{} | {}", recs[0].file, recs[0].section),
                detail: detail.clone(),
                bytes: bytes.len(),
                shrunk_bytes: small.len(),
                entry,
            });
        }
        results.push(TargetResult {
            key: target.key(),
            distinct: data.metas.len(),
            results: counts,
            beyond_protocol: beyond,
            sections_passing,
            dropped,
        });
    }
    let core_image = std::fs::read_to_string(opts.dataset.join("convert.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v["run_image"].as_str().map(str::to_string));
    let report = DatasetReport {
        core_image,
        metas: data.records.len(),
        distinct: data.metas.len(),
        sections: sections.len(),
        targets: results,
    };
    std::fs::create_dir_all(&opts.out)?;
    let mut text = serde_json::to_string_pretty(&report)?;
    text.push('\n');
    std::fs::write(opts.out.join("dataset-report.json"), text)?;
    std::fs::write(opts.out.join("DATASET.md"), render(&report))?;
    // one obligation per section, 29,691 for core 30: written compact, one line
    // the witness names the committed report, so the output has to sit inside the repository
    let out_abs = if opts.out.is_absolute() {
        opts.out.clone()
    } else {
        opts.root.join(&opts.out)
    };
    let rel = out_abs
        .strip_prefix(&opts.root)
        .with_context(|| {
            format!(
                "{} is outside the repository, so no witness can name its report",
                opts.out.display()
            )
        })?
        .join("dataset-report.json");
    let witness = format!("dataset:{}", rel.display());
    let ledger = obligations(&report, &sections, &witness);
    let mut text = serde_json::to_string(&ledger)?;
    text.push('\n');
    std::fs::write(opts.out.join("core-test-sections.obligations.json"), text)?;
    Ok(report)
}

/// One `core-test-section` obligation per section. A section is met when every sdk version
/// the run covered round-tripped every distinct meta of the section; its witness is the run's
/// committed report (`witness`, `dataset:<path>`) and its evidence names those versions. A
/// section some version fails stays open, and the evidence names the failing versions and
/// how many of the section's metas they got through.
pub fn obligations(
    report: &DatasetReport,
    sections: &BTreeMap<(String, String), BTreeSet<String>>,
    witness: &str,
) -> ObligationLedger {
    let failing: Vec<(String, BTreeSet<&str>)> = report
        .targets
        .iter()
        .map(|t| {
            (
                t.key.clone(),
                t.dropped.iter().map(|d| d.sha256.as_str()).collect(),
            )
        })
        .collect();
    let mut out = Vec::new();
    for ((file, section), shas) in sections {
        let mut passing = Vec::new();
        let mut failed = Vec::new();
        for (k, f) in &failing {
            let bad = shas.iter().filter(|s| f.contains(s.as_str())).count();
            if bad == 0 {
                passing.push(k.as_str());
            } else {
                failed.push(format!("{k} {}/{}", shas.len() - bad, shas.len()));
            }
        }
        let id = Obligation::id_for(
            ObligationKind::CoreTestSection,
            &format!("{file}|{section}"),
        );
        let ob = if failed.is_empty() && !passing.is_empty() {
            Obligation {
                id,
                kind: ObligationKind::CoreTestSection,
                status: ObligationStatus::Met,
                witnesses: vec![witness.to_string()],
                evidence: Some(format!(
                    "{} distinct metas of core's test dump, every one round-tripped by {}",
                    shas.len(),
                    passing.join(", ")
                )),
            }
        } else {
            Obligation {
                id,
                kind: ObligationKind::CoreTestSection,
                status: ObligationStatus::Open,
                witnesses: Vec::new(),
                evidence: Some(format!(
                    "{} distinct metas of core's test dump; not round-tripped by {}{}",
                    shas.len(),
                    if failed.is_empty() {
                        "any version: the run covered none".to_string()
                    } else {
                        failed.join(", ")
                    },
                    if passing.is_empty() {
                        String::new()
                    } else {
                        format!("; every one round-tripped by {}", passing.join(", "))
                    }
                )),
            }
        };
        out.push(ob);
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out.dedup_by(|a, b| a.id == b.id);
    ObligationLedger {
        schema_version: 1,
        obligations: out,
    }
}

fn write_candidate(
    opts: &DatasetOptions,
    target: &Target,
    sha: &str,
    bytes: &[u8],
    (class, detail): (ResultClass, Option<&str>),
    first: &IndexRecord,
    current: u32,
) -> Result<Option<String>> {
    let registry = shapekit_registry::Registry::load(&opts.root.join("registry"))
        .map_err(|e| anyhow::anyhow!("loading registry/: {e}"))?;
    let families = registry
        .families
        .map(|f| f.families)
        .context("registry/families.toml is missing")?;
    let Some(fam) = family_for(&families, bytes) else {
        return Ok(None);
    };
    let small_sha = shapekit_manifest::sha256_hex(bytes);
    let id = format!("{}.core-test-{}-{}", fam.id, target.sdk, &small_sha[..12]);
    let dir = opts.candidate.join("entries").join(&fam.id).join(&id);
    if dir.exists() {
        return Ok(Some(id));
    }
    let notes = format!(
        "core test meta {sha} from \"{}\", section \"{}\", protocol {}; {} {} it ({}{}), shrunk to {} bytes. the bytes are core's own output under its test network passphrase",
        first.file,
        first.section,
        first.protocol.unwrap_or(current),
        target.key(),
        if class == ResultClass::DivergesFromNetwork {
            "drops"
        } else {
            "fails on"
        },
        class.as_str(),
        detail.map(|d| format!(": {d}")).unwrap_or_default(),
        bytes.len()
    );
    let value = json!({
        "caps": fam.caps,
        "expected": {"decode": "must-accept", "roundtrip": true, "validity": "valid"},
        "family": fam.id,
        "fields_exercised": [],
        "flags": ["core-test-dump", target.sdk],
        "id": id,
        "kind": "xdr",
        "linked_issues": [],
        "notes": notes,
        "passphrase_dependent": true,
        "protocol": {"introduced": fam.protocol_introduced, "valid_min": fam.protocol_valid.min, "generated_at": first.protocol.unwrap_or(current)},
        "source": {"class": "core-test-dump"},
        "synthetic": false,
        "variants": {"any": {
            "xdr_file": format!("entries/{}/{id}/any.xdr", fam.id),
            "xdr_sha256": small_sha,
        }},
        "xdr_type": TYPE,
    });
    let entry: shapekit_manifest::Entry = serde_json::from_value(value)
        .with_context(|| format!("{id}: the record is not a manifest entry"))?;
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("any.xdr"), bytes)?;
    let mut text = serde_json::to_string_pretty(&entry)?;
    text.push('\n');
    std::fs::write(dir.join("entry.json"), text)?;
    Ok(Some(id))
}

/// The first family by priority that takes core-test-dump entries and whose fragments the
/// meta's shape signature matches.
fn family_for<'a>(
    families: &'a [shapekit_registry::Family],
    bytes: &[u8],
) -> Option<&'a shapekit_registry::Family> {
    let sig = shapekit_manifest::shape_signature(
        TYPE,
        bytes,
        shapekit_manifest::decode_limits(TYPE, bytes.len()),
    )
    .ok()?;
    let mut ordered: Vec<&shapekit_registry::Family> = families
        .iter()
        .filter(|f| {
            f.source_classes
                .iter()
                .any(|c| c.as_str() == "core-test-dump")
        })
        .collect();
    ordered.sort_by(|a, b| (a.priority, &a.id).cmp(&(b.priority, &b.id)));
    ordered.into_iter().find(|f| {
        f.signature
            .iter()
            .any(|frag| shapekit_manifest::fragment_matches(&sig, frag))
    })
}

pub fn render(r: &DatasetReport) -> String {
    let mut out = format!(
        "# Core's test metas through the SDKs\n\nWritten by `shapekit conformance run --dataset`. {} metas core wrote in its transaction tests{}, {} distinct, in {} test sections, each distinct meta through the SDK's decode and encode.\n\n\
         | testee | distinct | round-trip | fail | beyond max_protocol | sections passing |\n|---|---|---|---|---|---|\n",
        r.metas,
        r.core_image
            .as_deref()
            .map(|i| format!(" ({i})"))
            .unwrap_or_default(),
        r.distinct,
        r.sections
    );
    for t in &r.targets {
        let pass = t.results.get(&ResultClass::Pass).copied().unwrap_or(0);
        let fail: usize = t
            .results
            .iter()
            .filter(|(c, _)| **c != ResultClass::Pass)
            .map(|(_, n)| n)
            .sum();
        out.push_str(&format!(
            "| {} | {} | {pass} | {fail} | {} | {} of {} |\n",
            t.key, t.distinct, t.beyond_protocol, t.sections_passing, r.sections
        ));
    }
    for t in &r.targets {
        if t.dropped.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "\n## Failing in {}\n\n| meta | protocol | sections | result | shrunk to | candidate |\n|---|---|---|---|---|---|\n",
            t.key
        ));
        for d in &t.dropped {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} bytes | {} |\n",
                &d.sha256[..16],
                d.protocol.map_or("current".to_string(), |p| p.to_string()),
                d.sections,
                d.detail.as_deref().unwrap_or("").replace('|', "\\|"),
                d.shrunk_bytes,
                d.entry.as_deref().unwrap_or("no family takes it")
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_dataset(dir: &Path, metas: &[(&str, &str, Option<u32>, &[u8])]) {
        let mut index = String::new();
        for (file, section, protocol, bytes) in metas {
            let sha = shapekit_manifest::sha256_hex(bytes);
            let d = dir.join("meta").join(&sha[..2]);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(format!("{sha}.xdr")), bytes).unwrap();
            index.push_str(
                &json!({"test": "t", "file": file, "section": section, "ordinal": 0,
                        "protocol": protocol, "meta_version": 3, "sha256": sha})
                .to_string(),
            );
            index.push('\n');
        }
        std::fs::write(dir.join("index.jsonl"), index).unwrap();
    }

    #[test]
    fn loads_distinct_metas_sections_and_protocols() {
        let dir = tempfile::tempdir().unwrap();
        let a: &[u8] = &[0, 0, 0, 1];
        let b: &[u8] = &[0, 0, 0, 2];
        write_dataset(
            dir.path(),
            &[
                ("F", "s|protocol version 19", Some(19), a),
                ("F", "s|protocol version 20", Some(20), a),
                ("F", "plain", None, b),
            ],
        );
        let d = Dataset::load(dir.path()).unwrap();
        assert_eq!(d.records.len(), 3);
        assert_eq!(d.metas.len(), 2);
        assert_eq!(d.sections().len(), 3);
        assert_eq!(d.current_protocol(), 20);
        let sha_a = shapekit_manifest::sha256_hex(a);
        assert_eq!(d.protocol(&sha_a), Some(19));
        assert_eq!(d.protocol(&shapekit_manifest::sha256_hex(b)), None);
        assert_eq!(d.bytes(&sha_a).unwrap(), a);
        // a file whose bytes do not match its name is refused
        std::fs::write(
            dir.path()
                .join("meta")
                .join(&sha_a[..2])
                .join(format!("{sha_a}.xdr")),
            b,
        )
        .unwrap();
        assert!(d.bytes(&sha_a).is_err());
    }

    #[test]
    fn a_section_every_version_passes_is_met() {
        let mut sections = BTreeMap::new();
        sections.insert(
            ("F".to_string(), "a".to_string()),
            BTreeSet::from(["x".to_string(), "y".to_string()]),
        );
        sections.insert(
            ("F".to_string(), "b".to_string()),
            BTreeSet::from(["y".to_string()]),
        );
        let report = DatasetReport {
            core_image: None,
            metas: 3,
            distinct: 2,
            sections: 2,
            targets: vec![TargetResult {
                key: "go@v1".to_string(),
                distinct: 2,
                results: BTreeMap::new(),
                beyond_protocol: 0,
                sections_passing: 1,
                dropped: vec![Dropped {
                    sha256: "y".to_string(),
                    protocol: None,
                    meta_version: 3,
                    sections: 2,
                    first_section: String::new(),
                    detail: None,
                    bytes: 4,
                    shrunk_bytes: 4,
                    entry: None,
                }],
            }],
        };
        let w = "dataset:conformance/dataset/dataset-report.json";
        let ledger = obligations(&report, &sections, w);
        assert_eq!(ledger.obligations.len(), 2);
        let a = &ledger.obligations[0];
        assert_eq!(a.id, "core-test-section:F|a");
        assert_eq!(a.status, ObligationStatus::Open);
        assert!(a.evidence.as_deref().unwrap().contains("go@v1 1/2"));
        let b = &ledger.obligations[1];
        assert_eq!(b.status, ObligationStatus::Open);
        assert!(b.witnesses.is_empty());
        // without the failing meta both sections are met through the report
        let mut clean = report.clone();
        clean.targets[0].dropped.clear();
        let ledger = obligations(&clean, &sections, w);
        assert!(
            ledger
                .obligations
                .iter()
                .all(|o| o.status == ObligationStatus::Met && o.witnesses == [w])
        );
        assert!(
            ledger.obligations[1]
                .evidence
                .as_deref()
                .unwrap()
                .ends_with("round-tripped by go@v1")
        );
        use shapekit_contracts::Contract as _;
        assert!(ledger.problems().is_empty());
    }

    #[test]
    fn shrink_candidates_are_one_element_smaller() {
        let meta = TransactionMeta::V3(stellar_xdr::TransactionMetaV3 {
            ext: stellar_xdr::ExtensionPoint::V0,
            tx_changes_before: vec![stellar_xdr::LedgerEntryChange::Removed(
                stellar_xdr::LedgerKey::Ttl(stellar_xdr::LedgerKeyTtl {
                    key_hash: stellar_xdr::Hash([1; 32]),
                }),
            )]
            .try_into()
            .unwrap(),
            operations: vec![stellar_xdr::OperationMeta {
                changes: Default::default(),
            }]
            .try_into()
            .unwrap(),
            tx_changes_after: Default::default(),
            soroban_meta: None,
        });
        let c = candidates(&meta);
        assert_eq!(c.len(), 2);
        let n = meta.to_xdr(Limits::none()).unwrap().len();
        assert!(
            c.iter()
                .all(|m| m.to_xdr(Limits::none()).unwrap().len() < n)
        );
    }
}
