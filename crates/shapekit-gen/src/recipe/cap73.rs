//! `cap73.sac-trust`: CAP-73 on the Stellar Asset Contract. `trust(address)`
//! creates a trustline with limit `i64::MAX` and emits no event, a second
//! call finds the trustline and changes nothing, and an XLM SAC transfer to
//! an account that does not exist creates it.
//!
//! | ledger | content                                                   |
//! |--------|-----------------------------------------------------------|
//! | 2      | protocol upgrade                                          |
//! | 3 to 6 | protocol 30 only: testnet Soroban settings                |
//! | next   | root funds issuer and holder                              |
//! | next   | issuer deploys the SAC of its credit asset                |
//! | next   | holder deploys the XLM SAC                                |
//! | next   | holder calls `trust(holder)` on the credit SAC            |
//! | next   | holder calls `trust(holder)` again                        |
//! | next   | holder transfers XLM through the SAC to a missing account |
//!
//! Recorded at protocol 30 and at 28, which is what the pre-fix
//! wallet-backend commit of #731 reads.

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use shapekit_core_driver::chain::Applied;
use shapekit_core_driver::keys::{account_address, account_id};
use stellar_xdr::{
    AlphaNum4, Asset, AssetCode4, ContractEvent, ContractExecutable, ContractId,
    ContractIdPreimage, CreateContractArgs, HostFunction, Int128Parts, InvokeContractArgs,
    LedgerEntryChange, LedgerEntryData, ScAddress, ScSymbol, ScVal, TransactionMeta,
};

use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "cap73.sac-trust";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

/// What the XLM SAC sends to the missing account, 100 XLM.
const NEW_ACCOUNT_AMOUNT: i64 = 1_000_000_000;

pub struct SacTrust {
    pub protocol: u32,
}

pub(crate) fn i128_val(v: i128) -> ScVal {
    ScVal::I128(Int128Parts {
        hi: (v >> 64) as i64,
        lo: v as u64,
    })
}

/// A call of `function` on `contract`.
pub(crate) fn call(
    contract: &ContractId,
    function: &str,
    args: Vec<ScVal>,
) -> Result<HostFunction> {
    Ok(HostFunction::InvokeContract(InvokeContractArgs {
        contract_address: ScAddress::Contract(contract.clone()),
        function_name: ScSymbol(function.try_into()?),
        args: args.try_into()?,
    }))
}

/// A credit asset with a four letter code issued by `issuer`.
pub(crate) fn credit(code: &[u8; 4], issuer: &SigningKey) -> Asset {
    Asset::CreditAlphanum4(AlphaNum4 {
        asset_code: AssetCode4(*code),
        issuer: account_id(&issuer.verifying_key()),
    })
}

/// Deploys the SAC of `asset` from `source`; returns the applied
/// transaction and the contract id the host returned.
pub(crate) fn deploy_sac(
    ctx: &mut ChainCtx,
    source: &SigningKey,
    asset: Asset,
) -> Result<(Applied, ContractId)> {
    let hf = HostFunction::CreateContract(CreateContractArgs {
        contract_id_preimage: ContractIdPreimage::Asset(asset),
        executable: ContractExecutable::StellarAsset,
    });
    let env = ctx.driver().soroban(source, hf)?;
    let applied = ctx.submit_and_close(&env)?;
    match applied.return_value() {
        Some(ScVal::Address(ScAddress::Contract(id))) => {
            let id = id.clone();
            Ok((applied, id))
        }
        other => bail!("sac deploy returned {other:?}"),
    }
}

