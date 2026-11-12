//! `soroban.auth-trees`: Soroban authorization entries as transactions
//! carry them, at protocol 30 under testnet's settings, using the
//! `auth_tree` contract (`node(chain, levels)` requires one signer per level
//! and calls the next instance `fanout` times; `guarded(signer, enforce)`
//! requires auth only when told to).
//!
//! Captures are whole ledgers: what defines the shape is in the envelope,
//! and the ledger's meta beside it says what actually happened.
//!
//! - source-account credentials, and address credentials signed by a
//!   classic account;
//! - the deepest invocation tree one `require_auth` chain can match (one
//!   instance per level, since the host forbids re-entry), and the tree one
//!   level deeper, which fails at apply;
//! - the widest fan-out of one level that applies, and one wider, which
//!   simulation accepts but the host refuses at apply;
//! - declared trees far deeper than the call, whose nodes below the root
//!   are never matched and never checked: the deepest the host decodes, and
//!   one deeper, which core includes and the host traps on;
//! - a failed transaction whose address entry the host matched and checked
//!   before the call trapped;
//! - the three ways wallet-backend#746 reads contract creations that never
//!   happened out of declared auth: a failed transaction declaring one, a
//!   successful call that consumes no entry while three creations are
//!   declared, and a declared deployer that is a claimable balance with a
//!   creation nested below it. Each asserts the meta creates no contract.
//!
//! Limits found on this chain (testnet settings, protocol 30). Each one is
//! pinned by an entry at the limit and its twin past it:
//!
//! | shape                          | accepted | first refused | where                                |
//! |--------------------------------|----------|---------------|--------------------------------------|
//! | matched depth (invocations)    | 31       | 32            | host, at apply                       |
//! | fan-out of one level           | 323      | 324           | host, at apply (simulation: 325)     |
//! | declared levels below the root | 245      | 246           | host xdr depth 500, at apply         |
//!
//! Core itself accepts declared chains up to 492 levels below the root
//! (envelope depth 1000, `validateXDRForProtocol`), includes them and fails
//! them in the host, and refuses 493 as txMALFORMED at submission (measured
//! on this chain). Those two are not recorded: a ledger holding a
//! 492-level envelope needs decode depth 1012, and the SEP-54 packer and the
//! oracle decode meta at 1000.
//!
//! Simulation (soroban-simulation 30.0.0) gives up before the network on
//! declared trees: from 243 levels below the root it cannot size the
//! transaction ("depth limit exceeded"). Those entries carry the resources
//! simulated for the same tree 242 levels deep.
//!
//! Recording simulation draws nonces from a fixed seed, so it hands the
//! same nonce to every recording for one address; each address-credential
//! entry therefore has its own signer account.

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use shapekit_core_driver::chain::Applied;
use shapekit_core_driver::contracts::ECHO_WASM;
use shapekit_core_driver::keys::{account_address, account_id};
use shapekit_core_driver::settings::NetworkSettings;
use shapekit_core_driver::sim::AuthMode;
use shapekit_core_driver::tx::{contract_preimage, sign_authorization_entry};
use stellar_xdr::{
    Asset, ClaimPredicate, Claimant, ClaimantV0, ConfigSettingEntry, ConfigSettingId,
    ContractDataDurability, ContractExecutable, ContractId, CreateClaimableBalanceOp,
    CreateClaimableBalanceResult, CreateContractArgs, CreateContractArgsV2, Hash,
    InvokeContractArgs, InvokeHostFunctionResult, LedgerKey, LedgerKeyAccount,
    LedgerKeyContractData, OperationBody, OperationResult, OperationResultTr, ScAddress, ScMap,
    ScMapEntry, ScNonceKey, ScSymbol, ScVal, SorobanAddressCredentials, SorobanAuthorizationEntry,
    SorobanAuthorizedFunction, SorobanAuthorizedInvocation, SorobanCredentials,
    SorobanTransactionData, TransactionResultResult,
};

use super::soroban_steps::{
    AUTH_TREE_WASM, authorized_prefetched, call, code_key, contract_address, created_instances,
    ensure_invoke_result, expiration, instance_key, salt, simulator, soroban_data, sym, upload,
    vec_val,
};
use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "soroban.auth-trees";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

/// Deepest `require_auth` chain one transaction can match, in invocations.
pub const MATCHED_DEPTH: u32 = 31;

/// Most calls one level can fan out to and still apply. Simulation
/// accepts up to 325; core's host traps from 324 even when the transaction
/// declares every instruction it may.
pub const FANOUT: u32 = 323;

/// Deepest declared chain below a matched root that the host decodes
/// (`DEFAULT_XDR_RW_LIMITS` depth 500 in soroban-env-host 30.0.0).
pub const HOST_DECLARED_DEPTH: u32 = 245;

/// Deepest declared chain simulation can size.
const SIMULATED_DECLARED_DEPTH: u32 = 242;

pub struct AuthTrees {
    pub protocol: u32,
}

const RECORD: AuthMode = AuthMode::Record {
    allow_non_root: false,
    address_v2: false,
};

