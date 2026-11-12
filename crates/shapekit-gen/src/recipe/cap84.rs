//! `cap84.muxed-contract`: CAP-84 muxed contract addresses (strkey `W...`,
//! `SC_ADDRESS_TYPE_MUXED_CONTRACT`) as the destination of a Stellar Asset
//! Contract `transfer`, the only SAC function that takes one.
//!
//! | ledger | content                                                      |
//! |--------|--------------------------------------------------------------|
//! | 2      | protocol upgrade                                             |
//! | 3 to 6 | testnet Soroban settings                                     |
//! | next   | root funds issuer and sender                                 |
//! | next   | sender uploads echo, then deploys an instance (two ledgers) |
//! | next   | sender deploys the XLM SAC, issuer the SAC of its asset      |
//! | next   | XLM transfer from the sender to the echo instance, muxed     |
//! | next   | issuer transfer of its asset to the same contract, muxed     |
//! | next   | issuer `mint` to the muxed address, refused by the host      |
//!
//! The event names the plain contract (`C...`) in its `to` topic and moves
//! the mux id into the data, which becomes `{amount, to_muxed_id}`; the
//! muxed address itself only exists in the envelope, so every capture is a
//! whole ledger.

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use shapekit_core_driver::chain::Applied;
use shapekit_core_driver::keys::account_address;
use stellar_xdr::{
    Asset, ContractId, HostFunction, MuxedContract, ScAddress, ScMap, ScMapEntry, ScString,
    ScSymbol, ScVal, SorobanAuthorizationEntry, SorobanAuthorizedFunction,
    SorobanAuthorizedInvocation, SorobanCredentials,
};

use super::cap73::{call, credit, deploy_sac, i128_val, operation_events};
use super::scval::{BALANCE, apply_failing, deploy_echo, ensure_trapped, simulated_data};
use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "cap84.muxed-contract";

/// XLM sent to the muxed contract, 25 XLM.
const XLM_AMOUNT: i128 = 250_000_000;

/// Credit asset units the issuer sends, 1000.0000000.
const CREDIT_AMOUNT: i128 = 10_000_000_000;

const FIELDS: [&str; 2] = [
    "op.invoke_host_function.invoke_contract.args",
    "op.invoke_host_function.auth",
];

pub struct MuxedContractDestination {
    pub protocol: u32,
}

fn muxed(contract: &ContractId, id: u64) -> ScVal {
    ScVal::Address(ScAddress::MuxedContract(MuxedContract {
        id,
        contract_id: contract.clone(),
    }))
}

fn sym(s: &str) -> Result<ScVal> {
    Ok(ScVal::Symbol(ScSymbol(s.try_into()?)))
}

/// Checks the single SAC event of `applied`: `topics` and the map data
/// `{amount, to_muxed_id}` the host writes for a muxed destination.
fn ensure_event(applied: &Applied, topics: &[ScVal], amount: i128, mux_id: u64) -> Result<()> {
    let events = operation_events(&applied.meta)?;
    let [e] = events.as_slice() else {
        bail!(
            "ledger {}: expected one contract event, got {events:?}",
            applied.ledger
        );
    };
    let stellar_xdr::ContractEventBody::V0(body) = &e.body;
    ensure!(
        body.topics.as_slice() == topics,
        "ledger {}: event topics {:?}, expected {topics:?}",
        applied.ledger,
        body.topics
    );
    let want = ScVal::Map(Some(ScMap(
        vec![
            ScMapEntry {
                key: sym("amount")?,
                val: i128_val(amount),
            },
            ScMapEntry {
                key: sym("to_muxed_id")?,
                val: ScVal::U64(mux_id),
            },
        ]
        .try_into()?,
    )));
    ensure!(
        body.data == want,
        "ledger {}: event data {:?}, expected {want:?}",
        applied.ledger,
        body.data
    );
    Ok(())
}

fn asset_name(s: &str) -> Result<ScVal> {
    Ok(ScVal::String(ScString(s.try_into()?)))
}