/// Every ledger entry change of every operation of `meta`.
pub(crate) fn operation_changes(meta: &TransactionMeta) -> Vec<LedgerEntryChange> {
    match meta {
        TransactionMeta::V4(m) => m
            .operations
            .iter()
            .flat_map(|o| o.changes.iter().cloned())
            .collect(),
        TransactionMeta::V3(m) => m
            .operations
            .iter()
            .flat_map(|o| o.changes.iter().cloned())
            .collect(),
        TransactionMeta::V2(m) => m
            .operations
            .iter()
            .flat_map(|o| o.changes.iter().cloned())
            .collect(),
        TransactionMeta::V1(m) => m
            .operations
            .iter()
            .flat_map(|o| o.changes.iter().cloned())
            .collect(),
        TransactionMeta::V0(ops) => ops.iter().flat_map(|o| o.changes.iter().cloned()).collect(),
    }
}

/// Contract events of every operation of a v4 meta. Older meta versions
/// keep Soroban events elsewhere and are not produced at 28 or 30.
pub(crate) fn operation_events(meta: &TransactionMeta) -> Result<Vec<ContractEvent>> {
    match meta {
        TransactionMeta::V4(m) => Ok(m
            .operations
            .iter()
            .flat_map(|o| o.events.iter().cloned())
            .collect()),
        other => bail!("expected transaction meta v4, got {other:?}"),
    }
}

fn created(changes: &[LedgerEntryChange]) -> Vec<&LedgerEntryData> {
    changes
        .iter()
        .filter_map(|c| match c {
            LedgerEntryChange::Created(e) => Some(&e.data),
            _ => None,
        })
        .collect()
}

fn touches_trustline(c: &LedgerEntryChange) -> bool {
    match c {
        LedgerEntryChange::Created(e)
        | LedgerEntryChange::Updated(e)
        | LedgerEntryChange::State(e)
        | LedgerEntryChange::Restored(e) => matches!(e.data, LedgerEntryData::Trustline(_)),
        LedgerEntryChange::Removed(k) => matches!(k, stellar_xdr::LedgerKey::Trustline(_)),
    }
}

const INVOKE_FIELDS: [&str; 3] = [
    "op.invoke_host_function.invoke_contract.contract_address",
    "op.invoke_host_function.invoke_contract.function_name",
    "op.invoke_host_function.invoke_contract.args",
];

