//! Converting a recorded chain directory to its committed form, and checking one.
//!
//! The recorder writes `chain.json` (schema 1), core's raw stream `ledgers.xdr`, the
//! canonical stream `ledgers.canonical.xdr` and the archive directory `archive/`. The
//! committed form (schema 2) keeps `ledgers.xdr` as provenance, holds the canonical ledgers
//! as a SEP-54 lake in `ledgers/` and the archive as `archive.tar`, and records both in
//! `chain.json`. Nothing else is in a committed chain directory.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shapekit_manifest::framing::FramedReader;

use crate::archive::{ARCHIVE_TAR, ArchiveFile, ArchiveIndex, bundle_archive, list_archive};
use crate::lake::{
    COMPRESSION, CONFIG_FILE, Lake, LakeConfig, WrittenBatch, ZSTD_LEVEL, write_sep54, zstd_library,
};
use crate::layout::Layout;
use crate::{PackError, io_err};

pub const CHAIN_FILE: &str = "chain.json";
pub const LEDGERS_DIR: &str = "ledgers";
pub const RAW_STREAM_FILE: &str = "ledgers.xdr";
pub const CANONICAL_STREAM_FILE: &str = "ledgers.canonical.xdr";
pub const ARCHIVE_DIR: &str = "archive";

/// Upper bound for one framed ledger in a stream, the recorder's own bound.
pub const MAX_FRAME_LEN: usize = 512 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreImage {
    pub repository: String,
    pub tag: String,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloseSchedule {
    pub epoch: u64,
    pub step: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerRange {
    pub first: u32,
    pub last: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerRecord {
    pub ledger_seq: u32,
    /// SHA-256 of the canonical `LedgerCloseMeta`, the one the lake holds.
    pub sha256: String,
    /// SHA-256 of the meta as core wrote it.
    pub raw_sha256: String,
    pub xdr_len: usize,
}

/// `chain.json` as the recorder writes it. Unknown fields are refused so a field added to
/// the recorder cannot be dropped silently on conversion.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedChain {
    pub schema: u32,
    pub protocol: u32,
    pub passphrase: String,
    pub network_id: String,
    pub validator: String,
    pub core_image: CoreImage,
    pub close_schedule: CloseSchedule,
    pub ledger_range: LedgerRange,
    pub checkpoints: Vec<u32>,
    pub normalization: String,
    pub canonical_stream_file: String,
    pub canonical_stream_sha256: String,
    pub stream_file: String,
    pub stream_sha256: String,
    pub ledgers: Vec<LedgerRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LakeRecord {
    pub dir: String,
    pub ledgers_per_batch: u32,
    pub batches_per_partition: u32,
    pub compression: String,
    pub zstd_level: i32,
    pub zstd_library: String,
    pub batches: Vec<WrittenBatch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveRecord {
    pub file: String,
    /// Members with the SHA-256 of their decompressed content; the tar's own bytes differ
    /// between runs (see [`crate::archive`]).
    pub files: Vec<ArchiveFile>,
}

/// `chain.json` of a committed chain. Field order is the serialisation order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChainFile {
    pub schema: u32,
    pub protocol: u32,
    pub passphrase: String,
    pub network_id: String,
    pub validator: String,
    pub core_image: CoreImage,
    pub close_schedule: CloseSchedule,
    pub ledger_range: LedgerRange,
    pub checkpoints: Vec<u32>,
    /// How the lake's ledgers order change lists (`key-stable`).
    pub normalization: String,
    pub lake: LakeRecord,
    /// SHA-256 of the lake's ledgers as one record-marked stream, the canonical stream the
    /// recorder hashed.
    pub canonical_stream_sha256: String,
    /// Core's raw stream, kept as written.
    pub stream_file: String,
    pub stream_sha256: String,
    pub archive: ArchiveRecord,
    pub ledgers: Vec<LedgerRecord>,
}

pub const SCHEMA: u32 = 2;

impl ChainFile {
    pub fn read(dir: &Path) -> Result<ChainFile, PackError> {
        let path = dir.join(CHAIN_FILE);
        let bytes = std::fs::read(&path).map_err(io_err(&path))?;
        let v: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| PackError::Format(format!("{}: {e}", path.display())))?;
        let schema = v.get("schema").and_then(|s| s.as_u64());
        if schema != Some(u64::from(SCHEMA)) {
            return Err(PackError::Format(format!(
                "{}: schema {schema:?}, a committed chain has schema {SCHEMA}; run `shapekit pack sep54` on it",
                path.display()
            )));
        }
        serde_json::from_value(v).map_err(|e| PackError::Format(format!("{}: {e}", path.display())))
    }

    fn write(&self, dir: &Path) -> Result<(), PackError> {
        let mut text = serde_json::to_string_pretty(self).expect("chain.json is plain json");
        text.push('\n');
        let path = dir.join(CHAIN_FILE);
        let tmp = dir.join(".chain.json.partial");
        std::fs::write(&tmp, text).map_err(io_err(&tmp))?;
        std::fs::rename(&tmp, &path).map_err(io_err(&path))
    }

    pub fn layout(&self) -> Result<Layout, PackError> {
        Layout::new(self.lake.ledgers_per_batch, self.lake.batches_per_partition)
    }
}

/// What `pack_chain` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packed {
    Converted { ledgers: usize, batches: usize },
    AlreadyPacked,
}

/// SHA-256 of a file, read in pieces.
pub fn sha256_file(path: &Path) -> Result<String, PackError> {
    let mut f = BufReader::new(File::open(path).map_err(io_err(path))?);
    let mut h = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).map_err(io_err(path))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

fn frame_header(len: usize) -> [u8; 4] {
    (0x8000_0000 | len as u32).to_be_bytes()
}

/// Converts a recorded chain directory in place to the committed form. A chain already in
/// the committed form is left alone. Every canonical ledger is checked against the hash the
/// recorder wrote before anything is removed.
pub fn pack_chain(dir: &Path, layout: Layout) -> Result<Packed, PackError> {
    let path = dir.join(CHAIN_FILE);
    let bytes = std::fs::read(&path).map_err(io_err(&path))?;
    let v: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| PackError::Format(format!("{}: {e}", path.display())))?;
    match v.get("schema").and_then(|s| s.as_u64()) {
        Some(1) => {}
        Some(s) if s == u64::from(SCHEMA) => return Ok(Packed::AlreadyPacked),
        other => {
            return Err(PackError::Format(format!(
                "{}: unknown schema {other:?}",
                path.display()
            )));
        }
    }
    let rec: RecordedChain = serde_json::from_value(v)
        .map_err(|e| PackError::Format(format!("{}: {e}", path.display())))?;
    if rec.canonical_stream_file != CANONICAL_STREAM_FILE || rec.stream_file != RAW_STREAM_FILE {
        return Err(PackError::Format(format!(
            "{}: stream files {:?} and {:?} are not the recorder's",
            path.display(),
            rec.canonical_stream_file,
            rec.stream_file
        )));
    }

    let canonical_path = dir.join(CANONICAL_STREAM_FILE);
    let stream_sha = sha256_file(&canonical_path)?;
    if stream_sha != rec.canonical_stream_sha256 {
        return Err(PackError::Format(format!(
            "{}: sha256 {stream_sha}, chain.json says {}",
            canonical_path.display(),
            rec.canonical_stream_sha256
        )));
    }
    let mut expected = rec.ledgers.iter();
    let frames = FramedReader::new(
        BufReader::new(File::open(&canonical_path).map_err(io_err(&canonical_path))?),
        MAX_FRAME_LEN,
    );
    let checked = frames.map(|f| {
        let xdr = f.map_err(|e| PackError::Format(format!("{}: {e}", canonical_path.display())))?;
        let want = expected.next().ok_or_else(|| {
            PackError::Format(format!(
                "{} holds more ledgers than chain.json lists",
                canonical_path.display()
            ))
        })?;
        let sha = hex::encode(Sha256::digest(&xdr));
        if sha != want.sha256 || xdr.len() != want.xdr_len {
            return Err(PackError::Format(format!(
                "canonical ledger {} has sha256 {sha}, chain.json says {}",
                want.ledger_seq, want.sha256
            )));
        }
        Ok(xdr)
    });
    let lake_dir = dir.join(LEDGERS_DIR);
    let staging = dir.join(".ledgers.partial");
    if staging.exists() {
        std::fs::remove_dir_all(&staging).map_err(io_err(&staging))?;
    }
    let summary = write_sep54(checked, &rec.passphrase, &staging, layout)?;
    if expected.next().is_some() {
        return Err(PackError::Format(format!(
            "{} holds fewer ledgers than chain.json lists",
            canonical_path.display()
        )));
    }
    let first = summary.batches.first().map(|b| b.first);
    let last = summary.batches.last().map(|b| b.last);
    if first != Some(rec.ledger_range.first) || last != Some(rec.ledger_range.last) {
        return Err(PackError::Format(format!(
            "the canonical stream runs {first:?} to {last:?}, chain.json says {} to {}",
            rec.ledger_range.first, rec.ledger_range.last
        )));
    }

    let archive_src = dir.join(ARCHIVE_DIR);
    let tar_path = dir.join(ARCHIVE_TAR);
    let files = bundle_archive(&archive_src, &tar_path)?;
    let idx = ArchiveIndex::read(&tar_path)?;
    for c in &rec.checkpoints {
        if !idx.states.contains_key(c) {
            return Err(PackError::Format(format!(
                "chain.json lists checkpoint {c}, the archive has no state for it"
            )));
        }
    }

    if lake_dir.exists() {
        return Err(PackError::Format(format!(
            "{} already exists",
            lake_dir.display()
        )));
    }
    std::fs::rename(&staging, &lake_dir).map_err(io_err(&lake_dir))?;
    let batches = summary.batches.len();
    let chain = ChainFile {
        schema: SCHEMA,
        protocol: rec.protocol,
        passphrase: rec.passphrase,
        network_id: rec.network_id,
        validator: rec.validator,
        core_image: rec.core_image,
        close_schedule: rec.close_schedule,
        ledger_range: rec.ledger_range,
        checkpoints: rec.checkpoints,
        normalization: rec.normalization,
        lake: LakeRecord {
            dir: LEDGERS_DIR.into(),
            ledgers_per_batch: layout.ledgers_per_batch(),
            batches_per_partition: layout.batches_per_partition(),
            compression: COMPRESSION.into(),
            zstd_level: ZSTD_LEVEL,
            zstd_library: zstd_library().into(),
            batches: summary.batches,
        },
        canonical_stream_sha256: rec.canonical_stream_sha256,
        stream_file: rec.stream_file,
        stream_sha256: rec.stream_sha256,
        archive: ArchiveRecord {
            file: ARCHIVE_TAR.into(),
            files,
        },
        ledgers: rec.ledgers,
    };
    chain.write(dir)?;
    std::fs::remove_file(&canonical_path).map_err(io_err(&canonical_path))?;
    std::fs::remove_dir_all(&archive_src).map_err(io_err(&archive_src))?;
    Ok(Packed::Converted {
        ledgers: chain.ledgers.len(),
        batches,
    })
}

