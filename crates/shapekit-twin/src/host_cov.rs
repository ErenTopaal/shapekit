//! Replays a recorded chain's contract invocations through `soroban-env-host`
//! inside this process, with the inputs core handed to the host, and compares
//! what the host returns with what core wrote into the chain.
//!
//! `--enable-codecoverage` only instruments core's C and C++ code; the
//! contract host is Rust and is linked into core as a static library. Running
//! the host's own entry point here, in a binary built with
//! `-C instrument-coverage`, measures the host instead (`twin/host-cov.sh`).
//! The measurement only describes the chain if the host does here exactly
//! what it did inside core, which is why every replayed call is checked
//! against the meta: return value, ledger entry changes and contract events.
//!
//! The inputs are rebuilt the way stellar-core v30.0.0 builds them:
//!
//! - entry point: `invoke_host_function_v2` (`src/rust/src/soroban_proto_any.rs`),
//!   which calls `e2e_invoke::invoke_host_function` with entries and TTLs in
//!   footprint order, read-only keys first.
//! - budget: `Budget::try_from_configs(resources.instructions, txMemoryLimit,
//!   cpu cost params, memory cost params)` (`run_invoke_and_build_output`).
//! - ledger info: `getLedgerInfo` in `InvokeHostFunctionOpFrame.cpp`. During
//!   apply the header already carries the new sequence number and close time,
//!   but protocol version and base reserve are still the previous ledger's,
//!   because upgrades are applied after the transactions.
//! - PRNG seed: the contents hash of the transaction set is the base seed
//!   (`LedgerManagerImpl::applyTransactions`); a transaction in a parallel
//!   phase gets `subSha256(base, txNum)` (`applyThread`), where `txNum` is its
//!   index in apply order, which is also its index in `txProcessing`.
//!   `subSha256` hashes the seed followed by the counter as an 8-byte
//!   big-endian XDR `uint64` (`src/crypto/SHA.cpp`).
//! - module cache: core passes its module cache to every call, so a
//!   contract's instantiation is charged as cached
//!   (`SorobanModuleCache`, `src/rust/src/soroban_module_cache.rs`). Core
//!   compiles every contract code entry at startup and adds code written by a
//!   ledger once that ledger is committed, so the replay builds a cache of all
//!   code in the state at the start of each ledger, compiled for the host's
//!   protocol under an unlimited budget with empty cost parameters
//!   (`CoreCompilationContext`, `soroban_proto_any.rs`).
//! - rent sizes: `ledgerEntrySizeForRent` (`LedgerTypeUtils.cpp`), which from
//!   protocol 28 adds the module memory cost computed under a budget with
//!   zero limits; `e2e_invoke::entry_size_for_rent` is the same computation.
//!
//! The ledger state the inputs are read from is rebuilt from the chain itself:
//! every change core reported (fee processing, transaction apply, upgrades,
//! evictions) is applied in order, so the state before a transaction is exact
//! for every entry the chain created or touched.
//!
//! Only what the replay can rebuild exactly is accepted: protocol 30 ledgers
//! (the host here is the protocol 30 host), transactions in a parallel
//! Soroban phase, and footprints without archived entries to restore. Anything
//! else is an error, not an approximation.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use soroban_env_host::budget::AsBudget;
use soroban_env_host::budget::Budget;
use soroban_env_host::e2e_invoke::{self, LedgerEntryChange as HostChange, TtlLedgerEntryMeta};
use soroban_env_host::{CompilationContext, ErrorHandler, HostError, LedgerInfo, ModuleCache, Val};
use stellar_xdr::{
    AccountId, ConfigSettingEntry, ConfigSettingId, ContractCostParams, ContractEvent,
    GeneralizedTransactionSet, LedgerCloseMeta, LedgerCloseMetaV2, LedgerEntry, LedgerEntryChange,
    LedgerEntryChanges, LedgerEntryData, LedgerKey, LedgerKeyConfigSetting, LedgerKeyTtl, Limits,
    MuxedAccount, OperationBody, PublicKey, ReadXdr, ScVal, SorobanTransactionDataExt,
    TransactionEnvelope, TransactionMeta, TransactionPhase, TtlEntry, Uint256, WriteXdr,
};

/// Host version this module replays with; reported next to every result.
pub const HOST_VERSION: &str = "30.0.0";

/// The only ledger protocol the linked host runs.
pub const HOST_PROTOCOL: u32 = 30;

