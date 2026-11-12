//! Soroban steps shared by the invoke, auth-tree, CAP-71 and CAP-85
//! recipes: the committed contracts beyond echo, simulation with signed
//! auth, footprint-only operations, and reading what an operation created.

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use shapekit_core_driver::chain::Applied;
use shapekit_core_driver::keys::{account_address, account_id};
use shapekit_core_driver::sim::{AuthMode, InvokeSimulation};
use shapekit_core_driver::sim_legacy::AnySimulator;
use shapekit_core_driver::tx::{BASE_FEE, assemble_soroban};
use stellar_xdr::{
    ContractDataDurability, ContractEvent, ContractExecutable, ContractId, CreateContractArgsV2,
    ExtendFootprintTtlOp, ExtensionPoint, Hash, HostFunction, InvokeContractArgs,
    LedgerEntryChange, LedgerEntryData, LedgerKey, LedgerKeyContractCode, LedgerKeyContractData,
    OperationBody, RestoreFootprintOp, ScAddress, ScSymbol, ScVal, SorobanAuthorizationEntry,
    SorobanTransactionData, TransactionEnvelope, TransactionMeta,
};

use super::ChainCtx;

/// `contracts/wasm/auth_tree.wasm`: `node(chain, levels)` builds auth trees
/// of chosen depth and fan-out, `guarded(signer, enforce)` requires auth
/// only when told to.
pub const AUTH_TREE_WASM: &[u8] = include_bytes!("../../../../contracts/wasm/auth_tree.wasm");
/// `contracts/wasm/factory.wasm`: constructor `(owner)`, `deploy` and
/// `deploy_with_args`.
pub const FACTORY_WASM: &[u8] = include_bytes!("../../../../contracts/wasm/factory.wasm");
/// `contracts/wasm/exec_ref.wasm`: CAP-85 executable reference owner.
pub const EXEC_REF_WASM: &[u8] = include_bytes!("../../../../contracts/wasm/exec_ref.wasm");
/// `contracts/wasm/delegating_account.wasm`: CAP-71 custom account that
/// authenticates through delegated signers only.
pub const DELEGATING_ACCOUNT_WASM: &[u8] =
    include_bytes!("../../../../contracts/wasm/delegating_account.wasm");

/// Auth signatures stay valid this many ledgers past the last closed one.
pub const AUTH_VALID_FOR: u32 = 100;

/// Deterministic 32-byte salt for `use_` within `recipe`.
pub fn salt(recipe: &str, use_: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"shapekit/soroban-salt/v1");
    h.update(recipe.as_bytes());
    h.update([0]);
    h.update(use_.as_bytes());
    h.finalize().into()
}

pub fn sym(s: &str) -> Result<ScVal> {
    Ok(ScVal::Symbol(ScSymbol(s.try_into()?)))
}

pub fn string(s: &str) -> Result<ScVal> {
    Ok(ScVal::String(s.as_bytes().to_vec().try_into()?))
}

pub fn address(a: ScAddress) -> ScVal {
    ScVal::Address(a)
}

pub fn contract_address(id: &ContractId) -> ScAddress {
    ScAddress::Contract(id.clone())
}

pub fn bytes32(b: &[u8; 32]) -> Result<ScVal> {
    Ok(ScVal::Bytes(b.to_vec().try_into()?))
}

pub fn vec_val(items: Vec<ScVal>) -> Result<ScVal> {
    Ok(ScVal::Vec(Some(items.try_into()?)))
}

/// An `InvokeContract` host function.
pub fn call(contract: &ContractId, function: &str, args: Vec<ScVal>) -> Result<HostFunction> {
    Ok(HostFunction::InvokeContract(InvokeContractArgs {
        contract_address: contract_address(contract),
        function_name: ScSymbol(function.try_into()?),
        args: args.try_into()?,
    }))
}

