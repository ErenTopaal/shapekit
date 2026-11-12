//! `cap03.offer-liabilities`: offers that put buying and selling
//! liabilities on accounts and trustlines (CAP-3), a manage-buy offer
//! (CAP-6) that crosses the book and rests, an offer updated by id, and an
//! account merge that fails with `ACCOUNT_MERGE_DEST_FULL` because the
//! destination's native buying liabilities leave no room for the balance.
//!
//! | ledger | content                                                            |
//! |--------|--------------------------------------------------------------------|
//! | 2      | protocol upgrade                                                   |
//! | 3      | root funds issuer, maker, taker, whale and merger                  |
//! | 4 to 6 | maker, taker and whale trust USD                                   |
//! | 7      | issuer pays maker and taker 1000 USD, whale i64::MAX stroops of USD |
//! | 8      | maker sells 100 XLM for USD at 1: native selling liabilities       |
//! | 9      | maker sells 100 USD for XLM at 2: USD selling liabilities          |
//! | 10     | taker buys 150 XLM for USD at 1: crosses maker's XLM offer, rests  |
//! | 11     | maker updates its USD offer by id to 60 USD                        |
//! | 12     | whale bids for XLM up to 0.1 XLM short of i64::MAX                 |
//! | 13     | merger merges into whale and fails with ACCOUNT_MERGE_DEST_FULL    |
//! | 14     | root funds maker2, maker3, maker4 and taker2                       |
//! | 15     | issuer offers its own GBP for XLM                                  |
//! | 16 to 20 | the new accounts trust EUR and GBP; issuer pays maker4 and taker2 |
//! | 21, 22 | maker4 offers 100 GBP for EUR; taker2 sells 40 EUR into it         |
//! | 23, 24 | maker2 and maker3 offer 50 and 100 XLM for EUR                     |
//! | 25     | taker2 bids for 80 XLM: maker2's offer is removed, maker3's shrinks |
//!
//! Ledgers 15, 22 and 25 give the same per-operation change kinds as core's
//! offer goldens (`OfferTests`), which the structural agreement check
//! compares.
//!
//! Core's checks: `ManageOfferOpFrameBase` computes liabilities and refuses
//! an offer whose buying side would exceed `getMaxAmountReceive`;
//! `MergeOpFrame::doApply` returns `ACCOUNT_MERGE_DEST_FULL` when
//! `addBalance` on the destination fails, which for native counts buying
//! liabilities against `INT64_MAX` (stellar-core v30.0.0).

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use shapekit_core_driver::keys::{account_id, muxed_account};
use stellar_xdr::{
    AccountEntryExt, AccountMergeResult, AlphaNum4, Asset, AssetCode4, ChangeTrustAsset,
    ChangeTrustOp, LedgerEntryData, LedgerKey, LedgerKeyAccount, ManageBuyOfferOp,
    ManageBuyOfferResult, ManageOfferSuccessResultOffer, ManageSellOfferOp, ManageSellOfferResult,
    OperationBody, OperationResult, OperationResultTr, PaymentOp, Price, TransactionResultResult,
};

use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "cap03.offer-liabilities";

/// Starting balance of every funded account but the merger, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

/// The merger only needs more than [`HEADROOM`], 100 XLM.
const MERGER_BALANCE: i64 = 1_000_000_000;

/// One unit of USD or XLM in stroops.
const UNIT: i64 = 10_000_000;

/// What the whale's bid leaves between its balance plus buying liabilities
/// and `INT64_MAX`, 0.1 XLM: far less than the merger brings.
const HEADROOM: i64 = 1_000_000;

pub struct OfferLiabilities {
    pub protocol: u32,
}

fn credit(code: &[u8; 4], issuer: &SigningKey) -> Asset {
    Asset::CreditAlphanum4(AlphaNum4 {
        asset_code: AssetCode4(*code),
        issuer: account_id(&issuer.verifying_key()),
    })
}

fn trust(asset: &Asset) -> Result<OperationBody> {
    let Asset::CreditAlphanum4(a) = asset else {
        bail!("trust line to {asset:?} is not used here");
    };
    Ok(OperationBody::ChangeTrust(ChangeTrustOp {
        line: ChangeTrustAsset::CreditAlphanum4(a.clone()),
        limit: i64::MAX,
    }))
}

fn pay(to: &SigningKey, asset: &Asset, amount: i64) -> OperationBody {
    OperationBody::Payment(PaymentOp {
        destination: muxed_account(&to.verifying_key()),
        asset: asset.clone(),
        amount,
    })
}

