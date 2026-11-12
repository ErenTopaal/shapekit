//! `cap18.maintain-liabilities`: trustlines moved into and out of
//! AUTHORIZED_TO_MAINTAIN_LIABILITIES by AllowTrust (authorize = 2) and by
//! SetTrustLineFlags, with an open offer kept while liabilities are
//! maintained and removed on full revocation, plus the AllowTrust core
//! refuses for an authorize value with both flags.
//!
//! | ledger | content                                                         |
//! |--------|-----------------------------------------------------------------|
//! | 2      | protocol upgrade                                                |
//! | 3      | root funds the issuer and the holder                            |
//! | 4      | the issuer sets auth required and auth revocable                |
//! | 5      | the holder trusts MNT (alphanum4) and MAINTAINLIAB (alphanum12) |
//! | 6      | AllowTrust MNT authorize 1                                      |
//! | 7      | AllowTrust MAINTAINLIAB authorize 2, straight from unauthorized |
//! | 8      | the issuer pays the holder MNT                                  |
//! | 9      | the holder offers MNT for XLM                                   |
//! | 10     | AllowTrust MNT authorize 2: the offer stays                     |
//! | 11     | AllowTrust MNT authorize 0: core removes the offer              |
//! | 12     | SetTrustLineFlags MAINTAINLIAB 2 to 1 and AllowTrust MNT 0 to 2  |
//!
//! The flag values are leaves in the shape grammar, so every capture is the
//! whole ledger: its envelope carries the AllowTrust arm the family's
//! fragments name and its meta carries the trustline flags. A capture whose
//! only operation is SetTrustLineFlags would match none of this family's
//! fragments, so the SetTrustLineFlags transition shares a transaction with
//! an AllowTrust one.
//!
//! The refused envelope uses authorize = 3, which
//! `AllowTrustOpFrame::doCheckValid` (stellar-core v30.0.0) answers with
//! `ALLOW_TRUST_MALFORMED` because it exceeds
//! AUTHORIZED_TO_MAINTAIN_LIABILITIES_FLAG.

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use shapekit_core_driver::keys::{account_id, muxed_account};
use stellar_xdr::{
    AccountFlags, AllowTrustOp, AllowTrustResult, AlphaNum4, AlphaNum12, Asset, AssetCode,
    AssetCode4, AssetCode12, ChangeTrustAsset, ChangeTrustOp, LedgerEntryData, LedgerKey,
    LedgerKeyOffer, LedgerKeyTrustLine, ManageOfferSuccessResultOffer, ManageSellOfferOp,
    ManageSellOfferResult, OperationBody, OperationResultTr, PaymentOp, Price, SetOptionsOp,
    SetTrustLineFlagsOp, TrustLineAsset, TrustLineFlags,
};

use super::cap23::op_result;
use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "cap18.maintain-liabilities";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

/// Asset amounts, in stroops of the issued asset.
const PAID: i64 = 10_000_000_000;
const OFFERED: i64 = 4_000_000_000;

const AUTHORIZED: u32 = TrustLineFlags::AuthorizedFlag as u32;
const MAINTAIN: u32 = TrustLineFlags::AuthorizedToMaintainLiabilitiesFlag as u32;

const CODE4: [u8; 4] = *b"MNT\0";
const CODE12: [u8; 12] = *b"MAINTAINLIAB";

pub struct MaintainLiabilities {
    pub protocol: u32,
}

fn allow_trust(holder: &SigningKey, code: AssetCode, authorize: u32) -> OperationBody {
    OperationBody::AllowTrust(AllowTrustOp {
        trustor: account_id(&holder.verifying_key()),
        asset: code,
        authorize,
    })
}

impl MaintainLiabilities {
    fn trustline_flags(&self, ctx: &mut ChainCtx, key: &LedgerKey) -> Result<u32> {
        let found = ctx
            .driver()
            .node()
            .ledger_entries(std::slice::from_ref(key))?;
        match found.entries[0].entry.as_ref().map(|e| &e.data) {
            Some(LedgerEntryData::Trustline(t)) => Ok(t.flags),
            other => bail!("trustline {key:?} is missing: {other:?}"),
        }
    }