/// A `CreateContractV2` host function deploying `code` from `deployer`.
pub fn create_v2(
    deployer: &ScAddress,
    salt: &[u8; 32],
    executable: ContractExecutable,
    args: Vec<ScVal>,
) -> Result<HostFunction> {
    Ok(HostFunction::CreateContractV2(CreateContractArgsV2 {
        contract_id_preimage: shapekit_core_driver::tx::contract_preimage(deployer, salt),
        executable,
        constructor_args: args.try_into()?,
    }))
}

/// A simulator over the last closed ledger.
pub fn simulator(ctx: &mut ChainCtx) -> Result<AnySimulator> {
    let node = ctx.driver().node();
    let lc = node.ledger_context()?;
    Ok(AnySimulator::new(node.snapshot()?, &lc)?)
}

/// Simulates `hf` from `source` under `mode`.
pub fn simulate(
    ctx: &mut ChainCtx,
    source: &SigningKey,
    hf: &HostFunction,
    mode: AuthMode,
) -> Result<InvokeSimulation> {
    let sim = simulator(ctx)?;
    Ok(sim.simulate_invoke(hf.clone(), mode, &account_id(&source.verifying_key()))?)
}

/// The usual client flow for a call that needs auth beyond the source
/// account: record the auth entries, let `sign` complete each one, then
/// simulate again enforcing the signed entries so the resources include
/// the signature checks, and assemble the transaction.
pub fn authorized(
    ctx: &mut ChainCtx,
    source: &SigningKey,
    hf: HostFunction,
    record: AuthMode,
    sign: impl FnMut(&mut ChainCtx, SorobanAuthorizationEntry) -> Result<SorobanAuthorizationEntry>,
) -> Result<(TransactionEnvelope, Vec<SorobanAuthorizationEntry>)> {
    authorized_prefetched(ctx, source, hf, record, &[], sign)
}

/// [`authorized`], with `prefetch` loaded in one query before either
/// simulation. Calls that touch many entries need it: one query per entry
/// takes long enough for the standalone virtual clock to run past the next
/// close time.
pub fn authorized_prefetched(
    ctx: &mut ChainCtx,
    source: &SigningKey,
    hf: HostFunction,
    record: AuthMode,
    prefetch: &[LedgerKey],
    mut sign: impl FnMut(&mut ChainCtx, SorobanAuthorizationEntry) -> Result<SorobanAuthorizationEntry>,
) -> Result<(TransactionEnvelope, Vec<SorobanAuthorizationEntry>)> {
    let sim = simulator(ctx)?;
    if !prefetch.is_empty() {
        sim.prefetch(prefetch)?;
    }
    let source_id = account_id(&source.verifying_key());
    let recorded = sim.simulate_invoke(hf.clone(), record, &source_id)?;
    let mut signed = Vec::with_capacity(recorded.auth.len());
    for e in recorded.auth {
        signed.push(sign(ctx, e)?);
    }
    let enforced =
        sim.simulate_invoke(hf.clone(), AuthMode::Enforce(signed.clone()), &source_id)?;
    let env = ctx
        .driver()
        .soroban_with(source, hf, enforced.transaction_data, signed.clone())?;
    Ok((env, signed))
}

/// Expiration ledger for auth signed now.
pub fn expiration(ctx: &mut ChainCtx) -> u32 {
    ctx.driver().last_closed() + AUTH_VALID_FOR
}

/// A transaction from `source` whose single operation is a footprint-only
/// one (extend TTL or restore), with the simulated `data`.
pub fn footprint_tx(
    ctx: &mut ChainCtx,
    source: &SigningKey,
    body: OperationBody,
    data: SorobanTransactionData,
) -> Result<TransactionEnvelope> {
    let tx = ctx
        .tx(source)?
        .fee(BASE_FEE)
        .operation(None, body)
        .build()?;
    let tx = assemble_soroban(tx, data, None)?;
    ctx.sign(tx, &[source])
}

