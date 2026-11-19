//! Reading the run outputs findings are collected from: harness reports, conformance
//! divergence matrices and indexerlint results.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use shapekit_harness::report::Report;

pub fn read(path: &Path) -> Result<(Vec<u8>, String)> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let sha = shapekit_manifest::sha256_hex(&bytes);
    Ok((bytes, sha))
}

/// A harness report, of a whole run: one shard of a run (`harness run --shard`) holds only some
/// chains, so it is refused until `shapekit harness merge` has put every shard together.
pub fn harness_report(bytes: &[u8], path: &Path) -> Result<Report> {
    let r: Report =
        serde_json::from_slice(bytes).with_context(|| format!("parsing {}", path.display()))?;
    if let Some(s) = &r.shard {
        anyhow::bail!(
            "{} is shard {s} of a run; put every shard together with `shapekit harness merge` first",
            path.display()
        );
    }
    Ok(r)
}

/// The parts of `divergence-matrix.json` findings need. The matrix's check records hold
/// deeply nested XDR-JSON, so it is parsed without serde_json's recursion limit, on a thread
/// with a stack large enough for that (see [`crate::parse_on_big_stack`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatrixView {
    pub corpus_version: String,
    pub testees: Vec<TesteeView>,
    pub entries: Vec<MatrixEntry>,
    pub known_failure_problems: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TesteeView {
    pub key: String,
    pub sdk: String,
    pub package: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatrixEntry {
    pub id: String,
    pub cells: BTreeMap<String, CellView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellView {
    pub result: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub known_issue: Option<String>,
    #[serde(default)]
    pub checks: Vec<CheckView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckView {
    pub check: String,
    pub op: String,
    pub result: String,
    /// The kind of failure beyond the result (`panic`, `timeout`, ...), where the matrix
    /// records one; see `CheckView::outcome`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// What the SDK answered. Kept only for the checks of an implementation-defined entry,
    /// where the answer is recorded rather than judged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual: Option<serde_json::Value>,
}

/// How the conformance runner starts the detail of a check of an implementation-defined
/// entry (`decode = implementation-defined`): no answer is required, so the answer is
/// recorded in `actual` and the check passes. MATRIX.md lists these checks under
/// "Implementation-defined answers".
pub const IMPLEMENTATION_DEFINED: &str = "implementation-defined";

impl CheckView {
    /// The result with its kind of failure, as a known-failures line writes it: `<result>` or
    /// `<result>:<kind>`.
    pub fn outcome(&self) -> String {
        match &self.failure {
            None => self.result.clone(),
            Some(k) => format!("{}:{k}", self.result),
        }
    }

    /// A check of an implementation-defined entry that recorded an answer.
    pub fn is_implementation_defined(&self) -> bool {
        self.result == "pass"
            && self.actual.is_some()
            && self
                .detail
                .as_deref()
                .is_some_and(|d| d.starts_with(IMPLEMENTATION_DEFINED))
    }

    /// A check a run has to account for, as [`CellView::is_failure`] for a cell.
    pub fn is_failure(&self) -> bool {
        !matches!(self.result.as_str(), "pass" | "unsupported")
    }

    /// The recorded answer in one line: a string answer (`rejected`) as itself, anything else
    /// as compact JSON. Empty when no answer was recorded.
    pub fn answer(&self) -> String {
        match &self.actual {
            None => String::new(),
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
        }
    }
}

impl CellView {
    /// Results a run has to account for: everything but a pass and an entry the version
    /// cannot express.
    pub fn is_failure(&self) -> bool {
        !matches!(self.result.as_str(), "pass" | "unsupported")
    }
}

pub fn matrix(bytes: &[u8], path: &Path) -> Result<MatrixView> {
    let mut de = serde_json::Deserializer::from_slice(bytes);
    de.disable_recursion_limit();
    let m =
        MatrixView::deserialize(&mut de).with_context(|| format!("parsing {}", path.display()))?;
    de.end()
        .with_context(|| format!("trailing data in {}", path.display()))?;
    Ok(m)
}

/// `indexerlint -format json` output (lint/cmd/indexerlint).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LintResult {
    pub findings: Vec<LintFinding>,
    #[serde(default)]
    pub suppressed: Vec<serde_json::Value>,
    #[serde(default)]
    pub baselined: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LintFinding {
    pub rule: String,
    pub package: String,
    pub function: String,
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub message: String,
}

pub fn lint(bytes: &[u8], path: &Path) -> Result<LintResult> {
    serde_json::from_slice(bytes).with_context(|| format!("parsing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shard_report_is_not_an_input() {
        // any harness input of the findings
        let dir = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../findings/inputs"
        ));
        let mut inputs: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("harness-")
            })
            .collect();
        inputs.sort();
        let path = inputs.first().unwrap().as_path();
        let bytes = std::fs::read(path).unwrap();
        assert!(harness_report(&bytes, path).is_ok());
        let mut v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        v["shard"] = "1/3".into();
        let err = harness_report(&serde_json::to_vec(&v).unwrap(), path).unwrap_err();
        assert!(format!("{err}").contains("shard 1/3"), "{err}");
    }
}