#[derive(Debug, thiserror::Error)]
pub enum HostCovError {
    #[error("ledger {ledger}: {what}")]
    Unsupported { ledger: u32, what: String },
    #[error("ledger {ledger}: {what}")]
    Inconsistent { ledger: u32, what: String },
    #[error("xdr: {0}")]
    Xdr(#[from] stellar_xdr::Error),
    #[error("host: {0:?}")]
    Host(soroban_env_host::HostError),
}

impl From<soroban_env_host::HostError> for HostCovError {
    fn from(e: soroban_env_host::HostError) -> Self {
        Self::Host(e)
    }
}

/// Ledger state rebuilt from the chain's meta, keyed by the key's XDR.
#[derive(Debug, Default, Clone)]
pub struct LedgerState {
    entries: BTreeMap<Vec<u8>, LedgerEntry>,
}

fn key_bytes(k: &LedgerKey) -> Result<Vec<u8>, HostCovError> {
    Ok(k.to_xdr(Limits::none())?)
}

impl LedgerState {
    pub fn get(&self, key: &LedgerKey) -> Result<Option<&LedgerEntry>, HostCovError> {
        Ok(self.entries.get(&key_bytes(key)?))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Applies one change list. A `State` entry is the value before the
    /// change that follows it; it is only taken when the entry is not known
    /// yet (genesis entries never appear as created), never over a value the
    /// chain already set.
    pub fn apply(&mut self, changes: &LedgerEntryChanges) -> Result<(), HostCovError> {
        for c in changes.iter() {
            match c {
                LedgerEntryChange::Created(e)
                | LedgerEntryChange::Updated(e)
                | LedgerEntryChange::Restored(e) => {
                    self.entries.insert(key_bytes(&e.to_key())?, e.clone());
                }
                LedgerEntryChange::Removed(k) => {
                    self.entries.remove(&key_bytes(k)?);
                }
                LedgerEntryChange::State(e) => {
                    self.entries
                        .entry(key_bytes(&e.to_key())?)
                        .or_insert_with(|| e.clone());
                }
            }
        }
        Ok(())
    }

    pub fn remove(&mut self, key: &LedgerKey) -> Result<(), HostCovError> {
        self.entries.remove(&key_bytes(key)?);
        Ok(())
    }

    fn config(
        &self,
        id: ConfigSettingId,
        ledger: u32,
    ) -> Result<&ConfigSettingEntry, HostCovError> {
        let key = LedgerKey::ConfigSetting(LedgerKeyConfigSetting {
            config_setting_id: id,
        });
        match self.get(&key)?.map(|e| &e.data) {
            Some(LedgerEntryData::ConfigSetting(c)) => Ok(c),
            _ => Err(HostCovError::Inconsistent {
                ledger,
                what: format!("config setting {id:?} is not in the rebuilt state"),
            }),
        }
    }
}

/// The network settings the host call reads.
struct HostSettings {
    tx_memory_limit: u32,
    min_temp_ttl: u32,
    min_persistent_ttl: u32,
    max_entry_ttl: u32,
    cpu: ContractCostParams,
    mem: ContractCostParams,
}

fn host_settings(state: &LedgerState, ledger: u32) -> Result<HostSettings, HostCovError> {
    let tx_memory_limit = match state.config(ConfigSettingId::ContractComputeV0, ledger)? {
        ConfigSettingEntry::ContractComputeV0(c) => c.tx_memory_limit,
        _ => unreachable!("the key selects the arm"),
    };
    let (min_temp_ttl, min_persistent_ttl, max_entry_ttl) =
        match state.config(ConfigSettingId::StateArchival, ledger)? {
            ConfigSettingEntry::StateArchival(s) => {
                (s.min_temporary_ttl, s.min_persistent_ttl, s.max_entry_ttl)
            }
            _ => unreachable!("the key selects the arm"),
        };
    let cpu = match state.config(ConfigSettingId::ContractCostParamsCpuInstructions, ledger)? {
        ConfigSettingEntry::ContractCostParamsCpuInstructions(p) => p.clone(),
        _ => unreachable!("the key selects the arm"),
    };
    let mem = match state.config(ConfigSettingId::ContractCostParamsMemoryBytes, ledger)? {
        ConfigSettingEntry::ContractCostParamsMemoryBytes(p) => p.clone(),
        _ => unreachable!("the key selects the arm"),
    };
    Ok(HostSettings {
        tx_memory_limit,
        min_temp_ttl,
        min_persistent_ttl,
        max_entry_ttl,
        cpu,
        mem,
    })
}

/// `subSha256` of stellar-core: SHA-256 of the seed then the counter as an
/// XDR `uint64`.
pub fn sub_sha256(seed: &[u8; 32], counter: u64) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(seed);
    h.update(counter.to_be_bytes());
    h.finalize().into()
}

fn account_of(m: &MuxedAccount) -> AccountId {
    let k = match m {
        MuxedAccount::Ed25519(k) => k.clone(),
        MuxedAccount::MuxedEd25519(m) => m.ed25519.clone(),
    };
    AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(k.0)))
}

fn is_soroban_entry(k: &LedgerKey) -> bool {
    matches!(k, LedgerKey::ContractData(_) | LedgerKey::ContractCode(_))
}

fn ttl_key(k: &LedgerKey) -> Result<LedgerKey, HostCovError> {
    let hash: [u8; 32] = Sha256::digest(k.to_xdr(Limits::none())?).into();
    Ok(LedgerKey::Ttl(LedgerKeyTtl {
        key_hash: stellar_xdr::Hash(hash),
    }))
}

/// What core compiles modules with: an unlimited budget over empty cost
/// parameters, so compiling is never charged to a transaction.
#[derive(Clone)]
struct CoreCompilationContext {
    unlimited_budget: Budget,
}

