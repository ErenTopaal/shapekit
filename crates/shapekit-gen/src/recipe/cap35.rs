//! `cap35.clawback`: a clawback of a trustline balance, a claimable balance
//! that inherits the clawback flag and its clawback, trustline flag changes
//! through SetTrustLineFlags, the clawback that fails once the flag is
//! cleared, and the SetTrustLineFlags core refuses for setting the clawback
//! flag.
//!
//! | ledger | content                                                          |
//! |--------|------------------------------------------------------------------|
//! | 2      | protocol upgrade                                                 |
//! | 3      | root funds the issuer, the holder and the claimant               |
//! | 4      | the issuer sets auth revocable and auth clawback enabled         |
//! | 5      | the holder trusts the asset (the trustline inherits clawback)    |
//! | 6      | the issuer pays the holder                                       |
//! | 7      | the holder creates a claimable balance of the asset              |
//! | 8      | the issuer claws back the claimable balance                      |
//! | 9      | the issuer claws back part of the holder's balance               |
//! | 10     | SetTrustLineFlags: authorized to authorized to maintain liabilities |
//! | 11     | SetTrustLineFlags: back to authorized, clawback flag cleared      |
//! | 12     | a clawback that fails with CLAWBACK_NOT_CLAWBACK_ENABLED         |
//!
//! The refused envelope sets the clawback flag through SetTrustLineFlags,
//! which `SetTrustLineFlagsOpFrame::doCheckValid` (stellar-core v30.0.0)
//! answers with `SET_TRUST_LINE_FLAGS_MALFORMED`.

use anyhow::{Result, bail, ensure};
use ed25519_dalek::SigningKey;
use shapekit_core_driver::keys::{account_id, muxed_account};
use stellar_xdr::{
    AccountFlags, AlphaNum4, Asset, AssetCode4, ChangeTrustAsset, ChangeTrustOp, ClaimPredicate,
    ClaimableBalanceEntryExt, ClaimableBalanceFlags, Claimant, ClaimantV0,
    ClawbackClaimableBalanceOp, ClawbackClaimableBalanceResult, ClawbackOp, ClawbackResult,
    CreateClaimableBalanceOp, LedgerEntryData, LedgerKey, LedgerKeyTrustLine, OperationBody,
    OperationResultTr, PaymentOp, SetOptionsOp, SetTrustLineFlagsOp, SetTrustLineFlagsResult,
    TrustLineAsset, TrustLineFlags,
};

use super::cap23::{balance_key, created_balance_id, op_result};
use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "cap35.clawback";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

/// Asset amounts, in stroops of the issued asset.
const PAID: i64 = 10_000_000_000;
const CB_AMOUNT: i64 = 1_000_000_000;
const CLAWED: i64 = 2_500_000_000;

const AUTHORIZED: u32 = TrustLineFlags::AuthorizedFlag as u32;
const MAINTAIN: u32 = TrustLineFlags::AuthorizedToMaintainLiabilitiesFlag as u32;
const CLAWBACK: u32 = TrustLineFlags::TrustlineClawbackEnabledFlag as u32;

pub struct Clawback {
    pub protocol: u32,
}

fn flags_op(holder: &SigningKey, asset: &Asset, clear: u32, set: u32) -> OperationBody {
    OperationBody::SetTrustLineFlags(SetTrustLineFlagsOp {
        trustor: account_id(&holder.verifying_key()),
        asset: asset.clone(),
        clear_flags: clear,
        set_flags: set,
    })
}

fn clawback(holder: &SigningKey, asset: &Asset, amount: i64) -> OperationBody {
    OperationBody::Clawback(ClawbackOp {
        asset: asset.clone(),
        from: muxed_account(&holder.verifying_key()),
        amount,
    })
}

impl Clawback {
    fn trustline_flags(&self, ctx: &mut ChainCtx, key: &LedgerKey) -> Result<u32> {
        let found = ctx
            .driver()
            .node()
            .ledger_entries(std::slice::from_ref(key))?;
        match found.entries[0].entry.as_ref().map(|e| &e.data) {
            Some(LedgerEntryData::Trustline(t)) => Ok(t.flags),
            other => bail!("the holder's trustline is missing: {other:?}"),
        }
    }
}

