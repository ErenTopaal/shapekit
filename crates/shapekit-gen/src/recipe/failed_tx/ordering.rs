//! Apply-time failures that need an earlier transaction in the same ledger
//! to change the state a later one was validated against. Core validates
//! both against the last closed ledger and re-checks signatures, sequence
//! and sequence age at apply (`TransactionFrame::commonValid`,
//! `processSignatures`, stellar-core v30.0.0), so the later one fails only
//! if it is applied second; [`apply_in_order`] fixes that order.
//!
//! Each case can wrap the failing transaction in a fee bump paid by
//! `payer`, which gives the inner result arm instead of the outer one.

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use shapekit_core_driver::chain::Applied;
use shapekit_core_driver::keys::{account_id, ed25519_signer, muxed_account};
use shapekit_core_driver::tx::{fee_bump, sign_fee_bump, sign_transaction};
use stellar_xdr::{
    Asset, BeginSponsoringFutureReservesOp, BumpSequenceOp, Duration, Memo, OperationBody,
    PaymentOp, Preconditions, PreconditionsV2, SequenceNumber, SetOptionsOp, Signer, Transaction,
    TransactionEnvelope, TransactionResultResult,
};

use super::super::order::apply_in_order;
use super::super::{ChainCtx, Selector, Spec};
use super::codes::op_code;

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

/// One XLM in stroops.
const UNIT: i64 = 10_000_000;

/// Outer fee of a fee bump over one operation; any fee above the inner rate
/// over two operations will do, the set still uses the ledger base fee.
const OUTER_FEE: i64 = 1_000;

/// One case's accounts, derived from `prefix` so the outer and the fee bump
/// variants of a case never share state.
struct Roles {
    /// Sources the earlier transaction.
    first: SigningKey,
    /// The account whose state the earlier transaction changes.
    target: SigningKey,
    /// Sources the later transaction when the target does not.
    second: SigningKey,
    /// A signer key, never funded.
    signer: SigningKey,
}

fn roles(ctx: &mut ChainCtx, prefix: &str) -> Result<Roles> {
    let names = [
        format!("{prefix}-first"),
        format!("{prefix}-target"),
        format!("{prefix}-second"),
    ];
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    ctx.fund(&refs, BALANCE)?;
    Ok(Roles {
        first: ctx.account(&names[0]),
        target: ctx.account(&names[1]),
        second: ctx.account(&names[2]),
        signer: ctx.account(&format!("{prefix}-signer")),
    })
}

fn set_options() -> SetOptionsOp {
    SetOptionsOp {
        inflation_dest: None,
        clear_flags: None,
        set_flags: None,
        master_weight: None,
        low_threshold: None,
        med_threshold: None,
        high_threshold: None,
        home_domain: None,
        signer: None,
    }
}

fn signer(k: &SigningKey, weight: u32) -> Option<Signer> {
    Some(Signer {
        key: ed25519_signer(&k.verifying_key()),
        weight,
    })
}

fn pay(to: &SigningKey) -> OperationBody {
    OperationBody::Payment(PaymentOp {
        destination: muxed_account(&to.verifying_key()),
        asset: Asset::Native,
        amount: UNIT,
    })
}

fn mux(k: &SigningKey) -> stellar_xdr::MuxedAccount {
    muxed_account(&k.verifying_key())
}

/// Signs `tx` with `keys`, and wraps it in a fee bump from `payer` if
/// there is one.
fn envelope(
    ctx: &ChainCtx,
    tx: Transaction,
    keys: &[&SigningKey],
    payer: Option<&SigningKey>,
) -> Result<TransactionEnvelope> {
    let inner = sign_transaction(tx, &ctx.network_id(), keys)?;
    Ok(match payer {
        None => TransactionEnvelope::Tx(inner),
        Some(p) => TransactionEnvelope::TxFeeBump(sign_fee_bump(
            fee_bump(inner, mux(p), OUTER_FEE),
            &ctx.network_id(),
            &[p],
        )?),
    })
}

