//! The attempt loop: for one field, a base attempt and one attempt per replacement value,
//! each on its own funded accounts, each closed in its own ledger.

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use shapekit_contracts::validation::{
    ApplyStatus, FieldVerdict, ProbeAttempt, SubmitStatus, Verdict,
};
use shapekit_core_driver::chain::ChainError;
use shapekit_core_driver::node::SubmitStatus as NodeStatus;
use shapekit_core_driver::tx::envelope_hash;
use shapekit_schema::Definition;
use stellar_xdr::{LedgerCloseMeta, Limits, ReadXdr, TransactionEnvelope, WriteXdr};

use crate::carriers::{Base, BaseOutcome, Carrier, instance};
use crate::edit::{Typer, key};
use crate::fields::{Field, STRING_SHAPED};
use crate::result_code;
use crate::sign::sign;
use crate::world::{Subject, World};

/// The ledger an attempt was applied in: its sequence, its meta and the transaction hash.
pub type AppliedIn = (u32, Box<LedgerCloseMeta>, [u8; 32]);

/// What one attempt left behind for its witness entries.
pub struct Evidence {
    /// `<struct>-<field>-<n>`, lowercase.
    pub slug: String,
    pub field: String,
    pub variant: String,
    pub protocol: u32,
    /// The chain the attempt ran on: `p30`, `p12` or `p30-archival`.
    pub chain: &'static str,
    pub envelope: Vec<u8>,
    pub attempt: ProbeAttempt,
    /// The ledger the attempt was applied in: sequence, meta and the transaction hash.
    pub ledger: Option<AppliedIn>,
}

pub struct FieldRun {
    pub verdict: FieldVerdict,
    pub evidence: Vec<Evidence>,
}

/// The JSON keys of an object struct, `None` for a struct XDR-JSON writes as a string.
fn object_keys(typer: &Typer, structure: &str) -> Result<Option<Vec<String>>> {
    if STRING_SHAPED.contains(&structure) {
        return Ok(None);
    }
    match typer.schema().get(structure) {
        Some(Definition::Struct(s)) => Ok(Some(s.fields.iter().map(|f| key(&f.name)).collect())),
        _ => bail!("{structure} is not a struct"),
    }
}

/// A signed base and where its field sits.
struct Prepared {
    json: Value,
    /// The struct instance.
    at: String,
    /// The field's current value.
    value: Value,
    base: Base,
}

fn prepare(
    w: &mut World,
    typer: &Typer,
    carrier: &Carrier,
    field: &Field,
    s: &Subject,
) -> Result<Prepared> {
    let base = (carrier.build)(w, s)
        .with_context(|| format!("building the {} carrier", carrier.structure))?;
    let mut json = serde_json::to_value(&base.env)?;
    sign(&mut json, &base.plan, &w.network_id(), None)?;
    let keys = object_keys(typer, carrier.structure)?;
    let (at, value) = match &keys {
        Some(keys) => {
            let at = instance(&json, &base, keys, carrier.structure)?;
            let v = json
                .pointer(&format!("{at}/{}", key(&field.name)))
                .with_context(|| format!("{}: no {} at {at}", carrier.structure, field.name))?
                .clone();
            (at, v)
        }
        None => {
            let at = base.at.clone().with_context(|| {
                format!(
                    "{} is string-shaped; its carrier names the pointer",
                    carrier.structure
                )
            })?;
            let view = typer.view(carrier.structure, crate::edit::get(&json, &at)?)?;
            let v = view
                .get(&field.name)
                .with_context(|| format!("{} view has no {}", carrier.structure, field.name))?
                .clone();
            (at, v)
        }
    };
    Ok(Prepared {
        json,
        at,
        value,
        base,
    })
}

/// Writes `value` into the field; returns the pointer the edit wrote at.
fn write(
    typer: &Typer,
    carrier: &Carrier,
    field: &Field,
    p: &mut Prepared,
    value: Value,
) -> Result<String> {
    if STRING_SHAPED.contains(&carrier.structure) {
        let mut view = typer.view(carrier.structure, crate::edit::get(&p.json, &p.at)?)?;
        view.insert(field.name.clone(), value);
        let s = typer.unview(carrier.structure, &view)?;
        crate::edit::set(&mut p.json, &p.at, s)?;
        Ok(p.at.clone())
    } else {
        let at = format!("{}/{}", p.at, key(&field.name));
        crate::edit::set(&mut p.json, &at, value)?;
        Ok(at)
    }
}