impl Recipe for Clawback {
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
        ctx.fund(&["issuer", "holder", "claimant"], BALANCE)?;
        let issuer = ctx.account("issuer");
        let holder = ctx.account("holder");
        let claimant = ctx.account("claimant");
        let alpha = AlphaNum4 {
            asset_code: AssetCode4(*b"CLAW"),
            issuer: account_id(&issuer.verifying_key()),
        };
        let asset = Asset::CreditAlphanum4(alpha.clone());
        let trust_key = LedgerKey::Trustline(LedgerKeyTrustLine {
            account_id: account_id(&holder.verifying_key()),
            asset: TrustLineAsset::CreditAlphanum4(alpha.clone()),
        });

        // clawback needs revocable as well (SetOptionsOpFrame checks the pair)
        let env = ctx.classic(
            &issuer,
            vec![OperationBody::SetOptions(SetOptionsOp {
                inflation_dest: None,
                clear_flags: None,
                set_flags: Some(
                    AccountFlags::RevocableFlag as u32 | AccountFlags::ClawbackEnabledFlag as u32,
                ),
                master_weight: None,
                low_threshold: None,
                med_threshold: None,
                high_threshold: None,
                home_domain: None,
                signer: None,
            })],
        )?;
        ctx.submit_and_close(&env)?;
        let env = ctx.classic(
            &holder,
            vec![OperationBody::ChangeTrust(ChangeTrustOp {
                line: ChangeTrustAsset::CreditAlphanum4(alpha),
                limit: i64::MAX,
            })],
        )?;
        ctx.submit_and_close(&env)?;
        let flags = self.trustline_flags(ctx, &trust_key)?;
        ensure!(
            flags == AUTHORIZED | CLAWBACK,
            "the new trustline should be authorized with clawback enabled, flags are {flags}"
        );
        let env = ctx.classic(
            &issuer,
            vec![OperationBody::Payment(PaymentOp {
                destination: muxed_account(&holder.verifying_key()),
                asset: asset.clone(),
                amount: PAID,
            })],
        )?;
        ctx.submit_and_close(&env)?;

        let env = ctx.classic(
            &holder,
            vec![OperationBody::CreateClaimableBalance(
                CreateClaimableBalanceOp {
                    asset: asset.clone(),
                    amount: CB_AMOUNT,
                    claimants: vec![Claimant::ClaimantTypeV0(ClaimantV0 {
                        destination: account_id(&claimant.verifying_key()),
                        predicate: ClaimPredicate::Unconditional,
                    })]
                    .try_into()?,
                },
            )],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "create-clawback-enabled-balance",
            Selector::ledger_of(&applied),
            Spec::new(
                "a claimable balance created from a clawback-enabled trustline, which core marks clawback enabled in the entry's extension v1 flags; the whole ledger, since the envelope alone names no CAP-35 arm",
            )
            .fields(&["op.create_claimable_balance.asset_amount"]),
        )?;
        let id = created_balance_id(&applied)?;
        let key = balance_key(&id);
        ctx.capture(
            "clawback-enabled-balance-entry",
            Selector::LedgerEntry {
                ledger: applied.ledger,
                key: key.clone(),
            },
            Spec::new(
                "a claimable balance entry with extension v1 carrying CLAIMABLE_BALANCE_CLAWBACK_ENABLED_FLAG",
            ),
        )?;
        let found = ctx
            .driver()
            .node()
            .ledger_entries(std::slice::from_ref(&key))?;
        match found.entries[0].entry.as_ref().map(|e| &e.data) {
            Some(LedgerEntryData::ClaimableBalance(cb)) => match &cb.ext {
                ClaimableBalanceEntryExt::V1(v1) => ensure!(
                    v1.flags == ClaimableBalanceFlags::ClaimableBalanceClawbackEnabledFlag as u32,
                    "the balance should be clawback enabled, flags are {}",
                    v1.flags
                ),
                ClaimableBalanceEntryExt::V0 => {
                    bail!("the balance should carry extension v1 flags")
                }
            },
            other => bail!("the claimable balance is missing: {other:?}"),
        }