impl Recipe for SacTrust {
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
        self.protocol == 30
    }

    fn soroban_limits(&self) -> bool {
        true
    }

    fn run(&self, ctx: &mut ChainCtx) -> Result<()> {
        ctx.fund(&["issuer", "holder"], BALANCE)?;
        let issuer = ctx.account("issuer");
        let holder = ctx.account("holder");
        // never funded: the XLM SAC transfer is what creates it
        let newcomer = ctx.account("newcomer");

        let (applied, sac) = deploy_sac(ctx, &issuer, credit(b"TRST", &issuer))?;
        ctx.capture(
            "deploy-credit-sac",
            Selector::transaction(&applied),
            Spec::new("the issuer deploys the stellar asset contract of its credit asset")
                .fields(&["op.invoke_host_function.create_contract.preimage"]),
        )?;
        let (applied, xlm) = deploy_sac(ctx, &holder, Asset::Native)?;
        ctx.capture(
            "deploy-xlm-sac",
            Selector::transaction(&applied),
            Spec::new("an account deploys the stellar asset contract of the native asset")
                .fields(&["op.invoke_host_function.create_contract.preimage"]),
        )?;

        let holder_addr = ScVal::Address(account_address(&holder.verifying_key()));
        let trust = call(&sac, "trust", vec![holder_addr.clone()])?;
        let env = ctx.driver().soroban(&holder, trust.clone())?;
        let applied = ctx.submit_and_close(&env)?;
        let changes = operation_changes(&applied.meta);
        let lines: Vec<_> = created(&changes)
            .into_iter()
            .filter_map(|d| match d {
                LedgerEntryData::Trustline(t) => Some(t),
                _ => None,
            })
            .collect();
        ensure!(
            lines.len() == 1
                && lines[0].limit == i64::MAX
                && lines[0].account_id == account_id(&holder.verifying_key()),
            "trust should create one i64::MAX trustline for the holder, created {lines:?}"
        );
        let events = operation_events(&applied.meta)?;
        ensure!(
            events.is_empty(),
            "trust should emit no contract event, emitted {events:?}"
        );
        ctx.capture(
            "trust",
            Selector::transaction(&applied),
            Spec::new(
                "SAC trust(address) creates a trustline with limit i64::MAX for the address, authorised by the transaction source, and emits no event",
            )
            .fields(&INVOKE_FIELDS),
        )?;

        let env = ctx.driver().soroban(&holder, trust)?;
        let applied = ctx.submit_and_close(&env)?;
        let changes = operation_changes(&applied.meta);
        ensure!(
            !changes.iter().any(touches_trustline),
            "a second trust should leave the trustline alone, changes {changes:?}"
        );
        let events = operation_events(&applied.meta)?;
        ensure!(
            events.is_empty(),
            "a second trust should emit no contract event, emitted {events:?}"
        );
        ctx.capture(
            "trust-again",
            // its meta alone holds nothing of the family's shape (that is
            // the point), so the ledger, whose tx set holds the call
            Selector::ledger_of(&applied),
            Spec::new(
                "a second SAC trust(address) for an existing trustline succeeds without changing any ledger entry or emitting an event",
            )
            .fields(&INVOKE_FIELDS),
        )?;

        let to_new = call(
            &xlm,
            "transfer",
            vec![
                holder_addr,
                ScVal::Address(account_address(&newcomer.verifying_key())),
                i128_val(NEW_ACCOUNT_AMOUNT.into()),
            ],
        )?;
        let env = ctx.driver().soroban(&holder, to_new)?;
        let applied = ctx.submit_and_close(&env)?;
        let changes = operation_changes(&applied.meta);
        let accounts: Vec<_> = created(&changes)
            .into_iter()
            .filter_map(|d| match d {
                LedgerEntryData::Account(a) => Some(a),
                _ => None,
            })
            .collect();
        ensure!(
            accounts.len() == 1
                && accounts[0].account_id == account_id(&newcomer.verifying_key())
                && accounts[0].balance == NEW_ACCOUNT_AMOUNT,
            "the xlm transfer should create the newcomer with {NEW_ACCOUNT_AMOUNT}, created {accounts:?}"
        );
        ctx.capture(
            "xlm-transfer-creates-account",
            Selector::transaction(&applied),
            Spec::new(
                "an XLM SAC transfer to an account that does not exist creates it with the transferred amount",
            )
            .fields(&INVOKE_FIELDS),
        )
        .context("capturing the account-creating transfer")
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use stellar_xdr::{Limits, ReadXdr, TransactionResultMetaV1};

    use super::*;

    /// The recorded canonical meta of the trust call, both protocols and
    /// both networks: one trustline created with limit i64::MAX, no event
    /// from the operation.
    #[test]
    fn recorded_trust_creates_unlimited_trustline_without_event() {
        let entries =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/entries/cap73.sac-trust");
        for protocol in [30, 28] {
            for network in ["testnet", "mainnet"] {
                let path = entries
                    .join(format!("{FAMILY}.p{protocol}-trust-meta"))
                    .join(format!("{network}.xdr"));
                let bytes =
                    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                let m = TransactionResultMetaV1::from_xdr(bytes, Limits::none()).unwrap();
                let changes = operation_changes(&m.tx_apply_processing);
                let lines: Vec<_> = created(&changes)
                    .into_iter()
                    .filter_map(|d| match d {
                        LedgerEntryData::Trustline(t) => Some(t.limit),
                        _ => None,
                    })
                    .collect();
                assert_eq!(lines, vec![i64::MAX], "{}", path.display());
                assert!(
                    operation_events(&m.tx_apply_processing).unwrap().is_empty(),
                    "{}",
                    path.display()
                );
            }
        }
    }
}