fn short(v: &Value) -> String {
    let s = v.to_string();
    if s.chars().count() > 72 {
        let head: String = s.chars().take(69).collect();
        format!("{head}...")
    } else {
        s
    }
}

/// Submits `env` and closes a ledger when core queued it.
pub(crate) fn submit(
    w: &mut World,
    env: &TransactionEnvelope,
    variant: &str,
) -> Result<(ProbeAttempt, Option<AppliedIn>)> {
    match w.apply(env) {
        Ok(applied) => {
            let meta = Box::new(w.driver.last_meta().clone());
            let hash = envelope_hash(env, &w.network_id())?;
            Ok((
                ProbeAttempt {
                    variant: variant.to_owned(),
                    submission: SubmitStatus::Pending,
                    result_code: Some(result_code(&applied.result)?),
                    result_xdr: Some(applied.result.to_xdr_base64(Limits::none())?),
                    applied: if applied.succeeded() {
                        ApplyStatus::Succeeded
                    } else {
                        ApplyStatus::Failed
                    },
                    witnesses: Vec::new(),
                    carrier: None,
                },
                Some((applied.ledger, meta, hash)),
            ))
        }
        Err(ChainError::NotQueued { status, result }) => Ok((
            ProbeAttempt {
                variant: variant.to_owned(),
                submission: match status {
                    NodeStatus::Pending => SubmitStatus::Pending,
                    NodeStatus::Duplicate => SubmitStatus::Duplicate,
                    NodeStatus::Error => SubmitStatus::Error,
                    NodeStatus::TryAgainLater => SubmitStatus::TryAgainLater,
                    NodeStatus::Filtered => SubmitStatus::Filtered,
                },
                result_code: result.as_deref().map(result_code).transpose()?,
                result_xdr: result
                    .as_deref()
                    .map(|r| r.to_xdr_base64(Limits::none()))
                    .transpose()?,
                applied: ApplyStatus::NotApplied,
                witnesses: Vec::new(),
                carrier: None,
            },
            None,
        )),
        Err(e) => Err(e).with_context(|| format!("attempt {variant:?}")),
    }
}

fn slug(field: &Field, carrier: &Carrier, n: usize) -> String {
    let mut s = format!(
        "{}-{}",
        field.structure.to_ascii_lowercase(),
        field.name.to_ascii_lowercase()
    );
    if !carrier.id.is_empty() {
        s.push('-');
        s.push_str(carrier.id);
    }
    format!("{s}-{n}").replace('_', "-")
}

fn subject_tag(field: &Field, carrier: &Carrier, n: usize) -> String {
    if carrier.id.is_empty() {
        format!("{}#{n}", field.path)
    } else {
        format!("{}@{}#{n}", field.path, carrier.id)
    }
}

/// The verdict the attempts support.
pub fn verdict(attempts: &[ProbeAttempt]) -> Verdict {
    let mut outcomes: Vec<_> = attempts.iter().map(ProbeAttempt::outcome).collect();
    outcomes.sort();
    outcomes.dedup();
    if outcomes.len() > 1 {
        Verdict::Validated
    } else {
        Verdict::NotValidated
    }
}

/// Core's answer to a rebuilt Soroban variant must be the host's: applied, it succeeds
/// exactly when the host ran the call with the variant's own footprint and auth, and it
/// fails as INVOKE_HOST_FUNCTION_TRAPPED, the host refusing the call, and not for a resource
/// or a fee. A refusal at submission is core's validation and needs no agreement. Anything
/// else means the failure cannot be put on the edited value, so the run stops.
pub(crate) fn agrees_with_host(a: &ProbeAttempt, run: &crate::soroban::HostRun) -> Result<()> {
    match (a.applied, &run.error) {
        (ApplyStatus::NotApplied, _) => Ok(()),
        (ApplyStatus::Succeeded, None) => Ok(()),
        (ApplyStatus::Succeeded, Some(e)) => {
            bail!("core applied it successfully, the host failed it with {e}")
        }
        (ApplyStatus::Failed, None) => bail!(
            "core failed it ({:?}), the host ran it with the same footprint and auth",
            a.result_xdr
        ),
        (ApplyStatus::Failed, Some(_)) => {
            let xdr = a
                .result_xdr
                .as_deref()
                .context("an applied attempt has a result")?;
            let r = stellar_xdr::TransactionResult::from_xdr_base64(xdr, Limits::none())?;
            ensure!(
                trapped(&r),
                "core failed it with {:?}, not as the host refusing the call",
                r.result
            );
            Ok(())
        }
    }
}