impl CoreCompilationContext {
    fn new() -> Result<Self, HostError> {
        Ok(Self {
            unlimited_budget: Budget::try_from_configs(
                u64::MAX,
                u64::MAX,
                ContractCostParams(Default::default()),
                ContractCostParams(Default::default()),
            )?,
        })
    }
}

impl CompilationContext for CoreCompilationContext {}

impl AsBudget for CoreCompilationContext {
    fn as_budget(&self) -> &Budget {
        &self.unlimited_budget
    }
}

impl ErrorHandler for CoreCompilationContext {
    fn map_err<T, E>(&self, res: Result<T, E>) -> Result<T, HostError>
    where
        soroban_env_host::Error: From<E>,
        E: core::fmt::Debug,
    {
        res.map_err(HostError::from)
    }

    fn error(&self, error: soroban_env_host::Error, _msg: &str, _args: &[Val]) -> HostError {
        HostError::from(error)
    }
}

/// A module cache holding every contract code entry of `state`.
fn module_cache(state: &LedgerState) -> Result<ModuleCache, HostCovError> {
    let ctx = CoreCompilationContext::new()?;
    let cache = ModuleCache::new(&ctx)?;
    for e in state.entries.values() {
        if let LedgerEntryData::ContractCode(code) = &e.data {
            cache.parse_and_cache_module_simple(
                &CoreCompilationContext::new()?,
                HOST_PROTOCOL,
                code.code.as_slice(),
            )?;
        }
    }
    Ok(cache)
}

/// A ledger write, as the meta states it or as the host's output implies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Write {
    Upsert(Box<LedgerEntry>),
    Delete,
}

/// What core's transaction result says about the invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreResult {
    /// `INVOKE_HOST_FUNCTION_SUCCESS` with the hash of the success preimage.
    Success([u8; 32]),
    Trapped,
    ResourceLimitExceeded,
    InsufficientRefundableFee,
    EntryArchived,
    Malformed,
    /// The operation was not applied: the transaction failed before it.
    NotApplied(String),
}

/// How a replay compares with the chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The host did what core recorded.
    Verified,
    /// The host's outcome contradicts the chain.
    Differs(String),
    /// The host ran, but the chain cannot confirm that core ran it the same
    /// way: core failed the operation with a code it also sets before
    /// calling the host.
    Unverified(String),
    /// Core never called the host for this operation, so neither does the
    /// replay.
    NotRunByCore,
    /// The replay cannot rebuild this call's inputs exactly and did not run
    /// it.
    Skipped(String),
}

/// What the host returned in this process.
#[derive(Debug, Clone)]
pub struct HostRun {
    pub success: bool,
    pub error: Option<String>,
    pub return_value: Option<ScVal>,
    /// Writes keyed by the hex of the key's XDR.
    pub writes: BTreeMap<String, Write>,
    pub events: Vec<ContractEvent>,
    pub cpu_insns: u64,
    pub mem_bytes: u64,
}

/// One replayed invocation and how it compares with the chain.
#[derive(Debug, Clone)]
pub struct InvocationCheck {
    pub ledger: u32,
    pub tx_index: usize,
    pub tx_hash: [u8; 32],
    pub core: CoreResult,
    pub host: Option<HostRun>,
    pub meta_return: Option<ScVal>,
    pub meta_writes: BTreeMap<String, Write>,
    pub meta_events: Vec<ContractEvent>,
    pub verdict: Verdict,
}

fn lcm_v2(lcm: &LedgerCloseMeta) -> Option<&LedgerCloseMetaV2> {
    match lcm {
        LedgerCloseMeta::V2(m) => Some(m),
        _ => None,
    }
}

/// Every transaction envelope of the set, by hash, and whether it sits in a
/// parallel phase.
fn envelopes(
    set: &GeneralizedTransactionSet,
    network_id: [u8; 32],
) -> Result<BTreeMap<[u8; 32], (TransactionEnvelope, bool)>, HostCovError> {
    let GeneralizedTransactionSet::V1(v1) = set;
    let mut out = BTreeMap::new();
    for phase in v1.phases.iter() {
        match phase {
            TransactionPhase::V0(components) => {
                for c in components.iter() {
                    let stellar_xdr::TxSetComponent::TxsetCompTxsMaybeDiscountedFee(c) = c;
                    for e in c.txs.iter() {
                        out.insert(e.hash(network_id)?, (e.clone(), false));
                    }
                }
            }
            TransactionPhase::V1(p) => {
                for stage in p.execution_stages.iter() {
                    for cluster in stage.0.iter() {
                        for e in cluster.0.iter() {
                            out.insert(e.hash(network_id)?, (e.clone(), true));
                        }
                    }
                }
            }
        }
    }
    Ok(out)
}

