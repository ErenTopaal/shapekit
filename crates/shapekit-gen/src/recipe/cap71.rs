//! `cap71.credentials`: the two credential types CAP-71 added in protocol
//! 27, from transactions core applies.
//!
//! - Address-v2 credentials: a classic account authorises
//!   `auth_tree.guarded(account, true)` with the address-bound payload
//!   (`ENVELOPE_TYPE_SOROBAN_AUTHORIZATION_WITH_ADDRESS`).
//! - With-delegates credentials: a custom account
//!   (`contracts/delegating_account`) authenticates only through the
//!   delegated signers in its entry. Its delegates are a classic account and
//!   a second delegating account, whose nested delegates are two more
//!   classic accounts. Every classic signer signs the top-level payload, as
//!   CAP-71-01 specifies.
//! - The same entry with its delegates out of address order, which the host
//!   refuses before entering the contract (`SorobanAddressCredentialsWithDelegates`
//!   validation), so core includes the transaction and fails it.

use anyhow::{Result, ensure};
use ed25519_dalek::SigningKey;
use shapekit_core_driver::keys::account_address;
use shapekit_core_driver::sim::AuthMode;
use shapekit_core_driver::tx::{
    account_signature, authorization_payload, contract_id_from_address, sign_authorization_entry,
};
use stellar_xdr::{
    ContractExecutable, InvokeHostFunctionResult, ScAddress, ScVal, SorobanAddressCredentials,
    SorobanAddressCredentialsWithDelegates, SorobanAuthorizationEntry, SorobanCredentials,
    SorobanDelegateSignature,
};

use super::soroban_steps::{
    AUTH_TREE_WASM, DELEGATING_ACCOUNT_WASM, authorized, call, contract_address, create_v2,
    ensure_invoke_result, expiration, salt, simulate, soroban_data, upload, vec_val,
};
use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "cap71.credentials";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

pub struct Credentials {
    pub protocol: u32,
}

const RECORD: AuthMode = AuthMode::Record {
    allow_non_root: false,
    address_v2: false,
};

/// One classic delegate: its address and its signature over `payload`.
fn classic_delegate(key: &SigningKey, payload: &[u8; 32]) -> Result<SorobanDelegateSignature> {
    Ok(SorobanDelegateSignature {
        address: account_address(&key.verifying_key()),
        signature: account_signature(payload, &[key])?,
        nested_delegates: Default::default(),
    })
}

/// The delegate tree of the outer account: `direct` signs itself, `inner`
/// is a delegating account whose delegates `nested` sign. Each list is in
/// address order unless `shuffle` asks for the top level reversed.
fn delegates(
    payload: &[u8; 32],
    direct: &SigningKey,
    inner: &ScAddress,
    nested: &[&SigningKey],
    shuffle: bool,
) -> Result<Vec<SorobanDelegateSignature>> {
    let mut inner_delegates = nested
        .iter()
        .map(|k| classic_delegate(k, payload))
        .collect::<Result<Vec<_>>>()?;
    inner_delegates.sort_by(|a, b| a.address.cmp(&b.address));
    let mut top = vec![
        classic_delegate(direct, payload)?,
        SorobanDelegateSignature {
            address: inner.clone(),
            signature: ScVal::Void,
            nested_delegates: inner_delegates.try_into()?,
        },
    ];
    top.sort_by(|a, b| a.address.cmp(&b.address));
    if shuffle {
        top.reverse();
    }
    Ok(top)
}

/// Turns a recorded address entry of a delegating account into
/// with-delegates credentials with every signature filled.
fn with_delegates(
    entry: SorobanAuthorizationEntry,
    network_id: &[u8; 32],
    expiration_ledger: u32,
    tree: impl Fn(&[u8; 32]) -> Result<Vec<SorobanDelegateSignature>>,
) -> Result<SorobanAuthorizationEntry> {
    let SorobanCredentials::Address(c) = entry.credentials else {
        anyhow::bail!(
            "expected recorded address credentials, got {:?}",
            entry.credentials
        );
    };
    let mut entry = SorobanAuthorizationEntry {
        credentials: SorobanCredentials::AddressWithDelegates(
            SorobanAddressCredentialsWithDelegates {
                address_credentials: SorobanAddressCredentials {
                    signature_expiration_ledger: expiration_ledger,
                    // the account verifies no signature of its own
                    signature: ScVal::Void,
                    ..c
                },
                delegates: Default::default(),
            },
        ),
        root_invocation: entry.root_invocation,
    };
    // delegates sign the payload of the top-level address, which does not
    // cover the delegates themselves
    let payload = authorization_payload(&entry, network_id)?;
    if let SorobanCredentials::AddressWithDelegates(w) = &mut entry.credentials {
        w.delegates = tree(&payload)?.try_into()?;
    }
    Ok(entry)
}

impl Recipe for Credentials {
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
        ctx.fund(&["source", "alice"], BALANCE)?;
        let source = ctx.account("source");
        let alice = ctx.account("alice");
        // delegates are keys only: classic account authentication in the
        // host reads the account entry, so they are funded too
        ctx.fund(&["delegate-direct", "delegate-a", "delegate-b"], BALANCE)?;
        let direct = ctx.account("delegate-direct");
        let da = ctx.account("delegate-a");
        let db = ctx.account("delegate-b");

        let tree_code = upload(ctx, &source, AUTH_TREE_WASM)?;
        let tree = ctx
            .driver()
            .deploy(&source, &tree_code, &salt(FAMILY, "auth-tree"))?;
        let account_code = upload(ctx, &source, DELEGATING_ACCOUNT_WASM)?;
        let source_addr = account_address(&source.verifying_key());

