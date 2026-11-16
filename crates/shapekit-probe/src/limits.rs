//! `probe limits`: the largest value core accepts for a bounded quantity and the first it
//! refuses, found by bisection, each value on its own funded accounts and in its own ledger.
//! "Accepted" means applied successfully; the first value past it is refused at submission,
//! fails at apply, or for a bound the XDR itself sets, is bytes core cannot decode.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use serde_json::Value;
use shapekit_contracts::Stage;
use shapekit_contracts::limits::{Limit, LimitRejection};
use shapekit_contracts::validation::{ApplyStatus, ProbeAttempt, SubmitStatus};
use shapekit_core_driver::keys::{
    account_id, ed25519_signer, muxed_account, signed_payload_signer,
};
use shapekit_core_driver::tx::BASE_FEE;
use shapekit_schema::interp::Value as IValue;
use stellar_xdr::{
    Asset, ClaimPredicate, Claimant, ClaimantV0, ContractDataDurability, ContractId,
    CreateClaimableBalanceOp, HostFunction, InvokeContractArgs, LedgerKey, LedgerKeyContractData,
    Limits, ManageDataOp, Memo, OperationBody, PathPaymentStrictReceiveOp, PaymentOp,
    Preconditions, PreconditionsV2, ScAddress, ScBytes, ScSymbol, ScVal, ScVec, SetOptionsOp,
    Signer, SorobanAuthorizationEntry, SorobanAuthorizedFunction, SorobanAuthorizedInvocation,
    SorobanCredentials, SorobanTransactionData, StringM, WriteXdr,
};

use crate::carriers::{Base, XLM};
use crate::edit::Typer;
use crate::engine::{Evidence, submit, submit_raw};
use crate::sign::{SignPlan, sign};
use crate::world::{Subject, World, WorldKind};

/// What one value turns into.
pub enum Built {
    Env(Base),
    /// Bytes no XDR type holds: the envelope with one element past an XDR bound.
    Raw(Vec<u8>),
}

pub struct LimitProbe {
    /// The `registry/caps-limits.toml` row the limit answers.
    pub row: &'static str,
    /// The field the quantity belongs to.
    pub path: &'static str,
    pub measure: &'static str,
    pub world: WorldKind,
    /// A value known to be accepted and one known to be refused; the bisection runs between.
    pub lo: u64,
    pub hi: u64,
    pub build: fn(&mut World, &Typer, &Subject, u64) -> Result<Built>,
    /// Network settings the limit follows from, `entry.field` as XDR-JSON names them.
    pub settings: &'static [&'static str],
}

/// One measured limit and the two attempts at its edge.
pub struct Measured {
    pub limit: Limit,
    pub row: &'static str,
    pub accepted: Evidence,
    pub refused: Evidence,
    /// Every value tried, in order, with its outcome.
    pub tried: Vec<(u64, String)>,
}

fn accepted(a: &ProbeAttempt) -> bool {
    a.applied == ApplyStatus::Succeeded
}

fn describe(a: &ProbeAttempt) -> String {
    format!(
        "{} {} {:?}",
        format!("{:?}", a.submission).to_lowercase(),
        a.result_code.as_deref().unwrap_or("-"),
        a.applied
    )
}

/// The rejection core gave: a failed operation by its operation result.
fn rejection(a: &ProbeAttempt) -> Result<LimitRejection> {
    if a.submission == SubmitStatus::Undecodable {
        return Ok(LimitRejection {
            stage: Stage::Decode,
            result_code: None,
        });
    }
    let stage = if a.applied == ApplyStatus::Failed {
        Stage::Apply
    } else {
        Stage::Submit
    };
    let code = a
        .result_code
        .clone()
        .context("a refusal without a result")?;
    let detail = match (&a.result_xdr, code.as_str()) {
        (Some(x), "tx_failed") => {
            use stellar_xdr::ReadXdr;
            let r = stellar_xdr::TransactionResult::from_xdr_base64(x, Limits::none())?;
            match r.result {
                stellar_xdr::TransactionResultResult::TxFailed(ops) => {
                    let op = ops.first().context("tx_failed without operation results")?;
                    Some(op_code(op)?)
                }
                _ => None,
            }
        }
        _ => None,
    };
    Ok(LimitRejection {
        stage,
        result_code: Some(detail.unwrap_or(code)),
    })
}

/// `invoke_host_function.resource_limit_exceeded`: the operation and its result arm.
fn op_code(op: &stellar_xdr::OperationResult) -> Result<String> {
    let v = serde_json::to_value(op)?;
    let name = |v: &Value| -> Option<String> {
        match v {
            Value::String(s) => Some(s.clone()),
            Value::Object(m) if m.len() == 1 => m.keys().next().cloned(),
            _ => None,
        }
    };
    match &v {
        Value::Object(m) if m.contains_key("op_inner") => {
            let inner = &m["op_inner"];
            let (op, res) = match inner {
                Value::Object(i) if i.len() == 1 => i.iter().next().expect("one key"),
                other => bail!("unexpected operation result {other}"),
            };
            Ok(format!(
                "{op}.{}",
                name(res).context("an operation result without an arm")?
            ))
        }
        other => name(other).context("an operation result without an arm"),
    }
}

/// The network settings `names` name, read from core's live settings.
pub fn settings(w: &World, names: &[&str]) -> Result<BTreeMap<String, String>> {
    let set = w.driver.node().soroban_upgrade_set()?;
    let entries: Vec<Value> = set
        .updated_entry
        .iter()
        .map(serde_json::to_value)
        .collect::<std::result::Result<_, _>>()?;
    let mut out = BTreeMap::new();
    for n in names {
        let (entry, field) = match n.split_once('.') {
            Some((e, f)) => (e, Some(f)),
            None => (*n, None),
        };
        let v = entries
            .iter()
            .find_map(|e| e.get(entry))
            .with_context(|| format!("no live setting {entry}"))?;
        let v = match field {
            Some(f) => v.get(f).with_context(|| format!("no setting {n}"))?,
            None => v,
        };
        let s = match v {
            Value::Number(x) => x.to_string(),
            Value::String(x) => x.clone(),
            other => bail!("setting {n} is {other}"),
        };
        out.insert((*n).to_owned(), s);
    }
    Ok(out)
}

