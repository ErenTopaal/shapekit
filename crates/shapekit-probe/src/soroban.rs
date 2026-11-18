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

use crate::carriers::{Base, BaseOutcome, Carrier, XLM, extra, no_extra};
use crate::sign::{AuthSigner, SignPlan};
use crate::values::Replacement;
use crate::world::{ARCHIVAL_MIN_PERSISTENT_TTL, ECHO_WASM, Subject, World, WorldKind};
use serde_json::json;
use shapekit_core_driver::keys::account_strkey;

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

/// What an edit of a Soroban invocation changed, so the variant is built again around it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rebuild {
    /// The host function: the variant takes the auth and footprint the host records for the
    /// new call, as a client builds a call from scratch.
    HostFunction,
    /// An authorization entry: the entries stay as the edit wrote them, the footprint is
    /// the one the host touches running the call with them.
    Auth,
}

/// What an edit at `edit` changed in a Soroban invocation, `None` for an edit that leaves
/// the call, its auth and its footprint as the base has them, or that is itself the
/// footprint or the resources.
pub fn rebuild_scope(edit: &str) -> Option<Rebuild> {
    let inside = |p: &str| edit == p || edit.starts_with(&format!("{p}/"));
    if inside(HF) {
        Some(Rebuild::HostFunction)
    } else if inside(&format!("{OP}/auth")) {
        Some(Rebuild::Auth)
    } else {
        None
    }
}

/// How the host ran a rebuilt variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostRun {
    /// The host error the call failed with when it ran with the variant's own footprint and
    /// auth, `None` when it succeeded.
    pub error: Option<String>,
    /// Addresses whose recorded authorization the variant carries unsigned: the probe holds
    /// no key for them.
    pub unsigned: Vec<String>,
}

impl HostRun {
    /// The variant name's suffix that records the run.
    pub fn note(&self) -> String {
        let mut s = match &self.error {
            None => "own footprint and auth, the host runs it".to_owned(),
            Some(e) => format!("own footprint and auth, the host fails it with {e}"),
        };
        if !self.unsigned.is_empty() {
            s.push_str(&format!(
                ", authorization unsigned for {} (no key)",
                self.unsigned.join(", ")
            ));
        }
        s
    }
}

fn the_op(tx: &mut stellar_xdr::Transaction) -> Result<&mut stellar_xdr::InvokeHostFunctionOp> {
    ensure!(tx.operations.len() == 1, "a soroban transaction has one op");
    let op = tx
        .operations
        .iter_mut()
        .next()
        .context("a soroban transaction has one op")?;
    let OperationBody::InvokeHostFunction(op) = &mut op.body else {
        bail!("the edited transaction does not invoke a host function");
    };
    Ok(op)
}

fn v1(json: &serde_json::Value) -> Result<stellar_xdr::Transaction> {
    match serde_json::from_value(json.clone())? {
        stellar_xdr::TransactionEnvelope::Tx(e) => Ok(e.tx),
        _ => bail!("a soroban carrier is a v1 envelope"),
    }
}

fn put_tx(json: &mut serde_json::Value, tx: &stellar_xdr::Transaction) -> Result<()> {
    crate::edit::set(json, "/tx/tx", serde_json::to_value(tx)?)
}

