//! `cap27.muxed-accounts`: CAP-27 muxed accounts in every position core
//! accepts them on the classic path, with mux ids 0, 1, 2^53 and 2^64-1.
//!
//! | ledger   | content                                                         |
//! |----------|-----------------------------------------------------------------|
//! | 2        | protocol upgrade                                                |
//! | 3        | root funds alice, bob, issuer, holder and leaver                |
//! | 4 to 7   | muxed alice pays muxed bob, one ledger per mux id               |
//! | 8        | four payments whose operation sources are muxed alice, one per id |
//! | 9        | path payment strict receive to muxed bob (id 2^64-1)            |
//! | 10       | path payment strict send to muxed bob (id 1)                    |
//! | 11       | issuer enables revocation and clawback                          |
//! | 12       | holder trusts the issuer's asset                                |
//! | 13       | issuer pays muxed holder (id 2^53)                              |
//! | 14       | issuer claws back from muxed holder (id 2^64-1)                 |
//! | 15       | leaver merges into muxed alice (id 2^53)                        |
//!
//! Core resolves a muxed account to its ed25519 key and drops the id, so
//! the ids appear only in envelopes. A transaction's meta carries no muxed
//! account, which is why every capture here is the whole ledger: its
//! transaction set holds the envelope and its meta the effects.

use anyhow::{Result, ensure};
use ed25519_dalek::SigningKey;
use shapekit_core_driver::keys::{account_id, muxed_account, muxed_account_with_id};
use shapekit_core_driver::tx::{TransactionBuilder, sign_transaction};
use stellar_xdr::{
    AlphaNum4, Asset, AssetCode4, ChangeTrustAsset, ChangeTrustOp, ClawbackOp, LedgerEntryData,
    LedgerKey, LedgerKeyTrustLine, MuxedAccount, OperationBody, PathPaymentStrictReceiveOp,
    PathPaymentStrictSendOp, PaymentOp, SetOptionsOp, TransactionEnvelope, TrustLineAsset,
};

use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "cap27.muxed-accounts";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

/// Payment amount, 1 XLM or one unit of the issued asset.
const AMOUNT: i64 = 10_000_000;

/// The mux ids the family covers: both ends of the range and the first
/// value a float64 cannot hold exactly.
const IDS: [(u64, &str); 4] = [(0, "0"), (1, "1"), (1 << 53, "2p53"), (u64::MAX, "2p64-1")];

/// `AUTH_REVOCABLE_FLAG | AUTH_CLAWBACK_ENABLED_FLAG`; clawback needs both.
const CLAWBACK_FLAGS: u32 = 0x2 | 0x8;

/// The family's recipe at one protocol. Protocol 30 is the current tier;
/// protocol 28 is what the pre-fix wallet-backend commits read.
pub struct MuxedAccounts {
    pub protocol: u32,
}

fn mux(key: &SigningKey, id: u64) -> MuxedAccount {
    muxed_account_with_id(&key.verifying_key(), id)
}

fn payment(to: MuxedAccount, asset: Asset) -> OperationBody {
    OperationBody::Payment(PaymentOp {
        destination: to,
        asset,
        amount: AMOUNT,
    })
}

fn set_flags(flags: u32) -> OperationBody {
    OperationBody::SetOptions(SetOptionsOp {
        inflation_dest: None,
        clear_flags: None,
        set_flags: Some(flags),
        master_weight: None,
        low_threshold: None,
        med_threshold: None,
        high_threshold: None,
        home_domain: None,
        signer: None,
    })
}

/// A one-operation transaction from `tx_source`, a muxed form of
/// `signer`'s account, signed by `signer`.
fn from_muxed(
    ctx: &mut ChainCtx,
    signer: &SigningKey,
    tx_source: MuxedAccount,
    op: OperationBody,
) -> Result<TransactionEnvelope> {
    let seq = ctx
        .driver()
        .node()
        .account_seq(&account_id(&signer.verifying_key()))?;
    let tx = TransactionBuilder::new(tx_source, seq + 1)
        .operation(None, op)
        .build()?;
    Ok(TransactionEnvelope::Tx(sign_transaction(
        tx,
        &ctx.network_id(),
        &[signer],
    )?))
}

impl Recipe for MuxedAccounts {
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
        ctx.fund(&["alice", "bob", "issuer", "holder", "leaver"], BALANCE)?;
        let alice = ctx.account("alice");
        let bob = ctx.account("bob");
        let issuer = ctx.account("issuer");
        let holder = ctx.account("holder");
        let leaver = ctx.account("leaver");

        for (id, name) in IDS {
            let env = from_muxed(
                ctx,
                &alice,
                mux(&alice, id),
                payment(mux(&bob, id), Asset::Native),
            )?;
            let applied = ctx.submit_and_close(&env)?;
            ctx.capture(
                &format!("tx-source-and-payment-destination-id-{name}"),
                Selector::ledger_of(&applied),
                Spec::new(format!(
                    "a payment whose transaction source and destination are both muxed accounts with id {name}"
                ))
                .fields(&[
                    "tx.source_account",
                    "tx.source_account.mux_id",
                    "op.payment.destination",
                    "op.payment.destination.mux_id",
                ]),
            )?;
        }

