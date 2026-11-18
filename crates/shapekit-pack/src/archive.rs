//! A chain's history archive as one `archive.tar`, and the archive as it stood at a chosen
//! checkpoint.
//!
//! Core gzips every `.xdr.gz` file with a header that differs between runs, so the tar is
//! not reproducible byte for byte while its decompressed contents are. `chain.json`
//! therefore records the SHA-256 of each member's decompressed content,
//! and the tar headers themselves are fixed (mode, owner and time zeroed, members sorted)
//! so the only run-to-run difference is inside the gzip members.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::{Component, Path};

use flate2::read::MultiGzDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{PackError, io_err};

pub const ARCHIVE_TAR: &str = "archive.tar";

/// The root state file of a history archive.
pub const WELL_KNOWN: &str = ".well-known/stellar-history.json";

/// Members above this are treated as corrupt rather than read into memory.
pub const MAX_MEMBER_LEN: u64 = 1 << 30;

/// One member of `archive.tar`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveFile {
    pub path: String,
    /// SHA-256 of the content after gunzip for `.gz` members, of the bytes otherwise.
    pub sha256: String,
    pub gzip: bool,
}

/// What a member path is in core's archive layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Member {
    WellKnown,
    /// `history/xx/xx/xx/history-XXXXXXXX.json`, the state at that checkpoint.
    History(u32),
    /// `ledger`, `transactions`, `results` or `scp` file of a checkpoint.
    Checkpoint {
        category: String,
        checkpoint: u32,
    },
    /// `bucket/xx/xx/xx/bucket-<hash>.xdr.gz`.
    Bucket(String),
}

const CATEGORIES: [&str; 4] = ["ledger", "transactions", "results", "scp"];

/// Places a member path in the archive layout, checking that its directories are the ones
/// core derives from its name.
pub fn classify(path: &str) -> Option<Member> {
    if path == WELL_KNOWN {
        return Some(Member::WellKnown);
    }
    let parts: Vec<&str> = path.split('/').collect();
    let [category, a, b, c, name] = parts.as_slice() else {
        return None;
    };
    let dirs = format!("{a}/{b}/{c}");
    if *category == "history" {
        let hex = name.strip_prefix("history-")?.strip_suffix(".json")?;
        let seq = parse_hex8(hex)?;
        return (dirs == hex_dirs(hex)).then_some(Member::History(seq));
    }
    if *category == "bucket" {
        let hash = name.strip_prefix("bucket-")?.strip_suffix(".xdr.gz")?;
        let ok = hash.len() == 64
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        return (ok && dirs == hex_dirs(hash)).then(|| Member::Bucket(hash.to_string()));
    }
    if CATEGORIES.contains(category) {
        let hex = name
            .strip_prefix(&format!("{category}-"))?
            .strip_suffix(".xdr.gz")?;
        let seq = parse_hex8(hex)?;
        return (dirs == hex_dirs(hex)).then(|| Member::Checkpoint {
            category: category.to_string(),
            checkpoint: seq,
        });
    }
    None
}

