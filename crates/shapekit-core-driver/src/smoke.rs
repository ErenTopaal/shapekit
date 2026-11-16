//! The smoke chain: the smallest chain that exercises every part of the
//! driver at protocol 30 under real Soroban settings.
//!
//! | ledger | content                                                        |
//! |--------|----------------------------------------------------------------|
//! | 2      | protocol upgrade 0 to 30                                       |
//! | 3 to 5 | settings bootstrap: root uploads echo, deploys the writer      |
//! |        | instance, stores testnet's config upgrade set                  |
//! | 6      | config upgrade to testnet's settings                           |
//! | 7      | root funds four derived accounts                               |
//! | 8      | payment                                                        |
//! | 9      | trustline to the derived issuer's asset                        |
//! | 10, 11 | deployer uploads echo and deploys it                           |
//! | 12     | one echo invocation                                            |
//!
//! The bootstrap runs under `TESTING_SOROBAN_HIGH_LIMIT_OVERRIDE` because
//! core's initial settings cannot hold the 4.6 KB echo wasm; everything from
//! ledger 7 on runs under testnet's limits and fees. The bootstrap already
//! uploaded echo, so the upload in ledger 10 finds the code in place.

use std::path::Path;

use stellar_xdr::{
    AlphaNum4, AssetCode4, ChangeTrustAsset, ChangeTrustOp, ContractId, CreateAccountOp,
    OperationBody, PaymentOp, ScSymbol, ScVal, ScVec,
};

use crate::chain::{ChainDriver, ChainError, SettingsApplied};
use crate::config::{CoreImage, NodeConfig};
use crate::contracts::ECHO_WASM;
use crate::keys::{account_id, derive_key, muxed_account, root_key};
use crate::recorder::ChainSummary;
use crate::settings::NetworkSettings;
use sha2::{Digest, Sha256};

pub const TESTNET_PASSPHRASE: &str = "Test SDF Network ; September 2015";
pub const MAINNET_PASSPHRASE: &str = "Public Global Stellar Network ; September 2015";

/// Recipe id the smoke chain derives its keys and salts from.
pub const RECIPE: &str = "smoke";

pub const PROTOCOL: u32 = 30;

/// Roles of the four funded accounts.
pub const ROLES: [&str; 4] = ["alice", "bob", "issuer", "deployer"];

/// Starting balance of each funded account, 10,000 XLM.
pub const STARTING_BALANCE: i64 = 100_000_000_000;

/// The asset bob trusts.
pub const ASSET_CODE: [u8; 4] = *b"SMK\0";

/// The node config every smoke run uses. Diagnostic events are off: with
/// them on, core adds `core_metrics` events whose `invoke_time_nsecs` data is
/// the wall-clock time of the host call, which differs on every run.
pub fn node_config(passphrase: &str, image: CoreImage) -> NodeConfig {
    NodeConfig {
        passphrase: passphrase.to_owned(),
        image,
        protocol: PROTOCOL,
        soroban_high_limits: true,
        emit_classic_events: true,
        backfill_stellar_asset_events: true,
        diagnostic_events: false,
        min_persistent_ttl: None,
        invariant_checks: false,
    }
}

/// Salt of the smoke chain's echo instance.
pub fn echo_salt() -> [u8; 32] {
    Sha256::digest(b"shapekit/smoke/echo").into()
}

/// The value the smoke chain echoes.
pub fn echo_value() -> ScVal {
    ScVal::Vec(Some(ScVec(
        vec![
            ScVal::Symbol(ScSymbol("smoke".try_into().expect("short symbol"))),
            ScVal::U32(1),
        ]
        .try_into()
        .expect("two items"),
    )))
}

#[derive(Debug)]
pub struct SmokeChain {
    pub summary: ChainSummary,
    pub settings: SettingsApplied,
    pub echo: ContractId,
}

/// Records the smoke chain with `cfg` in `workdir` (empty) and writes it to
/// `target` (must not exist).
pub fn record(cfg: NodeConfig, workdir: &Path, target: &Path) -> Result<SmokeChain, ChainError> {
    let root = root_key(&cfg.passphrase);
    let mut chain = ChainDriver::start(cfg, workdir)?;
    let settings = chain.apply_settings(&NetworkSettings::testnet()?, &root, ECHO_WASM)?;

    let keys: Vec<_> = ROLES.iter().map(|r| derive_key(RECIPE, r)).collect();
    let [alice, bob, issuer, deployer] = [&keys[0], &keys[1], &keys[2], &keys[3]];
    let fund = keys
        .iter()
        .map(|k| {
            OperationBody::CreateAccount(CreateAccountOp {
                destination: account_id(&k.verifying_key()),
                starting_balance: STARTING_BALANCE,
            })
        })
        .collect();
    let env = chain.classic(&root, fund)?;
    chain.apply_ok(&env)?;

    let env = chain.classic(
        alice,
        vec![OperationBody::Payment(PaymentOp {
            destination: muxed_account(&bob.verifying_key()),
            asset: stellar_xdr::Asset::Native,
            amount: 1_000_000_000,
        })],
    )?;
    chain.apply_ok(&env)?;

    let env = chain.classic(
        bob,
        vec![OperationBody::ChangeTrust(ChangeTrustOp {
            line: ChangeTrustAsset::CreditAlphanum4(AlphaNum4 {
                asset_code: AssetCode4(ASSET_CODE),
                issuer: account_id(&issuer.verifying_key()),
            }),
            limit: i64::MAX,
        })],
    )?;
    chain.apply_ok(&env)?;

    let code = chain.upload(deployer, ECHO_WASM)?;
    let echo = chain.deploy(deployer, &code, &echo_salt())?;
    let applied = chain.invoke(alice, &echo, "echo", vec![echo_value()])?;
    if applied.return_value() != Some(&echo_value()) {
        return Err(ChainError::Unexpected {
            ledger: applied.ledger,
            what: format!("echo returned {:?}", applied.return_value()),
        });
    }

    let summary = chain.finish(target)?;
    Ok(SmokeChain {
        summary,
        settings,
        echo,
    })
}
