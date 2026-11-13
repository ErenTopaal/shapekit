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

pub fn harness_report(bytes: &[u8], path: &Path) -> Result<Report> {
    serde_json::from_slice(bytes).with_context(|| format!("parsing {}", path.display()))
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
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
