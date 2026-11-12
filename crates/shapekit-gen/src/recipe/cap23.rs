//! `cap23.claimable-balances`: a claimable balance with the maximum ten
//! claimants whose predicates reach the deepest nesting core accepts, its
//! stored entry, a claim through a depth-four predicate, and the one-deeper
//! twin core refuses.
//!
//! | ledger | content                                                        |
//! |--------|----------------------------------------------------------------|
//! | 2      | protocol upgrade                                               |
//! | 3      | root funds the creator and ten claimants                       |
//! | 4      | the creator creates a balance for ten claimants                |
//! | 5      | the claimant holding the depth-four predicate claims it        |
//!
//! Core starts `validatePredicate` at depth 1 and fails a predicate node at
//! depth 5 (`CreateClaimableBalanceOpFrame.cpp`, stellar-core v30.0.0), so a
//! leaf under and, or and not is the deepest accepted shape. The twin with
//! one more level is refused at submission with
//! `CREATE_CLAIMABLE_BALANCE_MALFORMED` and never reaches a ledger.

use anyhow::{Context, Result, bail, ensure};
use shapekit_core_driver::chain::Applied;
use shapekit_core_driver::keys::account_id;
use stellar_xdr::{
    Asset, ClaimClaimableBalanceOp, ClaimPredicate, ClaimableBalanceId, Claimant, ClaimantV0,
    CreateClaimableBalanceOp, CreateClaimableBalanceResult, LedgerEntryData, LedgerKey,
    LedgerKeyClaimableBalance, OperationBody, OperationResult, OperationResultTr,
    TransactionResultResult,
};

use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "cap23.claimable-balances";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

/// Amount held by the balance, 100 XLM.
const AMOUNT: i64 = 1_000_000_000;

/// 2100-01-01T00:00:00Z, far after every close time of the chain.
const FAR: i64 = 4_102_444_800;

/// Seconds in thirty days.
const MONTH: i64 = 30 * 86_400;

/// Deepest predicate level core accepts (`registry/caps-limits.toml`
/// `claim_predicate.depth`).
pub const MAX_DEPTH: u32 = 4;

pub struct ClaimableBalances {
    pub protocol: u32,
}

fn abs(t: i64) -> ClaimPredicate {
    ClaimPredicate::BeforeAbsoluteTime(t)
}

fn rel(t: i64) -> ClaimPredicate {
    ClaimPredicate::BeforeRelativeTime(t)
}

fn not(p: ClaimPredicate) -> ClaimPredicate {
    ClaimPredicate::Not(Some(Box::new(p)))
}

fn and(a: ClaimPredicate, b: ClaimPredicate) -> Result<ClaimPredicate> {
    Ok(ClaimPredicate::And(vec![a, b].try_into()?))
}

fn or(a: ClaimPredicate, b: ClaimPredicate) -> Result<ClaimPredicate> {
    Ok(ClaimPredicate::Or(vec![a, b].try_into()?))
}

/// Levels of the deepest path, counted the way core counts them: the root
/// is level 1.
pub fn depth(p: &ClaimPredicate) -> u32 {
    1 + match p {
        ClaimPredicate::And(v) | ClaimPredicate::Or(v) => v.iter().map(depth).max().unwrap_or(0),
        ClaimPredicate::Not(Some(inner)) => depth(inner),
        _ => 0,
    }
}

fn has_relative(p: &ClaimPredicate) -> bool {
    match p {
        ClaimPredicate::BeforeRelativeTime(_) => true,
        ClaimPredicate::And(v) | ClaimPredicate::Or(v) => v.iter().any(has_relative),
        ClaimPredicate::Not(Some(inner)) => has_relative(inner),
        _ => false,
    }
}

/// A depth-four tree that is true from the first ledger on: and over two
/// ors, each holding a not over a time bound in the past.
fn deepest() -> Result<ClaimPredicate> {
    and(or(not(abs(1)), not(rel(1)))?, or(not(abs(2)), abs(FAR))?)
}

