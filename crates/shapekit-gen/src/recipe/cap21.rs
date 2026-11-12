//! `cap21.preconditions`: CAP-21 version 2 preconditions on payments:
//! ledger bounds next to time bounds, a minimum sequence number that lets
//! the transaction skip sequence numbers, a minimum sequence age and ledger
//! gap, two extra signers, and all of them in one transaction; plus the
//! account entry extension v3 that records when the sequence last moved.
//!
//! | ledger | content                                                            |
//! |--------|--------------------------------------------------------------------|
//! | 2      | protocol upgrade                                                   |
//! | 3      | root funds alice and bob                                           |
//! | 4      | ledger bounds and time bounds                                      |
//! | 5      | ledger bounds with a lower bound only                              |
//! | 6      | min sequence number, the transaction skips 99 sequence numbers     |
//! | 7, 8   | empty, so the minimum age and ledger gap are met                   |
//! | 9      | min sequence age and ledger gap                                    |
//! | 10     | two extra signers, an ed25519 key and a hash-x                     |
//! | 11, 12 | empty                                                              |
//! | 13     | every precondition at once                                         |
//!
//! Refused at submission, never in a ledger: ledger bounds that already
//! ended (`txTOO_LATE`) and a minimum age and gap not yet met
//! (`txBAD_MIN_SEQ_AGE_OR_GAP`), both `TransactionFrame::commonValid` in
//! stellar-core v30.0.0.

use anyhow::{Result, ensure};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use shapekit_core_driver::config::close_time;
use shapekit_core_driver::keys::{account_id, ed25519_signer, muxed_account};
use shapekit_core_driver::tx::{TransactionBuilder, push_signature, sign_hash_x};
use stellar_xdr::{
    AccountEntryExt, AccountEntryExtensionV1Ext, AccountEntryExtensionV2Ext, Asset, Duration,
    LedgerBounds, LedgerEntryData, LedgerKey, LedgerKeyAccount, OperationBody, PaymentOp,
    Preconditions, PreconditionsV2, SequenceNumber, SignerKey, TimeBounds, TimePoint,
    TransactionEnvelope, TransactionResultResult, Uint256,
};

use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "cap21.preconditions";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

/// Payment amount, 1 XLM.
const AMOUNT: i64 = 10_000_000;

/// How far a transaction jumps ahead of its account's sequence number.
const SEQ_JUMP: i64 = 100;

/// One ledger's worth of close time. Core compares the minimum age with
/// the last closed ledger's close time, and closes step one day.
const MIN_SEQ_AGE: u64 = 86_400;

/// Ledgers that must close after the sequence number moved.
const MIN_SEQ_LEDGER_GAP: u32 = 2;

/// Preimage of the hash-x extra signer.
const HASH_X_PREIMAGE: &[u8] = b"shapekit/cap21/hash-x/v1";

pub struct Preconds {
    pub protocol: u32,
}

fn payment(to: &SigningKey) -> OperationBody {
    OperationBody::Payment(PaymentOp {
        destination: muxed_account(&to.verifying_key()),
        asset: Asset::Native,
        amount: AMOUNT,
    })
}

fn v2() -> PreconditionsV2 {
    PreconditionsV2 {
        time_bounds: None,
        ledger_bounds: None,
        min_seq_num: None,
        min_seq_age: Duration(0),
        min_seq_ledger_gap: 0,
        extra_signers: Vec::new().try_into().expect("an empty vec fits"),
    }
}

/// Bounds around the next ledger, wide enough that the standalone clock
/// running ahead of the last close time cannot push it past them.
fn time_bounds(next: u32) -> TimeBounds {
    TimeBounds {
        min_time: TimePoint(close_time(next - 1)),
        max_time: TimePoint(close_time(next + 5)),
    }
}

fn hash_x_signer() -> SignerKey {
    SignerKey::HashX(Uint256(Sha256::digest(HASH_X_PREIMAGE).into()))
}

fn seq(ctx: &mut ChainCtx, key: &SigningKey) -> Result<i64> {
    Ok(ctx
        .driver()
        .node()
        .account_seq(&account_id(&key.verifying_key()))?)
}

fn payment_tx(
    ctx: &ChainCtx,
    alice: &SigningKey,
    bob: &SigningKey,
    seq_num: Option<i64>,
    cond: PreconditionsV2,
) -> Result<TransactionBuilder> {
    let b = match seq_num {
        Some(s) => TransactionBuilder::new(muxed_account(&alice.verifying_key()), s),
        None => ctx.tx(alice)?,
    };
    Ok(b.preconditions(Preconditions::V2(cond))
        .operation(None, payment(bob)))
}