/// The setting `name` as a number.
fn setting(w: &World, name: &str) -> Result<u64> {
    Ok(settings(w, &[name])?[name].parse()?)
}

fn attempt(w: &mut World, typer: &Typer, p: &LimitProbe, n: u64) -> Result<Evidence> {
    let s = Subject::new(format!("limit:{}#{n}", p.row));
    w.fund(&s.funded())?;
    let variant = format!("{} = {n}", p.measure);
    let slug = format!("{}-{n}", p.row.replace(['.', '_'], "-"));
    let (attempt, ledger, envelope) =
        match (p.build)(w, typer, &s, n).with_context(|| format!("{}: building {n}", p.row))? {
            Built::Env(b) => {
                let mut json = serde_json::to_value(&b.env)?;
                let env = sign(&mut json, &b.plan, &w.network_id(), None)?;
                let (a, l) = submit(w, &env, &variant)?;
                (a, l, env.to_xdr(Limits::none())?)
            }
            Built::Raw(bytes) => (submit_raw(w, &bytes, &variant)?, None, bytes),
        };
    Ok(Evidence {
        slug,
        field: p.path.to_owned(),
        variant,
        protocol: p.world.protocol(),
        chain: p.world.chain(),
        envelope,
        attempt,
        ledger,
    })
}

/// Bisects one limit.
pub fn measure(w: &mut World, typer: &Typer, p: &LimitProbe) -> Result<Measured> {
    let mut tried = Vec::new();
    let mut lo = attempt(w, typer, p, p.lo)?;
    tried.push((p.lo, describe(&lo.attempt)));
    ensure!(
        accepted(&lo.attempt),
        "{}: {} = {} was expected to apply, core answered {}",
        p.row,
        p.measure,
        p.lo,
        describe(&lo.attempt)
    );
    let mut hi = attempt(w, typer, p, p.hi)?;
    tried.push((p.hi, describe(&hi.attempt)));
    ensure!(
        !accepted(&hi.attempt),
        "{}: {} = {} was expected to be refused, core applied it",
        p.row,
        p.measure,
        p.hi
    );
    let (mut a, mut b) = (p.lo, p.hi);
    while b - a > 1 {
        let mid = a + (b - a) / 2;
        let e = attempt(w, typer, p, mid)?;
        tried.push((mid, describe(&e.attempt)));
        if accepted(&e.attempt) {
            a = mid;
            lo = e;
        } else {
            b = mid;
            hi = e;
        }
    }
    let limit = Limit {
        path: p.path.to_owned(),
        measure: Some(p.measure.to_owned()),
        max_accepted: a.to_string(),
        first_rejected: b.to_string(),
        rejection: rejection(&hi.attempt)?,
        network_settings: if p.settings.is_empty() {
            BTreeMap::new()
        } else {
            settings(w, p.settings)?
        },
        witnesses: Vec::new(),
    };
    Ok(Measured {
        limit,
        row: p.row,
        accepted: lo,
        refused: hi,
        tried,
    })
}

// ---------------------------------------------------------------- building blocks

fn plan(keys: &[&SigningKey]) -> SignPlan {
    SignPlan {
        tx: keys.iter().map(|k| (*k).clone()).collect(),
        ..Default::default()
    }
}

fn payment(to: &SigningKey, amount: i64) -> OperationBody {
    OperationBody::Payment(PaymentOp {
        destination: muxed_account(&to.verifying_key()),
        asset: Asset::Native,
        amount,
    })
}

fn classic(w: &World, s: &Subject, ops: Vec<OperationBody>, keys: &[&SigningKey]) -> Result<Built> {
    let tx = w.tx(&s.source, ops.into_iter().map(|o| (None, o)).collect())?;
    Ok(Built::Env(Base {
        env: World::unsigned(tx),
        plan: plan(keys),
        at: None,
    }))
}

/// The signed envelope of `base`, encoded.
fn signed_bytes(w: &World, b: &Base) -> Result<Vec<u8>> {
    let mut json = serde_json::to_value(&b.env)?;
    Ok(sign(&mut json, &b.plan, &w.network_id(), None)?.to_xdr(Limits::none())?)
}

/// `bytes` with the variable-length value at `pointer` grown by one element (an array, which
/// takes a copy of its last element) or one byte (a string or opaque, which takes `fill`).
fn grow(typer: &Typer, bytes: &[u8], pointer: &str, fill: u8) -> Result<Vec<u8>> {
    let (off, ty, v) = typer.envelope_offset(bytes, pointer)?;
    grow_at(typer, bytes, off, &ty, &v, fill)
}

fn grow_at(
    typer: &Typer,
    bytes: &[u8],
    off: usize,
    ty: &shapekit_schema::Type,
    v: &IValue,
    fill: u8,
) -> Result<Vec<u8>> {
    use shapekit_schema::Type;
    let interp = shapekit_schema::interp::Interp::new(typer.schema());
    let enc = |t: &Type, x: &IValue| -> Result<Vec<u8>> {
        interp
            .encode_as(t, x)
            .map_err(|e| anyhow::anyhow!("encoding: {e}"))
    };
    let old = enc(ty, v)?;
    let new = match (typer.schema().resolve(ty), v) {
        (Type::VarArray { element, .. }, IValue::Array(items)) => {
            let last = items.last().context("growing an empty array")?;
            let mut out = ((items.len() + 1) as u32).to_be_bytes().to_vec();
            out.extend_from_slice(&old[4..]);
            out.extend(enc(element, last)?);
            out
        }
        (Type::String { .. } | Type::OpaqueVar { .. }, IValue::String(b) | IValue::Opaque(b)) => {
            let mut data = b.clone();
            data.push(fill);
            let mut out = (data.len() as u32).to_be_bytes().to_vec();
            out.extend(&data);
            while out.len() % 4 != 0 {
                out.push(0);
            }
            out
        }
        (t, x) => bail!("cannot grow a {t:?} holding {x:?}"),
    };
    ensure!(
        bytes[off..off + old.len()] == old[..],
        "the value's bytes are not at the offset found"
    );
    let mut out = bytes[..off].to_vec();
    out.extend(new);
    out.extend_from_slice(&bytes[off + old.len()..]);
    Ok(out)
}

