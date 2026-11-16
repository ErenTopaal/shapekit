//! The data formats shapekit's generators and checks exchange. Each format is a Rust type here,
//! and its JSON Schema under `schema/` is generated from that type by
//! `shapekit contracts schema --write`.
//!
//! | format | file | schema |
//! |---|---|---|
//! | [`ObligationLedger`] | `corpus/coverage/obligations.json` | `schema/obligations.schema.json` |
//! | [`ValidationTable`] | `registry/generated/validation.json` | `schema/validation.schema.json` |
//! | [`LimitsTable`] | `registry/generated/limits.json` | `schema/limits.schema.json` |
//! | [`GateList`] | `registry/generated/gates.json` | `schema/gates.schema.json` |
//! | [`CoreCoverage`] | `corpus/coverage/core-coverage.json` | `schema/core-coverage.schema.json` |
//! | [`RecipeIr`] | `corpus/recipes/<family>/<name>.json` | `schema/recipe-ir.schema.json` |
//! | [`AuditReport`] | written by `shapekit audit run` | `schema/audit-report.schema.json` |
//!
//! Every file carries a `schema_version`; [`load`] refuses a file whose version this build
//! does not read, and every type lists what is wrong with a value through [`Contract::problems`].

pub mod audit;
pub mod core_coverage;
pub mod gates;
pub mod limits;
pub mod obligations;
pub mod recipe_ir;
pub mod validation;
mod witness;

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use audit::AuditReport;
pub use core_coverage::CoreCoverage;
pub use gates::GateList;
pub use limits::LimitsTable;
pub use obligations::{ObligationKind, ObligationLedger, ObligationStatus};
pub use recipe_ir::RecipeIr;
pub use validation::ValidationTable;
pub use witness::WitnessRef;

/// One data format: its schema file, version and the checks a value must pass beyond its
/// schema.
pub trait Contract: Serialize + DeserializeOwned + JsonSchema {
    /// File name under `schema/`.
    const SCHEMA_FILE: &'static str;
    /// The only `schema_version` this build reads and writes.
    const VERSION: u32;
    const TITLE: &'static str;
    const DESCRIPTION: &'static str;

    fn schema_version(&self) -> u32;

    /// What is wrong with the value beyond what the schema says, one line each; empty when
    /// it is well formed.
    fn problems(&self) -> Vec<String>;
}

#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("parsing {path}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("{path}: schema_version {found}, this build reads {expected}")]
    Version {
        path: PathBuf,
        found: u32,
        expected: u32,
    },
    #[error("{path}: {}", .problems.join("; "))]
    Problems {
        path: PathBuf,
        problems: Vec<String>,
    },
}

/// Reads `path` as `T` and refuses it if its version differs from `T::VERSION` or
/// [`Contract::problems`] reports anything.
pub fn load<T: Contract>(path: &Path) -> Result<T, ContractError> {
    let text = std::fs::read_to_string(path).map_err(|source| ContractError::Io {
        path: path.to_owned(),
        source,
    })?;
    let value: T = serde_json::from_str(&text).map_err(|source| ContractError::Parse {
        path: path.to_owned(),
        source,
    })?;
    if value.schema_version() != T::VERSION {
        return Err(ContractError::Version {
            path: path.to_owned(),
            found: value.schema_version(),
            expected: T::VERSION,
        });
    }
    let problems = value.problems();
    if !problems.is_empty() {
        return Err(ContractError::Problems {
            path: path.to_owned(),
            problems,
        });
    }
    Ok(value)
}

/// The committed text of a value: pretty JSON in field declaration order, one trailing
/// newline. Generated files are written with this so a rerun gives the same bytes.
pub fn render<T: Contract>(value: &T) -> String {
    let mut text = serde_json::to_string_pretty(value).expect("contract types are plain json");
    text.push('\n');
    text
}

/// Checks `value` and writes it to `path` in its committed form.
pub fn write<T: Contract>(path: &Path, value: &T) -> Result<(), ContractError> {
    let problems = value.problems();
    if !problems.is_empty() {
        return Err(ContractError::Problems {
            path: path.to_owned(),
            problems,
        });
    }
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|source| ContractError::Io {
            path: dir.to_owned(),
            source,
        })?;
    }
    std::fs::write(path, render(value)).map_err(|source| ContractError::Io {
        path: path.to_owned(),
        source,
    })
}