/// Whether a one-operation transaction failed as INVOKE_HOST_FUNCTION_TRAPPED.
fn trapped(r: &stellar_xdr::TransactionResult) -> bool {
    use stellar_xdr::{
        InvokeHostFunctionResult, OperationResult, OperationResultTr, TransactionResultResult,
    };
    let TransactionResultResult::TxFailed(ops) = &r.result else {
        return false;
    };
    matches!(
        ops.as_slice(),
        [OperationResult::OpInner(
            OperationResultTr::InvokeHostFunction(InvokeHostFunctionResult::Trapped)
        )]
    )
}

/// Every attempt for one field.
pub fn probe_field(
    w: &mut World,
    typer: &Typer,
    carrier: &Carrier,
    field: &Field,
) -> Result<FieldRun> {
    let subject = |n: usize| Subject::new(subject_tag(field, carrier, n));
    let first = subject(0);
    w.fund(&first.funded())?;
    let p0 = prepare(w, typer, carrier, field, &first)?;
    let mut reps = typer.replacements(&field.ty, &p0.value)?;
    reps.extend((carrier.extra)(
        w,
        &first,
        &field.name,
        &p0.value,
        &p0.json,
    )?);
    if reps.is_empty() {
        return single_valued(w, typer, carrier, field, p0);
    }
    let others: Vec<Subject> = (1..=reps.len()).map(subject).collect();
    let keys: Vec<_> = others.iter().flat_map(Subject::funded).collect();
    w.fund(&keys)?;

    let mut evidence = Vec::new();
    let base_name = format!("base, {}: {}", carrier.context, short(&p0.value));
    let env: TransactionEnvelope = serde_json::from_value(p0.json.clone())?;
    let (a, ledger) = submit(w, &env, &base_name)?;
    let expected = match carrier.outcome {
        BaseOutcome::Succeeds => a.applied == ApplyStatus::Succeeded,
        BaseOutcome::Refused(_) => a.applied == ApplyStatus::NotApplied,
        BaseOutcome::Fails(_) => a.applied == ApplyStatus::Failed,
    };
    ensure!(
        expected,
        "{}: the base attempt ended {:?} {:?} {:?}, the carrier expects {:?}",
        field.path,
        a.submission,
        a.result_code,
        a.result_xdr,
        carrier.outcome
    );
    evidence.push(Evidence {
        slug: slug(field, carrier, 0),
        field: field.path.clone(),
        variant: base_name,
        protocol: carrier.world.protocol(),
        chain: carrier.world.chain(),
        envelope: env.to_xdr(Limits::none())?,
        attempt: a,
        ledger,
    });

    for (i, s) in others.iter().enumerate() {
        let mut p = prepare(w, typer, carrier, field, s)?;
        let mut mine = typer.replacements(&field.ty, &p.value)?;
        mine.extend((carrier.extra)(w, s, &field.name, &p.value, &p.json)?);
        ensure!(
            mine.len() == reps.len(),
            "{}: attempt {} has {} replacements, the first base had {}",
            field.path,
            i + 1,
            mine.len(),
            reps.len()
        );
        let rep = mine[i].clone();
        if let Some(before) = rep.before {
            // the state the value is judged against is closed before the variant is built, so
            // the rebuild simulates against the chain core applies the variant on
            before(w, s, &rep.value).with_context(|| {
                format!(
                    "{} attempt {:?}: the ledger before it",
                    field.path, rep.name
                )
            })?;
        }
        let raw = match &rep.xdr {
            Some(x) => Some(x.clone()),
            None => string_shaped_raw(typer, carrier, field, &p, &rep.value)?,
        };
        let name = match &raw {
            None => format!("{}: {}", rep.name, short(&rep.value)),
            Some(_) => format!(
                "{}: {}, written as bytes, XDR-JSON cannot hold it",
                rep.name,
                short(&rep.value)
            ),
        };
        let mut plan = p.base.plan.clone();
        let mut host = None;
        let env = match &raw {
            Some(target) => {
                let at = format!("{}/{}", p.at, key(&field.name));
                ensure!(
                    crate::soroban::rebuild_scope(&at).is_none(),
                    "{}: a value written as bytes inside a soroban invocation cannot be rebuilt",
                    field.path
                );
                spliced(w, typer, carrier, field, &p, target)?
            }
            None => {
                let edit_at = write(typer, carrier, field, &mut p, rep.value)?;
                match crate::soroban::rebuild_scope(&edit_at) {
                    // the base's footprint and auth fit the base's call, not this one: the
                    // variant gets its own, so a failure is the edited value's and not a read
                    // outside the footprint or an authorization for another call
                    Some(scope) => {
                        let run = crate::soroban::rebuild(
                            w,
                            s,
                            &mut p.json,
                            &mut plan,
                            &edit_at,
                            scope,
                            &p.base.env,
                        )
                        .with_context(|| format!("{} attempt {name:?}", field.path))?;
                        let env: TransactionEnvelope = serde_json::from_value(p.json.clone())?;
                        host = Some(run);
                        env
                    }
                    None => sign(&mut p.json, &plan, &w.network_id(), Some(&edit_at))
                        .with_context(|| format!("{} attempt {name:?}", field.path))?,
                }
            }
        };
        let name = match &host {
            Some(run) => format!("{name}; {}", run.note()),
            None => name,
        };
        let (a, ledger) = submit(w, &env, &name)?;
        if let Some(run) = &host {
            agrees_with_host(&a, run)
                .with_context(|| format!("{} attempt {name:?}", field.path))?;
        }
        evidence.push(Evidence {
            slug: slug(field, carrier, i + 1),
            field: field.path.clone(),
            variant: name,
            protocol: carrier.world.protocol(),
            chain: carrier.world.chain(),
            envelope: env.to_xdr(Limits::none())?,
            attempt: a,
            ledger,
        });
    }
    if !carrier.id.is_empty() {
        for e in &mut evidence {
            e.attempt.carrier = Some(carrier.id.to_owned());
        }
    }
    let attempts: Vec<ProbeAttempt> = evidence.iter().map(|e| e.attempt.clone()).collect();
    Ok(FieldRun {
        verdict: FieldVerdict {
            path: field.path.clone(),
            verdict: verdict(&attempts),
            attempts,
        },
        evidence,
    })
}

