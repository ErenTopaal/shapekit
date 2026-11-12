//! Simulation of this repository's own contracts (`contracts/wasm/`) over the
//! testnet CONFIG_SETTING snapshot, and an end-to-end check of auth entry
//! signing: a recorded entry signed by `sign_authorization_entry` must pass
//! soroban-env-host 30.0.0 in enforcing mode, and a tampered one must not.

use serde_json::Value;
use sha2::{Digest, Sha256};
use shapekit_core_driver::keys::{account_address, account_id, derive_key, network_id, root_key};
use shapekit_core_driver::sim::{
    AuthMode, FetchedEntry, LedgerContext, MemorySnapshot, SimError, Simulator,
};
use shapekit_core_driver::tx::sign_authorization_entry;
use stellar_xdr::{
    AccountEntry, AccountEntryExt, ContractCodeEntry, ContractCodeEntryExt, ContractDataDurability,
    ContractDataEntry, ContractExecutable, ContractId, ExtensionPoint, Hash, HostFunction,
    Int128Parts, InvokeContractArgs, LedgerEntry, LedgerEntryData, LedgerEntryExt,
    LedgerHeaderHistoryEntry, LedgerKey, LedgerKeyAccount, LedgerKeyContractCode,
    LedgerKeyContractData, Limits, ReadXdr, ScAddress, ScBytes, ScContractInstance, ScMap,
    ScString, ScSymbol, ScVal, ScVec, SequenceNumber, SorobanCredentials, Thresholds,
};

const TESTNET: &str = "Test SDF Network ; September 2015";

fn wasm(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/../../contracts/wasm/{name}.wasm",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&path).unwrap_or_else(|e| panic!("reading {path}: {e}"))
}

fn base() -> (MemorySnapshot, LedgerContext) {
    let v: Value =
        serde_json::from_str(include_str!("fixtures/testnet-sac-snapshot.json")).expect("json");
    let header = LedgerHeaderHistoryEntry::from_xdr_base64(
        v["ledger_header_xdr"].as_str().expect("header"),
        Limits::none(),
    )
    .expect("header")
    .header;
    let mut m = MemorySnapshot::new();
    for e in v["entries"].as_array().expect("entries") {
        let key = LedgerKey::from_xdr_base64(e["key_xdr"].as_str().expect("key"), Limits::none())
            .expect("key");
        if !matches!(key, LedgerKey::ConfigSetting(_)) {
            continue;
        }
        let data =
            LedgerEntryData::from_xdr_base64(e["data_xdr"].as_str().expect("data"), Limits::none())
                .expect("data");
        m.insert(
            key,
            FetchedEntry {
                entry: LedgerEntry {
                    last_modified_ledger_seq: e["last_modified_ledger_seq"].as_u64().expect("lm")
                        as u32,
                    data,
                    ext: LedgerEntryExt::V0,
                },
                live_until_ledger_seq: None,
            },
        );
    }
    let ctx = LedgerContext {
        network_passphrase: TESTNET.to_owned(),
        protocol_version: header.ledger_version,
        sequence_number: header.ledger_seq,
        timestamp: header.scp_value.close_time.0,
        base_reserve: header.base_reserve,
    };
    (m, ctx)
}

// installs code and an instance for `wasm` at a contract id derived from the
// name, live far beyond the snapshot ledger
fn install(m: &mut MemorySnapshot, ctx: &LedgerContext, name: &str) -> ScAddress {
    let code = wasm(name);
    let hash = Hash(Sha256::digest(&code).into());
    let live = Some(ctx.sequence_number + 100_000);
    m.insert(
        LedgerKey::ContractCode(LedgerKeyContractCode { hash: hash.clone() }),
        FetchedEntry {
            entry: LedgerEntry {
                last_modified_ledger_seq: 1,
                data: LedgerEntryData::ContractCode(ContractCodeEntry {
                    ext: ContractCodeEntryExt::V0,
                    hash: hash.clone(),
                    code: code.try_into().expect("code"),
                }),
                ext: LedgerEntryExt::V0,
            },
            live_until_ledger_seq: live,
        },
    );
    let address = ScAddress::Contract(ContractId(Hash(Sha256::digest(name.as_bytes()).into())));
    m.insert(
        instance_key(&address),
        FetchedEntry {
            entry: LedgerEntry {
                last_modified_ledger_seq: 1,
                data: LedgerEntryData::ContractData(ContractDataEntry {
                    ext: ExtensionPoint::V0,
                    contract: address.clone(),
                    key: ScVal::LedgerKeyContractInstance,
                    durability: ContractDataDurability::Persistent,
                    val: ScVal::ContractInstance(ScContractInstance {
                        executable: ContractExecutable::Wasm(hash),
                        storage: None,
                    }),
                }),
                ext: LedgerEntryExt::V0,
            },
            live_until_ledger_seq: live,
        },
    );
    address
}

fn instance_key(address: &ScAddress) -> LedgerKey {
    LedgerKey::ContractData(LedgerKeyContractData {
        contract: address.clone(),
        key: ScVal::LedgerKeyContractInstance,
        durability: ContractDataDurability::Persistent,
    })
}

