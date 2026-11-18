//! Incremental reader for core's `METADATA_OUTPUT_STREAM`.
//!
//! Core writes one `LedgerCloseMeta` per closed ledger, each as a single XDR
//! record-marked frame (RFC 5531 section 11): a 4-byte big-endian header whose
//! high bit flags the last fragment and whose low 31 bits are the fragment
//! length, then the value. Core always writes whole records in one fragment,
//! the same framing as its `--check-lcm` goldens. The reader keeps a byte
//! offset so it can be polled while core is still appending, and only
//! consumes a frame once all of its bytes are on disk.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use stellar_xdr::{LedgerCloseMeta, Limits, ReadXdr};

/// Frames larger than this are treated as corruption. Core caps a ledger's
/// meta well below it.
pub const MAX_FRAME_LEN: u32 = 512 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum MetaError {
    #[error("reading meta stream {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error(
        "meta stream {path} at offset {offset}: frame is not marked as the last fragment (header {header:#010x})"
    )]
    Fragmented {
        path: String,
        offset: u64,
        header: u32,
    },
    #[error("meta stream {path} at offset {offset}: frame length {len} over {MAX_FRAME_LEN}")]
    TooLong { path: String, offset: u64, len: u32 },
    #[error("meta stream {path} at offset {offset}: {source}")]
    Decode {
        path: String,
        offset: u64,
        source: stellar_xdr::Error,
    },
    #[error("meta stream {path}: {len} trailing bytes do not form a whole frame")]
    Truncated { path: String, len: u64 },
    #[error("meta stream {path}: no ledger {seq} after {waited:?} (last seen {last:?})")]
    Timeout {
        path: String,
        seq: u32,
        waited: Duration,
        last: Option<u32>,
    },
}

/// One decoded frame.
#[derive(Debug, Clone)]
pub struct FramedLcm {
    pub ledger_seq: u32,
    /// Offset of the frame header in the stream file.
    pub offset: u64,
    /// The XDR value without the frame header, exactly as core wrote it.
    pub xdr: Vec<u8>,
    pub meta: LedgerCloseMeta,
}

pub fn ledger_seq(meta: &LedgerCloseMeta) -> u32 {
    match meta {
        LedgerCloseMeta::V0(m) => m.ledger_header.header.ledger_seq,
        LedgerCloseMeta::V1(m) => m.ledger_header.header.ledger_seq,
        LedgerCloseMeta::V2(m) => m.ledger_header.header.ledger_seq,
    }
}

/// Limits for what core hands over: `xdr.meta.depth`, since envelopes in the transaction set
/// reach depth 1000 and sit 12 levels down, and core does emit meta past 512 (an argument of
/// 98 nested maps, the soroban.scval-shapes chain). The bound is there so a corrupt stream
/// fails instead of exhausting the stack.
pub fn decode_limits(len: usize) -> Limits {
    Limits {
        depth: shapekit_manifest::META_DEPTH,
        len,
    }
}

/// Polls a stream file that core may still be appending to.
#[derive(Debug)]
pub struct MetaStream {
    path: PathBuf,
    offset: u64,
    /// Frames read from the file but not handed out yet: those a timed out
    /// [`MetaStream::wait_for`] read, so the next read starts with them and sees no gap.
    held: VecDeque<FramedLcm>,
}