fn ensure_refused(r: &TransactionResultResult, want: TransactionResultResult) -> Result<()> {
    ensure!(*r == want, "core answered {r:?}, expected {want:?}");
    Ok(())
}

const AGE_GAP_FIELDS: [&str; 3] = [
    "tx.cond.type",
    "tx.cond.min_seq_age",
    "tx.cond.min_seq_ledger_gap",
];

impl Recipe for Preconds {
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
        ctx.fund(&["alice", "bob"], BALANCE)?;
        let alice = ctx.account("alice");
        let bob = ctx.account("bob");
        // a key that only signs as an extra signer, never an account
        let cosigner = ctx.account("cosigner");

        let next = ctx.driver().last_closed() + 1;
        let cond = PreconditionsV2 {
            time_bounds: Some(time_bounds(next)),
            ledger_bounds: Some(LedgerBounds {
                min_ledger: next,
                max_ledger: next + 10,
            }),
            ..v2()
        };
        let env = ctx.sign(
            payment_tx(ctx, &alice, &bob, None, cond)?.build()?,
            &[&alice],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "ledger-and-time-bounds",
            Selector::transaction(&applied),
            Spec::new(
                "a payment valid only in a window of ledgers that opens at its own ledger, next to time bounds",
            )
            .fields(&["tx.cond.type", "tx.cond.time_bounds", "tx.cond.ledger_bounds"]),
        )?;

        // max_ledger 0 means no upper bound
        let next = ctx.driver().last_closed() + 1;
        let cond = PreconditionsV2 {
            ledger_bounds: Some(LedgerBounds {
                min_ledger: next - 1,
                max_ledger: 0,
            }),
            ..v2()
        };
        let env = ctx.sign(
            payment_tx(ctx, &alice, &bob, None, cond)?.build()?,
            &[&alice],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "ledger-bounds-no-maximum",
            Selector::transaction(&applied),
            Spec::new(
                "a payment with ledger bounds whose maximum is 0, which core reads as unbounded",
            )
            .fields(&["tx.cond.type", "tx.cond.ledger_bounds"]),
        )?;

        let last = ctx.driver().last_closed();
        let cond = PreconditionsV2 {
            ledger_bounds: Some(LedgerBounds {
                min_ledger: 0,
                max_ledger: last,
            }),
            ..v2()
        };
        let env = ctx.sign(
            payment_tx(ctx, &alice, &bob, None, cond)?.build()?,
            &[&alice],
        )?;
        let r = ctx.capture_rejected(
            "ledger-bounds-ended",
            &env,
            Spec::new(
                "a payment whose ledger bounds ended at the last closed ledger, refused by core as too late",
            )
            .fields(&["tx.cond.type", "tx.cond.ledger_bounds"]),
        )?;
        ensure_refused(&r.result, TransactionResultResult::TxTooLate)?;

        let current = seq(ctx, &alice)?;
        let cond = PreconditionsV2 {
            min_seq_num: Some(SequenceNumber(current)),
            ..v2()
        };
        let tx = payment_tx(ctx, &alice, &bob, Some(current + SEQ_JUMP), cond)?.build()?;
        let env = ctx.sign(tx, &[&alice])?;
        let applied = ctx.submit_and_close(&env)?;
        ensure!(
            seq(ctx, &alice)? == current + SEQ_JUMP,
            "alice's sequence number should have jumped to {}",
            current + SEQ_JUMP
        );
        ctx.capture(
            "min-seq-num-jump",
            Selector::transaction(&applied),
            Spec::new(
                "a payment whose sequence number is 100 ahead of its account's, accepted because min_seq_num names the account's current one",
            )
            .fields(&["tx.cond.type", "tx.cond.min_seq_num", "tx.seq_num"]),
        )?;

        let age_gap = || PreconditionsV2 {
            min_seq_age: Duration(MIN_SEQ_AGE),
            min_seq_ledger_gap: MIN_SEQ_LEDGER_GAP,
            ..v2()
        };
        let env = ctx.sign(
            payment_tx(ctx, &alice, &bob, None, age_gap())?.build()?,
            &[&alice],
        )?;
        let r = ctx.capture_rejected(
            "min-seq-age-and-gap-not-met",
            &env,
            Spec::new(
                "a payment submitted in the ledger right after its account's sequence number moved, refused because its minimum sequence age and ledger gap are not met",
            )
            .fields(&AGE_GAP_FIELDS),
        )?;
        ensure_refused(&r.result, TransactionResultResult::TxBadMinSeqAgeOrGap)?;

