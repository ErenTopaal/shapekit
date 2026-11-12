//! `cap77.frozen-keys`: CAP-77 frozen ledger keys on a private network.
//! Freezing goes through a config upgrade set holding a
//! CONFIG_SETTING_FROZEN_LEDGER_KEYS_DELTA entry, which core folds into the
//! CONFIG_SETTING_FROZEN_LEDGER_KEYS entry (`ConfigUpgradeSetFrame::applyTo`,
//! Upgrades.cpp, v30.0.0); a freeze bypass works the same way through
//! CONFIG_SETTING_FREEZE_BYPASS_TXS_DELTA.
//!
//! | ledger   | content                                                  |
//! |----------|----------------------------------------------------------|
//! | 2        | protocol upgrade                                         |
//! | 3 to 6   | testnet Soroban settings                                 |
//! | 7        | root funds issuer and holder                             |
//! | 8 to 12  | holder trusts USD and the XLM/USD pool, gets USD,        |
//! |          | deposits, and is sent a USD claimable balance            |
//! | 13 to 14 | config upgrade freezing holder's USD trustline           |
//! | 15       | holder claims the balance: CLAIM_..._TRUSTLINE_FROZEN    |
//! | 16       | holder deposits: LIQUIDITY_POOL_DEPOSIT_TRUSTLINE_FROZEN |
//! | 17       | holder withdraws: LIQUIDITY_POOL_WITHDRAW_TRUSTLINE_FROZEN |
//! | 18 to 19 | config upgrade adding a USD payment to the bypass list   |
//! | 20       | that payment applies although the trustline is frozen    |
//!
//! Before the bypass, the same kind of payment is refused at submission with
//! `txFROZEN_KEY_ACCESSED` (`TransactionFrame::commonValid`): core checks
//! frozen keys whenever it validates a transaction, and freezing only
//! changes between ledgers, so that code never reaches a ledger from a
//! local node. The refused envelope is an ordinary payment and matches no
//! family fragment, so it is asserted and not captured.

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use shapekit_core_driver::keys::{account_id, muxed_account};
use shapekit_core_driver::node::SubmitStatus;
use shapekit_core_driver::tx::envelope_hash;
use stellar_xdr::{
    AlphaNum4, Asset, AssetCode4, ChangeTrustAsset, ChangeTrustOp, ClaimClaimableBalanceOp,
    ClaimClaimableBalanceResult, ClaimPredicate, ClaimableBalanceId, Claimant, ClaimantV0,
    ConfigSettingEntry, ConfigSettingId, CreateClaimableBalanceOp, CreateClaimableBalanceResult,
    EncodedLedgerKey, FreezeBypassTxsDelta, FrozenLedgerKeysDelta, Hash, LedgerKey,
    LedgerKeyTrustLine, Limits, LiquidityPoolConstantProductParameters, LiquidityPoolDepositOp,
    LiquidityPoolDepositResult, LiquidityPoolParameters, LiquidityPoolWithdrawOp,
    LiquidityPoolWithdrawResult, OperationBody, OperationResult, OperationResultTr, PaymentOp,
    PoolId, Price, TransactionResultResult, TrustLineAsset, WriteXdr,
};

use super::cap46::{apply_config_upgrade, config_key, live_setting, upgrade_wrote};
use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "cap77.frozen-keys";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

/// USD the issuer pays the holder, 1,000.
const USD_FUNDING: i64 = 10_000_000_000;

/// Amount of each asset deposited into the pool, 100.
const DEPOSIT: i64 = 1_000_000_000;

/// USD in the claimable balance and in each payment, 10.
const SMALL: i64 = 100_000_000;

/// Constant product pool fee, the only one core accepts.
const POOL_FEE_BPS: i32 = 30;

pub struct FrozenKeys {
    pub protocol: u32,
}

fn usd(issuer: &SigningKey) -> AlphaNum4 {
    AlphaNum4 {
        asset_code: AssetCode4(*b"USD\0"),
        issuer: account_id(&issuer.verifying_key()),
    }
}

fn pool_params(issuer: &SigningKey) -> LiquidityPoolParameters {
    LiquidityPoolParameters::LiquidityPoolConstantProduct(LiquidityPoolConstantProductParameters {
        asset_a: Asset::Native,
        asset_b: Asset::CreditAlphanum4(usd(issuer)),
        fee: POOL_FEE_BPS,
    })
}

fn pool_id(issuer: &SigningKey) -> Result<PoolId> {
    let xdr = pool_params(issuer).to_xdr(Limits::none())?;
    Ok(PoolId(Hash(Sha256::digest(xdr).into())))
}

fn change_trust(line: ChangeTrustAsset) -> OperationBody {
    OperationBody::ChangeTrust(ChangeTrustOp {
        line,
        limit: i64::MAX,
    })
}

