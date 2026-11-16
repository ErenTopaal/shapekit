//! The law book over lake objects, one decompressed object in memory at a time, summed into
//! an audit report.

use std::collections::BTreeMap;

use anyhow::{Context, Result, ensure};
use shapekit_contracts::AuditReport;
use shapekit_contracts::audit::{EnvelopeCounts, LawResult, SdkRef, Violation};
use shapekit_laws::{Checker, Law, check_encoding};
use stellar_xdr::{LedgerCloseMeta, LedgerCloseMetaBatch, LedgerHeaderHistoryEntry};

use crate::lake::{Network, Object, decompress};

/// Violations kept as examples per law; the counts are always complete.
pub const EXAMPLES_PER_LAW: usize = 20;

/// The last ledger checked: where a following window has to continue the header chain.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Tip {
    pub ledger: u32,
    /// The header hash of `ledger`, hex.
    pub hash: String,
}

/// Running state of one audit.
pub struct Audit {
    network: Network,
    checker: Checker,
    ledgers: Vec<u32>,
    /// Batch start of each encoding-law violation, which the law book reports without a
    /// ledger since it applies to a whole object.
    object_of_violation: Vec<u32>,
    pub compressed_bytes: u64,
    pub raw_bytes: u64,
    tip: Option<Tip>,
}

fn header(lcm: &LedgerCloseMeta) -> &LedgerHeaderHistoryEntry {
    match lcm {
        LedgerCloseMeta::V0(m) => &m.ledger_header,
        LedgerCloseMeta::V1(m) => &m.ledger_header,
        LedgerCloseMeta::V2(m) => &m.ledger_header,
    }
}

impl Audit {
    pub fn new(network: Network) -> Audit {
        Audit {
            network,
            checker: Checker::new(network.passphrase()),
            ledgers: Vec::new(),
            object_of_violation: Vec::new(),
            compressed_bytes: 0,
            raw_bytes: 0,
            tip: None,
        }
    }

    /// An audit that continues a window ending at `tip`: the first ledger checked is held
    /// to the header chain law against it.
    pub fn after(network: Network, tip: Tip) -> Audit {
        Audit {
            tip: Some(tip),
            ..Audit::new(network)
        }
    }

    /// The last ledger checked so far, or the one the audit continues from.
    pub fn tip(&self) -> Option<&Tip> {
        self.tip.as_ref()
    }

    pub fn ledgers(&self) -> &[u32] {
        &self.ledgers
    }

    pub fn laws(&self) -> &shapekit_laws::Report {
        &self.checker.report
    }

    /// Checks every law on one lake object. `want` is the ledger range the object's key
    /// names; the batch has to hold exactly those ledgers, in order.
    pub fn object(&mut self, object: &Object, want: (u32, u32)) -> Result<()> {
        let raw = decompress(object)?;
        self.compressed_bytes += object.bytes.len() as u64;
        self.raw_bytes += raw.len() as u64;
        let before = self.checker.report.violations.len();
        let batch = check_encoding::<LedgerCloseMetaBatch>(&raw, &mut self.checker.report)
            .with_context(|| format!("{} does not decode as LedgerCloseMetaBatch", object.key))?;
        drop(raw);
        ensure!(
            (batch.start_sequence, batch.end_sequence) == want,
            "{} holds ledgers {} to {}, its key names {} to {}",
            object.key,
            batch.start_sequence,
            batch.end_sequence,
            want.0,
            want.1
        );
        let metas = batch.ledger_close_metas.as_slice();
        ensure!(
            metas.len() as u64 == u64::from(want.1 - want.0) + 1,
            "{} holds {} ledgers for the range {} to {}",
            object.key,
            metas.len(),
            want.0,
            want.1
        );
        for (i, lcm) in metas.iter().enumerate() {
            let h = header(lcm);
            let seq = h.header.ledger_seq;
            ensure!(
                seq == want.0 + i as u32,
                "{} holds ledger {seq} at position {i}",
                object.key
            );
            if let Some(last) = self.ledgers.last()
                && *last >= seq
            {
                anyhow::bail!("ledger {seq} came after ledger {last}");
            }
            if self.ledgers.is_empty()
                && let Some(tip) = &self.tip
                && tip.ledger.checked_add(1) == Some(seq)
            {
                // the ledger before this one was checked by an earlier run; the checker
                // itself only links ledgers it saw
                let prev = hex::encode(h.header.previous_ledger_hash.0);
                let failure = (prev != tip.hash).then(|| {
                    format!(
                        "previousLedgerHash {prev} is not the hash {} of ledger {}",
                        tip.hash, tip.ledger
                    )
                });
                self.checker
                    .report
                    .record(Law::HeaderChain, Some(seq), failure);
            }
            self.checker.ledger(lcm);
            self.ledgers.push(seq);
            self.tip = Some(Tip {
                ledger: seq,
                hash: hex::encode(h.hash.0),
            });
        }
        let added = self.checker.report.violations.len() - before;
        self.object_of_violation
            .extend(std::iter::repeat_n(want.0, added));
        Ok(())
    }

    /// The report in the audit-report format.
    pub fn report(&self, seed: Option<u64>, sdk: Option<SdkRef>) -> AuditReport {
        let r = &self.checker.report;
        let mut examples: BTreeMap<Law, Vec<Violation>> = BTreeMap::new();
        for (v, object) in r.violations.iter().zip(&self.object_of_violation) {
            let e = examples.entry(v.law).or_default();
            if e.len() < EXAMPLES_PER_LAW {
                e.push(Violation {
                    ledger: v.ledger.unwrap_or(*object),
                    detail: v.detail.clone(),
                });
            }
        }
        let mut laws: Vec<LawResult> = r
            .laws
            .iter()
            .map(|(law, t)| LawResult {
                law: law.as_str().to_string(),
                checked: t.checked,
                violations: t.violated,
                examples: examples.remove(law).unwrap_or_default(),
            })
            .collect();
        laws.sort_by(|a, b| a.law.cmp(&b.law));
        let count = |k: &str| r.envelopes.get(k).copied().unwrap_or(0);
        let envelopes = EnvelopeCounts {
            v0: count("v0"),
            v1: count("v1"),
            fee_bump: count("fee-bump"),
        };
        AuditReport {
            schema_version: <AuditReport as shapekit_contracts::Contract>::VERSION,
            network: self.network.contract(),
            seed,
            ledgers: self.ledgers.clone(),
            protocols: r.protocols.keys().copied().collect(),
            transactions: r.transactions,
            // the tx set and the applied transactions are counted separately; when they
            // disagree the tx-hash law has failed and the envelope split describes neither
            envelopes: (envelopes.v0 + envelopes.v1 + envelopes.fee_bump == r.transactions)
                .then_some(envelopes),
            sdk,
            laws,
            coverage: None,
        }
    }

    /// Laws that could not be decided on some instances, with how many.
    pub fn undecided(&self) -> &BTreeMap<Law, u64> {
        &self.checker.report.undecided
    }
}
