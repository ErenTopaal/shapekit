//! The laws that hold inside one ledger.

use std::collections::{BTreeMap, BTreeSet};

use stellar_xdr::{
    ChangeTrustAsset, ClaimableBalanceId, ContractId, CreateClaimableBalanceResult,
    GeneralizedTransactionSet, Hash, HostFunction, InnerTransactionResultResult, LedgerCloseMeta,
    LedgerEntry, LedgerEntryChange, LedgerEntryChanges, LedgerEntryData, LedgerHeaderHistoryEntry,
    LedgerKey, LiquidityPoolEntryBody, LiquidityPoolParameters, OperationBody, OperationResult,
    OperationResultTr, PoolId, ScAddress, ScVal, TransactionEnvelope, TransactionMeta,
    TransactionPhase, TransactionResultPair, TransactionResultResult, TrustLineAsset,
    TxSetComponent,
};

use crate::derive::{self, Digest32};
use crate::{Law, Report};

/// One applied transaction as the ledger shows it.
struct Applied<'a> {
    result: &'a TransactionResultPair,
    meta: &'a TransactionMeta,
}

/// The parts every `LedgerCloseMeta` version shares.
struct View<'a> {
    header: &'a LedgerHeaderHistoryEntry,
    envelopes: Vec<&'a TransactionEnvelope>,
    tx_set_hash: Result<Digest32, stellar_xdr::Error>,
    tx_set_previous: &'a Hash,
    applied: Vec<Applied<'a>>,
}

fn generalized_envelopes(set: &GeneralizedTransactionSet) -> Vec<&TransactionEnvelope> {
    let GeneralizedTransactionSet::V1(v1) = set;
    let mut out = Vec::new();
    for phase in v1.phases.iter() {
        match phase {
            TransactionPhase::V0(components) => {
                for c in components.iter() {
                    let TxSetComponent::TxsetCompTxsMaybeDiscountedFee(comp) = c;
                    out.extend(comp.txs.iter());
                }
            }
            TransactionPhase::V1(parallel) => {
                for stage in parallel.execution_stages.iter() {
                    for cluster in stage.0.iter() {
                        out.extend(cluster.0.iter());
                    }
                }
            }
        }
    }
    out
}

fn view(lcm: &LedgerCloseMeta) -> View<'_> {
    match lcm {
        LedgerCloseMeta::V0(v) => View {
            header: &v.ledger_header,
            envelopes: v.tx_set.txs.iter().collect(),
            tx_set_hash: derive::legacy_tx_set_hash(&v.tx_set),
            tx_set_previous: &v.tx_set.previous_ledger_hash,
            applied: v
                .tx_processing
                .iter()
                .map(|t| Applied {
                    result: &t.result,
                    meta: &t.tx_apply_processing,
                })
                .collect(),
        },
        LedgerCloseMeta::V1(v) => View {
            header: &v.ledger_header,
            envelopes: generalized_envelopes(&v.tx_set),
            tx_set_hash: derive::generalized_tx_set_hash(&v.tx_set),
            tx_set_previous: generalized_previous(&v.tx_set),
            applied: v
                .tx_processing
                .iter()
                .map(|t| Applied {
                    result: &t.result,
                    meta: &t.tx_apply_processing,
                })
                .collect(),
        },
        LedgerCloseMeta::V2(v) => View {
            header: &v.ledger_header,
            envelopes: generalized_envelopes(&v.tx_set),
            tx_set_hash: derive::generalized_tx_set_hash(&v.tx_set),
            tx_set_previous: generalized_previous(&v.tx_set),
            applied: v
                .tx_processing
                .iter()
                .map(|t| Applied {
                    result: &t.result,
                    meta: &t.tx_apply_processing,
                })
                .collect(),
        },
    }
}

fn generalized_previous(set: &GeneralizedTransactionSet) -> &Hash {
    let GeneralizedTransactionSet::V1(v1) = set;
    &v1.previous_ledger_hash
}

