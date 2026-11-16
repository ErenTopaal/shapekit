//! Carriers of the Soroban structs. Base transactions take their footprint from simulation
//! and declare resources with room to spare, so a replacement that leaves the call's needs
//! unchanged is judged on its value and not on a budget it happens to exceed.

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use shapekit_core_driver::keys::{account_address, account_id};
use shapekit_core_driver::node::EntryState;
use shapekit_core_driver::sim::AuthMode;
use shapekit_core_driver::sim_legacy::AnySimulator;
use shapekit_core_driver::tx::{
    BASE_FEE, account_signature, assemble_soroban, authorization_payload, contract_preimage,
};
use stellar_xdr::{
    ConfigSettingId, ContractDataDurability, ContractExecutable, ContractExecutableExternalRef,
    ContractId, CreateContractArgs, CreateContractArgsV2, ExtendFootprintTtlOp, ExtensionPoint,
    Hash, HostFunction, Int128Parts, Int256Parts, InvokeContractArgs, LedgerKey,
    LedgerKeyConfigSetting, LedgerKeyContractData, LedgerKeyTtl, Limits, MuxedContract,
    MuxedEd25519Account, OperationBody, RestoreFootprintOp, ScAddress, ScContractInstance, ScMap,
    ScMapEntry, ScString, ScSymbol, ScVal, ScVec, SorobanAddressCredentials,
    SorobanAddressCredentialsWithDelegates, SorobanAuthorizationEntry, SorobanCredentials,
    SorobanDelegateSignature, SorobanTransactionData, SorobanTransactionDataExt, UInt128Parts,
    UInt256Parts, Uint256, WriteXdr,
};

use crate::carriers::{Base, BaseOutcome, Carrier, XLM, no_extra};
use crate::sign::{AuthSigner, SignPlan};
use crate::world::{ARCHIVAL_MIN_PERSISTENT_TTL, ECHO_WASM, Subject, World, WorldKind};

/// Ledgers an authorization signature stays valid.
const AUTH_VALID_FOR: u32 = 100;

/// Room added to simulated resources.
const SPARE_INSTRUCTIONS: u32 = 10_000_000;
const SPARE_BYTES: u32 = 10_000;
const SPARE_FEE: i64 = 50_000_000;

const OP: &str = "/tx/tx/operations/0/body/invoke_host_function";
const HF: &str = "/tx/tx/operations/0/body/invoke_host_function/host_function";
const DATA: &str = "/tx/tx/ext/v1";

fn simulator(w: &World) -> Result<AnySimulator> {
    let node = w.driver.node();
    Ok(AnySimulator::new(
        node.snapshot()?,
        &node.ledger_context()?,
    )?)
}

fn sym(s: &str) -> Result<ScVal> {
    Ok(ScVal::Symbol(ScSymbol(s.try_into()?)))
}

fn call(contract: &ContractId, function: &str, args: Vec<ScVal>) -> Result<HostFunction> {
    Ok(HostFunction::InvokeContract(InvokeContractArgs {
        contract_address: ScAddress::Contract(contract.clone()),
        function_name: ScSymbol(function.try_into()?),
        args: args.try_into()?,
    }))
}

fn spare(mut d: SorobanTransactionData) -> SorobanTransactionData {
    let r = &mut d.resources;
    r.instructions = r.instructions.saturating_add(SPARE_INSTRUCTIONS);
    r.disk_read_bytes = r.disk_read_bytes.saturating_add(SPARE_BYTES);
    r.write_bytes = r.write_bytes.saturating_add(SPARE_BYTES);
    d.resource_fee = d.resource_fee.saturating_mul(2).saturating_add(SPARE_FEE);
    d
}