/// Replays every `InvokeHostFunction` transaction of the chain and compares
/// it with the meta. `frames` must start at the chain's first ledger.
pub fn replay(
    lcms: &[LedgerCloseMeta],
    network_id: [u8; 32],
) -> Result<Vec<InvocationCheck>, HostCovError> {
    let mut state = LedgerState::default();
    let mut checks = Vec::new();
    let mut prev_header: Option<(u32, u32)> = None; // (protocol, base reserve)
    // meta before v2 has no post-apply fee changes
    let no_changes = LedgerEntryChanges::default();

    for lcm in lcms {
        let seq = shapekit_core_driver::meta::ledger_seq(lcm);
        let header_now = match lcm {
            LedgerCloseMeta::V0(m) => &m.ledger_header.header,
            LedgerCloseMeta::V1(m) => &m.ledger_header.header,
            LedgerCloseMeta::V2(m) => &m.ledger_header.header,
        };
        let header_now = (header_now.ledger_version, header_now.base_reserve);

        // fee processing of every transaction comes before any apply
        let txp: Vec<(
            &LedgerEntryChanges,
            &TransactionMeta,
            &LedgerEntryChanges,
            [u8; 32],
        )> = match lcm {
            LedgerCloseMeta::V0(m) => m
                .tx_processing
                .iter()
                .map(|t| {
                    (
                        &t.fee_processing,
                        &t.tx_apply_processing,
                        &no_changes,
                        t.result.transaction_hash.0,
                    )
                })
                .collect(),
            LedgerCloseMeta::V1(m) => m
                .tx_processing
                .iter()
                .map(|t| {
                    (
                        &t.fee_processing,
                        &t.tx_apply_processing,
                        &no_changes,
                        t.result.transaction_hash.0,
                    )
                })
                .collect(),
            LedgerCloseMeta::V2(m) => m
                .tx_processing
                .iter()
                .map(|t| {
                    (
                        &t.fee_processing,
                        &t.tx_apply_processing,
                        &t.post_tx_apply_fee_processing,
                        t.result.transaction_hash.0,
                    )
                })
                .collect(),
        };
        for (fee, _, _, _) in &txp {
            state.apply(fee)?;
        }

        let invokes = has_invocation(lcm, network_id)?;
        let cache = if invokes {
            Some(module_cache(&state)?)
        } else {
            None
        };
        let envs = match (invokes, lcm_v2(lcm)) {
            (false, _) => BTreeMap::new(),
            (true, Some(v2)) => envelopes(&v2.tx_set, network_id)?,
            (true, None) => {
                return Err(HostCovError::Unsupported {
                    ledger: seq,
                    what: "contract invocation in a ledger close meta before v2".into(),
                });
            }
        };

        for (i, (_, meta, _, hash)) in txp.iter().enumerate() {
            // the transaction-level changes (sequence number, one-time
            // signers) happen before its operations run
            apply_tx_before(&mut state, meta)?;
            if let Some((env, parallel)) = envs.get(hash) {
                let v2 = lcm_v2(lcm).expect("envelopes only collected for v2");
                let cache = cache.as_ref().expect("built when the ledger invokes");
                let pair = &v2.tx_processing[i].result;
                if let Some(check) = replay_one(
                    &state,
                    v2,
                    seq,
                    prev_header,
                    i,
                    pair,
                    env,
                    *parallel,
                    meta,
                    network_id,
                    cache,
                )? {
                    checks.push(check);
                }
            }
            apply_tx_rest(&mut state, meta)?;
        }
        for (_, _, post, _) in &txp {
            state.apply(post)?;
        }

        match lcm {
            LedgerCloseMeta::V0(m) => {
                for u in m.upgrades_processing.iter() {
                    state.apply(&u.changes)?;
                }
            }
            LedgerCloseMeta::V1(m) => {
                for u in m.upgrades_processing.iter() {
                    state.apply(&u.changes)?;
                }
                for k in m.evicted_keys.iter() {
                    state.remove(k)?;
                }
            }
            LedgerCloseMeta::V2(m) => {
                for u in m.upgrades_processing.iter() {
                    state.apply(&u.changes)?;
                }
                for k in m.evicted_keys.iter() {
                    state.remove(k)?;
                }
            }
        }
        prev_header = Some(header_now);
    }
    Ok(checks)
}

fn invoke_op(env: &TransactionEnvelope) -> Option<(&stellar_xdr::Transaction, usize)> {
    let tx = match env {
        TransactionEnvelope::Tx(e) => &e.tx,
        TransactionEnvelope::TxFeeBump(e) => match &e.tx.inner_tx {
            stellar_xdr::FeeBumpTransactionInnerTx::Tx(i) => &i.tx,
        },
        TransactionEnvelope::TxV0(_) => return None,
    };
    tx.operations
        .iter()
        .position(|o| matches!(o.body, OperationBody::InvokeHostFunction(_)))
        .map(|i| (tx, i))
}

fn has_invocation(lcm: &LedgerCloseMeta, network_id: [u8; 32]) -> Result<bool, HostCovError> {
    let envs: Vec<TransactionEnvelope> = match lcm {
        LedgerCloseMeta::V0(m) => m.tx_set.txs.to_vec(),
        LedgerCloseMeta::V1(m) => envelopes(&m.tx_set, network_id)?
            .into_values()
            .map(|(e, _)| e)
            .collect(),
        LedgerCloseMeta::V2(m) => envelopes(&m.tx_set, network_id)?
            .into_values()
            .map(|(e, _)| e)
            .collect(),
    };
    Ok(envs.iter().any(|e| invoke_op(e).is_some()))
}