/// An XDR-bounded value: `bound` builds through stellar-xdr, `bound + 1` is the bound's
/// envelope grown by one at `pointer`.
fn xdr_bounded(
    w: &mut World,
    typer: &Typer,
    s: &Subject,
    n: u64,
    bound: u64,
    pointer: &str,
    make: fn(&mut World, &Subject, u64) -> Result<Base>,
) -> Result<Built> {
    ensure!(
        n <= bound + 1,
        "only one value past an XDR bound can be sent"
    );
    if n <= bound {
        return Ok(Built::Env(make(w, s, n)?));
    }
    let b = make(w, s, bound)?;
    let bytes = signed_bytes(w, &b)?;
    Ok(Built::Raw(grow(typer, &bytes, pointer, b'a')?))
}

fn fresh_keys(s: &Subject, n: u64) -> Vec<SigningKey> {
    (0..n).map(|i| s.fresh(&format!("k{i}"))).collect()
}

/// The source adds `keys` as signers of weight 1, up to 19 per transaction.
fn add_signers(w: &mut World, s: &Subject, keys: &[SigningKey]) -> Result<()> {
    for chunk in keys.chunks(19) {
        let ops = chunk
            .iter()
            .map(|k| {
                OperationBody::SetOptions(SetOptionsOp {
                    signer: Some(Signer {
                        key: ed25519_signer(&k.verifying_key()),
                        weight: 1,
                    }),
                    ..Default::default()
                })
            })
            .collect();
        let src = s.source.clone();
        w.classic_ok(&src, ops, &[])?;
    }
    Ok(())
}

// ---------------------------------------------------------------- soroban building blocks

fn echo(w: &World) -> Result<ContractId> {
    Ok(w.contracts()?.echo.clone())
}

fn call(contract: &ContractId, f: &str, args: Vec<ScVal>) -> Result<HostFunction> {
    Ok(HostFunction::InvokeContract(InvokeContractArgs {
        contract_address: ScAddress::Contract(contract.clone()),
        function_name: ScSymbol(f.try_into()?),
        args: args.try_into()?,
    }))
}

/// The resources of a simulated `echo(7)` raised to the network's per-transaction maximum, for
/// calls whose own simulation fails past the limit.
fn max_resources(w: &World, s: &Subject) -> Result<SorobanTransactionData> {
    let hf = call(&echo(w)?, "echo", vec![ScVal::U32(7)])?;
    let mut d = crate::soroban::simulated(w, s, hf)?;
    d.resources.instructions =
        u32::try_from(setting(w, "contract_compute_v0.tx_max_instructions")?)?;
    d.resources.disk_read_bytes = u32::try_from(setting(
        w,
        "contract_ledger_cost_v0.tx_max_disk_read_bytes",
    )?)?;
    d.resources.write_bytes =
        u32::try_from(setting(w, "contract_ledger_cost_v0.tx_max_write_bytes")?)?;
    d.resource_fee = 2_000_000_000;
    Ok(d)
}

fn soroban(
    w: &World,
    s: &Subject,
    hf: HostFunction,
    data: SorobanTransactionData,
    auth: Vec<SorobanAuthorizationEntry>,
) -> Result<Built> {
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
    let tx = shapekit_core_driver::tx::assemble_soroban(tx, data, Some(auth))?;
    Ok(Built::Env(Base {
        env: World::unsigned(tx),
        plan: plan(&[&s.source]),
        at: None,
    }))
}

fn data_key(contract: &ContractId, key: ScVal) -> LedgerKey {
    LedgerKey::ContractData(LedgerKeyContractData {
        contract: ScAddress::Contract(contract.clone()),
        key,
        durability: ContractDataDurability::Persistent,
    })
}

/// `echo(7)` with `extra` keys added to the read-only (or read-write) footprint.
fn echo_with_keys(w: &World, s: &Subject, extra: Vec<LedgerKey>, write: bool) -> Result<Built> {
    let hf = call(&echo(w)?, "echo", vec![ScVal::U32(7)])?;
    let mut d = max_resources(w, s)?;
    let fp = &mut d.resources.footprint;
    if write {
        let mut v = fp.read_write.to_vec();
        v.extend(extra);
        fp.read_write = v.try_into()?;
    } else {
        let mut v = fp.read_only.to_vec();
        v.extend(extra);
        fp.read_only = v.try_into()?;
    }
    soroban(w, s, hf, d, Vec::new())
}

fn bytes(n: u64) -> Result<ScVal> {
    Ok(ScVal::Bytes(ScBytes(vec![0x5a; n as usize].try_into()?)))
}

/// A wasm module of exactly `n` bytes: echo with a custom section filling the rest.
pub fn padded_wasm(n: u64) -> Result<Vec<u8>> {
    let base = crate::world::ECHO_WASM;
    let n = usize::try_from(n)?;
    let leb = |mut v: usize| {
        let mut out = Vec::new();
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                return out;
            }
            out.push(b | 0x80);
        }
    };
    let name = b"shapekit-pad";
    // section: id 0, size, name length, name, payload
    for size_len in 1..=5 {
        let fixed = base.len() + 1 + size_len + 1 + name.len();
        if n < fixed {
            continue;
        }
        let payload = n - fixed;
        let size = 1 + name.len() + payload;
        if leb(size).len() != size_len {
            continue;
        }
        let mut out = base.to_vec();
        out.push(0);
        out.extend(leb(size));
        out.push(name.len() as u8);
        out.extend(name);
        out.extend(std::iter::repeat_n(0u8, payload));
        ensure!(
            out.len() == n,
            "padded wasm has {} bytes, wanted {n}",
            out.len()
        );
        return Ok(out);
    }
    bail!("no custom section pads echo to {n} bytes")
}

