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
//! | then   | payer adds to alice a pre-auth signer for the hash of a fee bump |
//! |        | alice pays for her own payment                                   |
//! | then   | that fee bump: core removes the one-time signer, then the inner  |
//! |        | transaction bumps alice's sequence, two groups for one account   |
//!
//! The failed inner payments name a destination that was never funded, so
//! they pass submission and fail at apply with `PAYMENT_NO_DESTINATION`;
//! the fee source is still charged.

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use shapekit_core_driver::chain::Applied;
use shapekit_core_driver::keys::{account_id, muxed_account, muxed_account_with_id};
use shapekit_core_driver::tx::{fee_bump, fee_bump_hash, sign_fee_bump, sign_transaction};
use stellar_xdr::{
    AccountEntry, Asset, InnerTransactionResultResult, LedgerEntryChange, LedgerEntryData,
    MuxedAccount, OperationBody, OperationResult, OperationResultTr, PaymentOp, PaymentResult,
    SetOptionsOp, Signer, SignerKey, TransactionEnvelope, TransactionMeta, TransactionResultResult,
    Uint256,
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
        ordering::bad_sponsorship(ctx, "inner-bad-sponsorship", &payer)?;

        self_bump_with_pre_auth_signer(ctx, &alice, &payer)
    }
}

/// Alice's account as `change` shows it.
fn account_of(change: &LedgerEntryChange) -> Option<&AccountEntry> {
    match change {
        LedgerEntryChange::State(e) | LedgerEntryChange::Updated(e) => match &e.data {
            LedgerEntryData::Account(a) => Some(a),
            _ => None,
        },
        _ => None,
    }
}

/// A fee bump over its own source, which holds a pre-auth signer for the fee-bump hash.
/// Core's `FeeBumpTransactionFrame::apply` removes that one-time signer from the fee source
/// in its own `LedgerTxn` and then the inner transaction bumps the sequence in another, so
/// `txChangesBefore` holds two groups for alice that each change her account: the shape an
/// interpreter reading only the last group gets wrong.
fn self_bump_with_pre_auth_signer(
    ctx: &mut ChainCtx,
    alice: &SigningKey,
    payer: &SigningKey,
) -> Result<()> {
    let alice_g = muxed_account(&alice.verifying_key());
    let bob = muxed_account(&ctx.account("bob").verifying_key());
    // built before the signer is added, which payer submits with alice as the operation
    // source so alice's sequence is still the one the inner transaction was built on
    let tx = ctx.tx(alice)?.operation(None, payment(bob)).build()?;
    let inner = sign_transaction(tx, &ctx.network_id(), &[alice])?;
    let outer = fee_bump(inner, alice_g.clone(), OUTER_FEE);
    let signer = SignerKey::PreAuthTx(Uint256(fee_bump_hash(&outer, &ctx.network_id())?));
    let set = ctx
        .tx(payer)?
        .operation(
            Some(alice_g),
            OperationBody::SetOptions(SetOptionsOp {
                inflation_dest: None,
                clear_flags: None,
                set_flags: None,
                master_weight: None,
                low_threshold: None,
                med_threshold: None,
                high_threshold: None,
                home_domain: None,
                signer: Some(Signer {
                    key: signer.clone(),
                    weight: 1,
                }),
            }),
        )
        .build()?;
    let env = ctx.sign(set, &[payer, alice])?;
    ctx.submit_and_close(&env)?;

    // the pre-auth signer alone authorises the fee bump; core refuses a master key signature
    // on top of it as tx_bad_auth_extra
    let env = TransactionEnvelope::TxFeeBump(sign_fee_bump(outer, &ctx.network_id(), &[])?);
    let applied = ctx.submit_and_close(&env)?;
    ensure_inner(&applied, Inner::Success)?;
    let TransactionMeta::V4(meta) = &applied.meta else {
        bail!("ledger {}: expected transaction meta v4", applied.ledger);
    };
    let id = account_id(&alice.verifying_key());
    let seen: Vec<(bool, i64)> = meta
        .tx_changes_before
        .iter()
        .filter_map(account_of)
        .filter(|a| a.account_id == id)
        .map(|a| (a.signers.iter().any(|s| s.key == signer), a.seq_num.0))
        .collect();
    let seq = seen.first().map(|s| s.1).unwrap_or_default();
    ensure!(
        seen == [(true, seq), (false, seq), (false, seq), (false, seq + 1)],
        "ledger {}: alice's txChangesBefore is {seen:?}, expected the signer removal and then the sequence bump as two groups",
        applied.ledger
    );
    ctx.capture(
        "fee-source-is-inner-source-pre-auth-signer",
        Selector::transaction(&applied),
        Spec::new(
            "a fee bump whose fee source is the inner source and holds a pre-auth signer for the fee-bump hash; core removes the signer in one LedgerTxn and the inner transaction bumps the sequence in another, so txChangesBefore holds two groups for the account",
        )
        .fields(&FIELDS),
    )
}
