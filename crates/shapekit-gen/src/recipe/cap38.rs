//! `cap38.liquidity-pools`: a constant-product pool between two assets with
//! the same code whose issuers sort one way by strkey and the other way by
//! key bytes, so only the byte order is a valid pool; the strkey-ordered
//! twin; deposit, a path payment through the pool, withdrawal, and an
//! authorization revocation that redeems the remaining pool shares into
//! claimable balances.
//!
//! | ledger | content                                                          |
//! |--------|------------------------------------------------------------------|
//! | 2      | protocol upgrade                                                 |
//! | 3      | root funds the two issuers, holder and trader                    |
//! | 4      | issuer a sets AUTH_REVOCABLE                                     |
//! | 5      | holder trusts USD of both issuers                                |
//! | 6      | trader trusts USD of both issuers                                |
//! | 7      | issuer a pays holder and trader                                  |
//! | 8      | issuer b pays holder                                             |
//! | 9      | holder trusts the pool share, assets in key byte order           |
//! | 10     | holder deposits                                                  |
//! | 11     | trader sends USD of a for USD of b through the pool              |
//! | 12     | holder withdraws a quarter of its shares                         |
//! | 13     | issuer a revokes holder's authorization, redeeming the rest      |
//!
//! The strkey-ordered twin never reaches a ledger: core refuses it with
//! `CHANGE_TRUST_MALFORMED` (`ChangeTrustOpFrame::doCheckValid` through
//! `isPoolShareAssetValid`, stellar-core v30.0.0).
//!
//! # The issuer pair
//!
//! [`issuer_pair`] walks the role names `issuer-0`, `issuer-1`, ... in
//! order and, for each new role `issuer-j`, compares it with every earlier
//! `issuer-i`. The first pair whose `G...` strkey order differs from the
//! order of its 32 raw key bytes is used. That happens when the first
//! differing base32 character is a digit on one side and a letter on the
//! other: digits sort before letters in ascii but encode the values 26 to
//! 31. The search is pure arithmetic over the derived keys, so it picks the
//! same pair on every run and under both passphrases. For this recipe id it
//! stops at `issuer-7`, against `issuer-2`; the 23 pairs compared before it all agree.
//! The two strkeys first differ in their third character, `Y` (value 24)
//! against `6` (value 30):
//!
//! | role       | strkey                                                     | order by bytes |
//! |------------|------------------------------------------------------------|----------------|
//! | `issuer-2` | `GAYL3ZJQZMNCRXA3HNX6WTDKUVXQDW3AS5HPZL45E4WPNILW2ARM3JBS` | first          |
//! | `issuer-7` | `GA66MKR5EGOIZDEXN32LU3UOZTZB3LW6VAZQKSXGZ73ADSIUEFE24Y5C` | second         |
//!
//! `issuer_pair_is_stable` in this file pins that result.

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use shapekit_core_driver::keys::{account_id, account_strkey, derive_key, muxed_account};
use stellar_xdr::{
    AlphaNum4, Asset, AssetCode4, ChangeTrustAsset, ChangeTrustOp, ChangeTrustResult, Hash,
    LedgerEntryData, LedgerKey, LedgerKeyLiquidityPool, LedgerKeyTrustLine, Limits,
    LiquidityPoolConstantProductParameters, LiquidityPoolDepositOp, LiquidityPoolParameters,
    LiquidityPoolWithdrawOp, OperationBody, OperationResult, OperationResultTr,
    PathPaymentStrictSendOp, PaymentOp, PoolId, Price, SetOptionsOp, SetTrustLineFlagsOp,
    TransactionResultResult, TrustLineAsset, WriteXdr,
};

use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "cap38.liquidity-pools";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

/// One USD unit in stroops.
const UNIT: i64 = 10_000_000;

/// The constant-product fee core accepts, 30 basis points.
const POOL_FEE: i32 = 30;

/// More roles than the search ever needs; reaching it means the key
/// derivation changed and the module doc is stale.
const SEARCH_LIMIT: usize = 64;

const AUTH_REVOCABLE_FLAG: u32 = 2;
const AUTHORIZED_FLAG: u32 = 1;

pub struct LiquidityPools {
    pub protocol: u32,
}