fn parse_hex8(hex: &str) -> Option<u32> {
    if hex.len() != 8
        || !hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

fn hex_dirs(hex: &str) -> String {
    format!("{}/{}/{}", &hex[0..2], &hex[2..4], &hex[4..6])
}

fn sha_of(path: &str, bytes: &[u8]) -> Result<(String, bool), PackError> {
    if path.ends_with(".gz") {
        let mut out = Vec::new();
        MultiGzDecoder::new(bytes)
            .take(MAX_MEMBER_LEN + 1)
            .read_to_end(&mut out)
            .map_err(|source| PackError::Io {
                path: format!("gunzip {path}"),
                source,
            })?;
        if out.len() as u64 > MAX_MEMBER_LEN {
            return Err(PackError::Format(format!(
                "{path} decompresses to more than {MAX_MEMBER_LEN} bytes"
            )));
        }
        Ok((hex::encode(Sha256::digest(&out)), true))
    } else {
        Ok((hex::encode(Sha256::digest(bytes)), false))
    }
}

/// Packs the archive directory `src` into the tar file `out`, which must not exist. Every
/// file must sit where core's layout puts it; the list of members with their decompressed
/// hashes is returned in member order.
pub fn bundle_archive(src: &Path, out: &Path) -> Result<Vec<ArchiveFile>, PackError> {
    if out.exists() {
        return Err(PackError::Format(format!(
            "{} already exists",
            out.display()
        )));
    }
    let mut paths = Vec::new();
    collect_files(src, src, &mut paths)?;
    paths.sort();
    let tmp = out.with_extension("tar.partial");
    // a failed bundle leaves nothing behind, so a re-run starts clean
    let written = write_tar(src, &paths, &tmp);
    if written.is_err() {
        // the error that matters is the one being returned
        let _ = std::fs::remove_file(&tmp);
    }
    let files = written?;
    std::fs::rename(&tmp, out).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        io_err(out)(e)
    })?;
    Ok(files)
}

fn write_tar(src: &Path, paths: &[String], tmp: &Path) -> Result<Vec<ArchiveFile>, PackError> {
    let file = File::create(tmp).map_err(io_err(tmp))?;
    let mut builder = tar::Builder::new(file);
    let mut files = Vec::with_capacity(paths.len());
    for rel in paths {
        if classify(rel).is_none() {
            return Err(PackError::Format(format!(
                "{rel} in {} is not part of a history archive",
                src.display()
            )));
        }
        let full = src.join(rel);
        let bytes = std::fs::read(&full).map_err(io_err(&full))?;
        let (sha256, gzip) = sha_of(rel, &bytes)?;
        let mut header = tar::Header::new_ustar();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        builder
            .append_data(&mut header, rel, bytes.as_slice())
            .map_err(io_err(tmp))?;
        files.push(ArchiveFile {
            path: rel.clone(),
            sha256,
            gzip,
        });
    }
    let mut file = builder.into_inner().map_err(io_err(tmp))?;
    file.flush().map_err(io_err(tmp))?;
    file.sync_all().map_err(io_err(tmp))?;
    Ok(files)
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), PackError> {
    for item in std::fs::read_dir(dir).map_err(io_err(dir))? {
        let path = item.map_err(io_err(dir))?.path();
        let ty = std::fs::symlink_metadata(&path)
            .map_err(io_err(&path))?
            .file_type();
        if ty.is_dir() {
            collect_files(root, &path, out)?;
        } else if ty.is_file() {
            let rel = path
                .strip_prefix(root)
                .expect("walked from root")
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            out.push(rel);
        } else {
            return Err(PackError::Format(format!(
                "{} is not a regular file or directory",
                path.display()
            )));
        }
    }
    Ok(())
}

/// Calls `f` with every member of the tar, in order, as (path, bytes).
pub fn for_each_member(
    tar_path: &Path,
    mut f: impl FnMut(&str, Vec<u8>) -> Result<(), PackError>,
) -> Result<(), PackError> {
    let file = File::open(tar_path).map_err(io_err(tar_path))?;
    let mut archive = tar::Archive::new(BufReader::new(file));
    for entry in archive.entries().map_err(io_err(tar_path))? {
        let mut entry = entry.map_err(io_err(tar_path))?;
        if entry.header().entry_type() != tar::EntryType::Regular {
            return Err(PackError::Format(format!(
                "{}: member {:?} is not a regular file",
                tar_path.display(),
                entry.path().map_err(io_err(tar_path))?
            )));
        }
        let path = entry
            .path()
            .map_err(io_err(tar_path))?
            .to_string_lossy()
            .into_owned();
        if entry.size() > MAX_MEMBER_LEN {
            return Err(PackError::Format(format!(
                "{}: member {path} is larger than {MAX_MEMBER_LEN} bytes",
                tar_path.display()
            )));
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes).map_err(io_err(tar_path))?;
        f(&path, bytes)?;
    }
    Ok(())
}

