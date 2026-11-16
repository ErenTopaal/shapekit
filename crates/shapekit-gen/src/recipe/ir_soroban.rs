//! Soroban transactions for the recipe IR interpreter beyond source-account invocations:
//! address credentials signed by roles, hand-shaped authorization entries, resource
//! overrides, and the extend and restore operations. The interpreter maps the IR steps onto
//! these; everything here works on stellar-xdr values with references already resolved.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use ed25519_dalek::SigningKey;
use shapekit_core_driver::keys::account_id;
use shapekit_core_driver::sim::AuthMode;
use shapekit_core_driver::tx::sign_authorization_entry;
use stellar_xdr::{
    AccountId, ExtendFootprintTtlOp, ExtensionPoint, HostFunction, LedgerKey, OperationBody,
    RestoreFootprintOp, ScAddress, SorobanAuthorizationEntry, SorobanCredentials,
    SorobanTransactionData, TransactionEnvelope,
};

use super::ChainCtx;

/// Ledgers past the last closed one an address signature stays valid for.
pub const SIGNATURE_LEDGERS: u32 = 100;

/// How an invocation is authorized, with the keys already chosen.
pub enum Auth {
    /// Source-account credentials only, recorded by simulation.
    Source,
    /// Simulation records the entries; every address-credential entry for an account in
    /// `signers` is signed by its keys.
    Record {
        allow_non_root: bool,
        address_v2: bool,
        signers: BTreeMap<AccountId, Vec<SigningKey>>,
    },
    /// These entries, each address-credential entry for an account in `signers` signed by its
    /// keys, then simulated in enforcing mode.
    Entries {
        entries: Vec<SorobanAuthorizationEntry>,
        signers: BTreeMap<AccountId, Vec<SigningKey>>,
    },
}

/// Simulated resources replaced by the recipe.
#[derive(Debug, Clone, Default)]
pub struct Resources {
    pub instructions: Option<u32>,
    pub disk_read_bytes: Option<u32>,
    pub write_bytes: Option<u32>,
    pub resource_fee: Option<i64>,
}

impl Resources {
    pub fn apply(&self, data: &mut SorobanTransactionData) {
        if let Some(v) = self.instructions {
            data.resources.instructions = v;
        }
        if let Some(v) = self.disk_read_bytes {
            data.resources.disk_read_bytes = v;
        }
        if let Some(v) = self.write_bytes {
            data.resources.write_bytes = v;
        }
        if let Some(v) = self.resource_fee {
            data.resource_fee = v;
        }
    }
}

fn credential_account(entry: &SorobanAuthorizationEntry) -> Option<&AccountId> {
    let creds = match &entry.credentials {
        SorobanCredentials::SourceAccount => return None,
        SorobanCredentials::Address(c) | SorobanCredentials::AddressV2(c) => c,
        SorobanCredentials::AddressWithDelegates(c) => &c.address_credentials,
    };
    match &creds.address {
        ScAddress::Account(a) => Some(a),
        _ => None,
    }
}

fn sign_all(
    ctx: &mut ChainCtx,
    entries: Vec<SorobanAuthorizationEntry>,
    signers: &BTreeMap<AccountId, Vec<SigningKey>>,
) -> Result<Vec<SorobanAuthorizationEntry>> {
    let expiration = ctx.driver().last_closed() + SIGNATURE_LEDGERS;
    let network = ctx.network_id();
    let mut out = Vec::with_capacity(entries.len());
    for e in entries {
        let keys = credential_account(&e).and_then(|a| signers.get(a));
        match keys {
            Some(keys) => {
                let refs: Vec<&SigningKey> = keys.iter().collect();
                out.push(sign_authorization_entry(e, &network, expiration, &refs)?);
            }
            None => {
                if !matches!(e.credentials, SorobanCredentials::SourceAccount) && signers.is_empty()
                {
                    bail!(
                        "the invocation needs address authorization, and the step names no signer"
                    );
                }
                out.push(e);
            }
        }
    }
    Ok(out)
}

/// A signed invoke-host-function transaction from `source` with sequence number `seq`.
pub fn invoke(
    ctx: &mut ChainCtx,
    source: &SigningKey,
    seq: i64,
    function: HostFunction,
    auth: Auth,
    resources: &Resources,
) -> Result<TransactionEnvelope> {
    let source_id = account_id(&source.verifying_key());
    let sim = ctx.driver().simulator()?;
    let (entries, mut data) = match auth {
        Auth::Source => {
            let r = sim.simulate_invoke(
                function.clone(),
                AuthMode::Record {
                    allow_non_root: false,
                    address_v2: false,
                },
                &source_id,
            )?;
            if r.auth
                .iter()
                .any(|a| !matches!(a.credentials, SorobanCredentials::SourceAccount))
            {
                bail!("the invocation needs address authorization; give the step an auth mode");
            }
            (r.auth, r.transaction_data)
        }
        Auth::Record {
            allow_non_root,
            address_v2,
            signers,
        } => {
            let r = sim.simulate_invoke(
                function.clone(),
                AuthMode::Record {
                    allow_non_root,
                    address_v2,
                },
                &source_id,
            )?;
            let signed = sign_all(ctx, r.auth, &signers)?;
            // signature checks cost instructions the recording run did not spend
            let r = sim
                .simulate_invoke(
                    function.clone(),
                    AuthMode::Enforce(signed.clone()),
                    &source_id,
                )
                .context("simulating with the signed entries")?;
            (signed, r.transaction_data)
        }
        Auth::Entries { entries, signers } => {
            let signed = sign_all(ctx, entries, &signers)?;
            let r = sim
                .simulate_invoke(
                    function.clone(),
                    AuthMode::Enforce(signed.clone()),
                    &source_id,
                )
                .context("simulating with the given entries")?;
            (signed, r.transaction_data)
        }
    };
    resources.apply(&mut data);
    Ok(ctx
        .driver()
        .soroban_with_at(source, function, data, entries, seq)?)
}

/// What a footprint operation does.
pub enum Footprint {
    Extend { extend_to: u32 },
    Restore,
}

/// A signed extend-footprint-TTL or restore-footprint transaction over `keys`.
pub fn footprint(
    ctx: &mut ChainCtx,
    source: &SigningKey,
    seq: i64,
    keys: &[LedgerKey],
    op: Footprint,
    resources: &Resources,
) -> Result<TransactionEnvelope> {
    let sim = ctx.driver().simulator()?;
    let (mut data, body) = match op {
        Footprint::Extend { extend_to } => (
            sim.simulate_extend_ttl(keys, extend_to)?.transaction_data,
            OperationBody::ExtendFootprintTtl(ExtendFootprintTtlOp {
                ext: ExtensionPoint::V0,
                extend_to,
            }),
        ),
        Footprint::Restore => (
            sim.simulate_restore(keys)?,
            OperationBody::RestoreFootprint(RestoreFootprintOp {
                ext: ExtensionPoint::V0,
            }),
        ),
    };
    resources.apply(&mut data);
    Ok(ctx.driver().soroban_op_at(source, body, data, seq)?)
}