/// A source-account auth entry for exactly this invocation.
fn source_auth(hf: &HostFunction) -> Result<SorobanAuthorizationEntry> {
    let HostFunction::InvokeContract(args) = hf else {
        bail!("source_auth needs a contract call");
    };
    Ok(SorobanAuthorizationEntry {
        credentials: SorobanCredentials::SourceAccount,
        root_invocation: SorobanAuthorizedInvocation {
            function: SorobanAuthorizedFunction::ContractFn(args.clone()),
            sub_invocations: Default::default(),
        },
    })
}

fn invoke_ok(ctx: &mut ChainCtx, source: &SigningKey, hf: HostFunction) -> Result<Applied> {
    let env = ctx.driver().soroban(source, hf)?;
    ctx.submit_and_close(&env)
}

impl Recipe for MuxedContractDestination {
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
        ctx.fund(&["issuer", "sender"], BALANCE)?;
        let issuer = ctx.account("issuer");
        let sender = ctx.account("sender");
        let target = deploy_echo(ctx, &sender, FAMILY)?;
        let (_, xlm) = deploy_sac(ctx, &sender, Asset::Native)?;
        let asset = credit(b"MUXC", &issuer);
        let (_, sac) = deploy_sac(ctx, &issuer, asset)?;
        let issuer_name = format!(
            "MUXC:{}",
            stellar_strkey::ed25519::PublicKey(issuer.verifying_key().to_bytes())
        );

        // the largest mux id, where a 53-bit number type loses precision
        let id = u64::MAX;
        let from = ScVal::Address(account_address(&sender.verifying_key()));
        let hf = call(
            &xlm,
            "transfer",
            vec![from.clone(), muxed(&target, id), i128_val(XLM_AMOUNT)],
        )?;
        let applied = invoke_ok(ctx, &sender, hf)?;
        ensure_event(
            &applied,
            &[
                sym("transfer")?,
                from,
                ScVal::Address(ScAddress::Contract(target.clone())),
                asset_name("native")?,
            ],
            XLM_AMOUNT,
            id,
        )?;
        ctx.capture(
            "xlm-transfer-to-muxed-contract",
            Selector::ledger_of(&applied),
            Spec::new(
                "an XLM SAC transfer from an account to a muxed contract address with mux id 2^64-1; the transfer event names the plain contract and carries {amount, to_muxed_id} as data",
            )
            .fields(&FIELDS),
        )?;

        // mux id 0 still makes the data a map: the address type decides
        let id = 0;
        let issuer_addr = ScVal::Address(account_address(&issuer.verifying_key()));
        let hf = call(
            &sac,
            "transfer",
            vec![issuer_addr, muxed(&target, id), i128_val(CREDIT_AMOUNT)],
        )?;
        let applied = invoke_ok(ctx, &issuer, hf)?;
        ensure_event(
            &applied,
            &[
                sym("mint")?,
                ScVal::Address(ScAddress::Contract(target.clone())),
                asset_name(&issuer_name)?,
            ],
            CREDIT_AMOUNT,
            id,
        )?;
        ctx.capture(
            "issuer-transfer-to-muxed-contract",
            Selector::ledger_of(&applied),
            Spec::new(
                "the issuer transfers its own asset through the SAC to a muxed contract address with mux id 0, which the SAC reports as a mint event whose data is {amount, to_muxed_id: 0}",
            )
            .fields(&FIELDS),
        )?;

        // mint keeps a plain Address parameter, so the host refuses the
        // muxed form at argument conversion
        let plain = call(
            &sac,
            "mint",
            vec![
                ScVal::Address(ScAddress::Contract(target.clone())),
                i128_val(1),
            ],
        )?;
        let data = simulated_data(ctx, &issuer, plain)?;
        let hf = call(&sac, "mint", vec![muxed(&target, 7), i128_val(1)])?;
        let auth = vec![source_auth(&hf)?];
        let twin = apply_failing(ctx, &issuer, hf, data, auth)?;
        ensure_trapped(&twin, "mint to a muxed contract")?;
        ctx.capture(
            "mint-to-muxed-contract-refused",
            Selector::ledger_of(&twin),
            Spec::new(
                "SAC mint with a muxed contract destination: CAP-84 extends only transfer, so the host refuses the argument and core includes the invocation and fails it with invoke_host_function trapped",
            )
            .fields(&FIELDS),
        )
        .context("capturing the refused mint")
    }
}
