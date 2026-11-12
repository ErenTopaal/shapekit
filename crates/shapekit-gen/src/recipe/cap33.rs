//! `cap33.sponsorship`: an account, a trustline, a signer and a claimable
//! balance created under another account's sponsorship, the entries that
//! record it, and revokes of the signer's and the trustline's sponsorship.
//!
//! | ledger | content                                                         |
//! |--------|-----------------------------------------------------------------|
//! | 2      | protocol upgrade                                                |
//! | 3      | root funds the sponsor and the issuer                           |
//! | 4      | the sponsor creates the sponsored account under sponsorship     |
//! | 5      | the sponsored account adds a trustline under sponsorship        |
//! | 6      | the sponsored account adds an ed25519 signer under sponsorship  |
//! | 7      | the sponsored account creates a claimable balance under sponsorship |
//! | 8      | the sponsor revokes its sponsorship of the signer               |
//! | 9      | the sponsor revokes its sponsorship of the trustline            |

use anyhow::{Result, bail, ensure};
use ed25519_dalek::SigningKey;
use shapekit_core_driver::keys::{account_id, ed25519_signer, muxed_account};
use stellar_xdr::{
    AccountEntry, AccountEntryExt, AccountEntryExtensionV1Ext, AccountId, AlphaNum4, Asset,
    AssetCode4, BeginSponsoringFutureReservesOp, ChangeTrustAsset, ChangeTrustOp, ClaimPredicate,
    Claimant, ClaimantV0, CreateAccountOp, CreateClaimableBalanceOp, LedgerEntry, LedgerEntryData,
    LedgerEntryExt, LedgerKey, LedgerKeyAccount, LedgerKeyTrustLine, OperationBody,
    RevokeSponsorshipOp, RevokeSponsorshipOpSigner, SetOptionsOp, Signer, TrustLineAsset,
};

use super::cap23::{balance_key, created_balance_id};
use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "cap33.sponsorship";

/// Starting balance of the root-funded accounts, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

/// Starting balance of the sponsored account, 100 XLM, enough to take over
/// the reserves the revokes hand back to it.
const SPONSORED_BALANCE: i64 = 1_000_000_000;

/// Amount held by the sponsored claimable balance, 10 XLM.
const CB_AMOUNT: i64 = 100_000_000;

pub struct Sponsorship {
    pub protocol: u32,
}

fn begin(sponsored: &SigningKey) -> OperationBody {
    OperationBody::BeginSponsoringFutureReserves(BeginSponsoringFutureReservesOp {
        sponsored_id: account_id(&sponsored.verifying_key()),
    })
}

fn account_key(key: &SigningKey) -> LedgerKey {
    LedgerKey::Account(LedgerKeyAccount {
        account_id: account_id(&key.verifying_key()),
    })
}

/// The sponsoring id core recorded on an entry, if any.
fn sponsor_of(e: &LedgerEntry) -> Option<&AccountId> {
    match &e.ext {
        LedgerEntryExt::V1(v1) => v1.sponsoring_id.0.as_ref(),
        LedgerEntryExt::V0 => None,
    }
}

fn signer_sponsors(a: &AccountEntry) -> Vec<Option<AccountId>> {
    match &a.ext {
        AccountEntryExt::V1(v1) => match &v1.ext {
            AccountEntryExtensionV1Ext::V2(v2) => v2
                .signer_sponsoring_i_ds
                .iter()
                .map(|d| d.0.clone())
                .collect(),
            AccountEntryExtensionV1Ext::V0 => Vec::new(),
        },
        AccountEntryExt::V0 => Vec::new(),
    }
}

impl Sponsorship {
    fn entry(&self, ctx: &mut ChainCtx, key: &LedgerKey) -> Result<LedgerEntry> {
        let found = ctx
            .driver()
            .node()
            .ledger_entries(std::slice::from_ref(key))?;
        match found.entries.into_iter().next().and_then(|e| e.entry) {
            Some(e) => Ok(e),
            None => bail!("ledger entry {key:?} is missing"),
        }
    }
}

