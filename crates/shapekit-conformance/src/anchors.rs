//! Checks whose answer the network wrote into a ledger: the hash of every transaction, the
//! id of every claimable balance and contract a transaction created. The runner computes each
//! with the law library (shapekit-laws) and takes it as the expected answer only where the
//! ledger confirms it: a transaction hash must be the hash core wrote in that transaction's
//! result pair, a balance id is the one core returned, a contract id is the id of the instance
//! the operation created. Where the ledger does not confirm the derivation the check is
//! `Unmappable`, an `error`, never a finding against an sdk.
//!
//! Requests (see docs/testee-protocol.md): `tx_hash` takes a `TransactionEnvelope` and the
//! network passphrase, and with `inner: true` hashes a fee bump's inner transaction;
//! `balance_id` takes the envelope and an operation index; `contract_id` takes a
//! `ContractIdPreimage` and the passphrase.

use std::collections::BTreeMap;
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Map, Value, json};
use shapekit_laws::derive::{self, Digest32};
use shapekit_manifest::{Entry, decode_limits};
use stellar_xdr::{
    CreateClaimableBalanceResult, GeneralizedTransactionSet, HostFunction,
    InnerTransactionResultResult, LedgerCloseMeta, LedgerEntry, LedgerEntryChange,
    LedgerEntryChanges, LedgerEntryData, Limits, OperationBody, OperationResult, OperationResultTr,
    ReadXdr, ScAddress, ScVal, TransactionEnvelope, TransactionMeta, TransactionPhase,
    TransactionResultPair, TransactionResultResult, TxSetComponent, WriteXdr,
};

use crate::plan::{Check, Expect};

/// The ops these checks send.
pub const OPS: [&str; 3] = ["tx_hash", "balance_id", "contract_id"];

struct Applied<'a> {
    result: &'a TransactionResultPair,
    meta: &'a TransactionMeta,
}

fn generalized(set: &GeneralizedTransactionSet) -> Vec<&TransactionEnvelope> {
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

/// The envelopes of the ledger's transaction set in set order, and its applied transactions.
fn parts(lcm: &LedgerCloseMeta) -> (Vec<&TransactionEnvelope>, Vec<Applied<'_>>) {
    macro_rules! applied {
        ($v:expr) => {
            $v.tx_processing
                .iter()
                .map(|t| Applied {
                    result: &t.result,
                    meta: &t.tx_apply_processing,
                })
                .collect()
        };
    }
    match lcm {
        LedgerCloseMeta::V0(v) => (v.tx_set.txs.iter().collect(), applied!(v)),
        LedgerCloseMeta::V1(v) => (generalized(&v.tx_set), applied!(v)),
        LedgerCloseMeta::V2(v) => (generalized(&v.tx_set), applied!(v)),
    }
}

fn operation_changes(meta: &TransactionMeta) -> Vec<&LedgerEntryChanges> {
    match meta {
        TransactionMeta::V0(ops) => ops.iter().map(|o| &o.changes).collect(),
        TransactionMeta::V1(m) => m.operations.iter().map(|o| &o.changes).collect(),
        TransactionMeta::V2(m) => m.operations.iter().map(|o| &o.changes).collect(),
        TransactionMeta::V3(m) => m.operations.iter().map(|o| &o.changes).collect(),
        TransactionMeta::V4(m) => m.operations.iter().map(|o| &o.changes).collect(),
    }
}

/// The operation results of a transaction that reached apply, the inner ones for a fee bump.
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

fn created_instance(change: &LedgerEntryChange) -> Option<&[u8; 32]> {
    let LedgerEntryChange::Created(LedgerEntry {
        data: LedgerEntryData::ContractData(cd),
        ..
    }) = change
    else {
        return None;
    };
    match (&cd.contract, &cd.key) {
        (ScAddress::Contract(id), ScVal::LedgerKeyContractInstance) => Some(&id.0.0),
        _ => None,
    }
}

fn encode<T: WriteXdr>(v: &T) -> Result<Vec<u8>, String> {
    v.to_xdr(Limits::none())
        .map_err(|e| format!("re-encoding failed: {e}"))
}

fn request(op: &str, members: Value) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("op".into(), Value::from(op));
    if let Value::Object(o) = members {
        m.extend(o);
    }
    m
}

fn envelope_request(env_xdr: &[u8], passphrase: &str) -> Value {
    json!({
        "xdr": B64.encode(env_xdr),
        "network_passphrase": passphrase,
        "depth_limit": shapekit_manifest::decode_depth("TransactionEnvelope"),
        "len_limit": env_xdr.len(),
    })
}