/// An unmatched source-account auth entry declaring `echo(bytes(n))`, beside a call of
/// `echo(7)`: the host converts the entry and never checks it.
fn declared_bytes(w: &World, s: &Subject, n: u64) -> Result<Built> {
    let e = echo(w)?;
    let hf = call(&e, "echo", vec![ScVal::U32(7)])?;
    let entry = SorobanAuthorizationEntry {
        credentials: SorobanCredentials::SourceAccount,
        root_invocation: SorobanAuthorizedInvocation {
            function: SorobanAuthorizedFunction::ContractFn(InvokeContractArgs {
                contract_address: ScAddress::Contract(e),
                function_name: ScSymbol("echo".try_into()?),
                args: vec![bytes(n)?].try_into()?,
            }),
            sub_invocations: Default::default(),
        },
    };
    let d = max_resources(w, s)?;
    soroban(w, s, hf, d, vec![entry])
}

fn nested(depth: u64) -> Result<ScVal> {
    let mut v = ScVal::U32(1);
    for _ in 1..depth {
        v = ScVal::Vec(Some(ScVec(vec![v].try_into()?)));
    }
    Ok(v)
}

// ---------------------------------------------------------------- the probes

fn memo_text(w: &mut World, s: &Subject, n: u64) -> Result<Base> {
    let mut tx = w.tx(&s.source, vec![(None, payment(&s.peers[0], XLM))])?;
    let text: StringM<28> = vec![b'a'; n as usize].try_into()?;
    tx.memo = Memo::Text(text);
    Ok(Base {
        env: World::unsigned(tx),
        plan: plan(&[&s.source]),
        at: None,
    })
}

fn signatures(w: &mut World, s: &Subject, n: u64) -> Result<Base> {
    let keys = fresh_keys(s, n - 1);
    add_signers(w, s, &keys)?;
    // every signature must count: a payment needs the medium threshold, and core stops
    // checking once the signers reach it, leaving any later signature unused (tx_bad_auth_extra)
    let t = Some(u32::try_from(n)?);
    let src = s.source.clone();
    w.classic_ok(
        &src,
        vec![OperationBody::SetOptions(SetOptionsOp {
            low_threshold: t,
            med_threshold: t,
            high_threshold: t,
            ..Default::default()
        })],
        &[],
    )?;
    let tx = w.tx(&s.source, vec![(None, payment(&s.peers[0], XLM))])?;
    let mut all = vec![&s.source];
    all.extend(keys.iter());
    Ok(Base {
        env: World::unsigned(tx),
        plan: plan(&all),
        at: None,
    })
}

fn extra_signers(w: &mut World, s: &Subject, n: u64) -> Result<Base> {
    let keys = fresh_keys(s, n);
    let mut tx = w.tx(&s.source, vec![(None, payment(&s.peers[0], XLM))])?;
    tx.cond = Preconditions::V2(PreconditionsV2 {
        extra_signers: keys
            .iter()
            .map(|k| ed25519_signer(&k.verifying_key()))
            .collect::<Vec<_>>()
            .try_into()?,
        ..Default::default()
    });
    let mut all = vec![&s.source];
    all.extend(keys.iter());
    Ok(Base {
        env: World::unsigned(tx),
        plan: plan(&all),
        at: None,
    })
}

fn operations(w: &mut World, s: &Subject, n: u64) -> Result<Base> {
    let ops = (0..n).map(|_| (None, payment(&s.peers[0], 1))).collect();
    let tx = w.tx(&s.source, ops)?;
    Ok(Base {
        env: World::unsigned(tx),
        plan: plan(&[&s.source]),
        at: None,
    })
}

fn path(w: &mut World, s: &Subject, n: u64) -> Result<Base> {
    let tx = w.tx(
        &s.source,
        vec![(
            None,
            OperationBody::PathPaymentStrictReceive(PathPaymentStrictReceiveOp {
                send_asset: Asset::Native,
                send_max: 2 * XLM,
                destination: muxed_account(&s.peers[0].verifying_key()),
                dest_asset: Asset::Native,
                dest_amount: XLM,
                path: vec![Asset::Native; n as usize].try_into()?,
            }),
        )],
    )?;
    Ok(Base {
        env: World::unsigned(tx),
        plan: plan(&[&s.source]),
        at: None,
    })
}

fn claimants(w: &mut World, s: &Subject, n: u64) -> Result<Base> {
    let keys = fresh_keys(s, n);
    let tx = w.tx(
        &s.source,
        vec![(
            None,
            OperationBody::CreateClaimableBalance(CreateClaimableBalanceOp {
                asset: Asset::Native,
                amount: XLM,
                claimants: keys
                    .iter()
                    .map(|k| {
                        Claimant::ClaimantTypeV0(ClaimantV0 {
                            destination: account_id(&k.verifying_key()),
                            predicate: ClaimPredicate::Unconditional,
                        })
                    })
                    .collect::<Vec<_>>()
                    .try_into()?,
            }),
        )],
    )?;
    Ok(Base {
        env: World::unsigned(tx),
        plan: plan(&[&s.source]),
        at: None,
    })
}