fn add_account(m: &mut MemorySnapshot, id: stellar_xdr::AccountId) {
    m.insert(
        LedgerKey::Account(LedgerKeyAccount {
            account_id: id.clone(),
        }),
        FetchedEntry {
            entry: LedgerEntry {
                last_modified_ledger_seq: 1,
                data: LedgerEntryData::Account(AccountEntry {
                    account_id: id,
                    balance: 100_000_000_000,
                    seq_num: SequenceNumber(1 << 32),
                    num_sub_entries: 0,
                    inflation_dest: None,
                    flags: 0,
                    home_domain: Default::default(),
                    thresholds: Thresholds([1, 0, 0, 0]),
                    signers: Default::default(),
                    ext: AccountEntryExt::V0,
                }),
                ext: LedgerEntryExt::V0,
            },
            live_until_ledger_seq: None,
        },
    );
}

fn invoke(contract: &ScAddress, function: &str, args: Vec<ScVal>) -> HostFunction {
    HostFunction::InvokeContract(InvokeContractArgs {
        contract_address: contract.clone(),
        function_name: ScSymbol(function.try_into().expect("symbol")),
        args: args.try_into().expect("args"),
    })
}

#[test]
fn echo_round_trips_a_mixed_key_map() {
    let (mut m, ctx) = base();
    let echo = install(&mut m, &ctx, "echo");
    // keys of three types that collide once converted to native strings,
    // in the order the host requires
    let map = ScMap::sorted_from(vec![
        (
            ScVal::Symbol(ScSymbol("1".try_into().expect("s"))),
            ScVal::I128(Int128Parts {
                hi: i64::MIN,
                lo: 0,
            }),
        ),
        (ScVal::U32(1), ScVal::Bool(true)),
        (
            ScVal::String(ScString("1".try_into().expect("s"))),
            ScVal::Bytes(ScBytes(vec![0xff; 3].try_into().expect("b"))),
        ),
    ])
    .expect("sorted map");
    let value = ScVal::Vec(Some(ScVec(
        vec![ScVal::Map(Some(map)), ScVal::U64(u64::MAX)]
            .try_into()
            .expect("vec"),
    )));

    let source = account_id(&root_key(TESTNET).verifying_key());
    let sim = Simulator::new(m, &ctx).expect("simulator");
    let r = sim
        .simulate_invoke(
            invoke(&echo, "echo", vec![value.clone()]),
            AuthMode::Record {
                allow_non_root: false,
                address_v2: false,
            },
            &source,
        )
        .expect("echo");
    assert_eq!(r.return_value, value);
    assert!(r.auth.is_empty());
    assert_eq!(r.contract_events.len(), 1);
    let stellar_xdr::ContractEventBody::V0(body) = &r.contract_events[0].body;
    assert_eq!(body.data, value);
    let fp = &r.transaction_data.resources.footprint;
    assert!(fp.read_only.iter().any(|k| *k == instance_key(&echo)));
    assert!(
        fp.read_only
            .iter()
            .any(|k| matches!(k, LedgerKey::ContractCode(_)))
    );
    let stored = LedgerKey::ContractData(LedgerKeyContractData {
        contract: echo.clone(),
        key: ScVal::Symbol(ScSymbol("last".try_into().expect("s"))),
        durability: ContractDataDurability::Persistent,
    });
    assert_eq!(fp.read_write.to_vec(), vec![stored]);
}

#[test]
fn signed_auth_entry_passes_the_host() {
    let (mut m, ctx) = base();
    let tree = install(&mut m, &ctx, "auth_tree");
    let signer = derive_key("sim-auth", "signer");
    let source_key = root_key(TESTNET);
    add_account(&mut m, account_id(&signer.verifying_key()));
    add_account(&mut m, account_id(&source_key.verifying_key()));
    let source = account_id(&source_key.verifying_key());
    let call = invoke(
        &tree,
        "guarded",
        vec![
            ScVal::Address(account_address(&signer.verifying_key())),
            ScVal::Bool(true),
        ],
    );
    let nid = network_id(TESTNET);

    for address_v2 in [false, true] {
        let sim = Simulator::new(m.clone(), &ctx).expect("simulator");
        let recorded = sim
            .simulate_invoke(
                call.clone(),
                AuthMode::Record {
                    allow_non_root: false,
                    address_v2,
                },
                &source,
            )
            .expect("record");
        assert_eq!(recorded.auth.len(), 1);
        assert_eq!(
            matches!(
                recorded.auth[0].credentials,
                SorobanCredentials::AddressV2(_)
            ),
            address_v2
        );
        let signed = sign_authorization_entry(
            recorded.auth[0].clone(),
            &nid,
            ctx.sequence_number + 10,
            &[&signer],
        )
        .expect("sign");
        let ok = sim
            .simulate_invoke(
                call.clone(),
                AuthMode::Enforce(vec![signed.clone()]),
                &source,
            )
            .expect("enforced run with our signature");
        assert_eq!(ok.return_value, ScVal::Bool(true));

        // signed for the other network: the host must reject it
        let wrong = sign_authorization_entry(
            recorded.auth[0].clone(),
            &network_id("Public Global Stellar Network ; September 2015"),
            ctx.sequence_number + 10,
            &[&signer],
        )
        .expect("sign");
        match sim.simulate_invoke(call.clone(), AuthMode::Enforce(vec![wrong]), &source) {
            Err(SimError::InvocationFailed { .. }) => {}
            other => panic!("mainnet-signed entry accepted on testnet: {other:?}"),
        }
    }
}