fn check(
    name: String,
    op: &str,
    members: Value,
    expect: Result<Expect, String>,
    source: String,
) -> Check {
    Check {
        name,
        op: op.to_string(),
        request: request(op, members),
        expect: expect.unwrap_or_else(Expect::Unmappable),
        source,
    }
}

fn hex_expect(member: &str, bytes: &[u8]) -> Expect {
    Expect::Hex {
        member: member.to_string(),
        value: hex::encode(bytes),
    }
}

/// The hash checks of one envelope that core applied in `lcm`. `label` names the envelope in
/// check names ("" for an envelope entry, " <i>" inside a ledger).
/// A network: its passphrase, which requests carry, and its id, which hashes take.
struct Net<'a> {
    id: Digest32,
    passphrase: &'a str,
}

fn hash_checks(
    variant: &str,
    label: &str,
    env: &TransactionEnvelope,
    by_hash: &BTreeMap<[u8; 32], &TransactionResultPair>,
    net: &Net<'_>,
    where_: &str,
    out: &mut Vec<Check>,
) {
    let env_xdr = match encode(env) {
        Ok(b) => b,
        Err(e) => {
            out.push(check(
                format!("{variant}/tx_hash{label}"),
                "tx_hash",
                json!({}),
                Err(format!("the envelope {e}")),
                where_.to_string(),
            ));
            return;
        }
    };
    let derived = derive::envelope_hash(&net.id, env);
    let pair = derived.as_ref().ok().and_then(|h| by_hash.get(h));
    let expect = match (&derived, pair) {
        (Err(e), _) => Err(format!("the law library cannot hash the envelope: {e}")),
        (Ok(h), None) => Err(format!(
            "no result pair in the ledger carries the derived hash {}",
            hex::encode(h)
        )),
        (Ok(h), Some(_)) => Ok(hex_expect("hash", h)),
    };
    out.push(check(
        format!("{variant}/tx_hash{label}"),
        "tx_hash",
        envelope_request(&env_xdr, net.passphrase),
        expect,
        format!("{where_}: transactionHash of the envelope's result pair"),
    ));
    if let (TransactionEnvelope::TxFeeBump(_), Some(pair)) = (env, pair) {
        let (TransactionResultResult::TxFeeBumpInnerSuccess(inner)
        | TransactionResultResult::TxFeeBumpInnerFailed(inner)) = &pair.result.result
        else {
            return;
        };
        let mut members = envelope_request(&env_xdr, net.passphrase);
        members["inner"] = Value::Bool(true);
        out.push(check(
            format!("{variant}/tx_hash{label} inner"),
            "tx_hash",
            members,
            Ok(hex_expect("hash", &inner.transaction_hash.0)),
            format!("{where_}: transactionHash of the fee bump's inner result pair"),
        ));
    }
}

fn result_index<'a>(applied: &[Applied<'a>]) -> BTreeMap<[u8; 32], &'a TransactionResultPair> {
    applied
        .iter()
        .map(|a| (a.result.transaction_hash.0, a.result))
        .collect()
}