/// `deepest` with one not wrapping an and, so one path reaches level five.
fn one_too_deep() -> Result<ClaimPredicate> {
    and(
        or(not(and(abs(1), abs(FAR))?), not(rel(1)))?,
        or(not(abs(2)), abs(FAR))?,
    )
}

/// The ten predicates, one per claimant, covering every predicate arm and
/// every depth from one to four. The last is the one that claims.
fn predicates() -> Result<Vec<ClaimPredicate>> {
    Ok(vec![
        ClaimPredicate::Unconditional,
        abs(FAR),
        rel(MONTH),
        not(abs(1)),
        and(abs(FAR), rel(MONTH))?,
        or(abs(1), ClaimPredicate::Unconditional)?,
        not(or(abs(1), rel(1))?),
        or(
            and(not(rel(1)), abs(FAR))?,
            not(ClaimPredicate::Unconditional),
        )?,
        not(and(or(abs(1), rel(MONTH))?, not(abs(FAR)))?),
        deepest()?,
    ])
}

/// The operation result at `index` of a transaction result, failed or not.
pub(super) fn op_result(r: &TransactionResultResult, index: usize) -> Result<&OperationResultTr> {
    let ops = match r {
        TransactionResultResult::TxSuccess(ops) | TransactionResultResult::TxFailed(ops) => ops,
        other => bail!("expected operation results, core answered {other:?}"),
    };
    match ops.get(index) {
        Some(OperationResult::OpInner(tr)) => Ok(tr),
        other => bail!("operation {index}: expected an inner result, core answered {other:?}"),
    }
}

/// The id of the balance the transaction's only CreateClaimableBalance made.
pub(super) fn created_balance_id(applied: &Applied) -> Result<ClaimableBalanceId> {
    let TransactionResultResult::TxSuccess(ops) = &applied.result.result else {
        bail!("ledger {}: transaction did not succeed", applied.ledger);
    };
    ops.iter()
        .find_map(|op| match op {
            OperationResult::OpInner(OperationResultTr::CreateClaimableBalance(
                CreateClaimableBalanceResult::Success(id),
            )) => Some(id.clone()),
            _ => None,
        })
        .with_context(|| {
            format!(
                "ledger {}: no CreateClaimableBalance success in the result",
                applied.ledger
            )
        })
}

pub(super) fn balance_key(id: &ClaimableBalanceId) -> LedgerKey {
    LedgerKey::ClaimableBalance(LedgerKeyClaimableBalance {
        balance_id: id.clone(),
    })
}

const CLAIMANTS: [&str; 10] = [
    "claimant-0",
    "claimant-1",
    "claimant-2",
    "claimant-3",
    "claimant-4",
    "claimant-5",
    "claimant-6",
    "claimant-7",
    "claimant-8",
    "claimant-9",
];

const CREATE_FIELDS: [&str; 3] = [
    "op.create_claimable_balance.claimants.destination",
    "op.create_claimable_balance.claimants.predicate",
    "op.create_claimable_balance.asset_amount",
];

impl Recipe for ClaimableBalances {
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
        let mut roles = vec!["creator"];
        roles.extend(CLAIMANTS);
        ctx.fund(&roles, BALANCE)?;
        let creator = ctx.account("creator");

        let preds = predicates()?;
        ensure!(
            preds.iter().map(depth).max() == Some(MAX_DEPTH),
            "the claimant predicates should reach depth {MAX_DEPTH}"
        );
        let claimants: Vec<Claimant> = CLAIMANTS
            .iter()
            .zip(preds)
            .map(|(role, predicate)| {
                Claimant::ClaimantTypeV0(ClaimantV0 {
                    destination: account_id(&ctx.account(role).verifying_key()),
                    predicate,
                })
            })
            .collect();
        let create = |claimants: Vec<Claimant>| -> Result<OperationBody> {
            Ok(OperationBody::CreateClaimableBalance(
                CreateClaimableBalanceOp {
                    asset: Asset::Native,
                    amount: AMOUNT,
                    claimants: claimants.try_into()?,
                },
            ))
        };