    fn offer_exists(&self, ctx: &mut ChainCtx, key: &LedgerKey) -> Result<bool> {
        let found = ctx
            .driver()
            .node()
            .ledger_entries(std::slice::from_ref(key))?;
        Ok(found.entries[0].entry.is_some())
    }
}

fn ensure_allow_trust_ok(r: &stellar_xdr::TransactionResultResult, index: usize) -> Result<()> {
    ensure!(
        matches!(
            op_result(r, index)?,
            OperationResultTr::AllowTrust(AllowTrustResult::Success)
        ),
        "operation {index} should be a successful AllowTrust, core answered {r:?}"
    );
    Ok(())
}

impl Recipe for MaintainLiabilities {
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
        ctx.fund(&["issuer", "holder"], BALANCE)?;
        let issuer = ctx.account("issuer");
        let holder = ctx.account("holder");
        let issuer_id = account_id(&issuer.verifying_key());
        let a4 = AlphaNum4 {
            asset_code: AssetCode4(CODE4),
            issuer: issuer_id.clone(),
        };
        let a12 = AlphaNum12 {
            asset_code: AssetCode12(CODE12),
            issuer: issuer_id,
        };
        let mnt = Asset::CreditAlphanum4(a4.clone());
        let code4 = AssetCode::CreditAlphanum4(AssetCode4(CODE4));
        let code12 = AssetCode::CreditAlphanum12(AssetCode12(CODE12));
        let holder_id = account_id(&holder.verifying_key());
        let key4 = LedgerKey::Trustline(LedgerKeyTrustLine {
            account_id: holder_id.clone(),
            asset: TrustLineAsset::CreditAlphanum4(a4.clone()),
        });
        let key12 = LedgerKey::Trustline(LedgerKeyTrustLine {
            account_id: holder_id.clone(),
            asset: TrustLineAsset::CreditAlphanum12(a12.clone()),
        });