const SOURCE_FIELDS: [&str; 3] = [
    "op.invoke_host_function.auth",
    "auth.credentials.source_account",
    "auth.root_invocation",
];

const ADDRESS_FIELDS: [&str; 6] = [
    "op.invoke_host_function.auth",
    "auth.credentials.address.address",
    "auth.credentials.address.nonce",
    "auth.credentials.address.signature_expiration_ledger",
    "auth.credentials.address.signature",
    "auth.root_invocation",
];

const MIXED_FIELDS: [&str; 7] = [
    "op.invoke_host_function.auth",
    "auth.credentials.source_account",
    "auth.credentials.address.address",
    "auth.credentials.address.nonce",
    "auth.credentials.address.signature_expiration_ledger",
    "auth.credentials.address.signature",
    "auth.root_invocation",
];

/// Every role that signs one address-credential entry.
const SIGNERS: [&str; 9] = [
    "address-signer",
    "deep-signer",
    "deep-over-signer",
    "wide-signer",
    "wide-over-signer",
    "declared-sim-signer",
    "declared-host-signer",
    "failed-signer",
    "unmatched-signer",
];

/// `Level { signer, fanout }` as the contract type encodes it: a map with
/// symbol keys in order.
fn level(signer: &ScAddress, fanout: u32) -> Result<ScVal> {
    Ok(ScVal::Map(Some(ScMap(
        vec![
            ScMapEntry {
                key: sym("fanout")?,
                val: ScVal::U32(fanout),
            },
            ScMapEntry {
                key: sym("signer")?,
                val: ScVal::Address(signer.clone()),
            },
        ]
        .try_into()?,
    ))))
}

/// The arguments of `node(chain, levels)`.
fn node_args(chain: &[ContractId], levels: &[(ScAddress, u32)]) -> Result<Vec<ScVal>> {
    let c = chain
        .iter()
        .map(|c| ScVal::Address(contract_address(c)))
        .collect();
    let l = levels
        .iter()
        .map(|(s, f)| level(s, *f))
        .collect::<Result<Vec<_>>>()?;
    Ok(vec![vec_val(c)?, vec_val(l)?])
}

/// `depth` levels all required by `signer`, each calling the next once.
fn chain_levels(signer: &ScAddress, depth: u32) -> Vec<(ScAddress, u32)> {
    (0..depth)
        .map(|i| (signer.clone(), u32::from(i + 1 < depth)))
        .collect()
}

fn contract_fn(
    c: &ContractId,
    function: &str,
    args: Vec<ScVal>,
) -> Result<SorobanAuthorizedFunction> {
    Ok(SorobanAuthorizedFunction::ContractFn(InvokeContractArgs {
        contract_address: contract_address(c),
        function_name: ScSymbol(function.try_into()?),
        args: args.try_into()?,
    }))
}

fn invocation(
    function: SorobanAuthorizedFunction,
    children: Vec<SorobanAuthorizedInvocation>,
) -> Result<SorobanAuthorizedInvocation> {
    Ok(SorobanAuthorizedInvocation {
        function,
        sub_invocations: children.try_into()?,
    })
}

/// The tree the host matches for `trees[0].node` over a chain of `depth`
/// instances all required by `signer`: each level is the call with its own
/// remaining arguments, its only child the next level.
fn matched_tree(
    trees: &[ContractId],
    signer: &ScAddress,
    depth: u32,
) -> Result<SorobanAuthorizedInvocation> {
    let levels = chain_levels(signer, depth);
    let d = depth as usize;
    let mut cur: Vec<SorobanAuthorizedInvocation> = Vec::new();
    for i in (0..d).rev() {
        let args = node_args(&trees[i + 1..d], &levels[i..])?;
        cur = vec![invocation(contract_fn(&trees[i], "node", args)?, cur)?];
    }
    cur.pop().context("a matched tree needs at least one level")
}

/// The tree for `root.node([next], [{signer, fanout}, {signer, 0}])`: the
/// root with `fanout` identical children.
fn fanout_tree(
    root: &ContractId,
    next: &ContractId,
    signer: &ScAddress,
    fanout: u32,
) -> Result<SorobanAuthorizedInvocation> {
    let child = invocation(
        contract_fn(next, "node", node_args(&[], &[(signer.clone(), 0)])?)?,
        Vec::new(),
    )?;
    invocation(
        contract_fn(
            root,
            "node",
            node_args(
                std::slice::from_ref(next),
                &[(signer.clone(), fanout), (signer.clone(), 0)],
            )?,
        )?,
        vec![child; fanout as usize],
    )
}

/// A single chain of `depth` invocations of `c.node()` with no arguments,
/// the smallest declared node there is.
fn declared_chain(c: &ContractId, depth: u32) -> Result<Vec<SorobanAuthorizedInvocation>> {
    let mut cur = Vec::new();
    for _ in 0..depth {
        cur = vec![invocation(contract_fn(c, "node", Vec::new())?, cur)?];
    }
    Ok(cur)
}

