//! `cap01.bump-sequence`: CAP-1 bump sequence forward, behind the current
//! sequence number (a no-op that still succeeds) and to `i64::MAX`; the
//! negative bump core refuses; and account merges that fail with
//! `ACCOUNT_MERGE_SEQNUM_TOO_FAR` after a bump past the ledger's starting
//! sequence number, inside the same transaction and in a later one.
//!
//! | ledger | content                                                          |
//! |--------|------------------------------------------------------------------|
//! | 2      | protocol upgrade                                                 |
//! | 3      | root funds alice, bob, maxed and mover                           |
//! | 4      | alice bumps her sequence number 2^20 ahead                       |
//! | 5      | alice bumps to 1, behind her sequence number                     |
//! | 6      | maxed bumps to `i64::MAX`                                        |
//! | 7      | mover bumps past the starting sequence and merges, in one tx (fails) |
//! | 8      | mover bumps past the starting sequence                           |
//! | 9      | mover merges into bob (fails)                                    |
//!
//! The starting sequence number of ledger `n` is `n << 32`; core refuses
//! to merge an account whose sequence number reached it, because a new
//! account created in that ledger would start there and could replay the
//! merged account's transactions (`MergeOpFrame::doApply`, stellar-core
//! v30.0.0). A negative `bump_to` is refused at submission with
//! `BUMP_SEQUENCE_BAD_SEQ` (`BumpSequenceOpFrame::doCheckValid`).

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use shapekit_core_driver::chain::Applied;
use shapekit_core_driver::keys::{account_id, muxed_account};
use stellar_xdr::{
    AccountMergeResult, BumpSequenceOp, BumpSequenceResult, OperationBody, OperationResult,
    OperationResultTr, SequenceNumber, TransactionResultResult,
};

use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "cap01.bump-sequence";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

pub struct BumpSequence {
    pub protocol: u32,
}

fn bump(to: i64) -> OperationBody {
    OperationBody::BumpSequence(BumpSequenceOp {
        bump_to: SequenceNumber(to),
    })
}

fn seq(ctx: &mut ChainCtx, key: &SigningKey) -> Result<i64> {
    Ok(ctx
        .driver()
        .node()
        .account_seq(&account_id(&key.verifying_key()))?)
}

/// A sequence number at or past the starting sequence number of the next
/// ledger and the one after, so a merge in either is refused.
fn past_start(ctx: &mut ChainCtx) -> i64 {
    (i64::from(ctx.driver().last_closed()) + 3) << 32
}

/// Fails unless operation `index` of a failed transaction is a merge
/// refused with `SEQNUM_TOO_FAR`.
fn ensure_too_far(applied: &Applied, index: usize) -> Result<()> {
    let TransactionResultResult::TxFailed(ops) = &applied.result.result else {
        bail!(
            "ledger {}: core answered {:?}, expected tx_failed",
            applied.ledger,
            applied.result.result
        );
    };
    let op = ops
        .get(index)
        .with_context(|| format!("no result for operation {index}"))?;
    ensure!(
        matches!(
            op,
            OperationResult::OpInner(OperationResultTr::AccountMerge(
                AccountMergeResult::SeqnumTooFar
            ))
        ),
        "operation {index} answered {op:?}, expected account_merge seqnum_too_far"
    );
    Ok(())
}

impl Recipe for BumpSequence {
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
        ctx.fund(&["alice", "bob", "maxed", "mover"], BALANCE)?;
        let alice = ctx.account("alice");
        let bob = ctx.account("bob");
        let maxed = ctx.account("maxed");
        let mover = ctx.account("mover");

        // a bump sequence result carries no account, so successes are
        // captured as their ledger, whose meta shows the new sequence number
        let target = seq(ctx, &alice)? + (1 << 20);
        let env = ctx.classic(&alice, vec![bump(target)])?;
        let applied = ctx.submit_and_close(&env)?;
        ensure!(
            seq(ctx, &alice)? == target,
            "alice's sequence number should be {target}"
        );
        ctx.capture(
            "bump-forward",
            Selector::ledger_of(&applied),
            Spec::new("bump sequence moves the source's sequence number 2^20 ahead")
                .fields(&["op.bump_sequence.bump_to"]),
        )?;