fn apply_tx_before(state: &mut LedgerState, meta: &TransactionMeta) -> Result<(), HostCovError> {
    match meta {
        TransactionMeta::V0(_) => Ok(()),
        TransactionMeta::V1(m) => state.apply(&m.tx_changes),
        TransactionMeta::V2(m) => state.apply(&m.tx_changes_before),
        TransactionMeta::V3(m) => state.apply(&m.tx_changes_before),
        TransactionMeta::V4(m) => state.apply(&m.tx_changes_before),
    }
}

fn apply_tx_rest(state: &mut LedgerState, meta: &TransactionMeta) -> Result<(), HostCovError> {
    let (ops, after): (Vec<&LedgerEntryChanges>, Option<&LedgerEntryChanges>) = match meta {
        TransactionMeta::V0(ops) => (ops.iter().map(|o| &o.changes).collect(), None),
        TransactionMeta::V1(m) => (m.operations.iter().map(|o| &o.changes).collect(), None),
        TransactionMeta::V2(m) => (
            m.operations.iter().map(|o| &o.changes).collect(),
            Some(&m.tx_changes_after),
        ),
        TransactionMeta::V3(m) => (
            m.operations.iter().map(|o| &o.changes).collect(),
            Some(&m.tx_changes_after),
        ),
        TransactionMeta::V4(m) => (
            m.operations.iter().map(|o| &o.changes).collect(),
            Some(&m.tx_changes_after),
        ),
    };
    for c in ops {
        state.apply(c)?;
    }
    if let Some(a) = after {
        state.apply(a)?;
    }
    Ok(())
}

/// The invocation's result in the transaction result.
fn core_result(pair: &stellar_xdr::TransactionResultPair, op_index: usize) -> CoreResult {
    use stellar_xdr::{
        InnerTransactionResultResult as Inner, InvokeHostFunctionResult as R, OperationResult,
        OperationResultTr, TransactionResultResult as Outer,
    };
    let ops = match &pair.result.result {
        Outer::TxSuccess(ops) | Outer::TxFailed(ops) => ops,
        Outer::TxFeeBumpInnerSuccess(i) | Outer::TxFeeBumpInnerFailed(i) => {
            match &i.result.result {
                Inner::TxSuccess(ops) | Inner::TxFailed(ops) => ops,
                other => return CoreResult::NotApplied(format!("{:?}", other.discriminant())),
            }
        }
        other => return CoreResult::NotApplied(format!("{:?}", other.discriminant())),
    };
    match ops.get(op_index) {
        Some(OperationResult::OpInner(OperationResultTr::InvokeHostFunction(r))) => match r {
            R::Success(h) => CoreResult::Success(h.0),
            R::Trapped => CoreResult::Trapped,
            R::ResourceLimitExceeded => CoreResult::ResourceLimitExceeded,
            R::InsufficientRefundableFee => CoreResult::InsufficientRefundableFee,
            R::EntryArchived => CoreResult::EntryArchived,
            R::Malformed => CoreResult::Malformed,
        },
        Some(other) => {
            CoreResult::NotApplied(format!("operation result {:?}", other.discriminant()))
        }
        None => CoreResult::NotApplied("no operation result".into()),
    }
}

#[allow(clippy::too_many_arguments)]
fn replay_one(
    state: &LedgerState,
    lcm: &LedgerCloseMetaV2,
    seq: u32,
    prev_header: Option<(u32, u32)>,
    tx_index: usize,
    pair: &stellar_xdr::TransactionResultPair,
    env: &TransactionEnvelope,
    parallel: bool,
    meta: &TransactionMeta,
    network_id: [u8; 32],
    cache: &ModuleCache,
) -> Result<Option<InvocationCheck>, HostCovError> {
    let Some((tx, op_index)) = invoke_op(env) else {
        return Ok(None);
    };
    let core = core_result(pair, op_index);
    let (meta_return, meta_writes, meta_events) = match meta {
        TransactionMeta::V4(m) => {
            let (w, e) = match m.operations.get(op_index) {
                Some(o) => (meta_writes(&o.changes)?, o.events.to_vec()),
                None => (BTreeMap::new(), Vec::new()),
            };
            (
                m.soroban_meta.as_ref().and_then(|s| s.return_value.clone()),
                w,
                e,
            )
        }
        _ => (None, BTreeMap::new(), Vec::new()),
    };
    let mut check = InvocationCheck {
        ledger: seq,
        tx_index,
        tx_hash: pair.transaction_hash.0,
        core,
        host: None,
        meta_return,
        meta_writes,
        meta_events,
        verdict: Verdict::NotRunByCore,
    };
    if matches!(
        check.core,
        CoreResult::EntryArchived | CoreResult::Malformed | CoreResult::NotApplied(_)
    ) {
        return Ok(Some(check));
    }
    match prepare(
        state,
        lcm,
        seq,
        prev_header,
        tx_index,
        tx,
        op_index,
        parallel,
        network_id,
    )? {
        Err(reason) => check.verdict = Verdict::Skipped(reason),
        Ok(call) => {
            if !matches!(meta, TransactionMeta::V4(_)) {
                return Err(HostCovError::Inconsistent {
                    ledger: seq,
                    what: "protocol 30 transaction without TransactionMeta v4".into(),
                });
            }
            let (instructions, memory) = (call.instructions, call.tx_memory_limit);
            let run = call.run(cache, seq)?;
            check.verdict = judge(&check, &run, instructions, memory)?;
            check.host = Some(run);
        }
    }
    Ok(Some(check))
}