fn create_v1_fn(deployer: &ScAddress, salt: &[u8; 32], code: &Hash) -> SorobanAuthorizedFunction {
    SorobanAuthorizedFunction::CreateContractHostFn(CreateContractArgs {
        contract_id_preimage: contract_preimage(deployer, salt),
        executable: ContractExecutable::Wasm(code.clone()),
    })
}

fn create_v2_fn(deployer: &ScAddress, salt: &[u8; 32], code: &Hash) -> SorobanAuthorizedFunction {
    SorobanAuthorizedFunction::CreateContractV2HostFn(CreateContractArgsV2 {
        contract_id_preimage: contract_preimage(deployer, salt),
        executable: ContractExecutable::Wasm(code.clone()),
        constructor_args: Default::default(),
    })
}

fn depth_of(inv: &SorobanAuthorizedInvocation) -> u32 {
    1 + inv.sub_invocations.iter().map(depth_of).max().unwrap_or(0)
}

/// Create-contract invocations an auth tree declares, at any depth.
fn declared_creations(inv: &SorobanAuthorizedInvocation) -> usize {
    let here = usize::from(matches!(
        inv.function,
        SorobanAuthorizedFunction::CreateContractHostFn(_)
            | SorobanAuthorizedFunction::CreateContractV2HostFn(_)
    ));
    here + inv
        .sub_invocations
        .iter()
        .map(declared_creations)
        .sum::<usize>()
}

/// Fails unless the auth declares at least `want` contract creations and
/// the meta creates no contract instance.
fn ensure_declared_not_created(
    applied: &Applied,
    auth: &[SorobanAuthorizationEntry],
    want: usize,
) -> Result<()> {
    let declared: usize = auth
        .iter()
        .map(|e| declared_creations(&e.root_invocation))
        .sum();
    ensure!(
        declared >= want,
        "the envelope declares {declared} contract creations, expected at least {want}"
    );
    let created = created_instances(&applied.meta);
    ensure!(
        created.is_empty(),
        "ledger {}: the meta creates {} contract instances while the creations were only declared",
        applied.ledger,
        created.len()
    );
    Ok(())
}

/// A deterministic nonce for an entry no simulation records.
fn nonce(use_: &str) -> i64 {
    let h = salt(FAMILY, use_);
    let mut b = [0u8; 8];
    b.copy_from_slice(&h[..8]);
    i64::from_be_bytes(b) & i64::MAX
}

/// An unsigned address-credential entry for `signer`.
fn address_entry(
    signer: &ScAddress,
    nonce: i64,
    root: SorobanAuthorizedInvocation,
) -> SorobanAuthorizationEntry {
    SorobanAuthorizationEntry {
        credentials: SorobanCredentials::Address(SorobanAddressCredentials {
            address: signer.clone(),
            nonce,
            signature_expiration_ledger: 0,
            signature: ScVal::Void,
        }),
        root_invocation: root,
    }
}

fn sign_with(
    ctx: &mut ChainCtx,
    e: SorobanAuthorizationEntry,
    key: &SigningKey,
) -> Result<SorobanAuthorizationEntry> {
    if matches!(e.credentials, SorobanCredentials::SourceAccount) {
        return Ok(e);
    }
    let exp = expiration(ctx);
    Ok(sign_authorization_entry(e, &ctx.network_id(), exp, &[key])?)
}

fn account_key(key: &SigningKey) -> LedgerKey {
    LedgerKey::Account(LedgerKeyAccount {
        account_id: account_id(&key.verifying_key()),
    })
}

fn nonce_key(signer: &ScAddress, nonce: i64) -> LedgerKey {
    LedgerKey::ContractData(LedgerKeyContractData {
        contract: signer.clone(),
        key: ScVal::LedgerKeyNonce(ScNonceKey { nonce }),
        durability: ContractDataDurability::Temporary,
    })
}

/// Moves simulated resources from one signer and nonce to another: the
/// account entry the signature check reads and the nonce entry it writes.
fn retarget(
    data: &mut SorobanTransactionData,
    from: (&SigningKey, i64),
    to: (&SigningKey, i64),
) -> Result<()> {
    let from_addr = account_address(&from.0.verifying_key());
    let to_addr = account_address(&to.0.verifying_key());
    let swap = |k: &LedgerKey| -> LedgerKey {
        if *k == account_key(from.0) {
            account_key(to.0)
        } else if *k == nonce_key(&from_addr, from.1) {
            nonce_key(&to_addr, to.1)
        } else {
            k.clone()
        }
    };
    let fp = &mut data.resources.footprint;
    let ro: Vec<LedgerKey> = fp.read_only.iter().map(swap).collect();
    let rw: Vec<LedgerKey> = fp.read_write.iter().map(swap).collect();
    ensure!(
        rw.contains(&nonce_key(&to_addr, to.1)) && ro.contains(&account_key(to.0)),
        "the simulated footprint holds no signer account and nonce to move"
    );
    fp.read_only = ro.try_into()?;
    fp.read_write = rw.try_into()?;
    Ok(())
}

