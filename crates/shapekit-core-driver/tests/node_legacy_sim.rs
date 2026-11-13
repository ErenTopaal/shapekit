//! Simulation below protocol 30 against a real core: on fresh networks at
//! protocols 22, 24 and 28 every Soroban transaction here is priced by the
//! matching older soroban-simulation (`sim_legacy`) and must then be applied
//! by core 30.0.0 with txSUCCESS, or be refused exactly where the protocol
//! says so. Needs docker, so it is ignored by default; run with `--include-ignored`.
//!
//! Per protocol: echo built with the older soroban-sdk (upload, deploy,
//! invoke), the XLM SAC deployed from its asset and a SAC transfer. At 28
//! also CAP-73 (SAC `trust`, XLM SAC transfer creating an account), a SAC
//! transfer to a muxed address, and extend TTL with a muxed source. An
//! invocation with a muxed source is refused at every protocol here.

mod node_support;

use ed25519_dalek::SigningKey;
use node_support::{TESTNET, config};
use sha2::{Digest, Sha256};
use shapekit_core_driver::chain::{Applied, ChainDriver, ChainError};
use shapekit_core_driver::contracts::{echo_wasm_for, interface_protocol};
use shapekit_core_driver::keys::{
    account_address, account_id, derive_key, muxed_account_with_id, root_key,
};
use shapekit_core_driver::node::SubmitStatus;
use shapekit_core_driver::sim::AuthMode;
use shapekit_core_driver::sim_legacy::AnySimulator;
use shapekit_core_driver::tx::{BASE_FEE, TransactionBuilder, assemble_soroban, sign_transaction};
use stellar_xdr::{
    AlphaNum4, Asset, AssetCode4, ContractDataDurability, ContractExecutable, ContractId,
    ContractIdPreimage, CreateAccountOp, CreateContractArgs, ExtendFootprintTtlOp, ExtensionPoint,
    HostFunction, Int128Parts, InvokeContractArgs, InvokeHostFunctionOp, LedgerEntryChange,
    LedgerEntryData, LedgerKey, LedgerKeyContractData, MuxedAccount, MuxedEd25519Account,
    OperationBody, OperationMeta, ScAddress, ScSymbol, ScVal, SorobanCredentials,
    TransactionEnvelope, TransactionMeta, TransactionResultResult,
};

const RECIPE: &str = "legacy-sim";
const STARTING_BALANCE: i64 = 100_000_000_000;

fn i128_val(v: i128) -> ScVal {
    ScVal::I128(Int128Parts {
        hi: (v >> 64) as i64,
        lo: v as u64,
    })
}

fn symbol(s: &str) -> ScSymbol {
    ScSymbol(s.try_into().unwrap())
}

fn call(contract: &ContractId, function: &str, args: Vec<ScVal>) -> HostFunction {
    HostFunction::InvokeContract(InvokeContractArgs {
        contract_address: ScAddress::Contract(contract.clone()),
        function_name: symbol(function),
        args: args.try_into().unwrap(),
    })
}

fn instance_key(id: &ContractId) -> LedgerKey {
    LedgerKey::ContractData(LedgerKeyContractData {
        contract: ScAddress::Contract(id.clone()),
        key: ScVal::LedgerKeyContractInstance,
        durability: ContractDataDurability::Persistent,
    })
}

fn operation_changes(meta: &TransactionMeta) -> Vec<LedgerEntryChange> {
    let ops: Vec<&OperationMeta> = match meta {
        TransactionMeta::V3(m) => m.operations.iter().collect(),
        TransactionMeta::V4(m) => {
            return m
                .operations
                .iter()
                .flat_map(|o| o.changes.iter().cloned())
                .collect();
        }
        TransactionMeta::V2(m) => m.operations.iter().collect(),
        TransactionMeta::V1(m) => m.operations.iter().collect(),
        TransactionMeta::V0(ops) => ops.iter().collect(),
    };
    ops.into_iter()
        .flat_map(|o| o.changes.iter().cloned())
        .collect()
}

