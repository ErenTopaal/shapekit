//! Picks the bytes of one capture out of a ledger's canonical meta: the
//! whole ledger, one transaction, or the last state of one ledger entry.

use anyhow::{Context, Result, bail, ensure};
use shapekit_core_driver::chain::Applied;
use shapekit_core_driver::tx::envelope_hash;
use stellar_xdr::{
    GeneralizedTransactionSet, LedgerCloseMeta, LedgerEntry, LedgerEntryChange, LedgerEntryChanges,
    LedgerKey, Limits, TransactionEnvelope, TransactionMeta, TransactionPhase, TxSetComponent,
    WriteXdr,
};

/// What a capture takes from the recorded meta.
#[derive(Debug, Clone)]
pub enum Selector {
    /// The whole `LedgerCloseMeta` of a ledger.
    Ledger(u32),
    /// One transaction, as two entries: `<slug>-envelope`, its
    /// `TransactionEnvelope` from the ledger's transaction set, and
    /// `<slug>-meta`, its result and meta (`TransactionResultMeta`, or
    /// `TransactionResultMetaV1` in a v2 ledger).
    Transaction { ledger: u32, hash: [u8; 32] },
    /// The `LedgerEntry` stored under `key` once `ledger` has closed, as the
    /// last change to it in the ledger's meta reports it.
    LedgerEntry { ledger: u32, key: LedgerKey },
}

impl Selector {
    /// The ledger `applied` closed in.
    pub fn ledger_of(applied: &Applied) -> Self {
        Self::Ledger(applied.ledger)
    }

    /// The transaction `applied` describes.
    pub fn transaction(applied: &Applied) -> Self {
        Self::Transaction {
            ledger: applied.ledger,
            hash: applied.hash,
        }
    }

    pub fn ledger(&self) -> u32 {
        match self {
            Self::Ledger(l) => *l,
            Self::Transaction { ledger, .. } | Self::LedgerEntry { ledger, .. } => *ledger,
        }
    }
}

/// One selected value, ready to become an entry variant.
#[derive(Debug, Clone)]
pub struct Selected {
    /// Appended to the capture's slug, empty or `-envelope` / `-meta`.
    pub suffix: &'static str,
    pub xdr_type: &'static str,
    pub bytes: Vec<u8>,
    pub lcm_version: Option<u32>,
    pub tx_meta_version: Option<u32>,
    pub tx_index: Option<u32>,
    pub tx_hash: Option<String>,
    /// Whether the bytes hold change lists in canonical order.
    pub normalized: bool,
}

pub fn lcm_version(lcm: &LedgerCloseMeta) -> u32 {
    match lcm {
        LedgerCloseMeta::V0(_) => 0,
        LedgerCloseMeta::V1(_) => 1,
        LedgerCloseMeta::V2(_) => 2,
    }
}

pub fn tx_meta_version(meta: &TransactionMeta) -> u32 {
    match meta {
        TransactionMeta::V0(_) => 0,
        TransactionMeta::V1(_) => 1,
        TransactionMeta::V2(_) => 2,
        TransactionMeta::V3(_) => 3,
        TransactionMeta::V4(_) => 4,
    }
}

fn tx_metas(lcm: &LedgerCloseMeta) -> Vec<&TransactionMeta> {
    match lcm {
        LedgerCloseMeta::V0(m) => m
            .tx_processing
            .iter()
            .map(|t| &t.tx_apply_processing)
            .collect(),
        LedgerCloseMeta::V1(m) => m
            .tx_processing
            .iter()
            .map(|t| &t.tx_apply_processing)
            .collect(),
        LedgerCloseMeta::V2(m) => m
            .tx_processing
            .iter()
            .map(|t| &t.tx_apply_processing)
            .collect(),
    }
}

fn tx_hashes(lcm: &LedgerCloseMeta) -> Vec<[u8; 32]> {
    match lcm {
        LedgerCloseMeta::V0(m) => m
            .tx_processing
            .iter()
            .map(|t| t.result.transaction_hash.0)
            .collect(),
        LedgerCloseMeta::V1(m) => m
            .tx_processing
            .iter()
            .map(|t| t.result.transaction_hash.0)
            .collect(),
        LedgerCloseMeta::V2(m) => m
            .tx_processing
            .iter()
            .map(|t| t.result.transaction_hash.0)
            .collect(),
    }
}