impl MetaStream {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            offset: 0,
            held: VecDeque::new(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Bytes read from the file so far, always a frame boundary. Frames a timed out
    /// [`MetaStream::wait_for`] read are counted, though they are still to be handed out.
    pub fn offset(&self) -> u64 {
        self.offset
    }

    fn shown(&self) -> String {
        self.path.display().to_string()
    }

    fn io(&self, source: std::io::Error) -> MetaError {
        MetaError::Io {
            path: self.shown(),
            source,
        }
    }

    /// The next frame: one held back by a timed out wait, else the next whole frame in the
    /// file, or `None` if it is not fully written yet. A missing file counts as empty, since
    /// core creates it lazily.
    pub fn try_next(&mut self) -> Result<Option<FramedLcm>, MetaError> {
        match self.held.pop_front() {
            Some(f) => Ok(Some(f)),
            None => self.read_frame(),
        }
    }

    fn read_frame(&mut self) -> Result<Option<FramedLcm>, MetaError> {
        let mut f = match File::open(&self.path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(self.io(e)),
        };
        let size = f.metadata().map_err(|e| self.io(e))?.len();
        if size < self.offset + 4 {
            return Ok(None);
        }
        f.seek(SeekFrom::Start(self.offset))
            .map_err(|e| self.io(e))?;
        let mut hdr = [0u8; 4];
        f.read_exact(&mut hdr).map_err(|e| self.io(e))?;
        let header = u32::from_be_bytes(hdr);
        if header >> 31 != 1 {
            return Err(MetaError::Fragmented {
                path: self.shown(),
                offset: self.offset,
                header,
            });
        }
        let len = header & 0x7fff_ffff;
        if len > MAX_FRAME_LEN {
            return Err(MetaError::TooLong {
                path: self.shown(),
                offset: self.offset,
                len,
            });
        }
        if size < self.offset + 4 + u64::from(len) {
            return Ok(None);
        }
        let mut xdr = vec![0u8; len as usize];
        f.read_exact(&mut xdr).map_err(|e| self.io(e))?;
        // from_xdr insists the value uses the whole frame, which the framed
        // iterator in stellar-xdr does not check
        let meta = LedgerCloseMeta::from_xdr(&xdr, decode_limits(xdr.len())).map_err(|source| {
            MetaError::Decode {
                path: self.shown(),
                offset: self.offset,
                source,
            }
        })?;
        let framed = FramedLcm {
            ledger_seq: ledger_seq(&meta),
            offset: self.offset,
            xdr,
            meta,
        };
        self.offset += 4 + u64::from(len);
        Ok(Some(framed))
    }

    /// Reads every whole frame available now.
    pub fn drain(&mut self) -> Result<Vec<FramedLcm>, MetaError> {
        let mut out = Vec::new();
        while let Some(f) = self.try_next()? {
            out.push(f);
        }
        Ok(out)
    }

    /// Reads frames until one for `seq` has arrived, returning every frame up to it. On a
    /// timeout the frames read are kept, so a later call or [`MetaStream::try_next`] hands
    /// them out first and the caller never sees a gap.
    pub fn wait_for(&mut self, seq: u32, timeout: Duration) -> Result<Vec<FramedLcm>, MetaError> {
        let start = Instant::now();
        loop {
            if let Some(i) = self.held.iter().position(|f| f.ledger_seq >= seq) {
                return Ok(self.held.drain(..=i).collect());
            }
            match self.read_frame()? {
                Some(f) => self.held.push_back(f),
                None if start.elapsed() > timeout => {
                    return Err(MetaError::Timeout {
                        path: self.shown(),
                        seq,
                        waited: timeout,
                        last: self.held.back().map(|f| f.ledger_seq),
                    });
                }
                None => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    }
}

/// Reads a finished stream file, failing on a trailing partial frame.
pub fn read_all(path: &Path) -> Result<Vec<FramedLcm>, MetaError> {
    let mut s = MetaStream::new(path);
    let frames = s.drain()?;
    let size = std::fs::metadata(path)
        .map_err(|source| MetaError::Io {
            path: path.display().to_string(),
            source,
        })?
        .len();
    if size != s.offset() {
        return Err(MetaError::Truncated {
            path: path.display().to_string(),
            len: size - s.offset(),
        });
    }
    Ok(frames)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;
    use stellar_xdr::{
        GeneralizedTransactionSet, Hash, LedgerCloseMetaExt, LedgerCloseMetaV2, LedgerHeader,
        LedgerHeaderHistoryEntry, TransactionSetV1, WriteXdr,
    };

    fn lcm(seq: u32) -> LedgerCloseMeta {
        let header = LedgerHeader {
            ledger_seq: seq,
            ..LedgerHeader::default()
        };
        LedgerCloseMeta::V2(LedgerCloseMetaV2 {
            ext: LedgerCloseMetaExt::V0,
            ledger_header: LedgerHeaderHistoryEntry {
                hash: Hash([seq as u8; 32]),
                header,
                ext: Default::default(),
            },
            tx_set: GeneralizedTransactionSet::V1(TransactionSetV1 {
                previous_ledger_hash: Hash([0; 32]),
                phases: Default::default(),
            }),
            tx_processing: Default::default(),
            upgrades_processing: Default::default(),
            scp_info: Default::default(),
            total_byte_size_of_live_soroban_state: 0,
            evicted_keys: Default::default(),
        })
    }

    fn frame(m: &LedgerCloseMeta) -> Vec<u8> {
        let body = m.to_xdr(Limits::none()).unwrap();
        let mut out = (0x8000_0000u32 | body.len() as u32).to_be_bytes().to_vec();
        out.extend(body);
        out
    }

    #[test]
    fn reads_frames_as_they_complete() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("meta.xdr");
        let mut s = MetaStream::new(&path);
        assert!(s.try_next().unwrap().is_none());

        let a = frame(&lcm(2));
        let b = frame(&lcm(3));
        let mut f = File::create(&path).unwrap();
        f.write_all(&a).unwrap();
        f.write_all(&b[..b.len() - 1]).unwrap();
        f.flush().unwrap();

        let got = s.drain().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].ledger_seq, 2);
        assert_eq!(got[0].xdr, a[4..]);
        assert!(
            matches!(read_all(&path), Err(MetaError::Truncated { len, .. }) if len == b.len() as u64 - 1)
        );

        f.write_all(&b[b.len() - 1..]).unwrap();
        f.flush().unwrap();
        let got = s.wait_for(3, Duration::from_secs(1)).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].ledger_seq, 3);
        assert_eq!(read_all(&path).unwrap().len(), 2);
    }

    #[test]
    fn a_timed_out_wait_keeps_the_frames_it_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("meta.xdr");
        let mut f = File::create(&path).unwrap();
        f.write_all(&frame(&lcm(2))).unwrap();
        f.write_all(&frame(&lcm(3))).unwrap();
        f.flush().unwrap();
        let mut s = MetaStream::new(&path);
        assert!(matches!(
            s.wait_for(4, Duration::from_millis(50)),
            Err(MetaError::Timeout { last: Some(3), .. })
        ));
        f.write_all(&frame(&lcm(4))).unwrap();
        f.write_all(&frame(&lcm(5))).unwrap();
        f.flush().unwrap();
        let seqs = |v: Vec<FramedLcm>| v.iter().map(|f| f.ledger_seq).collect::<Vec<_>>();
        assert_eq!(
            seqs(s.wait_for(4, Duration::from_secs(1)).unwrap()),
            [2, 3, 4]
        );
        assert_eq!(seqs(s.drain().unwrap()), [5]);
    }

    #[test]
    fn rejects_multi_fragment_and_padding() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("meta.xdr");
        let mut a = frame(&lcm(2));
        a[0] &= 0x7f;
        std::fs::write(&path, &a).unwrap();
        assert!(matches!(
            MetaStream::new(&path).try_next(),
            Err(MetaError::Fragmented { .. })
        ));

        // a frame longer than its value is corrupt even though it decodes
        let body = lcm(2).to_xdr(Limits::none()).unwrap();
        let mut b = (0x8000_0000u32 | (body.len() as u32 + 4))
            .to_be_bytes()
            .to_vec();
        b.extend(body);
        b.extend([0u8; 4]);
        std::fs::write(&path, &b).unwrap();
        assert!(matches!(
            MetaStream::new(&path).try_next(),
            Err(MetaError::Decode { .. })
        ));
    }
}