fn sell(selling: &Asset, buying: &Asset, amount: i64, price: Price, id: i64) -> OperationBody {
    OperationBody::ManageSellOffer(ManageSellOfferOp {
        selling: selling.clone(),
        buying: buying.clone(),
        amount,
        price,
        offer_id: id,
    })
}

fn buy(selling: &Asset, buying: &Asset, amount: i64, price: Price) -> OperationBody {
    OperationBody::ManageBuyOffer(ManageBuyOfferOp {
        selling: selling.clone(),
        buying: buying.clone(),
        buy_amount: amount,
        price,
        offer_id: 0,
    })
}

const ONE: Price = Price { n: 1, d: 1 };

fn op_result(r: &TransactionResultResult) -> Result<&OperationResultTr> {
    let TransactionResultResult::TxSuccess(ops) = r else {
        bail!("expected tx_success, core answered {r:?}");
    };
    match ops.first() {
        Some(OperationResult::OpInner(tr)) => Ok(tr),
        other => bail!("expected an inner operation result, got {other:?}"),
    }
}

/// The offer a single manage-offer operation left resting, with the number
/// of offers it crossed.
fn resting(r: &TransactionResultResult) -> Result<(ManageOfferSuccessResultOffer, usize)> {
    let s = match op_result(r)? {
        OperationResultTr::ManageSellOffer(ManageSellOfferResult::Success(s))
        | OperationResultTr::ManageBuyOffer(ManageBuyOfferResult::Success(s)) => s,
        other => bail!("expected a successful manage offer, got {other:?}"),
    };
    Ok((s.offer.clone(), s.offers_claimed.len()))
}

fn created_id(offer: &ManageOfferSuccessResultOffer) -> Result<i64> {
    match offer {
        ManageOfferSuccessResultOffer::Created(o) => Ok(o.offer_id),
        other => bail!("expected a created offer, got {other:?}"),
    }
}

fn native_balance_and_buying(ctx: &mut ChainCtx, who: &SigningKey) -> Result<(i64, i64)> {
    let key = LedgerKey::Account(LedgerKeyAccount {
        account_id: account_id(&who.verifying_key()),
    });
    let got = ctx
        .driver()
        .node()
        .ledger_entries(std::slice::from_ref(&key))?;
    match got.entries[0].entry.as_ref().map(|e| &e.data) {
        Some(LedgerEntryData::Account(a)) => {
            let buying = match &a.ext {
                AccountEntryExt::V1(v1) => v1.liabilities.buying,
                AccountEntryExt::V0 => 0,
            };
            Ok((a.balance, buying))
        }
        other => bail!("account entry missing: {other:?}"),
    }
}

impl Recipe for OfferLiabilities {
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
        ctx.fund(&["issuer", "maker", "taker", "whale"], BALANCE)?;
        ctx.fund(&["merger"], MERGER_BALANCE)?;
        let issuer = ctx.account("issuer");
        let maker = ctx.account("maker");
        let taker = ctx.account("taker");
        let whale = ctx.account("whale");
        let merger = ctx.account("merger");
        let usd = credit(b"USD\0", &issuer);
        let xlm = Asset::Native;

        for who in [&maker, &taker, &whale] {
            let env = ctx.classic(who, vec![trust(&usd)?])?;
            ctx.submit_and_close(&env)?;
        }
        let env = ctx.classic(
            &issuer,
            vec![
                pay(&maker, &usd, 1000 * UNIT),
                pay(&taker, &usd, 1000 * UNIT),
                pay(&whale, &usd, i64::MAX),
            ],
        )?;
        ctx.submit_and_close(&env)?;

        let env = ctx.classic(&maker, vec![sell(&xlm, &usd, 100 * UNIT, ONE, 0)])?;
        let applied = ctx.submit_and_close(&env)?;
        let (offer, crossed) = resting(&applied.result.result)?;
        created_id(&offer)?;
        ensure!(crossed == 0, "maker's XLM offer crossed {crossed} offers");
        ctx.capture(
            "sell-native-selling-liabilities",
            Selector::ledger_of(&applied),
            Spec::new(
                "a resting offer selling XLM, which gives the account entry its v1 extension with native selling liabilities",
            )
            .fields(&["op.manage_offer.offer_id", "op.manage_offer.assets_amount_price"]),
        )?;