/// The two issuer roles, lower raw key bytes first, found by the search the
/// module doc describes. Their strkeys sort the other way round.
pub fn issuer_pair() -> Result<(String, String)> {
    let key = |i: usize| derive_key(FAMILY, &format!("issuer-{i}")).verifying_key();
    for j in 1..SEARCH_LIMIT {
        let kj = key(j);
        for i in 0..j {
            let ki = key(i);
            let by_bytes = ki.to_bytes() < kj.to_bytes();
            let by_strkey = account_strkey(&ki) < account_strkey(&kj);
            if by_bytes != by_strkey {
                let (lo, hi) = if by_bytes { (i, j) } else { (j, i) };
                return Ok((format!("issuer-{lo}"), format!("issuer-{hi}")));
            }
        }
    }
    bail!("no issuer pair among {SEARCH_LIMIT} roles orders differently by strkey and by bytes")
}

fn usd(issuer: &SigningKey) -> Asset {
    Asset::CreditAlphanum4(AlphaNum4 {
        asset_code: AssetCode4(*b"USD\0"),
        issuer: account_id(&issuer.verifying_key()),
    })
}

fn params(a: &Asset, b: &Asset) -> LiquidityPoolParameters {
    LiquidityPoolParameters::LiquidityPoolConstantProduct(LiquidityPoolConstantProductParameters {
        asset_a: a.clone(),
        asset_b: b.clone(),
        fee: POOL_FEE,
    })
}

/// Pool id as core derives it: SHA-256 of the parameters' xdr.
fn pool_id(p: &LiquidityPoolParameters) -> Result<PoolId> {
    Ok(PoolId(Hash(
        Sha256::digest(p.to_xdr(Limits::none())?).into(),
    )))
}

fn trust(asset: &Asset) -> Result<OperationBody> {
    let line = match asset {
        Asset::CreditAlphanum4(a) => ChangeTrustAsset::CreditAlphanum4(a.clone()),
        other => bail!("trust line to {other:?} is not used here"),
    };
    Ok(OperationBody::ChangeTrust(ChangeTrustOp {
        line,
        limit: i64::MAX,
    }))
}

fn trust_pool(p: &LiquidityPoolParameters) -> OperationBody {
    OperationBody::ChangeTrust(ChangeTrustOp {
        line: ChangeTrustAsset::PoolShare(p.clone()),
        limit: i64::MAX,
    })
}

fn pay(to: &SigningKey, asset: &Asset, amount: i64) -> OperationBody {
    OperationBody::Payment(PaymentOp {
        destination: muxed_account(&to.verifying_key()),
        asset: asset.clone(),
        amount,
    })
}

fn trustline_key(owner: &SigningKey, asset: TrustLineAsset) -> LedgerKey {
    LedgerKey::Trustline(LedgerKeyTrustLine {
        account_id: account_id(&owner.verifying_key()),
        asset,
    })
}

fn usd_line(asset: &Asset) -> Result<TrustLineAsset> {
    match asset {
        Asset::CreditAlphanum4(a) => Ok(TrustLineAsset::CreditAlphanum4(a.clone())),
        other => bail!("{other:?} has no credit trust line"),
    }
}

const CHANGE_TRUST_FIELDS: [&str; 3] = [
    "op.change_trust.line",
    "op.change_trust.line.liquidity_pool",
    "op.change_trust.limit",
];

impl Recipe for LiquidityPools {
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
        let (lo, hi) = issuer_pair()?;
        ctx.fund(&[&lo, &hi, "holder", "trader"], BALANCE)?;
        let issuer_a = ctx.account(&lo);
        let issuer_b = ctx.account(&hi);
        let holder = ctx.account("holder");
        let trader = ctx.account("trader");
        let (sa, sb) = (
            account_strkey(&issuer_a.verifying_key()),
            account_strkey(&issuer_b.verifying_key()),
        );
        ensure!(
            issuer_a.verifying_key().to_bytes() < issuer_b.verifying_key().to_bytes() && sb < sa,
            "issuer pair {sa} {sb} does not disagree between byte and strkey order"
        );
        let a = usd(&issuer_a);
        let b = usd(&issuer_b);