/// The changes of each operation, in operation order.
fn operation_changes(meta: &TransactionMeta) -> Vec<&LedgerEntryChanges> {
    match meta {
        TransactionMeta::V0(ops) => ops.iter().map(|o| &o.changes).collect(),
        TransactionMeta::V1(m) => m.operations.iter().map(|o| &o.changes).collect(),
        TransactionMeta::V2(m) => m.operations.iter().map(|o| &o.changes).collect(),
        TransactionMeta::V3(m) => m.operations.iter().map(|o| &o.changes).collect(),
        TransactionMeta::V4(m) => m.operations.iter().map(|o| &o.changes).collect(),
    }
}

fn entry_of(change: &LedgerEntryChange) -> Option<&LedgerEntry> {
    match change {
        LedgerEntryChange::Created(e)
        | LedgerEntryChange::Updated(e)
        | LedgerEntryChange::State(e)
        | LedgerEntryChange::Restored(e) => Some(e),
        LedgerEntryChange::Removed(_) => None,
    }
}

/// The operation results of a transaction that reached apply, inner results for a fee bump.
fn operation_results(result: &TransactionResultResult) -> Option<&[OperationResult]> {
    match result {
        TransactionResultResult::TxSuccess(r) | TransactionResultResult::TxFailed(r) => {
            Some(r.as_slice())
        }
        TransactionResultResult::TxFeeBumpInnerSuccess(p)
        | TransactionResultResult::TxFeeBumpInnerFailed(p) => match &p.result.result {
            InnerTransactionResultResult::TxSuccess(r)
            | InnerTransactionResultResult::TxFailed(r) => Some(r.as_slice()),
            _ => None,
        },
        _ => None,
    }
}

fn succeeded(result: &TransactionResultResult) -> bool {
    matches!(
        result,
        TransactionResultResult::TxSuccess(_) | TransactionResultResult::TxFeeBumpInnerSuccess(_)
    )
}

fn envelope_kind(env: &TransactionEnvelope) -> &'static str {
    match env {
        TransactionEnvelope::TxV0(_) => "v0",
        TransactionEnvelope::Tx(_) => "v1",
        TransactionEnvelope::TxFeeBump(_) => "fee-bump",
    }
}

fn hex32(h: &[u8; 32]) -> String {
    hex::encode(h)
}

fn balance_hex(id: &ClaimableBalanceId) -> String {
    let ClaimableBalanceId::ClaimableBalanceIdTypeV0(h) = id;
    hex32(&h.0)
}

