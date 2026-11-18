//! Serde types for the five registry files.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapsFile {
    pub schema_version: u32,
    #[serde(default, rename = "cap")]
    pub caps: Vec<Cap>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cap {
    pub id: String,
    pub title: String,
    pub status: String,
    pub protocol: Option<u32>,
    pub xdr_additions: Vec<String>,
    pub indexer_visible: bool,
    pub reason: Option<String>,
    pub source: CapSource,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapSource {
    pub path: String,
    pub xdr_diff: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldsFile {
    pub schema_version: u32,
    #[serde(default, rename = "field")]
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ValidatedByCore {
    Yes,
    No,
    Partial,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub id: String,
    pub xdr_path: String,
    pub validated_by_core: ValidatedByCore,
    pub checks: String,
    pub protocol_min: u32,
    pub protocol_max: Option<u32>,
    pub risk: String,
    pub handling: String,
    pub go_accessors: Vec<String>,
    pub citations: Vec<Citation>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Citation {
    pub repo: String,
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub path: String,
    pub symbol: String,
    // a line number, a range string or a pair; the registry does not interpret it
    pub lines: toml::Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LimitsFile {
    pub schema_version: u32,
    #[serde(default, rename = "limit")]
    pub limits: Vec<Limit>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limit {
    pub id: String,
    pub applies_to: String,
    pub recommended: i64,
    pub unit: String,
    /// The value comes from a CONFIG_SETTING ledger entry and moves with config upgrades.
    #[serde(default)]
    pub network_setting: bool,
    /// The cap holds only for ledgers at or above this protocol.
    pub protocol_min: Option<u32>,
    pub derived_from: Vec<Derivation>,
    pub rationale: String,
}

/// Where a limit's value comes from. Each form has a fixed key set, so a typo in a key fails
/// to match any form instead of being dropped.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Derivation {
    Measurement(Measurement),
    SourceCode(SourceConstant),
    NetworkSetting(NetworkSetting),
}

/// An upstream constant read from source.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceConstant {
    pub repo: String,
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub path: String,
    pub symbol: String,
    // upstream constants are numbers or expressions, kept as written
    pub value: toml::Value,
}

/// A value measured by running the cited code.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Measurement {
    pub repo: String,
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub path: String,
    pub symbol: String,
    pub value: toml::Value,
    pub measured_with: String,
}

/// A network config setting read from a live network.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkSetting {
    pub network: String,
    pub rpc: String,
    pub setting: String,
    pub value: toml::Value,
    pub read_at: String,
    pub ledger: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LintRulesFile {
    pub schema_version: u32,
    #[serde(default, rename = "rule")]
    pub rules: Vec<LintRule>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LintRule {
    pub id: String,
    pub name: String,
    pub description: String,
    pub rationale: String,
    pub checklist: String,
    pub fields_registry: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamiliesFile {
    pub schema_version: u32,
    #[serde(default, rename = "family")]
    pub families: Vec<Family>,
}

/// Where a corpus entry's bytes come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceClass {
    CoreGolden,
    LocalCore,
    /// An envelope the generator built and a local core refused at submission. The bytes
    /// never reached a ledger, so they are not core-emitted output, but core's answer is.
    LocalCoreRefused,
    TestnetHistory,
    MainnetHistory,
    Synthetic,
    /// Transaction meta core's own test suite wrote (`--record-test-tx-meta`), converted back
    /// to XDR. Core emitted the bytes, but under its test network passphrase and with change
    /// lists in the order its test harness sorts them, so they are not a ledger an indexer
    /// could have read.
    CoreTestDump,
}

impl SourceClass {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceClass::CoreGolden => "core-golden",
            SourceClass::LocalCore => "local-core",
            SourceClass::LocalCoreRefused => "local-core-refused",
            SourceClass::TestnetHistory => "testnet-history",
            SourceClass::MainnetHistory => "mainnet-history",
            SourceClass::Synthetic => "synthetic",
            SourceClass::CoreTestDump => "core-test-dump",
        }
    }

    /// Made by a recipe against a local core, whether core accepted it or not.
    pub fn is_local(self) -> bool {
        matches!(self, SourceClass::LocalCore | SourceClass::LocalCoreRefused)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolRange {
    pub min: u32,
    pub max: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Family {
    pub id: String,
    pub title: String,
    pub caps: Vec<String>,
    pub protocol_introduced: u32,
    pub protocol_valid: ProtocolRange,
    pub signature: Vec<String>,
    pub source_classes: Vec<SourceClass>,
    pub fields: Vec<String>,
    pub linked_issues: Vec<String>,
    pub priority: u32,
    pub notes: Option<String>,
    /// Why the family belongs to no CAP, for shapes that come from the XDR definitions
    /// themselves rather than from a protocol change. Required exactly when `caps` is empty.
    #[serde(default)]
    pub no_cap_reason: Option<String>,
    /// Each entry of the family records the union arms it witnesses (`arms`) and carries the
    /// CAPs that introduced them, for families whose entries mix shapes of many CAPs.
    #[serde(default)]
    pub caps_from_arms: bool,
}