/// A field whose type holds exactly one value, a union with only its void case 0. The only
/// other attempt is the same envelope with the field's discriminant outside the type, which
/// no XDR-JSON can write, so the bytes are changed directly.
fn single_valued(
    w: &mut World,
    typer: &Typer,
    carrier: &Carrier,
    field: &Field,
    p0: Prepared,
) -> Result<FieldRun> {
    let ty = typer.schema().resolve(&field.ty).clone();
    let shapekit_schema::Type::Ref { name } = &ty else {
        bail!("{}: a single-valued field that is no union", field.path);
    };
    let Some(Definition::Union(u)) = typer.schema().get(name) else {
        bail!("{}: a single-valued field that is no union", field.path);
    };
    ensure!(
        u.arms.len() == 1 && u.arms[0].ty.is_none(),
        "{}: {name} is not a union of one void case",
        field.path
    );
    let outside = u.case_values().max().unwrap_or(0) + 1;

    let mut evidence = Vec::new();
    let base_name = format!("base, {}: {}", carrier.context, short(&p0.value));
    let env: TransactionEnvelope = serde_json::from_value(p0.json.clone())?;
    let (a, ledger) = submit(w, &env, &base_name)?;
    ensure!(
        a.applied == ApplyStatus::Succeeded,
        "{}: the base attempt ended {:?} {:?}",
        field.path,
        a.submission,
        a.result_code
    );
    evidence.push(Evidence {
        slug: slug(field, carrier, 0),
        field: field.path.clone(),
        variant: base_name,
        protocol: carrier.world.protocol(),
        chain: carrier.world.chain(),
        envelope: env.to_xdr(Limits::none())?,
        attempt: a,
        ledger,
    });

    let s = Subject::new(subject_tag(field, carrier, 1));
    w.fund(&s.funded())?;
    let p = prepare(w, typer, carrier, field, &s)?;
    let env: TransactionEnvelope = serde_json::from_value(p.json.clone())?;
    let mut bytes = env.to_xdr(Limits::none())?;
    let offset = field_offset(typer, carrier, field, &p, &bytes)?;
    ensure!(
        bytes[offset..offset + 4] == [0, 0, 0, 0],
        "{}: the field's bytes at {offset} are not discriminant 0",
        field.path
    );
    bytes[offset..offset + 4].copy_from_slice(&(outside as u32).to_be_bytes());
    let name =
        format!("discriminant {outside}, outside {name}: bytes edited, no XDR-JSON holds it");
    let attempt = submit_raw(w, &bytes, &name)?;
    evidence.push(Evidence {
        slug: slug(field, carrier, 1),
        field: field.path.clone(),
        variant: name,
        protocol: carrier.world.protocol(),
        chain: carrier.world.chain(),
        envelope: bytes,
        attempt,
        ledger: None,
    });
    if !carrier.id.is_empty() {
        for e in &mut evidence {
            e.attempt.carrier = Some(carrier.id.to_owned());
        }
    }
    let attempts: Vec<ProbeAttempt> = evidence.iter().map(|e| e.attempt.clone()).collect();
    Ok(FieldRun {
        verdict: FieldVerdict {
            path: field.path.clone(),
            verdict: verdict(&attempts),
            attempts,
        },
        evidence,
    })
}