/// Checks every single-ledger law and returns the ledger's sequence, its stored hash and its
/// `previousLedgerHash`, for the chain law across ledgers.
pub fn check_ledger(
    lcm: &LedgerCloseMeta,
    network: Option<&Digest32>,
    report: &mut Report,
) -> (u32, [u8; 32], [u8; 32]) {
    let v = view(lcm);
    let header = &v.header.header;
    let seq = header.ledger_seq;
    let at = Some(seq);
    report.ledgers += 1;
    report.transactions += v.applied.len() as u64;
    *report.protocols.entry(header.ledger_version).or_default() += 1;

    // header-hash
    let failure = match derive::header_hash(header) {
        Ok(h) if h == v.header.hash.0 => None,
        Ok(h) => Some(format!(
            "sha256 of the header is {}, the stored hash is {}",
            hex32(&h),
            hex32(&v.header.hash.0)
        )),
        Err(e) => Some(format!("the header does not encode: {e}")),
    };
    report.record(Law::HeaderHash, at, failure);

    // tx-set-hash; core skips the check for the empty-set hash (LedgerManagerImpl.cpp,
    // Herder::EMPTY_TX_SET_HASH, all zeros)
    let want = &header.scp_value.tx_set_hash.0;
    if *want == [0; 32] {
        report.undecided(Law::TxSetHash);
    } else {
        let failure = match &v.tx_set_hash {
            Ok(h) if h == want => None,
            Ok(h) => Some(format!(
                "the tx set hashes to {}, scpValue.txSetHash is {}",
                hex32(h),
                hex32(want)
            )),
            Err(e) => Some(format!("the tx set does not encode: {e}")),
        };
        report.record(Law::TxSetHash, at, failure);
    }

    // tx-set-previous
    let failure = (v.tx_set_previous != &header.previous_ledger_hash).then(|| {
        format!(
            "the tx set names previous ledger {}, the header {}",
            hex32(&v.tx_set_previous.0),
            hex32(&header.previous_ledger_hash.0)
        )
    });
    report.record(Law::TxSetPrevious, at, failure);

    // result-set-hash
    let results: Vec<TransactionResultPair> = v.applied.iter().map(|a| a.result.clone()).collect();
    let failure = match derive::result_set_hash(results) {
        Ok(h) if h == header.tx_set_result_hash.0 => None,
        Ok(h) => Some(format!(
            "the result set hashes to {}, txSetResultHash is {}",
            hex32(&h),
            hex32(&header.tx_set_result_hash.0)
        )),
        Err(e) => Some(format!("the result set does not encode: {e}")),
    };
    report.record(Law::ResultSetHash, at, failure);

    let Some(network) = network else {
        for env in &v.envelopes {
            *report.envelopes.entry(envelope_kind(env)).or_default() += 1;
        }
        for a in &v.applied {
            pool_ids(
                &operation_changes(a.meta),
                None,
                &a.result.result.result,
                at,
                report,
            );
        }
        for law in [Law::TxHash, Law::BalanceId, Law::ContractId] {
            report.undecided(law);
        }
        return (seq, v.header.hash.0, header.previous_ledger_hash.0);
    };

    // tx-hash: the multiset of envelope hashes is the multiset of result hashes, and each fee
    // bump's inner hash is the inner hash core wrote
    let mut by_hash: BTreeMap<[u8; 32], &TransactionEnvelope> = BTreeMap::new();
    let mut ours: Vec<[u8; 32]> = Vec::new();
    let mut encode_errors = Vec::new();
    for env in &v.envelopes {
        *report.envelopes.entry(envelope_kind(env)).or_default() += 1;
        match derive::envelope_hash(network, env) {
            Ok(h) => {
                ours.push(h);
                by_hash.insert(h, env);
            }
            Err(e) => encode_errors.push(e.to_string()),
        }
    }
    let mut theirs: Vec<[u8; 32]> = v
        .applied
        .iter()
        .map(|a| a.result.transaction_hash.0)
        .collect();
    ours.sort_unstable();
    theirs.sort_unstable();
    let failure = if !encode_errors.is_empty() {
        Some(format!(
            "envelopes do not encode: {}",
            encode_errors.join("; ")
        ))
    } else if ours != theirs {
        let a: BTreeSet<_> = ours.iter().collect();
        let b: BTreeSet<_> = theirs.iter().collect();
        let missing: Vec<String> = b.difference(&a).map(|h| hex32(h)).take(3).collect();
        Some(format!(
            "{} envelopes, {} results; result hashes no envelope hashes to: {}",
            ours.len(),
            theirs.len(),
            missing.join(", ")
        ))
    } else {
        None
    };
    report.record(Law::TxHash, at, failure);
    for a in &v.applied {
        let (TransactionResultResult::TxFeeBumpInnerSuccess(inner)
        | TransactionResultResult::TxFeeBumpInnerFailed(inner)) = &a.result.result.result
        else {
            continue;
        };
        let Some(TransactionEnvelope::TxFeeBump(fb)) = by_hash.get(&a.result.transaction_hash.0)
        else {
            // the outer hash law above already failed for this result
            continue;
        };
        let stellar_xdr::FeeBumpTransactionInnerTx::Tx(env) = &fb.tx.inner_tx;
        let failure = match derive::transaction_hash(network, &env.tx) {
            Ok(h) if h == inner.transaction_hash.0 => None,
            Ok(h) => Some(format!(
                "inner transaction of {} hashes to {}, core wrote {}",
                hex32(&a.result.transaction_hash.0),
                hex32(&h),
                hex32(&inner.transaction_hash.0)
            )),
            Err(e) => Some(format!("inner transaction does not encode: {e}")),
        };
        report.record(Law::TxHash, at, failure);
    }

    // the identifier laws read each applied transaction with its envelope
    for a in &v.applied {
        let env = by_hash.get(&a.result.transaction_hash.0).copied();
        let ops = operation_changes(a.meta);
        pool_ids(&ops, env, &a.result.result.result, at, report);
        let Some(env) = env else { continue };
        balance_ids(env, &ops, &a.result.result.result, at, report);
        contract_ids(env, &ops, &a.result.result.result, network, at, report);
    }

    (seq, v.header.hash.0, header.previous_ledger_hash.0)
}

