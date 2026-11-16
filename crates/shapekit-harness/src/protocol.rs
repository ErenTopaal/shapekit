//! The adapter protocol, version [`ADAPTER_PROTOCOL_VERSION`]: newline-delimited JSON between
//! the harness (stdin of the adapter) and an adapter (its stdout). `docs/adapter-interface.md`
//! is the prose form and `schema/adapter-protocol.schema.json` is generated from these types
//! by [`protocol_schema`].

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shapekit_oracle::vocab::{Change, Int};

/// Bumped when a message, member or rule changes meaning.
pub const ADAPTER_PROTOCOL_VERSION: u32 = 2;

/// The oldest adapter protocol the harness still drives. A version 1 adapter refuses a
/// `describe` for version 2 with an `unsupported` error; the harness then asks again for
/// version 1 and runs it without the commands version 2 added.
pub const MIN_ADAPTER_PROTOCOL_VERSION: u32 = 1;

/// One line from the harness to the adapter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Request {
    /// Chosen by the harness, unique within one adapter process. Every response line to this
    /// request carries it back.
    pub id: String,
    #[serde(flatten)]
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "cmd", rename_all = "kebab-case")]
pub enum Command {
    /// First request of every adapter process. Answered with [`DescribeResult`].
    Describe {
        /// The protocol version the harness speaks.
        protocol: u32,
    },
    /// Bring the target up against the delivery endpoints. Answered with [`StartResult`] once
    /// the target process runs; bootstrapping may still be in progress.
    Start(Box<StartParams>),
    /// Ledgers up to `head` are now visible on the replay server. Answered with
    /// [`AdvanceResult`] once the adapter has made them available in its delivery mode.
    Advance { head: u32 },
    /// Answered with [`StatusResult`].
    Status,
    /// Stream what the target stored for transactions in ledgers `from` to `to` inclusive:
    /// any number of `record` lines, then one `ok` line with [`ExportResult`].
    Export { from: u32, to: u32 },
    /// Since protocol 2, for adapters whose `describe` lists the `dump` capability: stream
    /// every row the target stored, table by table, as cell text. Any number of `table` and
    /// `row` lines, then one `ok` line with [`DumpResult`].
    Dump {
        /// Only these tables; every table when empty.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tables: Vec<String>,
    },
    /// Since protocol 2, for adapters whose `describe` lists the `restart` capability: stop
    /// the target's ingesting process and start it again on the same store, the way a
    /// deployment restarts it. Answered with [`RestartResult`] once it runs again.
    Restart,
    /// Tear the target down. Answered with [`StopResult`]; the adapter then exits.
    Stop,
}

/// Optional commands an adapter offers, listed in `describe`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    /// The adapter answers `dump` (protocol 2).
    Dump,
    /// The adapter answers `restart` (protocol 2).
    Restart,
}

/// How ledgers reach the target.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub enum DeliveryMode {
    /// The SEP-54 lake read from a directory.
    #[serde(rename = "sep54-filesystem")]
    Sep54Filesystem,
    /// The SEP-54 lake read from an S3-compatible object store the adapter runs.
    #[serde(rename = "sep54-s3")]
    Sep54S3,
    /// `getLedgers` and the other read methods of the replay server's JSON-RPC endpoint.
    #[serde(rename = "rpc")]
    Rpc,
}

impl DeliveryMode {
    pub fn name(self) -> &'static str {
        match self {
            DeliveryMode::Sep54Filesystem => "sep54-filesystem",
            DeliveryMode::Sep54S3 => "sep54-s3",
            DeliveryMode::Rpc => "rpc",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [
            DeliveryMode::Sep54Filesystem,
            DeliveryMode::Sep54S3,
            DeliveryMode::Rpc,
        ]
        .into_iter()
        .find(|m| m.name() == s)
    }
}