        // revocable from the start, so the trustlines below stay revocable
        let env = ctx.classic(
            &issuer_a,
            vec![OperationBody::SetOptions(SetOptionsOp {
                inflation_dest: None,
                clear_flags: None,
                set_flags: Some(AUTH_REVOCABLE_FLAG),
                master_weight: None,
                low_threshold: None,
                med_threshold: None,
                high_threshold: None,
                home_domain: None,
                signer: None,
            })],
        )?;
        ctx.submit_and_close(&env)?;
        for who in [&holder, &trader] {
            let env = ctx.classic(who, vec![trust(&a)?, trust(&b)?])?;
            ctx.submit_and_close(&env)?;
        }
        let env = ctx.classic(
            &issuer_a,
            vec![pay(&holder, &a, 1000 * UNIT), pay(&trader, &a, 1000 * UNIT)],
        )?;
        ctx.submit_and_close(&env)?;
        let env = ctx.classic(&issuer_b, vec![pay(&holder, &b, 1000 * UNIT)])?;
        ctx.submit_and_close(&env)?;

        let good = params(&a, &b);
        let pool = pool_id(&good)?;
        let env = ctx.classic(&holder, vec![trust_pool(&good)])?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "change-trust-pool-byte-order",
            Selector::transaction(&applied),
            Spec::new(format!(
                "a pool-share trustline whose asset a is USD:{sa} and asset b is USD:{sb}: ordered by issuer key bytes, which is the reverse of their strkey order"
            ))
            .fields(&CHANGE_TRUST_FIELDS)
            .flags(&["issuer-order-differs"]),
        )?;
        let line_a = trustline_key(&holder, usd_line(&a)?);
        ctx.capture(
            "asset-trustline-pool-use-count",
            Selector::LedgerEntry {
                ledger: applied.ledger,
                key: line_a.clone(),
            },
            Spec::new(
                "the holder's USD trustline once a pool-share trustline uses it, carrying the liquidity pool use count in the v2 extension",
            )
            .fields(&["op.change_trust.line.liquidity_pool"]),
        )?;
        let entries = ctx
            .driver()
            .node()
            .ledger_entries(std::slice::from_ref(&line_a))?;
        match entries.entries[0].entry.as_ref().map(|e| &e.data) {
            Some(LedgerEntryData::Trustline(t)) => {
                let count = match &t.ext {
                    stellar_xdr::TrustLineEntryExt::V1(v1) => match &v1.ext {
                        stellar_xdr::TrustLineEntryV1Ext::V2(v2) => v2.liquidity_pool_use_count,
                        stellar_xdr::TrustLineEntryV1Ext::V0 => 0,
                    },
                    stellar_xdr::TrustLineEntryExt::V0 => 0,
                };
                ensure!(
                    count == 1,
                    "holder's USD trustline has pool use count {count}, expected 1"
                );
            }
            other => bail!("holder's USD trustline is missing: {other:?}"),
        }

        // the twin orders the same assets by strkey text, as js-stellar-sdk
        // before 17.2.1 did
        let env = ctx.classic(&holder, vec![trust_pool(&params(&b, &a))])?;
        let r = ctx.capture_rejected(
            "change-trust-pool-strkey-order",
            &env,
            Spec::new(format!(
                "the same pool-share trustline with assets ordered by strkey text (USD:{sb} first), which core refuses as CHANGE_TRUST_MALFORMED"
            ))
            .fields(&CHANGE_TRUST_FIELDS)
            .flags(&["issuer-order-differs"]),
        )?;
        ensure_change_trust_malformed(&r.result)?;

        let env = ctx.classic(
            &holder,
            vec![OperationBody::LiquidityPoolDeposit(
                LiquidityPoolDepositOp {
                    liquidity_pool_id: pool.clone(),
                    max_amount_a: 100 * UNIT,
                    max_amount_b: 100 * UNIT,
                    min_price: Price { n: 1, d: 2 },
                    max_price: Price { n: 2, d: 1 },
                },
            )],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "deposit",
            Selector::transaction(&applied),
            Spec::new("the first deposit into an empty pool, 100 of each asset")
                .fields(&["op.liquidity_pool_deposit"]),
        )?;

        let env = ctx.classic(
            &trader,
            vec![OperationBody::PathPaymentStrictSend(
                PathPaymentStrictSendOp {
                    send_asset: a.clone(),
                    send_amount: 10 * UNIT,
                    destination: muxed_account(&trader.verifying_key()),
                    dest_asset: b.clone(),
                    dest_min: 1,
                    path: Default::default(),
                },
            )],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "path-payment-through-pool",
            Selector::ledger_of(&applied),
            Spec::new(
                "a strict-send path payment to the sender itself that converts through the pool, so its result holds a liquidity pool claim atom",
            )
            .fields(&["op.path_payment.amounts", "op.path_payment.path"]),
        )?;
        ctx.capture(
            "pool-after-trade",
            Selector::LedgerEntry {
                ledger: applied.ledger,
                key: LedgerKey::LiquidityPool(LedgerKeyLiquidityPool {
                    liquidity_pool_id: pool.clone(),
                }),
            },
            Spec::new("the liquidity pool entry after one trade, reserves no longer equal"),
        )?;

        let env = ctx.classic(
            &holder,
            vec![OperationBody::LiquidityPoolWithdraw(
                LiquidityPoolWithdrawOp {
                    liquidity_pool_id: pool.clone(),
                    amount: 25 * UNIT,
                    min_amount_a: 1,
                    min_amount_b: 1,
                },
            )],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "withdraw",
            Selector::transaction(&applied),
            Spec::new("the holder withdraws a quarter of its pool shares")
                .fields(&["op.liquidity_pool_withdraw"]),
        )?;

        let env = ctx.classic(
            &issuer_a,
            vec![OperationBody::SetTrustLineFlags(SetTrustLineFlagsOp {
                trustor: account_id(&holder.verifying_key()),
                asset: a.clone(),
                clear_flags: AUTHORIZED_FLAG,
                set_flags: 0,
            })],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "revoke-redeems-pool-shares",
            Selector::ledger_of(&applied),
            Spec::new(
                "issuer a clears the holder's authorization, so core redeems the holder's remaining pool shares into one claimable balance per asset and removes the pool-share trustline",
            )
            .fields(&["op.set_trust_line_flags"]),
        )?;
        let pool_line = trustline_key(&holder, TrustLineAsset::PoolShare(pool));
        let left = ctx
            .driver()
            .node()
            .ledger_entries(std::slice::from_ref(&pool_line))?;
        ensure!(
            left.entries.iter().all(|e| e.entry.is_none()),
            "the pool-share trustline survived the revocation"
        );
        let created = claimable_balances_created(&applied.meta)?;
        ensure!(
            created == 2,
            "revocation created {created} claimable balances, expected 2"
        );
        Ok(())
    }
}

