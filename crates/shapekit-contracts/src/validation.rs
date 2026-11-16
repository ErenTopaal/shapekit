//! Validation verdicts, `registry/generated/validation.json`: for each envelope field, what core
//! answered when only that field changed in an otherwise valid transaction.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Contract, CoreRef, sorted_unique, witness};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ValidationTable {
    pub schema_version: u32,
    /// The core every attempt ran on.
    pub core: CoreRef,
    /// Sorted by path, each path once.
    pub fields: Vec<FieldVerdict>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FieldVerdict {
    /// The field's path from `TransactionEnvelope`, as `shapekit schema` names it.
    pub path: String,
    pub verdict: Verdict,
    /// Every value tried. A verdict rests on at least two.
    pub attempts: Vec<ProbeAttempt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    /// Core refuses or fails some value of the field: the attempts end in at least two
    /// outcomes.
    Validated,
    /// Core checks some property of the field but not all; the attempts end in at least two
    /// outcomes and the ones core accepted include values a full check would refuse.
    Partial,
    /// Every value tried ends in the same outcome. Measured, not proven: a value outside the
    /// tried ones could still be refused.
    NotValidated,
}

/// One value of the field, submitted and closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProbeAttempt {
    /// The value, in words: `memo id 2^64-1`, `sequence number plus one`.
    pub variant: String,
    pub submission: SubmitStatus,
    /// The XDR-JSON arm name of the transaction result, `tx_success` or `tx_bad_seq`; absent
    /// when core returned no result (a pending transaction left out of the ledger).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_code: Option<String>,
    /// The `TransactionResult`, base64 XDR, as core returned it at submission or wrote it in
    /// the ledger.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_xdr: Option<String>,
    pub applied: ApplyStatus,
    /// The entries holding the envelope and, when it was applied, its ledger.
    #[serde(default)]
    pub witnesses: Vec<String>,
}

/// The status core's `tx` endpoint answers.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum SubmitStatus {
    Pending,
    Duplicate,
    Error,
    TryAgainLater,
    Filtered,
    /// Core's tx endpoint could not decode the bytes and returned an exception, no result.
    Undecodable,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ApplyStatus {
    /// The transaction never reached a ledger.
    NotApplied,
    /// It reached a ledger and failed there.
    Failed,
    /// It reached a ledger and succeeded.
    Succeeded,
}

impl ProbeAttempt {
    /// What the verdict compares: submission, result and whether it applied.
    pub fn outcome(&self) -> (SubmitStatus, Option<&str>, ApplyStatus) {
        (self.submission, self.result_code.as_deref(), self.applied)
    }
}

impl Contract for ValidationTable {
    const SCHEMA_FILE: &'static str = "validation.schema.json";
    const VERSION: u32 = 1;
    const TITLE: &'static str = "shapekit validation verdicts";
    const DESCRIPTION: &'static str = "registry/generated/validation.json: for each envelope field, whether core validates it, with every value tried, core's answer to it and the entries that witness it.";

    fn schema_version(&self) -> u32 {
        self.schema_version
    }

    fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.core.problems(&mut out);
        sorted_unique("field", &self.fields, |f| f.path.clone(), &mut out);
        for f in &self.fields {
            let what = format!("field {}", f.path);
            if f.attempts.len() < 2 {
                out.push(format!("{what}: a verdict needs at least two attempts"));
            }
            let mut outcomes: Vec<_> = f.attempts.iter().map(ProbeAttempt::outcome).collect();
            outcomes.sort();
            outcomes.dedup();
            match f.verdict {
                Verdict::NotValidated if outcomes.len() > 1 => out.push(format!(
                    "{what} is not-validated, but its attempts end in {} different outcomes",
                    outcomes.len()
                )),
                Verdict::Validated | Verdict::Partial if outcomes.len() < 2 => out.push(format!(
                    "{what} is {:?}, but every attempt ends the same way",
                    f.verdict
                )),
                _ => {}
            }
            for a in &f.attempts {
                let at = format!("{what}, attempt {:?}", a.variant);
                witness::check_all(&at, &a.witnesses, &mut out);
                if a.witnesses.is_empty() {
                    out.push(format!("{at} has no witness"));
                }
                if a.submission != SubmitStatus::Pending && a.applied != ApplyStatus::NotApplied {
                    out.push(format!(
                        "{at}: core answered {:?} at submission, so it cannot have been applied",
                        a.submission
                    ));
                }
                if a.submission == SubmitStatus::Undecodable
                    && (a.result_code.is_some() || a.result_xdr.is_some())
                {
                    out.push(format!(
                        "{at}: core could not decode the bytes, so it returned no result"
                    ));
                }
                if a.applied != ApplyStatus::NotApplied && a.result_code.is_none() {
                    out.push(format!("{at} was applied but has no result code"));
                }
                if a.result_code.is_some() != a.result_xdr.is_some() {
                    out.push(format!("{at}: result_code and result_xdr come together"));
                }
            }
        }
        out
    }
}