        // revocable is what lets the issuer step down from authorized
        let env = ctx.classic(
            &issuer,
            vec![OperationBody::SetOptions(SetOptionsOp {
                inflation_dest: None,
                clear_flags: None,
                set_flags: Some(
                    AccountFlags::RequiredFlag as u32 | AccountFlags::RevocableFlag as u32,
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
            vec![
                OperationBody::ChangeTrust(ChangeTrustOp {
                    line: ChangeTrustAsset::CreditAlphanum4(a4),
                    limit: i64::MAX,
                }),
                OperationBody::ChangeTrust(ChangeTrustOp {
                    line: ChangeTrustAsset::CreditAlphanum12(a12),
                    limit: i64::MAX,
                }),
            ],
        )?;
        ctx.submit_and_close(&env)?;
        ensure!(
            self.trustline_flags(ctx, &key4)? == 0 && self.trustline_flags(ctx, &key12)? == 0,
            "trustlines to an auth-required issuer should start unauthorized"
        );

        let env = ctx.classic(&issuer, vec![allow_trust(&holder, code4.clone(), 1)])?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "allow-trust-authorize-alphanum4",
            Selector::ledger_of(&applied),
            Spec::new("AllowTrust with authorize 1 on an alphanum4 trustline")
                .fields(&["op.allow_trust"]),
        )?;
        ensure!(self.trustline_flags(ctx, &key4)? == AUTHORIZED);

        let env = ctx.classic(&issuer, vec![allow_trust(&holder, code12.clone(), 2)])?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "allow-trust-unauthorized-to-maintain-alphanum12",
            Selector::ledger_of(&applied),
            Spec::new(
                "AllowTrust with authorize 2 on an unauthorized alphanum12 trustline, which goes straight to AUTHORIZED_TO_MAINTAIN_LIABILITIES",
            )
            .fields(&["op.allow_trust"]),
        )?;
        ensure!(self.trustline_flags(ctx, &key12)? == MAINTAIN);

        let env = ctx.classic(
            &issuer,
            vec![OperationBody::Payment(PaymentOp {
                destination: muxed_account(&holder.verifying_key()),
                asset: mnt.clone(),
                amount: PAID,
            })],
        )?;
        ctx.submit_and_close(&env)?;
        let env = ctx.classic(
            &holder,
            vec![OperationBody::ManageSellOffer(ManageSellOfferOp {
                selling: mnt,
                buying: Asset::Native,
                amount: OFFERED,
                price: Price { n: 2, d: 1 },
                offer_id: 0,
            })],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        let offer_id = match op_result(&applied.result.result, 0)? {
            OperationResultTr::ManageSellOffer(ManageSellOfferResult::Success(s)) => {
                match &s.offer {
                    ManageOfferSuccessResultOffer::Created(o) => Some(o.offer_id),
                    _ => None,
                }
            }
            _ => None,
        }
        .context("the holder's offer should rest on the book")?;
        let offer_key = LedgerKey::Offer(LedgerKeyOffer {
            seller_id: holder_id,
            offer_id,
        });

        let env = ctx.classic(&issuer, vec![allow_trust(&holder, code4.clone(), 2)])?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "allow-trust-authorized-to-maintain-alphanum4",
            Selector::ledger_of(&applied),
            Spec::new(
                "AllowTrust with authorize 2 on an authorized alphanum4 trustline with an open offer: the trustline keeps its liabilities and the offer stays",
            )
            .fields(&["op.allow_trust"]),
        )?;
        ensure!(self.trustline_flags(ctx, &key4)? == MAINTAIN);
        ensure!(
            self.offer_exists(ctx, &offer_key)?,
            "the offer should survive authorize 2"
        );

        let env = ctx.classic(&issuer, vec![allow_trust(&holder, code4.clone(), 0)])?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "allow-trust-maintain-to-revoked-alphanum4",
            Selector::ledger_of(&applied),
            Spec::new(
                "AllowTrust with authorize 0 on a trustline authorized to maintain liabilities: core removes the holder's offer in the same operation",
            )
            .fields(&["op.allow_trust"]),
        )?;
        ensure!(self.trustline_flags(ctx, &key4)? == 0);
        ensure!(
            !self.offer_exists(ctx, &offer_key)?,
            "full revocation should remove the offer"
        );

        let tx = ctx
            .tx(&issuer)?
            .operation(
                None,
                OperationBody::SetTrustLineFlags(SetTrustLineFlagsOp {
                    trustor: account_id(&holder.verifying_key()),
                    asset: Asset::CreditAlphanum12(AlphaNum12 {
                        asset_code: AssetCode12(CODE12),
                        issuer: account_id(&issuer.verifying_key()),
                    }),
                    clear_flags: MAINTAIN,
                    set_flags: AUTHORIZED,
                }),
            )
            .operation(None, allow_trust(&holder, code4.clone(), 2))
            .build()?;
        let env = ctx.sign(tx, &[&issuer])?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "set-flags-and-allow-trust-maintain",
            Selector::ledger_of(&applied),
            Spec::new(
                "one transaction moving an alphanum12 trustline from maintain liabilities to authorized with SetTrustLineFlags and an alphanum4 trustline from unauthorized to maintain liabilities with AllowTrust",
            )
            .fields(&["op.allow_trust", "op.set_trust_line_flags"]),
        )?;
        ensure_allow_trust_ok(&applied.result.result, 1)?;
        ensure!(self.trustline_flags(ctx, &key12)? == AUTHORIZED);
        ensure!(self.trustline_flags(ctx, &key4)? == MAINTAIN);

        let env = ctx.classic(
            &issuer,
            vec![allow_trust(&holder, code4, AUTHORIZED | MAINTAIN)],
        )?;
        let r = ctx.capture_rejected(
            "allow-trust-both-flags",
            &env,
            Spec::new(
                "AllowTrust with authorize 3, both authorization flags at once, refused with ALLOW_TRUST_MALFORMED before it reaches a ledger",
            )
            .fields(&["op.allow_trust"]),
        )?;
        ensure!(
            matches!(
                op_result(&r.result, 0)?,
                OperationResultTr::AllowTrust(AllowTrustResult::Malformed)
            ),
            "core answered {:?}, expected allow_trust malformed",
            r.result
        );
        Ok(())
    }
}
