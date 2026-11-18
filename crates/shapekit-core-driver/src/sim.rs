//! Soroban simulation over a ledger snapshot, wrapping `soroban-simulation`.
//!
//! The setup mirrors stellar-rpc v30.0.1 (`cmd/stellar-rpc/lib/preflight`):
//! the network config is loaded from the snapshot's CONFIG_SETTING entries,
//! the ledger info takes the protocol, sequence and close time of the latest
//! closed ledger, and the TTL limits come from the network config. As in
//! v30.0.1, only the host's own protocol is simulated, a failed ledger read
//! fails the simulation instead of reading as a missing entry, and the keys
//! of an extend or restore are sorted and deduplicated first. Two things
//! differ on purpose: the PRNG seed is fixed instead of random, so recorded
//! footprints and fees repeat, and the base reserve is the ledger's real one
//! instead of rpc's hardcoded 5000000.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use soroban_env_host::e2e_invoke::RecordingInvocationAuthMode;
use soroban_env_host::storage::{EntryWithLiveUntil, SnapshotSource};
use soroban_env_host::xdr::{ScErrorCode, ScErrorType};
use soroban_env_host::{HostError, LedgerInfo};
use soroban_simulation::simulation::{
    SimulationAdjustmentConfig, SimulationAdjustmentFactor, simulate_extend_ttl_op,
    simulate_invoke_host_function_op, simulate_restore_op,
};
use soroban_simulation::{AutoRestoringSnapshotSource, NetworkConfig};
use stellar_xdr::{
    AccountId, ContractEvent, DiagnosticEvent, HostFunction, LedgerEntry, LedgerKey, ScVal,
    SorobanAuthorizationEntry, SorobanTransactionData,
};

use shapekit_process::error_text::chain;

use crate::keys::network_id;

/// PRNG seed handed to the host in every simulation. The value only steers
/// recording-mode runs; the applied transaction uses the seed core derives
/// from the ledger. Exactly 32 ascii bytes.
pub const SIMULATION_PRNG_SEED: [u8; 32] = *b"shapekit simulation prng seed v1";

// resource adjustment, stated once. the values equal
// `SimulationAdjustmentConfig::default_adjustment()` in soroban-simulation
// 30.0.0, which stellar-rpc v30.0.1 uses with no extra instruction leeway, so our
// fees match what an sdk user gets from rpc. each factor adjusts a value v to
// max(v * multiplicative, v + additive).
pub const INSTRUCTIONS_MULTIPLIER: f64 = 1.04;
pub const INSTRUCTIONS_ADDEND: u32 = 50_000;
pub const TX_SIZE_MULTIPLIER: f64 = 1.1;
pub const TX_SIZE_ADDEND: u32 = 500;
pub const REFUNDABLE_FEE_MULTIPLIER: f64 = 1.15;

/// The adjustment config built from the constants above. Read and write
/// bytes are not adjusted: they are exact once the footprint is known.
pub fn adjustment_config() -> SimulationAdjustmentConfig {
    SimulationAdjustmentConfig {
        instructions: SimulationAdjustmentFactor::new(INSTRUCTIONS_MULTIPLIER, INSTRUCTIONS_ADDEND),
        read_bytes: SimulationAdjustmentFactor::no_adjustment(),
        write_bytes: SimulationAdjustmentFactor::no_adjustment(),
        tx_size: SimulationAdjustmentFactor::new(TX_SIZE_MULTIPLIER, TX_SIZE_ADDEND),
        refundable_fee: SimulationAdjustmentFactor::new(REFUNDABLE_FEE_MULTIPLIER, 0),
    }
}

/// A ledger entry as a snapshot source returns it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchedEntry {
    pub entry: LedgerEntry,
    /// Live-until ledger from the entry's TTL. Required for contract data
    /// and contract code, absent for every other entry type.
    pub live_until_ledger_seq: Option<u32>,
}

pub type FetchError = Box<dyn std::error::Error + Send + Sync>;

/// Supplies ledger entries for simulation. The result is position aligned
/// with `keys`; `None` means the entry does not exist.
pub trait SnapshotFetcher {
    fn fetch(&self, keys: &[LedgerKey]) -> Result<Vec<Option<FetchedEntry>>, FetchError>;
}

