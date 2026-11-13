//! `LedgerState`: every live ledger entry by key, built only by applying meta in the order
//! core applied it (see `view` for that order).
//!
//! Two tracking modes:
//!
//! - `Complete`: the state is known in full, either from genesis or from a seed such as a
//!   history-archive checkpoint. Every change is checked: a created key must be absent, a
//!   `STATE` value must equal the stored entry, an updated, removed or evicted key must
//!   exist, and a restored entry must match what was archived.
//! - `Partial`: the state before the stream is unknown (core goldens start from a test
//!   application whose setup is not in the meta). A key becomes tracked the first time the
//!   stream shows its value (`CREATED`, `STATE` or `RESTORED`); from then on it gets the
//!   same checks as in `Complete` mode. Changes to keys never seen are counted as unchecked.

use std::collections::BTreeMap;

use stellar_xdr::{
    AccountEntry, AccountEntryExt, ConfigSettingId, ContractDataDurability, LedgerCloseMeta,
    LedgerEntry, LedgerEntryChange, LedgerEntryData, LedgerEntryExt, LedgerHeader, LedgerKey,
    Limits, SequenceNumber, String32, Thresholds, WriteXdr,
};

use crate::network::Network;
use crate::view::{LedgerView, Scope, ViewError};

/// Core's genesis constants (`LedgerManager::GENESIS_*`, LedgerManagerImpl.cpp at v30.0.0).
pub const GENESIS_LEDGER_SEQ: u32 = 1;
pub const GENESIS_TOTAL_COINS: i64 = 1_000_000_000_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tracking {
    Complete,
    /// Complete except Soroban config settings, which a core test application writes at
    /// genesis without meta (`USE_CONFIG_FOR_GENESIS`,
    /// `SorobanNetworkConfig::initializeGenesisLedgerForTesting`). Config setting keys are
    /// tracked from the first time the stream shows them, every other key is strict. This is
    /// the mode core's goldens are checked in.
    CompleteExceptConfig,
    Partial,
}

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error(transparent)]
    View(#[from] ViewError),
    #[error("ledger {got} does not follow the state's head ledger {head}")]
    Sequence { head: u32, got: u32 },
    #[error("ledger {seq} {at}: {problem}")]
    Order {
        seq: u32,
        at: String,
        problem: String,
    },
    #[error("ledger {seq} {at}: {problem} for {key}")]
    Inconsistent {
        seq: u32,
        at: String,
        key: String,
        problem: &'static str,
    },
}

/// What one applied ledger did to the state, and how much of it could be checked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApplyReport {
    pub ledger_seq: u32,
    /// Changes checked against a tracked entry.
    pub checked: u64,
    /// Changes to keys the state had never seen (only in `Partial` mode).
    pub unchecked: u64,
    pub created: u64,
    pub updated: u64,
    pub removed: u64,
    pub restored: u64,
    pub evicted: u64,
}

#[derive(Debug, Clone)]
pub struct LedgerState {
    tracking: Tracking,
    live: BTreeMap<LedgerKey, LedgerEntry>,
    // persistent soroban entries evicted to the hot archive; they come back only through
    // a RESTORED change
    archived: BTreeMap<LedgerKey, LedgerEntry>,
    head_seq: Option<u32>,
    header: Option<LedgerHeader>,
}

impl LedgerState {
    /// The state core creates for a new network: one root account holding every lumen
    /// (`LedgerManagerImpl::startNewLedger`). Release builds never create the extra
    /// `GENESIS_TEST_ACCOUNT_COUNT` accounts, which are compiled only with BUILD_TESTS.
    pub fn genesis(network: &Network) -> Self {
        let root = LedgerEntry {
            last_modified_ledger_seq: GENESIS_LEDGER_SEQ,
            data: LedgerEntryData::Account(AccountEntry {
                account_id: network.root_account(),
                balance: GENESIS_TOTAL_COINS,
                seq_num: SequenceNumber(0),
                num_sub_entries: 0,
                inflation_dest: None,
                flags: 0,
                home_domain: String32::default(),
                thresholds: Thresholds([1, 0, 0, 0]),
                signers: Default::default(),
                ext: AccountEntryExt::V0,
            }),
            ext: LedgerEntryExt::V0,
        };
        let mut live = BTreeMap::new();
        live.insert(root.to_key(), root);
        LedgerState {
            tracking: Tracking::Complete,
            live,
            archived: BTreeMap::new(),
            head_seq: Some(GENESIS_LEDGER_SEQ),
            header: None,
        }
    }

