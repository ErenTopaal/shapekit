//! `cap15.fee-bump`: CAP-15 fee-bump envelopes over a successful and a
//! failed inner transaction, with a plain and a muxed fee source, and a fee
//! bump whose fee source is the inner transaction's own source.
//!
//! | ledger | content                                                          |
//! |--------|------------------------------------------------------------------|
//! | 2      | protocol upgrade                                                 |
//! | 3      | root funds alice, bob and payer                                  |
//! | 4      | payer bumps alice's payment to bob, inner success                |
//! | 5      | payer bumps alice's payment to an unfunded account, inner failed |
//! | 6      | payer as muxed fee source (id 2^64-1), inner success             |
//! | 7      | payer as muxed fee source (id 1), inner failed                   |
//! | 8      | alice bumps her own payment                                      |
//! | 9 on   | inner tx_bad_auth, tx_bad_seq, tx_bad_auth_extra and             |
//! |        | tx_bad_min_seq_age_or_gap, each after an earlier transaction in  |
//! |        | the same ledger (`failed_tx::ordering`), and tx_bad_sponsorship  |
//!
//! The failed inner payments name a destination that was never funded, so
//! they pass submission and fail at apply with `PAYMENT_NO_DESTINATION`;
//! the fee source is still charged.

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use shapekit_core_driver::chain::Applied;
use shapekit_core_driver::keys::{muxed_account, muxed_account_with_id};
use shapekit_core_driver::tx::{fee_bump, sign_fee_bump, sign_transaction};
use stellar_xdr::{
    Asset, InnerTransactionResultResult, MuxedAccount, OperationBody, OperationResult,
    OperationResultTr, PaymentOp, PaymentResult, TransactionEnvelope, TransactionResultResult,
};

use super::failed_tx::ordering;
use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "cap15.fee-bump";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

/// Payment amount, 1 XLM.
const AMOUNT: i64 = 10_000_000;

/// Outer fee of every bump. Core needs at least the base fee for each inner
/// operation plus one; this is well above it so the bump is visible.
const OUTER_FEE: i64 = 1_000;

const FIELDS: [&str; 5] = [
    "tx.envelope.type",
    "feebump.fee_source",
    "feebump.fee",
    "feebump.signatures",
    "feebump.inner_tx",
];

const MUXED_FIELDS: [&str; 6] = [
    "tx.envelope.type",
    "feebump.fee_source",
    "feebump.fee_source.mux_id",
    "feebump.fee",
    "feebump.signatures",
    "feebump.inner_tx",
];

pub struct FeeBump {
    pub protocol: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Inner {
    Success,
    Failed,
}

fn payment(to: MuxedAccount) -> OperationBody {
    OperationBody::Payment(PaymentOp {
        destination: to,
        asset: Asset::Native,
        amount: AMOUNT,
    })
}

/// Alice's payment to `to`, signed by alice, wrapped in a fee bump from
/// `fee_source` signed by `payer`.
fn bumped(
    ctx: &ChainCtx,
    inner_source: &SigningKey,
    to: MuxedAccount,
    fee_source: MuxedAccount,
    payer: &SigningKey,
) -> Result<TransactionEnvelope> {
    let tx = ctx.tx(inner_source)?.operation(None, payment(to)).build()?;
    let inner = sign_transaction(tx, &ctx.network_id(), &[inner_source])?;
    let outer = fee_bump(inner, fee_source, OUTER_FEE);
    Ok(TransactionEnvelope::TxFeeBump(sign_fee_bump(
        outer,
        &ctx.network_id(),
        &[payer],
    )?))
}

/// Checks core's result is the fee-bump arm `want`, and for a failed inner
/// transaction that its payment failed for the missing destination.
fn ensure_inner(applied: &Applied, want: Inner) -> Result<()> {
    let inner = match (&applied.result.result, want) {
        (TransactionResultResult::TxFeeBumpInnerSuccess(p), Inner::Success) => p,
        (TransactionResultResult::TxFeeBumpInnerFailed(p), Inner::Failed) => p,
        (other, _) => bail!(
            "ledger {}: core answered {other:?}, expected a fee bump with an inner {want:?}",
            applied.ledger
        ),
    };
    match (&inner.result.result, want) {
        (InnerTransactionResultResult::TxSuccess(_), Inner::Success) => Ok(()),
        (InnerTransactionResultResult::TxFailed(ops), Inner::Failed) => {
            let op = ops
                .first()
                .context("inner tx_failed without operation results")?;
            ensure!(
                matches!(
                    op,
                    OperationResult::OpInner(OperationResultTr::Payment(
                        PaymentResult::NoDestination
                    ))
                ),
                "inner payment failed with {op:?}, expected payment no_destination"
            );
            Ok(())
        }
        (other, _) => bail!("inner result {other:?}, expected {want:?}"),
    }
}

impl Recipe for FeeBump {
    fn id(&self) -> &str {
        FAMILY
    }