fn signed_payload(w: &mut World, s: &Subject, n: u64) -> Result<Base> {
    let payload = vec![0x5a; n as usize];
    let mut tx = w.tx(&s.source, vec![(None, payment(&s.peers[0], XLM))])?;
    tx.cond = Preconditions::V2(PreconditionsV2 {
        extra_signers: vec![signed_payload_signer(
            &s.peers[1].verifying_key(),
            &payload,
        )?]
        .try_into()?,
        ..Default::default()
    });
    let mut b = Base {
        env: World::unsigned(tx),
        plan: plan(&[&s.source]),
        at: None,
    };
    b.plan
        .payloads
        .push((s.peers[1].clone(), "/tx/tx/cond/v2/extra_signers/0".into()));
    Ok(b)
}

fn symbol_arg(w: &mut World, s: &Subject, n: u64) -> Result<Base> {
    let sym = ScVal::Symbol(ScSymbol(vec![b'a'; n as usize].try_into()?));
    let hf = call(&echo(w)?, "echo", vec![sym])?;
    crate::soroban::invoke_base(w, s, hf)
}

const PP: &str = "/tx/tx/operations/0/body/path_payment_strict_receive/path";
const CB: &str = "/tx/tx/operations/0/body/create_claimable_balance/claimants";
const ARG0: &str =
    "/tx/tx/operations/0/body/invoke_host_function/host_function/invoke_contract/args/0/symbol";

fn level(signer: &ScAddress, fanout: u32) -> Result<ScVal> {
    Ok(ScVal::Map(Some(stellar_xdr::ScMap(
        vec![
            stellar_xdr::ScMapEntry {
                key: ScVal::Symbol(ScSymbol("fanout".try_into()?)),
                val: ScVal::U32(fanout),
            },
            stellar_xdr::ScMapEntry {
                key: ScVal::Symbol(ScSymbol("signer".try_into()?)),
                val: ScVal::Address(signer.clone()),
            },
        ]
        .try_into()?,
    ))))
}

/// `root.node([child], [{peer, n}, {peer, 0}])`: the root requires peer 0 and calls the child
/// `n` times, each requiring peer 0 again, so peer 0's one auth entry matches a root with `n`
/// sub-invocations. The tree is recorded by simulation for at most [`SIMULATED_FANOUT`]
/// children and widened by hand past it, with the transaction's maximum instructions.
fn fanout(w: &mut World, s: &Subject, n: u64) -> Result<Built> {
    let c = w.contracts()?;
    let (root, child) = (c.auth_tree.clone(), c.auth_tree_child.clone());
    let peer = shapekit_core_driver::keys::account_address(&s.peers[0].verifying_key());
    let hf_for = |k: u64| -> Result<HostFunction> {
        call(
            &root,
            "node",
            vec![
                ScVal::Vec(Some(ScVec(
                    vec![ScVal::Address(ScAddress::Contract(child.clone()))].try_into()?,
                ))),
                ScVal::Vec(Some(ScVec(
                    vec![level(&peer, u32::try_from(k)?)?, level(&peer, 0)?].try_into()?,
                ))),
            ],
        )
    };
    let m = n.min(SIMULATED_FANOUT);
    let source = account_id(&s.source.verifying_key());
    let node = w.driver.node();
    let sim = shapekit_core_driver::sim_legacy::AnySimulator::new(
        node.snapshot()?,
        &node.ledger_context()?,
    )?;
    let rec = sim.simulate_invoke(
        hf_for(m)?,
        shapekit_core_driver::sim::AuthMode::Record {
            allow_non_root: false,
            address_v2: false,
        },
        &source,
    )?;
    let [mut entry] = <[SorobanAuthorizationEntry; 1]>::try_from(rec.auth)
        .map_err(|a| anyhow::anyhow!("expected one auth entry, got {}", a.len()))?;
    let hf = hf_for(n)?;
    let HostFunction::InvokeContract(args) = &hf else {
        unreachable!("hf_for builds a contract call")
    };
    let first = entry
        .root_invocation
        .sub_invocations
        .first()
        .cloned()
        .context("the recorded root has no child")?;
    entry.root_invocation = SorobanAuthorizedInvocation {
        function: SorobanAuthorizedFunction::ContractFn(args.clone()),
        sub_invocations: vec![first; n as usize].try_into()?,
    };
    let entry = shapekit_core_driver::tx::sign_authorization_entry(
        entry,
        w.driver.network_id(),
        w.driver.last_closed() + 100,
        &[&s.peers[0]],
    )?;
    let mut d = rec.transaction_data;
    d.resources.instructions =
        u32::try_from(setting(w, "contract_compute_v0.tx_max_instructions")?)?;
    d.resources.disk_read_bytes = u32::try_from(setting(
        w,
        "contract_ledger_cost_v0.tx_max_disk_read_bytes",
    )?)?;
    d.resources.write_bytes =
        u32::try_from(setting(w, "contract_ledger_cost_v0.tx_max_write_bytes")?)?;
    d.resource_fee = 2_000_000_000;
    soroban(w, s, hf, d, vec![entry])
}

/// The widest fan-out recorded by simulation; wider trees copy its child.
const SIMULATED_FANOUT: u64 = 300;