impl<F> SnapshotFetcher for F
where
    F: Fn(&[LedgerKey]) -> Result<Vec<Option<FetchedEntry>>, FetchError>,
{
    fn fetch(&self, keys: &[LedgerKey]) -> Result<Vec<Option<FetchedEntry>>, FetchError> {
        self(keys)
    }
}

/// A fixed set of entries, for snapshots captured to disk.
#[derive(Clone, Debug, Default)]
pub struct MemorySnapshot {
    entries: BTreeMap<LedgerKey, FetchedEntry>,
}

impl MemorySnapshot {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: LedgerKey, entry: FetchedEntry) {
        self.entries.insert(key, entry);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl SnapshotFetcher for MemorySnapshot {
    fn fetch(&self, keys: &[LedgerKey]) -> Result<Vec<Option<FetchedEntry>>, FetchError> {
        Ok(keys.iter().map(|k| self.entries.get(k).cloned()).collect())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SimError {
    #[error("fetching ledger entries: {0}")]
    Fetch(String),
    #[error("ledger entry {0} has a durability but the fetcher returned no ttl")]
    MissingTtl(String),
    #[error("fetcher returned {got} results for {want} keys")]
    FetchLength { want: usize, got: usize },
    #[error("simulation setup: {0}")]
    Setup(String),
    #[error("host function failed: {error}")]
    InvocationFailed {
        error: String,
        diagnostic_events: Vec<DiagnosticEvent>,
    },
    #[error("simulation returned no transaction data")]
    NoTransactionData,
    #[error(
        "soroban-env-host {host} only runs ledger protocol {host}, the ledger runs {ledger} (Host::check_ledger_protocol_supported); older protocols need hand-set resources or a matching soroban-simulation"
    )]
    UnsupportedProtocol { ledger: u32, host: u32 },
    #[error(
        "no simulator for ledger protocol {ledger}; simulation runs at protocols {supported:?}, each with the soroban-env-host release core applies it with"
    )]
    NoSimulator { ledger: u32, supported: Vec<u32> },
    #[error("{what} is not available at protocol {protocol}")]
    NotAtProtocol { protocol: u32, what: String },
    #[error("converting {what} between stellar-xdr releases: {error}")]
    Convert { what: &'static str, error: String },
}

/// Snapshot source over a fetcher. Results are cached for the lifetime of the
/// simulator, which matches the single-ledger view simulation assumes. The
/// host only sees a generic storage error when a fetch fails, so the real
/// error is kept here and surfaced after the run.
pub(crate) struct FetchingSnapshot {
    fetcher: Box<dyn SnapshotFetcher>,
    cache: RefCell<BTreeMap<LedgerKey, Option<FetchedEntry>>>,
    failure: RefCell<Option<SimError>>,
}

impl FetchingSnapshot {
    pub(crate) fn new(fetcher: Box<dyn SnapshotFetcher>) -> Self {
        Self {
            fetcher,
            cache: RefCell::new(BTreeMap::new()),
            failure: RefCell::new(None),
        }
    }

    /// The entry for `key`, fetched on first use. On failure the error is
    /// kept for [`Self::take_failure`] and `None` is returned.
    pub(crate) fn lookup(&self, key: &LedgerKey) -> Option<Option<FetchedEntry>> {
        if let Err(e) = self.prefetch(std::slice::from_ref(key)) {
            self.record_failure(e);
            return None;
        }
        self.cache.borrow().get(key).cloned()
    }

    /// Keeps the first failure of a run; later ones are consequences of it.
    pub(crate) fn record_failure(&self, e: SimError) {
        let mut slot = self.failure.borrow_mut();
        if slot.is_none() {
            *slot = Some(e);
        }
    }

    pub(crate) fn prefetch(&self, keys: &[LedgerKey]) -> Result<(), SimError> {
        let missing: Vec<LedgerKey> = {
            let cache = self.cache.borrow();
            keys.iter()
                .filter(|k| !cache.contains_key(*k))
                .cloned()
                .collect()
        };
        if missing.is_empty() {
            return Ok(());
        }
        let got = self
            .fetcher
            .fetch(&missing)
            .map_err(|e| SimError::Fetch(e.to_string()))?;
        if got.len() != missing.len() {
            return Err(SimError::FetchLength {
                want: missing.len(),
                got: got.len(),
            });
        }
        let mut cache = self.cache.borrow_mut();
        for (k, v) in missing.into_iter().zip(got) {
            let durable = matches!(k, LedgerKey::ContractData(_) | LedgerKey::ContractCode(_));
            if durable
                && v.as_ref()
                    .is_some_and(|e| e.live_until_ledger_seq.is_none())
            {
                return Err(SimError::MissingTtl(format!("{k:?}")));
            }
            cache.insert(k, v);
        }
        Ok(())
    }