/// Every anchored check of one `LedgerCloseMeta` variant.
pub fn ledger_checks(
    variant: &str,
    passphrase: &str,
    lcm: &LedgerCloseMeta,
    file: &str,
) -> Vec<Check> {
    let network = derive::network_id(passphrase);
    let (envelopes, applied) = parts(lcm);
    let by_hash = result_index(&applied);
    let mut out = Vec::new();
    for (i, env) in envelopes.iter().enumerate() {
        hash_checks(
            variant,
            &format!(" {i}"),
            env,
            &by_hash,
            &Net {
                id: network,
                passphrase,
            },
            &format!("{file}, envelope {i} of the transaction set"),
            &mut out,
        );
    }

    // the envelope of each applied transaction, found by the hash core wrote for it
    let mut env_of: BTreeMap<[u8; 32], (usize, &TransactionEnvelope)> = BTreeMap::new();
    for (i, env) in envelopes.iter().enumerate() {
        if let Ok(h) = derive::envelope_hash(&network, env) {
            env_of.insert(h, (i, env));
        }
    }
    for a in &applied {
        let Some(&(i, env)) = env_of.get(&a.result.transaction_hash.0) else {
            // the tx_hash check of this ledger already reports the missing envelope
            continue;
        };
        let Ok(env_xdr) = encode(env) else { continue };
        let tx = derive::inner_transaction(env);
        let results = operation_results(&a.result.result.result);
        for (k, r) in results.into_iter().flatten().enumerate() {
            let OperationResult::OpInner(OperationResultTr::CreateClaimableBalance(
                CreateClaimableBalanceResult::Success(id),
            )) = r
            else {
                continue;
            };
            let mut members = envelope_request(&env_xdr, passphrase);
            members["op_index"] = Value::from(k);
            out.push(check(
                format!("{variant}/balance_id {i}.{k}"),
                "balance_id",
                members,
                encode(id).map(|b| hex_expect("balance_id", &b)),
                format!(
                    "{file}, envelope {i}: the id CreateClaimableBalance operation {k} returned"
                ),
            ));
        }
        if !succeeded(&a.result.result.result) {
            continue;
        }
        let changes = operation_changes(a.meta);
        for (k, op) in tx.operations.iter().enumerate() {
            let OperationBody::InvokeHostFunction(ihf) = &op.body else {
                continue;
            };
            let preimage = match &ihf.host_function {
                HostFunction::CreateContract(args) => &args.contract_id_preimage,
                HostFunction::CreateContractV2(args) => &args.contract_id_preimage,
                _ => continue,
            };
            let expect = match (derive::contract_id(&network, preimage), encode(preimage)) {
                (Ok(id), Ok(_)) => {
                    let created = changes
                        .get(k)
                        .is_some_and(|c| c.iter().any(|ch| created_instance(ch) == Some(&id)));
                    if created {
                        Ok(hex_expect("contract_id", &id))
                    } else {
                        Err(format!(
                            "operation {k} created no contract instance at the derived id {}",
                            hex::encode(id)
                        ))
                    }
                }
                (Err(e), _) => Err(format!("the preimage does not encode: {e}")),
                (_, Err(e)) => Err(format!("the preimage {e}")),
            };
            let preimage_xdr = encode(preimage).unwrap_or_default();
            out.push(check(
                format!("{variant}/contract_id {i}.{k}"),
                "contract_id",
                json!({
                    "preimage": B64.encode(&preimage_xdr),
                    "network_passphrase": passphrase,
                    "depth_limit": shapekit_manifest::decode_depth("ContractIdPreimage"),
                    "len_limit": preimage_xdr.len(),
                }),
                expect,
                format!("{file}, envelope {i}: the contract instance operation {k} created"),
            ));
        }
    }
    out
}

/// Whether an entry is the network's own output, so its ledger can anchor expected answers.
pub fn anchored(entry: &Entry) -> bool {
    !entry.synthetic
        && entry.expected.decode == shapekit_manifest::DecodeExpectation::MustAccept
        && entry.expected.validity == shapekit_manifest::Validity::Valid
}

/// The ledger an envelope entry was applied in: its chain's ledger `ledger_seq`, from the
/// chain committed under `corpus/chains/<chain>/<variant>`.
pub fn chain_ledger(
    corpus: &Path,
    chain: &str,
    variant: &str,
    seq: u32,
) -> Result<LedgerCloseMeta, String> {
    let dir = corpus.join("chains").join(chain).join(variant);
    let file =
        shapekit_pack::ChainFile::read(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let lake = shapekit_pack::chain::open_lake(&dir, &file)
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    for raw in lake.ledgers() {
        let raw = raw.map_err(|e| format!("{}: {e}", dir.display()))?;
        if raw.seq == seq {
            return LedgerCloseMeta::from_xdr(
                &raw.xdr,
                decode_limits("LedgerCloseMeta", raw.xdr.len()),
            )
            .map_err(|e| format!("ledger {seq} of {}: {e}", dir.display()));
        }
    }
    Err(format!("{} has no ledger {seq}", dir.display()))
}

/// The hash checks of an envelope entry applied in a committed chain.
pub fn envelope_checks(
    variant: &str,
    passphrase: &str,
    env: &TransactionEnvelope,
    ledger: Result<LedgerCloseMeta, String>,
    where_: &str,
) -> Vec<Check> {
    let mut out = Vec::new();
    match ledger {
        Ok(lcm) => {
            let (_, applied) = parts(&lcm);
            let by_hash = result_index(&applied);
            hash_checks(
                variant,
                "",
                env,
                &by_hash,
                &Net {
                    id: derive::network_id(passphrase),
                    passphrase,
                },
                where_,
                &mut out,
            );
        }
        Err(e) => out.push(check(
            format!("{variant}/tx_hash"),
            "tx_hash",
            json!({}),
            Err(format!("the chain ledger cannot be read: {e}")),
            where_.to_string(),
        )),
    }
    out
}

pub fn decode_envelope(bytes: &[u8]) -> Result<TransactionEnvelope, stellar_xdr::Error> {
    TransactionEnvelope::from_xdr(bytes, decode_limits("TransactionEnvelope", bytes.len()))
}

pub fn decode_ledger(bytes: &[u8]) -> Result<LedgerCloseMeta, stellar_xdr::Error> {
    LedgerCloseMeta::from_xdr(bytes, decode_limits("LedgerCloseMeta", bytes.len()))
}
