//! The recorded chain behind the server: ledgers read on demand from the SEP-54 lake
//! through a cache bounded in bytes, the history archive, a transaction index and the
//! ledger state at the visible head.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use shapekit_oracle::state::GENESIS_LEDGER_SEQ;
use shapekit_oracle::view::LedgerView;
use shapekit_oracle::{LedgerState, Network, decode_lcm};
use shapekit_pack::archive::{ArchiveIndex, Member, classify};
use shapekit_pack::chain::{ChainFile, open_lake};
use shapekit_pack::lake::{Batch, BatchRef, Lake};
use shapekit_pack::{PackError, archive_at};
use stellar_xdr::{LedgerCloseMeta, LedgerHeaderHistoryEntry};

use crate::ServeError;

/// Default byte budget of the batch cache.
pub const DEFAULT_CACHE_BYTES: usize = 64 * 1024 * 1024;

/// Decompressed batches, least recently used first out, held under a byte budget. A batch
/// larger than the whole budget is returned without being kept. Recency is a counter per
/// use, so both lookup and eviction are logarithmic.
struct BatchCache {
    budget: usize,
    used: usize,
    tick: u64,
    by_use: BTreeMap<u64, usize>,
    items: HashMap<usize, (Arc<Batch>, u64)>,
}

impl BatchCache {
    fn new(budget: usize) -> Self {
        BatchCache {
            budget,
            used: 0,
            tick: 0,
            by_use: BTreeMap::new(),
            items: HashMap::new(),
        }
    }

    fn get(&mut self, i: usize) -> Option<Arc<Batch>> {
        self.tick += 1;
        let (b, used) = self.items.get_mut(&i)?;
        self.by_use.remove(used);
        *used = self.tick;
        self.by_use.insert(self.tick, i);
        Some(b.clone())
    }

    fn put(&mut self, i: usize, b: Arc<Batch>) {
        let size = b.len_bytes();
        if size > self.budget || self.items.contains_key(&i) {
            return;
        }
        while self.used + size > self.budget {
            let Some((_, old)) = self.by_use.pop_first() else {
                break;
            };
            if let Some((o, _)) = self.items.remove(&old) {
                self.used -= o.len_bytes();
            }
        }
        self.tick += 1;
        self.used += size;
        self.by_use.insert(self.tick, i);
        self.items.insert(i, (b, self.tick));
    }
}

/// Where a transaction sits: ledger and position in apply order, from 0.
#[derive(Debug, Clone, Copy)]
pub struct TxLocation {
    pub ledger: u32,
    pub index: u32,
}

pub struct Replay {
    pub dir: PathBuf,
    pub chain: ChainFile,
    pub network: Network,
    lake: Lake,
    /// Lake object key to the last ledger inside it, for the static lake route.
    lake_keys: HashMap<String, u32>,
    cache: Mutex<BatchCache>,
    head: AtomicU32,
    pub first: u32,
    pub last: u32,
    pub first_close_time: u64,
    txs: HashMap<[u8; 32], TxLocation>,
    archive: ArchiveIndex,
    archive_dir: shapekit_pack::scratch::Scratch,
    state: Mutex<LedgerState>,
}

impl Replay {
    /// Opens a committed chain directory. The head starts at `head`, or at the chain's last
    /// ledger. The archive is extracted once into a temporary directory; the lake is not
    /// loaded, only scanned once for the transaction index.
    pub fn open(dir: &Path, head: Option<u32>, cache_bytes: usize) -> Result<Replay, ServeError> {
        let chain = ChainFile::read(dir)?;
        let lake = open_lake(dir, &chain)?;
        let network = Network::new(&chain.passphrase);
        let (first, last) = (chain.ledger_range.first, chain.ledger_range.last);
        // the ledger state is rebuilt from genesis, so the chain has to start right after it;
        // a chain cut from later history would need a seeded state, which no chain needs
        if first != GENESIS_LEDGER_SEQ + 1 {
            return Err(ServeError::Chain(format!(
                "the chain starts at ledger {first}; replay rebuilds state from genesis and needs a chain that starts at ledger {}",
                GENESIS_LEDGER_SEQ + 1
            )));
        }
        let lake_keys = chain
            .lake
            .batches
            .iter()
            .map(|b| (b.key.clone(), b.last))
            .collect();

        let mut txs = HashMap::new();
        let mut first_close_time = None;
        for item in lake.ledgers() {
            let l = item?;
            let lcm = decode(&l.xdr, l.seq)?;
            if l.seq == first {
                first_close_time = Some(header_entry(&lcm).header.scp_value.close_time.0);
            }
            let view = LedgerView::new(&lcm, &network)
                .map_err(|e| ServeError::Chain(format!("ledger {}: {e}", l.seq)))?;
            for tx in &view.txs {
                txs.insert(
                    tx.result.transaction_hash.0,
                    TxLocation {
                        ledger: l.seq,
                        index: tx.index,
                    },
                );
            }
        }
        let first_close_time = first_close_time
            .ok_or_else(|| ServeError::Chain(format!("the lake has no ledger {first}")))?;

        let tar = dir.join(&chain.archive.file);
        let archive = ArchiveIndex::read(&tar)?;
        let newest =
            *archive.states.keys().next_back().ok_or_else(|| {
                ServeError::Chain(format!("{} has no history state", tar.display()))
            })?;
        let archive_dir = shapekit_pack::scratch::scratch_dir("shapekit-serve-archive-")
            .map_err(|e| ServeError::Io(format!("creating the archive directory: {e}")))?;
        archive_at(&tar, newest, &archive_dir.path().join("a"))?;

        let head = head.unwrap_or(last);
        check_head(head, first, last)?;
        Ok(Replay {
            dir: dir.to_path_buf(),
            network: network.clone(),
            lake,
            lake_keys,
            cache: Mutex::new(BatchCache::new(cache_bytes)),
            head: AtomicU32::new(head),
            first,
            last,
            first_close_time,
            txs,
            archive,
            archive_dir,
            state: Mutex::new(LedgerState::genesis(&network)),
            chain,
        })
    }

