//! `classic.memo-bytes`: text memos whose bytes are not valid UTF-8. The
//! xdr declares `string text<28>`, and core checks only that bound
//! (`TransactionFrame` has no content check on memo text, stellar-core
//! v30.0.0), so each of these is accepted and closes in a ledger.
//!
//! | ledger | memo bytes (hex)                       | why it is not UTF-8                 |
//! |--------|----------------------------------------|-------------------------------------|
//! | 2      | protocol upgrade                       |                                     |
//! | 3      | root funds sender and sink             |                                     |
//! | 4      | `ff`                                   | never valid; js-stellar-sdk#1823    |
//! | 5      | `80`                                   | continuation byte with no lead      |
//! | 6      | `c080`                                 | overlong encoding of NUL            |
//! | 7      | `eda080`                               | UTF-16 surrogate U+D800             |
//! | 8      | `f4908080`                             | above U+10FFFF                      |
//! | 9      | 26 ascii bytes then `e282`, 28 bytes   | a three byte sequence cut by the bound |
//!
//! js-stellar-sdk#1823: `Memo` decoded from `0000000100000001ff000000`
//! exposes U+FFFD and re-encodes as `0000000100000003efbfbd00`, so the
//! memo an indexer stores is not the memo the ledger holds.

use anyhow::{Result, ensure};
use shapekit_core_driver::keys::muxed_account;
use stellar_xdr::{Asset, Memo, OperationBody, PaymentOp, StringM};

use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "classic.memo-bytes";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

pub struct MemoBytes {
    pub protocol: u32,
}

/// Slug and bytes of each memo, in ledger order.
fn memos() -> Vec<(&'static str, Vec<u8>)> {
    let mut cut = b"shapekit memo cut at byte ".to_vec();
    cut.extend_from_slice(&[0xe2, 0x82]);
    vec![
        ("memo-ff", vec![0xff]),
        ("memo-lone-continuation", vec![0x80]),
        ("memo-overlong-nul", vec![0xc0, 0x80]),
        ("memo-surrogate", vec![0xed, 0xa0, 0x80]),
        ("memo-above-max-code-point", vec![0xf4, 0x90, 0x80, 0x80]),
        ("memo-truncated-sequence-28", cut),
    ]
}

impl Recipe for MemoBytes {
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
        for (slug, bytes) in memos() {
            ensure!(
                std::str::from_utf8(&bytes).is_err(),
                "memo {slug} is valid UTF-8"
            );
            let hex = hex::encode(&bytes);
            let len = bytes.len();
            let tx = ctx
                .tx(&sender)?
                .memo(Memo::Text(StringM::try_from(bytes)?))
                .operation(
                    None,
                    OperationBody::Payment(PaymentOp {
                        destination: muxed_account(&sink.verifying_key()),
                        asset: Asset::Native,
                        amount: 1,
                    }),
                )
                .build()?;
            let env = ctx.sign(tx, &[&sender])?;
            let applied = ctx.submit_and_close(&env)?;
            ctx.capture(
                slug,
                Selector::ledger_of(&applied),
                Spec::new(format!(
                    "a payment whose {len} byte text memo is {hex}, not valid UTF-8, accepted by core"
                ))
                .fields(&["tx.memo.type", "tx.memo.text"])
                .flags(&["non-utf8"]),
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncated_memo_is_at_the_bound() {
        let m = memos();
        let (_, cut) = m.last().unwrap();
        assert_eq!(cut.len(), 28);
    }
}
