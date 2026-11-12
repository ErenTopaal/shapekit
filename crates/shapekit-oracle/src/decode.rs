//! Decoding `LedgerCloseMeta` with explicit limits, one ledger at a time.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use shapekit_manifest::framing::{FrameError, FramedReader};
use stellar_xdr::{LedgerCloseMeta, Limited, Limits, ReadXdr};

/// Nesting limit while decoding meta. Core's own transaction depth bound from protocol 28
/// (R4) is 1000; meta nests a few levels above the envelope, and anything deeper is corrupt.
pub const LCM_DEPTH: u32 = 1000;

/// Upper bound for one framed ledger. Core's largest golden is about 5.3 MB and a full
/// Soroban ledger stays far below this, so a bigger frame is treated as corruption.
pub const MAX_LCM_LEN: usize = 256 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("framing: {0}")]
    Frame(#[from] FrameError),
    #[error("ledger {index} in the stream: {source}")]
    Xdr {
        index: usize,
        source: stellar_xdr::Error,
    },
    #[error("opening {path}: {source}")]
    Open {
        path: String,
        source: std::io::Error,
    },
}

/// Limits for one ledger of `len` bytes: the length budget is the record itself, so the
/// decoder can never be talked into allocating more than it was given.
pub fn lcm_limits(len: usize) -> Limits {
    Limits {
        depth: LCM_DEPTH,
        len,
    }
}

/// Decodes exactly one `LedgerCloseMeta`, rejecting trailing bytes.
pub fn decode_lcm(bytes: &[u8]) -> Result<LedgerCloseMeta, stellar_xdr::Error> {
    let mut r = Limited::new(std::io::Cursor::new(bytes), lcm_limits(bytes.len()));
    LedgerCloseMeta::read_xdr_to_end(&mut r)
}

/// Streams the ledgers of a record-marked file (core's meta stream, a golden, a chain's
/// `ledgers.xdr`). Only one decoded ledger is alive at a time.
pub struct LcmStream<R> {
    frames: FramedReader<R>,
    index: usize,
}

impl LcmStream<BufReader<File>> {
    pub fn open(path: &Path) -> Result<Self, DecodeError> {
        let file = File::open(path).map_err(|source| DecodeError::Open {
            path: path.display().to_string(),
            source,
        })?;
        Ok(LcmStream::new(BufReader::new(file)))
    }
}

impl<R: Read> LcmStream<R> {
    pub fn new(inner: R) -> Self {
        LcmStream {
            frames: FramedReader::new(inner, MAX_LCM_LEN),
            index: 0,
        }
    }
}

/// One ledger from a stream: its raw bytes (needed for hashing and re-serving) and the value.
pub struct StreamedLcm {
    pub xdr: Vec<u8>,
    pub meta: LedgerCloseMeta,
}

impl<R: Read> Iterator for LcmStream<R> {
    type Item = Result<StreamedLcm, DecodeError>;

    fn next(&mut self) -> Option<Self::Item> {
        let record = match self.frames.next()? {
            Ok(r) => r,
            Err(e) => return Some(Err(e.into())),
        };
        let index = self.index;
        self.index += 1;
        Some(
            decode_lcm(&record)
                .map(|meta| StreamedLcm { xdr: record, meta })
                .map_err(|source| DecodeError::Xdr { index, source }),
        )
    }
}
