//! One shape for every `LedgerCloseMeta` and `TransactionMeta` version.
//!
//! Where ledger entry changes and events live, and the order core applies them in
//! (`LedgerManagerImpl::applyLedger` at v30.0.0):
//!
//! 1. `txProcessing[i].feeProcessing` for every transaction, in apply order, before any
//!    transaction runs (`processFeesSeqNums`).
//! 2. For each transaction in apply order, `txApplyProcessing`: v0 has only operation
//!    changes; v1 `txChanges` then operations; v2 to v4 `txChangesBefore`, operations,
//!    `txChangesAfter`. v3 adds `sorobanMeta.events` (contract events of the single host
//!    function op); v4 moves those into `operations[i].events` and adds transaction-level
//!    `events` with a stage (fee charge before all transactions, refund after the
//!    transaction or after all of them).
//! 3. `txProcessing[i].postTxApplyFeeProcessing` (meta v2 ledgers only) for every
//!    transaction, after every transaction applied (`processPostTxSetApply`): Soroban
//!    refunds from protocol 23 on.
//! 4. `upgradesProcessing`, in order.
//! 5. `evictedKeys` (v1 and v2), produced while sealing the ledger: temporary entries and
//!    their TTLs are deleted, persistent ones move to the hot archive.
//!
//! Diagnostic events (v3 `sorobanMeta.diagnosticEvents`, v4 `diagnosticEvents`) are not
//! hashed into the ledger and are not facts.

use std::collections::BTreeMap;

use stellar_xdr::{
    ContractEvent, GeneralizedTransactionSet, LedgerCloseMeta, LedgerEntryChanges, LedgerHeader,
    LedgerKey, TransactionEnvelope, TransactionEventStage, TransactionMeta, TransactionPhase,
    TransactionResultPair, TxSetComponent, UpgradeEntryMeta,
};

use crate::network::Network;

/// Where a change list or event sits in the ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scope {
    FeeProcessing,
    TxBefore,
    Operation(u32),
    TxAfter,
    PostApplyFee,
    Upgrade(u32),
}

/// When a transaction-level event happened (meta v4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventStage {
    BeforeAllTxs,
    AfterTx,
    AfterAllTxs,
    Operation(u32),
}

impl From<TransactionEventStage> for EventStage {
    fn from(s: TransactionEventStage) -> Self {
        match s {
            TransactionEventStage::BeforeAllTxs => EventStage::BeforeAllTxs,
            TransactionEventStage::AfterTx => EventStage::AfterTx,
            TransactionEventStage::AfterAllTxs => EventStage::AfterAllTxs,
        }
    }
}

pub struct LedgerView<'a> {
    pub header: &'a LedgerHeader,
    pub hash: [u8; 32],
    pub lcm_version: u32,
    pub txs: Vec<TxView<'a>>,
    pub upgrades: &'a [UpgradeEntryMeta],
    pub evicted: &'a [LedgerKey],
}

pub struct TxView<'a> {
    /// Position in apply order, from 0.
    pub index: u32,
    pub result: &'a TransactionResultPair,
    pub fee_processing: &'a LedgerEntryChanges,
    pub meta: &'a TransactionMeta,
    pub post_apply_fee: Option<&'a LedgerEntryChanges>,
    pub envelope: &'a TransactionEnvelope,
    /// In a parallel Soroban phase (protocol 23 on). Core commits the `txChangesBefore` of
    /// every transaction in such a phase before any of them runs its operations
    /// (`GlobalParallelApplyLedgerState`, ParallelApplyUtils.cpp), and merges read-only TTL
    /// bumps of different transactions with max (CAP-63).
    pub parallel: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ViewError {
    #[error("ledger {seq}: hashing a transaction envelope: {source}")]
    Hash {
        seq: u32,
        source: stellar_xdr::Error,
    },
    #[error(
        "ledger {seq}: transaction {hash} in txProcessing has no envelope in the transaction set (wrong network passphrase?)"
    )]
    MissingEnvelope { seq: u32, hash: String },
    #[error("ledger {seq}: the transaction set holds {set} envelopes but txProcessing has {meta}")]
    CountMismatch { seq: u32, set: usize, meta: usize },
}