/// Restores `keys` from `source`; fails unless core applied it.
pub fn restore(ctx: &mut ChainCtx, source: &SigningKey, keys: &[LedgerKey]) -> Result<Applied> {
    let data = simulator(ctx)?.simulate_restore(keys)?;
    let body = OperationBody::RestoreFootprint(RestoreFootprintOp {
        ext: ExtensionPoint::V0,
    });
    let env = footprint_tx(ctx, source, body, data)?;
    ctx.submit_and_close(&env)
}

/// Extends `keys` to live `extend_to` ledgers past the current one.
pub fn extend_ttl(
    ctx: &mut ChainCtx,
    source: &SigningKey,
    keys: &[LedgerKey],
    extend_to: u32,
) -> Result<Applied> {
    let sim = simulator(ctx)?.simulate_extend_ttl(keys, extend_to)?;
    ensure!(
        sim.restore.is_none(),
        "extend ttl: some keys are archived and need a restore first"
    );
    let body = OperationBody::ExtendFootprintTtl(ExtendFootprintTtlOp {
        ext: ExtensionPoint::V0,
        extend_to,
    });
    let env = footprint_tx(ctx, source, body, sim.transaction_data)?;
    ctx.submit_and_close(&env)
}

/// Uploads `wasm` from `source`, returning its hash.
pub fn upload(ctx: &mut ChainCtx, source: &SigningKey, wasm: &[u8]) -> Result<Hash> {
    Ok(ctx.driver().upload(source, wasm)?)
}

/// The instance key of a contract.
pub fn instance_key(id: &ContractId) -> LedgerKey {
    LedgerKey::ContractData(LedgerKeyContractData {
        contract: contract_address(id),
        key: ScVal::LedgerKeyContractInstance,
        durability: ContractDataDurability::Persistent,
    })
}

pub fn code_key(hash: &Hash) -> LedgerKey {
    LedgerKey::ContractCode(LedgerKeyContractCode { hash: hash.clone() })
}

/// The account address of a key, as an `ScVal`.
pub fn account_val(key: &SigningKey) -> ScVal {
    ScVal::Address(account_address(&key.verifying_key()))
}