    /// Genesis as a core test application creates it (how every core golden starts): the
    /// root account as in [`LedgerState::genesis`], plus Soroban config settings written
    /// without meta, which are tracked from their first appearance.
    pub fn core_test_genesis(network: &Network) -> Self {
        LedgerState {
            tracking: Tracking::CompleteExceptConfig,
            ..LedgerState::genesis(network)
        }
    }

    /// A state seeded from a known snapshot, such as a history-archive checkpoint: every
    /// live entry plus the header of the ledger the snapshot was taken at.
    pub fn seeded(
        header: LedgerHeader,
        live: impl IntoIterator<Item = LedgerEntry>,
        archived: impl IntoIterator<Item = LedgerEntry>,
    ) -> Self {
        LedgerState {
            tracking: Tracking::Complete,
            live: live.into_iter().map(|e| (e.to_key(), e)).collect(),
            archived: archived.into_iter().map(|e| (e.to_key(), e)).collect(),
            head_seq: Some(header.ledger_seq),
            header: Some(header),
        }
    }

    /// A state that knows nothing before the first applied ledger.
    pub fn partial() -> Self {
        LedgerState {
            tracking: Tracking::Partial,
            live: BTreeMap::new(),
            archived: BTreeMap::new(),
            head_seq: None,
            header: None,
        }
    }

    pub fn tracking(&self) -> Tracking {
        self.tracking
    }

    /// Sequence of the last applied ledger (1 right after genesis).
    pub fn head_seq(&self) -> Option<u32> {
        self.head_seq
    }

    /// Header of the last applied ledger, once one has been applied or seeded.
    pub fn header(&self) -> Option<&LedgerHeader> {
        self.header.as_ref()
    }

    /// The live entry for `key` at the head ledger. Archived and evicted entries are not
    /// live, which is what `getLedgerEntries` answers too. For the keys
    /// [`written_without_meta`] names this is the value the meta last showed.
    pub fn get(&self, key: &LedgerKey) -> Option<&LedgerEntry> {
        self.live.get(key)
    }

    /// The archived value of a persistent entry evicted to the hot archive.
    pub fn get_archived(&self, key: &LedgerKey) -> Option<&LedgerEntry> {
        self.archived.get(key)
    }