impl<'a> LedgerView<'a> {
    /// Pairs every `txProcessing` record with its envelope by transaction hash, which needs
    /// the network the ledger was closed on.
    pub fn new(lcm: &'a LedgerCloseMeta, network: &Network) -> Result<Self, ViewError> {
        let (hentry, envelopes, records, upgrades, evicted, lcm_version): Parts<'a> = match lcm {
            LedgerCloseMeta::V0(m) => (
                &m.ledger_header,
                m.tx_set.txs.iter().map(|e| (e, false)).collect(),
                m.tx_processing
                    .iter()
                    .map(|r| Record {
                        result: &r.result,
                        fee: &r.fee_processing,
                        meta: &r.tx_apply_processing,
                        post: None,
                    })
                    .collect(),
                &m.upgrades_processing,
                &[],
                0,
            ),
            LedgerCloseMeta::V1(m) => (
                &m.ledger_header,
                generalized_envelopes(&m.tx_set),
                m.tx_processing
                    .iter()
                    .map(|r| Record {
                        result: &r.result,
                        fee: &r.fee_processing,
                        meta: &r.tx_apply_processing,
                        post: None,
                    })
                    .collect(),
                &m.upgrades_processing,
                &m.evicted_keys,
                1,
            ),
            LedgerCloseMeta::V2(m) => (
                &m.ledger_header,
                generalized_envelopes(&m.tx_set),
                m.tx_processing
                    .iter()
                    .map(|r| Record {
                        result: &r.result,
                        fee: &r.fee_processing,
                        meta: &r.tx_apply_processing,
                        post: Some(&r.post_tx_apply_fee_processing),
                    })
                    .collect(),
                &m.upgrades_processing,
                &m.evicted_keys,
                2,
            ),
        };
        let seq = hentry.header.ledger_seq;
        if envelopes.len() != records.len() {
            return Err(ViewError::CountMismatch {
                seq,
                set: envelopes.len(),
                meta: records.len(),
            });
        }
        let mut by_hash = BTreeMap::new();
        for (env, parallel) in envelopes {
            let h = env
                .hash(network.id)
                .map_err(|source| ViewError::Hash { seq, source })?;
            by_hash.insert(h, (env, parallel));
        }
        let mut txs = Vec::with_capacity(records.len());
        for (i, r) in records.into_iter().enumerate() {
            let (envelope, parallel) = by_hash
                .get(&r.result.transaction_hash.0)
                .copied()
                .ok_or_else(|| ViewError::MissingEnvelope {
                    seq,
                    hash: hex::encode(r.result.transaction_hash.0),
                })?;
            txs.push(TxView {
                index: i as u32,
                result: r.result,
                fee_processing: r.fee,
                meta: r.meta,
                post_apply_fee: r.post,
                envelope,
                parallel,
            });
        }
        Ok(LedgerView {
            header: &hentry.header,
            hash: hentry.hash.0,
            lcm_version,
            txs,
            upgrades,
            evicted,
        })
    }

    pub fn seq(&self) -> u32 {
        self.header.ledger_seq
    }
}

pub fn ledger_seq(lcm: &LedgerCloseMeta) -> u32 {
    match lcm {
        LedgerCloseMeta::V0(m) => m.ledger_header.header.ledger_seq,
        LedgerCloseMeta::V1(m) => m.ledger_header.header.ledger_seq,
        LedgerCloseMeta::V2(m) => m.ledger_header.header.ledger_seq,
    }
}

type Parts<'a> = (
    &'a stellar_xdr::LedgerHeaderHistoryEntry,
    Vec<(&'a TransactionEnvelope, bool)>,
    Vec<Record<'a>>,
    &'a [UpgradeEntryMeta],
    &'a [LedgerKey],
    u32,
);

struct Record<'a> {
    result: &'a TransactionResultPair,
    fee: &'a LedgerEntryChanges,
    meta: &'a TransactionMeta,
    post: Option<&'a LedgerEntryChanges>,
}

fn generalized_envelopes(set: &GeneralizedTransactionSet) -> Vec<(&TransactionEnvelope, bool)> {
    let GeneralizedTransactionSet::V1(v1) = set;
    let mut out = Vec::new();
    for phase in v1.phases.iter() {
        match phase {
            TransactionPhase::V0(components) => {
                for c in components.iter() {
                    let TxSetComponent::TxsetCompTxsMaybeDiscountedFee(c) = c;
                    out.extend(c.txs.iter().map(|e| (e, false)));
                }
            }
            TransactionPhase::V1(par) => {
                for stage in par.execution_stages.iter() {
                    for cluster in stage.0.iter() {
                        out.extend(cluster.0.iter().map(|e| (e, true)));
                    }
                }
            }
        }
    }
    out
}

/// One entry-change list of a transaction, with where it sits.
pub struct ChangeList<'a> {
    pub scope: Scope,
    pub changes: &'a LedgerEntryChanges,
}

/// One consensus event (contract or system type, never diagnostic) with its stage.
pub struct EventRef<'a> {
    pub stage: EventStage,
    pub event: &'a ContractEvent,
}