        let deploy_account = |ctx: &mut ChainCtx, use_: &str, signers: Vec<ScVal>| {
            let s = salt(FAMILY, use_);
            let hf = create_v2(
                &source_addr,
                &s,
                ContractExecutable::Wasm(account_code.clone()),
                vec![vec_val(signers)?],
            )?;
            let env = ctx.driver().soroban(&source, hf)?;
            ctx.submit_and_close(&env)?;
            Ok::<_, anyhow::Error>(contract_id_from_address(
                &ctx.network_id(),
                &source_addr,
                &s,
            )?)
        };
        let inner = deploy_account(
            ctx,
            "inner-account",
            vec![
                ScVal::Address(account_address(&da.verifying_key())),
                ScVal::Address(account_address(&db.verifying_key())),
            ],
        )?;
        let inner_addr = contract_address(&inner);
        let outer = deploy_account(
            ctx,
            "outer-account",
            vec![
                ScVal::Address(account_address(&direct.verifying_key())),
                ScVal::Address(inner_addr.clone()),
            ],
        )?;
        let outer_addr = contract_address(&outer);

        // address-v2: same flow as v1 address credentials, recorded with
        // the address-bound payload
        let alice_addr = account_address(&alice.verifying_key());
        let hf = call(
            &tree,
            "guarded",
            vec![ScVal::Address(alice_addr.clone()), ScVal::Bool(true)],
        )?;
        let (env, auth) = authorized(
            ctx,
            &source,
            hf,
            AuthMode::Record {
                allow_non_root: false,
                address_v2: true,
            },
            |ctx, e| {
                let exp = expiration(ctx);
                Ok(sign_authorization_entry(
                    e,
                    &ctx.network_id(),
                    exp,
                    &[&alice],
                )?)
            },
        )?;
        ensure!(
            auth.len() == 1 && matches!(auth[0].credentials, SorobanCredentials::AddressV2(_)),
            "address-v2 recording gave {auth:?}"
        );
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "address-v2",
            Selector::ledger_of(&applied),
            Spec::new(
                "a classic account authorises a contract call with address-v2 credentials, whose signature covers the address-bound payload",
            )
            .fields(&[
                "op.invoke_host_function.auth",
                "auth.credentials.address.address",
                "auth.credentials.address.nonce",
                "auth.credentials.address.signature_expiration_ledger",
                "auth.credentials.address.signature",
                "auth.root_invocation",
            ]),
        )?;

        // with delegates, nested one level. the twin with its top-level
        // delegates reversed goes first, while the nonce is still unused,
        // so the order is the only fault: core takes the transaction and
        // the host refuses the entry before the call
        let hf = call(
            &tree,
            "guarded",
            vec![ScVal::Address(outer_addr.clone()), ScVal::Bool(true)],
        )?;
        let nested = [&da, &db];
        let sign_sorted = |ctx: &mut ChainCtx, e| {
            let exp = expiration(ctx);
            with_delegates(e, &ctx.network_id(), exp, |p| {
                delegates(p, &direct, &inner_addr, &nested, false)
            })
        };
        let (probe, _) = authorized(ctx, &source, hf.clone(), RECORD, sign_sorted)?;
        let data = soroban_data(&probe)?;
        let exp = expiration(ctx);
        let recorded = simulate(ctx, &source, &hf, RECORD)?;
        let entry = recorded
            .auth
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("no recorded auth for the outer account"))?;
        let entry = with_delegates(entry, &ctx.network_id(), exp, |p| {
            delegates(p, &direct, &inner_addr, &nested, true)
        })?;
        let env = ctx
            .driver()
            .soroban_with(&source, hf.clone(), data, vec![entry])?;
        let applied = ctx.submit_and_close_failing(&env)?;
        ensure_invoke_result(
            &applied,
            |r| matches!(r, InvokeHostFunctionResult::Trapped),
            "invoke_host_function trapped",
        )?;
        ctx.capture(
            "with-delegates-unsorted",
            Selector::ledger_of(&applied),
            Spec::new(
                "with-delegates credentials whose delegates are not in address order, which the host refuses before the call, so the transaction fails",
            )
            .fields(&[
                "op.invoke_host_function.auth",
                "auth.credentials.address_with_delegates.delegates",
                "auth.credentials.address.address",
                "auth.credentials.address.nonce",
                "auth.credentials.address.signature_expiration_ledger",
                "auth.credentials.address.signature",
            ])
            .flags(&["failed-tx"]),
        )?;

        let (env, auth) = authorized(ctx, &source, hf, RECORD, sign_sorted)?;
        ensure!(
            auth.len() == 1
                && matches!(
                    &auth[0].credentials,
                    SorobanCredentials::AddressWithDelegates(w)
                        if w.delegates.len() == 2
                            && w.delegates.iter().any(|d| d.nested_delegates.len() == 2)
                ),
            "with-delegates entry has the wrong shape: {auth:?}"
        );
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "with-delegates-nested",
            Selector::ledger_of(&applied),
            Spec::new(
                "a custom account authorises a contract call through with-delegates credentials: a classic delegate and a delegating account whose two nested classic delegates sign the same top-level payload",
            )
            .fields(&[
                "op.invoke_host_function.auth",
                "auth.credentials.address_with_delegates.delegates",
                "auth.credentials.address.address",
                "auth.credentials.address.nonce",
                "auth.credentials.address.signature_expiration_ledger",
                "auth.credentials.address.signature",
                "auth.root_invocation",
            ]),
        )
    }
}