    pub(crate) fn take_failure(&self) -> Option<SimError> {
        self.failure.borrow_mut().take()
    }
}

impl SnapshotSource for FetchingSnapshot {
    fn get(&self, key: &Rc<LedgerKey>) -> Result<Option<EntryWithLiveUntil>, HostError> {
        let Some(found) = self.lookup(key.as_ref()) else {
            return Err((ScErrorType::Storage, ScErrorCode::InternalError).into());
        };
        Ok(found.map(|f| (Rc::new(f.entry), f.live_until_ledger_seq)))
    }
}

/// The ledger header values simulation needs, from the latest closed ledger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LedgerContext {
    pub network_passphrase: String,
    pub protocol_version: u32,
    pub sequence_number: u32,
    /// Close time in unix seconds.
    pub timestamp: u64,
    pub base_reserve: u32,
}

/// How the host treats authorization during an invoke simulation.
#[derive(Clone, Debug)]
pub enum AuthMode {
    /// Run with these signed entries and enforce them.
    Enforce(Vec<SorobanAuthorizationEntry>),
    /// Record the auth the invocation needs. `allow_non_root` permits
    /// recording `require_auth` calls that are not at the root of the tree;
    /// `address_v2` records CAP-71 address-v2 credentials.
    Record {
        allow_non_root: bool,
        address_v2: bool,
    },
}

/// Result of a successful invoke simulation.
#[derive(Clone, Debug)]
pub struct InvokeSimulation {
    pub return_value: ScVal,
    pub auth: Vec<SorobanAuthorizationEntry>,
    /// Footprint, resources and resource fee, ready for assembly.
    pub transaction_data: SorobanTransactionData,
    pub contract_events: Vec<ContractEvent>,
    pub diagnostic_events: Vec<DiagnosticEvent>,
    pub simulated_instructions: u32,
    pub simulated_memory: u32,
}

/// Result of an extend-TTL simulation. `restore` is set when some keys were
/// archived and must be restored first.
#[derive(Clone, Debug)]
pub struct ExtendTtlSimulation {
    pub transaction_data: SorobanTransactionData,
    pub restore: Option<SorobanTransactionData>,
}

// the keys sorted, each once, as stellar-rpc v30.0.1 hands them to an extend
// or restore (`unique` in lib/preflight/src/shared.rs). soroban-simulation
// sorts the footprint but keeps repeats, which would list a key twice and
// charge its rent twice.
fn unique(keys: &[LedgerKey]) -> Vec<LedgerKey> {
    let mut keys = keys.to_vec();
    keys.sort();
    keys.dedup();
    keys
}

pub struct Simulator {
    snapshot: Rc<FetchingSnapshot>,
    network_config: NetworkConfig,
    ledger_info: LedgerInfo,
}

impl Simulator {
    /// Loads the network config through the fetcher and builds the ledger
    /// info for `ctx`.
    pub fn new(
        fetcher: impl SnapshotFetcher + 'static,
        ctx: &LedgerContext,
    ) -> Result<Self, SimError> {
        // a release host refuses every other ledger protocol, but only once
        // a run starts and with a bare Context/InternalError
        let host = soroban_env_host::meta::INTERFACE_VERSION.protocol;
        if ctx.protocol_version != host {
            return Err(SimError::UnsupportedProtocol {
                ledger: ctx.protocol_version,
                host,
            });
        }
        let snapshot = Rc::new(FetchingSnapshot::new(Box::new(fetcher)));
        let loaded = NetworkConfig::load_from_snapshot(snapshot.as_ref());
        if let Some(e) = snapshot.take_failure() {
            return Err(e);
        }
        let network_config = loaded.map_err(|e| SimError::Setup(chain(e.as_ref())))?;
        let mut ledger_info = LedgerInfo {
            protocol_version: ctx.protocol_version,
            sequence_number: ctx.sequence_number,
            timestamp: ctx.timestamp,
            network_id: network_id(&ctx.network_passphrase),
            base_reserve: ctx.base_reserve,
            ..Default::default()
        };
        network_config.fill_config_fields_in_ledger_info(&mut ledger_info);
        Ok(Self {
            snapshot,
            network_config,
            ledger_info,
        })
    }