fn created(applied: &Applied) -> Vec<LedgerEntryData> {
    operation_changes(&applied.meta)
        .into_iter()
        .filter_map(|c| match c {
            LedgerEntryChange::Created(e) => Some(e.data),
            _ => None,
        })
        .collect()
}

/// Deploys the stellar asset contract of `asset` from `source`; returns its
/// id as the host reports it.
fn deploy_sac(chain: &mut ChainDriver, source: &SigningKey, asset: Asset) -> ContractId {
    let hf = HostFunction::CreateContract(CreateContractArgs {
        contract_id_preimage: ContractIdPreimage::Asset(asset),
        executable: ContractExecutable::StellarAsset,
    });
    let env = chain.soroban(source, hf).expect("simulated sac deploy");
    let applied = chain.apply_ok(&env).expect("sac deploy applied");
    match applied.return_value() {
        Some(ScVal::Address(ScAddress::Contract(id))) => id.clone(),
        other => panic!("sac deploy returned {other:?}"),
    }
}

/// A simulated invocation whose transaction source is `source` muxed with
/// `mux_id`. Simulation sees the base account, as stellar-rpc does.
fn muxed_invoke(
    chain: &ChainDriver,
    source: &SigningKey,
    mux_id: u64,
    hf: HostFunction,
) -> TransactionEnvelope {
    let ctx = chain.node().ledger_context().unwrap();
    let sim = AnySimulator::new(chain.node().snapshot().unwrap(), &ctx).unwrap();
    let base = account_id(&source.verifying_key());
    let r = sim
        .simulate_invoke(
            hf.clone(),
            AuthMode::Record {
                allow_non_root: false,
                address_v2: false,
            },
            &base,
        )
        .expect("simulated");
    assert!(
        r.auth
            .iter()
            .all(|a| matches!(a.credentials, SorobanCredentials::SourceAccount))
    );
    let seq = chain.node().account_seq(&base).unwrap() + 1;
    let tx = TransactionBuilder::new(muxed_account_with_id(&source.verifying_key(), mux_id), seq)
        .fee(BASE_FEE)
        .operation(
            None,
            OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                host_function: hf,
                auth: Default::default(),
            }),
        )
        .build()
        .unwrap();
    let tx = assemble_soroban(tx, r.transaction_data, Some(r.auth)).unwrap();
    TransactionEnvelope::Tx(sign_transaction(tx, chain.network_id(), &[source]).unwrap())
}

#[derive(Debug)]
struct Outcome {
    protocol: u32,
    echo_interface: u32,
    steps: Vec<String>,
}