impl Recipe for Sponsorship {
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
        ctx.fund(&["sponsor", "issuer"], BALANCE)?;
        let sponsor = ctx.account("sponsor");
        let sponsored = ctx.account("sponsored");
        let issuer = ctx.account("issuer");
        // a key that signs for the sponsored account, never an account itself
        let signer = ctx.account("signer");
        let sponsor_id = account_id(&sponsor.verifying_key());
        let sponsored_mux = Some(muxed_account(&sponsored.verifying_key()));

        let tx = ctx
            .tx(&sponsor)?
            .operation(None, begin(&sponsored))
            .operation(
                None,
                OperationBody::CreateAccount(CreateAccountOp {
                    destination: account_id(&sponsored.verifying_key()),
                    starting_balance: SPONSORED_BALANCE,
                }),
            )
            .operation(
                sponsored_mux.clone(),
                OperationBody::EndSponsoringFutureReserves,
            )
            .build()?;
        let env = ctx.sign(tx, &[&sponsor, &sponsored])?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "sponsored-account",
            Selector::transaction(&applied),
            Spec::new(
                "the sponsor creates an account inside begin and end sponsoring, so the new account's base reserve is the sponsor's",
            )
            .fields(&["op.begin_sponsoring.sponsored_id", "op.create_account.destination"]),
        )?;
        let acct = self.entry(ctx, &account_key(&sponsored))?;
        ensure!(
            sponsor_of(&acct) == Some(&sponsor_id),
            "the sponsored account should name the sponsor, has {:?}",
            acct.ext
        );

        let asset = AlphaNum4 {
            asset_code: AssetCode4(*b"SPON"),
            issuer: account_id(&issuer.verifying_key()),
        };
        let tx = ctx
            .tx(&sponsor)?
            .operation(None, begin(&sponsored))
            .operation(
                sponsored_mux.clone(),
                OperationBody::ChangeTrust(ChangeTrustOp {
                    line: ChangeTrustAsset::CreditAlphanum4(asset.clone()),
                    limit: i64::MAX,
                }),
            )
            .operation(
                sponsored_mux.clone(),
                OperationBody::EndSponsoringFutureReserves,
            )
            .build()?;
        let env = ctx.sign(tx, &[&sponsor, &sponsored])?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "sponsored-trustline",
            Selector::transaction(&applied),
            Spec::new(
                "a trustline created inside begin and end sponsoring, whose reserve the sponsor pays",
            )
            .fields(&["op.begin_sponsoring.sponsored_id", "op.change_trust.line"]),
        )?;
        let trust_key = LedgerKey::Trustline(LedgerKeyTrustLine {
            account_id: account_id(&sponsored.verifying_key()),
            asset: TrustLineAsset::CreditAlphanum4(asset),
        });
        ctx.capture(
            "sponsored-trustline-entry",
            Selector::LedgerEntry {
                ledger: applied.ledger,
                key: trust_key.clone(),
            },
            Spec::new("a trustline entry whose ledger entry extension names its sponsor"),
        )?;
        let tl = self.entry(ctx, &trust_key)?;
        ensure!(
            sponsor_of(&tl) == Some(&sponsor_id),
            "the trustline should name the sponsor, has {:?}",
            tl.ext
        );

        let signer_key = ed25519_signer(&signer.verifying_key());
        let tx = ctx
            .tx(&sponsor)?
            .operation(None, begin(&sponsored))
            .operation(
                sponsored_mux.clone(),
                OperationBody::SetOptions(SetOptionsOp {
                    inflation_dest: None,
                    clear_flags: None,
                    set_flags: None,
                    master_weight: None,
                    low_threshold: None,
                    med_threshold: None,
                    high_threshold: None,
                    home_domain: None,
                    signer: Some(Signer {
                        key: signer_key.clone(),
                        weight: 1,
                    }),
                }),
            )
            .operation(
                sponsored_mux.clone(),
                OperationBody::EndSponsoringFutureReserves,
            )
            .build()?;
        let env = ctx.sign(tx, &[&sponsor, &sponsored])?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "sponsored-signer",
            Selector::transaction(&applied),
            Spec::new(
                "an ed25519 signer added inside begin and end sponsoring, recorded in the account's signer sponsoring ids",
            )
            .fields(&["op.begin_sponsoring.sponsored_id", "op.set_options.signer"]),
        )?;
        ctx.capture(
            "sponsored-account-entry",
            Selector::LedgerEntry {
                ledger: applied.ledger,
                key: account_key(&sponsored),
            },
            Spec::new(
                "an account that is itself sponsored and holds a sponsored signer: ledger entry extension v1 and account extension v2 with a signer sponsoring id",
            )
            .fields(&["op.set_options.signer"]),
        )?;
        let acct = self.entry(ctx, &account_key(&sponsored))?;
        match &acct.data {
            LedgerEntryData::Account(a) => ensure!(
                signer_sponsors(a) == vec![Some(sponsor_id.clone())],
                "the signer should be sponsored by the sponsor, ids are {:?}",
                signer_sponsors(a)
            ),
            other => bail!("expected an account entry, found {other:?}"),
        }

        let tx = ctx
            .tx(&sponsor)?
            .operation(None, begin(&sponsored))
            .operation(
                sponsored_mux.clone(),
                OperationBody::CreateClaimableBalance(CreateClaimableBalanceOp {
                    asset: Asset::Native,
                    amount: CB_AMOUNT,
                    claimants: vec![Claimant::ClaimantTypeV0(ClaimantV0 {
                        destination: sponsor_id.clone(),
                        predicate: ClaimPredicate::Unconditional,
                    })]
                    .try_into()?,
                }),
            )
            .operation(sponsored_mux, OperationBody::EndSponsoringFutureReserves)
            .build()?;
        let env = ctx.sign(tx, &[&sponsor, &sponsored])?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "sponsored-claimable-balance",
            Selector::transaction(&applied),
            Spec::new(
                "a claimable balance created inside begin and end sponsoring, so its reserve is the sponsor's and not its creator's",
            )
            .fields(&[
                "op.begin_sponsoring.sponsored_id",
                "op.create_claimable_balance.asset_amount",
            ]),
        )?;
        let cb_key = balance_key(&created_balance_id(&applied)?);
        ctx.capture(
            "sponsored-claimable-balance-entry",
            Selector::LedgerEntry {
                ledger: applied.ledger,
                key: cb_key.clone(),
            },
            Spec::new(
                "a claimable balance entry whose sponsoring id is the sponsor rather than the account that created it",
            ),
        )?;
        let cb = self.entry(ctx, &cb_key)?;
        ensure!(
            sponsor_of(&cb) == Some(&sponsor_id),
            "the claimable balance should name the sponsor, has {:?}",
            cb.ext
        );

        let env = ctx.classic(
            &sponsor,
            vec![OperationBody::RevokeSponsorship(
                RevokeSponsorshipOp::Signer(RevokeSponsorshipOpSigner {
                    account_id: account_id(&sponsored.verifying_key()),
                    signer_key,
                }),
            )],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "revoke-signer-sponsorship",
            Selector::transaction(&applied),
            Spec::new(
                "the sponsor revokes its sponsorship of an ed25519 signer, so the account's signer sponsoring id becomes empty and it pays the reserve",
            )
            .fields(&["op.revoke_sponsorship"]),
        )?;
        let acct = self.entry(ctx, &account_key(&sponsored))?;
        match &acct.data {
            LedgerEntryData::Account(a) => ensure!(
                signer_sponsors(a) == vec![None],
                "the signer should no longer be sponsored, ids are {:?}",
                signer_sponsors(a)
            ),
            other => bail!("expected an account entry, found {other:?}"),
        }

        let env = ctx.classic(
            &sponsor,
            vec![OperationBody::RevokeSponsorship(
                RevokeSponsorshipOp::LedgerEntry(trust_key.clone()),
            )],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "revoke-trustline-sponsorship",
            Selector::transaction(&applied),
            Spec::new(
                "the sponsor revokes its sponsorship of a trustline, keyed by the trustline's ledger key",
            )
            .fields(&["op.revoke_sponsorship"]),
        )?;
        let tl = self.entry(ctx, &trust_key)?;
        ensure!(
            sponsor_of(&tl).is_none(),
            "the trustline should no longer name a sponsor, has {:?}",
            tl.ext
        );
        Ok(())
    }
}
