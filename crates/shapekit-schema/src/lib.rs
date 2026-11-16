//! The stellar-xdr definitions as data.
//!
//! The model reads the JSON that the upstream generator (`generator-definitions-json` in
//! rs-stellar-xdr) writes from the `.x` files, the same file the `stellar-xdr` crate ships as
//! `xdr-definitions-json/xdr.json`. `scripts/fetch-xdr-definitions.sh` writes it to
//! `schema/xdr/<tag>/xdr.json`; the pinned tag is embedded here as [`Schema::pinned`].

pub mod fragment;
pub mod gaps;
pub mod graph;
pub mod history;
pub mod interp;
pub mod names;
pub mod vectors;

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub use graph::{Graph, Stats};

/// The stellar-xdr tag whose definitions [`Schema::pinned`] returns.
pub const PINNED_TAG: &str = "v30.0";

const PINNED_JSON: &str = include_str!("../../../schema/xdr/v30.0/xdr.json");

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reading {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("parsing the definitions json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the definitions json has version {0}, this model reads version 1")]
    Version(u32),
    #[error("{0} is defined twice")]
    Duplicate(String),
    #[error("{from} refers to {to}, which is not defined")]
    Dangling { from: String, to: String },
    #[error("no definition named {0}")]
    Unknown(String),
}

/// One `.x` file the definitions were read from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFile {
    pub name: String,
    pub sha256: String,
}

/// A type as written in a field, union arm or typedef.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Type {
    Int,
    UnsignedInt,
    Hyper,
    UnsignedHyper,
    Float,
    Double,
    Bool,
    OpaqueFixed {
        size: u64,
    },
    OpaqueVar {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_size: Option<u64>,
    },
    String {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_size: Option<u64>,
    },
    Ref {
        name: String,
    },
    Array {
        element: Box<Type>,
        count: u64,
    },
    VarArray {
        element: Box<Type>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_count: Option<u64>,
    },
    Optional {
        element: Box<Type>,
    },
}