/// The byte offset of `field` inside the encoded envelope.
fn field_offset(
    typer: &Typer,
    carrier: &Carrier,
    field: &Field,
    p: &Prepared,
    env: &[u8],
) -> Result<usize> {
    if !STRING_SHAPED.contains(&carrier.structure) {
        return Ok(typer
            .envelope_offset(env, &format!("{}/{}", p.at, key(&field.name)))?
            .0);
    }
    // XDR-JSON writes a string-shaped struct in place of the union arm that holds it (a muxed
    // account is one M strkey), so the pointer can stop at the union
    let (mut start, ty, mut v) = typer.envelope_offset(env, &p.at)?;
    if let shapekit_schema::interp::Value::Union { arm: Some(x), .. } = v {
        start += 4;
        v = *x;
    } else if !matches!(typer.schema().resolve(&ty), shapekit_schema::Type::Ref { name } if name == carrier.structure)
    {
        bail!(
            "{}: {} does not point at a {}",
            field.path,
            p.at,
            carrier.structure
        );
    }
    let st = shapekit_schema::Type::Ref {
        name: carrier.structure.to_owned(),
    };
    Ok(start + typer.offset(&st, &v, &[key(&field.name).as_str()])?.0)
}

/// Submits bytes that may not decode. What xdrpp throws out of `xdr_from_opaque` in
/// `CommandHandler::tx` for bytes outside the type is `undecodable`: `xdr_bad_discriminant`
/// ("bad value of <field> in <type>") for a union or enum value, `xdr_overflow` ("xstring
/// overflow", "xvector overflow") for a length past the bound.
pub(crate) fn submit_raw(w: &mut World, bytes: &[u8], variant: &str) -> Result<ProbeAttempt> {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    match w.driver.node().submit_base64(&b64) {
        Err(shapekit_core_driver::node::NodeError::Rejected { message, .. }) => {
            ensure!(
                message.starts_with("bad value of ")
                    || message == "xstring overflow"
                    || message == "xvector overflow",
                "core's tx endpoint threw something other than a decode error: {message}"
            );
            Ok(ProbeAttempt {
                variant: variant.to_owned(),
                submission: SubmitStatus::Undecodable,
                result_code: None,
                result_xdr: None,
                applied: ApplyStatus::NotApplied,
                witnesses: Vec::new(),
                carrier: None,
            })
        }
        Err(e) => Err(e.into()),
        Ok(r) => bail!(
            "core decoded bytes outside the schema and answered {:?}",
            r.status
        ),
    }
}

/// The base envelope with the field's bytes replaced by `target`, signed again: for values
/// XDR-JSON cannot hold, which stellar-xdr still encodes.
fn spliced(
    w: &World,
    typer: &Typer,
    carrier: &Carrier,
    field: &Field,
    p: &Prepared,
    target: &[u8],
) -> Result<TransactionEnvelope> {
    let env: TransactionEnvelope = serde_json::from_value(p.json.clone())?;
    let mut bytes = env.to_xdr(Limits::none())?;
    let off = field_offset(typer, carrier, field, p, &bytes)?;
    let current = typer.value_of_type(&field.ty, &p.value)?;
    let interp = shapekit_schema::interp::Interp::new(typer.schema());
    let len = interp
        .encode_as(&field.ty, &current)
        .map_err(|e| anyhow::anyhow!("encoding {}: {e}", field.path))?
        .len();
    bytes.splice(off..off + len, target.iter().copied());
    let env = TransactionEnvelope::from_xdr(&bytes, Limits::none())
        .with_context(|| format!("{}: the spliced envelope does not decode", field.path))?;
    crate::sign::sign_typed(env, &p.base.plan, &w.network_id())
}