fn claimable_balances_created(meta: &stellar_xdr::TransactionMeta) -> Result<usize> {
    let ops: Vec<&stellar_xdr::LedgerEntryChanges> = match meta {
        stellar_xdr::TransactionMeta::V3(m) => m.operations.iter().map(|o| &o.changes).collect(),
        stellar_xdr::TransactionMeta::V4(m) => m.operations.iter().map(|o| &o.changes).collect(),
        other => bail!(
            "transaction meta v{} has no per-operation changes here",
            super::select::tx_meta_version(other)
        ),
    };
    Ok(ops
        .into_iter()
        .flat_map(|c| c.iter())
        .filter(|c| {
            matches!(
                c,
                stellar_xdr::LedgerEntryChange::Created(e)
                    if matches!(e.data, LedgerEntryData::ClaimableBalance(_))
            )
        })
        .count())
}

fn ensure_change_trust_malformed(r: &TransactionResultResult) -> Result<()> {
    let TransactionResultResult::TxFailed(ops) = r else {
        bail!("core answered {r:?} for the strkey-ordered pool, expected tx_failed");
    };
    let op = ops.first().context("tx_failed without operation results")?;
    ensure!(
        matches!(
            op,
            OperationResult::OpInner(OperationResultTr::ChangeTrust(ChangeTrustResult::Malformed))
        ),
        "core answered {op:?} for the strkey-ordered pool, expected change_trust malformed"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issuer_pair_is_stable() {
        let (lo, hi) = issuer_pair().unwrap();
        assert_eq!((lo.as_str(), hi.as_str()), ("issuer-2", "issuer-7"));
        let k = |r: &str| derive_key(FAMILY, r).verifying_key();
        assert_eq!(
            account_strkey(&k(&lo)),
            "GAYL3ZJQZMNCRXA3HNX6WTDKUVXQDW3AS5HPZL45E4WPNILW2ARM3JBS"
        );
        assert_eq!(
            account_strkey(&k(&hi)),
            "GA66MKR5EGOIZDEXN32LU3UOZTZB3LW6VAZQKSXGZ73ADSIUEFE24Y5C"
        );
        assert!(k(&lo).to_bytes() < k(&hi).to_bytes());
        assert!(account_strkey(&k(&hi)) < account_strkey(&k(&lo)));
    }
}