/// The result arm core gave, with `inner.` before the inner arm of a fee
/// bump.
fn tx_code(r: &TransactionResultResult) -> Result<String> {
    let arm = |v: serde_json::Value| -> Result<String> {
        match v {
            serde_json::Value::String(s) => Ok(s),
            serde_json::Value::Object(m) if m.len() == 1 => {
                Ok(m.keys().next().cloned().unwrap_or_default())
            }
            other => bail!("result {other}"),
        }
    };
    match r {
        TransactionResultResult::TxFeeBumpInnerFailed(p)
        | TransactionResultResult::TxFeeBumpInnerSuccess(p) => Ok(format!(
            "{}/inner.{}",
            arm(serde_json::to_value(r)?)?,
            arm(serde_json::to_value(&p.result.result)?)?
        )),
        other => arm(serde_json::to_value(other)?),
    }
}

/// Runs the pair, checks the earlier transaction succeeded and the later
/// one got `want`, and captures their ledger.
fn finish(
    ctx: &mut ChainCtx,
    slug: &str,
    applied: Vec<Applied>,
    want: &str,
    spec: Spec,
) -> Result<()> {
    let [first, second] = applied.as_slice() else {
        bail!("{slug}: expected two applied transactions");
    };
    ensure!(
        first.succeeded(),
        "{slug}: the earlier transaction failed with {:?}",
        first.result.result
    );
    let got = tx_code(&second.result.result)?;
    ensure!(got == want, "{slug}: core answered {got}, expected {want}");
    ctx.capture(slug, Selector::ledger_of(second), spec)
}

fn wanted(payer: Option<&SigningKey>, code: &str) -> String {
    match payer {
        None => code.to_string(),
        Some(_) => format!("tx_fee_bump_inner_failed/inner.{code}"),
    }
}

fn memo(variant: u64) -> Memo {
    Memo::Id(variant)
}

/// The target's signer is removed by an earlier transaction, so a payment
/// whose operation source is the target, signed only by that signer, fails
/// with op_bad_auth.
pub(super) fn op_bad_auth(ctx: &mut ChainCtx, slug: &str) -> Result<()> {
    let r = roles(ctx, slug)?;
    let add = ctx.classic(
        &r.target,
        vec![OperationBody::SetOptions(SetOptionsOp {
            signer: signer(&r.signer, 1),
            ..set_options()
        })],
    )?;
    ctx.submit_and_close(&add)?;
    let applied = apply_in_order(ctx, |ctx, v| {
        let remove = ctx
            .tx(&r.first)?
            .operation(
                Some(mux(&r.target)),
                OperationBody::SetOptions(SetOptionsOp {
                    signer: signer(&r.signer, 0),
                    ..set_options()
                }),
            )
            .build()?;
        let later = ctx
            .tx(&r.second)?
            .memo(memo(v))
            .operation(Some(mux(&r.target)), pay(&r.first))
            .build()?;
        Ok([
            envelope(ctx, remove, &[&r.first, &r.target], None)?,
            envelope(ctx, later, &[&r.second, &r.signer], None)?,
        ])
    })?;
    let ops = match &applied[1].result.result {
        TransactionResultResult::TxFailed(ops) => ops,
        other => bail!("{slug}: core answered {other:?}, expected tx_failed"),
    };
    let got = op_code(ops.first().context("no operation result")?)?;
    ensure!(got == "op_bad_auth", "{slug}: core answered {got}");
    finish(
        ctx,
        slug,
        applied,
        "tx_failed",
        Spec::new(
            "two transactions in one ledger: the first removes a signer from an account, the second pays from that account as operation source signed only by the removed signer, so its operation fails with op_bad_auth at apply",
        )
        .fields(&["op.source_account", "tx.signatures", "op.set_options.signer"]),
    )
}