/// The inputs of one host call.
struct Call {
    host_fn: Vec<u8>,
    resources: Vec<u8>,
    source: Vec<u8>,
    auth: Vec<Vec<u8>>,
    ledger_info: LedgerInfo,
    entries: Vec<(Option<Vec<u8>>, Option<TtlLedgerEntryMeta>)>,
    seed: [u8; 32],
    instructions: u32,
    tx_memory_limit: u32,
    cpu: ContractCostParams,
    mem: ContractCostParams,
}

/// Builds the call's inputs, or the reason they cannot be rebuilt exactly.
#[allow(clippy::too_many_arguments)]
fn prepare(
    state: &LedgerState,
    lcm: &LedgerCloseMetaV2,
    seq: u32,
    prev_header: Option<(u32, u32)>,
    tx_index: usize,
    tx: &stellar_xdr::Transaction,
    op_index: usize,
    parallel: bool,
    network_id: [u8; 32],
) -> Result<Result<Call, String>, HostCovError> {
    let Some((protocol, base_reserve)) = prev_header else {
        return Ok(Err("invocation in the chain's first ledger".into()));
    };
    if protocol != HOST_PROTOCOL {
        return Ok(Err(format!(
            "protocol {protocol}; the replay links the protocol {HOST_PROTOCOL} host"
        )));
    }
    if !parallel {
        return Ok(Err("outside a parallel soroban phase".into()));
    }
    let op = &tx.operations[op_index];
    let OperationBody::InvokeHostFunction(ihf) = &op.body else {
        unreachable!("invoke_op found this arm")
    };
    let stellar_xdr::TransactionExt::V1(sdata) = &tx.ext else {
        return Err(HostCovError::Inconsistent {
            ledger: seq,
            what: "invocation without soroban transaction data".into(),
        });
    };
    if let SorobanTransactionDataExt::V1(ext) = &sdata.ext
        && !ext.archived_soroban_entries.is_empty()
    {
        return Ok(Err(
            "restores archived entries; the replay does not rebuild the hot archive".into(),
        ));
    }
    let resources = &sdata.resources;
    let source = account_of(op.source_account.as_ref().unwrap_or(&tx.source_account));

    let settings = host_settings(state, seq)?;
    let header = &lcm.ledger_header.header;
    let set_hash: [u8; 32] = Sha256::digest(lcm.tx_set.to_xdr(Limits::none())?).into();
    if set_hash != header.scp_value.tx_set_hash.0 {
        return Err(HostCovError::Inconsistent {
            ledger: seq,
            what: "transaction set hash differs from the header's txSetHash".into(),
        });
    }

    // entries and ttls in footprint order, read-only first
    let rent_budget = Budget::try_from_configs(0, 0, settings.cpu.clone(), settings.mem.clone())?;
    let mut entries = Vec::new();
    for k in resources
        .footprint
        .read_only
        .iter()
        .chain(resources.footprint.read_write.iter())
    {
        let Some(entry) = state.get(k)? else {
            entries.push((None, None));
            continue;
        };
        let buf = entry.to_xdr(Limits::none())?;
        let ttl = if is_soroban_entry(k) {
            let live_until = match state.get(&ttl_key(k)?)?.map(|e| &e.data) {
                Some(LedgerEntryData::Ttl(TtlEntry {
                    live_until_ledger_seq,
                    ..
                })) => *live_until_ledger_seq,
                _ => {
                    return Err(HostCovError::Inconsistent {
                        ledger: seq,
                        what: format!("no ttl for footprint key {k:?}"),
                    });
                }
            };
            if live_until < seq {
                return Ok(Err(format!(
                    "footprint entry {k:?} is archived (live until {live_until})"
                )));
            }
            let size = e2e_invoke::entry_size_for_rent(&rent_budget, entry, buf.len() as u32)?;
            Some(TtlLedgerEntryMeta {
                live_until_ledger: live_until,
                entry_size_for_rent: size,
            })
        } else {
            None
        };
        entries.push((Some(buf), ttl));
    }

    Ok(Ok(Call {
        host_fn: ihf.host_function.to_xdr(Limits::none())?,
        resources: resources.to_xdr(Limits::none())?,
        source: source.to_xdr(Limits::none())?,
        auth: ihf
            .auth
            .iter()
            .map(|a| a.to_xdr(Limits::none()))
            .collect::<Result<_, _>>()?,
        ledger_info: LedgerInfo {
            protocol_version: protocol,
            sequence_number: seq,
            timestamp: header.scp_value.close_time.0,
            network_id,
            base_reserve,
            min_temp_entry_ttl: settings.min_temp_ttl,
            min_persistent_entry_ttl: settings.min_persistent_ttl,
            max_entry_ttl: settings.max_entry_ttl,
        },
        entries,
        seed: sub_sha256(&set_hash, tx_index as u64),
        instructions: resources.instructions,
        tx_memory_limit: settings.tx_memory_limit,
        cpu: settings.cpu,
        mem: settings.mem,
    }))
}

