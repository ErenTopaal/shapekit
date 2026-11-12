//! Whether stellar-core 30.0.0 applies Soroban upload, deploy and invoke on
//! fresh standalone networks at protocols 22, 24, 26 and 28 (the plan's
//! phase 3 blocker), and what happens to this repository's own contracts
//! there. Needs docker and `SHAPEKIT_DOCKER_TESTS=1`.
//!
//! The repository's contracts are built with soroban-sdk 29, which writes
//! interface protocol 29 into `contractenvmetav0`. soroban-env-host refuses
//! a wasm whose interface protocol is above the ledger's
//! (`check_contract_interface_version`), so echo.wasm is expected to fail on
//! every protocol below 29, both in simulation and when core applies it.
//! To answer the blocker itself the test also uploads a minimal contract
//! declaring the target protocol as its interface version.

mod node_support;

use node_support::{TESTNET, config, enabled};
use sha2::{Digest, Sha256};
use shapekit_core_driver::chain::{ChainDriver, ChainError};
use shapekit_core_driver::contracts::{
    CONTRACTS_INTERFACE_PROTOCOL, ECHO_WASM, interface_protocol,
};
use shapekit_core_driver::keys::{account_address, root_key};
use shapekit_core_driver::sim::SimError;
use shapekit_core_driver::sim_legacy::{AnySimulator, SIMULATED_PROTOCOLS};
use shapekit_core_driver::tx::{contract_id_from_address, contract_preimage};
use stellar_xdr::{
    ContractDataDurability, ContractEventBody, ContractExecutable, ContractId,
    CreateContractArgsV2, DiagnosticEvent, Hash, HostFunction, InvokeContractArgs,
    InvokeHostFunctionResult, LedgerCloseMeta, LedgerFootprint, LedgerKey, LedgerKeyContractCode,
    LedgerKeyContractData, OperationResult, OperationResultTr, ScAddress, ScSymbol, ScVal,
    SorobanAuthorizationEntry, SorobanAuthorizedFunction, SorobanAuthorizedInvocation,
    SorobanCredentials, SorobanResources, SorobanTransactionData, SorobanTransactionDataExt,
    TransactionMeta, TransactionResultResult,
};

fn section(out: &mut Vec<u8>, id: u8, body: &[u8]) {
    out.push(id);
    leb(out, body.len());
    out.extend_from_slice(body);
}

fn leb(out: &mut Vec<u8>, mut v: usize) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

/// A complete Soroban contract written as raw wasm: one exported function
/// `echo(v: Val) -> Val` that returns its argument, and a
/// `contractenvmetav0` section declaring `protocol` as the interface
/// version, pre-release 0. It imports no host functions, so nothing but the
/// interface version ties it to a protocol.
fn minimal_echo_wasm(protocol: u32) -> Vec<u8> {
    let mut w = b"\0asm\x01\0\0\0".to_vec();
    // one type: (i64) -> i64
    section(&mut w, 1, &[0x01, 0x60, 0x01, 0x7e, 0x01, 0x7e]);
    // one function of type 0
    section(&mut w, 3, &[0x01, 0x00]);
    // export "echo" as function 0
    section(&mut w, 7, &[0x01, 0x04, b'e', b'c', b'h', b'o', 0x00, 0x00]);
    // body: no locals, local.get 0, end
    section(&mut w, 10, &[0x01, 0x04, 0x00, 0x20, 0x00, 0x0b]);
    let name = b"contractenvmetav0";
    let mut custom = vec![name.len() as u8];
    custom.extend_from_slice(name);
    custom.extend_from_slice(&0u32.to_be_bytes());
    custom.extend_from_slice(&protocol.to_be_bytes());
    custom.extend_from_slice(&0u32.to_be_bytes());
    section(&mut w, 0, &custom);
    w
}

/// How the minimal contract's transactions got their resources.
#[derive(Debug, PartialEq, Eq)]
enum Pricing {
    Simulated,
    /// no soroban-simulation release for this protocol is linked; the
    /// reason is kept and the resources were set by hand.
    ByHand(String),
}

#[derive(Debug)]
struct Outcome {
    protocol: u32,
    meta: &'static str,
    echo_simulation: String,
    echo_applied: String,
    echo_host_messages: Vec<String>,
    minimal_pricing: Pricing,
    minimal_results: Vec<String>,
    minimal_return: ScVal,
}

fn meta_version(m: &LedgerCloseMeta) -> &'static str {
    match m {
        LedgerCloseMeta::V0(_) => "LedgerCloseMeta v0",
        LedgerCloseMeta::V1(_) => "LedgerCloseMeta v1",
        LedgerCloseMeta::V2(_) => "LedgerCloseMeta v2",
    }
}