fn add_read_only(data: &mut SorobanTransactionData, keys: &[LedgerKey]) -> Result<()> {
    let mut ro = data.resources.footprint.read_only.to_vec();
    ro.extend(keys.iter().cloned());
    data.resources.footprint.read_only = ro.try_into()?;
    Ok(())
}

/// The address entry's nonce.
fn entry_nonce(e: &SorobanAuthorizationEntry) -> Result<i64> {
    match &e.credentials {
        SorobanCredentials::Address(c) => Ok(c.nonce),
        other => bail!("expected address credentials, got {other:?}"),
    }
}

/// Testnet's per-transaction instruction limit, the most a transaction can
/// declare.
fn tx_max_instructions() -> Result<u32> {
    match NetworkSettings::testnet()?.get(ConfigSettingId::ContractComputeV0) {
        Some(ConfigSettingEntry::ContractComputeV0(c)) => Ok(u32::try_from(c.tx_max_instructions)?),
        other => bail!("testnet settings have no compute setting: {other:?}"),
    }
}

fn ensure_failed_over_limit(applied: &Applied, what: &str) -> Result<()> {
    ensure_invoke_result(
        applied,
        |r| {
            matches!(
                r,
                InvokeHostFunctionResult::Trapped | InvokeHostFunctionResult::ResourceLimitExceeded
            )
        },
        what,
    )
}

impl Recipe for AuthTrees {
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
        ctx.fund(&["source"], BALANCE)?;
        ctx.fund(&SIGNERS, BALANCE)?;
        let source = ctx.account("source");
        let source_addr = account_address(&source.verifying_key());
        let echo_hash = Hash(Sha256::digest(ECHO_WASM).into());

        let code = upload(ctx, &source, AUTH_TREE_WASM)?;
        // one instance per level of the deepest chain and its twin
        let mut trees = Vec::new();
        for i in 0..=MATCHED_DEPTH {
            let s = salt(FAMILY, &format!("tree-{i}"));
            trees.push(ctx.driver().deploy(&source, &code, &s)?);
        }
        let echo = ctx
            .driver()
            .deploy(&source, &echo_hash, &salt(FAMILY, "echo"))?;
        let mut prefetch: Vec<LedgerKey> = trees.iter().map(instance_key).collect();
        prefetch.extend([
            code_key(&code),
            account_key(&source),
            instance_key(&echo),
            code_key(&echo_hash),
        ]);
        for role in SIGNERS {
            prefetch.push(account_key(&ctx.account(role)));
        }
        let t0 = trees[0].clone();

        // source-account credentials: the source itself is the signer
        let hf = call(&t0, "node", node_args(&[], &[(source_addr.clone(), 0)])?)?;
        let (env, auth) = authorized_prefetched(ctx, &source, hf, RECORD, &prefetch, |_, e| Ok(e))?;
        ensure!(
            matches!(auth.as_slice(), [e] if matches!(e.credentials, SorobanCredentials::SourceAccount)),
            "expected one source-account entry, recorded {auth:?}"
        );
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "source-account-credentials",
            Selector::ledger_of(&applied),
            Spec::new(
                "a contract call that requires the transaction source, authorised by a source-account entry that carries no signature",
            )
            .fields(&SOURCE_FIELDS),
        )?;

        let k = ctx.account("address-signer");
        let addr = account_address(&k.verifying_key());
        let hf = call(&t0, "node", node_args(&[], &[(addr, 0)])?)?;
        let (env, auth) = authorized_prefetched(ctx, &source, hf, RECORD, &prefetch, |ctx, e| {
            sign_with(ctx, e, &k)
        })?;
        ensure!(
            matches!(auth.as_slice(), [e] if matches!(e.credentials, SorobanCredentials::Address(_))),
            "expected one address entry, recorded {auth:?}"
        );
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "address-credentials",
            Selector::ledger_of(&applied),
            Spec::new(
                "a contract call that requires another classic account, authorised by its signed address-credential entry",
            )
            .fields(&ADDRESS_FIELDS),
        )?;

        matched_depth(ctx, &source, &trees, &prefetch)?;
        fanout(ctx, &source, &trees, &prefetch)?;
        declared_depth(ctx, &source, &t0, &trees[1], &prefetch)?;
        failed_with_auth(ctx, &source, &trees, &echo, &echo_hash, &prefetch)?;
        unmatched_creations(ctx, &source, &t0, &code, &echo, &echo_hash, &prefetch)
    }
}