/// The target needs two signatures until an earlier transaction lowers its
/// thresholds and removes the second signer, so the later transaction
/// carries a signature nothing uses: tx_bad_auth_extra.
pub(crate) fn bad_auth_extra(
    ctx: &mut ChainCtx,
    slug: &str,
    payer: Option<&SigningKey>,
) -> Result<()> {
    let r = roles(ctx, slug)?;
    let setup = ctx.classic(
        &r.target,
        vec![OperationBody::SetOptions(SetOptionsOp {
            master_weight: Some(1),
            low_threshold: Some(2),
            med_threshold: Some(2),
            high_threshold: Some(2),
            signer: signer(&r.signer, 1),
            ..set_options()
        })],
    )?;
    ctx.submit_and_close(&setup)?;
    let applied = apply_in_order(ctx, |ctx, v| {
        let relax = ctx
            .tx(&r.first)?
            .operation(
                Some(mux(&r.target)),
                OperationBody::SetOptions(SetOptionsOp {
                    low_threshold: Some(1),
                    med_threshold: Some(1),
                    high_threshold: Some(1),
                    signer: signer(&r.signer, 0),
                    ..set_options()
                }),
            )
            .build()?;
        let later = ctx
            .tx(&r.target)?
            .memo(memo(v))
            .operation(None, pay(&r.first))
            .build()?;
        Ok([
            envelope(ctx, relax, &[&r.first, &r.target, &r.signer], None)?,
            envelope(ctx, later, &[&r.target, &r.signer], payer)?,
        ])
    })?;
    finish(
        ctx,
        slug,
        applied,
        &wanted(payer, "tx_bad_auth_extra"),
        Spec::new(format!(
            "two transactions in one ledger: the first lowers an account's thresholds to one and removes its second signer, the second{} is signed by the master key and that signer, so at apply one signature is unused: tx_bad_auth_extra",
            if payer.is_some() { ", inside a fee bump," } else { "" }
        ))
        .fields(&["tx.signatures", "op.set_options.signer", "op.set_options.thresholds_and_flags"]),
    )
}

/// An earlier transaction bumps the target's sequence (to zero, a no-op
/// that still stamps the account's sequence ledger,
/// `BumpSequenceOpFrame::doApply`), so the target's own transaction asking
/// for a ledger gap of one fails with tx_bad_min_seq_age_or_gap.
pub(crate) fn bad_min_seq_gap(
    ctx: &mut ChainCtx,
    slug: &str,
    payer: Option<&SigningKey>,
) -> Result<()> {
    let r = roles(ctx, slug)?;
    let warm = ctx.classic(&r.second, vec![pay(&r.first)])?;
    ctx.submit_and_close(&warm)?;
    let applied = apply_in_order(ctx, |ctx, v| {
        let stamp = ctx
            .tx(&r.first)?
            .operation(
                Some(mux(&r.target)),
                OperationBody::BumpSequence(BumpSequenceOp {
                    bump_to: SequenceNumber(0),
                }),
            )
            .build()?;
        let later = ctx
            .tx(&r.target)?
            .memo(memo(v))
            .preconditions(Preconditions::V2(PreconditionsV2 {
                time_bounds: None,
                ledger_bounds: None,
                min_seq_num: None,
                min_seq_age: Duration(0),
                min_seq_ledger_gap: 1,
                extra_signers: Default::default(),
            }))
            .operation(None, pay(&r.first))
            .build()?;
        Ok([
            envelope(ctx, stamp, &[&r.first, &r.target], None)?,
            envelope(ctx, later, &[&r.target], payer)?,
        ])
    })?;
    finish(
        ctx,
        slug,
        applied,
        &wanted(payer, "tx_bad_min_seq_age_or_gap"),
        Spec::new(format!(
            "two transactions in one ledger: the first bumps an account's sequence to zero as operation source, which changes nothing but its sequence ledger, so the second{}, from that account with a minimum ledger gap of one, fails with tx_bad_min_seq_age_or_gap",
            if payer.is_some() { ", inside a fee bump," } else { "" }
        ))
        .fields(&["tx.cond.min_seq_ledger_gap", "op.bump_sequence.bump_to"]),
    )
}

