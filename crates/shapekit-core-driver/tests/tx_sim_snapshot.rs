//! Simulation against a real testnet snapshot: the CONFIG_SETTING entries and
//! the native asset contract instance, fetched once and stored with
//! provenance in `fixtures/testnet-sac-snapshot.provenance.json`.

use serde_json::Value;
use sha2::{Digest, Sha256};
use shapekit_core_driver::keys::{account_id, network_id, root_key};
use shapekit_core_driver::sim::{
    AuthMode, FetchError, FetchedEntry, LedgerContext, MemorySnapshot, SimError, Simulator,
};
use stellar_xdr::{
    Asset, ContractDataDurability, ContractId, ContractIdPreimage, Hash, HashIdPreimage,
    HashIdPreimageContractId, HostFunction, InvokeContractArgs, LedgerEntry, LedgerEntryData,
    LedgerEntryExt, LedgerHeaderHistoryEntry, LedgerKey, LedgerKeyContractData, Limits, ReadXdr,
    ScAddress, ScString, ScSymbol, ScVal, WriteXdr,
};

const TESTNET: &str = "Test SDF Network ; September 2015";

struct Snapshot {
    entries: Vec<(LedgerKey, FetchedEntry)>,
    ctx: LedgerContext,
}

fn load() -> Snapshot {
    let v: Value =
        serde_json::from_str(include_str!("fixtures/testnet-sac-snapshot.json")).expect("json");
    assert_eq!(v["network_passphrase"], TESTNET);
    let header = LedgerHeaderHistoryEntry::from_xdr_base64(
        v["ledger_header_xdr"].as_str().expect("header"),
        Limits::none(),
    )
    .expect("decode header")
    .header;
    assert_eq!(
        u64::from(header.ledger_seq),
        v["latest_ledger"].as_u64().expect("seq")
    );
    let entries = v["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|e| {
            let key =
                LedgerKey::from_xdr_base64(e["key_xdr"].as_str().expect("key"), Limits::none())
                    .expect("decode key");
            let data = LedgerEntryData::from_xdr_base64(
                e["data_xdr"].as_str().expect("data"),
                Limits::none(),
            )
            .expect("decode data");
            let entry = LedgerEntry {
                last_modified_ledger_seq: e["last_modified_ledger_seq"].as_u64().expect("lm")
                    as u32,
                data,
                ext: LedgerEntryExt::V0,
            };
            let live_until_ledger_seq = e["live_until_ledger_seq"].as_u64().map(|x| x as u32);
            (
                key,
                FetchedEntry {
                    entry,
                    live_until_ledger_seq,
                },
            )
        })
        .collect();
    Snapshot {
        entries,
        ctx: LedgerContext {
            network_passphrase: TESTNET.to_owned(),
            protocol_version: header.ledger_version,
            sequence_number: header.ledger_seq,
            timestamp: header.scp_value.close_time.0,
            base_reserve: header.base_reserve,
        },
    }
}

fn memory(s: &Snapshot) -> MemorySnapshot {
    let mut m = MemorySnapshot::new();
    for (k, e) in &s.entries {
        m.insert(k.clone(), e.clone());
    }
    m
}

// computed from the asset, not read from the fixture
fn native_contract() -> ScAddress {
    let preimage = HashIdPreimage::ContractId(HashIdPreimageContractId {
        network_id: Hash(network_id(TESTNET)),
        contract_id_preimage: ContractIdPreimage::Asset(Asset::Native),
    });
    let bytes = preimage.to_xdr(Limits::none()).expect("xdr");
    let id: [u8; 32] = Sha256::digest(&bytes).into();
    ScAddress::Contract(ContractId(Hash(id)))
}

fn instance_key() -> LedgerKey {
    LedgerKey::ContractData(LedgerKeyContractData {
        contract: native_contract(),
        key: ScVal::LedgerKeyContractInstance,
        durability: ContractDataDurability::Persistent,
    })
}

fn call(function: &str) -> HostFunction {
    HostFunction::InvokeContract(InvokeContractArgs {
        contract_address: native_contract(),
        function_name: ScSymbol(function.try_into().expect("symbol")),
        args: Default::default(),
    })
}

fn record() -> AuthMode {
    AuthMode::Record {
        allow_non_root: false,
        address_v2: false,
    }
}

#[test]
fn native_contract_id_matches_the_fixture() {
    let s = load();
    assert_eq!(
        stellar_strkey::Contract(match native_contract() {
            ScAddress::Contract(ContractId(Hash(h))) => h,
            _ => unreachable!(),
        })
        .to_string()
        .as_str(),
        "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"
    );
    assert!(s.entries.iter().any(|(k, _)| *k == instance_key()));
}

