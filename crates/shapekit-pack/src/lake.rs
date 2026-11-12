//! Writing and reading a SEP-54 ledger lake on a filesystem: `.config.json` at the root
//! and one zstd-compressed `LedgerCloseMetaBatch` per object (R9).

use std::fs::File;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use stellar_xdr::{LedgerCloseMeta, Limited, Limits, ReadXdr};

use crate::layout::{BATCH_EXTENSION, Layout, parse_batch_name};
use crate::{PackError, io_err};

pub const CONFIG_FILE: &str = ".config.json";

/// `datastore.Version` in go-stellar-sdk v0.8.0, which it compares against the config.
pub const DATASTORE_VERSION: &str = "1.0";

pub const COMPRESSION: &str = "zstd";

/// Compression level of every batch. The lake is committed and written once per chain, so
/// size wins over speed. The level and the zstd library version (recorded in `chain.json`)
/// together fix the compressed bytes.
pub const ZSTD_LEVEL: i32 = 19;

/// Decompressed batches above this are treated as corrupt rather than read into memory.
pub const MAX_BATCH_LEN: u64 = 1 << 30;

/// Nesting limit while decoding meta, the bound the oracle uses too.
pub const LCM_DEPTH: u32 = 1000;

/// The zstd library the batches are compressed with, as `major.minor.patch`.
pub fn zstd_library() -> &'static str {
    zstd::zstd_safe::version_string()
}

/// `.config.json`, field for field and in the order go-stellar-sdk's `DatastoreManifest`
/// marshals it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LakeConfig {
    pub network_passphrase: String,
    pub version: String,
    pub compression: String,
    pub ledgers_per_batch: u32,
    pub batches_per_partition: u32,
}

impl LakeConfig {
    pub fn new(passphrase: &str, layout: Layout) -> Self {
        LakeConfig {
            network_passphrase: passphrase.to_string(),
            version: DATASTORE_VERSION.to_string(),
            compression: COMPRESSION.to_string(),
            ledgers_per_batch: layout.ledgers_per_batch(),
            batches_per_partition: layout.batches_per_partition(),
        }
    }

    pub fn layout(&self) -> Result<Layout, PackError> {
        Layout::new(self.ledgers_per_batch, self.batches_per_partition)
    }

    /// The bytes galexie writes: Go's `json.Marshal`, which is compact and escapes `<`, `>`,
    /// `&`, U+2028 and U+2029.
    pub fn to_go_json(&self) -> Vec<u8> {
        let text = serde_json::to_string(self).expect("a config is plain json");
        let mut out = String::with_capacity(text.len());
        for c in text.chars() {
            match c {
                '<' => out.push_str("\\u003c"),
                '>' => out.push_str("\\u003e"),
                '&' => out.push_str("\\u0026"),
                '\u{2028}' => out.push_str("\\u2028"),
                '\u{2029}' => out.push_str("\\u2029"),
                c => out.push(c),
            }
        }
        out.into_bytes()
    }
}

/// One batch object as written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrittenBatch {
    pub key: String,
    /// First and last ledger inside the batch.
    pub first: u32,
    pub last: u32,
    /// SHA-256 of the compressed object.
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct LakeSummary {
    pub config: LakeConfig,
    pub batches: Vec<WrittenBatch>,
}

pub(crate) fn lcm_limits(len: usize) -> Limits {
    Limits {
        depth: LCM_DEPTH,
        len,
    }
}

/// Sequence of one `LedgerCloseMeta` given as XDR, checking that it decodes exactly.
pub fn lcm_seq(xdr: &[u8]) -> Result<u32, PackError> {
    let mut r = Limited::new(Cursor::new(xdr), lcm_limits(xdr.len()));
    let lcm = LedgerCloseMeta::read_xdr_to_end(&mut r).map_err(|source| PackError::Xdr {
        context: "decoding a LedgerCloseMeta".into(),
        source,
    })?;
    Ok(seq_of(&lcm))
}

pub fn seq_of(lcm: &LedgerCloseMeta) -> u32 {
    match lcm {
        LedgerCloseMeta::V0(m) => m.ledger_header.header.ledger_seq,
        LedgerCloseMeta::V1(m) => m.ledger_header.header.ledger_seq,
        LedgerCloseMeta::V2(m) => m.ledger_header.header.ledger_seq,
    }
}