    pub fn head(&self) -> u32 {
        self.head.load(Ordering::SeqCst)
    }

    pub fn set_head(&self, head: u32) -> Result<(), ServeError> {
        check_head(head, self.first, self.last)?;
        self.head.store(head, Ordering::SeqCst);
        Ok(())
    }

    fn batch(&self, i: usize, r: &BatchRef) -> Result<Arc<Batch>, ServeError> {
        if let Some(b) = self.cache.lock().expect("cache lock").get(i) {
            return Ok(b);
        }
        let b = Arc::new(self.lake.read_batch(r)?);
        self.cache.lock().expect("cache lock").put(i, b.clone());
        Ok(b)
    }

    /// The canonical XDR of a recorded ledger, whether or not it is visible yet.
    pub fn ledger_xdr(&self, seq: u32) -> Result<Vec<u8>, ServeError> {
        let batches = self.lake.batches();
        let i = batches.partition_point(|b| b.key_last < seq);
        let r = batches
            .get(i)
            .filter(|b| b.key_first <= seq)
            .ok_or_else(|| ServeError::Chain(format!("ledger {seq} is not in the lake")))?;
        let b = self.batch(i, r)?;
        b.ledger(seq)
            .map(<[u8]>::to_vec)
            .ok_or_else(|| ServeError::Chain(format!("ledger {seq} is not in batch {}", r.key)))
    }

    pub fn ledger(&self, seq: u32) -> Result<(Vec<u8>, LedgerCloseMeta), ServeError> {
        let xdr = self.ledger_xdr(seq)?;
        let lcm = decode(&xdr, seq)?;
        Ok((xdr, lcm))
    }

    pub fn close_time(&self, seq: u32) -> Result<u64, ServeError> {
        Ok(header_entry(&self.ledger(seq)?.1)
            .header
            .scp_value
            .close_time
            .0)
    }

    pub fn tx(&self, hash: &[u8; 32]) -> Option<TxLocation> {
        self.txs.get(hash).copied()
    }

    /// Runs `f` on the ledger state at the visible head, applying ledgers forward from where
    /// the state stands, or from genesis when the head moved back.
    pub fn with_state<T>(&self, f: impl FnOnce(&LedgerState, u32) -> T) -> Result<T, ServeError> {
        let head = self.head();
        let mut state = self.state.lock().expect("state lock");
        if state.head_seq().is_some_and(|h| h > head) {
            *state = LedgerState::genesis(&self.network);
        }
        // genesis has a head, and open checked that the chain starts right after it
        let from = state
            .head_seq()
            .expect("a state built from genesis has a head")
            + 1;
        for seq in from..=head {
            let (_, lcm) = self.ledger(seq)?;
            state
                .apply_ledger(&lcm, &self.network)
                .map_err(|e| ServeError::Chain(format!("applying ledger {seq}: {e}")))?;
        }
        Ok(f(&state, head))
    }

    /// A lake object key's file, if the batch is complete at the visible head.
    pub fn lake_file(&self, key: &str) -> Option<PathBuf> {
        if key == shapekit_pack::lake::CONFIG_FILE {
            return Some(self.lake.root().join(key));
        }
        let last = *self.lake_keys.get(key)?;
        (last <= self.head()).then(|| self.lake.root().join(key))
    }

    /// An archive file as the archive stood at the newest checkpoint at or below the head.
    /// The root state file is that checkpoint's.
    pub fn archive_file(&self, path: &str) -> Option<PathBuf> {
        let member = classify(path)?;
        let checkpoint = self.archive.checkpoint_at(self.head())?;
        let root = self.archive_dir.path().join("a");
        match member {
            Member::WellKnown => Some(root.join(format!(
                "history/{}/history-{checkpoint:08x}.json",
                hex_dirs(checkpoint)
            ))),
            m if self.archive.visible_at(&m, checkpoint) => Some(root.join(path)),
            _ => None,
        }
    }
}

fn hex_dirs(seq: u32) -> String {
    let h = format!("{seq:08x}");
    format!("{}/{}/{}", &h[0..2], &h[2..4], &h[4..6])
}

fn check_head(head: u32, first: u32, last: u32) -> Result<(), ServeError> {
    if head < first || head > last {
        return Err(ServeError::Head(format!(
            "head {head} is outside the chain's ledgers {first} to {last}"
        )));
    }
    Ok(())
}

pub fn decode(xdr: &[u8], seq: u32) -> Result<LedgerCloseMeta, ServeError> {
    decode_lcm(xdr).map_err(|e| ServeError::Chain(format!("decoding ledger {seq}: {e}")))
}

pub fn header_entry(lcm: &LedgerCloseMeta) -> &LedgerHeaderHistoryEntry {
    match lcm {
        LedgerCloseMeta::V0(m) => &m.ledger_header,
        LedgerCloseMeta::V1(m) => &m.ledger_header,
        LedgerCloseMeta::V2(m) => &m.ledger_header,
    }
}

impl From<PackError> for ServeError {
    fn from(e: PackError) -> Self {
        ServeError::Chain(e.to_string())
    }
}