        let env = ctx.classic(&creator, vec![create(claimants)?])?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "create-ten-claimants-depth-4",
            Selector::transaction(&applied),
            Spec::new(
                "CreateClaimableBalance with ten claimants whose predicates use every arm and nest and, or and not down to depth 4, the deepest core accepts",
            )
            .fields(&CREATE_FIELDS)
            .flags(&["max-length-vec"]),
        )?;
        let id = created_balance_id(&applied)?;
        let key = balance_key(&id);
        ctx.capture(
            "entry-ten-claimants-depth-4",
            Selector::LedgerEntry {
                ledger: applied.ledger,
                key: key.clone(),
            },
            Spec::new(
                "the stored claimable balance with ten claimants, where core has rewritten every relative time bound into an absolute one",
            )
            .fields(&[
                "op.create_claimable_balance.claimants.destination",
                "op.create_claimable_balance.claimants.predicate",
            ])
            .flags(&["max-length-vec"]),
        )?;
        let stored = ctx
            .driver()
            .node()
            .ledger_entries(std::slice::from_ref(&key))?;
        match stored.entries[0].entry.as_ref().map(|e| &e.data) {
            Some(LedgerEntryData::ClaimableBalance(cb)) => {
                ensure!(
                    cb.claimants.len() == 10,
                    "the balance should hold ten claimants, holds {}",
                    cb.claimants.len()
                );
                let preds: Vec<&ClaimPredicate> = cb
                    .claimants
                    .iter()
                    .map(|Claimant::ClaimantTypeV0(c)| &c.predicate)
                    .collect();
                ensure!(
                    !preds.iter().any(|p| has_relative(p)),
                    "core should store relative time bounds as absolute ones: {preds:?}"
                );
                ensure!(
                    preds.iter().map(|p| depth(p)).max() == Some(MAX_DEPTH),
                    "the stored predicates should keep depth {MAX_DEPTH}"
                );
            }
            other => bail!("the claimable balance is missing: {other:?}"),
        }

        let claimer = ctx.account(CLAIMANTS[9]);
        let env = ctx.classic(
            &claimer,
            vec![OperationBody::ClaimClaimableBalance(
                ClaimClaimableBalanceOp { balance_id: id },
            )],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "claim-through-depth-4",
            Selector::transaction(&applied),
            Spec::new(
                "the claimant whose predicate is a depth 4 and, or, not tree claims the balance, which deletes the entry",
            )
            .fields(&["op.claim_claimable_balance.balance_id"]),
        )?;

        let deep = one_too_deep()?;
        ensure!(
            depth(&deep) == MAX_DEPTH + 1,
            "the twin should be one level deeper"
        );
        let twin = vec![Claimant::ClaimantTypeV0(ClaimantV0 {
            destination: account_id(&ctx.account(CLAIMANTS[0]).verifying_key()),
            predicate: deep,
        })];
        let env = ctx.classic(&creator, vec![create(twin)?])?;
        let r = ctx.capture_rejected(
            "create-predicate-depth-5",
            &env,
            Spec::new(
                "CreateClaimableBalance whose predicate reaches depth 5, one more than core accepts, refused with CREATE_CLAIMABLE_BALANCE_MALFORMED before it reaches a ledger",
            )
            .fields(&["op.create_claimable_balance.claimants.predicate"]),
        )?;
        ensure!(
            matches!(
                op_result(&r.result, 0)?,
                OperationResultTr::CreateClaimableBalance(CreateClaimableBalanceResult::Malformed)
            ),
            "core answered {:?} for a depth 5 predicate, expected create_claimable_balance malformed",
            r.result
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_counts_like_core() {
        assert_eq!(depth(&ClaimPredicate::Unconditional), 1);
        assert_eq!(depth(&not(abs(1))), 2);
        assert_eq!(depth(&deepest().unwrap()), MAX_DEPTH);
        assert_eq!(depth(&one_too_deep().unwrap()), MAX_DEPTH + 1);
        assert_eq!(predicates().unwrap().len(), 10);
    }
}