impl Type {
    /// Calls `f` with every definition name this type refers to, nested ones included.
    pub fn for_each_ref<'a>(&'a self, f: &mut impl FnMut(&'a str)) {
        match self {
            Type::Ref { name } => f(name),
            Type::Array { element, .. }
            | Type::VarArray { element, .. }
            | Type::Optional { element } => element.for_each_ref(f),
            _ => {}
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Case {
    pub value: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// A union arm: the discriminant values that select it and its type (`None` is void).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Arm {
    pub cases: Vec<Case>,
    pub name: Option<String>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<Type>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub name: String,
    pub value: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Struct {
    pub name: String,
    pub fixed_size: Option<u32>,
    pub source: String,
    pub file_index: usize,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Union {
    pub name: String,
    pub fixed_size: Option<u32>,
    pub source: String,
    pub file_index: usize,
    pub discriminant: Field,
    pub arms: Vec<Arm>,
}

impl Union {
    /// The arm a discriminant value selects, or `None` when the value names no arm.
    pub fn arm(&self, value: i64) -> Option<&Arm> {
        self.arms
            .iter()
            .find(|a| a.cases.iter().any(|c| c.value == value))
    }

    /// Every discriminant value with an arm, in declaration order.
    pub fn case_values(&self) -> impl Iterator<Item = i64> + '_ {
        self.arms
            .iter()
            .flat_map(|a| a.cases.iter().map(|c| c.value))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Enum {
    pub name: String,
    pub source: String,
    pub file_index: usize,
    pub member_prefix: String,
    pub members: Vec<Member>,
}

impl Enum {
    pub fn has_value(&self, value: i64) -> bool {
        self.members.iter().any(|m| m.value == value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Typedef {
    pub name: String,
    pub source: String,
    pub file_index: usize,
    #[serde(rename = "type")]
    pub ty: Type,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Const {
    pub name: String,
    pub value: i64,
    pub source: String,
    pub file_index: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Definition {
    Struct(Struct),
    Union(Union),
    Enum(Enum),
    Typedef(Typedef),
    Const(Const),
}

impl Definition {
    pub fn name(&self) -> &str {
        match self {
            Definition::Struct(d) => &d.name,
            Definition::Union(d) => &d.name,
            Definition::Enum(d) => &d.name,
            Definition::Typedef(d) => &d.name,
            Definition::Const(d) => &d.name,
        }
    }

    pub fn kind(&self) -> Kind {
        match self {
            Definition::Struct(_) => Kind::Struct,
            Definition::Union(_) => Kind::Union,
            Definition::Enum(_) => Kind::Enum,
            Definition::Typedef(_) => Kind::Typedef,
            Definition::Const(_) => Kind::Const,
        }
    }

    pub fn source(&self) -> &str {
        match self {
            Definition::Struct(d) => &d.source,
            Definition::Union(d) => &d.source,
            Definition::Enum(d) => &d.source,
            Definition::Typedef(d) => &d.source,
            Definition::Const(d) => &d.source,
        }
    }

    pub fn file_index(&self) -> usize {
        match self {
            Definition::Struct(d) => d.file_index,
            Definition::Union(d) => d.file_index,
            Definition::Enum(d) => d.file_index,
            Definition::Typedef(d) => d.file_index,
            Definition::Const(d) => d.file_index,
        }
    }

    /// The types this definition is built from, a union's discriminant included.
    pub fn component_types(&self) -> Vec<&Type> {
        match self {
            Definition::Struct(s) => s.fields.iter().map(|f| &f.ty).collect(),
            Definition::Union(u) => std::iter::once(&u.discriminant.ty)
                .chain(u.arms.iter().filter_map(|a| a.ty.as_ref()))
                .collect(),
            Definition::Typedef(t) => vec![&t.ty],
            Definition::Enum(_) | Definition::Const(_) => Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Struct,
    Union,
    Enum,
    Typedef,
    Const,
}

impl Kind {
    pub const ALL: [Kind; 5] = [
        Kind::Struct,
        Kind::Union,
        Kind::Enum,
        Kind::Typedef,
        Kind::Const,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Struct => "struct",
            Kind::Union => "union",
            Kind::Enum => "enum",
            Kind::Typedef => "typedef",
            Kind::Const => "const",
        }
    }
}

#[derive(Deserialize)]
struct Raw {
    version: u32,
    files: Vec<SourceFile>,
    resolved_features: Vec<String>,
    definitions: Vec<Definition>,
}

/// The definitions of one stellar-xdr revision.
#[derive(Clone, Debug)]
pub struct Schema {
    files: Vec<SourceFile>,
    resolved_features: Vec<String>,
    definitions: Vec<Definition>,
    index: BTreeMap<String, usize>,
}

impl Schema {
    /// The definitions of the pinned stellar-xdr tag ([`PINNED_TAG`]).
    pub fn pinned() -> Schema {
        // the embedded file is checked by this crate's tests, so a failure here is a build defect
        Schema::from_json(PINNED_JSON).expect("the embedded schema/xdr json is valid")
    }

    pub fn load(path: &Path) -> Result<Schema, Error> {
        let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.display().to_string(),
            source,
        })?;
        Schema::from_json(&text)
    }

    /// Parses the generator output and checks that every reference resolves.
    pub fn from_json(text: &str) -> Result<Schema, Error> {
        let raw: Raw = serde_json::from_str(text)?;
        if raw.version != 1 {
            return Err(Error::Version(raw.version));
        }
        let mut index = BTreeMap::new();
        for (i, d) in raw.definitions.iter().enumerate() {
            if index.insert(d.name().to_string(), i).is_some() {
                return Err(Error::Duplicate(d.name().to_string()));
            }
        }
        let schema = Schema {
            files: raw.files,
            resolved_features: raw.resolved_features,
            definitions: raw.definitions,
            index,
        };
        for d in &schema.definitions {
            let mut missing = None;
            for t in d.component_types() {
                t.for_each_ref(&mut |n| {
                    if missing.is_none() && !schema.index.contains_key(n) {
                        missing = Some(n.to_string());
                    }
                });
            }
            if let Some(to) = missing {
                return Err(Error::Dangling {
                    from: d.name().to_string(),
                    to,
                });
            }
        }
        Ok(schema)
    }

    pub fn files(&self) -> &[SourceFile] {
        &self.files
    }

    pub fn resolved_features(&self) -> &[String] {
        &self.resolved_features
    }

    /// Every definition, in the order of the `.x` files.
    pub fn definitions(&self) -> &[Definition] {
        &self.definitions
    }

    pub fn get(&self, name: &str) -> Option<&Definition> {
        self.index.get(name).map(|&i| &self.definitions[i])
    }

    pub fn require(&self, name: &str) -> Result<&Definition, Error> {
        self.get(name)
            .ok_or_else(|| Error::Unknown(name.to_string()))
    }

    /// Follows typedefs until the type is no longer a reference to a typedef.
    pub fn resolve<'a>(&'a self, mut ty: &'a Type) -> &'a Type {
        while let Type::Ref { name } = ty {
            match self.get(name) {
                Some(Definition::Typedef(t)) => ty = &t.ty,
                _ => break,
            }
        }
        ty
    }

    /// The value of a named constant.
    pub fn constant(&self, name: &str) -> Option<i64> {
        match self.get(name) {
            Some(Definition::Const(c)) => Some(c.value),
            _ => None,
        }
    }

    pub fn graph(&self) -> Graph<'_> {
        Graph::new(self)
    }
}
