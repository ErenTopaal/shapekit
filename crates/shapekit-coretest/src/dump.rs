//! The text written by `stellar-core test --debug-test-tx-meta <file>`.
//!
//! core writes one record per transaction meta (`recordOrCheckGlobalTestTxMetadata`,
//! `src/test/test.cpp`): a header line
//!
//! ```text
//! === "<test file stem>" : <section>|<section>|... ===
//! ```
//!
//! followed by the meta as indented cereal json (`xdrToCerealString(txMeta, "TransactionMeta")`).
//! The stem is a `std::filesystem::path` streamed with `<<`, so it is quoted the way
//! `std::quoted` quotes. rapidjson escapes control characters inside strings, so no line of
//! the json body starts with `=== `, and a header line is never ambiguous.
//!
//! Bodies are kept as bytes: core streams string fields as raw bytes, and a test may well
//! write a string field that is not utf-8.

use std::io::BufRead;

use crate::{Error, Result};

/// One transaction meta of the dump.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// Stem of the test source file, the baseline file name without `.json`.
    pub file: String,
    /// Catch section path joined by `|`, the key inside the baseline file.
    pub section: String,
    /// 1-based line of the header in the dump.
    pub line: u64,
    /// The json body, without its trailing newline.
    pub body: Vec<u8>,
}

impl Record {
    /// The protocol version the section ran at, from its `protocol version N` component
    /// (`test_versions_wrapper`, `src/test/test.cpp`). `None` for sections outside the
    /// version wrapper.
    pub fn protocol(&self) -> Option<u32> {
        protocol_of(&self.section)
    }

    /// The `TransactionMeta` union arm (`v`), read from the body's third line.
    pub fn meta_version(&self) -> Option<u32> {
        let mut lines = self.body.split(|&b| b == b'\n');
        if lines.next()? != b"{" {
            return None;
        }
        if lines.next()?.trim_ascii() != b"\"TransactionMeta\": {" {
            return None;
        }
        let v = lines.next()?.trim_ascii();
        let rest = v.strip_prefix(b"\"v\": ")?;
        let rest = rest.strip_suffix(b",").unwrap_or(rest);
        std::str::from_utf8(rest).ok()?.parse().ok()
    }
}

/// `protocol version N` anywhere in a `|`-joined section path.
pub fn protocol_of(section: &str) -> Option<u32> {
    section
        .split('|')
        .find_map(|part| part.strip_prefix("protocol version ")?.parse().ok())
}

/// Parses a header line; `None` when the line is not a header.
pub fn parse_header(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("=== ")?.strip_suffix(" ===")?;
    let (file, after) = unquote(rest)?;
    let section = after.strip_prefix(" : ")?;
    Some((file, section.to_owned()))
}

/// Reads a `std::quoted` string at the start of `s`, returns it and the rest.
fn unquote(s: &str) -> Option<(String, &str)> {
    let mut chars = s.char_indices();
    if chars.next()?.1 != '"' {
        return None;
    }
    let mut out = String::new();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => out.push(chars.next()?.1),
            '"' => return Some((out, &s[i + 1..])),
            c => out.push(c),
        }
    }
    None
}

/// Streams the records of a dump.
pub struct Reader<R> {
    input: R,
    path: String,
    line: u64,
    pending: Option<(String, String, u64)>,
    done: bool,
}

impl<R: BufRead> Reader<R> {
    pub fn new(input: R, path: impl Into<String>) -> Self {
        Reader {
            input,
            path: path.into(),
            line: 0,
            pending: None,
            done: false,
        }
    }

    fn err(&self, message: impl Into<String>) -> Error {
        Error::Dump {
            path: self.path.clone(),
            line: self.line,
            message: message.into(),
        }
    }

