//! Audit report: what one run of the law checks over public ledgers counted.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Contract, sorted_unique};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditReport {
    pub schema_version: u32,
    pub network: AuditNetwork,
    /// The seed the ledgers were drawn with; absent for a contiguous window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Every ledger checked, ascending, each once.
    pub ledgers: Vec<u32>,
    /// The protocol versions those ledgers were closed at, ascending.
    pub protocols: Vec<u32>,
    /// Transactions in those ledgers.
    pub transactions: u64,
    /// Envelopes by type, when the run decoded envelopes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub envelopes: Option<EnvelopeCounts>,
    /// The SDK testee the ledgers went through; absent when only the in-process checks ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sdk: Option<SdkRef>,
    /// One row per law, sorted by name.
    pub laws: Vec<LawResult>,
    /// Consumer branches the run reached beyond a baseline, when coverage was measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<CoverageDelta>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AuditNetwork {
    Pubnet,
    Testnet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EnvelopeCounts {
    pub v0: u64,
    pub v1: u64,
    pub fee_bump: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SdkRef {
    /// `rust`, `go` or `js`.
    pub sdk: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LawResult {
    /// The law's name as `shapekit laws` lists it.
    pub law: String,
    /// How many values the law was checked on.
    pub checked: u64,
    pub violations: u64,
    /// Some of the violations, at most as many as `violations`.
    #[serde(default)]
    pub examples: Vec<Violation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Violation {
    pub ledger: u32,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CoverageDelta {
    /// What the run is compared with, `corpus` for the committed corpus.
    pub baseline: String,
    /// Code ranges the baseline reaches.
    pub baseline_ranges: u64,
    /// Code ranges the baseline and this run reach together.
    pub ranges: u64,
    /// Each range the baseline does not reach, with the first ledger that reached it.
    #[serde(default)]
    pub new: Vec<NewBranch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewBranch {
    /// The range in the consumer's source, `<file>:<line>:<column>-<line>:<column>`.
    pub location: String,
    pub ledger: u32,
}

impl AuditReport {
    pub fn total_violations(&self) -> u64 {
        self.laws.iter().map(|l| l.violations).sum()
    }
}

impl Contract for AuditReport {
    const SCHEMA_FILE: &'static str = "audit-report.schema.json";
    const VERSION: u32 = 1;
    const TITLE: &'static str = "shapekit audit report";
    const DESCRIPTION: &'static str = "One audit run over public ledgers: the network, the ledgers checked, checked and violation counts per law, and consumer branches reached beyond a baseline.";

    fn schema_version(&self) -> u32 {
        self.schema_version
    }

    fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        sorted_unique("ledger", &self.ledgers, |l| *l, &mut out);
        sorted_unique("protocol", &self.protocols, |p| *p, &mut out);
        sorted_unique("law", &self.laws, |l| l.law.clone(), &mut out);
        if self.ledgers.is_empty() {
            out.push("the report checked no ledger".into());
        }
        for l in &self.laws {
            if l.violations > l.checked {
                out.push(format!(
                    "law {}: {} violations in {} checks",
                    l.law, l.violations, l.checked
                ));
            }
            if l.examples.len() as u64 > l.violations {
                out.push(format!("law {}: more examples than violations", l.law));
            }
            for e in &l.examples {
                if self.ledgers.binary_search(&e.ledger).is_err() {
                    out.push(format!(
                        "law {}: example ledger {} was not checked",
                        l.law, e.ledger
                    ));
                }
            }
        }
        if let Some(c) = &self.coverage {
            if c.ranges < c.baseline_ranges {
                out.push(format!(
                    "coverage: {} ranges together is below the baseline's {}",
                    c.ranges, c.baseline_ranges
                ));
            } else if c.ranges - c.baseline_ranges != c.new.len() as u64 {
                out.push(format!(
                    "coverage: {} new ranges listed, {} counted",
                    c.new.len(),
                    c.ranges - c.baseline_ranges
                ));
            }
            for n in &c.new {
                if self.ledgers.binary_search(&n.ledger).is_err() {
                    out.push(format!(
                        "coverage: {} names unchecked ledger {}",
                        n.location, n.ledger
                    ));
                }
            }
        }
        if let Some(e) = &self.envelopes
            && e.v0 + e.v1 + e.fee_bump != self.transactions
        {
            out.push(format!(
                "envelopes add up to {}, transactions are {}",
                e.v0 + e.v1 + e.fee_bump,
                self.transactions
            ));
        }
        out
    }
}