/// The deepest matched chain, then one level deeper.
fn matched_depth(
    ctx: &mut ChainCtx,
    source: &SigningKey,
    trees: &[ContractId],
    prefetch: &[LedgerKey],
) -> Result<()> {
    let k = ctx.account("deep-signer");
    let addr = account_address(&k.verifying_key());
    let d = MATCHED_DEPTH as usize;
    let hf = call(
        &trees[0],
        "node",
        node_args(&trees[1..d], &chain_levels(&addr, MATCHED_DEPTH))?,
    )?;
    let (env, auth) = authorized_prefetched(ctx, source, hf, RECORD, prefetch, |ctx, e| {
        sign_with(ctx, e, &k)
    })?;
    ensure!(
        auth.len() == 1 && auth[0].root_invocation == matched_tree(trees, &addr, MATCHED_DEPTH)?,
        "the deep chain recorded {} entries, depth {:?}",
        auth.len(),
        auth.first().map(|e| depth_of(&e.root_invocation))
    );
    let deep_nonce = entry_nonce(&auth[0])?;
    let mut over_data = soroban_data(&env)?;
    let applied = ctx.submit_and_close(&env)?;
    ensure!(
        applied.return_value() == Some(&ScVal::U32(MATCHED_DEPTH)),
        "the deep chain returned {:?}",
        applied.return_value()
    );
    ctx.capture(
        &format!("matched-depth-{MATCHED_DEPTH}"),
        Selector::ledger_of(&applied),
        Spec::new(format!(
            "one address-credential entry whose invocation tree is {MATCHED_DEPTH} levels deep, every level matched by a require_auth of the same account across {MATCHED_DEPTH} contract instances"
        ))
        .fields(&ADDRESS_FIELDS),
    )?;

    // one level deeper: simulation fails, so the tree is built the way the
    // host records it, and the resources are the deepest chain's moved to
    // another signer, plus the extra instance and every instruction a
    // transaction may declare
    let over = MATCHED_DEPTH + 1;
    let k2 = ctx.account("deep-over-signer");
    let addr2 = account_address(&k2.verifying_key());
    let n2 = nonce("deep-over");
    let entry = address_entry(&addr2, n2, matched_tree(trees, &addr2, over)?);
    let entry = sign_with(ctx, entry, &k2)?;
    retarget(&mut over_data, (&k, deep_nonce), (&k2, n2))?;
    add_read_only(&mut over_data, &[instance_key(&trees[over as usize - 1])])?;
    over_data.resources.instructions = tx_max_instructions()?;
    // generous: the excess resource fee is refunded
    over_data.resource_fee = over_data.resource_fee * 4 + 10_000_000;
    let hf = call(
        &trees[0],
        "node",
        node_args(&trees[1..over as usize], &chain_levels(&addr2, over))?,
    )?;
    let env = ctx
        .driver()
        .soroban_with(source, hf, over_data, vec![entry])?;
    let applied = ctx.submit_and_close_failing(&env)?;
    ensure_failed_over_limit(
        &applied,
        "a failed invocation one level past the deepest chain",
    )?;
    ctx.capture(
        &format!("matched-depth-{over}-fails"),
        Selector::ledger_of(&applied),
        Spec::new(format!(
            "the same chain {over} invocations deep, declaring the maximum instructions: the host refuses it, core includes the transaction and fails it"
        ))
        .fields(&ADDRESS_FIELDS)
        .flags(&["failed-tx"]),
    )
}

/// The widest fan-out of one level, then one wider.
fn fanout(
    ctx: &mut ChainCtx,
    source: &SigningKey,
    trees: &[ContractId],
    prefetch: &[LedgerKey],
) -> Result<()> {
    let k = ctx.account("wide-signer");
    let addr = account_address(&k.verifying_key());
    let levels = [(addr.clone(), FANOUT), (addr.clone(), 0)];
    let hf = call(&trees[0], "node", node_args(&trees[1..2], &levels)?)?;
    let (env, auth) = authorized_prefetched(ctx, source, hf, RECORD, prefetch, |ctx, e| {
        sign_with(ctx, e, &k)
    })?;
    ensure!(
        auth.len() == 1
            && auth[0].root_invocation == fanout_tree(&trees[0], &trees[1], &addr, FANOUT)?,
        "the wide call recorded an unexpected tree"
    );
    let wide_nonce = entry_nonce(&auth[0])?;
    let mut over_data = soroban_data(&env)?;
    let applied = ctx.submit_and_close(&env)?;
    ensure!(
        applied.return_value() == Some(&ScVal::U32(FANOUT + 1)),
        "the wide call returned {:?}",
        applied.return_value()
    );
    ctx.capture(
        &format!("fanout-{FANOUT}"),
        Selector::ledger_of(&applied),
        Spec::new(format!(
            "one address-credential entry whose root invocation has {FANOUT} matched sub-invocations, the widest one level can carry and still apply"
        ))
        .fields(&ADDRESS_FIELDS)
        .flags(&["max-length-vec"]),
    )?;

    let over = FANOUT + 1;
    let k2 = ctx.account("wide-over-signer");
    let addr2 = account_address(&k2.verifying_key());
    let n2 = nonce("wide-over");
    let entry = address_entry(&addr2, n2, fanout_tree(&trees[0], &trees[1], &addr2, over)?);
    let entry = sign_with(ctx, entry, &k2)?;
    retarget(&mut over_data, (&k, wide_nonce), (&k2, n2))?;
    over_data.resources.instructions = tx_max_instructions()?;
    over_data.resource_fee = over_data.resource_fee * 2 + 10_000_000;
    let levels = [(addr2.clone(), over), (addr2, 0)];
    let hf = call(&trees[0], "node", node_args(&trees[1..2], &levels)?)?;
    let env = ctx
        .driver()
        .soroban_with(source, hf, over_data, vec![entry])?;
    let applied = ctx.submit_and_close_failing(&env)?;
    ensure_failed_over_limit(&applied, "a failed invocation one call wider")?;
    ctx.capture(
        &format!("fanout-{over}-fails"),
        Selector::ledger_of(&applied),
        Spec::new(format!(
            "the same call fanned out {over} times, declaring the maximum instructions, which simulation still accepts: the host traps at apply and core fails the transaction"
        ))
        .fields(&ADDRESS_FIELDS)
        .flags(&["failed-tx"]),
    )
}

