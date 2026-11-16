//! Representation gap tables (`registry/gaps/<language>.toml`): where a value changes or is
//! lost between an XDR type and a language's native type, and the vectors that probe each
//! point. This module reads and checks the tables; `shapekit vectors gaps` turns their vectors
//! into conformance entries.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    pub schema: u32,
    pub language: String,
    pub sdk: String,
    #[serde(rename = "gap")]
    pub gaps: Vec<Gap>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gap {
    pub id: String,
    pub xdr: Vec<String>,
    pub native: String,
    pub loss: String,
    pub sources: Vec<String>,
    pub sdk_note: String,
    /// Why part or all of the loss has no vector.
    #[serde(default)]
    pub no_vector: Option<String>,
    #[serde(default, rename = "vector")]
    pub vectors: Vec<GapVector>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum GapVector {
    /// A textual or numeric id handed to the sdk's id api (testee op `id_parse`).
    IdParse {
        id_kind: IdKind,
        input: IdInput,
        expect: IdExpect,
        #[serde(default)]
        id: Option<String>,
        reason: String,
    },
    /// An ScVal map projected to a native value (testee op `to_native`), whose keys print
    /// alike once native.
    ToNative {
        map: Vec<MapEntry>,
        collides_as: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdKind {
    Muxed,
    Offer,
}

impl IdKind {
    pub fn as_str(self) -> &'static str {
        match self {
            IdKind::Muxed => "muxed",
            IdKind::Offer => "offer",
        }
    }
}

/// The input exactly as the vector hands it to the testee: JSON text or a JSON number.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum IdInput {
    Text(String),
    Number(serde_json::Number),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IdExpect {
    /// The sdk must refuse the input.
    Reject,
    /// The sdk must read exactly `id` or refuse; any other id is a loss.
    IdOrReject,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapEntry {
    pub key: MapValue,
    pub val: MapValue,
}

/// The few ScVal kinds the tables use as map keys and values.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum MapValue {
    U32(u32),
    Symbol(String),
    /// A string given by its bytes, so it can hold bytes that are not UTF-8.
    StringHex(String),
}

#[derive(Debug, thiserror::Error)]
pub enum GapError {
    #[error("reading {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("parsing {path}: {source}")]
    Toml {
        path: String,
        source: toml::de::Error,
    },
    #[error("{path}: {message}")]
    Invalid { path: String, message: String },
}

impl Table {
    pub fn load(path: &Path) -> Result<Table, GapError> {
        let p = path.display().to_string();
        let text = std::fs::read_to_string(path).map_err(|source| GapError::Io {
            path: p.clone(),
            source,
        })?;
        let table: Table = toml::from_str(&text).map_err(|source| GapError::Toml {
            path: p.clone(),
            source,
        })?;
        let problems = table.problems();
        if !problems.is_empty() {
            return Err(GapError::Invalid {
                path: p,
                message: problems.join("; "),
            });
        }
        Ok(table)
    }

    /// Every rule a table must keep: schema 1, the file named after its language, unique
    /// kebab-case ids, https sources, and every row with a vector or a reason it has none.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.schema != 1 {
            out.push(format!("schema {} is not 1", self.schema));
        }
        let mut seen = BTreeSet::new();
        for g in &self.gaps {
            let kebab = !g.id.is_empty()
                && g.id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
            if !kebab {
                out.push(format!("gap id {:?} is not lowercase kebab-case", g.id));
            }
            if !seen.insert(g.id.as_str()) {
                out.push(format!("gap {} appears twice", g.id));
            }
            if g.sources.is_empty() || g.sources.iter().any(|s| !s.starts_with("https://")) {
                out.push(format!("gap {} needs https sources", g.id));
            }
            if g.vectors.is_empty() && g.no_vector.is_none() {
                out.push(format!("gap {} has neither a vector nor no_vector", g.id));
            }
            for v in &g.vectors {
                match v {
                    GapVector::IdParse { expect, id, .. } => {
                        if (*expect == IdExpect::IdOrReject) != id.is_some() {
                            out.push(format!(
                                "gap {}: an id-or-reject vector names its id, a reject vector does not",
                                g.id
                            ));
                        }
                    }
                    GapVector::ToNative { map, .. } => {
                        if map.len() < 2 {
                            out.push(format!(
                                "gap {}: a to_native map needs two keys to collide",
                                g.id
                            ));
                        }
                    }
                }
            }
        }
        out
    }

    /// The obligation a row's vectors meet: `gap:<language>:<row>`.
    pub fn obligation(&self, gap: &Gap) -> String {
        format!("gap:{}:{}", self.language, gap.id)
    }
}