impl<'a> TxView<'a> {
    /// The transaction's change lists in the order core applied them. Fee processing and
    /// post-apply fee processing are included here, but in the ledger they run in separate
    /// passes over all transactions (see the module docs); `LedgerState` applies them in
    /// that ledger order.
    /// The apply lists without `txChangesBefore`, for the parallel-phase order.
    pub fn apply_lists_after_before(&self) -> Vec<ChangeList<'a>> {
        self.apply_lists()
            .into_iter()
            .filter(|l| l.scope != Scope::TxBefore)
            .collect()
    }

    pub fn tx_changes_before(&self) -> Option<&'a LedgerEntryChanges> {
        match self.meta {
            TransactionMeta::V0(_) => None,
            TransactionMeta::V1(m) => Some(&m.tx_changes),
            TransactionMeta::V2(m) => Some(&m.tx_changes_before),
            TransactionMeta::V3(m) => Some(&m.tx_changes_before),
            TransactionMeta::V4(m) => Some(&m.tx_changes_before),
        }
    }

    pub fn apply_lists(&self) -> Vec<ChangeList<'a>> {
        let mut out = Vec::new();
        match self.meta {
            TransactionMeta::V0(ops) => {
                for (i, op) in ops.iter().enumerate() {
                    out.push(ChangeList {
                        scope: Scope::Operation(i as u32),
                        changes: &op.changes,
                    });
                }
            }
            TransactionMeta::V1(m) => {
                out.push(ChangeList {
                    scope: Scope::TxBefore,
                    changes: &m.tx_changes,
                });
                for (i, op) in m.operations.iter().enumerate() {
                    out.push(ChangeList {
                        scope: Scope::Operation(i as u32),
                        changes: &op.changes,
                    });
                }
            }
            TransactionMeta::V2(m) => {
                out.push(ChangeList {
                    scope: Scope::TxBefore,
                    changes: &m.tx_changes_before,
                });
                for (i, op) in m.operations.iter().enumerate() {
                    out.push(ChangeList {
                        scope: Scope::Operation(i as u32),
                        changes: &op.changes,
                    });
                }
                out.push(ChangeList {
                    scope: Scope::TxAfter,
                    changes: &m.tx_changes_after,
                });
            }
            TransactionMeta::V3(m) => {
                out.push(ChangeList {
                    scope: Scope::TxBefore,
                    changes: &m.tx_changes_before,
                });
                for (i, op) in m.operations.iter().enumerate() {
                    out.push(ChangeList {
                        scope: Scope::Operation(i as u32),
                        changes: &op.changes,
                    });
                }
                out.push(ChangeList {
                    scope: Scope::TxAfter,
                    changes: &m.tx_changes_after,
                });
            }
            TransactionMeta::V4(m) => {
                out.push(ChangeList {
                    scope: Scope::TxBefore,
                    changes: &m.tx_changes_before,
                });
                for (i, op) in m.operations.iter().enumerate() {
                    out.push(ChangeList {
                        scope: Scope::Operation(i as u32),
                        changes: &op.changes,
                    });
                }
                out.push(ChangeList {
                    scope: Scope::TxAfter,
                    changes: &m.tx_changes_after,
                });
            }
        }
        out
    }

    /// Every change list of the transaction: fee processing, apply lists, post-apply fee.
    pub fn all_lists(&self) -> Vec<ChangeList<'a>> {
        let mut out = vec![ChangeList {
            scope: Scope::FeeProcessing,
            changes: self.fee_processing,
        }];
        out.extend(self.apply_lists());
        if let Some(p) = self.post_apply_fee {
            out.push(ChangeList {
                scope: Scope::PostApplyFee,
                changes: p,
            });
        }
        out
    }

    /// Consensus events: v3 Soroban events belong to the single operation; v4 has
    /// per-operation events and staged transaction events.
    pub fn events(&self) -> Vec<EventRef<'a>> {
        let mut out = Vec::new();
        match self.meta {
            TransactionMeta::V3(m) => {
                if let Some(s) = &m.soroban_meta {
                    for e in s.events.iter() {
                        out.push(EventRef {
                            stage: EventStage::Operation(0),
                            event: e,
                        });
                    }
                }
            }
            TransactionMeta::V4(m) => {
                for e in m
                    .events
                    .iter()
                    .filter(|e| e.stage == TransactionEventStage::BeforeAllTxs)
                {
                    out.push(EventRef {
                        stage: e.stage.into(),
                        event: &e.event,
                    });
                }
                for (i, op) in m.operations.iter().enumerate() {
                    for e in op.events.iter() {
                        out.push(EventRef {
                            stage: EventStage::Operation(i as u32),
                            event: e,
                        });
                    }
                }
                for e in m
                    .events
                    .iter()
                    .filter(|e| e.stage != TransactionEventStage::BeforeAllTxs)
                {
                    out.push(EventRef {
                        stage: e.stage.into(),
                        event: &e.event,
                    });
                }
            }
            _ => {}
        }
        out
    }
}