    fn family(&self) -> &str {
        FAMILY
    }

    fn protocol(&self) -> u32 {
        self.protocol
    }

    fn run(&self, ctx: &mut ChainCtx) -> Result<()> {
        ctx.fund(&["alice", "bob", "payer"], BALANCE)?;
        let alice = ctx.account("alice");
        let bob = muxed_account(&ctx.account("bob").verifying_key());
        let payer = ctx.account("payer");
        // never funded, so a payment to it fails at apply, not at submission
        let ghost = muxed_account(&ctx.account("ghost").verifying_key());
        let payer_g = muxed_account(&payer.verifying_key());

        let cases: [(&str, MuxedAccount, MuxedAccount, Inner, String); 4] = [
            (
                "inner-success",
                bob.clone(),
                payer_g.clone(),
                Inner::Success,
                "a fee bump paid by a plain account over a successful inner payment".into(),
            ),
            (
                "inner-failed",
                ghost.clone(),
                payer_g,
                Inner::Failed,
                "a fee bump paid by a plain account over an inner payment that fails at apply with payment no_destination; the fee source is charged".into(),
            ),
            (
                "muxed-fee-source-inner-success",
                bob,
                muxed_account_with_id(&payer.verifying_key(), u64::MAX),
                Inner::Success,
                "a fee bump whose fee source is a muxed account with id 2^64-1, over a successful inner payment".into(),
            ),
            (
                "muxed-fee-source-inner-failed",
                ghost,
                muxed_account_with_id(&payer.verifying_key(), 1),
                Inner::Failed,
                "a fee bump whose fee source is a muxed account with id 1, over an inner payment that fails with payment no_destination".into(),
            ),
        ];
        for (slug, to, fee_source, want, note) in cases {
            let muxed = matches!(fee_source, MuxedAccount::MuxedEd25519(_));
            let env = bumped(ctx, &alice, to, fee_source, &payer)?;
            let applied = match want {
                Inner::Success => ctx.submit_and_close(&env)?,
                Inner::Failed => ctx.submit_and_close_failing(&env)?,
            };
            ensure_inner(&applied, want)?;
            let fields: &[&str] = if muxed { &MUXED_FIELDS } else { &FIELDS };
            ctx.capture(
                slug,
                Selector::transaction(&applied),
                Spec::new(note).fields(fields),
            )?;
        }

        // the fee source and the inner source are the same account, which
        // pays the fee once and has its sequence number consumed by the inner
        let alice_g = muxed_account(&alice.verifying_key());
        let bob = muxed_account(&ctx.account("bob").verifying_key());
        let env = bumped(ctx, &alice, bob, alice_g, &alice)?;
        let applied = ctx.submit_and_close(&env)?;
        ensure_inner(&applied, Inner::Success)?;
        ctx.capture(
            "fee-source-is-inner-source",
            Selector::transaction(&applied),
            Spec::new("a fee bump whose fee source is the inner transaction's own source account")
                .fields(&FIELDS),
        )?;

        // inner results only apply can give: an earlier transaction in the
        // same ledger changes what the inner one was validated against
        ordering::bad_auth(ctx, "inner-bad-auth", &payer)?;
        ordering::bad_seq(ctx, "inner-bad-seq", &payer)?;
        ordering::bad_auth_extra(ctx, "inner-bad-auth-extra", Some(&payer))?;
        ordering::bad_min_seq_gap(ctx, "inner-bad-min-seq-gap", Some(&payer))?;
        ordering::bad_sponsorship(ctx, "inner-bad-sponsorship", &payer)
    }
}