fn usd_payment(issuer: &SigningKey, to: &SigningKey, amount: i64) -> OperationBody {
    OperationBody::Payment(PaymentOp {
        destination: muxed_account(&to.verifying_key()),
        asset: Asset::CreditAlphanum4(usd(issuer)),
        amount,
    })
}

/// The single operation result of a failed transaction.
fn failed_op(r: &TransactionResultResult) -> Result<&OperationResultTr> {
    let TransactionResultResult::TxFailed(ops) = r else {
        bail!("expected tx_failed, core answered {r:?}");
    };
    match ops.first() {
        Some(OperationResult::OpInner(tr)) if ops.len() == 1 => Ok(tr),
        other => bail!("expected one inner operation result, got {other:?}"),
    }
}

impl Recipe for FrozenKeys {
    fn id(&self) -> &str {
        FAMILY
    }

    fn family(&self) -> &str {
        FAMILY
    }

    fn protocol(&self) -> u32 {
        self.protocol
    }

    fn soroban_settings(&self) -> bool {
        true
    }

    fn run(&self, ctx: &mut ChainCtx) -> Result<()> {
        ctx.fund(&["issuer", "holder"], BALANCE)?;
        let issuer = ctx.account("issuer");
        let holder = ctx.account("holder");

        let env = ctx.classic(
            &holder,
            vec![change_trust(ChangeTrustAsset::CreditAlphanum4(usd(
                &issuer,
            )))],
        )?;
        ctx.submit_and_close(&env)?;
        let env = ctx.classic(&issuer, vec![usd_payment(&issuer, &holder, USD_FUNDING)])?;
        ctx.submit_and_close(&env)?;
        let env = ctx.classic(
            &holder,
            vec![change_trust(ChangeTrustAsset::PoolShare(pool_params(
                &issuer,
            )))],
        )?;
        ctx.submit_and_close(&env)?;
        let pool = pool_id(&issuer)?;
        let price = Price { n: 1, d: 1 };
        let deposit = OperationBody::LiquidityPoolDeposit(LiquidityPoolDepositOp {
            liquidity_pool_id: pool.clone(),
            max_amount_a: DEPOSIT,
            max_amount_b: DEPOSIT,
            min_price: price.clone(),
            max_price: price,
        });
        let env = ctx.classic(&holder, vec![deposit.clone()])?;
        ctx.submit_and_close(&env)?;
        let env = ctx.classic(
            &issuer,
            vec![OperationBody::CreateClaimableBalance(
                CreateClaimableBalanceOp {
                    asset: Asset::CreditAlphanum4(usd(&issuer)),
                    amount: SMALL,
                    claimants: vec![Claimant::ClaimantTypeV0(ClaimantV0 {
                        destination: account_id(&holder.verifying_key()),
                        predicate: ClaimPredicate::Unconditional,
                    })]
                    .try_into()?,
                },
            )],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        let balance_id: ClaimableBalanceId = match &applied.result.result {
            TransactionResultResult::TxSuccess(ops) => match ops.first() {
                Some(OperationResult::OpInner(OperationResultTr::CreateClaimableBalance(
                    CreateClaimableBalanceResult::Success(id),
                ))) => id.clone(),
                other => bail!("claimable balance creation answered {other:?}"),
            },
            other => bail!("claimable balance creation answered {other:?}"),
        };

        let trustline = LedgerKey::Trustline(LedgerKeyTrustLine {
            account_id: account_id(&holder.verifying_key()),
            asset: TrustLineAsset::CreditAlphanum4(usd(&issuer)),
        });
        let freeze = FrozenLedgerKeysDelta {
            keys_to_freeze: vec![EncodedLedgerKey(
                trustline.to_xdr(Limits::none())?.try_into()?,
            )]
            .try_into()?,
            keys_to_unfreeze: Default::default(),
        };
        let (_, seq) =
            apply_config_upgrade(ctx, vec![ConfigSettingEntry::FrozenLedgerKeysDelta(freeze)])?;
        ensure!(
            upgrade_wrote(ctx, ConfigSettingId::FrozenLedgerKeys),
            "the upgrade meta of ledger {seq} does not write the frozen ledger keys setting"
        );
        match live_setting(ctx, ConfigSettingId::FrozenLedgerKeys)? {
            ConfigSettingEntry::FrozenLedgerKeys(k) => ensure!(
                k.keys.len() == 1,
                "{} keys are frozen, expected holder's trustline only",
                k.keys.len()
            ),
            other => bail!("frozen ledger keys setting is {other:?}"),
        }
        ctx.capture(
            "freeze-upgrade",
            Selector::Ledger(seq),
            Spec::new(
                "a config upgrade whose set holds a frozen ledger keys delta freezing one trustline; the upgrade meta updates the frozen ledger keys config setting entry",
            ),
        )?;
        ctx.capture(
            "frozen-keys-entry",
            Selector::LedgerEntry {
                ledger: seq,
                key: config_key(ConfigSettingId::FrozenLedgerKeys),
            },
            Spec::new(
                "the frozen ledger keys config setting entry holding one XDR-encoded trustline key",
            ),
        )?;

        let env = ctx.classic(
            &holder,
            vec![OperationBody::ClaimClaimableBalance(
                ClaimClaimableBalanceOp { balance_id },
            )],
        )?;
        let applied = ctx.submit_and_close_failing(&env)?;
        let op = failed_op(&applied.result.result)?;
        ensure!(
            matches!(
                op,
                OperationResultTr::ClaimClaimableBalance(
                    ClaimClaimableBalanceResult::TrustlineFrozen
                )
            ),
            "claiming into a frozen trustline answered {op:?}"
        );
        ctx.capture(
            "claim-into-frozen-trustline",
            Selector::ledger_of(&applied),
            Spec::new(
                "a claimable balance claim that fails with CLAIM_CLAIMABLE_BALANCE_TRUSTLINE_FROZEN because the claimant's trustline for the balance's asset is frozen",
            ),
        )?;

        let env = ctx.classic(&holder, vec![deposit])?;
        let applied = ctx.submit_and_close_failing(&env)?;
        let op = failed_op(&applied.result.result)?;
        ensure!(
            matches!(
                op,
                OperationResultTr::LiquidityPoolDeposit(
                    LiquidityPoolDepositResult::TrustlineFrozen
                )
            ),
            "depositing with a frozen trustline answered {op:?}"
        );
        ctx.capture(
            "deposit-frozen-trustline",
            Selector::ledger_of(&applied),
            Spec::new(
                "a liquidity pool deposit that fails with LIQUIDITY_POOL_DEPOSIT_TRUSTLINE_FROZEN because the depositor's trustline for one pool asset is frozen",
            ),
        )?;

        let env = ctx.classic(
            &holder,
            vec![OperationBody::LiquidityPoolWithdraw(
                LiquidityPoolWithdrawOp {
                    liquidity_pool_id: pool,
                    amount: SMALL,
                    min_amount_a: 0,
                    min_amount_b: 0,
                },
            )],
        )?;
        let applied = ctx.submit_and_close_failing(&env)?;
        let op = failed_op(&applied.result.result)?;
        ensure!(
            matches!(
                op,
                OperationResultTr::LiquidityPoolWithdraw(
                    LiquidityPoolWithdrawResult::TrustlineFrozen
                )
            ),
            "withdrawing with a frozen trustline answered {op:?}"
        );
        ctx.capture(
            "withdraw-frozen-trustline",
            Selector::ledger_of(&applied),
            Spec::new(
                "a liquidity pool withdrawal that fails with LIQUIDITY_POOL_WITHDRAW_TRUSTLINE_FROZEN because the withdrawer's trustline for one pool asset is frozen",
            ),
        )?;

        // the same payment is refused at submission until it is bypassed
        let payment = ctx.classic(&holder, vec![usd_payment(&issuer, &issuer, SMALL)])?;
        let sub = ctx.driver().node().submit(&payment)?;
        match (&sub.status, &sub.error) {
            (SubmitStatus::Error, Some(r))
                if matches!(r.result, TransactionResultResult::TxFrozenKeyAccessed) => {}
            other => bail!(
                "a payment from a frozen trustline was expected to be refused with tx_frozen_key_accessed, core answered {other:?}"
            ),
        }

        let hash = envelope_hash(&payment, &ctx.network_id())?;
        let bypass = FreezeBypassTxsDelta {
            add_txs: vec![Hash(hash)].try_into()?,
            remove_txs: Default::default(),
        };
        let (_, seq) =
            apply_config_upgrade(ctx, vec![ConfigSettingEntry::FreezeBypassTxsDelta(bypass)])?;
        ensure!(
            upgrade_wrote(ctx, ConfigSettingId::FreezeBypassTxs),
            "the upgrade meta of ledger {seq} does not write the freeze bypass setting"
        );
        ctx.capture(
            "bypass-upgrade",
            Selector::Ledger(seq),
            Spec::new(
                "a config upgrade whose set holds a freeze bypass delta adding one transaction hash; the upgrade meta updates the freeze bypass transactions config setting entry",
            ),
        )?;
        ctx.capture(
            "bypass-entry",
            Selector::LedgerEntry {
                ledger: seq,
                key: config_key(ConfigSettingId::FreezeBypassTxs),
            },
            Spec::new(
                "the freeze bypass transactions config setting entry holding the hash of one transaction allowed to touch frozen keys",
            ),
        )?;
        ctx.submit_and_close(&payment)
            .context("the bypassed payment from the frozen trustline should apply")?;
        Ok(())
    }
}