fn pool_ids(
    ops: &[&LedgerEntryChanges],
    env: Option<&TransactionEnvelope>,
    result: &TransactionResultResult,
    at: Option<u32>,
    report: &mut Report,
) {
    for changes in ops {
        for change in changes.iter() {
            let Some(LedgerEntry {
                data: LedgerEntryData::LiquidityPool(pool),
                ..
            }) = entry_of(change)
            else {
                continue;
            };
            let LiquidityPoolEntryBody::LiquidityPoolConstantProduct(cp) = &pool.body;
            let params = LiquidityPoolParameters::LiquidityPoolConstantProduct(cp.params.clone());
            let failure = match derive::pool_id(&params) {
                Ok(id) if id == pool.liquidity_pool_id => None,
                Ok(id) => Some(format!(
                    "pool entry {} has parameters that hash to {}",
                    hex32(&pool.liquidity_pool_id.0.0),
                    hex32(&id.0.0)
                )),
                Err(e) => Some(format!("pool parameters do not encode: {e}")),
            };
            report.record(Law::PoolId, at, failure);
        }
    }

    // a successful ChangeTrust on a pool share touches the trustline of the derived pool
    let Some(env) = env else { return };
    if !succeeded(result) {
        return;
    }
    let tx = derive::inner_transaction(env);
    for (i, op) in tx.operations.iter().enumerate() {
        let OperationBody::ChangeTrust(ct) = &op.body else {
            continue;
        };
        let ChangeTrustAsset::PoolShare(params) = &ct.line else {
            continue;
        };
        let Some(changes) = ops.get(i) else {
            report.record(Law::PoolId, at, Some(format!("operation {i} has no meta")));
            continue;
        };
        let failure = match derive::pool_id(params) {
            Ok(id) => {
                let touched = changes.iter().any(|c| trustline_pool(c) == Some(&id));
                (!touched).then(|| {
                    format!(
                        "ChangeTrust operation {i} succeeded, but no pool share trustline of the derived pool {} changed",
                        hex32(&id.0.0)
                    )
                })
            }
            Err(e) => Some(format!("pool parameters do not encode: {e}")),
        };
        report.record(Law::PoolId, at, failure);
    }
}

fn trustline_pool(change: &LedgerEntryChange) -> Option<&PoolId> {
    match change {
        LedgerEntryChange::Removed(LedgerKey::Trustline(k)) => match &k.asset {
            TrustLineAsset::PoolShare(p) => Some(p),
            _ => None,
        },
        _ => match entry_of(change) {
            Some(LedgerEntry {
                data: LedgerEntryData::Trustline(t),
                ..
            }) => match &t.asset {
                TrustLineAsset::PoolShare(p) => Some(p),
                _ => None,
            },
            _ => None,
        },
    }
}

fn balance_ids(
    env: &TransactionEnvelope,
    ops: &[&LedgerEntryChanges],
    result: &TransactionResultResult,
    at: Option<u32>,
    report: &mut Report,
) {
    let tx = derive::inner_transaction(env);
    let source = derive::account_id(&tx.source_account);
    let results = operation_results(result);

    // the id a successful CreateClaimableBalance returns
    if let Some(results) = results {
        for (i, r) in results.iter().enumerate() {
            let OperationResult::OpInner(OperationResultTr::CreateClaimableBalance(
                CreateClaimableBalanceResult::Success(id),
            )) = r
            else {
                continue;
            };
            let failure = match derive::operation_balance_id(&source, &tx.seq_num, i as u32) {
                Ok(d) if &d == id => None,
                Ok(d) => Some(format!(
                    "CreateClaimableBalance operation {i} returned {}, the operation id derives {}",
                    balance_hex(id),
                    balance_hex(&d)
                )),
                Err(e) => Some(format!("operation id does not encode: {e}")),
            };
            report.record(Law::BalanceId, at, failure);
        }
    }

    // every balance an operation creates, whichever operation created it
    for (i, changes) in ops.iter().enumerate() {
        for change in changes.iter() {
            let LedgerEntryChange::Created(LedgerEntry {
                data: LedgerEntryData::ClaimableBalance(cb),
                ..
            }) = change
            else {
                continue;
            };
            let failure = match explain_balance(&tx, &source, i, changes, &cb.asset) {
                Ok(candidates) if candidates.contains(&cb.balance_id) => None,
                Ok(candidates) => Some(format!(
                    "operation {i} created balance {}, which none of its {} derivable ids match",
                    balance_hex(&cb.balance_id),
                    candidates.len()
                )),
                Err(e) => Some(format!("a balance id preimage does not encode: {e}")),
            };
            report.record(Law::BalanceId, at, failure);
        }
    }
}