/// Declared chains below a matched root: the host limit and its twin.
fn declared_depth(
    ctx: &mut ChainCtx,
    source: &SigningKey,
    root: &ContractId,
    child: &ContractId,
    prefetch: &[LedgerKey],
) -> Result<()> {
    let declared_root = |addr: &ScAddress, depth: u32| -> Result<SorobanAuthorizedInvocation> {
        invocation(
            contract_fn(root, "node", node_args(&[], &[(addr.clone(), 0)])?)?,
            declared_chain(child, depth)?,
        )
    };
    // resources for every declared entry come from the deepest simulated
    // one; only the signer and nonce change
    let sim_key = ctx.account("declared-sim-signer");
    let sim_addr = account_address(&sim_key.verifying_key());
    let sim_nonce = nonce("declared-sim");
    let probe = address_entry(
        &sim_addr,
        sim_nonce,
        declared_root(&sim_addr, SIMULATED_DECLARED_DEPTH)?,
    );
    let probe = sign_with(ctx, probe, &sim_key)?;
    let hf = call(root, "node", node_args(&[], &[(sim_addr, 0)])?)?;
    let sim = simulator(ctx)?;
    sim.prefetch(prefetch)?;
    let base = sim
        .simulate_invoke(
            hf,
            AuthMode::Enforce(vec![probe]),
            &account_id(&source.verifying_key()),
        )
        .context("simulating the deepest declared chain simulation can size")?
        .transaction_data;

    let cases = [
        ("declared-host-signer", HOST_DECLARED_DEPTH, "host-limit"),
        ("declared-host-signer", HOST_DECLARED_DEPTH + 1, "host-over"),
    ];
    for (role, depth, case) in cases {
        let k = ctx.account(role);
        let addr = account_address(&k.verifying_key());
        let n = nonce(&format!("declared-{case}"));
        let entry = address_entry(&addr, n, declared_root(&addr, depth)?);
        let entry = sign_with(ctx, entry, &k)?;
        let mut data = base.clone();
        retarget(&mut data, (&sim_key, sim_nonce), (&k, n))?;
        // the deeper envelopes are larger than the simulated one
        data.resource_fee *= 3;
        let hf = call(root, "node", node_args(&[], &[(addr, 0)])?)?;
        let env = ctx.driver().soroban_with(source, hf, data, vec![entry])?;
        let slug = format!("declared-depth-{depth}");
        if case == "host-limit" {
            let applied = ctx.submit_and_close(&env)?;
            ctx.capture(
                &slug,
                Selector::ledger_of(&applied),
                Spec::new(format!(
                    "an address entry whose matched root declares a chain {depth} invocations deep below it that no call reaches, the deepest the host still decodes; the transaction succeeds and the declared nodes are never checked"
                ))
                .fields(&ADDRESS_FIELDS),
            )?;
        } else {
            let applied = ctx.submit_and_close_failing(&env)?;
            ensure_invoke_result(
                &applied,
                |r| matches!(r, InvokeHostFunctionResult::Trapped),
                "invoke_host_function trapped",
            )?;
            ctx.capture(
                &slug,
                Selector::ledger_of(&applied),
                Spec::new(format!(
                    "an address entry declaring a chain {depth} invocations deep below its root, one past what the host decodes at depth 500: core includes it and the host traps"
                ))
                .fields(&ADDRESS_FIELDS)
                .flags(&["failed-tx"]),
            )?;
        }
    }
    Ok(())
}