        for _ in 0..MIN_SEQ_LEDGER_GAP {
            ctx.driver().close_empty()?;
        }
        let env = ctx.sign(
            payment_tx(ctx, &alice, &bob, None, age_gap())?.build()?,
            &[&alice],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "min-seq-age-and-gap",
            Selector::transaction(&applied),
            Spec::new(
                "a payment requiring one day and two ledgers since its account's sequence number last moved",
            )
            .fields(&AGE_GAP_FIELDS),
        )?;

        let extra = || -> Result<PreconditionsV2> {
            Ok(PreconditionsV2 {
                extra_signers: vec![ed25519_signer(&cosigner.verifying_key()), hash_x_signer()]
                    .try_into()?,
                ..v2()
            })
        };
        let tx = payment_tx(ctx, &alice, &bob, None, extra()?)?.build()?;
        let mut env = ctx.sign(tx, &[&alice, &cosigner])?;
        push_signature(&mut env, sign_hash_x(HASH_X_PREIMAGE)?)?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "two-extra-signers",
            Selector::transaction(&applied),
            Spec::new(
                "a payment requiring two extra signers, an ed25519 key that is no signer of the account and a hash-x, signed by the source, that key and the preimage",
            )
            .fields(&[
                "tx.cond.type",
                "tx.cond.extra_signers",
                "tx.signatures",
                "tx.signatures.hint",
            ]),
        )?;

        for _ in 0..MIN_SEQ_LEDGER_GAP {
            ctx.driver().close_empty()?;
        }
        let next = ctx.driver().last_closed() + 1;
        let current = seq(ctx, &alice)?;
        let cond = PreconditionsV2 {
            time_bounds: Some(time_bounds(next)),
            ledger_bounds: Some(LedgerBounds {
                min_ledger: next,
                max_ledger: next + 1,
            }),
            min_seq_num: Some(SequenceNumber(current)),
            ..age_gap()
        };
        let cond = PreconditionsV2 {
            extra_signers: extra()?.extra_signers,
            ..cond
        };
        let tx = payment_tx(ctx, &alice, &bob, Some(current + SEQ_JUMP), cond)?.build()?;
        let mut env: TransactionEnvelope = ctx.sign(tx, &[&alice, &cosigner])?;
        push_signature(&mut env, sign_hash_x(HASH_X_PREIMAGE)?)?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "all-preconditions",
            Selector::transaction(&applied),
            Spec::new(
                "a payment carrying every v2 precondition at once: time bounds, a two-ledger window, a sequence jump allowed by min_seq_num, minimum age and gap, and two extra signers",
            )
            .fields(&[
                "tx.cond.type",
                "tx.cond.time_bounds",
                "tx.cond.ledger_bounds",
                "tx.cond.min_seq_num",
                "tx.cond.min_seq_age",
                "tx.cond.min_seq_ledger_gap",
                "tx.cond.extra_signers",
                "tx.seq_num",
            ]),
        )?;

        let key = LedgerKey::Account(LedgerKeyAccount {
            account_id: account_id(&alice.verifying_key()),
        });
        ctx.capture(
            "account-seq-ledger-and-time",
            Selector::LedgerEntry {
                ledger: applied.ledger,
                key: key.clone(),
            },
            Spec::new(
                "an account entry whose extension v3 records the ledger and close time its sequence number last moved",
            )
            .fields(&["tx.cond.min_seq_age", "tx.cond.min_seq_ledger_gap"]),
        )?;
        let entries = ctx
            .driver()
            .node()
            .ledger_entries(std::slice::from_ref(&key))?;
        match entries.entries[0].entry.as_ref().map(|e| &e.data) {
            Some(LedgerEntryData::Account(a)) => {
                let v3 = match &a.ext {
                    AccountEntryExt::V1(v1) => match &v1.ext {
                        AccountEntryExtensionV1Ext::V2(v2) => match &v2.ext {
                            AccountEntryExtensionV2Ext::V3(v3) => Some(v3),
                            AccountEntryExtensionV2Ext::V0 => None,
                        },
                        AccountEntryExtensionV1Ext::V0 => None,
                    },
                    AccountEntryExt::V0 => None,
                };
                let v3 = v3.ok_or_else(|| anyhow::anyhow!("alice has no account extension v3"))?;
                ensure!(
                    v3.seq_ledger == applied.ledger && v3.seq_time.0 == close_time(applied.ledger),
                    "alice's extension v3 records ledger {} time {}, expected ledger {}",
                    v3.seq_ledger,
                    v3.seq_time.0,
                    applied.ledger
                );
            }
            other => anyhow::bail!("alice's account is missing: {other:?}"),
        }
        Ok(())
    }
}