        let before = seq(ctx, &alice)?;
        let env = ctx.classic(&alice, vec![bump(1)])?;
        let applied = ctx.submit_and_close(&env)?;
        ensure!(
            seq(ctx, &alice)? == before + 1,
            "a bump behind the sequence number should only consume the transaction's own"
        );
        ctx.capture(
            "bump-behind",
            Selector::ledger_of(&applied),
            Spec::new(
                "bump sequence to 1, behind the source's sequence number: it succeeds and changes nothing beyond the transaction's own sequence number",
            )
            .fields(&["op.bump_sequence.bump_to"]),
        )?;

        let env = ctx.classic(&maxed, vec![bump(i64::MAX)])?;
        let applied = ctx.submit_and_close(&env)?;
        ensure!(
            seq(ctx, &maxed)? == i64::MAX,
            "maxed's sequence number should be i64::MAX"
        );
        ctx.capture(
            "bump-to-i64-max",
            Selector::ledger_of(&applied),
            Spec::new(
                "bump sequence to i64::MAX, after which the account can never submit another transaction",
            )
            .fields(&["op.bump_sequence.bump_to"]),
        )?;

        let env = ctx.classic(&alice, vec![bump(-1)])?;
        let r = ctx.capture_rejected(
            "bump-negative",
            &env,
            Spec::new(
                "bump sequence to -1, refused by core at submission with bump_sequence bad_seq",
            )
            .fields(&["op.bump_sequence.bump_to"]),
        )?;
        let TransactionResultResult::TxFailed(ops) = &r.result else {
            bail!(
                "core answered {:?} for a negative bump, expected tx_failed",
                r.result
            );
        };
        ensure!(
            matches!(
                ops.first(),
                Some(OperationResult::OpInner(OperationResultTr::BumpSequence(
                    BumpSequenceResult::BadSeq
                )))
            ),
            "core answered {ops:?} for a negative bump, expected bump_sequence bad_seq"
        );

        let far = past_start(ctx);
        let env = ctx.classic(
            &mover,
            vec![
                bump(far),
                OperationBody::AccountMerge(muxed_account(&bob.verifying_key())),
            ],
        )?;
        let applied = ctx.submit_and_close_failing(&env)?;
        ensure_too_far(&applied, 1)?;
        ctx.capture(
            "bump-then-merge-seqnum-too-far",
            Selector::transaction(&applied),
            Spec::new(
                "one transaction bumps its source past the ledger's starting sequence number and then merges it, so the merge fails with seqnum_too_far and the bump is rolled back",
            )
            .fields(&["op.bump_sequence.bump_to", "op.account_merge.destination"]),
        )?;

        let far = past_start(ctx);
        let env = ctx.classic(&mover, vec![bump(far)])?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "bump-past-starting-sequence",
            Selector::ledger_of(&applied),
            Spec::new(
                "bump sequence past the starting sequence number of the following ledgers, which makes the account unmergeable until the ledger sequence catches up",
            )
            .fields(&["op.bump_sequence.bump_to"]),
        )?;

        let env = ctx.classic(
            &mover,
            vec![OperationBody::AccountMerge(muxed_account(
                &bob.verifying_key(),
            ))],
        )?;
        let applied = ctx.submit_and_close_failing(&env)?;
        ensure_too_far(&applied, 0)?;
        ctx.capture(
            "merge-seqnum-too-far",
            Selector::ledger_of(&applied),
            Spec::new(
                "an account merge whose source's sequence number is past the ledger's starting sequence number, failing with seqnum_too_far",
            )
            .fields(&["op.account_merge.destination"]),
        )
    }
}