/// The signer of an address in an auth entry.
type Signers<'a> = &'a [(ScAddress, &'a SigningKey)];

const RECORD: AuthMode = AuthMode::Record {
    allow_non_root: false,
    address_v2: false,
};

/// The usual client flow: record the auth the call needs, sign every address entry, simulate
/// again with the signed entries for the resources, add room to spare.
fn invoke(w: &World, s: &Subject, hf: HostFunction, signers: Signers) -> Result<Base> {
    let sim = simulator(w)?;
    let source = account_id(&s.source.verifying_key());
    let rec = sim.simulate_invoke(hf.clone(), RECORD, &source)?;
    let exp = w.driver.last_closed() + AUTH_VALID_FOR;
    let mut plan = SignPlan {
        tx: vec![s.source.clone()],
        ..Default::default()
    };
    let mut auth = Vec::new();
    for (i, mut e) in rec.auth.into_iter().enumerate() {
        if let SorobanCredentials::Address(c) = &mut e.credentials {
            let key = signers
                .iter()
                .find(|(a, _)| *a == c.address)
                .map(|(_, k)| (*k).clone())
                .with_context(|| format!("no signer for {:?}", c.address))?;
            c.signature_expiration_ledger = exp;
            let payload = authorization_payload(&e, w.driver.network_id())?;
            if let SorobanCredentials::Address(c) = &mut e.credentials {
                c.signature = account_signature(&payload, &[&key])?;
            }
            plan.auth.push(AuthSigner {
                entry: format!("{OP}/auth/{i}"),
                keys: vec![key],
                delegates: Vec::new(),
            });
        }
        auth.push(e);
    }
    let enforced = sim.simulate_invoke(hf.clone(), AuthMode::Enforce(auth.clone()), &source)?;
    assemble(w, s, hf, spare(enforced.transaction_data), auth, plan)
}

/// [`invoke`] for a call that needs no address credentials.
pub fn invoke_base(w: &World, s: &Subject, hf: HostFunction) -> Result<Base> {
    invoke(w, s, hf, &[])
}

/// The simulated transaction data of a call from the subject, recording its auth.
pub fn simulated(w: &World, s: &Subject, hf: HostFunction) -> Result<SorobanTransactionData> {
    let source = account_id(&s.source.verifying_key());
    Ok(simulator(w)?
        .simulate_invoke(hf, RECORD, &source)?
        .transaction_data)
}

fn assemble(
    w: &World,
    s: &Subject,
    hf: HostFunction,
    data: SorobanTransactionData,
    auth: Vec<SorobanAuthorizationEntry>,
    plan: SignPlan,
) -> Result<Base> {
    let tx = w
        .builder(&s.source)?
        .fee(BASE_FEE)
        .operation(
            None,
            OperationBody::InvokeHostFunction(stellar_xdr::InvokeHostFunctionOp {
                host_function: hf,
                auth: Default::default(),
            }),
        )
        .build()?;
    let tx = assemble_soroban(tx, data, Some(auth))?;
    Ok(Base {
        env: World::unsigned(tx),
        plan,
        at: None,
    })
}

fn echo(w: &World, s: &Subject, value: ScVal) -> Result<Base> {
    let c = w.contracts()?;
    let hf = call(&c.echo, "echo", vec![value])?;
    invoke(w, s, hf, &[])
}

fn at(mut b: Base, pointer: &str) -> Base {
    b.at = Some(pointer.into());
    b
}

fn echo_u32(w: &mut World, s: &Subject) -> Result<Base> {
    echo(w, s, ScVal::U32(7))
}

/// `echo(value)` with the resources of `echo(7)`: for values simulation cannot run.
fn echo_unsimulated(w: &World, s: &Subject, value: ScVal) -> Result<Base> {
    let mut b = echo(w, s, ScVal::U32(7))?;
    let stellar_xdr::TransactionEnvelope::Tx(e) = &mut b.env else {
        bail!("an echo call is a v1 envelope");
    };
    let mut ops = e.tx.operations.to_vec();
    let OperationBody::InvokeHostFunction(op) = &mut ops[0].body else {
        bail!("an echo call invokes a host function");
    };
    let HostFunction::InvokeContract(args) = &mut op.host_function else {
        bail!("an echo call invokes a contract");
    };
    args.args = vec![value].try_into()?;
    e.tx.operations = ops.try_into()?;
    Ok(b)
}

fn with_footprint_key(mut b: Base, key: LedgerKey) -> Result<Base> {
    let stellar_xdr::TransactionEnvelope::Tx(e) = &mut b.env else {
        bail!("a v1 envelope");
    };
    let stellar_xdr::TransactionExt::V1(d) = &mut e.tx.ext else {
        bail!("a soroban transaction");
    };
    let mut ro = d.resources.footprint.read_only.to_vec();
    ro.push(key);
    d.resources.footprint.read_only = ro.try_into()?;
    Ok(b)
}

fn echo_last_key(contract: &ContractId) -> Result<LedgerKey> {
    Ok(LedgerKey::ContractData(LedgerKeyContractData {
        contract: ScAddress::Contract(contract.clone()),
        key: sym("last")?,
        durability: ContractDataDurability::Persistent,
    }))
}

/// `auth_tree.node([], [{signer, fanout: 0}])`: one `require_auth` of `signer`.
fn auth_call(w: &World, signer: ScAddress) -> Result<HostFunction> {
    let c = w.contracts()?;
    let level = ScVal::Map(Some(ScMap(
        vec![
            ScMapEntry {
                key: sym("fanout")?,
                val: ScVal::U32(0),
            },
            ScMapEntry {
                key: sym("signer")?,
                val: ScVal::Address(signer),
            },
        ]
        .try_into()?,
    )));
    call(
        &c.auth_tree,
        "node",
        vec![
            ScVal::Vec(Some(ScVec(Default::default()))),
            ScVal::Vec(Some(ScVec(vec![level].try_into()?))),
        ],
    )
}

/// A call that needs peer 0's address credentials.
fn address_auth(w: &mut World, s: &Subject) -> Result<Base> {
    let peer = account_address(&s.peers[0].verifying_key());
    let hf = auth_call(w, peer.clone())?;
    invoke(w, s, hf, &[(peer, &s.peers[0])])
}

/// A call that needs the delegating account's authorization, given through its delegate.
fn delegated_auth(w: &mut World, s: &Subject) -> Result<Base> {
    let c = w.contracts()?;
    let account = ScAddress::Contract(c.delegating.clone());
    let delegate = c.delegate.clone();
    let hf = auth_call(w, account)?;
    let sim = simulator(w)?;
    let source = account_id(&s.source.verifying_key());
    let rec = sim.simulate_invoke(hf.clone(), RECORD, &source)?;
    let [e] = <[SorobanAuthorizationEntry; 1]>::try_from(rec.auth)
        .map_err(|a| anyhow::anyhow!("expected one auth entry, simulation recorded {}", a.len()))?;
    let SorobanCredentials::Address(creds) = e.credentials else {
        bail!("expected address credentials for the delegating account");
    };
    let mut entry = SorobanAuthorizationEntry {
        credentials: SorobanCredentials::AddressWithDelegates(
            SorobanAddressCredentialsWithDelegates {
                address_credentials: SorobanAddressCredentials {
                    signature_expiration_ledger: w.driver.last_closed() + AUTH_VALID_FOR,
                    // the delegating account checks no signature of its own
                    signature: ScVal::Void,
                    ..creds
                },
                delegates: Default::default(),
            },
        ),
        root_invocation: e.root_invocation,
    };
    let payload = authorization_payload(&entry, w.driver.network_id())?;
    if let SorobanCredentials::AddressWithDelegates(d) = &mut entry.credentials {
        d.delegates = vec![SorobanDelegateSignature {
            address: account_address(&delegate.verifying_key()),
            signature: account_signature(&payload, &[&delegate])?,
            nested_delegates: Default::default(),
        }]
        .try_into()?;
    }
    let enforced =
        sim.simulate_invoke(hf.clone(), AuthMode::Enforce(vec![entry.clone()]), &source)?;
    let plan = SignPlan {
        tx: vec![s.source.clone()],
        auth: vec![AuthSigner {
            entry: format!("{OP}/auth/0"),
            keys: Vec::new(),
            delegates: vec![(
                format!("{OP}/auth/0/credentials/address_with_delegates/delegates/0"),
                delegate,
            )],
        }],
        ..Default::default()
    };
    assemble(
        w,
        s,
        hf,
        spare(enforced.transaction_data),
        vec![entry],
        plan,
    )
}

fn subject_salt(s: &Subject) -> [u8; 32] {
    Sha256::digest(format!("shapekit.probe/{}/salt", s.tag)).into()
}

fn create_v1(w: &mut World, s: &Subject) -> Result<Base> {
    let code = w.contracts()?.echo_code.clone();
    let me = account_address(&s.source.verifying_key());
    let hf = HostFunction::CreateContract(CreateContractArgs {
        contract_id_preimage: contract_preimage(&me, &subject_salt(s)),
        executable: ContractExecutable::Wasm(code),
    });
    invoke(w, s, hf, &[])
}

fn create_v2(
    w: &World,
    s: &Subject,
    executable: ContractExecutable,
    args: Vec<ScVal>,
) -> Result<Base> {
    let me = account_address(&s.source.verifying_key());
    let hf = HostFunction::CreateContractV2(CreateContractArgsV2 {
        contract_id_preimage: contract_preimage(&me, &subject_salt(s)),
        executable,
        constructor_args: args.try_into()?,
    });
    invoke(w, s, hf, &[])
}

fn factory(w: &mut World, s: &Subject) -> Result<Base> {
    let code = w.contracts()?.factory_code.clone();
    let owner = ScVal::Address(account_address(&s.source.verifying_key()));
    create_v2(w, s, ContractExecutable::Wasm(code), vec![owner])
}

/// A native asset transfer of one lumen from the source to `to`.
fn transfer(w: &mut World, s: &Subject, to: ScAddress) -> Result<Base> {
    let c = w.contracts()?;
    let hf = call(
        &c.native_sac,
        "transfer",
        vec![
            ScVal::Address(account_address(&s.source.verifying_key())),
            ScVal::Address(to),
            ScVal::I128(Int128Parts {
                hi: 0,
                lo: XLM as u64,
            }),
        ],
    )?;
    invoke(w, s, hf, &[])
}

/// Archival world: the next spare echo instance stores a value, and the probe closes
/// ledgers until that value is archived. Returns the instance and the value's key.
fn archived_value(w: &mut World) -> Result<(ContractId, LedgerKey)> {
    let c = w
        .contracts
        .as_mut()
        .context("an archival world has contracts")?;
    ensure!(
        !c.spare.is_empty(),
        "the archival world ran out of spare echo instances"
    );
    let id = c.spare.remove(0);
    let deployer = c.deployer.clone();
    w.driver
        .invoke(&deployer, &id, "echo", vec![ScVal::U32(5)])?;
    let key = echo_last_key(&id)?;
    for _ in 0..4 * ARCHIVAL_MIN_PERSISTENT_TTL {
        let got = w.driver.node().ledger_entries(std::slice::from_ref(&key))?;
        if got.entries[0].state == EntryState::Archived {
            return Ok((id, key));
        }
        w.driver.close_empty()?;
    }
    bail!(
        "the stored value is still live after {} ledgers",
        4 * ARCHIVAL_MIN_PERSISTENT_TTL
    )
}

fn footprint_op(
    w: &World,
    s: &Subject,
    body: OperationBody,
    data: SorobanTransactionData,
) -> Result<Base> {
    let tx = w
        .builder(&s.source)?
        .fee(BASE_FEE)
        .operation(None, body)
        .build()?;
    let tx = assemble_soroban(tx, spare(data), None)?;
    Ok(Base {
        env: World::unsigned(tx),
        plan: SignPlan {
            tx: vec![s.source.clone()],
            ..Default::default()
        },
        at: None,
    })
}

/// A restore of the value an archival world just archived.
fn restore(w: &mut World, s: &Subject) -> Result<Base> {
    let (_, key) = archived_value(w)?;
    let data = simulator(w)?.simulate_restore(std::slice::from_ref(&key))?;
    footprint_op(
        w,
        s,
        OperationBody::RestoreFootprint(RestoreFootprintOp {
            ext: ExtensionPoint::V0,
        }),
        data,
    )
}

const FOOTPRINT_REFUSED: &str = "TransactionFrame::checkSorobanResources refuses a footprint holding a TTL, config setting, offer, data, claimable balance or liquidity pool key (stellar-core v30.0.0 src/transactions/TransactionFrame.cpp 868-883), so core refuses the transaction at submission";

pub fn carriers() -> Vec<Carrier> {
    let c = |structure, context, build| Carrier {
        structure,
        id: "",
        context,
        world: WorldKind::Current,
        outcome: BaseOutcome::Succeeds,
        build,
        extra: no_extra,
    };
    vec![
        Carrier {
            id: "echo",
            ..c("InvokeHostFunctionOp", "a call of echo(7)", echo_u32)
        },
        Carrier {
            id: "upload",
            ..c(
                "InvokeHostFunctionOp",
                "an upload of the echo wasm",
                |w, s| {
                    let hf = HostFunction::UploadContractWasm(ECHO_WASM.to_vec().try_into()?);
                    invoke(w, s, hf, &[])
                },
            )
        },
        Carrier {
            id: "auth",
            ..c(
                "InvokeHostFunctionOp",
                "a call that needs a peer's address credentials",
                address_auth,
            )
        },
        Carrier {
            id: "invoke",
            ..c("Transaction", "a call of echo(7)", |w, s| {
                Ok(at(echo_u32(w, s)?, "/tx/tx"))
            })
        },
        Carrier {
            id: "invoke",
            ..c("Operation", "a call of echo(7)", |w, s| {
                Ok(at(echo_u32(w, s)?, "/tx/tx/operations/0"))
            })
        },
        c("InvokeContractArgs", "a call of echo(7)", echo_u32),
        c("SorobanTransactionData", "a call of echo(7)", |w, s| {
            Ok(at(echo_u32(w, s)?, DATA))
        }),
        c("SorobanResources", "a call of echo(7)", |w, s| {
            Ok(at(echo_u32(w, s)?, &format!("{DATA}/resources")))
        }),
        Carrier {
            id: "echo",
            ..c("LedgerFootprint", "a call of echo(7)", echo_u32)
        },
        Carrier {
            id: "restore",
            world: WorldKind::Archival,
            ..c(
                "LedgerFootprint",
                "a restore of an archived echo value",
                |w, s| Ok(at(restore(w, s)?, &format!("{DATA}/resources/footprint"))),
            )
        },
        c(
            "LedgerKeyContractData",
            "the read-write key of the value echo stores",
            |w, s| {
                Ok(at(
                    echo_u32(w, s)?,
                    &format!("{DATA}/resources/footprint/read_write/0/contract_data"),
                ))
            },
        ),
        c(
            "LedgerKeyContractCode",
            "the echo wasm key in a call's footprint",
            echo_u32,
        ),
        Carrier {
            outcome: BaseOutcome::Refused(FOOTPRINT_REFUSED),
            ..c(
                "LedgerKeyTtl",
                "a TTL key added to the read-only footprint of echo(7)",
                |w, s| {
                    let c = w.contracts()?;
                    let k = echo_last_key(&c.echo)?;
                    let hash = Hash(Sha256::digest(k.to_xdr(Limits::none())?).into());
                    let b = echo_u32(w, s)?;
                    with_footprint_key(b, LedgerKey::Ttl(LedgerKeyTtl { key_hash: hash }))
                },
            )
        },
        Carrier {
            outcome: BaseOutcome::Refused(FOOTPRINT_REFUSED),
            ..c(
                "LedgerKeyConfigSetting",
                "a config setting key added to the read-only footprint of echo(7)",
                |w, s| {
                    let b = echo_u32(w, s)?;
                    with_footprint_key(
                        b,
                        LedgerKey::ConfigSetting(LedgerKeyConfigSetting {
                            config_setting_id: ConfigSettingId::ContractMaxSizeBytes,
                        }),
                    )
                },
            )
        },
        c(
            "SCMapEntry",
            "the one entry of a map echo receives",
            |w, s| {
                echo(
                    w,
                    s,
                    ScVal::Map(Some(ScMap(
                        vec![ScMapEntry {
                            key: sym("k")?,
                            val: ScVal::U32(1),
                        }]
                        .try_into()?,
                    ))),
                )
            },
        ),
        c("Int128Parts", "an i128 echo receives", |w, s| {
            let b = echo(w, s, ScVal::I128(Int128Parts { hi: 1, lo: 2 }))?;
            Ok(at(b, &format!("{HF}/invoke_contract/args/0/i128")))
        }),
        c("UInt128Parts", "a u128 echo receives", |w, s| {
            let b = echo(w, s, ScVal::U128(UInt128Parts { hi: 1, lo: 2 }))?;
            Ok(at(b, &format!("{HF}/invoke_contract/args/0/u128")))
        }),
        c("Int256Parts", "an i256 echo receives", |w, s| {
            let v = ScVal::I256(Int256Parts {
                hi_hi: 1,
                hi_lo: 2,
                lo_hi: 3,
                lo_lo: 4,
            });
            let b = echo(w, s, v)?;
            Ok(at(b, &format!("{HF}/invoke_contract/args/0/i256")))
        }),
        c("UInt256Parts", "a u256 echo receives", |w, s| {
            let v = ScVal::U256(UInt256Parts {
                hi_hi: 1,
                hi_lo: 2,
                lo_hi: 3,
                lo_lo: 4,
            });
            let b = echo(w, s, v)?;
            Ok(at(b, &format!("{HF}/invoke_contract/args/0/u256")))
        }),
        Carrier {
            outcome: BaseOutcome::Fails(
                "a contract instance value has no host value: soroban-env-common 30.0.0 src/convert.rs 589-600 and soroban-env-host 30.0.0 src/host/conversion.rs 650-663 refuse ScVal::ContractInstance, so the call's argument conversion traps and the operation fails",
            ),
            ..c(
                "SCContractInstance",
                "a contract instance value passed to echo, with echo(7)'s resources",
                |w, s| {
                    let code = w.contracts()?.echo_code.clone();
                    echo_unsimulated(
                        w,
                        s,
                        ScVal::ContractInstance(ScContractInstance {
                            executable: ContractExecutable::Wasm(code),
                            storage: None,
                        }),
                    )
                },
            )
        },
        c(
            "MuxedEd25519Account",
            "a native asset transfer to a muxed account, id 9",
            |w, s| {
                let to = ScAddress::MuxedAccount(MuxedEd25519Account {
                    id: 9,
                    ed25519: Uint256(s.peers[0].verifying_key().to_bytes()),
                });
                let b = transfer(w, s, to)?;
                Ok(at(b, &format!("{HF}/invoke_contract/args/1/address")))
            },
        ),
        c(
            "MuxedContract",
            "a native asset transfer to the echo contract, muxed with id 9",
            |w, s| {
                let id = w.contracts()?.echo.clone();
                let to = ScAddress::MuxedContract(MuxedContract {
                    id: 9,
                    contract_id: id,
                });
                let b = transfer(w, s, to)?;
                Ok(at(b, &format!("{HF}/invoke_contract/args/1/address")))
            },
        ),
        Carrier {
            id: "address",
            ..c(
                "SorobanAuthorizationEntry",
                "a peer's address credentials for one require_auth",
                address_auth,
            )
        },
        Carrier {
            id: "source",
            ..c(
                "SorobanAuthorizationEntry",
                "source account credentials for the source's own require_auth",
                |w, s| {
                    let me = account_address(&s.source.verifying_key());
                    let hf = auth_call(w, me)?;
                    invoke(w, s, hf, &[])
                },
            )
        },
        c(
            "SorobanAuthorizedInvocation",
            "the root invocation a peer authorizes",
            address_auth,
        ),
        c(
            "SorobanAddressCredentials",
            "a peer's address credentials for one require_auth",
            address_auth,
        ),
        c(
            "SCNonceKey",
            "the nonce key of a peer's credentials in the read-write footprint",
            address_auth,
        ),
        c(
            "SorobanAddressCredentialsWithDelegates",
            "a delegating account authorizing through its one delegate",
            delegated_auth,
        ),
        c(
            "SorobanDelegateSignature",
            "the delegate's signature for a delegating account",
            delegated_auth,
        ),
        c(
            "CreateContractArgs",
            "a create contract of echo from the source",
            |w, s| Ok(at(create_v1(w, s)?, &format!("{HF}/create_contract"))),
        ),
        c(
            "ContractIDPreimageFromAddress",
            "the source and salt a create contract derives the id from",
            |w, s| {
                Ok(at(
                    create_v1(w, s)?,
                    &format!("{HF}/create_contract/contract_id_preimage/address"),
                ))
            },
        ),
        c(
            "CreateContractArgsV2",
            "a create contract of the factory, constructor argument the source",
            |w, s| Ok(at(factory(w, s)?, &format!("{HF}/create_contract_v2"))),
        ),
        c(
            "ContractExecutableExternalRef",
            "a create contract whose executable is the exec_ref contract's tag echo",
            |w, s| {
                let owner = ScAddress::Contract(w.contracts()?.exec_ref.clone());
                let exe = ContractExecutable::ExternalRef(ContractExecutableExternalRef {
                    executable_owner: owner,
                    tag: ScString("echo".try_into()?),
                });
                let b = create_v2(w, s, exe, Vec::new())?;
                Ok(at(
                    b,
                    &format!("{HF}/create_contract_v2/executable/external_ref"),
                ))
            },
        ),
        c(
            "ExtendFootprintTTLOp",
            "an extension of echo's instance and code",
            |w, s| {
                let c = w.contracts()?;
                let keys = vec![
                    LedgerKey::ContractData(LedgerKeyContractData {
                        contract: ScAddress::Contract(c.echo.clone()),
                        key: ScVal::LedgerKeyContractInstance,
                        durability: ContractDataDurability::Persistent,
                    }),
                    LedgerKey::ContractCode(stellar_xdr::LedgerKeyContractCode {
                        hash: c.echo_code.clone(),
                    }),
                ];
                let extend_to = 5000;
                let sim = simulator(w)?.simulate_extend_ttl(&keys, extend_to)?;
                ensure!(sim.restore.is_none(), "echo's entries are archived");
                footprint_op(
                    w,
                    s,
                    OperationBody::ExtendFootprintTtl(ExtendFootprintTtlOp {
                        ext: ExtensionPoint::V0,
                        extend_to,
                    }),
                    sim.transaction_data,
                )
            },
        ),
        Carrier {
            world: WorldKind::Archival,
            ..c(
                "RestoreFootprintOp",
                "a restore of an archived echo value",
                |w, s| {
                    Ok(at(
                        restore(w, s)?,
                        "/tx/tx/operations/0/body/restore_footprint",
                    ))
                },
            )
        },
        Carrier {
            world: WorldKind::Archival,
            ..c(
                "SorobanResourcesExtV0",
                "a call of echo whose stored value is archived and restored on the way",
                |w, s| {
                    let (id, _) = archived_value(w)?;
                    let hf = call(&id, "echo", vec![ScVal::U32(6)])?;
                    let b = invoke(w, s, hf, &[])?;
                    let stellar_xdr::TransactionEnvelope::Tx(e) = &b.env else {
                        bail!("a v1 envelope");
                    };
                    let stellar_xdr::TransactionExt::V1(d) = &e.tx.ext else {
                        bail!("a soroban transaction");
                    };
                    ensure!(
                        matches!(d.ext, SorobanTransactionDataExt::V1(_)),
                        "simulation marked no archived entry"
                    );
                    Ok(at(b, &format!("{DATA}/ext/v1")))
                },
            )
        },
    ]
}