fn data(read_only: Vec<LedgerKey>, read_write: Vec<LedgerKey>) -> SorobanTransactionData {
    SorobanTransactionData {
        ext: SorobanTransactionDataExt::V0,
        resources: SorobanResources {
            footprint: LedgerFootprint {
                read_only: read_only.try_into().unwrap(),
                read_write: read_write.try_into().unwrap(),
            },
            instructions: 100_000_000,
            disk_read_bytes: 20_000,
            write_bytes: 20_000,
        },
        resource_fee: 100_000_000,
    }
}

fn describe(r: &TransactionResultResult) -> String {
    match r {
        TransactionResultResult::TxSuccess(_) => "txSUCCESS".into(),
        TransactionResultResult::TxFailed(ops) => match ops.as_slice() {
            [OperationResult::OpInner(OperationResultTr::InvokeHostFunction(r))] => match r {
                InvokeHostFunctionResult::Trapped => {
                    "txFAILED, INVOKE_HOST_FUNCTION_TRAPPED".into()
                }
                other => format!("txFAILED, {other:?}"),
            },
            other => format!("txFAILED, {other:?}"),
        },
        other => format!("{other:?}"),
    }
}

fn strings(v: &ScVal, out: &mut Vec<String>) {
    match v {
        ScVal::String(s) => out.push(String::from_utf8_lossy(s.as_slice()).into_owned()),
        ScVal::Vec(Some(items)) => items.iter().for_each(|i| strings(i, out)),
        _ => {}
    }
}

/// The text of every diagnostic event core recorded for a transaction.
fn host_messages(meta: &TransactionMeta) -> Vec<String> {
    let events: Vec<&DiagnosticEvent> = match meta {
        TransactionMeta::V3(m) => m
            .soroban_meta
            .iter()
            .flat_map(|s| s.diagnostic_events.iter())
            .collect(),
        TransactionMeta::V4(m) => m.diagnostic_events.iter().collect(),
        _ => Vec::new(),
    };
    let mut out = Vec::new();
    for e in events {
        let ContractEventBody::V0(body) = &e.event.body;
        strings(&body.data, &mut out);
    }
    out
}

fn code_key(wasm: &[u8]) -> LedgerKey {
    LedgerKey::ContractCode(LedgerKeyContractCode {
        hash: Hash(Sha256::digest(wasm).into()),
    })
}

fn instance_key(id: &ContractId) -> LedgerKey {
    LedgerKey::ContractData(LedgerKeyContractData {
        contract: ScAddress::Contract(id.clone()),
        key: ScVal::LedgerKeyContractInstance,
        durability: ContractDataDurability::Persistent,
    })
}

