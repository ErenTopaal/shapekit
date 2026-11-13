//! Records one chain from a running node: every `LedgerCloseMeta` from
//! ledger 2 (the first ledger after genesis) on, the node's history archive,
//! and a `chain.json` describing both.
//!
//! The raw framed stream is kept as core wrote it (`ledgers.xdr`), as the
//! provenance record. Beside it the recorder writes the canonical stream
//! (`ledgers.canonical.xdr`): the same LCMs with their change lists stably
//! sorted by ledger key, each key's changes in core's order (see
//! [`crate::canonical`]).
//! Release core orders those lists differently in every process, so only
//! the canonical stream is reproducible byte for byte, and the corpus is
//! built from it. Writing SEP-54 batches is a separate step.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use sha2::{Digest, Sha256};

use stellar_xdr::{Limits, WriteXdr};

use crate::canonical::{canonicalize_lcm, change_multisets, per_key_sequences, permutation_equal};
use crate::config::{CHAIN_EPOCH, CLOSE_STEP, CoreImage, node_public_key};
use crate::keys::network_id;
use crate::meta::{FramedLcm, MetaError, MetaStream};
use crate::node::{CoreNode, NodeError, checkpoints_through};

/// First ledger core closes after genesis, and so the first in every chain.
pub const FIRST_LEDGER: u32 = 2;

pub const CHAIN_FILE: &str = "chain.json";
pub const STREAM_FILE: &str = "ledgers.xdr";
pub const CANONICAL_STREAM_FILE: &str = "ledgers.canonical.xdr";

/// Names the ordering rule applied to the canonical stream, recorded in
/// `chain.json` so a reader knows the canonical bytes are not core's raw
/// output order.
pub const NORMALIZATION: &str = "key-stable";
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
    #[error("canonicalizing ledger {ledger_seq}: {source}")]
    Canonical {
        ledger_seq: u32,
        source: stellar_xdr::Error,
    },
    #[error("ledger {0}: canonical meta is not a permutation of core's meta")]
    NotPermutation(u32),
    #[error("ledger {0}: canonical meta reorders the changes of one ledger key")]
    KeyOrder(u32),
    #[error(
        "the archive holds checkpoints {found:?}, but the ledgers through {last} need exactly {expected:?}"
    )]
    Checkpoints {
        last: u32,
        expected: Vec<u32>,
        found: Vec<u32>,
    },
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
    /// SHA-256 of the canonical `LedgerCloseMeta` XDR, without the frame
    /// header. Reproducible across runs.
    pub sha256: String,
    /// SHA-256 of the meta exactly as core wrote it. Differs between runs
    /// whenever a change list holds two or more entries.
    pub raw_sha256: String,
    /// Length of the XDR; the same for both forms, which only differ in order.
    pub xdr_len: usize,
}

/// One ledger's meta in canonical form.
#[derive(Debug, Clone)]
pub struct CanonicalLcm {
    pub ledger_seq: u32,
    pub xdr: Vec<u8>,
    pub meta: stellar_xdr::LedgerCloseMeta,
}

/// Canonical form of a recorded frame, checked against it by [`check_canonical`].
pub fn canonical_frame(f: &FramedLcm) -> Result<CanonicalLcm, RecorderError> {
    let err = |source| RecorderError::Canonical {
        ledger_seq: f.ledger_seq,
        source,
    };
    let mut meta = f.meta.clone();
    canonicalize_lcm(&mut meta).map_err(err)?;
    check_canonical(f.ledger_seq, &f.meta, &meta)?;
    let xdr = meta.to_xdr(Limits::none()).map_err(err)?;
    if xdr.len() != f.xdr.len() {
        return Err(RecorderError::NotPermutation(f.ledger_seq));
    }
    Ok(CanonicalLcm {
        ledger_seq: f.ledger_seq,
        xdr,
        meta,
    })
}

/// Checks a canonical LCM against core's raw one: the same changes in every list, and each
/// key's changes in core's order. The second check does not go through
/// [`canonicalize_lcm`], so it also catches that function reordering one key's changes,
/// which [`permutation_equal`] (it canonicalizes both sides) cannot.
pub fn check_canonical(
    ledger_seq: u32,
    raw: &stellar_xdr::LedgerCloseMeta,
    canonical: &stellar_xdr::LedgerCloseMeta,
) -> Result<(), RecorderError> {
    let err = |source| RecorderError::Canonical { ledger_seq, source };
    // the same changes first, so a key order failure below means order and nothing else
    if change_multisets(raw).map_err(err)? != change_multisets(canonical).map_err(err)? {
        return Err(RecorderError::NotPermutation(ledger_seq));
    }
    if per_key_sequences(raw) != per_key_sequences(canonical) {
        return Err(RecorderError::KeyOrder(ledger_seq));
    }
    if !permutation_equal(raw, canonical).map_err(err)? {
        return Err(RecorderError::NotPermutation(ledger_seq));
    }
    Ok(())
}

/// Frames `values` with single-fragment record marking, as core does. Every
/// value here has the length of a frame the reader accepted, which
/// `MAX_FRAME_LEN` keeps far below the 31-bit limit.
pub fn frame_stream<'a>(values: impl IntoIterator<Item = &'a [u8]>) -> Vec<u8> {
    let mut out = Vec::new();
    for v in values {
        let len = u32::try_from(v.len())
            .ok()
            .filter(|l| *l <= crate::meta::MAX_FRAME_LEN)
            .expect("lcm longer than MAX_FRAME_LEN");
        out.extend((0x8000_0000 | len).to_be_bytes());
        out.extend_from_slice(v);
    }
    out
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
    /// How the canonical stream orders change lists, [`NORMALIZATION`].
    pub normalization: String,
    /// The canonical stream, reproducible byte for byte.
    pub canonical_stream_file: String,
    pub canonical_stream_sha256: String,
    /// Core's raw stream, kept as written.
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
    /// last recorded ledger is published, and `chain.json`. Halts core before
    /// copying the archive, so nothing can write to it during the copy, and
    /// checks that the copy holds exactly the checkpoints the ledgers need. The
    /// node takes no more commands afterwards; the caller still stops it to
    /// remove its container and network.
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

        let canonical = self
            .frames
            .iter()
            .map(canonical_frame)
            .collect::<Result<Vec<_>, _>>()?;
        let canonical_stream = frame_stream(canonical.iter().map(|c| c.xdr.as_slice()));
        let canonical_path = target.join(CANONICAL_STREAM_FILE);
        std::fs::write(&canonical_path, &canonical_stream).map_err(io(&canonical_path))?;

        node.halt()?;
        let archive = target.join(ARCHIVE_DIR);
        copy_dir(&node.history_dir(), &archive)?;
        let checkpoints = archive_checkpoints(&archive)?;
        if checkpoints != expected_checkpoints {
            return Err(RecorderError::Checkpoints {
                last,
                expected: expected_checkpoints,
                found: checkpoints,
            });
        }

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
            normalization: NORMALIZATION.to_owned(),
            canonical_stream_file: CANONICAL_STREAM_FILE.to_owned(),
            canonical_stream_sha256: hex::encode(Sha256::digest(&canonical_stream)),
            stream_file: STREAM_FILE.to_owned(),
            stream_sha256: hex::encode(Sha256::digest(&raw)),
            ledgers: self
                .frames
                .iter()
                .zip(&canonical)
                .map(|(f, c)| LedgerRecord {
                    ledger_seq: f.ledger_seq,
                    sha256: hex::encode(Sha256::digest(&c.xdr)),
                    raw_sha256: hex::encode(Sha256::digest(&f.xdr)),
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
