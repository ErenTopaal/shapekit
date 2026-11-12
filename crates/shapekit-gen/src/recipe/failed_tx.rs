//! `classic.failed-tx`: failed transactions whose envelope names accounts
//! the ledger never touches, so participants read from the envelope include
//! accounts the meta never mentions. A failed transaction still changes
//! state: the fee and sequence number of its source, and (CAP-28) the
//! removal of the pre-auth signer that authorised it.
//!
//! | ledger | content                                                              |
//! |--------|----------------------------------------------------------------------|
//! | 2      | protocol upgrade                                                     |
//! | 3      | root funds owner and cosigner                                        |
//! | 4      | owner creates ghost-created, then cosigner (as operation source)     |
//! |        | pays ghost-destination, which does not exist: the transaction fails |
//! | 5      | owner adds a pre-auth signer for the hash of the next transaction    |
//! | 6      | that transaction, with no signatures, pays ghost-preauth, which does |
//! |        | not exist: it fails and core still removes the pre-auth signer       |
//!
//! None of the three ghost accounts, nor the cosigner in ledger 4, appears
//! in any ledger entry change of those ledgers.

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use shapekit_core_driver::keys::{account_id, muxed_account};
use shapekit_core_driver::tx::{TransactionBuilder, transaction_hash};
use stellar_xdr::{
    AccountId, Asset, CreateAccountOp, CreateAccountResult, LedgerEntryChange, LedgerEntryChanges,
    LedgerEntryData, LedgerKey, LedgerKeyAccount, OperationBody, OperationResult,
    OperationResultTr, PaymentOp, PaymentResult, SetOptionsOp, Signer, SignerKey, TransactionMeta,
    TransactionResultResult, Uint256,
};

use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "classic.failed-tx";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

/// One XLM in stroops.
const UNIT: i64 = 10_000_000;

pub struct FailedTx {
    pub protocol: u32,
}

fn pay(to: &SigningKey, amount: i64) -> OperationBody {
    OperationBody::Payment(PaymentOp {
        destination: muxed_account(&to.verifying_key()),
        asset: Asset::Native,
        amount,
    })
}

fn account_key(k: &SigningKey) -> LedgerKey {
    LedgerKey::Account(LedgerKeyAccount {
        account_id: account_id(&k.verifying_key()),
    })
}

/// Every account whose entry the meta of one transaction changes.
fn touched_accounts(meta: &TransactionMeta) -> Result<Vec<AccountId>> {
    let lists: Vec<&LedgerEntryChanges> = match meta {
        TransactionMeta::V3(m) => [&m.tx_changes_before, &m.tx_changes_after]
            .into_iter()
            .chain(m.operations.iter().map(|o| &o.changes))
            .collect(),
        TransactionMeta::V4(m) => [&m.tx_changes_before, &m.tx_changes_after]
            .into_iter()
            .chain(m.operations.iter().map(|o| &o.changes))
            .collect(),
        other => bail!(
            "transaction meta v{} is not read here",
            super::select::tx_meta_version(other)
        ),
    };
    let mut out = Vec::new();
    for change in lists.into_iter().flat_map(|l| l.iter()) {
        let data = match change {
            LedgerEntryChange::Created(e)
            | LedgerEntryChange::Updated(e)
            | LedgerEntryChange::State(e)
            | LedgerEntryChange::Restored(e) => &e.data,
            LedgerEntryChange::Removed(LedgerKey::Account(k)) => {
                out.push(k.account_id.clone());
                continue;
            }
            LedgerEntryChange::Removed(_) => continue,
        };
        if let LedgerEntryData::Account(a) = data {
            out.push(a.account_id.clone());
        }
    }
    Ok(out)
}

fn ensure_untouched(
    meta: &TransactionMeta,
    named: &[(&str, &SigningKey)],
    what: &str,
) -> Result<()> {
    let touched = touched_accounts(meta)?;
    for (role, k) in named {
        ensure!(
            !touched.contains(&account_id(&k.verifying_key())),
            "{what}: the meta changes {role}, which only the envelope should name"
        );
    }
    Ok(())
}

fn op_results(r: &TransactionResultResult) -> Result<&[OperationResult]> {
    match r {
        TransactionResultResult::TxFailed(ops) => Ok(ops.as_slice()),
        other => bail!("core answered {other:?}, expected tx_failed"),
    }
}

fn is_no_destination(op: Option<&OperationResult>) -> bool {
    matches!(
        op,
        Some(OperationResult::OpInner(OperationResultTr::Payment(
            PaymentResult::NoDestination
        )))
    )
}