    fn read_line(&mut self, buf: &mut Vec<u8>) -> Result<bool> {
        buf.clear();
        let n = self
            .input
            .read_until(b'\n', buf)
            .map_err(|source| Error::Read {
                path: self.path.clone(),
                source,
            })?;
        if n == 0 {
            return Ok(false);
        }
        self.line += 1;
        if buf.last() == Some(&b'\n') {
            buf.pop();
        }
        Ok(true)
    }

    fn header_of(&self, buf: &[u8]) -> Result<Option<(String, String)>> {
        if !buf.starts_with(b"=== ") {
            return Ok(None);
        }
        let text = std::str::from_utf8(buf).map_err(|_| self.err("header is not utf-8"))?;
        parse_header(text)
            .map(Some)
            .ok_or_else(|| self.err(format!("malformed header {text:?}")))
    }

    /// The next record, or `None` at the end of the dump.
    pub fn next_record(&mut self) -> Result<Option<Record>> {
        if self.done {
            return Ok(None);
        }
        let mut buf = Vec::new();
        let (file, section, line) = match self.pending.take() {
            Some(p) => p,
            None => {
                if !self.read_line(&mut buf)? {
                    self.done = true;
                    return Ok(None);
                }
                match self.header_of(&buf)? {
                    Some((f, s)) => (f, s, self.line),
                    None => return Err(self.err("dump does not start with a record header")),
                }
            }
        };
        let mut body = Vec::new();
        loop {
            if !self.read_line(&mut buf)? {
                self.done = true;
                break;
            }
            if let Some((f, s)) = self.header_of(&buf)? {
                self.pending = Some((f, s, self.line));
                break;
            }
            if !body.is_empty() {
                body.push(b'\n');
            }
            body.extend_from_slice(&buf);
        }
        if body.is_empty() {
            return Err(Error::Dump {
                path: self.path.clone(),
                line,
                message: "record header without a body".into(),
            });
        }
        Ok(Some(Record {
            file,
            section,
            line,
            body,
        }))
    }
}

impl<R: BufRead> Iterator for Reader<R> {
    type Item = Result<Record>;
    fn next(&mut self) -> Option<Self::Item> {
        self.next_record().transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "=== \"BumpSequenceTests\" : bump sequence|protocol version 0 ===\n\
{\n    \"TransactionMeta\": {\n        \"v\": 2,\n        \"v2\": {}\n    }\n}\n\
=== \"Odd \\\"name\\\"\" : a : b|c ===\n\
{\n    \"TransactionMeta\": {\n        \"v\": 4\n    }\n}\n";

    #[test]
    fn reads_records_with_quoted_stems() {
        let records: Vec<Record> = Reader::new(SAMPLE.as_bytes(), "sample")
            .collect::<Result<_>>()
            .unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].file, "BumpSequenceTests");
        assert_eq!(records[0].section, "bump sequence|protocol version 0");
        assert_eq!(records[0].protocol(), Some(0));
        assert_eq!(records[0].meta_version(), Some(2));
        assert_eq!(records[0].line, 1);
        assert_eq!(records[1].file, "Odd \"name\"");
        assert_eq!(records[1].section, "a : b|c");
        assert_eq!(records[1].protocol(), None);
        assert_eq!(records[1].meta_version(), Some(4));
        assert_eq!(records[1].line, 8);
    }

    #[test]
    fn rejects_text_before_the_first_header() {
        let err = Reader::new("{}\n".as_bytes(), "x")
            .next()
            .unwrap()
            .unwrap_err();
        assert!(err.to_string().contains("does not start"));
    }

    #[test]
    fn rejects_a_header_without_body() {
        let text = "=== \"A\" : s ===\n=== \"A\" : s ===\n{\n}\n";
        let err = Reader::new(text.as_bytes(), "x")
            .next()
            .unwrap()
            .unwrap_err();
        assert!(err.to_string().contains("without a body"));
    }

    #[test]
    fn empty_dump_has_no_records() {
        assert!(Reader::new("".as_bytes(), "x").next().is_none());
    }
}