/// Where the target starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Bootstrap {
    /// The target reads the newest history archive checkpoint and starts ingesting at that
    /// checkpoint ledger (wallet-backend live mode on an empty database). The harness makes a
    /// checkpoint before the chain's first shape ledger the visible head before `start`.
    Checkpoint,
    /// The target can be told to start at any ledger. The harness passes the chain's first
    /// ledger as `ledgers.bootstrap`.
    Ledger,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DescribeResult {
    /// Must equal the harness's version; the harness stops otherwise.
    pub protocol: u32,
    /// Name of the adapter program.
    pub adapter: String,
    pub adapter_version: String,
    /// The indexer under test, for example `wallet-backend`.
    pub target: String,
    /// The exact target build, for example a full commit hash.
    pub target_version: String,
    /// Delivery modes this adapter can drive, at least one.
    pub delivery_modes: Vec<DeliveryMode>,
    /// The mode used when the harness is not told otherwise. Must be in `delivery_modes`.
    pub default_delivery: DeliveryMode,
    pub bootstrap: Bootstrap,
    /// The oracle vocabulary version the claims and exports are written in. Must equal the
    /// harness's `ORACLE_VOCAB_VERSION`.
    pub vocab_version: u32,
    /// Lowest and highest ledger protocol the target build can ingest. Chains outside it are
    /// reported `out-of-scope`.
    pub protocols: ProtocolRange,
    pub claims: Claims,
    /// Optional commands the adapter answers. Version 2; a version 1 adapter has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<Capability>,
}

