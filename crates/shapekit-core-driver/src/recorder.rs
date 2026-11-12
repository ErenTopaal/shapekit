//! Records one chain from a running node: every `LedgerCloseMeta` from
//! ledger 2 (the first ledger after genesis) on, the node's history archive,
//! and a `chain.json` describing both.
//!
//! The raw framed stream is kept as core wrote it (`ledgers.xdr`). Writing
//! SEP-54 batches from it is a separate step.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::config::{CHAIN_EPOCH, CLOSE_STEP, CoreImage, node_public_key};
use crate::keys::network_id;
use crate::meta::{FramedLcm, MetaError, MetaStream};
use crate::node::{CoreNode, NodeError, checkpoints_through};

/// First ledger core closes after genesis, and so the first in every chain.
pub const FIRST_LEDGER: u32 = 2;

pub const CHAIN_FILE: &str = "chain.json";
pub const STREAM_FILE: &str = "ledgers.xdr";
pub const ARCHIVE_DIR: &str = "archive";

/// How long to wait for a closed ledger's meta or a checkpoint publish.
pub const WAIT: Duration = Duration::from_secs(60);

#[derive(Debug, thiserror::Error)]
pub enum RecorderError {
    #[error(transparent)]
    Meta(#[from] MetaError),
    #[error(transparent)]
    Node(#[from] NodeError),
    #[error("meta stream gap: expected ledger {expected}, got {got}")]
    Gap { expected: u32, got: u32 },
    #[error("no ledgers recorded")]
    Empty,
    #[error("{path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("target {0} already exists")]
    TargetExists(String),
    #[error("serialising chain.json: {0}")]
    Json(#[from] serde_json::Error),
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> RecorderError + '_ {
    move |source| RecorderError::Io {
        path: path.display().to_string(),
        source,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LedgerRecord {
    pub ledger_seq: u32,
    /// SHA-256 of the `LedgerCloseMeta` XDR, without the frame header.
    pub sha256: String,
    pub xdr_len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LedgerRange {
    pub first: u32,
    pub last: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CloseSchedule {
    pub epoch: u64,
    pub step: u64,
}

/// Contents of `chain.json`. Field order is the serialisation order, so the
/// file is byte-stable for a byte-stable chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChainSummary {
    pub schema: u32,
    pub protocol: u32,
    pub passphrase: String,
    pub network_id: String,
    pub validator: String,
    pub core_image: CoreImage,
    pub close_schedule: CloseSchedule,
    pub ledger_range: LedgerRange,
    /// Checkpoint ledgers whose files are in the archive.
    pub checkpoints: Vec<u32>,
    pub stream_file: String,
    pub stream_sha256: String,
    pub ledgers: Vec<LedgerRecord>,
}

/// Collects meta from a node as ledgers close.
#[derive(Debug)]
pub struct ChainRecorder {
    stream: MetaStream,
    frames: Vec<FramedLcm>,
}

impl ChainRecorder {
    /// Starts reading the node's meta stream from its first byte. Create it
    /// right after `CoreNode::start`, before anything is closed.
    pub fn new(node: &CoreNode) -> Self {
        Self {
            stream: MetaStream::new(node.meta_path()),
            frames: Vec::new(),
        }
    }

    pub fn frames(&self) -> &[FramedLcm] {
        &self.frames
    }

    fn push(&mut self, f: FramedLcm) -> Result<(), RecorderError> {
        let expected = self
            .frames
            .last()
            .map_or(FIRST_LEDGER, |l| l.ledger_seq + 1);
        if f.ledger_seq != expected {
            return Err(RecorderError::Gap {
                expected,
                got: f.ledger_seq,
            });
        }
        self.frames.push(f);
        Ok(())
    }

    /// Reads meta until ledger `seq` has been recorded.
    pub fn collect_through(&mut self, seq: u32) -> Result<(), RecorderError> {
        if self.frames.last().is_some_and(|l| l.ledger_seq >= seq) {
            return Ok(());
        }
        for f in self.stream.wait_for(seq, WAIT)? {
            self.push(f)?;
        }
        Ok(())
    }

    /// Writes the chain into `target`, which must not exist yet: the framed
    /// stream, a copy of the history archive once every checkpoint up to the
    /// last recorded ledger is published, and `chain.json`.
    pub fn finish(mut self, node: &CoreNode, target: &Path) -> Result<ChainSummary, RecorderError> {
        for f in self.stream.drain()? {
            self.push(f)?;
        }
        let last = self.frames.last().ok_or(RecorderError::Empty)?.ledger_seq;
        let expected_checkpoints = checkpoints_through(last);
        if let Some(c) = expected_checkpoints.last() {
            node.wait_published(*c, WAIT)?;
        }

        if target.exists() {
            return Err(RecorderError::TargetExists(target.display().to_string()));
        }
        std::fs::create_dir_all(target).map_err(io(target))?;

        // exactly the frames recorded, even if core appended more meanwhile
        let end = self.stream.offset();
        let mut raw = std::fs::read(self.stream.path()).map_err(io(self.stream.path()))?;
        raw.truncate(end as usize);
        let stream_path = target.join(STREAM_FILE);
        std::fs::write(&stream_path, &raw).map_err(io(&stream_path))?;

        let archive = target.join(ARCHIVE_DIR);
        copy_dir(&node.history_dir(), &archive)?;
        let checkpoints = archive_checkpoints(&archive)?;

        let cfg = node.config();
        let summary = ChainSummary {
            schema: 1,
            protocol: cfg.protocol,
            passphrase: cfg.passphrase.clone(),
            network_id: hex::encode(network_id(&cfg.passphrase)),
            validator: node_public_key(&cfg.passphrase),
            core_image: cfg.image.clone(),
            close_schedule: CloseSchedule {
                epoch: CHAIN_EPOCH,
                step: CLOSE_STEP,
            },
            ledger_range: LedgerRange {
                first: FIRST_LEDGER,
                last,
            },
            checkpoints,
            stream_file: STREAM_FILE.to_owned(),
            stream_sha256: hex::encode(Sha256::digest(&raw)),
            ledgers: self
                .frames
                .iter()
                .map(|f| LedgerRecord {
                    ledger_seq: f.ledger_seq,
                    sha256: hex::encode(Sha256::digest(&f.xdr)),
                    xdr_len: f.xdr.len(),
                })
                .collect(),
        };
        let mut json = serde_json::to_string_pretty(&summary)?;
        json.push('\n');
        let chain_path = target.join(CHAIN_FILE);
        std::fs::write(&chain_path, json).map_err(io(&chain_path))?;
        Ok(summary)
    }
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), RecorderError> {
    std::fs::create_dir_all(to).map_err(io(to))?;
    let mut entries: Vec<PathBuf> = std::fs::read_dir(from)
        .map_err(io(from))?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<_, _>>()
        .map_err(io(from))?;
    entries.sort();
    for src in entries {
        let name = src.file_name().expect("read_dir entries have names");
        let dst = to.join(name);
        let ty = std::fs::symlink_metadata(&src)
            .map_err(io(&src))?
            .file_type();
        if ty.is_dir() {
            copy_dir(&src, &dst)?;
        } else if ty.is_file() {
            std::fs::copy(&src, &dst).map_err(io(&src))?;
        } else {
            return Err(RecorderError::Io {
                path: src.display().to_string(),
                source: std::io::Error::other("not a regular file or directory"),
            });
        }
    }
    Ok(())
}

/// Checkpoint ledgers that have a `ledger-XXXXXXXX.xdr.gz` file.
pub fn archive_checkpoints(archive: &Path) -> Result<Vec<u32>, RecorderError> {
    let mut out = Vec::new();
    let ledger_dir = archive.join("ledger");
    if !ledger_dir.exists() {
        return Ok(out);
    }
    let mut stack = vec![ledger_dir];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).map_err(io(&dir))? {
            let p = e.map_err(io(&dir))?.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Some(hex_seq) = p
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_prefix("ledger-"))
                .and_then(|n| n.strip_suffix(".xdr.gz"))
            {
                let seq = u32::from_str_radix(hex_seq, 16).map_err(|_| RecorderError::Io {
                    path: p.display().to_string(),
                    source: std::io::Error::other("ledger file name is not a hex sequence"),
                })?;
                out.push(seq);
            }
        }
    }
    out.sort_unstable();
    Ok(out)
}