/// The JSON Schema of `T` as committed under `schema/`: draft 2020-12, keys sorted.
pub fn schema_text<T: Contract>() -> String {
    let schema = schemars::generate::SchemaSettings::draft2020_12()
        .into_generator()
        .into_root_schema_for::<T>();
    let mut value = serde_json::to_value(schema).expect("a schema is plain json");
    if let Value::Object(map) = &mut value {
        map.insert(
            "$id".into(),
            Value::String(format!(
                "https://github.com/ErenTopaal/shapekit/schema/{}",
                T::SCHEMA_FILE
            )),
        );
        map.insert(
            "title".into(),
            Value::String(format!("{}, version {}", T::TITLE, T::VERSION)),
        );
        map.insert("description".into(), Value::String(T::DESCRIPTION.into()));
    }
    let mut text = serde_json::to_string_pretty(&sorted(&value)).expect("plain json");
    text.push('\n');
    text
}

fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|k| (k.clone(), sorted(&map[k])))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

/// Every contract: its schema file name, the generated schema text and a checker that loads a
/// data file of that format.
pub struct ContractInfo {
    /// The short name `shapekit contracts validate --contract` takes.
    pub name: &'static str,
    pub schema_file: &'static str,
    pub schema: fn() -> String,
    pub validate: fn(&Path) -> Result<(), ContractError>,
}

fn info<T: Contract>(name: &'static str) -> ContractInfo {
    ContractInfo {
        name,
        schema_file: T::SCHEMA_FILE,
        schema: schema_text::<T>,
        validate: |p| load::<T>(p).map(|_| ()),
    }
}

/// The seven contracts, in the order of the table at the top of this crate.
pub fn all() -> Vec<ContractInfo> {
    vec![
        info::<ObligationLedger>("obligations"),
        info::<ValidationTable>("validation"),
        info::<LimitsTable>("limits"),
        info::<GateList>("gates"),
        info::<CoreCoverage>("core-coverage"),
        info::<RecipeIr>("recipe-ir"),
        info::<AuditReport>("audit-report"),
    ]
}

/// Writes (when `write`) or compares every schema under `schema_dir`; returns the files whose
/// committed text differs from the generated one.
pub fn sync_schemas(schema_dir: &Path, write: bool) -> Result<Vec<PathBuf>, ContractError> {
    let mut stale = Vec::new();
    for c in all() {
        let path = schema_dir.join(c.schema_file);
        let want = (c.schema)();
        let have = match std::fs::read_to_string(&path) {
            Ok(text) => Some(text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(source) => return Err(ContractError::Io { path, source }),
        };
        if have.as_deref() != Some(want.as_str()) {
            if write {
                std::fs::write(&path, &want).map_err(|source| ContractError::Io {
                    path: path.clone(),
                    source,
                })?;
            }
            stale.push(path);
        }
    }
    Ok(stale)
}

/// The stellar-core a table was measured on or read from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CoreRef {
    /// The release tag, `v30.0.0`.
    pub version: String,
    /// The full commit sha the tag names.
    pub commit: String,
    /// The image that ran, absent for tables read from source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<ImageRef>,
}

/// A container image: a registry image with its manifest digest, or a local build with its
/// image id in `digest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImageRef {
    pub repository: String,
    pub tag: String,
    /// `sha256:<64 hex>`.
    pub digest: String,
}

impl CoreRef {
    fn problems(&self, out: &mut Vec<String>) {
        if !self.version.starts_with('v') {
            out.push(format!(
                "core version {:?} is not a release tag like v30.0.0",
                self.version
            ));
        }
        if !is_hex(&self.commit, 40) {
            out.push(format!(
                "core commit {:?} is not a 40 digit hex sha",
                self.commit
            ));
        }
        if let Some(image) = &self.image {
            let ok = image
                .digest
                .strip_prefix("sha256:")
                .is_some_and(|h| is_hex(h, 64));
            if !ok {
                out.push(format!(
                    "image digest {:?} is not sha256:<64 hex>",
                    image.digest
                ));
            }
        }
    }
}

/// Where a submitted transaction was refused or failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    /// Refused by core's `tx` endpoint; the transaction never reached a ledger.
    Submit,
    /// Included in a ledger and failed there.
    Apply,
}

pub(crate) fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Problems with a list that must be sorted ascending without repeats, by `key`.
pub(crate) fn sorted_unique<T, K: Ord + std::fmt::Debug>(
    what: &str,
    items: &[T],
    key: impl Fn(&T) -> K,
    out: &mut Vec<String>,
) {
    for pair in items.windows(2) {
        let (a, b) = (key(&pair[0]), key(&pair[1]));
        if a == b {
            out.push(format!("{what} {a:?} appears twice"));
        } else if a > b {
            out.push(format!("{what} {b:?} comes after {a:?}; keep them sorted"));
        }
    }
}

/// A decimal integer as the tables write numbers too wide for JSON numbers.
pub(crate) fn parse_decimal(s: &str) -> Option<i128> {
    let digits = s.strip_prefix('-').unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if (digits.len() > 1 && digits.starts_with('0')) || s == "-0" {
        return None;
    }
    s.parse().ok()
}