/// Every limit the probe measures.
pub fn probes() -> Vec<LimitProbe> {
    let p = |row, path, measure, lo, hi, build| LimitProbe {
        row,
        path,
        measure,
        world: WorldKind::Current,
        lo,
        hi,
        build,
        settings: &[],
    };
    vec![
        p(
            "tx.memo_text.len",
            "Transaction.memo",
            "bytes of a text memo",
            28,
            29,
            |w, t, s, n| xdr_bounded(w, t, s, n, 28, "/tx/tx/memo/text", memo_text),
        ),
        p(
            "tx.signatures",
            "TransactionV1Envelope.signatures",
            "signatures, each of a signer of the source",
            20,
            21,
            |w, t, s, n| xdr_bounded(w, t, s, n, 20, "/tx/signatures", signatures),
        ),
        p(
            "tx.extra_signers",
            "PreconditionsV2.extraSigners",
            "extra signers, each signing",
            2,
            3,
            |w, t, s, n| xdr_bounded(w, t, s, n, 2, "/tx/tx/cond/v2/extra_signers", extra_signers),
        ),
        p(
            "tx.operations",
            "Transaction.operations",
            "payments in one transaction",
            100,
            101,
            |w, t, s, n| xdr_bounded(w, t, s, n, 100, "/tx/tx/operations", operations),
        ),
        p(
            "path.len",
            "PathPaymentStrictReceiveOp.path",
            "native hops of a native path payment",
            5,
            6,
            |w, t, s, n| xdr_bounded(w, t, s, n, 5, PP, path),
        ),
        p(
            "claimants.per_balance",
            "CreateClaimableBalanceOp.claimants",
            "claimants of one balance",
            10,
            11,
            |w, t, s, n| xdr_bounded(w, t, s, n, 10, CB, claimants),
        ),
        p(
            "signer.signed_payload.len",
            "SignerKeyEd25519SignedPayload.payload",
            "bytes of an extra signer's signed payload",
            64,
            65,
            |w, t, s, n| {
                if n <= 64 {
                    return Ok(Built::Env(signed_payload(w, s, n)?));
                }
                let b = signed_payload(w, s, 64)?;
                let bytes = signed_bytes(w, &b)?;
                // the signer key is one P strkey in XDR-JSON: the payload follows its arm word and
                // the 32 key bytes
                let (off, _, v) = t.envelope_offset(&bytes, "/tx/tx/cond/v2/extra_signers/0")?;
                let IValue::Union {
                    arm: Some(inner), ..
                } = v
                else {
                    bail!("the extra signer is no union value");
                };
                let IValue::Struct(fields) = *inner else {
                    bail!("the signed payload is no struct");
                };
                let ty = shapekit_schema::Type::OpaqueVar { max_size: Some(64) };
                Ok(Built::Raw(grow_at(
                    t,
                    &bytes,
                    off + 4 + 32,
                    &ty,
                    &fields[1],
                    0x5a,
                )?))
            },
        ),
        p(
            "scval.symbol.len",
            "InvokeContractArgs.args",
            "bytes of a symbol echo receives",
            32,
            33,
            |w, t, s, n| xdr_bounded(w, t, s, n, 32, ARG0, symbol_arg),
        ),
        p(
            "claim_predicate.depth",
            "ClaimantV0.predicate",
            "levels of a claimant's predicate",
            1,
            8,
            |w, _, s, n| {
                let mut pred = ClaimPredicate::Unconditional;
                for _ in 1..n {
                    pred = ClaimPredicate::Not(Some(Box::new(pred)));
                }
                classic(
                    w,
                    s,
                    vec![OperationBody::CreateClaimableBalance(
                        CreateClaimableBalanceOp {
                            asset: Asset::Native,
                            amount: XLM,
                            claimants: vec![Claimant::ClaimantTypeV0(ClaimantV0 {
                                destination: account_id(&s.peers[0].verifying_key()),
                                predicate: pred,
                            })]
                            .try_into()?,
                        },
                    )],
                    &[&s.source],
                )
            },
        ),
        p(
            "account.signers",
            "SetOptionsOp.signer",
            "signers of the source after the operation",
            1,
            21,
            |w, _, s, n| {
                let keys = fresh_keys(s, n);
                add_signers(w, s, &keys[..n as usize - 1])?;
                classic(
                    w,
                    s,
                    vec![OperationBody::SetOptions(SetOptionsOp {
                        signer: Some(Signer {
                            key: ed25519_signer(&keys[n as usize - 1].verifying_key()),
                            weight: 1,
                        }),
                        ..Default::default()
                    })],
                    &[&s.source],
                )
            },
        ),
        p(
            "account.subentries",
            "ManageDataOp.dataName",
            "subentries of the source after the operation",
            1,
            1001,
            |w, _, s, n| {
                let name = |i: u64| -> Result<stellar_xdr::String64> {
                    Ok(stellar_xdr::String64(format!("d{i}").try_into()?))
                };
                let mut i = 0;
                while i + 1 < n {
                    let end = (i + 100).min(n - 1);
                    let ops = (i..end)
                        .map(|k| {
                            Ok(OperationBody::ManageData(ManageDataOp {
                                data_name: name(k)?,
                                data_value: Some(vec![1u8].try_into()?),
                            }))
                        })
                        .collect::<Result<Vec<_>>>()?;
                    let src = s.source.clone();
                    w.classic_ok(&src, ops, &[])?;
                    i = end;
                }
                classic(
                    w,
                    s,
                    vec![OperationBody::ManageData(ManageDataOp {
                        data_name: name(n)?,
                        data_value: Some(vec![1u8].try_into()?),
                    })],
                    &[&s.source],
                )
            },
        ),
        LimitProbe {
            settings: &["contract_ledger_cost_ext_v0.tx_max_footprint_entries"],
            ..p(
                "soroban.footprint.entries",
                "LedgerFootprint.readOnly",
                "footprint keys of echo(7), read-only keys added",
                3,
                1000,
                |w, _, s, n| {
                    let e = echo(w)?;
                    let extra = (3..n)
                        .map(|i| Ok(data_key(&e, ScVal::U32(i as u32 + 1000))))
                        .collect::<Result<Vec<_>>>()?;
                    echo_with_keys(w, s, extra, false)
                },
            )
        },
        LimitProbe {
            settings: &["contract_ledger_cost_v0.tx_max_write_ledger_entries"],
            ..p(
                "soroban.footprint.write_entries",
                "LedgerFootprint.readWrite",
                "read-write keys of echo(7), keys added",
                1,
                400,
                |w, _, s, n| {
                    let e = echo(w)?;
                    let extra = (1..n)
                        .map(|i| Ok(data_key(&e, ScVal::U32(i as u32 + 1000))))
                        .collect::<Result<Vec<_>>>()?;
                    echo_with_keys(w, s, extra, true)
                },
            )
        },
        LimitProbe {
            settings: &["contract_data_key_size_bytes"],
            ..p(
                "soroban.contract_data.key_bytes",
                "LedgerKeyContractData.key",
                "bytes of an ScBytes key in a read-only footprint key",
                0,
                400,
                |w, _, s, n| {
                    let e = echo(w)?;
                    echo_with_keys(w, s, vec![data_key(&e, bytes(n)?)], false)
                },
            )
        },
        LimitProbe {
            settings: &["contract_data_entry_size_bytes"],
            ..p(
                "soroban.contract_data.entry_bytes",
                "InvokeContractArgs.args",
                "bytes echo stores in one entry",
                1,
                70000,
                |w, _, s, n| {
                    let hf = call(&echo(w)?, "echo", vec![bytes(n)?])?;
                    let d = max_resources(w, s)?;
                    soroban(w, s, hf, d, Vec::new())
                },
            )
        },
        LimitProbe {
            settings: &["contract_events_v0.tx_max_contract_events_size_bytes"],
            ..p(
                "events.contract.bytes_per_tx",
                "InvokeContractArgs.args",
                "bytes of the data of the one event emit sends",
                1,
                20000,
                |w, _, s, n| {
                    let hf = call(
                        &echo(w)?,
                        "emit",
                        vec![ScVal::Vec(Some(ScVec(Default::default()))), bytes(n)?],
                    )?;
                    let d = max_resources(w, s)?;
                    soroban(w, s, hf, d, Vec::new())
                },
            )
        },
        LimitProbe {
            settings: &["contract_max_size_bytes"],
            ..p(
                "soroban.contract_code.bytes",
                "InvokeHostFunctionOp.hostFunction",
                "bytes of an uploaded wasm module",
                5000,
                140000,
                |w, _, s, n| {
                    let hf = HostFunction::UploadContractWasm(padded_wasm(n)?.try_into()?);
                    let mut d = max_resources(w, s)?;
                    d.resources.footprint = Default::default();
                    let hash = stellar_xdr::Hash(
                        <sha2::Sha256 as sha2::Digest>::digest(padded_wasm(n)?).into(),
                    );
                    d.resources.footprint.read_write = vec![LedgerKey::ContractCode(
                        stellar_xdr::LedgerKeyContractCode { hash },
                    )]
                    .try_into()?;
                    soroban(w, s, hf, d, Vec::new())
                },
            )
        },
        LimitProbe {
            settings: &["contract_bandwidth_v0.tx_max_size_bytes"],
            ..p(
                "xdr.envelope.len",
                "SorobanAuthorizationEntry.rootInvocation",
                "bytes of an argument in a declared, unmatched auth entry",
                1,
                140000,
                |w, _, s, n| declared_bytes(w, s, n),
            )
        },
        LimitProbe {
            settings: &["contract_compute_v0.tx_memory_limit"],
            ..p(
                "auth.invocation.nodes",
                "SorobanAuthorizedInvocation.subInvocations",
                "matched sub-invocations under one root",
                1,
                400,
                |w, _, s, n| fanout(w, s, n),
            )
        },
        p(
            "scval.depth",
            "InvokeContractArgs.args",
            "nesting of a vec echo receives",
            1,
            300,
            |w, _, s, n| {
                let hf = call(&echo(w)?, "echo", vec![nested(n)?])?;
                let d = max_resources(w, s)?;
                soroban(w, s, hf, d, Vec::new())
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padded_wasm_has_the_asked_length_and_parses() {
        for n in [5000u64, 5001, 5127, 5128, 20000, 131072] {
            let w = padded_wasm(n).unwrap();
            assert_eq!(w.len() as u64, n);
            assert!(w.starts_with(b"\0asm"));
        }
    }
}

/// How a measured limit answers a row of `registry/caps-limits.toml`.
pub enum Check {
    /// The row's recommended value is the largest value core accepted.
    Equal,
    /// The row counts encoded bytes: the accepted value's size is within the row's value and
    /// the refused value's is past it. The function gives both sizes.
    Size(fn(&Measured) -> Result<(u64, u64)>),
    /// The row bounds a wider quantity than the probe measures, so the measured limit lies
    /// below it; the reading says why, from the source.
    Below(&'static str),
    /// No single transaction measures the row; the reading says what bounds it instead.
    Unmeasured(&'static str),
}

fn envelope_sizes(m: &Measured) -> Result<(u64, u64)> {
    Ok((
        m.accepted.envelope.len() as u64,
        m.refused.envelope.len() as u64,
    ))
}

fn key_sizes(m: &Measured) -> Result<(u64, u64)> {
    let size = |n: u64| -> Result<u64> {
        let k = data_key(&ContractId(stellar_xdr::Hash([0; 32])), bytes(n)?);
        Ok(k.to_xdr(Limits::none())?.len() as u64)
    };
    let a: u64 = m.limit.max_accepted.parse()?;
    Ok((size(a)?, size(a + 1)?))
}

fn entry_sizes(m: &Measured) -> Result<(u64, u64)> {
    let size = |n: u64| -> Result<u64> {
        let e = stellar_xdr::LedgerEntry {
            last_modified_ledger_seq: 0,
            data: stellar_xdr::LedgerEntryData::ContractData(stellar_xdr::ContractDataEntry {
                ext: stellar_xdr::ExtensionPoint::V0,
                contract: ScAddress::Contract(ContractId(stellar_xdr::Hash([0; 32]))),
                key: ScVal::Symbol(ScSymbol("last".try_into()?)),
                durability: ContractDataDurability::Persistent,
                val: bytes(n)?,
            }),
            ext: stellar_xdr::LedgerEntryExt::V0,
        };
        Ok(e.to_xdr(Limits::none())?.len() as u64)
    };
    let a: u64 = m.limit.max_accepted.parse()?;
    Ok((size(a)?, size(a + 1)?))
}

fn event_sizes(m: &Measured) -> Result<(u64, u64)> {
    // emit returns void, 4 bytes, which core adds to the events' size
    let size = |n: u64| -> Result<u64> {
        let e = stellar_xdr::ContractEvent {
            ext: stellar_xdr::ExtensionPoint::V0,
            contract_id: Some(ContractId(stellar_xdr::Hash([0; 32]))),
            type_: stellar_xdr::ContractEventType::Contract,
            body: stellar_xdr::ContractEventBody::V0(stellar_xdr::ContractEventV0 {
                topics: Default::default(),
                data: bytes(n)?,
            }),
        };
        Ok(e.to_xdr(Limits::none())?.len() as u64 + 4)
    };
    let a: u64 = m.limit.max_accepted.parse()?;
    Ok((size(a)?, size(a + 1)?))
}

/// The answer to every row of caps-limits.toml.
pub fn checks() -> Vec<(&'static str, Check)> {
    use Check::*;
    vec![
        (
            "xdr.envelope.depth",
            Unmeasured(
                "an envelope deeper than the bound is refused at submission (validateXDRForProtocol, stellar-core v30.0.0 src/transactions/TransactionUtils.cpp 1349-1380) but every envelope that deep still fails at apply, so no depth is applied successfully at the bound: the host decodes declared auth only to depth 500 (soroban-env-host 30.0.0 DEFAULT_XDR_RW_LIMITS). The submission bound is witnessed by the corpus entries of soroban.auth-trees, 492 declared levels included and 493 refused tx_malformed",
            ),
        ),
        ("xdr.envelope.len", Size(envelope_sizes)),
        (
            "xdr.meta.depth",
            Unmeasured(
                "meta is written by core and never submitted; the row adds the measured 12 levels of a LedgerCloseMeta V2 above an envelope to the envelope bound",
            ),
        ),
        (
            "xdr.meta.len",
            Unmeasured(
                "no network setting bounds a whole LedgerCloseMeta and it is never submitted; the row takes the host's last-resort XDR length (soroban-env-host 30.0.0)",
            ),
        ),
        ("scval.depth", Equal),
        (
            "scval.depth.envelope",
            Unmeasured(
                "values that deep are included but fail at apply, because the host converts arguments only to DEFAULT_HOST_DEPTH_LIMIT = 100 (measured as scval.depth); the row's bound is the envelope decode depth, which the corpus entries of soroban.scval-shapes witness",
            ),
        ),
        (
            "scval.vec.len",
            Unmeasured(
                "derived from the envelope size (measured as xdr.envelope.len) divided by the 4 byte size of the smallest element",
            ),
        ),
        (
            "scval.map.len",
            Unmeasured(
                "derived from the envelope size (measured as xdr.envelope.len) divided by the 8 byte size of the smallest entry",
            ),
        ),
        (
            "scval.bytes.len",
            Unmeasured(
                "derived from the envelope size, measured as xdr.envelope.len with an ScBytes value as the growing part",
            ),
        ),
        ("scval.symbol.len", Equal),
        (
            "auth.invocation.depth",
            Unmeasured(
                "the declared chain is bounded by the envelope depth check at submission (see xdr.envelope.depth); deeper than the host's decode depth the transaction is included and fails, so no applied chain reaches the row's 493",
            ),
        ),
        (
            "auth.invocation.nodes",
            Below(
                "the row bounds declared nodes by the envelope size; matched nodes are bounded lower, by the transaction memory limit (contract_compute_v0.tx_memory_limit), because each matched call instantiates the contract again (soroban-env-host 30.0.0 budget); declared nodes the host never matches cost no instantiation",
            ),
        ),
        ("events.contract.bytes_per_tx", Size(event_sizes)),
        (
            "events.contract.per_tx",
            Unmeasured(
                "derived from events.contract.bytes_per_tx (measured) divided by the 24 byte smallest event; the test contracts emit one event per call",
            ),
        ),
        (
            "events.contract.per_ledger",
            Unmeasured(
                "a ledger-wide product of two settings (ledgerMaxTxCount and the per-transaction events bound); no single transaction measures it",
            ),
        ),
        (
            "events.contract.bytes_per_ledger",
            Unmeasured("a ledger-wide product of two settings; no single transaction measures it"),
        ),
        ("soroban.footprint.entries", Equal),
        ("soroban.footprint.write_entries", Equal),
        ("soroban.contract_data.entry_bytes", Size(entry_sizes)),
        ("soroban.contract_data.key_bytes", Size(key_sizes)),
        ("soroban.contract_code.bytes", Equal),
        (
            "soroban.txs.per_ledger",
            Unmeasured(
                "a ledger-wide setting (ledgerMaxTxCount); the probe closes one transaction per ledger",
            ),
        ),
        (
            "state_changes.per_tx.query",
            Unmeasured("an indexer's query-time bound in wallet-backend, not a core limit"),
        ),
        ("claim_predicate.depth", Equal),
        ("claimants.per_balance", Equal),
        ("path.len", Equal),
        (
            "offers.crossed.per_op",
            Unmeasured(
                "crossing 1001 offers needs an order book of 1001 offers from several accounts (each account holds at most 1000 subentries); the bound is MAX_OFFERS_TO_CROSS = 1000 in stellar-core v30.0.0 src/transactions/TransactionUtils.cpp 37, not measured by this probe",
            ),
        ),
        ("tx.operations", Equal),
        ("tx.signatures", Equal),
        ("tx.extra_signers", Equal),
        ("tx.memo_text.len", Equal),
        ("signer.signed_payload.len", Equal),
        ("account.signers", Equal),
        ("account.subentries", Equal),
        (
            "ledger.upgrades",
            Unmeasured(
                "StellarValue.upgrades is written by validators in the ledger header, never by a transaction",
            ),
        ),
    ]
}