/// An address entry the host matched and verified, in a transaction that
/// then traps: the call chain ends in a contract without `node`.
fn failed_with_auth(
    ctx: &mut ChainCtx,
    source: &SigningKey,
    trees: &[ContractId],
    echo: &ContractId,
    echo_hash: &Hash,
    prefetch: &[LedgerKey],
) -> Result<()> {
    let k = ctx.account("failed-signer");
    let addr = account_address(&k.verifying_key());
    // resources from the same call ending in a third auth_tree instance
    let good = call(
        &trees[0],
        "node",
        node_args(&trees[1..3], &chain_levels(&addr, 3))?,
    )?;
    let sim = simulator(ctx)?;
    sim.prefetch(prefetch)?;
    let source_id = account_id(&source.verifying_key());
    let recorded = sim.simulate_invoke(good.clone(), RECORD, &source_id)?;
    let mut signed = Vec::new();
    for e in recorded.auth {
        signed.push(sign_with(ctx, e, &k)?);
    }
    let n = entry_nonce(signed.first().context("no recorded auth")?)?;
    let mut data = sim
        .simulate_invoke(good, AuthMode::Enforce(signed), &source_id)?
        .transaction_data;
    add_read_only(&mut data, &[instance_key(echo), code_key(echo_hash)])?;

    // what the host matches before the trap: the root and the second
    // level, each with its own arguments
    let chain = [trees[1].clone(), echo.clone()];
    let levels = chain_levels(&addr, 3);
    let second = invocation(
        contract_fn(&trees[1], "node", node_args(&chain[1..], &levels[1..])?)?,
        Vec::new(),
    )?;
    let root = invocation(
        contract_fn(&trees[0], "node", node_args(&chain, &levels)?)?,
        vec![second],
    )?;
    let entry = sign_with(ctx, address_entry(&addr, n, root), &k)?;
    let hf = call(&trees[0], "node", node_args(&chain, &levels)?)?;
    let env = ctx.driver().soroban_with(source, hf, data, vec![entry])?;
    let applied = ctx.submit_and_close_failing(&env)?;
    ensure_invoke_result(
        &applied,
        |r| matches!(r, InvokeHostFunctionResult::Trapped),
        "invoke_host_function trapped",
    )?;
    ctx.capture(
        "failed-carries-auth",
        Selector::ledger_of(&applied),
        Spec::new(
            "a failed transaction whose address entry the host matched and verified at two levels before the third call trapped (that contract has no node function); the meta keeps only the fee",
        )
        .fields(&ADDRESS_FIELDS)
        .flags(&["failed-tx"]),
    )
}