        // every operation names the transaction's own account under a
        // different id, so one signature covers all four
        let mut b = ctx.tx(&alice)?;
        for (id, _) in IDS {
            b = b.operation(
                Some(mux(&alice, id)),
                payment(muxed_account(&bob.verifying_key()), Asset::Native),
            );
        }
        let env = ctx.sign(b.build()?, &[&alice])?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "operation-sources-four-ids",
            Selector::ledger_of(&applied),
            Spec::new(
                "four payments whose operation sources are the transaction source's account as muxed accounts with ids 0, 1, 2^53 and 2^64-1",
            )
            .fields(&["op.source_account", "op.source_account.mux_id"]),
        )?;

        // native to native with an empty path needs no order book
        let env = ctx.classic(
            &alice,
            vec![OperationBody::PathPaymentStrictReceive(
                PathPaymentStrictReceiveOp {
                    send_asset: Asset::Native,
                    send_max: AMOUNT,
                    destination: mux(&bob, u64::MAX),
                    dest_asset: Asset::Native,
                    dest_amount: AMOUNT,
                    path: Vec::new().try_into()?,
                },
            )],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "path-payment-strict-receive-destination-id-2p64-1",
            Selector::ledger_of(&applied),
            Spec::new(
                "a path payment strict receive whose destination is a muxed account with id 2^64-1",
            )
            .fields(&[
                "op.path_payment.destination",
                "op.path_payment.destination.mux_id",
            ]),
        )?;

        let env = ctx.classic(
            &alice,
            vec![OperationBody::PathPaymentStrictSend(
                PathPaymentStrictSendOp {
                    send_asset: Asset::Native,
                    send_amount: AMOUNT,
                    destination: mux(&bob, 1),
                    dest_asset: Asset::Native,
                    dest_min: AMOUNT,
                    path: Vec::new().try_into()?,
                },
            )],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "path-payment-strict-send-destination-id-1",
            Selector::ledger_of(&applied),
            Spec::new("a path payment strict send whose destination is a muxed account with id 1")
                .fields(&[
                    "op.path_payment.destination",
                    "op.path_payment.destination.mux_id",
                ]),
        )?;

        let asset = Asset::CreditAlphanum4(AlphaNum4 {
            asset_code: AssetCode4(*b"MUXD"),
            issuer: account_id(&issuer.verifying_key()),
        });
        let env = ctx.classic(&issuer, vec![set_flags(CLAWBACK_FLAGS)])?;
        ctx.submit_and_close(&env)?;
        let line = ChangeTrustAsset::CreditAlphanum4(AlphaNum4 {
            asset_code: AssetCode4(*b"MUXD"),
            issuer: account_id(&issuer.verifying_key()),
        });
        let env = ctx.classic(
            &holder,
            vec![OperationBody::ChangeTrust(ChangeTrustOp {
                line,
                limit: i64::MAX,
            })],
        )?;
        ctx.submit_and_close(&env)?;

        let env = ctx.classic(&issuer, vec![payment(mux(&holder, 1 << 53), asset.clone())])?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "issued-payment-destination-id-2p53",
            Selector::ledger_of(&applied),
            Spec::new("an issuer pays its clawback-enabled asset to a muxed account with id 2^53")
                .fields(&[
                    "op.payment.destination",
                    "op.payment.destination.mux_id",
                    "op.payment.asset",
                ]),
        )?;

        let env = ctx.classic(
            &issuer,
            vec![OperationBody::Clawback(ClawbackOp {
                asset,
                from: mux(&holder, u64::MAX),
                amount: AMOUNT,
            })],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "clawback-from-id-2p64-1",
            Selector::ledger_of(&applied),
            Spec::new(
                "an issuer claws its asset back from a holder named as a muxed account with id 2^64-1",
            )
            .fields(&["op.clawback.from", "op.clawback.from.mux_id"]),
        )?;
        let holder_key = account_id(&holder.verifying_key());
        let trust =
            ctx.driver()
                .node()
                .ledger_entries(&[LedgerKey::Trustline(LedgerKeyTrustLine {
                    account_id: holder_key,
                    asset: TrustLineAsset::CreditAlphanum4(AlphaNum4 {
                        asset_code: AssetCode4(*b"MUXD"),
                        issuer: account_id(&issuer.verifying_key()),
                    }),
                })])?;
        match trust.entries[0].entry.as_ref().map(|e| &e.data) {
            Some(LedgerEntryData::Trustline(t)) => ensure!(
                t.balance == 0,
                "the clawback should leave the holder with nothing, balance is {}",
                t.balance
            ),
            other => anyhow::bail!("the holder's trustline is missing: {other:?}"),
        }

        let env = ctx.classic(
            &leaver,
            vec![OperationBody::AccountMerge(mux(&alice, 1 << 53))],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "account-merge-destination-id-2p53",
            Selector::ledger_of(&applied),
            Spec::new("an account merges into a muxed account with id 2^53")
                .fields(&["op.account_merge.destination"]),
        )
    }
}