    pub fn len(&self) -> usize {
        self.live.len()
    }

    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&LedgerKey, &LedgerEntry)> {
        self.live.iter()
    }

    /// Applies one ledger in core's order. The state must be at the previous ledger, except
    /// a `Partial` state that has not applied anything yet.
    pub fn apply_ledger(
        &mut self,
        lcm: &LedgerCloseMeta,
        network: &Network,
    ) -> Result<ApplyReport, StateError> {
        let view = LedgerView::new(lcm, network)?;
        let seq = view.seq();
        if let Some(head) = self.head_seq
            && head.checked_add(1) != Some(seq)
        {
            return Err(StateError::Sequence { head, got: seq });
        }
        let mut report = ApplyReport {
            ledger_seq: seq,
            ..ApplyReport::default()
        };
        for tx in &view.txs {
            self.apply_changes(
                tx.fee_processing,
                seq,
                &label(tx.index, Scope::FeeProcessing),
                &mut report,
            )?;
        }
        // sequential phases run each transaction start to end; a parallel phase first
        // commits every transaction's changes-before, then runs their operations
        let mut i = 0;
        while i < view.txs.len() {
            let tx = &view.txs[i];
            if !tx.parallel {
                for list in tx.apply_lists() {
                    self.apply_changes_in(
                        list.changes,
                        seq,
                        &label(tx.index, list.scope),
                        false,
                        &mut report,
                    )?;
                }
                i += 1;
                continue;
            }
            let end = view.txs[i..]
                .iter()
                .position(|t| !t.parallel)
                .map_or(view.txs.len(), |p| i + p);
            for tx in &view.txs[i..end] {
                if let Some(before) = tx.tx_changes_before() {
                    self.apply_changes_in(
                        before,
                        seq,
                        &label(tx.index, Scope::TxBefore),
                        true,
                        &mut report,
                    )?;
                }
            }
            for tx in &view.txs[i..end] {
                for list in tx.apply_lists_after_before() {
                    self.apply_changes_in(
                        list.changes,
                        seq,
                        &label(tx.index, list.scope),
                        true,
                        &mut report,
                    )?;
                }
            }
            i = end;
        }
        for tx in &view.txs {
            if let Some(post) = tx.post_apply_fee {
                self.apply_changes(
                    post,
                    seq,
                    &label(tx.index, Scope::PostApplyFee),
                    &mut report,
                )?;
            }
        }
        for (i, up) in view.upgrades.iter().enumerate() {
            self.apply_changes(&up.changes, seq, &format!("upgrade {i}"), &mut report)?;
        }
        self.evict(view.evicted, seq, &mut report)?;
        self.head_seq = Some(seq);
        self.header = Some(view.header.clone());
        Ok(report)
    }

    /// Applies one change list. Exposed so a caller holding meta in another form (a single
    /// transaction's meta, an archive delta) can drive the same checks.
    pub fn apply_changes(
        &mut self,
        changes: &[LedgerEntryChange],
        seq: u32,
        at: &str,
        report: &mut ApplyReport,
    ) -> Result<(), StateError> {
        self.apply_changes_in(changes, seq, at, false, report)
    }

    // `parallel` enables CAP-63's read-only TTL bump merge: transactions in different
    // clusters of a stage see the TTL as it was when the stage began, so a TTL's STATE may
    // lag the stored value and its new value is the max of the competing bumps
    fn apply_changes_in(
        &mut self,
        changes: &[LedgerEntryChange],
        seq: u32,
        at: &str,
        parallel: bool,
        report: &mut ApplyReport,
    ) -> Result<(), StateError> {
        // one key's changes are read in the order core emitted them, which the canonical
        // form keeps; a key whose changes do not chain is refused before anything is applied
        let ordered = crate::order::raw_order(changes).map_err(|problem| StateError::Order {
            seq,
            at: at.to_string(),
            problem,
        })?;
        for change in ordered {
            match change {
                LedgerEntryChange::Created(e) => {
                    let key = e.to_key();
                    if self.live.contains_key(&key) {
                        return Err(bad(seq, at, &key, "created entry already exists"));
                    }
                    if self.archived.contains_key(&key) {
                        return Err(bad(seq, at, &key, "created entry is in the hot archive"));
                    }
                    self.count_known(true, &key, report);
                    report.created += 1;
                    self.live.insert(key, e.clone());
                }
                LedgerEntryChange::State(e) => {
                    let key = e.to_key();
                    match self.live.get(&key) {
                        Some(cur) => {
                            let lagging_ttl = parallel
                                && match (&cur.data, &e.data) {
                                    (LedgerEntryData::Ttl(c), LedgerEntryData::Ttl(s)) => {
                                        s.live_until_ledger_seq <= c.live_until_ledger_seq
                                    }
                                    _ => false,
                                };
                            if cur != e && written_without_meta(&key) {
                                // core moved it since the meta last showed it; this STATE
                                // is the first sight of the new value
                                report.unchecked += 1;
                                self.live.insert(key, e.clone());
                                continue;
                            }
                            if cur != e && !lagging_ttl {
                                return Err(bad(
                                    seq,
                                    at,
                                    &key,
                                    "STATE differs from the stored entry",
                                ));
                            }
                            report.checked += 1;
                        }
                        None => {
                            if self.strict(&key) {
                                return Err(bad(
                                    seq,
                                    at,
                                    &key,
                                    "STATE for an entry that does not exist",
                                ));
                            }
                            report.unchecked += 1;
                            self.live.insert(key, e.clone());
                        }
                    }
                }
                LedgerEntryChange::Updated(e) => {
                    let key = e.to_key();
                    let known = self.live.contains_key(&key);
                    if !known && self.strict(&key) {
                        return Err(bad(seq, at, &key, "updated entry does not exist"));
                    }
                    self.count_known(known, &key, report);
                    report.updated += 1;
                    let keep_stored = parallel
                        && match (self.live.get(&key).map(|c| &c.data), &e.data) {
                            (Some(LedgerEntryData::Ttl(c)), LedgerEntryData::Ttl(n)) => {
                                c.live_until_ledger_seq > n.live_until_ledger_seq
                            }
                            _ => false,
                        };
                    if !keep_stored {
                        self.live.insert(key, e.clone());
                    }
                }
                LedgerEntryChange::Removed(key) => {
                    let known = self.live.remove(key).is_some();
                    if !known && self.strict(key) {
                        return Err(bad(seq, at, key, "removed entry does not exist"));
                    }
                    self.count_known(known, key, report);
                    report.removed += 1;
                }
                LedgerEntryChange::Restored(e) => {
                    let key = e.to_key();
                    self.restore(&key, e, seq, at, report)?;
                    report.restored += 1;
                    self.live.insert(key, e.clone());
                }
            }
        }
        Ok(())
    }

    // a restored data or code entry comes back with its archived (or expired live) value,
    // only lastModifiedLedgerSeq may move; a restored TTL carries its new live-until value
    // (TransactionMeta.cpp, processOpLedgerEntryChanges), so only its presence is checked
    fn restore(
        &mut self,
        key: &LedgerKey,
        e: &LedgerEntry,
        seq: u32,
        at: &str,
        report: &mut ApplyReport,
    ) -> Result<(), StateError> {
        let prior = match self.archived.remove(key) {
            Some(a) => {
                if self.live.contains_key(key) {
                    return Err(bad(seq, at, key, "entry is both live and archived"));
                }
                Some(a)
            }
            None => self.live.get(key).cloned(),
        };
        match prior {
            Some(p) => {
                if !matches!(key, LedgerKey::Ttl(_)) && p.data != e.data {
                    return Err(bad(
                        seq,
                        at,
                        key,
                        "RESTORED value differs from the archived entry",
                    ));
                }
                report.checked += 1;
            }
            None => {
                // a hot-archive restore recreates the TTL entry that eviction deleted
                let ttl_from_archive = matches!(key, LedgerKey::Ttl(_));
                if self.strict(key) && !ttl_from_archive {
                    return Err(bad(seq, at, key, "RESTORED entry was never archived"));
                }
                if ttl_from_archive {
                    report.checked += 1;
                } else {
                    report.unchecked += 1;
                }
            }
        }
        Ok(())
    }

    fn evict(
        &mut self,
        keys: &[LedgerKey],
        seq: u32,
        report: &mut ApplyReport,
    ) -> Result<(), StateError> {
        for key in keys {
            let entry = self.live.remove(key);
            let Some(entry) = entry else {
                if self.strict(key) {
                    return Err(bad(seq, "eviction", key, "evicted entry does not exist"));
                }
                report.unchecked += 1;
                report.evicted += 1;
                continue;
            };
            report.checked += 1;
            report.evicted += 1;
            if is_persistent(&entry.data) {
                self.archived.insert(key.clone(), entry);
            }
        }
        Ok(())
    }

    // whether a key's prior state is fully known, so that its absence is an error
    fn strict(&self, key: &LedgerKey) -> bool {
        match self.tracking {
            Tracking::Complete => true,
            Tracking::CompleteExceptConfig => !matches!(key, LedgerKey::ConfigSetting(_)),
            Tracking::Partial => false,
        }
    }

    fn count_known(&self, known: bool, key: &LedgerKey, report: &mut ApplyReport) {
        if known || self.strict(key) {
            report.checked += 1;
        } else {
            report.unchecked += 1;
        }
    }
}