fn run(protocol: u32) -> Outcome {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path().join("work");
    std::fs::create_dir(&work).unwrap();
    let mut cfg = config(TESTNET, protocol);
    // diagnostic events would only add wall-clock core_metrics events
    cfg.diagnostic_events = false;
    let root = root_key(TESTNET);
    let mut chain = ChainDriver::start(cfg, &work).expect("chain starts");
    let mut steps = Vec::new();

    let holder = derive_key(RECIPE, "holder");
    let issuer = derive_key(RECIPE, "issuer");
    let newcomer = derive_key(RECIPE, "newcomer");
    let fund = [&holder, &issuer]
        .iter()
        .map(|k| {
            OperationBody::CreateAccount(CreateAccountOp {
                destination: account_id(&k.verifying_key()),
                starting_balance: STARTING_BALANCE,
            })
        })
        .collect();
    let env = chain.classic(&root, fund).unwrap();
    chain.apply_ok(&env).expect("funding");

    // echo built for this protocol, every step simulated by the legacy host
    let wasm = echo_wasm_for(protocol).expect("an echo build");
    let echo_interface = interface_protocol(wasm).unwrap();
    let code = chain.upload(&holder, wasm).expect("echo upload");
    let salt: [u8; 32] = Sha256::digest(b"shapekit/legacy-sim/echo").into();
    let echo = chain.deploy(&holder, &code, &salt).expect("echo deploy");
    let value = ScVal::Vec(Some(
        vec![ScVal::Symbol(symbol("legacy")), ScVal::U32(protocol)]
            .try_into()
            .unwrap(),
    ));
    let applied = chain
        .invoke(&holder, &echo, "echo", vec![value.clone()])
        .expect("echo invoke");
    assert_eq!(applied.return_value(), Some(&value));
    steps.push("echo upload, deploy, invoke".to_owned());

    let xlm = deploy_sac(&mut chain, &root, Asset::Native);
    steps.push("xlm sac deploy".to_owned());
    let to_holder = call(
        &xlm,
        "transfer",
        vec![
            ScVal::Address(account_address(&root.verifying_key())),
            ScVal::Address(account_address(&holder.verifying_key())),
            i128_val(5_000_000_000),
        ],
    );
    let env = chain.soroban(&root, to_holder).expect("simulated");
    chain.apply_ok(&env).expect("xlm sac transfer");
    steps.push("xlm sac transfer".to_owned());

    let self_transfer = call(
        &xlm,
        "transfer",
        vec![
            ScVal::Address(account_address(&holder.verifying_key())),
            ScVal::Address(account_address(&issuer.verifying_key())),
            i128_val(1_000_000),
        ],
    );
    let env = muxed_invoke(&chain, &holder, 24, self_transfer);
    // refused at every protocol here, by two different checks: before 25
    // core's queue filters it (TransactionQueue.cpp, `canAdd`, "not allowed
    // to use memo or muxed source account") although apply would accept it,
    // and from 25 the transaction itself is invalid (validateSorobanMemo in
    // TransactionFrame.cpp). a standalone node only applies what its queue
    // admits, so core 30.0.0 cannot put such an invocation in a ledger.
    match chain.apply(&env) {
        Err(ChainError::NotQueued {
            status: SubmitStatus::Error,
            result: Some(r),
        }) => {
            assert!(
                matches!(r.result, TransactionResultResult::TxSorobanInvalid),
                "{r:?}"
            );
        }
        other => panic!("muxed invoke source at {protocol}: {other:?}"),
    }
    steps.push("invoke with muxed source refused txSOROBAN_INVALID".to_owned());

    if protocol == 28 {
        // CAP-73: trust creates an unlimited trustline, no classic op needed
        let code = AssetCode4(*b"LGC\0");
        let credit = Asset::CreditAlphanum4(AlphaNum4 {
            asset_code: code.clone(),
            issuer: account_id(&issuer.verifying_key()),
        });
        let sac = deploy_sac(&mut chain, &issuer, credit);
        let trust = call(
            &sac,
            "trust",
            vec![ScVal::Address(account_address(&holder.verifying_key()))],
        );
        let env = chain.soroban(&holder, trust).expect("simulated trust");
        let applied = chain.apply_ok(&env).expect("sac trust");
        let lines: Vec<_> = created(&applied)
            .into_iter()
            .filter_map(|d| match d {
                LedgerEntryData::Trustline(t) => Some(t),
                _ => None,
            })
            .collect();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0].limit, i64::MAX);
        assert_eq!(lines[0].account_id, account_id(&holder.verifying_key()));
        match &applied.meta {
            TransactionMeta::V4(m) => {
                assert!(m.operations.iter().all(|o| o.events.is_empty()), "{m:?}")
            }
            other => panic!("meta at 28: {other:?}"),
        }
        steps.push("sac trust created an i64::MAX trustline, no event".to_owned());

        let mint = call(
            &sac,
            "transfer",
            vec![
                ScVal::Address(account_address(&issuer.verifying_key())),
                ScVal::Address(account_address(&holder.verifying_key())),
                i128_val(700),
            ],
        );
        let env = chain.soroban(&issuer, mint).expect("simulated");
        chain.apply_ok(&env).expect("credit sac transfer");
        steps.push("credit sac transfer from issuer".to_owned());

        // CAP-73: the XLM SAC creates a missing destination account
        let create = call(
            &xlm,
            "transfer",
            vec![
                ScVal::Address(account_address(&root.verifying_key())),
                ScVal::Address(account_address(&newcomer.verifying_key())),
                i128_val(1_000_000_000),
            ],
        );
        let env = chain.soroban(&root, create).expect("simulated");
        let applied = chain
            .apply_ok(&env)
            .expect("xlm sac transfer to new account");
        let accounts: Vec<_> = created(&applied)
            .into_iter()
            .filter_map(|d| match d {
                LedgerEntryData::Account(a) => Some(a),
                _ => None,
            })
            .collect();
        assert_eq!(accounts.len(), 1, "{accounts:?}");
        assert_eq!(
            accounts[0].account_id,
            account_id(&newcomer.verifying_key())
        );
        assert_eq!(accounts[0].balance, 1_000_000_000);
        steps.push("xlm sac transfer created the destination account".to_owned());

        // SEP-41 transfer whose `to` is a muxed address (CAP-67)
        let to_muxed = call(
            &xlm,
            "transfer",
            vec![
                ScVal::Address(account_address(&root.verifying_key())),
                ScVal::Address(ScAddress::MuxedAccount(MuxedEd25519Account {
                    id: 9,
                    ed25519: stellar_xdr::Uint256(holder.verifying_key().to_bytes()),
                })),
                i128_val(3_000_000),
            ],
        );
        let env = chain.soroban(&root, to_muxed).expect("simulated");
        chain
            .apply_ok(&env)
            .expect("xlm sac transfer to a muxed address");
        steps.push("xlm sac transfer to a muxed address".to_owned());

        // extend TTL priced by the p28 host, sent from a muxed source,
        // which core accepts for extend and restore at every protocol
        let ctx = chain.node().ledger_context().unwrap();
        let sim = AnySimulator::new(chain.node().snapshot().unwrap(), &ctx).unwrap();
        assert_eq!(sim.protocol(), 28);
        let keys = vec![instance_key(&echo)];
        let ext = sim.simulate_extend_ttl(&keys, 50_000).expect("extend sim");
        assert!(ext.restore.is_none());
        assert_eq!(
            ext.transaction_data.resources.footprint.read_only.to_vec(),
            keys
        );
        let base = account_id(&holder.verifying_key());
        let seq = chain.node().account_seq(&base).unwrap() + 1;
        let tx = TransactionBuilder::new(muxed_account_with_id(&holder.verifying_key(), 3), seq)
            .fee(BASE_FEE)
            .operation(
                None,
                OperationBody::ExtendFootprintTtl(ExtendFootprintTtlOp {
                    ext: ExtensionPoint::V0,
                    extend_to: 50_000,
                }),
            )
            .build()
            .unwrap();
        let tx = assemble_soroban(tx, ext.transaction_data, None).unwrap();
        assert!(matches!(tx.source_account, MuxedAccount::MuxedEd25519(_)));
        let env =
            TransactionEnvelope::Tx(sign_transaction(tx, chain.network_id(), &[&holder]).unwrap());
        chain
            .apply_ok(&env)
            .expect("extend ttl with a muxed source");
        steps.push("extend ttl with muxed source".to_owned());
    }

    let summary = chain.finish(&tmp.path().join("chain")).expect("finish");
    assert_eq!(summary.protocol, protocol);
    Outcome {
        protocol,
        echo_interface,
        steps,
    }
}

#[test]
#[ignore = "needs docker and the pinned stellar-core image; run with --include-ignored"]
fn legacy_simulation_is_accepted_by_core() {
    for (protocol, interface) in [(22, 22), (24, 23), (28, 28)] {
        let o = run(protocol);
        eprintln!("{o:#?}");
        assert_eq!(o.protocol, protocol);
        assert_eq!(o.echo_interface, interface);
        let want = if protocol == 28 { 9 } else { 4 };
        assert_eq!(o.steps.len(), want, "{:?}", o.steps);
    }
}