impl Call {
    fn run(self, cache: &ModuleCache, seq: u32) -> Result<HostRun, HostCovError> {
        let budget = Budget::try_from_configs(
            u64::from(self.instructions),
            u64::from(self.tx_memory_limit),
            self.cpu,
            self.mem,
        )?;
        let mut diagnostics = Vec::new();
        // an Err here is a failed call, as in core's run_invoke_and_build_output
        let res = e2e_invoke::invoke_host_function(
            &budget,
            false,
            self.host_fn,
            self.resources,
            &[],
            self.source,
            self.auth.into_iter(),
            self.ledger_info,
            self.entries.into_iter(),
            self.seed.to_vec(),
            &mut diagnostics,
            None,
            Some(cache.clone()),
        );
        let cpu_insns = budget.get_cpu_insns_consumed()?;
        let mem_bytes = budget.get_mem_bytes_consumed()?;
        let out = match res {
            Ok(out) => out,
            Err(e) => {
                return Ok(HostRun {
                    success: false,
                    error: Some(format!("{e:?}")),
                    return_value: None,
                    writes: BTreeMap::new(),
                    events: Vec::new(),
                    cpu_insns,
                    mem_bytes,
                });
            }
        };
        Ok(match out.encoded_invoke_result {
            Ok(b) => HostRun {
                success: true,
                error: None,
                return_value: Some(ScVal::from_xdr(&b, Limits::none())?),
                writes: host_writes(&out.ledger_changes, seq)?,
                events: out
                    .encoded_contract_events
                    .iter()
                    .map(|b| ContractEvent::from_xdr(b, Limits::none()))
                    .collect::<Result<Vec<_>, _>>()?,
                cpu_insns,
                mem_bytes,
            },
            Err(e) => HostRun {
                success: false,
                error: Some(format!("{e:?}")),
                return_value: None,
                writes: BTreeMap::new(),
                events: Vec::new(),
                cpu_insns,
                mem_bytes,
            },
        })
    }
}

/// `INVOKE_HOST_FUNCTION_SUCCESS` carries SHA-256 of
/// `InvokeHostFunctionSuccessPreImage { returnValue, events }`.
fn success_hash(ret: &ScVal, events: &[ContractEvent]) -> Result<[u8; 32], HostCovError> {
    let pre = stellar_xdr::InvokeHostFunctionSuccessPreImage {
        return_value: ret.clone(),
        events: events.to_vec().try_into()?,
    };
    Ok(Sha256::digest(pre.to_xdr(Limits::none())?).into())
}

/// Compares a host run with core's result, using the code core derives
/// from a failed call (`invokeHostFunction` in `InvokeHostFunctionOpFrame.cpp`):
/// instructions over the declared limit or memory over the network limit
/// give `RESOURCE_LIMIT_EXCEEDED`, any other failure `TRAPPED`.
fn judge(
    c: &InvocationCheck,
    run: &HostRun,
    instructions: u32,
    tx_memory_limit: u32,
) -> Result<Verdict, HostCovError> {
    let failed_code =
        if run.cpu_insns > u64::from(instructions) || run.mem_bytes > u64::from(tx_memory_limit) {
            CoreResult::ResourceLimitExceeded
        } else {
            CoreResult::Trapped
        };
    Ok(match (&c.core, run.success) {
        (CoreResult::Success(h), true) => {
            let ret = run.return_value.as_ref().expect("set on success");
            let mut diffs = Vec::new();
            if Some(ret) != c.meta_return.as_ref() {
                diffs.push("return value");
            }
            if run.writes != c.meta_writes {
                diffs.push("ledger writes");
            }
            if run.events != c.meta_events {
                diffs.push("contract events");
            }
            if success_hash(ret, &run.events)? != *h {
                diffs.push("success hash");
            }
            if diffs.is_empty() {
                Verdict::Verified
            } else {
                Verdict::Differs(diffs.join(", "))
            }
        }
        (CoreResult::Success(_), false) => {
            Verdict::Differs("core succeeded, the host failed".into())
        }
        (CoreResult::Trapped | CoreResult::ResourceLimitExceeded, false) => {
            if failed_code == c.core {
                Verdict::Verified
            } else {
                Verdict::Differs(format!(
                    "core recorded {:?}, the host's failure maps to {failed_code:?}",
                    c.core
                ))
            }
        }
        (CoreResult::Trapped, true) => Verdict::Differs("core trapped, the host succeeded".into()),
        (CoreResult::ResourceLimitExceeded, true) => Verdict::Unverified(
            "core exceeded a resource limit, which it also checks before calling the host; the host succeeded"
                .into(),
        ),
        // core charges refundable fees after a successful call
        (CoreResult::InsufficientRefundableFee, true) => Verdict::Verified,
        (CoreResult::InsufficientRefundableFee, false) => {
            Verdict::Differs("core ran out of refundable fee after a successful call, the host failed".into())
        }
        (CoreResult::EntryArchived | CoreResult::Malformed | CoreResult::NotApplied(_), _) => {
            unreachable!("not run")
        }
    })
}