impl Recipe for FailedTx {
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
        ctx.fund(&["owner", "cosigner"], BALANCE)?;
        let owner = ctx.account("owner");
        let cosigner = ctx.account("cosigner");
        // derived but never funded: these accounts exist only in envelopes
        let ghost_created = ctx.account("ghost-created");
        let ghost_destination = ctx.account("ghost-destination");
        let ghost_preauth = ctx.account("ghost-preauth");

        let tx = ctx
            .tx(&owner)?
            .operation(
                None,
                OperationBody::CreateAccount(CreateAccountOp {
                    destination: account_id(&ghost_created.verifying_key()),
                    starting_balance: 100 * UNIT,
                }),
            )
            .operation(
                Some(muxed_account(&cosigner.verifying_key())),
                pay(&ghost_destination, UNIT),
            )
            .build()?;
        let env = ctx.sign(tx, &[&owner, &cosigner])?;
        let applied = ctx.submit_and_close_failing(&env)?;
        let ops = op_results(&applied.result.result)?;
        ensure!(
            matches!(
                ops.first(),
                Some(OperationResult::OpInner(OperationResultTr::CreateAccount(
                    CreateAccountResult::Success
                )))
            ) && is_no_destination(ops.get(1)),
            "expected create_account success then payment no_destination, core answered {ops:?}"
        );
        ensure_untouched(
            &applied.meta,
            &[
                ("cosigner", &cosigner),
                ("ghost-created", &ghost_created),
                ("ghost-destination", &ghost_destination),
            ],
            "failed payment",
        )?;
        ctx.capture(
            "unknown-destination",
            Selector::ledger_of(&applied),
            Spec::new(
                "a failed transaction whose envelope names a created account, an operation source and a payment destination, none of which the meta touches; only the source pays the fee and consumes the sequence number",
            )
            .fields(&[
                "tx.source_account",
                "op.source_account",
                "op.payment.destination",
                "op.create_account.destination",
            ]),
        )?;

        // the pre-authorised transaction is built first: its hash is the
        // signer key, and it uses the sequence number after the set options
        let owner_id = account_id(&owner.verifying_key());
        let seq = ctx.driver().node().account_seq(&owner_id)?;
        let preauth = TransactionBuilder::new(muxed_account(&owner.verifying_key()), seq + 2)
            .operation(None, pay(&ghost_preauth, UNIT))
            .build()?;
        let hash = transaction_hash(&preauth, &ctx.network_id())?;
        let signer = SignerKey::PreAuthTx(Uint256(hash));
        let env = ctx.classic(
            &owner,
            vec![OperationBody::SetOptions(SetOptionsOp {
                inflation_dest: None,
                clear_flags: None,
                set_flags: None,
                master_weight: None,
                low_threshold: None,
                med_threshold: None,
                high_threshold: None,
                home_domain: None,
                signer: Some(Signer {
                    key: signer.clone(),
                    weight: 1,
                }),
            })],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "add-pre-auth-signer",
            Selector::transaction(&applied),
            Spec::new(
                "the owner adds a pre-auth signer for the hash of its next transaction, which pays an account that does not exist",
            )
            .fields(&["op.set_options.signer"]),
        )?;

        let env = ctx.sign(preauth, &[])?;
        let applied = ctx.submit_and_close_failing(&env)?;
        let ops = op_results(&applied.result.result)?;
        ensure!(
            is_no_destination(ops.first()),
            "expected payment no_destination, core answered {ops:?}"
        );
        ensure_untouched(
            &applied.meta,
            &[("ghost-preauth", &ghost_preauth)],
            "failed pre-authorised payment",
        )?;
        let got = ctx.driver().node().ledger_entries(&[account_key(&owner)])?;
        let entry = got.entries[0]
            .entry
            .as_ref()
            .context("owner's account is missing")?;
        match &entry.data {
            LedgerEntryData::Account(a) => ensure!(
                a.signers.is_empty(),
                "the failed pre-authorised transaction left signers {:?}",
                a.signers
            ),
            other => bail!("owner's entry is not an account: {other:?}"),
        }
        ctx.capture(
            "pre-auth-failed-removes-signer",
            Selector::ledger_of(&applied),
            Spec::new(
                "a pre-authorised transaction with no signatures fails on a payment to an account that does not exist; core still removes the pre-auth signer (CAP-28), and the destination appears only in the envelope",
            )
            .fields(&[
                "tx.source_account",
                "tx.signatures",
                "op.payment.destination",
            ]),
        )
    }
}