/// Builds the edited invocation in `json` again so that only the edited value can make it
/// fail: a host function edit gets the auth the host records for the new call, signed with
/// every key the subject holds, and an auth edit keeps its entries; either way the
/// footprint and resources are the ones the host uses running the call with the auth the
/// variant carries, with room to spare. A call the host fails keeps the footprint of what
/// it touched up to the failure, so core fails it at the same point. `plan` is the base's
/// and takes the new auth signers; `json` ends up signed.
pub fn rebuild(
    w: &World,
    s: &Subject,
    json: &mut serde_json::Value,
    plan: &mut SignPlan,
    edit: &str,
    scope: Rebuild,
    base: &stellar_xdr::TransactionEnvelope,
) -> Result<HostRun> {
    let sim = simulator(w)?;
    let source = account_id(&s.source.verifying_key());
    let network = w.network_id();
    let mut unsigned = Vec::new();
    // the host error of a call whose recording run failed: the run that grants every
    // authorization and reads every entry, so the error is the call's own
    let mut recorded_error = None;
    if scope == Rebuild::HostFunction {
        let mut tx = v1(json)?;
        let hf = the_op(&mut tx)?.host_function.clone();
        // non-root authorizations recorded too: a recording that refuses them fails a call
        // for an authorization, not for its value
        let rec = sim.record_invoke(
            hf.clone(),
            AuthMode::Record {
                allow_non_root: true,
                address_v2: false,
            },
            &source,
        )?;
        match rec.result {
            Ok(_) => {
                let exp = w.driver.last_closed() + AUTH_VALID_FOR;
                let keys: Vec<&SigningKey> = s.funded();
                let mut signers = Vec::new();
                let mut auth = Vec::new();
                for (i, mut e) in rec.auth.into_iter().enumerate() {
                    if let SorobanCredentials::Address(c) = &mut e.credentials {
                        c.signature_expiration_ledger = exp;
                        match keys
                            .iter()
                            .find(|k| account_address(&k.verifying_key()) == c.address)
                        {
                            Some(k) => signers.push(AuthSigner {
                                entry: format!("{OP}/auth/{i}"),
                                keys: vec![(*k).clone()],
                                delegates: Vec::new(),
                            }),
                            None => unsigned.push(
                                serde_json::to_value(&c.address)?
                                    .as_str()
                                    .context("an address is a strkey in XDR-JSON")?
                                    .to_owned(),
                            ),
                        }
                    }
                    auth.push(e);
                }
                the_op(&mut tx)?.auth = auth.try_into()?;
                plan.auth = signers;
            }
            Err(e) => {
                // a failed invocation rolls back the authorizations it recorded, so the run
                // hands none back (soroban-env-host 30.0.0 src/auth.rs, the authorization
                // manager's snapshot and rollback around every frame). the variant carries
                // instead what its client would sign: the base's entries, each one that
                // authorizes the base call now authorizing the edited call, signed by the
                // base's signers
                let base_call = base_function(base)?;
                let op = the_op(&mut tx)?;
                let mut auth = op.auth.to_vec();
                for entry in &mut auth {
                    let f = &entry.root_invocation.function;
                    if authorized_as(f, &base_call).as_ref() == Some(f)
                        && let Some(to) = authorized_as(f, &hf)
                    {
                        entry.root_invocation.function = to;
                    }
                }
                op.auth = auth.try_into()?;
                recorded_error = Some(e);
            }
        }
        put_tx(json, &tx)?;
    }
    // the entries as the variant carries them, signed
    crate::sign::sign(json, plan, &network, Some(edit))?;
    let mut tx = v1(json)?;
    let op = the_op(&mut tx)?;
    let (hf, auth) = (op.host_function.clone(), op.auth.to_vec());
    let run = sim.record_invoke(hf.clone(), AuthMode::Enforce(auth.clone()), &source)?;
    let stellar_xdr::TransactionExt::V1(old) = &tx.ext else {
        bail!("a soroban carrier has transaction data");
    };
    let inclusion = i64::from(tx.fee) - old.resource_fee;
    let data = match (&run.result, &run.resources) {
        (Ok(_), _) => spare(
            sim.simulate_invoke(hf, AuthMode::Enforce(auth), &source)?
                .transaction_data,
        ),
        // refused before the call touched any entry: the base's footprint is as good as any,
        // the host never reads it
        (Err(_), None) => old.clone(),
        (Err(_), Some(recorded)) => {
            let mut resources = recorded.clone();
            resources.instructions = resources.instructions.saturating_add(SPARE_INSTRUCTIONS);
            resources.disk_read_bytes = resources.disk_read_bytes.saturating_add(SPARE_BYTES);
            resources.write_bytes = resources.write_bytes.saturating_add(SPARE_BYTES);
            SorobanTransactionData {
                ext: if run.restored_rw_entry_indices.is_empty() {
                    SorobanTransactionDataExt::V0
                } else {
                    SorobanTransactionDataExt::V1(stellar_xdr::SorobanResourcesExtV0 {
                        archived_soroban_entries: run
                            .restored_rw_entry_indices
                            .clone()
                            .try_into()?,
                    })
                },
                resources,
                // set below, once the envelope's size is known
                resource_fee: 0,
            }
        }
    };
    tx.ext = stellar_xdr::TransactionExt::V1(data);
    if run.result.is_err() && run.resources.is_some() {
        // the fee is a fixed-width field, so the size measured with a zero fee is the size
        // the envelope keeps
        put_tx(json, &tx)?;
        let size = crate::sign::sign(json, plan, &network, Some(edit))?
            .to_xdr(Limits::none())?
            .len();
        let stellar_xdr::TransactionExt::V1(d) = &mut tx.ext else {
            unreachable!("set above");
        };
        let fee = sim.failed_invoke_fee(
            &d.resources,
            run.restored_rw_entry_indices.len() as u32,
            size.try_into()?,
        )?;
        d.resource_fee = fee.saturating_mul(2).saturating_add(SPARE_FEE);
    }
    let stellar_xdr::TransactionExt::V1(d) = &tx.ext else {
        unreachable!("set above");
    };
    tx.fee = u32::try_from(inclusion + d.resource_fee)
        .context("the rebuilt transaction's fee does not fit a u32")?;
    put_tx(json, &tx)?;
    crate::sign::sign(json, plan, &network, Some(edit))?;
    if let Some(e) = &recorded_error {
        // the variant reaches the error the call fails with when nothing else stands in its
        // way; any other error is the variant's authorization or footprint, not its value
        ensure!(
            run.result.as_ref().err() == Some(e),
            "the call fails with {e} when every authorization is granted, but with {:?} as the variant carries it",
            run.result
        );
    }
    Ok(HostRun {
        error: run.result.err(),
        unsigned,
    })
}

