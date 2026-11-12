//! Soroban simulation for chains below protocol 30.
//!
//! A release soroban-env-host runs exactly one ledger protocol
//! (`Host::check_ledger_protocol_supported`), so [`crate::sim::Simulator`]
//! cannot price a transaction for a protocol 28 chain. stellar-core links
//! one host per protocol under renamed dependencies (`src/rust/Cargo.toml`,
//! v30.0.0); this module does the same with soroban-simulation, one release
//! per supported protocol:
//!
//! | protocol | soroban-simulation, soroban-env-host | stellar-xdr | core 30.0.0 applies with |
//! |---|---|---|---|
//! | 22 | 22.1.4 | 22.1.0 | soroban-env-host 22.0.0 |
//! | 24 | 24.0.0 | 24.0.0 | soroban-env-host 24.0.0 |
//! | 28 | 28.0.2 | 28.0.0 | soroban-env-host 28.0.0 |
//!
//! Patch releases of a host keep its protocol's behaviour, which is what lets
//! simulation use the newest patch of the host core applies with. Each host
//! has its own stellar-xdr major, so every value crossing the boundary is
//! converted through its XDR bytes, and an entry or input that the older
//! XDR cannot hold fails the run with [`SimError::Convert`].
//!
//! Setup otherwise follows `sim.rs`: network config from the snapshot's
//! CONFIG_SETTING entries, ledger info from the latest closed ledger, the same
//! fixed PRNG seed and adjustment constants. Hosts before 25 take the state
//! size used for rent fees as an argument; it is the average of the live
//! state size window, which is what core computes
//! (`SorobanNetworkConfig::loadLiveSorobanStateSizeWindow`) and what
//! soroban-simulation 25 and later read on their own.

use std::rc::Rc;

use stellar_xdr::{
    AccountId, ConfigSettingEntry, ConfigSettingId, HostFunction, LedgerEntryData, LedgerKey,
    LedgerKeyConfigSetting, SorobanTransactionData,
};

use crate::sim::{
    AuthMode, ExtendTtlSimulation, FetchingSnapshot, InvokeSimulation, LedgerContext, SimError,
    Simulator, SnapshotFetcher,
};

/// Ledger protocols a simulator exists for: the ones below 30 that this
/// module covers, plus 30 itself through [`Simulator`].
pub const SIMULATED_PROTOCOLS: [u32; 4] = [22, 24, 28, 30];

/// Average of the live state size window, as core computes it. Every value
/// read is converted from the snapshot, so a missing or malformed entry
/// fails here instead of pricing with a zero size.
fn average_state_size(snapshot: &FetchingSnapshot) -> Result<u64, SimError> {
    let key = LedgerKey::ConfigSetting(LedgerKeyConfigSetting {
        config_setting_id: ConfigSettingId::LiveSorobanStateSizeWindow,
    });
    let found = snapshot.lookup(&key);
    if let Some(e) = snapshot.take_failure() {
        return Err(e);
    }
    let window = match found.flatten().map(|f| f.entry.data) {
        Some(LedgerEntryData::ConfigSetting(ConfigSettingEntry::LiveSorobanStateSizeWindow(w))) => {
            w
        }
        other => {
            return Err(SimError::Setup(format!(
                "live state size window config setting: {other:?}"
            )));
        }
    };
    if window.is_empty() {
        return Err(SimError::Setup("live state size window is empty".into()));
    }
    let sum = window.iter().fold(0u64, |a, s| a.saturating_add(*s));
    Ok(sum / window.len() as u64)
}