/// The lake of a committed chain, after checking its config agrees with `chain.json`.
pub fn open_lake(dir: &Path, chain: &ChainFile) -> Result<Lake, PackError> {
    let lake = Lake::open(&dir.join(&chain.lake.dir))?;
    let want = LakeConfig::new(&chain.passphrase, chain.layout()?);
    if lake.config() != &want {
        return Err(PackError::Format(format!(
            "{}/{CONFIG_FILE} is {:?}, chain.json implies {want:?}",
            chain.lake.dir,
            lake.config()
        )));
    }
    Ok(lake)
}

/// Checks a committed chain directory against its `chain.json`: the lake's config, batch
/// files and every ledger hash, the canonical stream hash, core's raw stream, the archive
/// members and their decompressed hashes, and that nothing else is in the directory.
/// Returns every problem found.
pub fn verify_chain(dir: &Path) -> Result<Vec<String>, PackError> {
    let chain = ChainFile::read(dir)?;
    let mut problems = Vec::new();

    let mut extra: Vec<String> = Vec::new();
    for item in std::fs::read_dir(dir).map_err(io_err(dir))? {
        let name = item
            .map_err(io_err(dir))?
            .file_name()
            .to_string_lossy()
            .into_owned();
        if ![
            CHAIN_FILE,
            chain.lake.dir.as_str(),
            chain.stream_file.as_str(),
            chain.archive.file.as_str(),
        ]
        .contains(&name.as_str())
        {
            extra.push(name);
        }
    }
    extra.sort();
    for e in extra {
        problems.push(format!("{e}: not part of a committed chain"));
    }
    if chain.lake.compression != COMPRESSION {
        problems.push(format!("lake compression {:?}", chain.lake.compression));
    }

    let lake = open_lake(dir, &chain)?;
    let found: Vec<&str> = lake.batches().iter().map(|b| b.key.as_str()).collect();
    let listed: Vec<&str> = chain.lake.batches.iter().map(|b| b.key.as_str()).collect();
    if found != listed {
        problems.push(format!(
            "lake objects {found:?} differ from the ones chain.json lists {listed:?}"
        ));
    }
    for b in &chain.lake.batches {
        let p = lake.root().join(&b.key);
        match std::fs::read(&p) {
            Ok(bytes) => {
                let sha = hex::encode(Sha256::digest(&bytes));
                if sha != b.sha256 {
                    problems.push(format!(
                        "{}: sha256 {sha}, chain.json says {}",
                        b.key, b.sha256
                    ));
                }
            }
            Err(e) => problems.push(format!("{}: {e}", b.key)),
        }
    }
    let mut stream = Sha256::new();
    let mut records = chain.ledgers.iter();
    let mut count = 0usize;
    for item in lake.ledgers() {
        let l = item?;
        count += 1;
        stream.update(frame_header(l.xdr.len()));
        stream.update(&l.xdr);
        match records.next() {
            Some(r) if r.ledger_seq == l.seq => {
                let sha = hex::encode(Sha256::digest(&l.xdr));
                if sha != r.sha256 || l.xdr.len() != r.xdr_len {
                    problems.push(format!(
                        "lake ledger {}: sha256 {sha}, chain.json says {}",
                        l.seq, r.sha256
                    ));
                }
            }
            Some(r) => problems.push(format!(
                "lake ledger {} where chain.json lists {}",
                l.seq, r.ledger_seq
            )),
            None => problems.push(format!("lake ledger {} is not in chain.json", l.seq)),
        }
    }
    if count != chain.ledgers.len() {
        problems.push(format!(
            "the lake holds {count} ledgers, chain.json lists {}",
            chain.ledgers.len()
        ));
    }
    let first = chain.ledgers.first().map(|r| r.ledger_seq);
    let last = chain.ledgers.last().map(|r| r.ledger_seq);
    if first != Some(chain.ledger_range.first) || last != Some(chain.ledger_range.last) {
        problems.push(format!(
            "ledgers run {first:?} to {last:?}, ledger_range says {} to {}",
            chain.ledger_range.first, chain.ledger_range.last
        ));
    }
    let sha = hex::encode(stream.finalize());
    if sha != chain.canonical_stream_sha256 {
        problems.push(format!(
            "the lake's ledgers as one stream hash to {sha}, chain.json says {}",
            chain.canonical_stream_sha256
        ));
    }

    let raw_path = dir.join(&chain.stream_file);
    let sha = sha256_file(&raw_path)?;
    if sha != chain.stream_sha256 {
        problems.push(format!(
            "{}: sha256 {sha}, chain.json says {}",
            chain.stream_file, chain.stream_sha256
        ));
    }
    let mut records = chain.ledgers.iter();
    let raw = BufReader::new(File::open(&raw_path).map_err(io_err(&raw_path))?);
    for f in FramedReader::new(raw, MAX_FRAME_LEN) {
        let xdr = f.map_err(|e| PackError::Format(format!("{}: {e}", chain.stream_file)))?;
        let Some(r) = records.next() else {
            problems.push(format!(
                "{} holds more ledgers than chain.json",
                chain.stream_file
            ));
            break;
        };
        let sha = hex::encode(Sha256::digest(&xdr));
        if sha != r.raw_sha256 {
            problems.push(format!(
                "{} ledger {}: sha256 {sha}, chain.json says {}",
                chain.stream_file, r.ledger_seq, r.raw_sha256
            ));
        }
    }
    if records.next().is_some() {
        problems.push(format!(
            "{} holds fewer ledgers than chain.json",
            chain.stream_file
        ));
    }

    let tar_path = dir.join(&chain.archive.file);
    let members = list_archive(&tar_path)?;
    if members != chain.archive.files {
        let a: Vec<_> = members.iter().map(|m| (&m.path, &m.sha256)).collect();
        let b: Vec<_> = chain
            .archive
            .files
            .iter()
            .map(|m| (&m.path, &m.sha256))
            .collect();
        problems.push(format!(
            "{} members differ from chain.json: found {a:?}, listed {b:?}",
            chain.archive.file
        ));
    }
    let idx = ArchiveIndex::read(&tar_path)?;
    for c in &chain.checkpoints {
        if !idx.states.contains_key(c) {
            problems.push(format!("checkpoint {c} has no state in the archive"));
        }
    }
    Ok(problems)
}
