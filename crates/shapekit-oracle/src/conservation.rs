//! Conservation of lumens, checked from meta alone.
//!
//! The rule is core's own `ConservationOfLumens` invariant (src/invariant/
//! ConservationOfLumens.cpp at v30.0.0): lumens live in account balances, native claimable
//! balances, native pool reserves and native Stellar Asset Contract balance entries
//! (`getAssetBalance`, TransactionUtils.cpp), plus the header's fee pool, and their sum is
//! the header's `totalCoins`. Only an inflation operation (gone since protocol 12) changes
//! `totalCoins`. The fee pool moves only by fees: `processFeeSeqNum` adds the fee charged
//! up front, `refundSorobanFee` subtracts the refund, and the result's `feeCharged` is the
//! net of the two. One exception is a mainnet-only fee pool credit of 31879035 stroops in
//! the protocol 23 to 24 upgrade (herder/Upgrades.cpp), which restores lumens burned by a
//! protocol 23 bug and so breaks the equation in that one ledger.
//!
//! Checked here:
//!
//! 1. per transaction: the native delta over its fee processing, apply and post-apply
//!    lists plus its `feeCharged` is zero, unless it ran an inflation operation;
//! 2. per ledger: the native delta of all transactions and upgrades plus the sum of
//!    `feeCharged` is zero, unless an inflation operation ran;
//! 3. when the previous header is known: `feePool` grew by exactly the sum of `feeCharged`
//!    and `totalCoins` did not move (again unless inflation ran), which together with 2 is
//!    the full invariant `sum(holdings) + feePool = totalCoins`.

use stellar_xdr::{
    LedgerCloseMeta, LedgerHeader, OperationResult, OperationResultTr, TransactionResultResult,
};

use crate::interpret::{Context, diffs, native_amount};
use crate::network::Network;
use crate::view::{LedgerView, ViewError};

#[derive(Debug, thiserror::Error)]
pub enum ConservationError {
    #[error(transparent)]
    View(#[from] ViewError),
    #[error("ledger {seq} tx {tx}: native balances moved by {delta} but feeCharged is {fee}")]
    Tx {
        seq: u32,
        tx: String,
        delta: i128,
        fee: i128,
    },
    #[error("ledger {seq}: native balances moved by {delta} but fees charged total {fees}")]
    Ledger { seq: u32, delta: i128, fees: i128 },
    #[error("ledger {seq}: feePool moved by {pool_delta} but fees charged total {fees}")]
    FeePool {
        seq: u32,
        pool_delta: i128,
        fees: i128,
    },
    #[error("ledger {seq}: totalCoins moved by {delta} without inflation")]
    TotalCoins { seq: u32, delta: i128 },
    #[error("ledger {seq} does not follow the given previous header {prev}")]
    NotNext { seq: u32, prev: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConservationReport {
    pub ledger_seq: u32,
    pub native_delta: i128,
    pub fees_charged: i128,
    pub transactions_checked: usize,
    /// Transactions skipped because they ran inflation.
    pub inflation_txs: usize,
    pub header_checked: bool,
}

pub fn check_ledger(
    lcm: &LedgerCloseMeta,
    network: &Network,
    prev: Option<&LedgerHeader>,
) -> Result<ConservationReport, ConservationError> {
    let view = LedgerView::new(lcm, network)?;
    let ctx = Context::build(&view, network);
    let seq = view.seq();
    let mut ledger_delta = 0i128;
    let mut fees = 0i128;
    let mut inflation_txs = 0;
    for tx in &view.txs {
        let mut delta = 0i128;
        for list in tx.all_lists() {
            for d in diffs(list.changes) {
                delta += native_amount(&ctx, d.after) - native_amount(&ctx, d.before);
            }
        }
        let fee = i128::from(tx.result.result.fee_charged);
        ledger_delta += delta;
        fees += fee;
        if ran_inflation(&tx.result.result.result) {
            inflation_txs += 1;
            continue;
        }
        if delta + fee != 0 {
            return Err(ConservationError::Tx {
                seq,
                tx: hex::encode(tx.result.transaction_hash.0),
                delta,
                fee,
            });
        }
    }
    for up in view.upgrades {
        for d in diffs(&up.changes) {
            ledger_delta += native_amount(&ctx, d.after) - native_amount(&ctx, d.before);
        }
    }
    if inflation_txs == 0 && ledger_delta + fees != 0 {
        return Err(ConservationError::Ledger {
            seq,
            delta: ledger_delta,
            fees,
        });
    }
    let mut header_checked = false;
    if let Some(prev) = prev {
        if prev.ledger_seq.checked_add(1) != Some(seq) {
            return Err(ConservationError::NotNext {
                seq,
                prev: prev.ledger_seq,
            });
        }
        if inflation_txs == 0 {
            let pool_delta = i128::from(view.header.fee_pool) - i128::from(prev.fee_pool);
            if pool_delta != fees {
                return Err(ConservationError::FeePool {
                    seq,
                    pool_delta,
                    fees,
                });
            }
            let coins = i128::from(view.header.total_coins) - i128::from(prev.total_coins);
            if coins != 0 {
                return Err(ConservationError::TotalCoins { seq, delta: coins });
            }
            header_checked = true;
        }
    }
    Ok(ConservationReport {
        ledger_seq: seq,
        native_delta: ledger_delta,
        fees_charged: fees,
        transactions_checked: view.txs.len() - inflation_txs,
        inflation_txs,
        header_checked,
    })
}

fn ran_inflation(r: &TransactionResultResult) -> bool {
    let ops: &[OperationResult] = match r {
        TransactionResultResult::TxSuccess(ops) | TransactionResultResult::TxFailed(ops) => ops,
        TransactionResultResult::TxFeeBumpInnerSuccess(p)
        | TransactionResultResult::TxFeeBumpInnerFailed(p) => match &p.result.result {
            stellar_xdr::InnerTransactionResultResult::TxSuccess(ops)
            | stellar_xdr::InnerTransactionResultResult::TxFailed(ops) => ops,
            _ => &[],
        },
        _ => &[],
    };
    ops.iter().any(|o| {
        matches!(
            o,
            OperationResult::OpInner(OperationResultTr::Inflation(
                stellar_xdr::InflationResult::Success(_)
            ))
        )
    })
}