/// Lists the tar's members with their decompressed hashes, the form `chain.json` records.
pub fn list_archive(tar_path: &Path) -> Result<Vec<ArchiveFile>, PackError> {
    let mut out = Vec::new();
    for_each_member(tar_path, |path, bytes| {
        let (sha256, gzip) = sha_of(path, &bytes)?;
        out.push(ArchiveFile {
            path: path.to_string(),
            sha256,
            gzip,
        });
        Ok(())
    })?;
    Ok(out)
}

/// Buckets a history archive state names: `curr`, `snap` and a finished merge's `output`
/// of every level, live and hot archive. All-zero hashes stand for empty buckets, which
/// are never published.
pub fn has_buckets(has: &serde_json::Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for list in ["currentBuckets", "hotArchiveBuckets"] {
        let Some(levels) = has.get(list).and_then(|v| v.as_array()) else {
            continue;
        };
        for level in levels {
            let next = level.get("next");
            for h in [
                level.get("curr"),
                level.get("snap"),
                next.and_then(|n| n.get("output")),
            ]
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
            {
                if h.bytes().any(|b| b != b'0') {
                    out.insert(h.to_string());
                }
            }
        }
    }
    out
}

/// What the archive holds per checkpoint, read from the tar: which checkpoints have a state
/// file, and from which checkpoint on each bucket is referenced.
#[derive(Debug, Clone, Default)]
pub struct ArchiveIndex {
    /// Checkpoint ledger to the bytes of its `history-XXXXXXXX.json`.
    pub states: BTreeMap<u32, Vec<u8>>,
    /// Bucket hash to the first checkpoint whose state references it.
    pub bucket_from: BTreeMap<String, u32>,
}

impl ArchiveIndex {
    pub fn read(tar_path: &Path) -> Result<ArchiveIndex, PackError> {
        let mut idx = ArchiveIndex::default();
        for_each_member(tar_path, |path, bytes| {
            if let Some(Member::History(seq)) = classify(path) {
                idx.states.insert(seq, bytes);
            }
            Ok(())
        })?;
        for (seq, bytes) in &idx.states {
            let has: serde_json::Value = serde_json::from_slice(bytes)
                .map_err(|e| PackError::Format(format!("history state {seq}: {e}")))?;
            let current = has.get("currentLedger").and_then(|v| v.as_u64());
            if current != Some(u64::from(*seq)) {
                return Err(PackError::Format(format!(
                    "history-{seq:08x}.json says currentLedger {current:?}"
                )));
            }
            for b in has_buckets(&has) {
                idx.bucket_from.entry(b).or_insert(*seq);
            }
        }
        Ok(idx)
    }

    /// The newest checkpoint at or below `ledger`.
    pub fn checkpoint_at(&self, ledger: u32) -> Option<u32> {
        self.states.range(..=ledger).next_back().map(|(s, _)| *s)
    }

    /// Whether `member` was already published when `checkpoint` was the archive's newest.
    pub fn visible_at(&self, member: &Member, checkpoint: u32) -> bool {
        match member {
            Member::WellKnown => true,
            Member::History(s) => *s <= checkpoint,
            Member::Checkpoint { checkpoint: s, .. } => *s <= checkpoint,
            Member::Bucket(h) => self.bucket_from.get(h).is_some_and(|s| *s <= checkpoint),
        }
    }
}

/// Materialises in `out` (which must not exist) the archive as it stood when `checkpoint`
/// was its newest: every file of that checkpoint and the ones before it, every bucket their
/// states reference, and `.well-known/stellar-history.json` equal to that checkpoint's
/// state. Returns the files written, relative to `out`. The files are written to a
/// directory beside `out` and moved to `out` only once all of them are there, so a failure
/// part way (a bucket the archive lacks, a bad member) leaves no `out` behind.
pub fn archive_at(tar_path: &Path, checkpoint: u32, out: &Path) -> Result<Vec<String>, PackError> {
    if out.exists() {
        return Err(PackError::Format(format!(
            "{} already exists",
            out.display()
        )));
    }
    let parent = match out.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    std::fs::create_dir_all(parent).map_err(io_err(parent))?;
    let stage = tempfile::Builder::new()
        .prefix(".archive-at-")
        .tempdir_in(parent)
        .map_err(io_err(parent))?;
    let written = write_archive_at(tar_path, checkpoint, stage.path())?;
    std::fs::rename(stage.path(), out).map_err(io_err(out))?;
    // the directory now lives at `out`; there is nothing left for the guard to remove
    let _ = stage.keep();
    Ok(written)
}