fn run(protocol: u32) -> Outcome {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path().join("work");
    std::fs::create_dir(&work).unwrap();
    // diagnostic events on: core then records the host's reason for a failed
    // invocation in the meta, which is the evidence wanted here
    let mut cfg = config(TESTNET, protocol);
    cfg.diagnostic_events = true;
    let root = root_key(TESTNET);
    let mut chain = ChainDriver::start(cfg, &work).expect("chain starts");
    let nid = *chain.network_id();

    let ctx = chain.node().ledger_context().unwrap();
    let sim_setup = AnySimulator::new(chain.node().snapshot().unwrap(), &ctx).err();

    // simulation, at the ledger's protocol, of the sdk 29 wasm
    let upload = HostFunction::UploadContractWasm(ECHO_WASM.to_vec().try_into().unwrap());
    let echo_simulation = match chain.soroban(&root, upload.clone()) {
        Err(ChainError::Sim(SimError::InvocationFailed {
            error,
            diagnostic_events,
        })) => {
            let mut texts = Vec::new();
            for e in &diagnostic_events {
                let ContractEventBody::V0(body) = &e.event.body;
                strings(&body.data, &mut texts);
            }
            format!("{error}; {texts:?}")
        }
        Ok(_) => "simulation succeeded".to_owned(),
        Err(e) => format!("simulation setup failed: {e}"),
    };

    // core decides on its own: the same upload with generous hand-set
    // resources
    let env = chain
        .soroban_with(
            &root,
            upload,
            data(vec![], vec![code_key(ECHO_WASM)]),
            vec![],
        )
        .expect("hand-built upload");
    let applied = chain.apply(&env).expect("core applies it");
    let echo_applied = describe(&applied.result.result);
    let echo_host_messages = host_messages(&applied.meta);

    // a contract declaring this protocol: upload, deploy, invoke
    let wasm = minimal_echo_wasm(protocol);
    assert_eq!(interface_protocol(&wasm), Some(protocol));
    let salt: [u8; 32] = Sha256::digest(b"shapekit/protocols/minimal-echo").into();
    let (minimal_pricing, minimal_results, minimal_return) = match sim_setup {
        None => {
            let code = chain.upload(&root, &wasm).expect("minimal upload");
            let id = chain.deploy(&root, &code, &salt).expect("minimal deploy");
            let call = chain
                .invoke(&root, &id, "echo", vec![ScVal::U32(7)])
                .expect("minimal invoke");
            let ret = call.return_value().cloned().expect("a return value");
            (Pricing::Simulated, vec!["txSUCCESS".to_owned(); 3], ret)
        }
        Some(reason) => {
            let deployer = account_address(&root.verifying_key());
            let id = contract_id_from_address(&nid, &deployer, &salt).unwrap();
            let code = Hash(Sha256::digest(&wasm).into());
            let mut results = Vec::new();
            let up = HostFunction::UploadContractWasm(wasm.clone().try_into().unwrap());
            let env = chain
                .soroban_with(&root, up, data(vec![], vec![code_key(&wasm)]), vec![])
                .unwrap();
            results.push(describe(&chain.apply(&env).unwrap().result.result));
            let create = CreateContractArgsV2 {
                contract_id_preimage: contract_preimage(&deployer, &salt),
                executable: ContractExecutable::Wasm(code),
                constructor_args: Default::default(),
            };
            let auth = SorobanAuthorizationEntry {
                credentials: SorobanCredentials::SourceAccount,
                root_invocation: SorobanAuthorizedInvocation {
                    function: SorobanAuthorizedFunction::CreateContractV2HostFn(create.clone()),
                    sub_invocations: Default::default(),
                },
            };
            let env = chain
                .soroban_with(
                    &root,
                    HostFunction::CreateContractV2(create),
                    data(vec![code_key(&wasm)], vec![instance_key(&id)]),
                    vec![auth],
                )
                .unwrap();
            results.push(describe(&chain.apply(&env).unwrap().result.result));
            let call = HostFunction::InvokeContract(InvokeContractArgs {
                contract_address: ScAddress::Contract(id.clone()),
                function_name: ScSymbol("echo".try_into().unwrap()),
                args: vec![ScVal::U32(7)].try_into().unwrap(),
            });
            let env = chain
                .soroban_with(
                    &root,
                    call,
                    data(vec![instance_key(&id), code_key(&wasm)], vec![]),
                    vec![],
                )
                .unwrap();
            let applied = chain.apply(&env).unwrap();
            results.push(describe(&applied.result.result));
            let ret = applied.return_value().cloned().unwrap_or(ScVal::Void);
            (Pricing::ByHand(reason.to_string()), results, ret)
        }
    };
    let meta = meta_version(chain.last_meta());
    let summary = chain.finish(&tmp.path().join("chain")).expect("finish");
    assert_eq!(summary.protocol, protocol);
    Outcome {
        protocol,
        meta,
        echo_simulation,
        echo_applied,
        echo_host_messages,
        minimal_pricing,
        minimal_results,
        minimal_return,
    }
}

#[test]
fn soroban_on_older_protocols() {
    if !enabled("soroban_on_older_protocols") {
        return;
    }
    assert_eq!(
        interface_protocol(ECHO_WASM),
        Some(CONTRACTS_INTERFACE_PROTOCOL)
    );
    let mut outcomes = Vec::new();
    for protocol in [22, 24, 26, 28, 30] {
        let o = run(protocol);
        eprintln!("{o:#?}");
        outcomes.push(o);
    }
    for o in &outcomes {
        let p = o.protocol;
        assert_eq!(o.minimal_results, vec!["txSUCCESS"; 3], "protocol {p}");
        assert_eq!(o.minimal_return, ScVal::U32(7), "protocol {p}");
        if p < CONTRACTS_INTERFACE_PROTOCOL {
            assert_eq!(
                o.echo_applied, "txFAILED, INVOKE_HOST_FUNCTION_TRAPPED",
                "protocol {p}"
            );
            assert!(
                o.echo_host_messages
                    .iter()
                    .any(|m| m.contains("contract protocol number is newer than host")),
                "protocol {p}: {:?}",
                o.echo_host_messages
            );
        } else {
            assert_eq!(o.echo_simulation, "simulation succeeded", "protocol {p}");
            assert_eq!(o.echo_applied, "txSUCCESS", "protocol {p}");
        }
        // each release host runs one protocol; sim_legacy links 22, 24
        // and 28 beside 30, and nothing for 26
        let simulated = SIMULATED_PROTOCOLS.contains(&p);
        assert_eq!(
            o.minimal_pricing == Pricing::Simulated,
            simulated,
            "protocol {p}"
        );
        if !simulated {
            assert!(
                o.echo_simulation
                    .contains("no simulator for ledger protocol"),
                "protocol {p}: {}",
                o.echo_simulation
            );
        } else if p < CONTRACTS_INTERFACE_PROTOCOL {
            // the older host refuses the sdk 29 wasm in simulation already
            assert!(
                o.echo_simulation
                    .contains("contract protocol number is newer than host"),
                "protocol {p}: {}",
                o.echo_simulation
            );
        }
        let want = if p < 23 {
            "LedgerCloseMeta v1"
        } else {
            "LedgerCloseMeta v2"
        };
        assert_eq!(o.meta, want, "protocol {p}");
    }
}