/// Writes consecutive ledgers (each a `LedgerCloseMeta` as XDR) as a SEP-54 lake in `dir`,
/// which must not exist. Only one batch is held in memory. Each batch's
/// `startSequence` and `endSequence` are the first and last ledger it actually holds, so a
/// chain starting at ledger 2 gets a first batch starting at 2 even when the key's range
/// starts at 0; the sdk indexes a batch from `startSequence`.
pub fn write_sep54<I>(
    ledgers: I,
    passphrase: &str,
    dir: &Path,
    layout: Layout,
) -> Result<LakeSummary, PackError>
where
    I: IntoIterator<Item = Result<Vec<u8>, PackError>>,
{
    if dir.exists() {
        return Err(PackError::Format(format!(
            "{} already exists",
            dir.display()
        )));
    }
    std::fs::create_dir_all(dir).map_err(io_err(dir))?;
    let config = LakeConfig::new(passphrase, layout);
    let config_path = dir.join(CONFIG_FILE);
    std::fs::write(&config_path, config.to_go_json()).map_err(io_err(&config_path))?;

    let mut batches = Vec::new();
    let mut pending: Vec<Vec<u8>> = Vec::new();
    let mut pending_first = 0u32;
    let mut last_seq: Option<u32> = None;
    for item in ledgers {
        let xdr = item?;
        let seq = lcm_seq(&xdr)?;
        if let Some(prev) = last_seq
            && prev.checked_add(1) != Some(seq)
        {
            return Err(PackError::Format(format!(
                "ledger {seq} follows ledger {prev}; a lake needs consecutive ledgers"
            )));
        }
        if !pending.is_empty() && layout.batch_range(seq).0 != layout.batch_range(pending_first).0 {
            batches.push(flush(dir, layout, pending_first, &pending)?);
            pending.clear();
        }
        if pending.is_empty() {
            pending_first = seq;
        }
        pending.push(xdr);
        last_seq = Some(seq);
    }
    if pending.is_empty() {
        return Err(PackError::Format("no ledgers to write".into()));
    }
    batches.push(flush(dir, layout, pending_first, &pending)?);
    Ok(LakeSummary { config, batches })
}

fn flush(
    dir: &Path,
    layout: Layout,
    first: u32,
    ledgers: &[Vec<u8>],
) -> Result<WrittenBatch, PackError> {
    let last = first + (ledgers.len() as u32 - 1);
    let body: usize = ledgers.iter().map(Vec::len).sum();
    let mut xdr = Vec::with_capacity(12 + body);
    xdr.extend(first.to_be_bytes());
    xdr.extend(last.to_be_bytes());
    xdr.extend((ledgers.len() as u32).to_be_bytes());
    for l in ledgers {
        xdr.extend_from_slice(l);
    }
    // bulk compression writes the content size into the frame header, which streaming
    // compression without a pledged size would not
    let compressed = zstd::bulk::compress(&xdr, ZSTD_LEVEL).map_err(|source| PackError::Io {
        path: format!("compressing batch {first}-{last}"),
        source,
    })?;
    let key = layout.object_key(first);
    let path = dir.join(&key);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io_err(parent))?;
    }
    std::fs::write(&path, &compressed).map_err(io_err(&path))?;
    Ok(WrittenBatch {
        key,
        first,
        last,
        sha256: hex::encode(Sha256::digest(&compressed)),
    })
}

/// A batch object found in a lake, located by its key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchRef {
    pub key: String,
    /// The range the key names; the ledgers inside can be a narrower range.
    pub key_first: u32,
    pub key_last: u32,
}

/// One decompressed batch: the ledgers' XDR, kept as bytes so they can be hashed and
/// served without a re-encode.
#[derive(Debug, Clone)]
pub struct Batch {
    pub start: u32,
    pub end: u32,
    data: Vec<u8>,
    spans: Vec<(usize, usize)>,
}