/// Entries core rewrites while closing a ledger without reporting it in meta, so the state
/// holds the value the meta last showed, which may be stale at the head. Both are config
/// settings written after the transactions and upgrades, in
/// `LedgerManagerImpl::finalizeLedgerTxnChanges` (v30.0.0): the eviction scan
/// stores its new position on every Soroban ledger (`BucketManager::resolveBackgroundEvictionScan`,
/// `SorobanNetworkConfig::updateEvictionIterator`), and every
/// `liveSorobanStateSizeWindowSamplePeriod` ledgers the state size window drops its oldest
/// sample and appends one (`SorobanNetworkConfig::maybeSnapshotSorobanStateSize`). Neither
/// value can be derived from meta: one is a bucket file position, the other core's in-memory
/// state size. A STATE that shows a newer value is taken, not treated as an inconsistency.
pub fn written_without_meta(key: &LedgerKey) -> bool {
    matches!(
        key,
        LedgerKey::ConfigSetting(k) if matches!(
            k.config_setting_id,
            ConfigSettingId::EvictionIterator | ConfigSettingId::LiveSorobanStateSizeWindow
        )
    )
}

fn is_persistent(data: &LedgerEntryData) -> bool {
    match data {
        LedgerEntryData::ContractData(d) => d.durability == ContractDataDurability::Persistent,
        LedgerEntryData::ContractCode(_) => true,
        _ => false,
    }
}