    pub fn network_config(&self) -> &NetworkConfig {
        &self.network_config
    }

    pub fn ledger_info(&self) -> &LedgerInfo {
        &self.ledger_info
    }

    /// Fetches keys in one batch ahead of a run, so a node-backed fetcher
    /// makes one request instead of one per key.
    pub fn prefetch(&self, keys: &[LedgerKey]) -> Result<(), SimError> {
        self.snapshot.prefetch(keys)
    }

    pub fn simulate_invoke(
        &self,
        host_function: HostFunction,
        auth_mode: AuthMode,
        source: &AccountId,
    ) -> Result<InvokeSimulation, SimError> {
        let mode = match auth_mode {
            AuthMode::Enforce(entries) => RecordingInvocationAuthMode::Enforcing(entries),
            AuthMode::Record {
                allow_non_root,
                address_v2,
            } => RecordingInvocationAuthMode::recording(!allow_non_root, address_v2),
        };
        let source_snapshot: Rc<dyn SnapshotSource> = self.snapshot.clone();
        let run = simulate_invoke_host_function_op(
            source_snapshot,
            &self.network_config,
            &adjustment_config(),
            &self.ledger_info,
            host_function,
            mode,
            source,
            SIMULATION_PRNG_SEED,
            true,
        );
        if let Some(e) = self.snapshot.take_failure() {
            return Err(e);
        }
        let r = run.map_err(|e| SimError::Setup(chain(e.as_ref())))?;
        let return_value = match r.invoke_result {
            Ok(v) => v,
            Err(e) => {
                return Err(SimError::InvocationFailed {
                    error: format!("{e:?}"),
                    diagnostic_events: r.diagnostic_events,
                });
            }
        };
        Ok(InvokeSimulation {
            return_value,
            auth: r.auth,
            transaction_data: r.transaction_data.ok_or(SimError::NoTransactionData)?,
            contract_events: r.contract_events,
            diagnostic_events: r.diagnostic_events,
            simulated_instructions: r.simulated_instructions,
            simulated_memory: r.simulated_memory,
        })
    }

    /// Extend-TTL simulation for `keys`, extending to `extend_to` ledgers past
    /// the current one. Archived keys are restored in a view the way
    /// stellar-rpc does it, and the restore needed first is returned beside.
    pub fn simulate_extend_ttl(
        &self,
        keys: &[LedgerKey],
        extend_to: u32,
    ) -> Result<ExtendTtlSimulation, SimError> {
        let keys = unique(keys);
        let restoring = AutoRestoringSnapshotSource::new(self.snapshot.clone(), &self.ledger_info)
            .map_err(|e| SimError::Setup(chain(e.as_ref())))?;
        let run = simulate_extend_ttl_op(
            &restoring,
            &self.network_config,
            &adjustment_config(),
            &self.ledger_info,
            &keys,
            extend_to,
        );
        if let Some(e) = self.snapshot.take_failure() {
            return Err(e);
        }
        let extend = run.map_err(|e| SimError::Setup(chain(e.as_ref())))?;
        let restore = restoring
            .simulate_restore_keys_op(
                &self.network_config,
                &adjustment_config(),
                &self.ledger_info,
            )
            .map_err(|e| SimError::Setup(chain(e.as_ref())))?;
        if let Some(e) = self.snapshot.take_failure() {
            return Err(e);
        }
        Ok(ExtendTtlSimulation {
            transaction_data: extend.transaction_data,
            restore: restore.map(|r| r.transaction_data),
        })
    }

    /// Restore simulation. Live keys are left out of the result; a missing or
    /// temporary key is an error.
    pub fn simulate_restore(&self, keys: &[LedgerKey]) -> Result<SorobanTransactionData, SimError> {
        let keys = unique(keys);
        let run = simulate_restore_op(
            self.snapshot.as_ref(),
            &self.network_config,
            &adjustment_config(),
            &self.ledger_info,
            &keys,
        );
        if let Some(e) = self.snapshot.take_failure() {
            return Err(e);
        }
        Ok(run
            .map_err(|e| SimError::Setup(chain(e.as_ref())))?
            .transaction_data)
    }
}