fn generalized(set: &GeneralizedTransactionSet) -> Vec<&TransactionEnvelope> {
    let GeneralizedTransactionSet::V1(v1) = set;
    let mut out = Vec::new();
    for phase in v1.phases.iter() {
        match phase {
            TransactionPhase::V0(components) => {
                for c in components.iter() {
                    let TxSetComponent::TxsetCompTxsMaybeDiscountedFee(c) = c;
                    out.extend(c.txs.iter());
                }
            }
            TransactionPhase::V1(parallel) => {
                for stage in parallel.execution_stages.iter() {
                    for cluster in stage.iter() {
                        out.extend(cluster.0.iter());
                    }
                }
            }
        }
    }
    out
}

fn envelopes(lcm: &LedgerCloseMeta) -> Vec<&TransactionEnvelope> {
    match lcm {
        LedgerCloseMeta::V0(m) => m.tx_set.txs.iter().collect(),
        LedgerCloseMeta::V1(m) => generalized(&m.tx_set),
        LedgerCloseMeta::V2(m) => generalized(&m.tx_set),
    }
}

/// The envelope with transaction hash `hash` in the ledger's transaction set.
fn find_envelope<'a>(
    lcm: &'a LedgerCloseMeta,
    hash: &[u8; 32],
    network_id: &[u8; 32],
) -> Result<&'a TransactionEnvelope> {
    let mut found = None;
    for env in envelopes(lcm) {
        if envelope_hash(env, network_id)? == *hash {
            ensure!(
                found.is_none(),
                "two envelopes hash to {}",
                hex::encode(hash)
            );
            found = Some(env);
        }
    }
    found.with_context(|| format!("no envelope in the tx set hashes to {}", hex::encode(hash)))
}

/// The ledger's change lists in the order core applied them: every
/// transaction's fee processing, then each transaction's own changes
/// (before, per operation, after) and post-apply fee processing, then the
/// upgrades.
fn change_lists(lcm: &LedgerCloseMeta) -> Vec<&LedgerEntryChanges> {
    fn tx_lists<'a>(meta: &'a TransactionMeta, out: &mut Vec<&'a LedgerEntryChanges>) {
        match meta {
            TransactionMeta::V0(ops) => out.extend(ops.iter().map(|o| &o.changes)),
            TransactionMeta::V1(m) => {
                out.push(&m.tx_changes);
                out.extend(m.operations.iter().map(|o| &o.changes));
            }
            TransactionMeta::V2(m) => {
                out.push(&m.tx_changes_before);
                out.extend(m.operations.iter().map(|o| &o.changes));
                out.push(&m.tx_changes_after);
            }
            TransactionMeta::V3(m) => {
                out.push(&m.tx_changes_before);
                out.extend(m.operations.iter().map(|o| &o.changes));
                out.push(&m.tx_changes_after);
            }
            TransactionMeta::V4(m) => {
                out.push(&m.tx_changes_before);
                out.extend(m.operations.iter().map(|o| &o.changes));
                out.push(&m.tx_changes_after);
            }
        }
    }
    let mut out = Vec::new();
    match lcm {
        LedgerCloseMeta::V0(m) => {
            out.extend(m.tx_processing.iter().map(|t| &t.fee_processing));
            for t in m.tx_processing.iter() {
                tx_lists(&t.tx_apply_processing, &mut out);
            }
            out.extend(m.upgrades_processing.iter().map(|u| &u.changes));
        }
        LedgerCloseMeta::V1(m) => {
            out.extend(m.tx_processing.iter().map(|t| &t.fee_processing));
            for t in m.tx_processing.iter() {
                tx_lists(&t.tx_apply_processing, &mut out);
            }
            out.extend(m.upgrades_processing.iter().map(|u| &u.changes));
        }
        LedgerCloseMeta::V2(m) => {
            out.extend(m.tx_processing.iter().map(|t| &t.fee_processing));
            for t in m.tx_processing.iter() {
                tx_lists(&t.tx_apply_processing, &mut out);
                out.push(&t.post_tx_apply_fee_processing);
            }
            out.extend(m.upgrades_processing.iter().map(|u| &u.changes));
        }
    }
    out
}

