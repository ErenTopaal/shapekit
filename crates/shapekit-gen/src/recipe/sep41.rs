//! `sep41.muxed-addresses`: SEP-41 transfers on Stellar Asset Contracts
//! whose `to` argument is a muxed account address
//! (`SC_ADDRESS_TYPE_MUXED_ACCOUNT`, CAP-67). The balance lands on the base
//! account; the event's topic names the base account and its data becomes a
//! map carrying `to_muxed_id` next to `amount`.
//!
//! | ledger  | content                                                    |
//! |---------|------------------------------------------------------------|
//! | 3 to 6  | protocol 30 only: testnet Soroban settings                 |
//! | next    | root funds issuer, sender and holder                       |
//! | next 2  | sender and holder trust the credit asset                   |
//! | next    | issuer pays sender                                         |
//! | next 2  | the credit and XLM SACs are deployed                       |
//! | next    | sender sends XLM through the SAC to muxed holder           |
//! | next    | sender sends the credit asset through the SAC to muxed holder |
//! | next    | the issuer mints through the SAC to muxed holder, mux id 0 |

use anyhow::{Result, bail, ensure};
use ed25519_dalek::SigningKey;
use shapekit_core_driver::chain::Applied;
use shapekit_core_driver::keys::{account_address, account_id, muxed_account};
use stellar_xdr::{
    AlphaNum4, Asset, AssetCode4, ChangeTrustAsset, ChangeTrustOp, ContractEventBody, ContractId,
    MuxedEd25519Account, OperationBody, PaymentOp, ScAddress, ScVal, Uint256,
};

use super::cap73::{call, deploy_sac, i128_val, operation_events};
use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "sep41.muxed-addresses";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

const CODE: &[u8; 4] = b"MUXD";

/// Credit units the issuer pays the sender.
const FUNDING: i64 = 1_000_000_000;

/// Amount of every SAC transfer.
const AMOUNT: i64 = 3_000_000;

const MUX_XLM: u64 = 1_234_567_890_123;
const MUX_CREDIT: u64 = u64::MAX;
const MUX_MINT: u64 = 0;

pub struct MuxedAddresses {
    pub protocol: u32,
}

fn muxed_address(key: &SigningKey, id: u64) -> ScAddress {
    ScAddress::MuxedAccount(MuxedEd25519Account {
        id,
        ed25519: Uint256(key.verifying_key().to_bytes()),
    })
}

/// Checks the single contract event of `applied` is `name` with map data
/// `{amount, to_muxed_id}` carrying `id`.
fn ensure_muxed_event(applied: &Applied, name: &str, id: u64) -> Result<()> {
    let events = operation_events(&applied.meta)?;
    ensure!(
        events.len() == 1,
        "expected one {name} event, got {events:?}"
    );
    let ContractEventBody::V0(body) = &events[0].body;
    ensure!(
        matches!(body.topics.first(), Some(ScVal::Symbol(s)) if s.as_slice() == name.as_bytes()),
        "expected a {name} event, topics {:?}",
        body.topics
    );
    let ScVal::Map(Some(map)) = &body.data else {
        bail!(
            "{name} to a muxed address should carry map data, got {:?}",
            body.data
        );
    };
    let found = map.iter().find_map(|e| match &e.key {
        ScVal::Symbol(s) if s.as_slice() == b"to_muxed_id" => Some(&e.val),
        _ => None,
    });
    ensure!(
        found == Some(&ScVal::U64(id)),
        "{name} event data should carry to_muxed_id {id}, has {map:?}"
    );
    Ok(())
}

const TRANSFER_FIELDS: [&str; 3] = [
    "op.invoke_host_function.invoke_contract.contract_address",
    "op.invoke_host_function.invoke_contract.function_name",
    "op.invoke_host_function.invoke_contract.args",
];

/// `transfer(from, muxed to, AMOUNT)` on `sac` sent by `from`. Captured as
/// its ledger: the muxed address is only in the envelope, the mux id only
/// in the meta's event data, and neither half alone shows both.
fn transfer_to_muxed(
    ctx: &mut ChainCtx,
    sac: &ContractId,
    from: &SigningKey,
    to: &SigningKey,
    id: u64,
) -> Result<Applied> {
    let hf = call(
        sac,
        "transfer",
        vec![
            ScVal::Address(account_address(&from.verifying_key())),
            ScVal::Address(muxed_address(to, id)),
            i128_val(AMOUNT.into()),
        ],
    )?;
    let env = ctx.driver().soroban(from, hf)?;
    ctx.submit_and_close(&env)
}

impl Recipe for MuxedAddresses {
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
        ctx.fund(&["issuer", "sender", "holder"], BALANCE)?;
        let issuer = ctx.account("issuer");
        let sender = ctx.account("sender");
        let holder = ctx.account("holder");
        let a4 = AlphaNum4 {
            asset_code: AssetCode4(*CODE),
            issuer: account_id(&issuer.verifying_key()),
        };
        let asset = Asset::CreditAlphanum4(a4.clone());
        for who in [&sender, &holder] {
            let env = ctx.classic(
                who,
                vec![OperationBody::ChangeTrust(ChangeTrustOp {
                    line: ChangeTrustAsset::CreditAlphanum4(a4.clone()),
                    limit: i64::MAX,
                })],
            )?;
            ctx.submit_and_close(&env)?;
        }
        let env = ctx.classic(
            &issuer,
            vec![OperationBody::Payment(PaymentOp {
                destination: muxed_account(&sender.verifying_key()),
                asset: asset.clone(),
                amount: FUNDING,
            })],
        )?;
        ctx.submit_and_close(&env)?;
        let (_, sac) = deploy_sac(ctx, &issuer, asset)?;
        let (_, xlm) = deploy_sac(ctx, &sender, Asset::Native)?;

        let applied = transfer_to_muxed(ctx, &xlm, &sender, &holder, MUX_XLM)?;
        ensure_muxed_event(&applied, "transfer", MUX_XLM)?;
        ctx.capture(
            "xlm-transfer-to-muxed",
            Selector::ledger_of(&applied),
            Spec::new(
                "an XLM SAC transfer whose to argument is a muxed account address; the event names the base account and carries to_muxed_id in map data",
            )
            .fields(&TRANSFER_FIELDS),
        )?;

        let applied = transfer_to_muxed(ctx, &sac, &sender, &holder, MUX_CREDIT)?;
        ensure_muxed_event(&applied, "transfer", MUX_CREDIT)?;
        ctx.capture(
            "credit-transfer-to-muxed",
            Selector::ledger_of(&applied),
            Spec::new(
                "a credit asset SAC transfer whose to argument is a muxed account address with mux id 2^64-1",
            )
            .fields(&TRANSFER_FIELDS),
        )?;

        let applied = transfer_to_muxed(ctx, &sac, &issuer, &holder, MUX_MINT)?;
        ensure_muxed_event(&applied, "mint", MUX_MINT)?;
        ctx.capture(
            "credit-mint-to-muxed",
            Selector::ledger_of(&applied),
            Spec::new(
                "the issuer's SAC transfer to a muxed account address with mux id 0, which the SAC reports as a mint carrying to_muxed_id 0",
            )
            .fields(&TRANSFER_FIELDS),
        )
    }
}