#[test]
fn network_config_loads_from_config_settings() {
    let s = load();
    let sim = Simulator::new(memory(&s), &s.ctx).expect("simulator");
    let li = sim.ledger_info();
    assert_eq!(li.network_id, network_id(TESTNET));
    assert_eq!(li.protocol_version, 30);
    assert!(li.min_persistent_entry_ttl > 0 && li.max_entry_ttl > li.min_persistent_entry_ttl);
    assert!(sim.network_config().tx_max_instructions > 0);
}

#[test]
fn read_only_calls_return_the_asset_metadata() {
    let s = load();
    let sim = Simulator::new(memory(&s), &s.ctx).expect("simulator");
    let source = account_id(&root_key(TESTNET).verifying_key());

    let decimals = sim
        .simulate_invoke(call("decimals"), record(), &source)
        .expect("decimals");
    assert_eq!(decimals.return_value, ScVal::U32(7));
    assert!(decimals.auth.is_empty());
    let fp = &decimals.transaction_data.resources.footprint;
    assert_eq!(fp.read_only.to_vec(), vec![instance_key()]);
    assert!(fp.read_write.is_empty());
    assert!(decimals.transaction_data.resource_fee > 0);

    let symbol = sim
        .simulate_invoke(call("symbol"), record(), &source)
        .expect("symbol");
    assert_eq!(
        symbol.return_value,
        ScVal::String(ScString("native".try_into().expect("str")))
    );
    let name = sim
        .simulate_invoke(call("name"), record(), &source)
        .expect("name");
    assert_eq!(
        name.return_value,
        ScVal::String(ScString("native".try_into().expect("str")))
    );
}

#[test]
fn simulation_is_repeatable() {
    let s = load();
    let source = account_id(&root_key(TESTNET).verifying_key());
    let a = Simulator::new(memory(&s), &s.ctx)
        .expect("sim")
        .simulate_invoke(call("decimals"), record(), &source)
        .expect("a");
    let b = Simulator::new(memory(&s), &s.ctx)
        .expect("sim")
        .simulate_invoke(call("decimals"), record(), &source)
        .expect("b");
    assert_eq!(a.transaction_data, b.transaction_data);
    assert_eq!(a.simulated_instructions, b.simulated_instructions);
}

#[test]
fn extend_ttl_and_restore() {
    let s = load();
    let live_until = s
        .entries
        .iter()
        .find(|(k, _)| *k == instance_key())
        .and_then(|(_, e)| e.live_until_ledger_seq)
        .expect("instance ttl");

    let sim = Simulator::new(memory(&s), &s.ctx).expect("simulator");
    let max_ttl = sim.ledger_info().max_entry_ttl - 1;
    let ext = sim
        .simulate_extend_ttl(&[instance_key()], max_ttl)
        .expect("extend");
    assert!(ext.restore.is_none());
    assert_eq!(
        ext.transaction_data.resources.footprint.read_only.to_vec(),
        vec![instance_key()]
    );
    assert!(ext.transaction_data.resource_fee > 0);
    // restoring a live entry is a no-op, so the footprint stays empty
    let live = sim
        .simulate_restore(&[instance_key()])
        .expect("restore live");
    assert!(live.resources.footprint.read_write.is_empty());

    // one ledger after the ttl the instance is archived
    let mut later = s.ctx.clone();
    later.sequence_number = live_until + 1;
    let sim = Simulator::new(memory(&s), &later).expect("simulator");
    let restore = sim.simulate_restore(&[instance_key()]).expect("restore");
    assert_eq!(
        restore.resources.footprint.read_write.to_vec(),
        vec![instance_key()]
    );
    let ext = sim
        .simulate_extend_ttl(&[instance_key()], 1000)
        .expect("extend archived");
    let pre = ext.restore.expect("restore preamble for an archived key");
    assert_eq!(
        pre.resources.footprint.read_write.to_vec(),
        vec![instance_key()]
    );
}

// stellar-rpc v30.0.1 sorts and dedups the extend and restore keys before
// simulating (lib/preflight/src/shared.rs `unique`), so a repeated key is
// counted once in the footprint and the fee
#[test]
fn repeated_keys_count_once() {
    let s = load();
    let live_until = s
        .entries
        .iter()
        .find(|(k, _)| *k == instance_key())
        .and_then(|(_, e)| e.live_until_ledger_seq)
        .expect("instance ttl");
    let twice = [instance_key(), instance_key()];

    let sim = Simulator::new(memory(&s), &s.ctx).expect("simulator");
    let max_ttl = sim.ledger_info().max_entry_ttl - 1;
    let once = sim
        .simulate_extend_ttl(&[instance_key()], max_ttl)
        .expect("extend once");
    let both = sim
        .simulate_extend_ttl(&twice, max_ttl)
        .expect("extend twice");
    assert_eq!(both.transaction_data, once.transaction_data);

    let mut later = s.ctx.clone();
    later.sequence_number = live_until + 1;
    let sim = Simulator::new(memory(&s), &later).expect("simulator");
    let once = sim
        .simulate_restore(&[instance_key()])
        .expect("restore once");
    let both = sim.simulate_restore(&twice).expect("restore twice");
    assert_eq!(both, once);
    let ext = sim
        .simulate_extend_ttl(&twice, 1000)
        .expect("extend archived");
    assert_eq!(ext.restore.expect("restore preamble"), once);
}