        let env = ctx.classic(
            &maker,
            vec![sell(&usd, &xlm, 100 * UNIT, Price { n: 2, d: 1 }, 0)],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        let (offer, _) = resting(&applied.result.result)?;
        let usd_offer = created_id(&offer)?;
        ctx.capture(
            "sell-credit-trustline-liabilities",
            Selector::ledger_of(&applied),
            Spec::new(
                "a resting offer selling USD, which gives the trustline its v1 extension with selling liabilities and adds native buying liabilities to the account",
            )
            .fields(&["op.manage_offer.offer_id", "op.manage_offer.assets_amount_price"]),
        )?;

        let env = ctx.classic(&taker, vec![buy(&usd, &xlm, 150 * UNIT, ONE)])?;
        let applied = ctx.submit_and_close(&env)?;
        let (offer, crossed) = resting(&applied.result.result)?;
        created_id(&offer)?;
        ensure!(
            crossed == 1,
            "taker's bid crossed {crossed} offers, expected the maker's XLM offer"
        );
        ctx.capture(
            "buy-offer-crosses-and-rests",
            Selector::transaction(&applied),
            Spec::new(
                "a manage-buy offer for 150 XLM that takes the maker's 100 XLM offer from the book and rests with the remaining 50, so its result holds an order-book claim atom and a created offer",
            )
            .fields(&["op.manage_offer.offer_id", "op.manage_offer.assets_amount_price"]),
        )?;

        let env = ctx.classic(
            &maker,
            vec![sell(&usd, &xlm, 60 * UNIT, Price { n: 2, d: 1 }, usd_offer)],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        let (offer, _) = resting(&applied.result.result)?;
        ensure!(
            matches!(offer, ManageOfferSuccessResultOffer::Updated(ref o) if o.offer_id == usd_offer),
            "updating offer {usd_offer} gave {offer:?}"
        );
        ctx.capture(
            "update-offer-by-id",
            Selector::ledger_of(&applied),
            Spec::new(
                "the maker lowers its USD offer to 60 by offer id, so the offer and the trustline's selling liabilities shrink in place",
            )
            .fields(&["op.manage_offer.offer_id", "op.manage_offer.assets_amount_price"]),
        )?;

        // the bid is sized from the whale's balance once the offer's fee is
        // charged, leaving exactly HEADROOM below INT64_MAX
        let (balance, buying) = native_balance_and_buying(ctx, &whale)?;
        ensure!(buying == 0, "whale already has buying liabilities {buying}");
        let fee = i64::from(shapekit_core_driver::tx::BASE_FEE);
        let bid = i64::MAX - (balance - fee) - HEADROOM;
        let env = ctx.classic(&whale, vec![buy(&usd, &xlm, bid, ONE)])?;
        let applied = ctx.submit_and_close(&env)?;
        created_id(&resting(&applied.result.result)?.0)?;
        ctx.capture(
            "buy-offer-near-int64-max",
            Selector::transaction(&applied),
            Spec::new(
                "a manage-buy offer whose native buying liabilities bring balance plus liabilities to 0.1 XLM below INT64_MAX",
            )
            .fields(&["op.manage_offer.offer_id", "op.manage_offer.assets_amount_price"]),
        )?;
        let (after, buying) = native_balance_and_buying(ctx, &whale)?;
        ensure!(
            buying == bid && i64::MAX - after - buying == HEADROOM,
            "whale holds balance {after} and buying liabilities {buying}, expected {bid} with {HEADROOM} headroom"
        );

        let env = ctx.classic(
            &merger,
            vec![OperationBody::AccountMerge(muxed_account(
                &whale.verifying_key(),
            ))],
        )?;
        let applied = ctx.submit_and_close_failing(&env)?;
        ensure_dest_full(&applied.result.result)?;
        ctx.capture(
            "merge-dest-full",
            Selector::ledger_of(&applied),
            Spec::new(
                "an account merge into the whale fails with ACCOUNT_MERGE_DEST_FULL: the whale's balance is far from INT64_MAX, but its native buying liabilities count against it",
            )
            .fields(&["op.account_merge.destination"]),
        )?;
        second_book(ctx, &issuer)
    }
}

/// Crossings on pairs the first book never trades, so nothing above rests
/// in their way: an issuer offering its own asset, a credit-for-credit
/// cross, and a bid that takes one offer whole and another in part.
fn second_book(ctx: &mut ChainCtx, issuer: &SigningKey) -> Result<()> {
    ctx.fund(&["maker2", "maker3", "maker4", "taker2"], BALANCE)?;
    let maker2 = ctx.account("maker2");
    let maker3 = ctx.account("maker3");
    let maker4 = ctx.account("maker4");
    let taker2 = ctx.account("taker2");
    let eur = credit(b"EUR\0", issuer);
    let gbp = credit(b"GBP\0", issuer);
    let xlm = Asset::Native;
    let fields = [
        "op.manage_offer.offer_id",
        "op.manage_offer.assets_amount_price",
    ];

    // the issuer holds no trustline for its own asset, so only its account
    // and the new offer change
    let env = ctx.classic(
        issuer,
        vec![sell(&gbp, &xlm, 100 * UNIT, Price { n: 5, d: 1 }, 0)],
    )?;
    let applied = ctx.submit_and_close(&env)?;
    let (offer, crossed) = resting(&applied.result.result)?;
    created_id(&offer)?;
    ensure!(
        crossed == 0,
        "the issuer's GBP offer crossed {crossed} offers"
    );
    ctx.capture(
        "issuer-sells-own-asset",
        Selector::ledger_of(&applied),
        Spec::new(
            "an issuer offers its own GBP for XLM: native buying liabilities on its account, and no trustline because issuers hold none for their own asset",
        )
        .fields(&fields),
    )?;

    for (who, assets) in [
        (&maker2, vec![&eur]),
        (&maker3, vec![&eur]),
        (&maker4, vec![&eur, &gbp]),
        (&taker2, vec![&eur, &gbp]),
    ] {
        let ops = assets.into_iter().map(trust).collect::<Result<Vec<_>>>()?;
        let env = ctx.classic(who, ops)?;
        ctx.submit_and_close(&env)?;
    }
    let env = ctx.classic(
        issuer,
        vec![
            pay(&maker4, &gbp, 1000 * UNIT),
            pay(&taker2, &eur, 1000 * UNIT),
        ],
    )?;
    ctx.submit_and_close(&env)?;

    let env = ctx.classic(&maker4, vec![sell(&gbp, &eur, 100 * UNIT, ONE, 0)])?;
    ctx.submit_and_close(&env)?;
    let env = ctx.classic(&taker2, vec![sell(&eur, &gbp, 40 * UNIT, ONE, 0)])?;
    let applied = ctx.submit_and_close(&env)?;
    let (offer, crossed) = resting(&applied.result.result)?;
    ensure!(
        crossed == 1 && matches!(offer, ManageOfferSuccessResultOffer::Deleted),
        "taker2's EUR offer crossed {crossed} offers and left {offer:?}, expected one cross and nothing resting"
    );
    ctx.capture(
        "credit-pair-cross-partial",
        Selector::ledger_of(&applied),
        Spec::new(
            "a sell offer of EUR for GBP filled entirely against part of a resting GBP offer, so only the two traders' four trustlines and the resting offer change",
        )
        .fields(&fields),
    )?;

    let env = ctx.classic(&maker2, vec![sell(&xlm, &eur, 50 * UNIT, ONE, 0)])?;
    ctx.submit_and_close(&env)?;
    let env = ctx.classic(&maker3, vec![sell(&xlm, &eur, 100 * UNIT, ONE, 0)])?;
    ctx.submit_and_close(&env)?;
    let env = ctx.classic(&taker2, vec![buy(&eur, &xlm, 80 * UNIT, ONE)])?;
    let applied = ctx.submit_and_close(&env)?;
    let (offer, crossed) = resting(&applied.result.result)?;
    ensure!(
        crossed == 2 && matches!(offer, ManageOfferSuccessResultOffer::Deleted),
        "taker2's bid crossed {crossed} offers and left {offer:?}, expected two crosses and nothing resting"
    );
    ctx.capture(
        "buy-offer-takes-two-makers",
        Selector::transaction(&applied),
        Spec::new(
            "a manage-buy offer for 80 XLM that takes one maker's 50 XLM offer whole, removing it, and 30 of another maker's 100",
        )
        .fields(&fields),
    )
}

fn ensure_dest_full(r: &TransactionResultResult) -> Result<()> {
    let TransactionResultResult::TxFailed(ops) = r else {
        bail!("core answered {r:?} for the merge, expected tx_failed");
    };
    let op = ops.first().context("tx_failed without operation results")?;
    ensure!(
        matches!(
            op,
            OperationResult::OpInner(OperationResultTr::AccountMerge(
                AccountMergeResult::DestFull
            ))
        ),
        "core answered {op:?} for the merge, expected account_merge dest_full"
    );
    Ok(())
}