/// The ids operation `i` can create a balance under: its operation id for
/// `CreateClaimableBalance`, and for a revocation (AllowTrust, SetTrustLineFlags) the revoke id
/// of each pool share trustline it removed, with the balance's asset.
fn explain_balance(
    tx: &stellar_xdr::Transaction,
    source: &stellar_xdr::AccountId,
    i: usize,
    changes: &LedgerEntryChanges,
    asset: &stellar_xdr::Asset,
) -> Result<Vec<ClaimableBalanceId>, stellar_xdr::Error> {
    let index = i as u32;
    let mut out = Vec::new();
    match tx.operations.get(i).map(|o| &o.body) {
        Some(OperationBody::CreateClaimableBalance(_)) => {
            out.push(derive::operation_balance_id(source, &tx.seq_num, index)?);
        }
        Some(OperationBody::AllowTrust(_) | OperationBody::SetTrustLineFlags(_)) => {
            for c in changes.iter() {
                if let LedgerEntryChange::Removed(LedgerKey::Trustline(k)) = c
                    && let TrustLineAsset::PoolShare(pool) = &k.asset
                {
                    out.push(derive::revoke_balance_id(
                        source,
                        &tx.seq_num,
                        index,
                        pool,
                        asset,
                    )?);
                }
            }
        }
        _ => {}
    }
    Ok(out)
}

fn contract_ids(
    env: &TransactionEnvelope,
    ops: &[&LedgerEntryChanges],
    result: &TransactionResultResult,
    network: &Digest32,
    at: Option<u32>,
    report: &mut Report,
) {
    let tx = derive::inner_transaction(env);
    let mut explained = BTreeSet::new();
    if succeeded(result) {
        for (i, op) in tx.operations.iter().enumerate() {
            let OperationBody::InvokeHostFunction(ihf) = &op.body else {
                continue;
            };
            let preimage = match &ihf.host_function {
                HostFunction::CreateContract(args) => &args.contract_id_preimage,
                HostFunction::CreateContractV2(args) => &args.contract_id_preimage,
                _ => continue,
            };
            let failure = match derive::contract_id(network, preimage) {
                Ok(id) => {
                    explained.insert(id);
                    let created = ops.get(i).is_some_and(|changes| {
                        changes.iter().any(|c| created_instance(c) == Some(&id))
                    });
                    (!created).then(|| {
                        format!(
                            "host function in operation {i} created a contract, but no instance at the derived id {} was created",
                            hex32(&id)
                        )
                    })
                }
                Err(e) => Some(format!("contract id preimage does not encode: {e}")),
            };
            report.record(Law::ContractId, at, failure);
        }
    }
    // instances created from inside a contract carry their preimage in the host's state only
    for changes in ops {
        for c in changes.iter() {
            if let Some(id) = created_instance(c)
                && !explained.contains(id)
            {
                report.undecided(Law::ContractId);
            }
        }
    }
}

fn created_instance(change: &LedgerEntryChange) -> Option<&[u8; 32]> {
    let LedgerEntryChange::Created(LedgerEntry {
        data: LedgerEntryData::ContractData(cd),
        ..
    }) = change
    else {
        return None;
    };
    match (&cd.contract, &cd.key) {
        (ScAddress::Contract(ContractId(h)), ScVal::LedgerKeyContractInstance) => Some(&h.0),
        _ => None,
    }
}