/// The three wallet-backend#746 cases: creations declared in auth that
/// never happen.
fn unmatched_creations(
    ctx: &mut ChainCtx,
    source: &SigningKey,
    t0: &ContractId,
    tree_code: &Hash,
    echo: &ContractId,
    echo_hash: &Hash,
    prefetch: &[LedgerKey],
) -> Result<()> {
    let source_addr = account_address(&source.verifying_key());
    let source_id = account_id(&source.verifying_key());
    let k = ctx.account("unmatched-signer");
    let addr = account_address(&k.verifying_key());

    // case 1: the transaction fails. the source authorises the root call
    // and declares a creation the failed call never reaches
    let levels = [(source_addr.clone(), 1), (source_addr.clone(), 0)];
    let chain = [echo.clone()];
    let hf = call(t0, "node", node_args(&chain, &levels)?)?;
    let auth = vec![
        SorobanAuthorizationEntry {
            credentials: SorobanCredentials::SourceAccount,
            root_invocation: invocation(
                contract_fn(t0, "node", node_args(&chain, &levels)?)?,
                Vec::new(),
            )?,
        },
        SorobanAuthorizationEntry {
            credentials: SorobanCredentials::SourceAccount,
            root_invocation: invocation(
                create_v2_fn(&source_addr, &salt(FAMILY, "case-1-v2"), echo_hash),
                Vec::new(),
            )?,
        },
    ];
    let sim = simulator(ctx)?;
    sim.prefetch(prefetch)?;
    let good = call(t0, "node", node_args(&[], &[(source_addr.clone(), 0)])?)?;
    let mut data = sim
        .simulate_invoke(good, RECORD, &source_id)?
        .transaction_data;
    add_read_only(&mut data, &[instance_key(echo), code_key(echo_hash)])?;
    // the simulated call never loads echo, so the instructions are
    // raised to what a transaction may declare; the excess fee is refunded
    data.resources.instructions = tx_max_instructions()?;
    data.resource_fee = data.resource_fee * 2 + 10_000_000;
    let env = ctx.driver().soroban_with(source, hf, data, auth.clone())?;
    let applied = ctx.submit_and_close_failing(&env)?;
    ensure_invoke_result(
        &applied,
        |r| matches!(r, InvokeHostFunctionResult::Trapped),
        "invoke_host_function trapped",
    )?;
    ensure_declared_not_created(&applied, &auth, 1)?;
    ctx.capture(
        "unmatched-create-failed-tx",
        Selector::ledger_of(&applied),
        Spec::new(
            "a failed transaction whose source-account auth declares a CreateContractV2 deployment: the call traps, the meta creates no contract",
        )
        .fields(&SOURCE_FIELDS)
        .flags(&["failed-tx", "declared-not-executed"]),
    )?;

    // case 2: the call succeeds without consuming any entry
    let guarded_args = vec![ScVal::Address(addr.clone()), ScVal::Bool(false)];
    let hf = call(t0, "guarded", guarded_args.clone())?;
    let declared = address_entry(
        &addr,
        nonce("case-2"),
        invocation(
            contract_fn(t0, "guarded", guarded_args)?,
            vec![
                invocation(
                    create_v1_fn(&addr, &salt(FAMILY, "case-2-v1"), echo_hash),
                    Vec::new(),
                )?,
                invocation(
                    create_v2_fn(&addr, &salt(FAMILY, "case-2-v2"), tree_code),
                    Vec::new(),
                )?,
            ],
        )?,
    );
    let auth = vec![
        sign_with(ctx, declared, &k)?,
        SorobanAuthorizationEntry {
            credentials: SorobanCredentials::SourceAccount,
            root_invocation: invocation(
                create_v2_fn(&source_addr, &salt(FAMILY, "case-2-source"), echo_hash),
                Vec::new(),
            )?,
        },
    ];
    let sim = simulator(ctx)?;
    sim.prefetch(prefetch)?;
    let data = sim
        .simulate_invoke(hf.clone(), AuthMode::Enforce(auth.clone()), &source_id)
        .context("simulating guarded(false) with unmatched entries")?
        .transaction_data;
    let env = ctx.driver().soroban_with(source, hf, data, auth.clone())?;
    let applied = ctx.submit_and_close(&env)?;
    ensure!(
        applied.return_value() == Some(&ScVal::Bool(false)),
        "guarded returned {:?}",
        applied.return_value()
    );
    ensure_declared_not_created(&applied, &auth, 3)?;
    ctx.capture(
        "unmatched-create-success",
        Selector::ledger_of(&applied),
        Spec::new(
            "a successful call that never requires auth while its entries declare three deployments: an address entry rooted at the call with CreateContract and CreateContractV2 below it, and a source-account CreateContractV2 entry; the meta creates no contract",
        )
        .fields(&MIXED_FIELDS)
        .flags(&["declared-not-executed"]),
    )?;

    // case 3: the declared deployer is a claimable balance, which can never
    // deploy, with a creation nested below it
    let create_cb = OperationBody::CreateClaimableBalance(CreateClaimableBalanceOp {
        asset: Asset::Native,
        amount: 10_000_000,
        claimants: vec![Claimant::ClaimantTypeV0(ClaimantV0 {
            destination: account_id(&k.verifying_key()),
            predicate: ClaimPredicate::Unconditional,
        })]
        .try_into()?,
    });
    let env = ctx.classic(source, vec![create_cb])?;
    let applied = ctx.submit_and_close(&env)?;
    let balance_id = match &applied.result.result {
        TransactionResultResult::TxSuccess(ops) => match ops.first() {
            Some(OperationResult::OpInner(OperationResultTr::CreateClaimableBalance(
                CreateClaimableBalanceResult::Success(id),
            ))) => id.clone(),
            other => bail!("create claimable balance answered {other:?}"),
        },
        other => bail!("create claimable balance answered {other:?}"),
    };
    let cb_addr = ScAddress::ClaimableBalance(balance_id);
    let hf = call(
        t0,
        "guarded",
        vec![ScVal::Address(source_addr.clone()), ScVal::Bool(false)],
    )?;
    let auth = vec![SorobanAuthorizationEntry {
        credentials: SorobanCredentials::SourceAccount,
        root_invocation: invocation(
            create_v1_fn(&cb_addr, &salt(FAMILY, "case-3-cb"), echo_hash),
            vec![invocation(
                create_v2_fn(&source_addr, &salt(FAMILY, "case-3-nested"), echo_hash),
                Vec::new(),
            )?],
        )?,
    }];
    let sim = simulator(ctx)?;
    sim.prefetch(prefetch)?;
    let data = sim
        .simulate_invoke(hf.clone(), AuthMode::Enforce(auth.clone()), &source_id)
        .context("simulating guarded(false) with a claimable balance deployer declared")?
        .transaction_data;
    let env = ctx.driver().soroban_with(source, hf, data, auth.clone())?;
    let applied = ctx.submit_and_close(&env)?;
    ensure_declared_not_created(&applied, &auth, 2)?;
    ctx.capture(
        "unmatched-create-claimable-balance-deployer",
        Selector::ledger_of(&applied),
        Spec::new(
            "a successful call whose source-account entry declares a CreateContract from a claimable balance address, which can never deploy, with a CreateContractV2 nested below it; the meta creates no contract",
        )
        .fields(&SOURCE_FIELDS)
        .flags(&["declared-not-executed"]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(b: u8) -> ContractId {
        ContractId(Hash([b; 32]))
    }

    #[test]
    fn matched_tree_has_one_child_per_level() {
        let trees: Vec<ContractId> = (1..=4).map(id).collect();
        let signer = ScAddress::Contract(id(9));
        let t = matched_tree(&trees, &signer, 4).unwrap();
        assert_eq!(depth_of(&t), 4);
        assert_eq!(t.sub_invocations.len(), 1);
    }

    #[test]
    fn declared_creations_counts_nested_ones() {
        let a = ScAddress::Contract(id(1));
        let h = Hash([2; 32]);
        let t = invocation(
            create_v1_fn(&a, &[0; 32], &h),
            vec![invocation(create_v2_fn(&a, &[1; 32], &h), Vec::new()).unwrap()],
        )
        .unwrap();
        assert_eq!(declared_creations(&t), 2);
        assert_eq!(depth_of(&t), 2);
    }

    #[test]
    fn nonces_are_non_negative_and_distinct() {
        assert!(nonce("a") >= 0);
        assert_ne!(nonce("a"), nonce("b"));
    }
}