/// Writes the host's output implies. Core stamps every written entry with
/// the ledger it closes in; a TTL change only becomes a write when it
/// extends the TTL.
fn host_writes(changes: &[HostChange], seq: u32) -> Result<BTreeMap<String, Write>, HostCovError> {
    let mut out = BTreeMap::new();
    for c in changes {
        let key = LedgerKey::from_xdr(&c.encoded_key, Limits::none())?;
        if !c.read_only {
            let w = match &c.encoded_new_value {
                Some(v) => {
                    let mut e = LedgerEntry::from_xdr(v, Limits::none())?;
                    e.last_modified_ledger_seq = seq;
                    Write::Upsert(Box::new(e))
                }
                None => Write::Delete,
            };
            out.insert(hex::encode(key_bytes(&key)?), w);
        }
        if let Some(t) = &c.ttl_change
            && t.new_live_until_ledger > t.old_live_until_ledger
        {
            let tk = ttl_key(&key)?;
            let entry = LedgerEntry {
                last_modified_ledger_seq: seq,
                data: LedgerEntryData::Ttl(TtlEntry {
                    key_hash: stellar_xdr::Hash(t.key_hash.as_slice().try_into().map_err(
                        |_| HostCovError::Inconsistent {
                            ledger: seq,
                            what: "ttl key hash is not 32 bytes".into(),
                        },
                    )?),
                    live_until_ledger_seq: t.new_live_until_ledger,
                }),
                ext: stellar_xdr::LedgerEntryExt::V0,
            };
            out.insert(hex::encode(key_bytes(&tk)?), Write::Upsert(Box::new(entry)));
        }
    }
    Ok(out)
}

/// Writes an operation's meta states. `State` entries are the values before
/// a write, not writes.
fn meta_writes(changes: &LedgerEntryChanges) -> Result<BTreeMap<String, Write>, HostCovError> {
    let mut out = BTreeMap::new();
    for c in changes.iter() {
        match c {
            LedgerEntryChange::Created(e)
            | LedgerEntryChange::Updated(e)
            | LedgerEntryChange::Restored(e) => {
                out.insert(
                    hex::encode(key_bytes(&e.to_key())?),
                    Write::Upsert(Box::new(e.clone())),
                );
            }
            LedgerEntryChange::Removed(k) => {
                out.insert(hex::encode(key_bytes(k)?), Write::Delete);
            }
            LedgerEntryChange::State(_) => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sub_sha256_appends_a_big_endian_uint64() {
        let seed = [7u8; 32];
        let mut h = Sha256::new();
        h.update([7u8; 32]);
        h.update([0, 0, 0, 0, 0, 0, 1, 2]);
        let want: [u8; 32] = h.finalize().into();
        assert_eq!(sub_sha256(&seed, 258), want);
    }

    fn account(byte: u8, balance: i64) -> LedgerEntry {
        LedgerEntry {
            last_modified_ledger_seq: 1,
            data: LedgerEntryData::Account(stellar_xdr::AccountEntry {
                account_id: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([byte; 32]))),
                balance,
                seq_num: stellar_xdr::SequenceNumber(0),
                num_sub_entries: 0,
                inflation_dest: None,
                flags: 0,
                home_domain: Default::default(),
                thresholds: stellar_xdr::Thresholds([1, 0, 0, 0]),
                signers: Default::default(),
                ext: stellar_xdr::AccountEntryExt::V0,
            }),
            ext: stellar_xdr::LedgerEntryExt::V0,
        }
    }

    #[test]
    fn state_entries_only_fill_unknown_keys() {
        let mut s = LedgerState::default();
        let before = account(1, 10);
        let after = account(1, 20);
        s.apply(&LedgerEntryChanges(
            vec![
                LedgerEntryChange::State(before.clone()),
                LedgerEntryChange::Updated(after.clone()),
            ]
            .try_into()
            .unwrap(),
        ))
        .unwrap();
        // a later State with the old value must not roll the entry back
        s.apply(&LedgerEntryChanges(
            vec![LedgerEntryChange::State(before.clone())]
                .try_into()
                .unwrap(),
        ))
        .unwrap();
        assert_eq!(s.get(&after.to_key()).unwrap(), Some(&after));
        s.apply(&LedgerEntryChanges(
            vec![LedgerEntryChange::Removed(after.to_key())]
                .try_into()
                .unwrap(),
        ))
        .unwrap();
        assert!(s.is_empty());
    }

    #[test]
    fn meta_writes_skip_prior_state() {
        let a = account(2, 5);
        let b = account(3, 6);
        let w = meta_writes(&LedgerEntryChanges(
            vec![
                LedgerEntryChange::State(a.clone()),
                LedgerEntryChange::Updated(a.clone()),
                LedgerEntryChange::Removed(b.to_key()),
            ]
            .try_into()
            .unwrap(),
        ))
        .unwrap();
        assert_eq!(w.len(), 2);
        assert_eq!(
            w.get(&hex::encode(key_bytes(&a.to_key()).unwrap())),
            Some(&Write::Upsert(Box::new(a)))
        );
        assert_eq!(
            w.get(&hex::encode(key_bytes(&b.to_key()).unwrap())),
            Some(&Write::Delete)
        );
    }
}