impl Batch {
    /// Splits a `LedgerCloseMetaBatch` into its ledgers, checking that every ledger decodes,
    /// that they run from `startSequence` to `endSequence` and that nothing trails.
    pub fn parse(data: Vec<u8>) -> Result<Batch, PackError> {
        let bad = |m: String| PackError::Format(format!("LedgerCloseMetaBatch: {m}"));
        if data.len() < 12 {
            return Err(bad(format!(
                "{} bytes is shorter than its header",
                data.len()
            )));
        }
        let word = |i: usize| u32::from_be_bytes(data[i..i + 4].try_into().expect("4 bytes"));
        let (start, end, count) = (word(0), word(4), word(8));
        if end < start || u64::from(count) != u64::from(end - start) + 1 {
            return Err(bad(format!(
                "holds {count} ledgers for the range {start} to {end}"
            )));
        }
        let mut spans = Vec::with_capacity(count as usize);
        let mut r = Limited::new(Cursor::new(&data[..]), lcm_limits(data.len()));
        r.inner.set_position(12);
        for i in 0..count {
            let from = r.inner.position() as usize;
            let lcm = LedgerCloseMeta::read_xdr(&mut r).map_err(|source| PackError::Xdr {
                context: format!("ledger {i} of batch {start}-{end}"),
                source,
            })?;
            let to = r.inner.position() as usize;
            if seq_of(&lcm) != start + i {
                return Err(bad(format!(
                    "entry {i} is ledger {}, expected {}",
                    seq_of(&lcm),
                    start + i
                )));
            }
            spans.push((from, to));
        }
        if r.inner.position() as usize != data.len() {
            return Err(bad(format!(
                "{} trailing bytes",
                data.len() - r.inner.position() as usize
            )));
        }
        Ok(Batch {
            start,
            end,
            data,
            spans,
        })
    }

    pub fn ledger(&self, seq: u32) -> Option<&[u8]> {
        if seq < self.start || seq > self.end {
            return None;
        }
        let (a, b) = self.spans[(seq - self.start) as usize];
        Some(&self.data[a..b])
    }

    pub fn ledgers(&self) -> impl Iterator<Item = (u32, &[u8])> {
        self.spans
            .iter()
            .enumerate()
            .map(|(i, (a, b))| (self.start + i as u32, &self.data[*a..*b]))
    }

    /// Decompressed size, what a cache of batches is accounted in.
    pub fn len_bytes(&self) -> usize {
        self.data.len()
    }
}

/// A lake on disk: its config and the batch objects it holds, sorted by ledger.
#[derive(Debug, Clone)]
pub struct Lake {
    root: PathBuf,
    config: LakeConfig,
    layout: Layout,
    batches: Vec<BatchRef>,
}