        let env = ctx.classic(
            &issuer,
            vec![OperationBody::ClawbackClaimableBalance(
                ClawbackClaimableBalanceOp { balance_id: id },
            )],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "clawback-claimable-balance",
            Selector::transaction(&applied),
            Spec::new(
                "the issuer claws back a whole claimable balance, which deletes the entry without any claimant acting",
            )
            .fields(&["op.clawback_claimable_balance.balance_id"]),
        )?;
        ensure!(
            matches!(
                op_result(&applied.result.result, 0)?,
                OperationResultTr::ClawbackClaimableBalance(
                    ClawbackClaimableBalanceResult::Success
                )
            ),
            "clawback of the claimable balance should succeed"
        );

        let env = ctx.classic(&issuer, vec![clawback(&holder, &asset, CLAWED)])?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "clawback",
            Selector::ledger_of(&applied),
            Spec::new(
                "the issuer claws back part of a holder's trustline balance; the ledger carries the envelope and the trustline change",
            )
            .fields(&["op.clawback.from", "op.clawback.asset_amount"]),
        )?;

        let env = ctx.classic(
            &issuer,
            vec![flags_op(&holder, &asset, AUTHORIZED, MAINTAIN)],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "set-flags-authorized-to-maintain",
            Selector::ledger_of(&applied),
            Spec::new(
                "SetTrustLineFlags clears AUTHORIZED and sets AUTHORIZED_TO_MAINTAIN_LIABILITIES in one operation",
            )
            .fields(&["op.set_trust_line_flags"]),
        )?;
        let flags = self.trustline_flags(ctx, &trust_key)?;
        ensure!(
            flags == MAINTAIN | CLAWBACK,
            "the trustline should be maintain-liabilities with clawback, flags are {flags}"
        );

        let env = ctx.classic(
            &issuer,
            vec![flags_op(&holder, &asset, MAINTAIN | CLAWBACK, AUTHORIZED)],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "set-flags-reauthorize-clear-clawback",
            Selector::ledger_of(&applied),
            Spec::new(
                "SetTrustLineFlags re-authorizes the trustline and clears its clawback flag, which only clearing can change",
            )
            .fields(&["op.set_trust_line_flags"]),
        )?;
        let flags = self.trustline_flags(ctx, &trust_key)?;
        ensure!(
            flags == AUTHORIZED,
            "the trustline should be authorized without clawback, flags are {flags}"
        );

        let env = ctx.classic(&issuer, vec![clawback(&holder, &asset, CLAWED)])?;
        let applied = ctx.submit_and_close_failing(&env)?;
        ctx.capture(
            "clawback-not-enabled",
            Selector::ledger_of(&applied),
            Spec::new(
                "a clawback included in a ledger and failed with CLAWBACK_NOT_CLAWBACK_ENABLED because the trustline's clawback flag was cleared",
            )
            .fields(&["op.clawback.from", "op.clawback.asset_amount"]),
        )?;
        ensure!(
            matches!(
                op_result(&applied.result.result, 0)?,
                OperationResultTr::Clawback(ClawbackResult::NotClawbackEnabled)
            ),
            "core answered {:?}, expected clawback not_clawback_enabled",
            applied.result.result
        );

        let env = ctx.classic(&issuer, vec![flags_op(&holder, &asset, 0, CLAWBACK)])?;
        let r = ctx.capture_rejected(
            "set-flags-clawback-enabled",
            &env,
            Spec::new(
                "SetTrustLineFlags trying to set TRUSTLINE_CLAWBACK_ENABLED_FLAG, refused with SET_TRUST_LINE_FLAGS_MALFORMED before it reaches a ledger",
            )
            .fields(&["op.set_trust_line_flags"]),
        )?;
        ensure!(
            matches!(
                op_result(&r.result, 0)?,
                OperationResultTr::SetTrustLineFlags(SetTrustLineFlagsResult::Malformed)
            ),
            "core answered {:?}, expected set_trust_line_flags malformed",
            r.result
        );
        Ok(())
    }
}