impl DescribeResult {
    pub fn offers(&self, c: Capability) -> bool {
        self.protocol >= 2 && self.capabilities.contains(&c)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProtocolRange {
    pub min: u32,
    pub max: u32,
}

/// What the target records, in oracle vocabulary terms. Oracle facts outside the claims are
/// `out-of-scope`; facts inside them must appear in the export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Claims {
    /// Whether and which participants the target records per transaction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub participants: Option<ParticipantClaim>,
    /// State change kinds the target records, each with the fields it stores.
    pub changes: Vec<ChangeClaim>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ParticipantClaim {
    /// Address kinds recorded: `account`, `contract`, `liquidity-pool`, `claimable-balance`.
    pub address_kinds: Vec<String>,
    /// Whether the target keeps the mux ids a participant carried.
    pub mux_ids: bool,
}

/// One claimed change kind. The harness compares an oracle change of `kind` that satisfies
/// `where` with the exported changes of the same kind, on `fields` only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChangeClaim {
    /// A `kind` value of the oracle vocabulary.
    pub kind: String,
    /// Restricts the claim to changes whose field equals one of the listed values (an absent
    /// field equals `null`). For example `{"event": ["transfer", "mint"]}` on `token-event`.
    #[serde(default, rename = "where", skip_serializing_if = "BTreeMap::is_empty")]
    pub filter: BTreeMap<String, Vec<Value>>,
    /// Fields of the kind the target stores and the harness compares. May include `scope` and
    /// `op_index`.
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StartParams {
    /// The chain id, for logs and container names.
    pub chain: String,
    /// `testnet` or `mainnet`: which passphrase variant of the chain is replayed.
    pub network: String,
    pub network_passphrase: String,
    /// Ledger protocol of the chain.
    pub protocol: u32,
    pub delivery: Delivery,
    pub ledgers: LedgerWindow,
    pub limits: Limits,
    /// A Docker network created by the harness with `--internal` (no route off the host).
    /// Containers of the target join it and reach the replay server at the URLs below, which
    /// point at the network's gateway address. Absent when the harness runs without Docker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docker_network: Option<String>,
    /// A fresh empty directory the adapter may use for this chain; the harness removes it.
    pub workdir: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Delivery {
    /// The mode the harness picked, one of the adapter's `delivery_modes`.
    pub mode: DeliveryMode,
    /// JSON-RPC endpoint of the replay server (stellar-rpc read methods). Present in every
    /// mode, since targets such as wallet-backend need it next to a lake.
    pub rpc_url: String,
    /// Root of the history archive as it stood at the newest checkpoint at or below the head.
    pub archive_url: String,
    pub lake: Lake,
}

/// The chain's SEP-54 lake. `url` serves `.config.json` and only those batches whose
/// ledgers are all visible; `path` holds every batch, so an adapter copying from it must
/// skip batches past the head it was last told.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Lake {
    pub url: String,
    /// Absolute path on the harness host.
    pub path: String,
    pub ledgers_per_batch: u32,
    pub batches_per_partition: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LedgerWindow {
    /// Visible head when `start` is sent. A checkpoint ledger for `checkpoint` targets.
    pub bootstrap: u32,
    /// The chain's last ledger. The harness releases `bootstrap + 1` to `last` one at a time.
    pub last: u32,
    /// Ledgers per history checkpoint the archive was written with.
    pub checkpoint_frequency: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Limits {
    /// Memory limit the adapter must apply to the target (a container `--memory` limit, or
    /// the sum over the target's containers that ingest). A target killed for exceeding it is
    /// reported with `health: "oom"`.
    pub memory_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StartResult {
    /// What actually runs, for example container image ids; free form, kept in the report.
    #[serde(default)]
    pub components: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AdvanceResult {
    /// The head the target can now read up to; equals the requested head.
    pub head: u32,
}

/// Target health as the adapter observes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Health {
    /// Up, still bootstrapping (no ledger ingested yet).
    Starting,
    /// The ingest process is alive. It may be stuck; the harness decides that from progress
    /// and `last_error`.
    Running,
    /// The ingest process exited or was killed for a reason other than its memory limit.
    Crashed,
    /// The ingest process was killed for exceeding `limits.memory_bytes`.
    Oom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StatusResult {
    /// The highest ledger the target has durably ingested, read from the target's own store
    /// (for wallet-backend, `ingest_store.latest_ingest_ledger`). `null` before the first.
    pub last_ingested_ledger: Option<u32>,
    pub health: Health,
    /// The most recent error the target reported (log line, exit status, health endpoint
    /// text), verbatim. `null` when it has reported none since `start`.
    pub last_error: Option<String>,
    /// Peak memory of the target since `start` in bytes, from the container runtime or the
    /// kernel. `null` when it cannot be read.
    pub peak_memory_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExportResult {
    /// Number of `record` lines sent for this request.
    pub records: u64,
}

/// One stored table, sent before its rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DumpTable {
    /// The table's name in the target's store, for example `state_changes`.
    pub name: String,
    /// Its columns in the order every `row` of the table lists cells.
    pub columns: Vec<DumpColumn>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DumpColumn {
    pub name: String,
    /// The store's own type name, for example `bytea`, `text`, `bigint`.
    #[serde(rename = "type")]
    pub ty: String,
}

/// One stored row. `cells` follow the table's `columns`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DumpRow {
    pub table: String,
    /// Cell text: byte columns as lowercase hex with no prefix, every other column in the
    /// store's own text form (for PostgreSQL, the value cast to `text`). `null` for SQL NULL.
    pub cells: Vec<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DumpResult {
    /// Number of `table` lines sent for this request.
    pub tables: u64,
    /// Number of `row` lines sent for this request.
    pub rows: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RestartResult {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StopResult {
    /// The tail of the target's logs (at most 1 MiB), kept next to the report.
    pub logs: String,
}

/// One streamed export line's payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ExportRecord {
    /// What the target stored for one transaction.
    Transaction(TxRecord),
    /// Stored rows the adapter cannot tie to a transaction hash.
    Unattributed {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ledger: Option<u32>,
        unmapped: Vec<Unmapped>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TxRecord {
    pub ledger: u32,
    /// Transaction hash, lowercase hex. For a fee bump, the outer (fee bump) hash, which is
    /// what the oracle's `hash` holds.
    pub hash: String,
    /// Every participant the target stored for the transaction.
    pub participants: Vec<ExportParticipant>,
    /// Every stored state change the adapter maps to a claimed kind.
    pub changes: Vec<ExportChange>,
    /// Stored rows for this transaction the adapter has no oracle mapping for. Never dropped.
    #[serde(default)]
    pub unmapped: Vec<Unmapped>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExportParticipant {
    /// Canonical strkey, or `null` when the stored value does not decode (see `raw`).
    pub address: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mux_ids: Vec<Int>,
    /// The stored bytes, where the target stores an address as bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<RawAddress>,
}

/// One stored state change in oracle vocabulary terms. `kind` and the claimed fields use the
/// names and encodings of the vocabulary (`$defs/Change`). An address field whose stored
/// bytes do not decode is `null` and has an entry in `raw`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExportChange {
    pub kind: String,
    /// Stored address bytes, by vocabulary field name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub raw: BTreeMap<String, RawAddress>,
    /// The stored row the change was read from, for triage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<RowRef>,
    /// The vocabulary fields of `kind` the target stores.
    #[serde(flatten)]
    #[schemars(with = "BTreeMap<String, Value>")]
    pub fields: serde_json::Map<String, Value>,
}

/// Address bytes as the target stored them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RawAddress {
    /// `table.column` the bytes came from.
    pub column: String,
    /// The stored bytes, lowercase hex, exactly as stored (for wallet-backend's
    /// `AddressBytea`, the version byte then the key bytes).
    pub hex: String,
    /// The strkey the bytes decode to. Exactly one of `strkey` and `error` is present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strkey: Option<String>,
    /// Why the bytes do not decode to any strkey.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RowRef {
    pub table: String,
    /// The target's own classification, for example wallet-backend's
    /// `state_change_category` and `state_change_reason`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Unmapped {
    pub table: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The row, column name to value, byte columns as lowercase hex.
    pub row: serde_json::Map<String, Value>,
    /// Why the adapter could not map it.
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorClass {
    /// The request does not follow this protocol. A harness bug.
    BadRequest,
    /// Valid, but this adapter cannot do it (a delivery mode it lacks).
    Unsupported,
    /// The target could not be brought up or queried: build failure, a container that
    /// does not start, a database that refuses connections.
    Target,
    /// The adapter itself failed.
    Internal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AdapterError {
    pub class: ErrorClass,
    pub message: String,
}

/// One line from the adapter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Response {
    pub id: String,
    #[serde(flatten)]
    pub body: ResponseBody,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum ResponseBody {
    /// The final line for a request.
    Ok {
        #[schemars(with = "AnyResult")]
        result: Value,
    },
    /// One export record; only while an `export` is answered.
    Record { record: Box<ExportRecord> },
    /// A table header; only while a `dump` is answered, before the table's rows.
    Table { table: DumpTable },
    /// One stored row; only while a `dump` is answered, after its table's header.
    Row { row: DumpRow },
    /// The final line for a failed request.
    Error { error: AdapterError },
}

/// The result member of an `ok` line, by command.
#[derive(JsonSchema)]
#[serde(untagged)]
#[allow(dead_code)]
enum AnyResult {
    Describe(DescribeResult),
    Start(StartResult),
    Advance(AdvanceResult),
    Status(StatusResult),
    Export(ExportResult),
    Dump(DumpResult),
    Restart(RestartResult),
    Stop(StopResult),
}

/// The committed `schema/adapter-protocol.schema.json`: requests, responses, and the oracle
/// vocabulary as `$defs/Change`, all generated from the Rust types.
pub fn protocol_schema() -> String {
    let mut gen_ = schemars::generate::SchemaSettings::draft2020_12().into_generator();
    let request = gen_.subschema_for::<Request>();
    let response = gen_.subschema_for::<Response>();
    gen_.subschema_for::<Change>();
    let mut defs = gen_.take_definitions(true);
    let kinds = change_kinds_from(&defs);
    // the export's kind is any vocabulary kind; schemars cannot see that from a String
    if let Some(Value::Object(props)) = defs
        .get_mut("ExportChange")
        .and_then(|d| d.get_mut("properties"))
    {
        props.insert(
            "kind".into(),
            serde_json::json!({"enum": kinds, "description": "A `kind` of the oracle vocabulary."}),
        );
    }
    let root = serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://github.com/ErenTopaal/shapekit/schema/adapter-protocol.schema.json",
        "title": format!("shapekit adapter protocol, version {ADAPTER_PROTOCOL_VERSION}"),
        "description": "One JSON object per line: requests on the adapter's stdin, responses on its stdout. See docs/adapter-interface.md.",
        "oneOf": [request, response],
        "$defs": defs,
    });
    let mut s = serde_json::to_string_pretty(&root).expect("a schema is plain json");
    s.push('\n');
    s
}

/// Every `kind` value of the vocabulary, read from the generated `Change` schema.
fn change_kinds_from(defs: &serde_json::Map<String, Value>) -> Vec<String> {
    let mut kinds: Vec<String> = defs
        .get("Change")
        .and_then(|c| c.get("oneOf"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| v.pointer("/properties/kind/const").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    kinds.sort();
    kinds
}

/// Vocabulary kind to the field names it has, from the generated schema. `scope` and
/// `op_index` (from `StateChange`) are valid for every kind.
pub fn vocabulary_fields() -> BTreeMap<String, Vec<String>> {
    let mut gen_ = schemars::generate::SchemaSettings::draft2020_12().into_generator();
    gen_.subschema_for::<Change>();
    let defs = gen_.take_definitions(true);
    let mut out = BTreeMap::new();
    for variant in vocabulary_variants(&defs) {
        out.insert(
            variant.kind,
            variant.fields.into_iter().map(|f| f.name).collect(),
        );
    }
    out
}

/// One field of a vocabulary kind, as the doc table shows it.
#[derive(Debug, Clone)]
pub struct VocabField {
    pub name: String,
    pub ty: String,
    pub required: bool,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct VocabVariant {
    pub kind: String,
    pub description: String,
    pub fields: Vec<VocabField>,
}

fn type_name(schema: &Value, defs: &serde_json::Map<String, Value>) -> String {
    if let Some(r) = schema.get("$ref").and_then(Value::as_str) {
        let name = r.rsplit('/').next().unwrap_or(r);
        if let Some(Value::Array(vals)) = defs.get(name).and_then(|d| d.get("enum")) {
            let names: Vec<String> = vals.iter().map(|v| format!("`{v}`")).collect();
            return names.join(" / ");
        }
        return match name {
            "Int" => "decimal string".into(),
            other => other.into(),
        };
    }
    if let Some(any) = schema.get("anyOf").and_then(Value::as_array) {
        let parts: Vec<String> = any
            .iter()
            .filter(|s| s.get("type").and_then(Value::as_str) != Some("null"))
            .map(|s| type_name(s, defs))
            .collect();
        return parts.join(" or ");
    }
    match schema.get("type") {
        Some(Value::String(t)) if t == "array" => format!(
            "array of {}",
            schema
                .get("items")
                .map_or_else(|| "values".into(), |i| type_name(i, defs))
        ),
        Some(Value::String(t)) if t == "integer" => {
            let f = schema.get("format").and_then(Value::as_str).unwrap_or("");
            match f {
                "" => "integer".into(),
                f => format!("integer ({f})"),
            }
        }
        Some(Value::String(t)) => t.clone(),
        Some(Value::Array(ts)) => ts
            .iter()
            .filter_map(Value::as_str)
            .filter(|t| *t != "null")
            .collect::<Vec<_>>()
            .join(" or "),
        _ => "value".into(),
    }
}

/// The vocabulary's kinds and fields, from the `Change` schema schemars derives from
/// `shapekit_oracle::vocab`.
pub fn vocabulary_variants(defs: &serde_json::Map<String, Value>) -> Vec<VocabVariant> {
    let mut out = Vec::new();
    let variants = defs
        .get("Change")
        .and_then(|c| c.get("oneOf"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for v in variants {
        let Some(kind) = v.pointer("/properties/kind/const").and_then(Value::as_str) else {
            continue;
        };
        // token-event is a newtype variant: its fields live on the TokenEvent definition
        let (props, required, description) = match v.get("$ref").and_then(Value::as_str) {
            Some(r) => {
                let def = defs
                    .get(r.rsplit('/').next().unwrap_or(r))
                    .cloned()
                    .unwrap_or(Value::Null);
                let desc = v
                    .get("description")
                    .or_else(|| def.get("description"))
                    .cloned();
                (
                    def.get("properties").cloned(),
                    def.get("required").cloned(),
                    desc,
                )
            }
            None => (
                v.get("properties").cloned(),
                v.get("required").cloned(),
                v.get("description").cloned(),
            ),
        };
        let required: Vec<String> = required
            .and_then(|r| serde_json::from_value(r).ok())
            .unwrap_or_default();
        let mut fields = Vec::new();
        if let Some(Value::Object(props)) = props {
            for (name, schema) in props {
                if name == "kind" {
                    continue;
                }
                fields.push(VocabField {
                    ty: type_name(&schema, defs),
                    required: required.contains(&name),
                    description: schema
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .replace('\n', " "),
                    name,
                });
            }
        }
        out.push(VocabVariant {
            kind: kind.to_owned(),
            description: description
                .and_then(|d| d.as_str().map(|s| s.replace('\n', " ")))
                .unwrap_or_default(),
            fields,
        });
    }
    out
}

pub const VOCAB_BEGIN: &str =
    "<!-- vocabulary: generated by `shapekit harness schema --write`, do not edit -->";
pub const VOCAB_END: &str = "<!-- end of generated vocabulary -->";

/// The generated vocabulary section of `docs/adapter-interface.md`, between the markers.
pub fn vocabulary_markdown() -> String {
    let mut gen_ = schemars::generate::SchemaSettings::draft2020_12().into_generator();
    gen_.subschema_for::<Change>();
    let defs = gen_.take_definitions(true);
    let mut s = String::new();
    s.push_str(VOCAB_BEGIN);
    s.push_str(&format!(
        "\n\nOracle vocabulary version **{}** (`ORACLE_VOCAB_VERSION` in `crates/shapekit-oracle/src/vocab.rs`).\n",
        shapekit_oracle::vocab::ORACLE_VOCAB_VERSION
    ));
    s.push_str(
        "Every change also has `scope` (`fee-processing`, `tx-before`, `operation`, `tx-after`, \
`post-apply-fee`, `upgrade`) and, for `operation` scope, `op_index` (integer, from 0).\n",
    );
    for v in vocabulary_variants(&defs) {
        s.push_str(&format!("\n### `{}`\n\n", v.kind));
        if !v.description.is_empty() {
            s.push_str(&v.description);
            s.push_str("\n\n");
        }
        s.push_str("| field | type | always present | meaning |\n|---|---|---|---|\n");
        for f in &v.fields {
            s.push_str(&format!(
                "| `{}` | {} | {} | {} |\n",
                f.name,
                f.ty,
                if f.required { "yes" } else { "no" },
                f.description.replace('|', "\\|")
            ));
        }
    }
    s.push('\n');
    s.push_str(VOCAB_END);
    s
}

/// Replaces the generated section of `doc` with the current one. Errors when the markers are
/// missing.
pub fn splice_vocabulary(doc: &str) -> Result<String, String> {
    let begin = doc
        .find(VOCAB_BEGIN)
        .ok_or("the doc has no vocabulary begin marker")?;
    let end = doc[begin..]
        .find(VOCAB_END)
        .map(|e| begin + e + VOCAB_END.len())
        .ok_or("the doc has no vocabulary end marker")?;
    Ok(format!(
        "{}{}{}",
        &doc[..begin],
        vocabulary_markdown(),
        &doc[end..]
    ))
}

/// Brings `schema/adapter-protocol.schema.json` and the vocabulary section of
/// `docs/adapter-interface.md` under `root` up to date, or with `write` false only reports
/// which of them are stale. Returns the stale paths.
pub fn sync_generated(root: &std::path::Path, write: bool) -> anyhow::Result<Vec<String>> {
    use anyhow::Context;
    let mut stale = Vec::new();
    let schema_path = root.join("schema/adapter-protocol.schema.json");
    let schema = protocol_schema();
    let current = std::fs::read_to_string(&schema_path).unwrap_or_default();
    if current != schema {
        stale.push(schema_path.display().to_string());
        if write {
            std::fs::write(&schema_path, &schema)
                .with_context(|| format!("writing {}", schema_path.display()))?;
        }
    }
    let doc_path = root.join("docs/adapter-interface.md");
    let doc = std::fs::read_to_string(&doc_path)
        .with_context(|| format!("reading {}", doc_path.display()))?;
    let spliced =
        splice_vocabulary(&doc).map_err(|e| anyhow::anyhow!("{}: {e}", doc_path.display()))?;
    if spliced != doc {
        stale.push(doc_path.display().to_string());
        if write {
            std::fs::write(&doc_path, &spliced)
                .with_context(|| format!("writing {}", doc_path.display()))?;
        }
    }
    Ok(stale)
}