impl Lake {
    /// Reads `.config.json` and lists the batch objects. Every file must be a batch at the
    /// key the layout gives its range; anything else is an error, not skipped.
    pub fn open(root: &Path) -> Result<Lake, PackError> {
        let config_path = root.join(CONFIG_FILE);
        let config: LakeConfig =
            serde_json::from_slice(&std::fs::read(&config_path).map_err(io_err(&config_path))?)
                .map_err(|e| PackError::Format(format!("{}: {e}", config_path.display())))?;
        if config.compression != COMPRESSION {
            return Err(PackError::Format(format!(
                "{}: compression {:?}, SEP-54 defines only zstd",
                config_path.display(),
                config.compression
            )));
        }
        let layout = config.layout()?;
        let mut batches = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for item in std::fs::read_dir(&dir).map_err(io_err(&dir))? {
                let path = item.map_err(io_err(&dir))?.path();
                let rel = rel_key(root, &path);
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if rel == CONFIG_FILE {
                    continue;
                }
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let (first, last) = parse_batch_name(name).ok_or_else(|| {
                    PackError::Format(format!("{rel} in the lake is not a batch object"))
                })?;
                if layout.object_key(first) != rel || layout.batch_range(first) != (first, last) {
                    return Err(PackError::Format(format!(
                        "{rel} is not where the layout puts ledgers {first} to {last} ({})",
                        layout.object_key(first)
                    )));
                }
                batches.push(BatchRef {
                    key: rel,
                    key_first: first,
                    key_last: last,
                });
            }
        }
        batches.sort_by_key(|b| b.key_first);
        Ok(Lake {
            root: root.to_path_buf(),
            config,
            layout,
            batches,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config(&self) -> &LakeConfig {
        &self.config
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }

    pub fn batches(&self) -> &[BatchRef] {
        &self.batches
    }

    /// The batch object whose key covers `seq`.
    pub fn batch_for(&self, seq: u32) -> Option<&BatchRef> {
        let i = self.batches.partition_point(|b| b.key_last < seq);
        self.batches.get(i).filter(|b| b.key_first <= seq)
    }

    /// Decompresses and splits one batch, checking it sits inside its key's range.
    pub fn read_batch(&self, b: &BatchRef) -> Result<Batch, PackError> {
        let path = self.root.join(&b.key);
        let file = File::open(&path).map_err(io_err(&path))?;
        let decoder = zstd::stream::Decoder::new(file).map_err(io_err(&path))?;
        let mut data = Vec::new();
        decoder
            .take(MAX_BATCH_LEN + 1)
            .read_to_end(&mut data)
            .map_err(io_err(&path))?;
        if data.len() as u64 > MAX_BATCH_LEN {
            return Err(PackError::Format(format!(
                "{} decompresses to more than {MAX_BATCH_LEN} bytes",
                b.key
            )));
        }
        let batch = Batch::parse(data).map_err(|e| PackError::Format(format!("{}: {e}", b.key)))?;
        if batch.start < b.key_first || batch.end > b.key_last {
            return Err(PackError::Format(format!(
                "{} holds ledgers {} to {}, outside its key",
                b.key, batch.start, batch.end
            )));
        }
        Ok(batch)
    }

    /// First and last ledger in the lake, reading the first and last batch.
    pub fn range(&self) -> Result<(u32, u32), PackError> {
        let (Some(first), Some(last)) = (self.batches.first(), self.batches.last()) else {
            return Err(PackError::Format(format!(
                "{} holds no batches",
                self.root.display()
            )));
        };
        Ok((self.read_batch(first)?.start, self.read_batch(last)?.end))
    }

    /// Every ledger in order, one batch in memory at a time, checking that the lake has no
    /// gaps.
    pub fn ledgers(&self) -> LakeLedgers<'_> {
        LakeLedgers {
            lake: self,
            next_batch: 0,
            current: None,
            index: 0,
            expect: None,
        }
    }
}

fn rel_key(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// One ledger read back from a lake.
#[derive(Debug, Clone)]
pub struct RawLedger {
    pub seq: u32,
    pub xdr: Vec<u8>,
}

pub struct LakeLedgers<'a> {
    lake: &'a Lake,
    next_batch: usize,
    current: Option<Batch>,
    index: u32,
    expect: Option<u32>,
}

impl Iterator for LakeLedgers<'_> {
    type Item = Result<RawLedger, PackError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(b) = &self.current {
                let seq = b.start + self.index;
                if seq <= b.end {
                    self.index += 1;
                    if let Some(e) = self.expect
                        && e != seq
                    {
                        self.current = None;
                        self.next_batch = self.lake.batches.len();
                        return Some(Err(PackError::Format(format!(
                            "the lake jumps from ledger {} to {seq}",
                            e - 1
                        ))));
                    }
                    self.expect = Some(seq + 1);
                    let xdr = b.ledger(seq).expect("seq is in the batch").to_vec();
                    return Some(Ok(RawLedger { seq, xdr }));
                }
            }
            let r = self.lake.batches.get(self.next_batch)?;
            self.next_batch += 1;
            match self.lake.read_batch(r) {
                Ok(b) => {
                    self.current = Some(b);
                    self.index = 0;
                }
                Err(e) => {
                    self.next_batch = self.lake.batches.len();
                    self.current = None;
                    return Some(Err(e));
                }
            }
        }
    }
}

/// Opens the lake at `dir` and streams its ledgers back in order.
pub fn read_sep54(dir: &Path) -> Result<Lake, PackError> {
    Lake::open(dir)
}

/// Hash over every file of a lake, key and content, in key order: what two lakes must agree
/// on to be byte-identical.
pub fn lake_digest(dir: &Path) -> Result<String, PackError> {
    let lake = Lake::open(dir)?;
    let mut h = Sha256::new();
    let config = dir.join(CONFIG_FILE);
    h.update(CONFIG_FILE.as_bytes());
    h.update(Sha256::digest(
        std::fs::read(&config).map_err(io_err(&config))?,
    ));
    for b in lake.batches() {
        let p = dir.join(&b.key);
        h.update(b.key.as_bytes());
        h.update(Sha256::digest(std::fs::read(&p).map_err(io_err(&p))?));
    }
    Ok(hex::encode(h.finalize()))
}

/// True for a lake object key, the files `Lake::open` accepts besides the config.
pub fn is_batch_key(key: &str) -> bool {
    key.ends_with(BATCH_EXTENSION)
}
