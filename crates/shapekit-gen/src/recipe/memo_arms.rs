//! `classic.memo-arms`: id, hash and return memos at the edges of their
//! values. Core checks nothing beyond the xdr shape (a uint64 or 32 opaque
//! bytes; `TransactionFrame` has no content check on any memo arm,
//! stellar-core v30.0.0), so every one is accepted.
//!
//! From protocol 23 a memo also reaches the transfer event: for a payment
//! to a plain account `getPossibleMuxedData` (src/transactions/EventManager.cpp)
//! writes `{amount, to_muxed_id}` with the memo converted by
//! `makeClassicMemoSCVal` (u64 for an id, bytes for a hash or return), so an
//! indexer reading events sees a 2^64-1 memo id as if it were a mux id. For
//! a muxed destination the mux id wins and the memo is not in the event.
//!
//! | ledger  | content                                                        |
//! |---------|----------------------------------------------------------------|
//! | 2       | protocol upgrade                                               |
//! | 3       | root funds sender and sink                                     |
//! | 4 to 6  | payments with MEMO_ID 0, 2^63 and 2^64-1                       |
//! | 7 to 9  | MEMO_HASH all zero, all 0xff, sha256 of a fixed string         |
//! | 10 to 12| MEMO_RETURN all zero, all 0xff, sha256 of a fixed string       |
//! | 13      | MEMO_ID 2^64-1 on a payment to sink muxed with id 7            |

use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use shapekit_core_driver::chain::Applied;
use shapekit_core_driver::keys::{muxed_account, muxed_account_with_id};
use stellar_xdr::{
    Asset, ContractEventBody, Hash, Memo, MuxedAccount, OperationBody, PaymentOp, ScBytes, ScVal,
    TransactionMeta,
};

use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "classic.memo-arms";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

/// Mux id of the muxed destination, small so it cannot be mistaken for the
/// memo id riding in the same transaction.
const MUX_ID: u64 = 7;

pub struct MemoArms {
    pub protocol: u32,
}

fn digest(label: &str) -> [u8; 32] {
    Sha256::digest(label.as_bytes()).into()
}

/// Slug, memo, the memo's field id and the to_muxed_id core is expected to
/// write for a payment to a plain account, in ledger order.
fn memos() -> Result<Vec<(&'static str, Memo, &'static str, ScVal)>> {
    let bytes =
        |b: [u8; 32]| -> Result<ScVal> { Ok(ScVal::Bytes(ScBytes(b.to_vec().try_into()?))) };
    let mixed_hash = digest("shapekit/memo-arms/hash");
    let mixed_return = digest("shapekit/memo-arms/return");
    let mut v = Vec::new();
    for (slug, id) in [
        ("memo-id-zero", 0u64),
        ("memo-id-2-63", 1u64 << 63),
        ("memo-id-max", u64::MAX),
    ] {
        v.push((slug, Memo::Id(id), "tx.memo.id", ScVal::U64(id)));
    }
    for (slug, h) in [
        ("memo-hash-zero", [0u8; 32]),
        ("memo-hash-ff", [0xff; 32]),
        ("memo-hash-sha256", mixed_hash),
    ] {
        v.push((slug, Memo::Hash(Hash(h)), "tx.memo.hash", bytes(h)?));
    }
    for (slug, h) in [
        ("memo-return-zero", [0u8; 32]),
        ("memo-return-ff", [0xff; 32]),
        ("memo-return-sha256", mixed_return),
    ] {
        v.push((slug, Memo::Return(Hash(h)), "tx.memo.return", bytes(h)?));
    }
    Ok(v)
}

/// The data of the single transfer event of a one-payment transaction.
fn transfer_data(applied: &Applied) -> Result<ScVal> {
    let l = applied.ledger;
    let TransactionMeta::V4(m) = &applied.meta else {
        bail!("ledger {l}: expected TransactionMeta v4");
    };
    let events: Vec<_> = m.operations.iter().flat_map(|o| o.events.iter()).collect();
    ensure!(
        events.len() == 1,
        "ledger {l}: {} operation events, expected one transfer",
        events.len()
    );
    let ContractEventBody::V0(b) = &events[0].body;
    ensure!(
        matches!(b.topics.first(), Some(ScVal::Symbol(s)) if s.as_slice() == b"transfer"),
        "ledger {l}: the operation event is not a transfer: {:?}",
        b.topics.first()
    );
    Ok(b.data.clone())
}

/// The to_muxed_id of a transfer whose data must be `{amount, to_muxed_id}`.
fn to_muxed_id(applied: &Applied) -> Result<ScVal> {
    let data = transfer_data(applied)?;
    let ScVal::Map(Some(m)) = &data else {
        bail!(
            "ledger {}: transfer data is {data:?}, expected an {{amount, to_muxed_id}} map",
            applied.ledger
        );
    };
    m.iter()
        .find(|e| matches!(&e.key, ScVal::Symbol(s) if s.as_slice() == b"to_muxed_id"))
        .map(|e| e.val.clone())
        .with_context(|| {
            format!(
                "ledger {}: transfer data has no to_muxed_id",
                applied.ledger
            )
        })
}

fn payment(to: MuxedAccount) -> OperationBody {
    OperationBody::Payment(PaymentOp {
        destination: to,
        asset: Asset::Native,
        amount: 1,
    })
}

impl Recipe for MemoArms {
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
        ctx.fund(&["sender", "sink"], BALANCE)?;
        let sender = ctx.account("sender");
        let sink = ctx.account("sink");
        for (slug, memo, field, want) in memos()? {
            let tx = ctx
                .tx(&sender)?
                .memo(memo)
                .operation(None, payment(muxed_account(&sink.verifying_key())))
                .build()?;
            let env = ctx.sign(tx, &[&sender])?;
            let applied = ctx.submit_and_close(&env)?;
            let got = to_muxed_id(&applied)?;
            ensure!(
                got == want,
                "{slug}: to_muxed_id is {got:?}, expected the memo value {want:?}"
            );
            ctx.capture(
                slug,
                Selector::ledger_of(&applied),
                Spec::new(format!(
                    "an XLM payment to a plain account with {slug}, accepted by core; the memo value reappears as to_muxed_id in the transfer event"
                ))
                .fields(&["tx.memo.type", field]),
            )?;
        }

        let tx = ctx
            .tx(&sender)?
            .memo(Memo::Id(u64::MAX))
            .operation(
                None,
                payment(muxed_account_with_id(&sink.verifying_key(), MUX_ID)),
            )
            .build()?;
        let env = ctx.sign(tx, &[&sender])?;
        let applied = ctx.submit_and_close(&env)?;
        let got = to_muxed_id(&applied)?;
        ensure!(
            got == ScVal::U64(MUX_ID),
            "memo-id-muxed-destination: to_muxed_id is {got:?}, expected the mux id {MUX_ID}"
        );
        ctx.capture(
            "memo-id-muxed-destination",
            Selector::ledger_of(&applied),
            Spec::new(format!(
                "an XLM payment with MEMO_ID 2^64-1 to a muxed account with id {MUX_ID}; the transfer event's to_muxed_id is the mux id, the memo is not in the event"
            ))
            .fields(&["tx.memo.type", "tx.memo.id"]),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memo_values_cover_the_edges() {
        let m = memos().unwrap();
        assert_eq!(m.len(), 9);
        assert!(matches!(m[2].1, Memo::Id(u64::MAX)));
        assert!(matches!(&m[4].1, Memo::Hash(Hash(h)) if h == &[0xff; 32]));
        assert!(matches!(&m[6].1, Memo::Return(Hash(h)) if h == &[0u8; 32]));
    }
}
