//! The corpus manifest, `corpus/manifest.json`. `schema/manifest.schema.json` is generated
//! from these types by `shapekit manifest schema`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use shapekit_registry::SourceClass;

/// The only manifest format this build reads and writes.
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

pub const TESTNET_PASSPHRASE: &str = "Test SDF Network ; September 2015";
pub const MAINNET_PASSPHRASE: &str = "Public Global Stellar Network ; September 2015";
/// The flag of a contrast entry: bytes from a protocol before its family's shape existed.
/// Such an entry's `protocol.valid_max` ends before `protocol.introduced`.
pub const BELOW_PROTOCOL_VALID: &str = "below-protocol-valid";

/// The corpus manifest: a header and every entry, sorted by id. All file paths are relative
/// to the directory holding `manifest.json` and use forward slashes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub corpus_version: String,
    pub generated_with: GeneratedWith,
    /// Every protocol some entry was generated at, ascending.
    pub protocol_tiers: Vec<u32>,
    pub entries: Vec<Entry>,
}

/// The exact upstream stack the corpus was produced with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GeneratedWith {
    /// Version of the `stellar-xdr` crate that decoded and checked every entry.
    pub stellar_xdr: String,
    pub xdr_definitions: XdrDefinitions,
    pub core_image: CoreImage,
    pub soroban_env_host: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct XdrDefinitions {
    pub tag: String,
    pub commit: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CoreImage {
    pub repository: String,
    pub tag: String,
    pub digest: String,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum EntryKind {
    Xdr,
    Ordering,
    Strkey,
    TextualId,
    NativeProjection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// `<family>.<slug>`.
    pub id: String,
    pub kind: EntryKind,
    pub family: String,
    pub caps: Vec<String>,
    pub protocol: EntryProtocol,
    /// The `stellar-xdr` type name of every variant's bytes. Required for `kind = xdr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xdr_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lcm_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tx_meta_version: Option<u32>,
    pub source: Source,
    pub synthetic: bool,
    pub passphrase_dependent: bool,
    /// The bytes per network. Only `kind = xdr` entries have variants.
    #[serde(default, skip_serializing_if = "Variants::is_empty")]
    pub variants: Variants,
    /// The vector file of a non-xdr entry. Filled in by `shapekit manifest build`, never
    /// written inside the vector file itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vector: Option<FileRef>,
    pub expected: Expected,
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub fields_exercised: Vec<String>,
    #[serde(default)]
    pub linked_issues: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EntryProtocol {
    /// The protocol that introduced the shape.
    pub introduced: u32,
    /// The protocol the bytes were produced at; absent for vectors that have no protocol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<u32>,
    /// The lowest protocol at which these bytes are a valid ledger value: the family's
    /// `protocol_valid.min`, or for a `below-protocol-valid` entry the protocol it was
    /// recorded at.
    pub valid_min: u32,
    /// The highest protocol at which these bytes are a valid ledger value, absent when the
    /// shape is still valid at the current protocol. A `below-protocol-valid` entry always has
    /// one, the last protocol before its family's shape existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_max: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Source {
    #[schemars(with = "SourceClassSchema")]
    pub class: SourceClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ledger_seq: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tx_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tx_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream: Option<Upstream>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_url: Option<String>,
    /// How the stored bytes were reordered from what core wrote. Set on local-core entries
    /// that carry meta, whose change lists core emits in a per-process order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalization: Option<Normalization>,
    /// What core answered when the bytes were submitted, for envelopes core refused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core_rejection: Option<CoreRejection>,
}

/// Ordering rules applied to recorded meta before it is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Normalization {
    /// Every change list stably sorted by ledger key: the order between keys is fixed, and
    /// each key's changes keep the order core emitted them in. Only the order between keys
    /// changes, which core leaves to a per-process hash map, so the stored bytes are an
    /// order core itself can emit and the bytes core wrote are a permutation of them.
    KeyStable,
}

/// Core's refusal of an envelope: where it happened and the `TransactionResult` it returned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CoreRejection {
    pub stage: RejectionStage,
    /// The XDR-JSON arm name of the result, for example `tx_malformed` or `tx_failed`.
    pub result_code: String,
    /// The `TransactionResult` core returned, base64 XDR.
    pub result_xdr: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RejectionStage {
    /// Refused by the `tx` endpoint, so the envelope never reached a ledger.
    Submit,
    /// Included in a ledger and failed there.
    Apply,
}

// mirrors shapekit_registry::SourceClass for the schema, which the registry crate does not
// generate; a test checks the two agree
#[derive(JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[allow(dead_code)]
enum SourceClassSchema {
    CoreGolden,
    LocalCore,
    TestnetHistory,
    MainnetHistory,
    Synthetic,
}

/// A file taken from an upstream repository, such as a core golden.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Upstream {
    pub repo: String,
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Variants {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub testnet: Option<Variant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mainnet: Option<Variant>,
    /// Bytes that do not depend on a network passphrase, or that were produced under a
    /// passphrase that is neither testnet nor mainnet (core goldens).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub any: Option<Variant>,
}

impl Variants {
    pub fn is_empty(&self) -> bool {
        self.testnet.is_none() && self.mainnet.is_none() && self.any.is_none()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &Variant)> {
        [
            ("testnet", self.testnet.as_ref()),
            ("mainnet", self.mainnet.as_ref()),
            ("any", self.any.as_ref()),
        ]
        .into_iter()
        .filter_map(|(n, v)| v.map(|v| (n, v)))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Variant {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_passphrase: Option<String>,
    pub xdr_file: String,
    pub xdr_sha256: String,
    /// SEP-51 XDR-JSON of the decoded value, as `to_xdr_json` writes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_json_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_json_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oracle_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oracle_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileRef {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Expected {
    pub validity: Validity,
    pub decode: DecodeExpectation,
    /// Whether decoding and re-encoding must give back the same bytes.
    pub roundtrip: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Validity {
    Valid,
    InvalidByCore,
    NotEmittable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DecodeExpectation {
    MustAccept,
    MustReject,
    ImplementationDefined,
}

/// A non-xdr vector file, `corpus/vectors/<kind>/<entry-id>.json`: the entry's manifest record
/// (without `vector`) and the kind-specific cases.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VectorFile {
    pub entry: Entry,
    pub cases: Vec<serde_json::Value>,
}

/// The JSON Schema of the manifest, as committed in `schema/manifest.schema.json`.
pub fn manifest_schema() -> String {
    let schema = schemars::schema_for!(Manifest);
    crate::xdr::render_json(&serde_json::to_value(schema).expect("a schema is plain json"))
}