/// Every change the operations of a transaction made, transaction-level
/// changes left out.
pub fn operation_changes(meta: &TransactionMeta) -> Vec<LedgerEntryChange> {
    match meta {
        TransactionMeta::V3(m) => m
            .operations
            .iter()
            .flat_map(|o| o.changes.iter().cloned())
            .collect(),
        TransactionMeta::V4(m) => m
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

/// Contract events the operations of a transaction emitted.
pub fn contract_events(meta: &TransactionMeta) -> Result<Vec<ContractEvent>> {
    Ok(match meta {
        TransactionMeta::V4(m) => m
            .operations
            .iter()
            .flat_map(|o| o.events.iter().cloned())
            .collect(),
        TransactionMeta::V3(m) => m
            .soroban_meta
            .as_ref()
            .map(|s| s.events.to_vec())
            .unwrap_or_default(),
        other => bail!("transaction meta {other:?} carries no contract events"),
    })
}

/// Contracts whose instance entry the operations created.
pub fn created_instances(meta: &TransactionMeta) -> Vec<(ScAddress, ContractExecutable)> {
    operation_changes(meta)
        .into_iter()
        .filter_map(|c| match c {
            LedgerEntryChange::Created(e) => match e.data {
                LedgerEntryData::ContractData(d) if d.key == ScVal::LedgerKeyContractInstance => {
                    match d.val {
                        ScVal::ContractInstance(i) => Some((d.contract, i.executable)),
                        _ => None,
                    }
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// The keys a transaction's footprint names, read-only then read-write.
pub fn footprint(env: &TransactionEnvelope) -> Result<Vec<LedgerKey>> {
    let TransactionEnvelope::Tx(e) = env else {
        bail!("not a v1 envelope");
    };
    let stellar_xdr::TransactionExt::V1(d) = &e.tx.ext else {
        bail!("not a soroban transaction");
    };
    let fp = &d.resources.footprint;
    Ok(fp
        .read_only
        .iter()
        .chain(fp.read_write.iter())
        .cloned()
        .collect())
}

/// The Soroban transaction data of a v1 envelope.
pub fn soroban_data(env: &TransactionEnvelope) -> Result<SorobanTransactionData> {
    let TransactionEnvelope::Tx(e) = env else {
        bail!("not a v1 envelope");
    };
    match &e.tx.ext {
        stellar_xdr::TransactionExt::V1(d) => Ok(d.clone()),
        stellar_xdr::TransactionExt::V0 => bail!("not a soroban transaction"),
    }
}

/// Replaces the transaction data of a signed envelope's transaction and
/// signs it again with `signers`, for steps that submit deliberately
/// wrong resources.
pub fn with_data(
    ctx: &mut ChainCtx,
    env: &TransactionEnvelope,
    edit: impl FnOnce(&mut SorobanTransactionData),
    signers: &[&SigningKey],
) -> Result<TransactionEnvelope> {
    let TransactionEnvelope::Tx(e) = env else {
        bail!("not a v1 envelope");
    };
    let mut tx = e.tx.clone();
    let stellar_xdr::TransactionExt::V1(d) = &mut tx.ext else {
        bail!("not a soroban transaction");
    };
    edit(d);
    ctx.sign(tx, signers)
}

/// A Soroban transaction from `source` with exactly these parts, for
/// envelopes that simulation cannot produce (calls that fail, or auth
/// trees built by hand). Resources come from the caller.
pub fn raw_soroban(
    ctx: &mut ChainCtx,
    source: &SigningKey,
    hf: HostFunction,
    data: SorobanTransactionData,
    auth: Vec<SorobanAuthorizationEntry>,
) -> Result<TransactionEnvelope> {
    Ok(ctx.driver().soroban_with(source, hf, data, auth)?)
}

/// Fails unless the transaction's result names this invoke-host-function
/// result.
pub fn ensure_invoke_result(
    applied: &Applied,
    want: fn(&stellar_xdr::InvokeHostFunctionResult) -> bool,
    what: &str,
) -> Result<()> {
    use stellar_xdr::{OperationResult, OperationResultTr, TransactionResultResult};
    let TransactionResultResult::TxFailed(ops) = &applied.result.result else {
        bail!(
            "ledger {}: expected a failed transaction ({what}), core answered {:?}",
            applied.ledger,
            applied.result.result
        );
    };
    let op = ops.first().context("tx_failed without operation results")?;
    match op {
        OperationResult::OpInner(OperationResultTr::InvokeHostFunction(r)) if want(r) => Ok(()),
        other => bail!(
            "ledger {}: expected {what}, core answered {other:?}",
            applied.ledger
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contracts_are_the_committed_builds() {
        let sums = include_str!("../../../../contracts/wasm/SHA256SUMS");
        for (name, wasm) in [
            ("auth_tree.wasm", AUTH_TREE_WASM),
            ("factory.wasm", FACTORY_WASM),
            ("exec_ref.wasm", EXEC_REF_WASM),
            ("delegating_account.wasm", DELEGATING_ACCOUNT_WASM),
        ] {
            let want = sums
                .lines()
                .find_map(|l| l.strip_suffix(&format!("  {name}")))
                .unwrap_or_else(|| panic!("{name} in SHA256SUMS"));
            assert_eq!(hex::encode(Sha256::digest(wasm)), want, "{name}");
        }
    }

    #[test]
    fn salts_differ_by_use_and_recipe() {
        assert_ne!(salt("a", "x"), salt("a", "y"));
        assert_ne!(salt("a", "x"), salt("b", "x"));
        assert_eq!(salt("a", "x"), salt("a", "x"));
    }
}
