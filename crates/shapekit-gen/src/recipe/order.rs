//! Two transactions in one ledger, applied in a chosen order.
//!
//! The transaction queue holds at most one transaction per source account
//! (`TransactionQueue::canAdd` answers `TRY_AGAIN_LATER` for a second one,
//! stellar-core v30.0.0), so a sequence chain cannot fix the order. Core
//! applies a classic phase account by account in batches, and sorts each
//! batch by the full envelope hash xored with the hash of the transaction
//! set (`sortedForApplySequential`, `ApplyTxSorter`, `lessThanXored`). The
//! set itself is a pure function of the previous ledger hash, the base fee
//! and the envelopes sorted by full hash (`sequentialPhaseToXdr`), so the
//! order is computable before submission. [`apply_in_order`] varies a memo
//! id on the second transaction until the predicted order puts the first
//! one first, submits both, and checks core built exactly the predicted set
//! and applied it in that order.

use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use shapekit_core_driver::chain::Applied;
use stellar_xdr::{
    GeneralizedTransactionSet, Hash, LedgerCloseMeta, Limits, TransactionEnvelope,
    TransactionPhase, TransactionSetV1, TxSetComponent, TxSetComponentTxsMaybeDiscountedFee,
    WriteXdr,
};

use super::ChainCtx;

/// Memo ids tried before giving up. Each try is a coin flip, so reaching
/// this means the prediction is wrong, not unlucky.
const MAX_VARIANTS: u64 = 64;

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn full_hash(env: &TransactionEnvelope) -> Result<[u8; 32]> {
    Ok(sha256(&env.to_xdr(Limits::none())?))
}

/// The base fee of the classic component and the Soroban phase of a ledger
/// that held classic transactions only, to build the next set from.
struct Template {
    base_fee: Option<i64>,
    soroban_phase: TransactionPhase,
}

fn template(lcm: &LedgerCloseMeta) -> Result<Template> {
    let LedgerCloseMeta::V2(m) = lcm else {
        bail!("ordering needs a v2 ledger close meta");
    };
    let GeneralizedTransactionSet::V1(set) = &m.tx_set;
    let [classic, soroban] = set.phases.as_slice() else {
        bail!(
            "tx set with {} phases, expected classic and soroban",
            set.phases.len()
        );
    };
    let TransactionPhase::V0(components) = classic else {
        bail!("the classic phase is not a v0 phase");
    };
    let [TxSetComponent::TxsetCompTxsMaybeDiscountedFee(c)] = components.as_slice() else {
        bail!(
            "the template ledger's classic phase has {} components, expected one",
            components.len()
        );
    };
    ensure!(
        !c.txs.is_empty(),
        "the template ledger holds no classic transaction"
    );
    Ok(Template {
        base_fee: c.base_fee,
        soroban_phase: soroban.clone(),
    })
}

/// The set core builds from `envs` on top of `previous`, and the indices of
/// `envs` in the order core applies them.
fn predict(
    previous: &Hash,
    t: &Template,
    envs: &[TransactionEnvelope],
) -> Result<(GeneralizedTransactionSet, Vec<usize>)> {
    let mut by_hash: Vec<([u8; 32], usize)> = envs
        .iter()
        .enumerate()
        .map(|(i, e)| Ok((full_hash(e)?, i)))
        .collect::<Result<_>>()?;
    by_hash.sort();
    let txs: Vec<TransactionEnvelope> = by_hash.iter().map(|(_, i)| envs[*i].clone()).collect();
    let set = GeneralizedTransactionSet::V1(TransactionSetV1 {
        previous_ledger_hash: previous.clone(),
        phases: vec![
            TransactionPhase::V0(
                vec![TxSetComponent::TxsetCompTxsMaybeDiscountedFee(
                    TxSetComponentTxsMaybeDiscountedFee {
                        base_fee: t.base_fee,
                        txs: txs.try_into()?,
                    },
                )]
                .try_into()?,
            ),
            t.soroban_phase.clone(),
        ]
        .try_into()?,
    });
    let set_hash = sha256(&set.to_xdr(Limits::none())?);
    let xored = |h: &[u8; 32]| -> [u8; 32] { std::array::from_fn(|i| h[i] ^ set_hash[i]) };
    let mut order: Vec<([u8; 32], usize)> = by_hash.iter().map(|(h, i)| (xored(h), *i)).collect();
    order.sort();
    Ok((set, order.into_iter().map(|(_, i)| i).collect()))
}

/// Builds the pair for memo id variants 0, 1, ... until core would apply
/// `pair(v)[0]` before `pair(v)[1]`, then applies both in one ledger. The
/// last closed ledger must hold classic transactions only, at the base fee
/// the pair pays. Each transaction needs its own source account. Returns
/// what core applied, first transaction first.
pub(crate) fn apply_in_order(
    ctx: &mut ChainCtx,
    mut pair: impl FnMut(&ChainCtx, u64) -> Result<[TransactionEnvelope; 2]>,
) -> Result<Vec<Applied>> {
    let last = ctx.driver().last_meta().clone();
    let t = template(&last).context("reading the last ledger's tx set")?;
    let LedgerCloseMeta::V2(m) = &last else {
        bail!("ordering needs a v2 ledger close meta");
    };
    let previous = m.ledger_header.hash.clone();
    for variant in 0..MAX_VARIANTS {
        let envs = pair(ctx, variant)?;
        let (set, order) = predict(&previous, &t, &envs)?;
        if order != [0, 1] {
            continue;
        }
        let applied = ctx.driver().apply_together(&envs)?;
        let LedgerCloseMeta::V2(got) = ctx.driver().last_meta() else {
            bail!("ordering needs a v2 ledger close meta");
        };
        ensure!(
            got.tx_set == set,
            "ledger {}: core built a different tx set than predicted",
            applied[0].ledger
        );
        let applied_hashes: Vec<[u8; 32]> = got
            .tx_processing
            .iter()
            .map(|t| t.result.transaction_hash.0)
            .collect();
        ensure!(
            applied_hashes == [applied[0].hash, applied[1].hash],
            "ledger {}: core applied the pair in another order than predicted",
            applied[0].ledger
        );
        return Ok(applied);
    }
    bail!("no memo id below {MAX_VARIANTS} orders the pair as wanted")
}