// one module per host release. the hosts differ in three places, passed in
// as expressions: how the auth mode is expressed, how the network config is
// loaded, and whether archived entries need their own snapshot trait.
macro_rules! legacy_simulator {
    (
        $(#[$doc:meta])*
        mod $name:ident,
        protocol: $protocol:literal,
        host: $host:ident,
        sim: $sim:ident,
        enforce: |$entries:ident| $enforce:expr,
        record: |$non_root:ident, $v2:ident| $record:expr,
        load: |$snap:ident, $avg:ident| $load:expr,
        extra: { $($extra:item)* }
    ) => {
        $(#[$doc])*
        pub mod $name {
            use std::rc::Rc;

            use $host::e2e_invoke::RecordingInvocationAuthMode;
            use $host::storage::{EntryWithLiveUntil, SnapshotSource};
            use $host::xdr::{self as old, ScErrorCode, ScErrorType};
            use $host::{HostError, LedgerInfo};
            use $sim::simulation::{
                SimulationAdjustmentConfig, SimulationAdjustmentFactor, simulate_extend_ttl_op,
                simulate_invoke_host_function_op, simulate_restore_op,
            };
            use $sim::{AutoRestoringSnapshotSource, NetworkConfig};
            use stellar_xdr as new;

            use crate::keys::network_id;
            use crate::sim::{
                AuthMode, ExtendTtlSimulation, FetchingSnapshot, InvokeSimulation, LedgerContext,
                SIMULATION_PRNG_SEED, SimError, INSTRUCTIONS_ADDEND, INSTRUCTIONS_MULTIPLIER,
                REFUNDABLE_FEE_MULTIPLIER, TX_SIZE_ADDEND, TX_SIZE_MULTIPLIER,
            };

            /// Ledger protocol this host runs, and the only one it accepts.
            pub const PROTOCOL: u32 = $protocol;

            fn to_old<T, U>(v: &T, what: &'static str) -> Result<U, SimError>
            where
                T: new::WriteXdr,
                U: old::ReadXdr,
            {
                let bytes = v.to_xdr(new::Limits::none()).map_err(|e| SimError::Convert {
                    what,
                    error: e.to_string(),
                })?;
                U::from_xdr(bytes, old::Limits::none()).map_err(|e| SimError::Convert {
                    what,
                    error: e.to_string(),
                })
            }

            fn to_new<T, U>(v: &T, what: &'static str) -> Result<U, SimError>
            where
                T: old::WriteXdr,
                U: new::ReadXdr,
            {
                let bytes = v.to_xdr(old::Limits::none()).map_err(|e| SimError::Convert {
                    what,
                    error: e.to_string(),
                })?;
                U::from_xdr(bytes, new::Limits::none()).map_err(|e| SimError::Convert {
                    what,
                    error: e.to_string(),
                })
            }

            fn to_new_all<T, U>(v: &[T], what: &'static str) -> Result<Vec<U>, SimError>
            where
                T: old::WriteXdr,
                U: new::ReadXdr,
            {
                v.iter().map(|x| to_new(x, what)).collect()
            }

            fn adjustment_config() -> SimulationAdjustmentConfig {
                SimulationAdjustmentConfig {
                    instructions: SimulationAdjustmentFactor::new(
                        INSTRUCTIONS_MULTIPLIER,
                        INSTRUCTIONS_ADDEND,
                    ),
                    read_bytes: SimulationAdjustmentFactor::no_adjustment(),
                    write_bytes: SimulationAdjustmentFactor::no_adjustment(),
                    tx_size: SimulationAdjustmentFactor::new(TX_SIZE_MULTIPLIER, TX_SIZE_ADDEND),
                    refundable_fee: SimulationAdjustmentFactor::new(REFUNDABLE_FEE_MULTIPLIER, 0),
                }
            }

            /// The shared fetching cache seen through this host's types.
            pub(crate) struct Snap(Rc<FetchingSnapshot>);

            impl Snap {
                fn entry(
                    &self,
                    key: &Rc<old::LedgerKey>,
                ) -> Result<Option<EntryWithLiveUntil>, HostError> {
                    let found = to_new::<_, new::LedgerKey>(key.as_ref(), "ledger key")
                        .map(|k| self.0.lookup(&k))
                        .and_then(|found| match found {
                            None => Ok(None),
                            Some(None) => Ok(Some(None)),
                            Some(Some(f)) => Ok(Some(Some((
                                Rc::new(to_old(&f.entry, "ledger entry")?),
                                f.live_until_ledger_seq,
                            )))),
                        });
                    match found {
                        Ok(Some(v)) => Ok(v),
                        // the fetch failure is already kept by the cache
                        Ok(None) => Err((ScErrorType::Storage, ScErrorCode::InternalError).into()),
                        Err(e) => {
                            self.0.record_failure(e);
                            Err((ScErrorType::Storage, ScErrorCode::InternalError).into())
                        }
                    }
                }
            }

            impl SnapshotSource for Snap {
                fn get(
                    &self,
                    key: &Rc<old::LedgerKey>,
                ) -> Result<Option<EntryWithLiveUntil>, HostError> {
                    self.entry(key)
                }
            }

            $($extra)*

            pub struct Simulator {
                snapshot: Rc<Snap>,
                network_config: NetworkConfig,
                ledger_info: LedgerInfo,
            }

            impl Simulator {
                pub(crate) fn new(
                    fetching: Rc<FetchingSnapshot>,
                    ctx: &LedgerContext,
                ) -> Result<Self, SimError> {
                    if ctx.protocol_version != PROTOCOL {
                        return Err(SimError::UnsupportedProtocol {
                            ledger: ctx.protocol_version,
                            host: PROTOCOL,
                        });
                    }
                    let $avg = super::average_state_size(&fetching)?;
                    let snapshot = Rc::new(Snap(fetching));
                    let $snap = snapshot.as_ref();
                    let loaded = $load;
                    if let Some(e) = snapshot.0.take_failure() {
                        return Err(e);
                    }
                    let network_config = loaded.map_err(|e| SimError::Setup(format!("{e:#}")))?;
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

                pub(crate) fn prefetch(&self, keys: &[new::LedgerKey]) -> Result<(), SimError> {
                    self.snapshot.0.prefetch(keys)
                }

                fn failure(&self) -> Result<(), SimError> {
                    match self.snapshot.0.take_failure() {
                        Some(e) => Err(e),
                        None => Ok(()),
                    }
                }

                pub(crate) fn simulate_invoke(
                    &self,
                    host_function: &new::HostFunction,
                    auth_mode: AuthMode,
                    source: &new::AccountId,
                ) -> Result<InvokeSimulation, SimError> {
                    let mode = match auth_mode {
                        AuthMode::Enforce(entries) => {
                            let $entries: Vec<old::SorobanAuthorizationEntry> = entries
                                .iter()
                                .map(|e| to_old(e, "auth entry"))
                                .collect::<Result<_, _>>()?;
                            $enforce
                        }
                        AuthMode::Record {
                            allow_non_root: $non_root,
                            address_v2: $v2,
                        } => $record?,
                    };
                    let source_snapshot: Rc<dyn SnapshotSource> = self.snapshot.clone();
                    let run = simulate_invoke_host_function_op(
                        source_snapshot,
                        &self.network_config,
                        &adjustment_config(),
                        &self.ledger_info,
                        to_old(host_function, "host function")?,
                        mode,
                        &to_old(source, "source account")?,
                        SIMULATION_PRNG_SEED,
                        true,
                    );
                    self.failure()?;
                    let r = run.map_err(|e| SimError::Setup(format!("{e:#}")))?;
                    let diagnostic_events = to_new_all(&r.diagnostic_events, "diagnostic event")?;
                    let return_value = match r.invoke_result {
                        Ok(v) => to_new(&v, "return value")?,
                        Err(e) => {
                            return Err(SimError::InvocationFailed {
                                error: format!("{e:?}"),
                                diagnostic_events,
                            });
                        }
                    };
                    let data = r.transaction_data.ok_or(SimError::NoTransactionData)?;
                    Ok(InvokeSimulation {
                        return_value,
                        auth: to_new_all(&r.auth, "auth entry")?,
                        transaction_data: to_new(&data, "transaction data")?,
                        contract_events: to_new_all(&r.contract_events, "contract event")?,
                        diagnostic_events,
                        simulated_instructions: r.simulated_instructions,
                        simulated_memory: r.simulated_memory,
                    })
                }

                pub(crate) fn simulate_extend_ttl(
                    &self,
                    keys: &[new::LedgerKey],
                    extend_to: u32,
                ) -> Result<ExtendTtlSimulation, SimError> {
                    let keys: Vec<old::LedgerKey> = keys
                        .iter()
                        .map(|k| to_old(k, "ledger key"))
                        .collect::<Result<_, _>>()?;
                    let restoring =
                        AutoRestoringSnapshotSource::new(self.snapshot.clone(), &self.ledger_info)
                            .map_err(|e| SimError::Setup(format!("{e:#}")))?;
                    let run = simulate_extend_ttl_op(
                        &restoring,
                        &self.network_config,
                        &adjustment_config(),
                        &self.ledger_info,
                        &keys,
                        extend_to,
                    );
                    self.failure()?;
                    let extend = run.map_err(|e| SimError::Setup(format!("{e:#}")))?;
                    let restore = restoring
                        .simulate_restore_keys_op(
                            &self.network_config,
                            &adjustment_config(),
                            &self.ledger_info,
                        )
                        .map_err(|e| SimError::Setup(format!("{e:#}")))?;
                    self.failure()?;
                    Ok(ExtendTtlSimulation {
                        transaction_data: to_new(&extend.transaction_data, "transaction data")?,
                        restore: restore
                            .map(|r| to_new(&r.transaction_data, "transaction data"))
                            .transpose()?,
                    })
                }

                pub(crate) fn simulate_restore(
                    &self,
                    keys: &[new::LedgerKey],
                ) -> Result<new::SorobanTransactionData, SimError> {
                    let keys: Vec<old::LedgerKey> = keys
                        .iter()
                        .map(|k| to_old(k, "ledger key"))
                        .collect::<Result<_, _>>()?;
                    let run = simulate_restore_op(
                        self.snapshot.as_ref(),
                        &self.network_config,
                        &adjustment_config(),
                        &self.ledger_info,
                        &keys,
                    );
                    self.failure()?;
                    let r = run.map_err(|e| SimError::Setup(format!("{e:#}")))?;
                    to_new(&r.transaction_data, "transaction data")
                }
            }
        }
    };
}

legacy_simulator! {
    /// Protocol 22: soroban-simulation 22.1.4. Archived entries still sit in
    /// the live state at this protocol, and restore and auto-restore read
    /// them through `SnapshotSourceWithArchive`.
    mod p22,
    protocol: 22,
    host: soroban_env_host_p22,
    sim: soroban_simulation_p22,
    // without its unstable-next-api feature (which stellar-rpc 22 did not
    // enable) this host's mode is an option: `None` records with non-root
    // auth disabled, and there is no way to allow it
    enforce: |entries| Some(entries),
    record: |allow_non_root, address_v2| if address_v2 {
        Err(SimError::NotAtProtocol {
            protocol: PROTOCOL,
            what: "address-v2 credentials (CAP-71, protocol 27)".into(),
        })
    } else if allow_non_root {
        Err(SimError::NotAtProtocol {
            protocol: PROTOCOL,
            what: "recording non-root auth (soroban-env-host 22 records root auth only)".into(),
        })
    } else {
        Ok::<RecordingInvocationAuthMode, SimError>(None)
    },
    load: |snap, avg| NetworkConfig::load_from_snapshot(snap, avg),
    extra: {
        impl soroban_simulation_p22::SnapshotSourceWithArchive for Snap {
            fn get_including_archived(
                &self,
                key: &Rc<old::LedgerKey>,
            ) -> Result<Option<EntryWithLiveUntil>, HostError> {
                self.entry(key)
            }
        }
    }
}

legacy_simulator! {
    /// Protocol 24: soroban-simulation 24.0.0.
    mod p24,
    protocol: 24,
    host: soroban_env_host_p24,
    sim: soroban_simulation_p24,
    enforce: |entries| RecordingInvocationAuthMode::Enforcing(entries),
    record: |allow_non_root, address_v2| if address_v2 {
        Err(SimError::NotAtProtocol {
            protocol: PROTOCOL,
            what: "address-v2 credentials (CAP-71, protocol 27)".into(),
        })
    } else {
        Ok(RecordingInvocationAuthMode::Recording(!allow_non_root))
    },
    load: |snap, avg| NetworkConfig::load_from_snapshot(snap, avg),
    extra: {}
}

legacy_simulator! {
    /// Protocol 28: soroban-simulation 28.0.2.
    mod p28,
    protocol: 28,
    host: soroban_env_host_p28,
    sim: soroban_simulation_p28,
    enforce: |entries| RecordingInvocationAuthMode::Enforcing(entries),
    record: |allow_non_root, address_v2| Ok::<_, SimError>(
        RecordingInvocationAuthMode::recording(!allow_non_root, address_v2)
    ),
    load: |snap, _avg| NetworkConfig::load_from_snapshot(snap),
    extra: {}
}

enum Inner {
    P22(p22::Simulator),
    P24(p24::Simulator),
    P28(p28::Simulator),
    Current(Simulator),
}

/// A simulator for the ledger's own protocol: [`Simulator`] at 30, one of
/// the older hosts above at 22, 24 and 28. Inputs and outputs are
/// stellar-xdr 30 types whatever the protocol.
pub struct AnySimulator {
    protocol: u32,
    inner: Inner,
}

impl AnySimulator {
    pub fn new(
        fetcher: impl SnapshotFetcher + 'static,
        ctx: &LedgerContext,
    ) -> Result<Self, SimError> {
        let protocol = ctx.protocol_version;
        if !SIMULATED_PROTOCOLS.contains(&protocol) {
            return Err(SimError::NoSimulator {
                ledger: protocol,
                supported: SIMULATED_PROTOCOLS.to_vec(),
            });
        }
        if protocol == soroban_env_host::meta::INTERFACE_VERSION.protocol {
            return Ok(Self {
                protocol,
                inner: Inner::Current(Simulator::new(fetcher, ctx)?),
            });
        }
        let fetching = Rc::new(FetchingSnapshot::new(Box::new(fetcher)));
        let inner = match protocol {
            p22::PROTOCOL => Inner::P22(p22::Simulator::new(fetching, ctx)?),
            p24::PROTOCOL => Inner::P24(p24::Simulator::new(fetching, ctx)?),
            p28::PROTOCOL => Inner::P28(p28::Simulator::new(fetching, ctx)?),
            other => {
                return Err(SimError::NoSimulator {
                    ledger: other,
                    supported: SIMULATED_PROTOCOLS.to_vec(),
                });
            }
        };
        Ok(Self { protocol, inner })
    }

    /// The ledger protocol this simulator runs.
    pub fn protocol(&self) -> u32 {
        self.protocol
    }

    /// See [`Simulator::prefetch`].
    pub fn prefetch(&self, keys: &[LedgerKey]) -> Result<(), SimError> {
        match &self.inner {
            Inner::P22(s) => s.prefetch(keys),
            Inner::P24(s) => s.prefetch(keys),
            Inner::P28(s) => s.prefetch(keys),
            Inner::Current(s) => s.prefetch(keys),
        }
    }

    pub fn simulate_invoke(
        &self,
        host_function: HostFunction,
        auth_mode: AuthMode,
        source: &AccountId,
    ) -> Result<InvokeSimulation, SimError> {
        match &self.inner {
            Inner::P22(s) => s.simulate_invoke(&host_function, auth_mode, source),
            Inner::P24(s) => s.simulate_invoke(&host_function, auth_mode, source),
            Inner::P28(s) => s.simulate_invoke(&host_function, auth_mode, source),
            Inner::Current(s) => s.simulate_invoke(host_function, auth_mode, source),
        }
    }

    /// See [`Simulator::simulate_extend_ttl`].
    pub fn simulate_extend_ttl(
        &self,
        keys: &[LedgerKey],
        extend_to: u32,
    ) -> Result<ExtendTtlSimulation, SimError> {
        match &self.inner {
            Inner::P22(s) => s.simulate_extend_ttl(keys, extend_to),
            Inner::P24(s) => s.simulate_extend_ttl(keys, extend_to),
            Inner::P28(s) => s.simulate_extend_ttl(keys, extend_to),
            Inner::Current(s) => s.simulate_extend_ttl(keys, extend_to),
        }
    }

    /// See [`Simulator::simulate_restore`].
    pub fn simulate_restore(&self, keys: &[LedgerKey]) -> Result<SorobanTransactionData, SimError> {
        match &self.inner {
            Inner::P22(s) => s.simulate_restore(keys),
            Inner::P24(s) => s.simulate_restore(keys),
            Inner::P28(s) => s.simulate_restore(keys),
            Inner::Current(s) => s.simulate_restore(keys),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::MemorySnapshot;

    fn ctx(protocol: u32) -> LedgerContext {
        LedgerContext {
            network_passphrase: "Test SDF Network ; September 2015".into(),
            protocol_version: protocol,
            sequence_number: 10,
            timestamp: 1767225600,
            base_reserve: 100_000_000,
        }
    }

    #[test]
    fn protocols_without_a_host_are_refused() {
        for p in [21, 23, 25, 26, 27, 29, 31] {
            match AnySimulator::new(MemorySnapshot::new(), &ctx(p)) {
                Err(SimError::NoSimulator { ledger, supported }) => {
                    assert_eq!(ledger, p);
                    assert_eq!(supported, SIMULATED_PROTOCOLS.to_vec());
                }
                Err(e) => panic!("{p}: {e}"),
                Ok(_) => panic!("{p}: a simulator"),
            }
        }
    }

    #[test]
    fn older_hosts_read_the_state_size_window_first() {
        // an empty snapshot gets as far as the first entry every older host
        // needs, so the error shows the protocol's own path was taken
        for p in [22, 24, 28] {
            match AnySimulator::new(MemorySnapshot::new(), &ctx(p)) {
                Err(SimError::Setup(m)) => {
                    assert!(m.contains("live state size window"), "{p}: {m}")
                }
                Err(e) => panic!("{p}: {e}"),
                Ok(_) => panic!("{p}: a simulator over nothing"),
            }
        }
    }
}