fn label(tx: u32, scope: Scope) -> String {
    match scope {
        Scope::FeeProcessing => format!("tx {tx} fee processing"),
        Scope::TxBefore => format!("tx {tx} changes before"),
        Scope::Operation(i) => format!("tx {tx} op {i}"),
        Scope::TxAfter => format!("tx {tx} changes after"),
        Scope::PostApplyFee => format!("tx {tx} post-apply fee processing"),
        Scope::Upgrade(i) => format!("upgrade {i}"),
    }
}

fn bad(seq: u32, at: &str, key: &LedgerKey, problem: &'static str) -> StateError {
    StateError::Inconsistent {
        seq,
        at: at.to_string(),
        key: describe_key(key),
        problem,
    }
}

/// A short, stable description of a key for error messages and reports.
pub fn describe_key(key: &LedgerKey) -> String {
    use stellar_xdr::LedgerKey as K;
    match key {
        K::Account(k) => format!("account {}", k.account_id),
        K::Trustline(k) => format!(
            "trustline {} {}",
            k.account_id,
            crate::vocab::trustline_asset_id(&k.asset)
        ),
        K::Offer(k) => format!("offer {} of {}", k.offer_id, k.seller_id),
        K::Data(k) => format!(
            "data {} of {}",
            hex::encode(k.data_name.as_slice()),
            k.account_id
        ),
        K::ClaimableBalance(k) => format!("claimable balance {}", k.balance_id),
        K::LiquidityPool(k) => format!("liquidity pool {}", k.liquidity_pool_id),
        K::ContractData(k) => format!("contract data of {} key {}", k.contract, xdr_hex(&k.key)),
        K::ContractCode(k) => format!("contract code {}", hex::encode(k.hash.0)),
        K::ConfigSetting(k) => format!("config setting {:?}", k.config_setting_id),
        K::Ttl(k) => format!("ttl {}", hex::encode(k.key_hash.0)),
    }
}

fn xdr_hex<T: WriteXdr>(v: &T) -> String {
    hex::encode(
        v.to_xdr(Limits::none())
            .expect("a decoded value re-encodes"),
    )
}