/// The entry under `key` after the ledger: the last created, updated or
/// restored state the meta reports for it.
fn final_entry(lcm: &LedgerCloseMeta, key: &LedgerKey) -> Result<LedgerEntry> {
    let mut last: Option<Option<&LedgerEntry>> = None;
    for list in change_lists(lcm) {
        for change in list.0.iter() {
            match change {
                LedgerEntryChange::Created(e)
                | LedgerEntryChange::Updated(e)
                | LedgerEntryChange::Restored(e)
                    if e.to_key() == *key =>
                {
                    last = Some(Some(e));
                }
                LedgerEntryChange::Removed(k) if k == key => last = Some(None),
                _ => {}
            }
        }
    }
    match last {
        Some(Some(e)) => Ok(e.clone()),
        Some(None) => bail!("the ledger removes the entry"),
        None => bail!("the ledger's meta does not change the entry"),
    }
}

/// Resolves `selector` against `lcm`, the canonical meta of the selected
/// ledger, with `xdr` its encoding.
pub fn select(
    selector: &Selector,
    lcm: &LedgerCloseMeta,
    xdr: &[u8],
    network_id: &[u8; 32],
) -> Result<Vec<Selected>> {
    let metas = tx_metas(lcm);
    let ledger_tx_meta = metas.first().map(|m| tx_meta_version(m));
    match selector {
        Selector::Ledger(_) => Ok(vec![Selected {
            suffix: "",
            xdr_type: "LedgerCloseMeta",
            bytes: xdr.to_vec(),
            lcm_version: Some(lcm_version(lcm)),
            tx_meta_version: ledger_tx_meta,
            tx_index: None,
            tx_hash: None,
            normalized: true,
        }]),
        Selector::Transaction { hash, .. } => {
            let index = tx_hashes(lcm)
                .iter()
                .position(|h| h == hash)
                .with_context(|| format!("transaction {} is not in the meta", hex::encode(hash)))?;
            let env = find_envelope(lcm, hash, network_id)?;
            let (meta_type, meta_bytes): (&'static str, Vec<u8>) = match lcm {
                LedgerCloseMeta::V0(m) => (
                    "TransactionResultMeta",
                    m.tx_processing[index].to_xdr(Limits::none())?,
                ),
                LedgerCloseMeta::V1(m) => (
                    "TransactionResultMeta",
                    m.tx_processing[index].to_xdr(Limits::none())?,
                ),
                LedgerCloseMeta::V2(m) => (
                    "TransactionResultMetaV1",
                    m.tx_processing[index].to_xdr(Limits::none())?,
                ),
            };
            let tx_index = Some(u32::try_from(index)?);
            let tx_hash = Some(hex::encode(hash));
            Ok(vec![
                Selected {
                    suffix: "-envelope",
                    xdr_type: "TransactionEnvelope",
                    bytes: env.to_xdr(Limits::none())?,
                    lcm_version: None,
                    tx_meta_version: None,
                    tx_index,
                    tx_hash: tx_hash.clone(),
                    normalized: false,
                },
                Selected {
                    suffix: "-meta",
                    xdr_type: meta_type,
                    bytes: meta_bytes,
                    lcm_version: None,
                    tx_meta_version: Some(tx_meta_version(metas[index])),
                    tx_index,
                    tx_hash,
                    normalized: true,
                },
            ])
        }
        Selector::LedgerEntry { key, .. } => {
            let entry =
                final_entry(lcm, key).with_context(|| format!("selecting ledger entry {key:?}"))?;
            Ok(vec![Selected {
                suffix: "",
                xdr_type: "LedgerEntry",
                bytes: entry.to_xdr(Limits::none())?,
                lcm_version: None,
                tx_meta_version: None,
                tx_index: None,
                tx_hash: None,
                // a single entry carries no change list, so no order was applied
                normalized: false,
            }])
        }
    }
}
