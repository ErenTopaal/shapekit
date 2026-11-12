//! XDR record marking (RFC 5531 section 11), the framing used by core's `--check-lcm`
//! goldens and by `METADATA_OUTPUT_STREAM`.
//!
//! Each fragment starts with a 4-byte big-endian header: the high bit marks the last fragment
//! of a record, the low 31 bits hold the fragment length. Core writes one fragment per record,
//! but multi-fragment records are joined here rather than refused, and the length in the header
//! is honoured (the crate's own `Frame` reader ignores it).

use std::io::{self, Read, Write};

use stellar_xdr::{Limits, Type};

use crate::{ToolkitError, xdr};

const LAST_FRAGMENT: u32 = 1 << 31;
const MAX_FRAGMENT: usize = (LAST_FRAGMENT - 1) as usize;

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("io error after {offset} bytes: {source}")]
    Io { offset: u64, source: io::Error },
    #[error("stream ends inside a fragment header at byte {offset}")]
    TruncatedHeader { offset: u64 },
    #[error("fragment at byte {offset} declares {declared} bytes but the stream ends first")]
    TruncatedFragment { offset: u64, declared: usize },
    #[error("record starting at byte {offset} is longer than the {max} byte limit")]
    RecordTooLong { offset: u64, max: usize },
    #[error("writing a record: {0}")]
    Write(io::Error),
    #[error("record is {len} bytes, more than one fragment can hold")]
    FragmentTooLong { len: usize },
}

/// Iterates the records of a framed stream. Each item is one record's bytes with the
/// fragment headers removed. A clean end of stream between records ends the iteration;
/// any other end is an error.
pub struct FramedReader<R> {
    inner: R,
    offset: u64,
    max_record_len: usize,
    failed: bool,
}

impl<R: Read> FramedReader<R> {
    /// `max_record_len` bounds the memory one record may take, so a corrupt header cannot
    /// make the reader allocate gigabytes.
    pub fn new(inner: R, max_record_len: usize) -> Self {
        FramedReader {
            inner,
            offset: 0,
            max_record_len,
            failed: false,
        }
    }

    // fills buf completely; returns how many bytes were read before a clean end
    fn fill(&mut self, buf: &mut [u8]) -> Result<usize, FrameError> {
        let mut got = 0;
        while got < buf.len() {
            match self.inner.read(&mut buf[got..]) {
                Ok(0) => break,
                Ok(n) => got += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(source) => {
                    return Err(FrameError::Io {
                        offset: self.offset + got as u64,
                        source,
                    });
                }
            }
        }
        self.offset += got as u64;
        Ok(got)
    }

    fn next_record(&mut self) -> Result<Option<Vec<u8>>, FrameError> {
        let start = self.offset;
        let mut record = Vec::new();
        loop {
            let header_at = self.offset;
            let mut header = [0u8; 4];
            let got = self.fill(&mut header)?;
            if got == 0 && header_at == start {
                return Ok(None);
            }
            if got < 4 {
                return Err(FrameError::TruncatedHeader { offset: header_at });
            }
            let header = u32::from_be_bytes(header);
            let len = (header & !LAST_FRAGMENT) as usize;
            if record.len().saturating_add(len) > self.max_record_len {
                return Err(FrameError::RecordTooLong {
                    offset: start,
                    max: self.max_record_len,
                });
            }
            let at = record.len();
            record.resize(at + len, 0);
            if self.fill(&mut record[at..])? < len {
                return Err(FrameError::TruncatedFragment {
                    offset: header_at,
                    declared: len,
                });
            }
            if header & LAST_FRAGMENT != 0 {
                return Ok(Some(record));
            }
        }
    }
}

impl<R: Read> Iterator for FramedReader<R> {
    type Item = Result<Vec<u8>, FrameError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }
        let item = self.next_record().transpose();
        if matches!(item, Some(Err(_))) {
            self.failed = true;
        }
        item
    }
}

/// Writes one record as a single last fragment, the way core does.
pub fn write_record<W: Write>(w: &mut W, record: &[u8]) -> Result<(), FrameError> {
    if record.len() > MAX_FRAGMENT {
        return Err(FrameError::FragmentTooLong { len: record.len() });
    }
    let header = LAST_FRAGMENT | record.len() as u32;
    w.write_all(&header.to_be_bytes())
        .and_then(|()| w.write_all(record))
        .map_err(FrameError::Write)
}

/// Frames each record and concatenates them.
pub fn frame_records<'a>(
    records: impl IntoIterator<Item = &'a [u8]>,
) -> Result<Vec<u8>, FrameError> {
    let mut out = Vec::new();
    for r in records {
        write_record(&mut out, r)?;
    }
    Ok(out)
}

/// Splits a framed stream and decodes every record as `type_name`, each with its own
/// copy of `limits` and with no trailing bytes allowed inside a record.
pub fn decode_framed(
    type_name: &str,
    stream: &[u8],
    limits: Limits,
) -> Result<Vec<Type>, ToolkitError> {
    FramedReader::new(stream, limits.len.min(stream.len()))
        .map(|record| {
            let record = record.map_err(ToolkitError::Frame)?;
            xdr::decode(type_name, &record, limits.clone())
        })
        .collect()
}
