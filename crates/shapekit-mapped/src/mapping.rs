//! The mapping file: how to build and start an indexer, how ledgers reach it, and which of its
//! PostgreSQL tables and columns hold which oracle facts. The format is in
//! docs/mapping-adapter.md; `adapters/walletbackend/adapter.toml` is a complete example.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use serde::Deserialize;
use shapekit_harness::protocol::{Bootstrap, Claims, DeliveryMode};

use crate::template;

/// The only mapping format version so far.
pub const SCHEMA: u32 = 1;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    pub schema: u32,
    pub target: Target,
    #[serde(default)]
    pub vars: BTreeMap<String, VarSpec>,
    #[serde(default)]
    pub source: Option<Source>,
    pub image: Image,
    #[serde(default, rename = "service")]
    pub services: Vec<Service>,
    pub database: Database,
    #[serde(default)]
    pub delivery: DeliveryConfig,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub env_when: Vec<EnvWhen>,
    #[serde(default, rename = "step")]
    pub steps: Vec<Step>,
    pub ingest: Ingest,
    #[serde(default)]
    pub components: BTreeMap<String, String>,
    pub status: Status,
    pub claims: Claims,
    pub export: Export,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub name: String,
    /// The exact build, a template such as `{commit}`.
    pub version: String,
    pub bootstrap: Bootstrap,
    pub delivery_modes: Vec<DeliveryMode>,
    pub default_delivery: DeliveryMode,
    pub protocols: ProtocolTemplates,
    /// Container names start with this; the target name when absent.
    #[serde(default)]
    pub container_prefix: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolTemplates {
    pub min: String,
    pub max: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VarSpec {
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<String>,
    /// A regular expression the whole value must match.
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub about: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// The repository to clone when `dir` holds no clone.
    pub git: String,
    pub dir: String,
    pub commit: String,
    #[serde(default, rename = "fact")]
    pub facts: Vec<Fact>,
}

/// A value read from a file of the source at the commit.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fact {
    pub name: String,
    pub file: String,
    /// The first capture group of the first match is the value.
    #[serde(default)]
    pub pattern: Option<String>,
    /// The value is `true` when the file contains this text, else `false`.
    #[serde(default)]
    pub contains: Option<String>,
    /// Translates the captured text; a capture missing here is an error naming it.
    #[serde(default)]
    pub values: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Image {
    /// An image to run as it is, pulled when it is not present.
    #[serde(default)]
    pub name: Option<String>,
    /// The tag to build the source's commit into.
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub dockerfile: Option<String>,
    #[serde(default)]
    pub build_args: BTreeMap<String, String>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    /// The label holding the digest of the pinned base set, so an image built on other bases
    /// is rebuilt.
    #[serde(default)]
    pub base_label: Option<String>,
    /// Every `FROM` of the Dockerfile and the image by digest it is built on instead.
    #[serde(default)]
    pub bases: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Service {
    pub name: String,
    pub image: String,
    /// The start result names the image under this key.
    #[serde(default)]
    pub component: Option<String>,
    /// Bytes, a template; no limit when absent.
    #[serde(default)]
    pub memory: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub entrypoint: Option<String>,
    #[serde(default)]
    pub command: Vec<String>,
    /// Only in these delivery modes; in every mode when absent.
    #[serde(default)]
    pub delivery: Option<Vec<DeliveryMode>>,
    #[serde(default)]
    pub ready: Option<Ready>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ready {
    /// A command run inside the container that exits 0 once the service is up.
    #[serde(default)]
    pub exec: Option<Vec<String>>,
    /// A URL that answers 200 once the service is up.
    #[serde(default)]
    pub http: Option<String>,
    pub seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Database {
    /// How the adapter reaches the database from the host, usually through `{service.<name>.ip}`.
    pub url: String,
    /// Statements run once the database answers, before any step.
    #[serde(default)]
    pub init: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryConfig {
    #[serde(default, rename = "sep54-s3")]
    pub s3: Option<S3Delivery>,
    #[serde(default, rename = "sep54-gcs")]
    pub gcs: Option<GcsDelivery>,
    #[serde(default, rename = "sep54-filesystem")]
    pub filesystem: Option<FilesystemDelivery>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct S3Delivery {
    /// Path-style bucket URL; each object is a `PUT` to `<bucket_url>/<key>`.
    pub bucket_url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GcsDelivery {
    /// Root URL of a Google Cloud Storage JSON API endpoint (fake-gcs-server).
    pub url: String,
    pub bucket: String,
    /// Prepended to every object name, for example `{network}/`.
    #[serde(default)]
    pub prefix: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilesystemDelivery {
    /// Where the released lake is mounted in the target's containers, read only.
    pub mount: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvWhen {
    #[serde(default)]
    pub delivery: Option<DeliveryMode>,
    #[serde(default)]
    pub fact: BTreeMap<String, String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub name: String,
    /// The target image when absent.
    #[serde(default)]
    pub image: Option<String>,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ingest {
    #[serde(default)]
    pub image: Option<String>,
    pub command: Vec<String>,
    /// docker's `--restart` policy.
    #[serde(default)]
    pub restart: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    /// One row, one column: the highest ledger the target durably ingested. No row, NULL or
    /// 0 means none yet.
    pub cursor: String,
    #[serde(default = "default_log_tail")]
    pub log_tail: u32,
    /// Matched against the whole log: a match means the ingest process crashed.
    #[serde(default)]
    pub crash: Option<String>,
    /// The newest log line any of these rules matches is `last_error`.
    #[serde(default)]
    pub error: Vec<ErrorRule>,
}

fn default_log_tail() -> u32 {
    5000
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorRule {
    pub pattern: String,
    /// The first match's first capture group must be one of these.
    #[serde(default)]
    pub values: Option<Vec<String>>,
    /// The line must match this too.
    #[serde(default)]
    pub also: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Export {
    /// `$1`, `$2`: the first and last ledger. One row per stored transaction, in apply order.
    pub transactions: String,
    pub transaction: TransactionColumns,
    /// `$1`: the transaction's id column value.
    pub participants: String,
    pub participant: AddressColumn,
    /// `$1`: the transaction's id column value. Every stored row of the transaction.
    pub rows: String,
    pub table: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    /// Rows are sorted by these columns, stable, before the passes.
    #[serde(default)]
    pub order: Vec<String>,
    pub operation: OperationColumn,
    #[serde(default = "default_window")]
    pub unattributed_window: u32,
    /// Named sets of strkey letters an address column may hold.
    #[serde(default)]
    pub forms: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub lookup: Vec<Lookup>,
    #[serde(default)]
    pub unattributed: Vec<Unattributed>,
    #[serde(default)]
    pub pass: Vec<Pass>,
}

fn default_window() -> u32 {
    64
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionColumns {
    pub id: String,
    pub hash: String,
    pub ledger: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddressColumn {
    pub address: String,
    pub forms: Forms,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Forms {
    Named(String),
    Letters(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OperationEncoding {
    /// A TOID: the operation index is the low 12 bits minus one; 0 there is the transaction.
    Toid,
    /// The 0-based operation index; NULL is the transaction.
    Index,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationColumn {
    pub column: String,
    pub encoding: OperationEncoding,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lookup {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: LookupKind,
    /// Rows of `code` and `issuer`.
    pub query: String,
    /// The native asset's contract too.
    #[serde(default)]
    pub native: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LookupKind {
    /// Keyed by the Stellar Asset Contract id of each asset under the run's passphrase; the
    /// value is the vocabulary's asset name.
    AssetContract,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unattributed {
    /// `$1`, `$2`: the first and last ledger of a window.
    pub query: String,
    pub ledger: LedgerColumn,
    pub table: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerColumn {
    pub column: String,
    /// `toid`: the ledger is the upper 32 bits.
    #[serde(default)]
    pub encoding: Option<OperationEncoding>,
}

/// A column value condition: one value or any of several.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    pub fn contains(&self, v: &str) -> bool {
        match self {
            OneOrMany::One(s) => s == v,
            OneOrMany::Many(m) => m.iter().any(|s| s == v),
        }
    }
}

pub type Condition = BTreeMap<String, OneOrMany>;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pass {
    /// The rows of the operation this pass takes, among those no earlier pass took.
    #[serde(default)]
    pub select: Condition,
    #[serde(default, rename = "case")]
    pub cases: Vec<Case>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Level {
    /// Rows of the transaction itself (fee rows).
    Transaction,
    /// Rows of one operation.
    Operation,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    #[serde(default)]
    pub level: Option<Level>,
    #[serde(default)]
    pub when: Condition,
    /// The column holds stored address bytes whose version byte is one of these letters.
    #[serde(default)]
    pub when_version: BTreeMap<String, Vec<String>>,
    /// The column holds at least one byte and its version byte is none of these letters.
    #[serde(default)]
    pub when_version_not: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub kind: Option<String>,
    /// The row is reported as unmapped with this note; `{<column>:version}` renders the
    /// column's first byte.
    #[serde(default)]
    pub unmapped: Option<String>,
    /// One change per operation and value of these columns; later rows with the same values
    /// are taken without a change of their own.
    #[serde(default)]
    pub merge: Vec<String>,
    #[serde(default)]
    pub fields: BTreeMap<String, Field>,
    #[serde(default)]
    pub absorb: Option<Absorb>,
    #[serde(default)]
    pub pair: Option<Pair>,
    #[serde(default)]
    pub paired: BTreeMap<String, Field>,
    #[serde(default)]
    pub unpaired: BTreeMap<String, Field>,
}

/// The first later row of the operation, not taken yet, that matches `select` and agrees on
/// `same`, is taken with the row, and `fields` are read from it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Absorb {
    pub select: Condition,
    pub same: Vec<String>,
    pub fields: BTreeMap<String, Field>,
}

/// The row right after this one in the pass is its partner when it matches `select`, its
/// `next` column is one higher, and it agrees on `same`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pair {
    pub select: Condition,
    pub next: String,
    pub same: Vec<String>,
}

/// Where one vocabulary field comes from. Exactly one source key is set.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    /// Stored address bytes: version byte, then payload.
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub forms: Option<Forms>,
    /// An integer column, as a JSON integer.
    #[serde(default)]
    pub int: Option<String>,
    /// A text column, as a string.
    #[serde(default)]
    pub text: Option<String>,
    /// With `text`: wrap the value as `{"type": <typed>, "value": <text>}`.
    #[serde(default)]
    pub typed: Option<String>,
    /// With `text`: an empty value counts as absent.
    #[serde(default)]
    pub skip_empty: bool,
    /// A constant, or with `equals` the value compared.
    #[serde(default)]
    pub value: Option<toml::Value>,
    /// True when this column equals `value`.
    #[serde(default)]
    pub equals: Option<String>,
    /// A decimal amount with at most 7 fraction digits, as stroops.
    #[serde(default)]
    pub stroops: Option<String>,
    /// A decimal integer, negated.
    #[serde(default)]
    pub negate: Option<String>,
    /// A text column's bytes as lowercase hex.
    #[serde(default)]
    pub text_hex: Option<String>,
    /// A JSON column, read at `path` and converted `as`.
    #[serde(default)]
    pub json: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default, rename = "as")]
    pub json_as: Option<JsonAs>,
    /// An integer column of flag bits, translated bit by bit through `map`.
    #[serde(default)]
    pub bits: Option<String>,
    #[serde(default)]
    pub map: BTreeMap<String, u32>,
    /// An asset contract address column, named through `lookup`, or a pool id in `pool`.
    #[serde(default)]
    pub asset: Option<String>,
    #[serde(default)]
    pub pool: Option<String>,
    #[serde(default)]
    pub lookup: Option<String>,
    /// The value of a NULL column, where the source would otherwise be absent.
    #[serde(default)]
    pub default: Option<String>,
    /// The unmapped note when the value cannot be read.
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JsonAs {
    /// A string, else `default`.
    String,
    /// Base64 text as lowercase hex; absent when the key is absent.
    Base64Hex,
    /// A non-negative 32-bit integer (or its decimal string); absent otherwise.
    Uint32,
}

impl Field {
    /// The source keys that are set, for validation.
    fn sources(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        let pairs: [(&'static str, bool); 10] = [
            ("address", self.address.is_some()),
            ("int", self.int.is_some()),
            ("text", self.text.is_some()),
            ("value", self.value.is_some() && self.equals.is_none()),
            ("equals", self.equals.is_some()),
            ("stroops", self.stroops.is_some()),
            ("negate", self.negate.is_some()),
            ("text_hex", self.text_hex.is_some()),
            ("json", self.json.is_some()),
            ("bits", self.bits.is_some()),
        ];
        for (k, set) in pairs {
            if set {
                out.push(k);
            }
        }
        if self.asset.is_some() || self.pool.is_some() {
            out.push("asset");
        }
        out
    }
}

/// Every strkey letter an address column can name.
pub const LETTERS: [&str; 10] = ["G", "S", "M", "T", "X", "P", "C", "L", "B", "W"];

impl Mapping {
    pub fn load(path: &Path) -> Result<Mapping> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let m: Mapping =
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        m.validate()
            .with_context(|| format!("checking {}", path.display()))?;
        Ok(m)
    }

    /// Checks what can be checked before anything runs: references between sections,
    /// regular expressions, field sources and the placeholders of describe.
    pub fn validate(&self) -> Result<()> {
        if self.schema != SCHEMA {
            bail!("schema {} is not {SCHEMA}", self.schema);
        }
        if !self
            .target
            .delivery_modes
            .contains(&self.target.default_delivery)
        {
            bail!("target.default_delivery is not one of target.delivery_modes");
        }
        for mode in &self.target.delivery_modes {
            let ok = match mode {
                DeliveryMode::Rpc => true,
                DeliveryMode::Sep54S3 => self.delivery.s3.is_some(),
                DeliveryMode::Sep54Gcs => self.delivery.gcs.is_some(),
                DeliveryMode::Sep54Filesystem => self.delivery.filesystem.is_some(),
            };
            if !ok {
                bail!(
                    "target.delivery_modes lists {} but there is no [delivery.{}]",
                    mode.name(),
                    mode.name()
                );
            }
        }
        for (name, v) in &self.vars {
            if v.required && v.default.is_some() {
                bail!("vars.{name} is required and has a default");
            }
            if let Some(p) = &v.pattern {
                regex::Regex::new(p).with_context(|| format!("vars.{name}.pattern"))?;
            }
        }
        match (&self.image.name, &self.image.tag) {
            (Some(_), None) => {
                if !self.image.bases.is_empty() || self.image.dockerfile.is_some() {
                    bail!("image.name runs an image as it is; bases and dockerfile need image.tag");
                }
            }
            (None, Some(_)) => {
                if self.source.is_none() {
                    bail!("image.tag builds the source, but there is no [source]");
                }
            }
            _ => bail!("set exactly one of image.name and image.tag"),
        }
        let mut facts = BTreeSet::new();
        if let Some(src) = &self.source {
            for f in &src.facts {
                if !facts.insert(f.name.clone()) {
                    bail!("fact {} is defined twice", f.name);
                }
                match (&f.pattern, &f.contains) {
                    (Some(p), None) => {
                        regex::Regex::new(p).with_context(|| format!("fact {}", f.name))?;
                    }
                    (None, Some(_)) => {
                        if f.values.is_some() {
                            bail!("fact {}: values need a pattern", f.name);
                        }
                    }
                    _ => bail!("fact {}: set exactly one of pattern and contains", f.name),
                }
            }
        }
        let mut services = BTreeSet::new();
        for s in &self.services {
            if !services.insert(s.name.clone()) {
                bail!("service {} is defined twice", s.name);
            }
            if let Some(r) = &s.ready
                && r.exec.is_some() == r.http.is_some()
            {
                bail!(
                    "service {}: ready needs exactly one of exec and http",
                    s.name
                );
            }
        }
        for w in &self.env_when {
            for f in w.fact.keys() {
                if !facts.contains(f) {
                    bail!("env_when names fact {f}, which is not defined");
                }
            }
        }
        let describe_names: Vec<String> = [
            &self.target.version,
            &self.target.protocols.min,
            &self.target.protocols.max,
        ]
        .iter()
        .flat_map(|s| template::names(s))
        .collect();
        for n in describe_names {
            let known = self.vars.contains_key(&n)
                || n == "cache"
                || n.strip_prefix("fact.").is_some_and(|f| facts.contains(f));
            if !known {
                bail!("{{{n}}} in [target] is neither a variable nor a fact");
            }
        }
        if let Some(c) = &self.status.crash {
            regex::Regex::new(c).context("status.crash")?;
        }
        for e in &self.status.error {
            regex::Regex::new(&e.pattern).context("status.error.pattern")?;
            if let Some(a) = &e.also {
                regex::Regex::new(a).context("status.error.also")?;
            }
        }
        self.validate_export()
    }

    fn validate_export(&self) -> Result<()> {
        let ex = &self.export;
        for (name, letters) in &ex.forms {
            for l in letters {
                if !LETTERS.contains(&l.as_str()) {
                    bail!("export.forms.{name}: {l} is not a strkey letter");
                }
            }
        }
        self.forms(&ex.participant.forms)
            .context("export.participant")?;
        if ex.unattributed_window == 0 {
            bail!("export.unattributed_window is 0");
        }
        let lookups: BTreeSet<&str> = ex.lookup.iter().map(|l| l.name.as_str()).collect();
        for (pi, pass) in ex.pass.iter().enumerate() {
            for (ci, case) in pass.cases.iter().enumerate() {
                let at = format!("export.pass {} case {}", pi + 1, ci + 1);
                for (col, letters) in case.when_version.iter().chain(&case.when_version_not) {
                    for l in letters {
                        if !LETTERS.contains(&l.as_str()) {
                            bail!("{at}: {col}: {l} is not a strkey letter");
                        }
                    }
                }
                match (&case.kind, &case.unmapped) {
                    (Some(_), None) => {}
                    (None, Some(_)) => {
                        if !case.fields.is_empty()
                            || case.pair.is_some()
                            || case.absorb.is_some()
                            || !case.merge.is_empty()
                        {
                            bail!("{at}: an unmapped case reads no fields");
                        }
                    }
                    _ => bail!("{at}: set exactly one of kind and unmapped"),
                }
                if case.pair.is_none() && (!case.paired.is_empty() || !case.unpaired.is_empty()) {
                    bail!("{at}: paired and unpaired fields need a pair");
                }
                if case.pair.is_some() && case.absorb.is_some() {
                    bail!("{at}: a case cannot both pair and absorb");
                }
                let all = case
                    .fields
                    .iter()
                    .chain(&case.paired)
                    .chain(&case.unpaired)
                    .chain(case.absorb.iter().flat_map(|a| &a.fields));
                for (name, f) in all {
                    self.validate_field(f)
                        .with_context(|| format!("{at}: field {name}"))?;
                    if let Some(l) = &f.lookup
                        && !lookups.contains(l.as_str())
                    {
                        bail!("{at}: field {name}: no lookup named {l}");
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_field(&self, f: &Field) -> Result<()> {
        let s = f.sources();
        if s.len() != 1 {
            bail!("needs exactly one source, has {}", s.join(", "));
        }
        match s[0] {
            "address" => {
                let forms = f.forms.as_ref().context("address needs forms")?;
                self.forms(forms)?;
            }
            "asset" => {
                if f.asset.is_none() || f.lookup.is_none() {
                    bail!("an asset needs asset and lookup");
                }
            }
            "json" => {
                if f.path.is_none() || f.json_as.is_none() {
                    bail!("json needs path and as");
                }
            }
            "bits" => {
                for k in f.map.keys() {
                    let b: u32 = k
                        .parse()
                        .with_context(|| format!("bit {k:?} is not a number"))?;
                    if b > 63 {
                        bail!("bit {b} is past 63");
                    }
                }
            }
            "equals" if f.value.is_none() => bail!("equals needs value"),
            _ => {}
        }
        if f.typed.is_some() && f.text.is_none() {
            bail!("typed needs text");
        }
        Ok(())
    }

    /// The strkey letters of a forms reference.
    pub fn forms(&self, f: &Forms) -> Result<Vec<String>> {
        match f {
            Forms::Letters(l) => {
                for x in l {
                    if !LETTERS.contains(&x.as_str()) {
                        bail!("{x} is not a strkey letter");
                    }
                }
                Ok(l.clone())
            }
            Forms::Named(n) => self
                .export
                .forms
                .get(n)
                .cloned()
                .with_context(|| format!("no export.forms named {n}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallet_backend_mapping_loads() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../adapters/walletbackend/adapter.toml");
        let m = Mapping::load(&path).unwrap();
        assert_eq!(m.target.name, "wallet-backend");
        assert_eq!(m.export.pass.len(), 4);
    }
}