/// For a field of a string-shaped struct: the field's new XDR when XDR-JSON cannot write the
/// struct with it (a signed payload signer with an empty payload), `None` when it can.
fn string_shaped_raw(
    typer: &Typer,
    carrier: &Carrier,
    field: &Field,
    p: &Prepared,
    value: &Value,
) -> Result<Option<Vec<u8>>> {
    if !STRING_SHAPED.contains(&carrier.structure) {
        return Ok(None);
    }
    let mut view = typer.view(carrier.structure, crate::edit::get(&p.json, &p.at)?)?;
    view.insert(field.name.clone(), value.clone());
    let s = typer.unview(carrier.structure, &view)?;
    if typer.xdr_of(carrier.structure, &s).is_ok() {
        return Ok(None);
    }
    let v = typer.value_of_type(&field.ty, value)?;
    let interp = shapekit_schema::interp::Interp::new(typer.schema());
    Ok(Some(interp.encode_as(&field.ty, &v).map_err(|e| {
        anyhow::anyhow!("encoding {}: {e}", field.path)
    })?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soroban::HostRun;
    use stellar_xdr::{
        InvokeHostFunctionResult, OperationResult, OperationResultTr, TransactionResult,
        TransactionResultExt, TransactionResultResult,
    };

    fn failed(r: InvokeHostFunctionResult) -> ProbeAttempt {
        let result = TransactionResult {
            fee_charged: 100,
            result: TransactionResultResult::TxFailed(
                vec![OperationResult::OpInner(
                    OperationResultTr::InvokeHostFunction(r),
                )]
                .try_into()
                .unwrap(),
            ),
            ext: TransactionResultExt::V0,
        };
        ProbeAttempt {
            variant: "v".into(),
            submission: SubmitStatus::Pending,
            result_code: Some("tx_failed".into()),
            result_xdr: Some(result.to_xdr_base64(Limits::none()).unwrap()),
            applied: ApplyStatus::Failed,
            witnesses: Vec::new(),
            carrier: None,
        }
    }

    fn host(error: Option<&str>) -> HostRun {
        HostRun {
            error: error.map(str::to_owned),
            unsigned: Vec::new(),
        }
    }

    #[test]
    fn a_trap_the_host_also_hits_is_the_values_own() {
        let a = failed(InvokeHostFunctionResult::Trapped);
        agrees_with_host(&a, &host(Some("Error(Storage, MissingValue)"))).unwrap();
    }

    #[test]
    fn a_failure_the_host_does_not_reproduce_stops_the_run() {
        let a = failed(InvokeHostFunctionResult::Trapped);
        assert!(agrees_with_host(&a, &host(None)).is_err());
    }

    #[test]
    fn a_resource_failure_is_not_the_values() {
        let a = failed(InvokeHostFunctionResult::ResourceLimitExceeded);
        assert!(agrees_with_host(&a, &host(Some("Error(Budget, ExceededLimit)"))).is_err());
        let a = failed(InvokeHostFunctionResult::InsufficientRefundableFee);
        assert!(agrees_with_host(&a, &host(Some("Error(Budget, ExceededLimit)"))).is_err());
    }

    #[test]
    fn success_must_match_the_host() {
        let mut a = failed(InvokeHostFunctionResult::Trapped);
        a.applied = ApplyStatus::Succeeded;
        agrees_with_host(&a, &host(None)).unwrap();
        assert!(agrees_with_host(&a, &host(Some("Error(Auth, InvalidAction)"))).is_err());
    }

    #[test]
    fn a_refusal_at_submission_is_cores_validation() {
        let mut a = failed(InvokeHostFunctionResult::Trapped);
        a.applied = ApplyStatus::NotApplied;
        a.submission = SubmitStatus::Error;
        agrees_with_host(&a, &host(None)).unwrap();
        agrees_with_host(&a, &host(Some("Error(Value, InvalidInput)"))).unwrap();
    }
}