#[test]
fn failed_invocation_is_an_error_with_diagnostics() {
    let s = load();
    let sim = Simulator::new(memory(&s), &s.ctx).expect("simulator");
    let source = account_id(&root_key(TESTNET).verifying_key());
    let missing = HostFunction::InvokeContract(InvokeContractArgs {
        contract_address: ScAddress::Contract(ContractId(Hash([9; 32]))),
        function_name: ScSymbol("decimals".try_into().expect("symbol")),
        args: Default::default(),
    });
    match sim.simulate_invoke(missing, record(), &source) {
        Err(SimError::InvocationFailed {
            diagnostic_events, ..
        }) => assert!(!diagnostic_events.is_empty()),
        other => panic!("expected invocation failure, got {other:?}"),
    }
}

#[test]
fn fetch_errors_surface() {
    let s = load();
    let failing = |_: &[LedgerKey]| -> Result<Vec<Option<FetchedEntry>>, FetchError> {
        Err("core query server down".into())
    };
    match Simulator::new(failing, &s.ctx) {
        Err(SimError::Fetch(m)) => assert!(m.contains("core query server down")),
        Err(other) => panic!("unexpected error {other}"),
        Ok(_) => panic!("simulator built without a snapshot"),
    }

    // an entry fetched mid-run that fails must not turn into a host error
    let mut m = memory(&s);
    m.insert(
        instance_key(),
        FetchedEntry {
            live_until_ledger_seq: None,
            ..s.entries
                .iter()
                .find(|(k, _)| *k == instance_key())
                .expect("instance")
                .1
                .clone()
        },
    );
    let sim = Simulator::new(m, &s.ctx).expect("simulator");
    let source = account_id(&root_key(TESTNET).verifying_key());
    match sim.simulate_invoke(call("decimals"), record(), &source) {
        Err(SimError::MissingTtl(_)) => {}
        other => panic!("expected missing ttl, got {other:?}"),
    }
}

#[test]
fn a_recording_run_keeps_the_footprint_of_a_failed_invocation() {
    let s = load();
    let sim = Simulator::new(memory(&s), &s.ctx).expect("simulator");
    let source = account_id(&root_key(TESTNET).verifying_key());
    let gone = ContractId(Hash([9; 32]));
    let missing = HostFunction::InvokeContract(InvokeContractArgs {
        contract_address: ScAddress::Contract(gone.clone()),
        function_name: ScSymbol("decimals".try_into().expect("symbol")),
        args: Default::default(),
    });
    let run = sim
        .record_invoke(missing, record(), &source)
        .expect("a failed invocation is a result, not an error");
    assert_eq!(run.result, Err("Error(Storage, MissingValue)".to_owned()));
    // the instance the call looked for is in the footprint, so a transaction carrying it
    // fails reading the missing instance and not outside its footprint
    let resources = run.resources.expect("the run touched the instance");
    let wanted = LedgerKey::ContractData(LedgerKeyContractData {
        contract: ScAddress::Contract(gone),
        key: ScVal::LedgerKeyContractInstance,
        durability: ContractDataDurability::Persistent,
    });
    assert!(resources.footprint.read_only.contains(&wanted));
    let small = sim.failed_invoke_fee(&resources, 0, 300);
    let large = sim.failed_invoke_fee(&resources, 0, 3000);
    assert!(small > 0 && large > small, "{small} {large}");
}

#[test]
fn a_recording_run_of_a_working_call_matches_simulation() {
    let s = load();
    let sim = Simulator::new(memory(&s), &s.ctx).expect("simulator");
    let source = account_id(&root_key(TESTNET).verifying_key());
    let run = sim
        .record_invoke(call("decimals"), record(), &source)
        .expect("recording run");
    let simulated = sim
        .simulate_invoke(call("decimals"), record(), &source)
        .expect("simulation");
    assert_eq!(run.result, Ok(simulated.return_value));
    assert_eq!(
        run.resources.expect("footprint").footprint,
        simulated.transaction_data.resources.footprint
    );
}