fn write_archive_at(
    tar_path: &Path,
    checkpoint: u32,
    out: &Path,
) -> Result<Vec<String>, PackError> {
    let idx = ArchiveIndex::read(tar_path)?;
    let state = idx.states.get(&checkpoint).ok_or_else(|| {
        PackError::Format(format!(
            "{} has no state for checkpoint {checkpoint}; it has {:?}",
            tar_path.display(),
            idx.states.keys().collect::<Vec<_>>()
        ))
    })?;
    let mut written = Vec::new();
    for_each_member(tar_path, |path, bytes| {
        let member = classify(path).ok_or_else(|| {
            PackError::Format(format!("{path} in the archive tar is not an archive file"))
        })?;
        if member == Member::WellKnown || !idx.visible_at(&member, checkpoint) {
            return Ok(());
        }
        write_member(out, path, &bytes)?;
        written.push(path.to_string());
        Ok(())
    })?;
    for (h, _) in idx.bucket_from.iter().filter(|(_, s)| **s <= checkpoint) {
        let p = format!("bucket/{}/bucket-{h}.xdr.gz", hex_dirs(h));
        if !written.contains(&p) {
            return Err(PackError::Format(format!(
                "checkpoint {checkpoint} references bucket {h}, which the archive does not hold"
            )));
        }
    }
    write_member(out, WELL_KNOWN, state)?;
    written.push(WELL_KNOWN.to_string());
    written.sort();
    Ok(written)
}

fn write_member(out: &Path, rel: &str, bytes: &[u8]) -> Result<(), PackError> {
    let rel_path = Path::new(rel);
    if rel_path
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(PackError::Format(format!(
            "{rel} is not a plain relative path"
        )));
    }
    let path = out.join(rel_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io_err(parent))?;
    }
    std::fs::write(&path, bytes).map_err(io_err(&path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_core_layout() {
        assert_eq!(classify(WELL_KNOWN), Some(Member::WellKnown));
        assert_eq!(
            classify("history/00/00/00/history-0000003f.json"),
            Some(Member::History(63))
        );
        assert_eq!(
            classify("ledger/00/00/00/ledger-0000007f.xdr.gz"),
            Some(Member::Checkpoint {
                category: "ledger".into(),
                checkpoint: 127
            })
        );
        let h = "ab".repeat(32);
        assert_eq!(
            classify(&format!("bucket/ab/ab/ab/bucket-{h}.xdr.gz")),
            Some(Member::Bucket(h.clone()))
        );
        // directories must follow from the name
        assert_eq!(classify("history/00/00/01/history-0000003f.json"), None);
        assert_eq!(
            classify(&format!("bucket/ab/ab/ac/bucket-{h}.xdr.gz")),
            None
        );
        assert_eq!(classify("history/00/00/00/history-0000003F.json"), None);
        assert_eq!(classify("other/00/00/00/x"), None);
    }

    #[test]
    fn buckets_skip_empty_hashes() {
        let z = "0".repeat(64);
        let a = "a".repeat(64);
        let b = "b".repeat(64);
        let has = serde_json::json!({
            "currentBuckets": [
                {"curr": a, "snap": z, "next": {"state": 0}},
                {"curr": z, "snap": z, "next": {"state": 1, "output": b}},
            ]
        });
        let got: Vec<String> = has_buckets(&has).into_iter().collect();
        assert_eq!(got, vec![a, b]);
    }
}