/// The host function of the base envelope.
fn base_function(base: &stellar_xdr::TransactionEnvelope) -> Result<HostFunction> {
    let stellar_xdr::TransactionEnvelope::Tx(e) = base else {
        bail!("a soroban carrier is a v1 envelope");
    };
    let mut tx = e.tx.clone();
    Ok(the_op(&mut tx)?.host_function.clone())
}

/// What an authorization entry names when it authorizes `hf` at the root, in the arm of
/// `like`; `None` for an upload, which no entry authorizes. The host reads either create arm
/// as a v2 create, a v1 one with no constructor arguments, and records it in the v2 arm
/// (soroban-env-host 30.0.0 src/auth.rs 767-810, `AuthorizedFunction::from_xdr` and
/// `to_xdr`), so an entry keeps the arm it was recorded in.
fn authorized_as(
    like: &stellar_xdr::SorobanAuthorizedFunction,
    hf: &HostFunction,
) -> Option<stellar_xdr::SorobanAuthorizedFunction> {
    use stellar_xdr::SorobanAuthorizedFunction as F;
    match hf {
        HostFunction::InvokeContract(a) => Some(F::ContractFn(a.clone())),
        HostFunction::CreateContract(a) => Some(match like {
            F::CreateContractV2HostFn(_) => F::CreateContractV2HostFn(CreateContractArgsV2 {
                contract_id_preimage: a.contract_id_preimage.clone(),
                executable: a.executable.clone(),
                constructor_args: Default::default(),
            }),
            _ => F::CreateContractHostFn(a.clone()),
        }),
        HostFunction::CreateContractV2(a) => Some(F::CreateContractV2HostFn(a.clone())),
        HostFunction::UploadContractWasm(_) => None,
    }
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
                    // recording draws its nonce from a fixed seed, the same one for every
                    // recording of this one shared account, so each attempt takes its own
                    nonce: subject_nonce(s),
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

/// `auth_tree.guarded(signer, false)`, which never calls `require_auth`, with an auth entry for
/// `credentials` declared anyway: the host converts the entry and never matches it.
fn unmatched(
    w: &World,
    s: &Subject,
    credentials: SorobanCredentials,
    plan: SignPlan,
) -> Result<Base> {
    let c = w.contracts()?;
    let signer = match &credentials {
        SorobanCredentials::Address(a) => a.address.clone(),
        SorobanCredentials::AddressWithDelegates(d) => d.address_credentials.address.clone(),
        _ => bail!("unmatched credentials name an address"),
    };
    let args = InvokeContractArgs {
        contract_address: ScAddress::Contract(c.auth_tree.clone()),
        function_name: ScSymbol("guarded".try_into()?),
        args: vec![ScVal::Address(signer), ScVal::Bool(false)].try_into()?,
    };
    let hf = HostFunction::InvokeContract(args.clone());
    let data = spare(simulated(w, s, hf.clone())?);
    let entry = SorobanAuthorizationEntry {
        credentials,
        root_invocation: stellar_xdr::SorobanAuthorizedInvocation {
            function: stellar_xdr::SorobanAuthorizedFunction::ContractFn(args),
            sub_invocations: Default::default(),
        },
    };
    let mut b = assemble(w, s, hf, data, vec![entry], plan)?;
    b.at = None;
    Ok(b)
}

fn unmatched_auth(w: &mut World, s: &Subject) -> Result<Base> {
    let creds = SorobanCredentials::Address(SorobanAddressCredentials {
        address: account_address(&s.peers[0].verifying_key()),
        nonce: subject_nonce(s),
        signature_expiration_ledger: w.driver.last_closed() + AUTH_VALID_FOR,
        signature: ScVal::Void,
    });
    let plan = SignPlan {
        tx: vec![s.source.clone()],
        auth: vec![AuthSigner {
            entry: format!("{OP}/auth/0"),
            keys: vec![s.peers[0].clone()],
            delegates: Vec::new(),
        }],
        ..Default::default()
    };
    unmatched(w, s, creds, plan)
}

fn unmatched_delegated(w: &mut World, s: &Subject) -> Result<Base> {
    let c = w.contracts()?;
    let delegate = c.delegate.clone();
    let creds = SorobanCredentials::AddressWithDelegates(SorobanAddressCredentialsWithDelegates {
        address_credentials: SorobanAddressCredentials {
            address: ScAddress::Contract(c.delegating.clone()),
            nonce: subject_nonce(s),
            signature_expiration_ledger: w.driver.last_closed() + AUTH_VALID_FOR,
            signature: ScVal::Void,
        },
        delegates: vec![SorobanDelegateSignature {
            address: account_address(&delegate.verifying_key()),
            signature: ScVal::Void,
            nested_delegates: Default::default(),
        }]
        .try_into()?,
    });
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
    unmatched(w, s, creds, plan)
}

/// Peer 0 consumes `nonce`: a call of `auth_tree.node` that requires peer 0, with peer 0's
/// address credentials at that nonce, sent by peer 1 so the subject's source keeps its
/// sequence number. The nonce entry lives until the credentials' signature expiration,
/// [`AUTH_VALID_FOR`] ledgers ahead (soroban-env-host 30.0.0 src/auth.rs 2958-3010,
/// `consume_nonce`).
fn consume_nonce(w: &mut World, s: &Subject, nonce: &serde_json::Value) -> Result<()> {
    let nonce: i64 = match nonce {
        serde_json::Value::String(n) => n.parse()?,
        other => other.as_i64().context("a nonce is an i64")?,
    };
    let peer = account_address(&s.peers[0].verifying_key());
    let hf = auth_call(w, peer.clone())?;
    let sim = simulator(w)?;
    let sender = &s.peers[1];
    let source = account_id(&sender.verifying_key());
    let rec = sim.simulate_invoke(hf.clone(), RECORD, &source)?;
    let exp = w.driver.last_closed() + AUTH_VALID_FOR;
    let mut auth = Vec::new();
    for mut e in rec.auth {
        let SorobanCredentials::Address(c) = &mut e.credentials else {
            bail!("the call that consumes a nonce records address credentials only");
        };
        ensure!(
            c.address == peer,
            "the call requires an address other than peer 0"
        );
        c.nonce = nonce;
        c.signature_expiration_ledger = exp;
        let payload = authorization_payload(&e, w.driver.network_id())?;
        if let SorobanCredentials::Address(c) = &mut e.credentials {
            c.signature = account_signature(&payload, &[&s.peers[0]])?;
        }
        auth.push(e);
    }
    ensure!(
        auth.len() == 1,
        "expected one auth entry, simulation recorded {}",
        auth.len()
    );
    let enforced = sim.simulate_invoke(hf.clone(), AuthMode::Enforce(auth.clone()), &source)?;
    let tx = w
        .builder(sender)?
        .fee(BASE_FEE)
        .operation(
            None,
            OperationBody::InvokeHostFunction(stellar_xdr::InvokeHostFunctionOp {
                host_function: hf,
                auth: Default::default(),
            }),
        )
        .build()?;
    let tx = assemble_soroban(tx, spare(enforced.transaction_data), Some(auth))?;
    let plan = SignPlan {
        tx: vec![sender.clone()],
        ..Default::default()
    };
    let env = crate::sign::sign_typed(World::unsigned(tx), &plan, &w.network_id())?;
    w.driver
        .apply_ok(&env)
        .context("peer 0 consuming the nonce")?;
    Ok(())
}

/// The value of a nonce that is already used: the base's own nonce, which the same address
/// consumed in an earlier ledger, its entry still live. Every value the type gives is a
/// nonce the address never used.
fn nonce_replays(f: &str, base: &serde_json::Value) -> Vec<Replacement> {
    match f {
        "nonce" => vec![Replacement {
            name:
                "the same nonce, consumed by the same address in an earlier ledger and still live"
                    .into(),
            value: base.clone(),
            xdr: None,
            before: Some(consume_nonce),
        }],
        _ => Vec::new(),
    }
}

fn replayed_nonce(
    _: &World,
    _: &Subject,
    f: &str,
    base: &serde_json::Value,
    _: &serde_json::Value,
) -> Result<Vec<Replacement>> {
    Ok(nonce_replays(f, base))
}

fn subject_nonce(s: &Subject) -> i64 {
    let h: [u8; 32] = Sha256::digest(format!("shapekit.probe/{}/nonce", s.tag)).into();
    i64::from_be_bytes(h[..8].try_into().expect("eight bytes")) & i64::MAX
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
        Carrier {
            extra: |_, _, f, _, _| {
                Ok(match f {
                    "args" => vec![
                        extra(
                            "a value of another type, which echo takes",
                            json!([{"symbol": "other"}]),
                        ),
                        extra(
                            "a value of another type, which echo takes, i64",
                            json!([{"i64": "-5"}]),
                        ),
                    ],
                    _ => Vec::new(),
                })
            },
            ..c("InvokeContractArgs", "a call of echo(7)", echo_u32)
        },
        c("SorobanTransactionData", "a call of echo(7)", |w, s| {
            Ok(at(echo_u32(w, s)?, DATA))
        }),
        c("SorobanResources", "a call of echo(7)", |w, s| {
            Ok(at(echo_u32(w, s)?, &format!("{DATA}/resources")))
        }),
        Carrier {
            id: "echo",
            extra: |w, _, f, base, _| {
                // a key no part of the call touches: core requires footprint keys to be of an
                // allowed type and size, not to exist or be read
                let c = w.contracts()?;
                let untouched =
                    serde_json::to_value(LedgerKey::ContractData(LedgerKeyContractData {
                        contract: ScAddress::Contract(c.echo.clone()),
                        key: sym("untouched")?,
                        durability: ContractDataDurability::Persistent,
                    }))?;
                let mut more = base.as_array().cloned().unwrap_or_default();
                more.push(untouched);
                Ok(match f {
                    "readOnly" | "readWrite" => vec![extra(
                        "an extra key the call never touches",
                        serde_json::Value::Array(more),
                    )],
                    _ => Vec::new(),
                })
            },
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
        Carrier {
            id: "unmatched",
            ..c(
                "SorobanAuthorizationEntry",
                "a peer's address credentials for a call that never requires them",
                unmatched_auth,
            )
        },
        Carrier {
            id: "matched",
            ..c(
                "SorobanAuthorizedInvocation",
                "the root invocation a peer authorizes",
                address_auth,
            )
        },
        Carrier {
            id: "unmatched",
            ..c(
                "SorobanAuthorizedInvocation",
                "the root invocation of credentials no require_auth matches",
                unmatched_auth,
            )
        },
        Carrier {
            id: "matched",
            extra: replayed_nonce,
            ..c(
                "SorobanAddressCredentials",
                "a peer's address credentials for one require_auth",
                address_auth,
            )
        },
        Carrier {
            id: "unmatched",
            extra: replayed_nonce,
            ..c(
                "SorobanAddressCredentials",
                "a peer's address credentials for a call that never requires them",
                unmatched_auth,
            )
        },
        c(
            "SCNonceKey",
            "the nonce key of a peer's credentials in the read-write footprint",
            address_auth,
        ),
        Carrier {
            id: "matched",
            ..c(
                "SorobanAddressCredentialsWithDelegates",
                "a delegating account authorizing through its one delegate",
                delegated_auth,
            )
        },
        Carrier {
            id: "unmatched",
            ..c(
                "SorobanAddressCredentialsWithDelegates",
                "a delegating account's credentials for a call that never requires them",
                unmatched_delegated,
            )
        },
        Carrier {
            id: "matched",
            ..c(
                "SorobanDelegateSignature",
                "the delegate's signature for a delegating account",
                delegated_auth,
            )
        },
        Carrier {
            id: "unmatched",
            ..c(
                "SorobanDelegateSignature",
                "the delegate's signature in credentials no require_auth matches",
                unmatched_delegated,
            )
        },
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
        Carrier {
            extra: |_, s, f, _, _| {
                Ok(match f {
                    "constructorArgs" => vec![extra(
                        "another owner, which the constructor takes",
                        json!([{"address": account_strkey(&s.peers[0].verifying_key())}]),
                    )],
                    _ => Vec::new(),
                })
            },
            ..c(
                "CreateContractArgsV2",
                "a create contract of the factory, constructor argument the source",
                |w, s| Ok(at(factory(w, s)?, &format!("{HF}/create_contract_v2"))),
            )
        },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_edits_of_the_call_or_its_auth_are_rebuilt() {
        let hf = format!("{HF}/invoke_contract/function_name");
        assert_eq!(rebuild_scope(&hf), Some(Rebuild::HostFunction));
        assert_eq!(rebuild_scope(HF), Some(Rebuild::HostFunction));
        assert_eq!(
            rebuild_scope(&format!("{OP}/auth/0/credentials/address/nonce")),
            Some(Rebuild::Auth)
        );
        assert_eq!(rebuild_scope(&format!("{OP}/auth")), Some(Rebuild::Auth));
        // the footprint, the resources and the transaction around the call are the field
        // themselves, and stay as the edit wrote them
        assert_eq!(
            rebuild_scope(&format!("{DATA}/resources/footprint/read_only")),
            None
        );
        assert_eq!(rebuild_scope("/tx/tx/fee"), None);
        assert_eq!(rebuild_scope(&format!("{OP}/authx")), None);
        assert_eq!(rebuild_scope(&format!("{HF}x")), None);
    }

    #[test]
    fn a_nonce_also_takes_the_base_nonce_already_consumed() {
        let base = json!("4598685041475779676");
        let got = nonce_replays("nonce", &base);
        assert_eq!(got.len(), 1);
        // the same value as the base: only the ledger before the attempt differs
        assert_eq!(got[0].value, base);
        assert!(got[0].xdr.is_none());
        assert!(got[0].before.is_some());
        for other in ["address", "signatureExpirationLedger", "signature"] {
            assert!(nonce_replays(other, &base).is_empty());
        }
    }

    #[test]
    fn the_note_names_the_host_error_and_unsigned_addresses() {
        let ok = HostRun {
            error: None,
            unsigned: Vec::new(),
        };
        assert_eq!(ok.note(), "own footprint and auth, the host runs it");
        let failed = HostRun {
            error: Some("Error(Storage, MissingValue)".into()),
            unsigned: vec!["GA".into(), "CB".into()],
        };
        assert_eq!(
            failed.note(),
            "own footprint and auth, the host fails it with Error(Storage, MissingValue), authorization unsigned for GA, CB (no key)"
        );
    }
}