/// An earlier transaction removes the target's only other signer, so the
/// later one signed by that signer alone fails with tx_bad_auth.
pub(crate) fn bad_auth(ctx: &mut ChainCtx, slug: &str, payer: &SigningKey) -> Result<()> {
    let r = roles(ctx, slug)?;
    let add = ctx.classic(
        &r.target,
        vec![OperationBody::SetOptions(SetOptionsOp {
            signer: signer(&r.signer, 1),
            ..set_options()
        })],
    )?;
    ctx.submit_and_close(&add)?;
    let applied = apply_in_order(ctx, |ctx, v| {
        let remove = ctx
            .tx(&r.first)?
            .operation(
                Some(mux(&r.target)),
                OperationBody::SetOptions(SetOptionsOp {
                    signer: signer(&r.signer, 0),
                    ..set_options()
                }),
            )
            .build()?;
        let later = ctx
            .tx(&r.target)?
            .memo(memo(v))
            .operation(None, pay(&r.first))
            .build()?;
        Ok([
            envelope(ctx, remove, &[&r.first, &r.target], None)?,
            envelope(ctx, later, &[&r.signer], Some(payer))?,
        ])
    })?;
    finish(
        ctx,
        slug,
        applied,
        &wanted(Some(payer), "tx_bad_auth"),
        Spec::new(
            "two transactions in one ledger: the first removes a signer, the second, inside a fee bump, is signed by that signer alone, so its inner transaction fails with tx_bad_auth at apply",
        )
        .fields(&["feebump.inner_tx", "tx.signatures", "op.set_options.signer"]),
    )
}

/// An earlier transaction bumps the target's sequence ahead, so the later
/// one carrying the next sequence number fails with tx_bad_seq.
pub(crate) fn bad_seq(ctx: &mut ChainCtx, slug: &str, payer: &SigningKey) -> Result<()> {
    let r = roles(ctx, slug)?;
    let warm = ctx.classic(&r.second, vec![pay(&r.first)])?;
    ctx.submit_and_close(&warm)?;
    let target_seq = ctx
        .driver()
        .node()
        .account_seq(&account_id(&r.target.verifying_key()))?;
    let applied = apply_in_order(ctx, |ctx, v| {
        let jump = ctx
            .tx(&r.first)?
            .operation(
                Some(mux(&r.target)),
                OperationBody::BumpSequence(BumpSequenceOp {
                    bump_to: SequenceNumber(target_seq + 100),
                }),
            )
            .build()?;
        let later = ctx
            .tx(&r.target)?
            .memo(memo(v))
            .operation(None, pay(&r.first))
            .build()?;
        Ok([
            envelope(ctx, jump, &[&r.first, &r.target], None)?,
            envelope(ctx, later, &[&r.target], Some(payer))?,
        ])
    })?;
    finish(
        ctx,
        slug,
        applied,
        &wanted(Some(payer), "tx_bad_seq"),
        Spec::new(
            "two transactions in one ledger: the first bumps an account's sequence 100 ahead as operation source, so the second, a fee bump over that account's next sequence number, fails with inner tx_bad_seq at apply",
        )
        .fields(&["feebump.inner_tx", "tx.seq_num", "op.bump_sequence.bump_to"]),
    )
}

/// A fee bump over a transaction that begins sponsoring and never ends it:
/// core checks open sponsorships after the operations
/// (`TransactionFrame::applyOperations`), so the inner transaction fails
/// with tx_bad_sponsorship. No ordering is involved.
pub(crate) fn bad_sponsorship(ctx: &mut ChainCtx, slug: &str, payer: &SigningKey) -> Result<()> {
    let r = roles(ctx, slug)?;
    let tx = ctx
        .tx(&r.first)?
        .operation(
            None,
            OperationBody::BeginSponsoringFutureReserves(BeginSponsoringFutureReservesOp {
                sponsored_id: account_id(&r.target.verifying_key()),
            }),
        )
        .build()?;
    let env = envelope(ctx, tx, &[&r.first], Some(payer))?;
    let applied = ctx.submit_and_close_failing(&env)?;
    let want = wanted(Some(payer), "tx_bad_sponsorship");
    let got = tx_code(&applied.result.result)?;
    ensure!(got == want, "{slug}: core answered {got}, expected {want}");
    ctx.capture(
        slug,
        Selector::ledger_of(&applied),
        Spec::new(
            "a fee bump over a transaction that begins sponsoring another account's reserves and never ends it, so the inner transaction fails with tx_bad_sponsorship",
        )
        .fields(&["feebump.inner_tx", "op.begin_sponsoring.sponsored_id"]),
    )
}
