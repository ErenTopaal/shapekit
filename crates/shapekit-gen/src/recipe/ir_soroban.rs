//! Soroban transactions for the recipe IR interpreter beyond source-account invocations:
//! address credentials signed by roles, hand-shaped authorization entries, resource
//! overrides, and the extend and restore operations. The interpreter maps the IR steps onto
//! these; everything here works on stellar-xdr values with references already resolved.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use ed25519_dalek::SigningKey;
use shapekit_core_driver::keys::{account_address, account_id};
use shapekit_core_driver::sim::AuthMode;
use shapekit_core_driver::tx::{
    account_signature, authorization_payload, sign_authorization_entry,
};
use stellar_xdr::{
    AccountId, ExtendFootprintTtlOp, ExtensionPoint, HostFunction, LedgerFootprint, LedgerKey,
    OperationBody, RestoreFootprintOp, ScAddress, ScVal, SorobanAddressCredentialsWithDelegates,
    SorobanAuthorizationEntry, SorobanCredentials, SorobanDelegateSignature, SorobanResources,
    SorobanTransactionData, SorobanTransactionDataExt, TransactionEnvelope,
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
        /// Every entry for an account here gets with-delegates credentials whose delegates
        /// are these keys' accounts, each signing the entry's payload.
        delegates: BTreeMap<AccountId, Vec<SigningKey>>,
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
            delegates,
        } => {
            let r = sim.simulate_invoke(
                function.clone(),
                AuthMode::Record {
                    allow_non_root,
                    address_v2,
                },
                &source_id,
            )?;
            let entries = with_delegates(r.auth, &delegates)?;
            let signed = sign_all(ctx, entries, &signers)?;
            let signed = sign_delegates(ctx, signed, &delegates)?;
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

/// Entries recorded for an account `delegates` names, turned into with-delegates credentials
/// (CAP-71) whose delegates are the accounts of its keys in address order, signatures empty
/// until [`sign_delegates`].
fn with_delegates(
    entries: Vec<SorobanAuthorizationEntry>,
    delegates: &BTreeMap<AccountId, Vec<SigningKey>>,
) -> Result<Vec<SorobanAuthorizationEntry>> {
    let mut out = Vec::with_capacity(entries.len());
    for e in entries {
        let keys = credential_account(&e).and_then(|a| delegates.get(a));
        let Some(keys) = keys.filter(|k| !k.is_empty()) else {
            out.push(e);
            continue;
        };
        let creds = match e.credentials {
            SorobanCredentials::Address(c) | SorobanCredentials::AddressV2(c) => c,
            other => bail!("delegates need recorded address credentials, got {other:?}"),
        };
        let mut list: Vec<SorobanDelegateSignature> = keys
            .iter()
            .map(|k| SorobanDelegateSignature {
                address: account_address(&k.verifying_key()),
                signature: ScVal::Void,
                nested_delegates: Default::default(),
            })
            .collect();
        // the host refuses delegates out of address order
        list.sort_by(|a, b| a.address.cmp(&b.address));
        out.push(SorobanAuthorizationEntry {
            credentials: SorobanCredentials::AddressWithDelegates(
                SorobanAddressCredentialsWithDelegates {
                    address_credentials: creds,
                    delegates: list.try_into()?,
                },
            ),
            root_invocation: e.root_invocation,
        });
    }
    Ok(out)
}

/// Each delegate of a with-delegates entry signs the entry's payload, which covers the
/// expiration ledger the primary signature set but not the delegates themselves.
fn sign_delegates(
    ctx: &mut ChainCtx,
    entries: Vec<SorobanAuthorizationEntry>,
    delegates: &BTreeMap<AccountId, Vec<SigningKey>>,
) -> Result<Vec<SorobanAuthorizationEntry>> {
    let network = ctx.network_id();
    let mut out = Vec::with_capacity(entries.len());
    for mut e in entries {
        let keys = credential_account(&e)
            .and_then(|a| delegates.get(a))
            .cloned();
        if let (Some(keys), true) = (
            keys,
            matches!(e.credentials, SorobanCredentials::AddressWithDelegates(_)),
        ) {
            let payload = authorization_payload(&e, &network)?;
            if let SorobanCredentials::AddressWithDelegates(w) = &mut e.credentials {
                let mut list = w.delegates.to_vec();
                for d in &mut list {
                    let k = keys
                        .iter()
                        .find(|k| account_address(&k.verifying_key()) == d.address)
                        .context("a delegate without its key")?;
                    d.signature = account_signature(&payload, &[k])?;
                }
                w.delegates = list.try_into()?;
            }
        }
        out.push(e);
    }
    Ok(out)
}

/// A signed invoke-host-function transaction built without simulation: the footprint and
/// all four resources come from the recipe. Source authorization sends no entries; given
/// entries are signed as written.
pub fn invoke_unsimulated(
    ctx: &mut ChainCtx,
    source: &SigningKey,
    seq: i64,
    function: HostFunction,
    auth: Auth,
    footprint: LedgerFootprint,
    resources: &Resources,
) -> Result<TransactionEnvelope> {
    let entries = match auth {
        Auth::Source => Vec::new(),
        Auth::Entries { entries, signers } => sign_all(ctx, entries, &signers)?,
        Auth::Record { .. } => {
            bail!("auth mode record writes its entries by simulation, so it needs simulate: true")
        }
    };
    let (Some(instructions), Some(disk_read_bytes), Some(write_bytes), Some(resource_fee)) = (
        resources.instructions,
        resources.disk_read_bytes,
        resources.write_bytes,
        resources.resource_fee,
    ) else {
        bail!("simulate: false needs all four resources in the step");
    };
    let data = SorobanTransactionData {
        ext: SorobanTransactionDataExt::V0,
        resources: SorobanResources {
            footprint,
            instructions,
            disk_read_bytes,
            write_bytes,
        },
        resource_fee,
    };
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
