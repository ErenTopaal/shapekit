//! The definitions of every stellar-xdr tag in `schema/xdr/`, in release order, with the first
//! tag each name appears in and the differences between two tags.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Serialize;

use crate::names::{case_name, field_name, member_name, rust_type_name};
use crate::{Definition, Error, Schema};

/// One tag's label, the protocol its major number names (`None` for a branch such as `next`)
/// and its definitions.
pub struct Tagged {
    pub label: String,
    pub protocol: Option<u32>,
    pub schema: Schema,
}

/// Every schema under a directory of `<label>/xdr.json` files, oldest first: tags `v<major>` or
/// `v<major>.<minor>` by version, then any other label (a branch) in name order.
pub struct History {
    pub tags: Vec<Tagged>,
}

fn version(label: &str) -> Option<(u32, u32)> {
    let rest = label.strip_prefix('v')?;
    let mut parts = rest.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = match parts.next() {
        Some(m) => m.parse().ok()?,
        None => 0,
    };
    parts.next().is_none().then_some((major, minor))
}

impl History {
    pub fn load(dir: &Path) -> Result<History, Error> {
        let io = |source| Error::Io {
            path: dir.display().to_string(),
            source,
        };
        let mut labels = Vec::new();
        for e in std::fs::read_dir(dir).map_err(io)? {
            let e = e.map_err(io)?;
            if e.path().join("xdr.json").is_file() {
                labels.push(e.file_name().to_string_lossy().into_owned());
            }
        }
        labels.sort_by(|a, b| match (version(a), version(b)) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.cmp(b),
        });
        let mut tags = Vec::new();
        for label in labels {
            let schema = Schema::load(&dir.join(&label).join("xdr.json"))?;
            tags.push(Tagged {
                protocol: version(&label).map(|v| v.0),
                label,
                schema,
            });
        }
        Ok(History { tags })
    }

    pub fn get(&self, label: &str) -> Option<&Tagged> {
        self.tags.iter().find(|t| t.label == label)
    }

    /// For each name of [`names`], the first tag that has it.
    pub fn first_seen(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for t in &self.tags {
            for n in names(&t.schema) {
                out.entry(n).or_insert_with(|| t.label.clone());
            }
        }
        out
    }
}

/// Every name a schema defines, in the spelling signatures use: `Type` for a definition (its
/// Rust name), `Type.field`, `Type:arm` and `Type=value`.
pub fn names(schema: &Schema) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for d in schema.definitions() {
        if matches!(d, Definition::Const(_)) {
            out.insert(format!("const {}", d.name()));
            continue;
        }
        let ty = rust_type_name(d.name());
        out.insert(ty.clone());
        match d {
            Definition::Struct(s) => {
                for f in &s.fields {
                    out.insert(format!("{ty}.{}", field_name(&f.name)));
                }
            }
            Definition::Union(u) => {
                for a in &u.arms {
                    for c in &a.cases {
                        out.insert(format!("{ty}:{}", case_name(schema, u, c)));
                    }
                }
            }
            Definition::Enum(e) => {
                for m in &e.members {
                    out.insert(format!("{ty}={}", member_name(&m.name, &e.member_prefix)));
                }
            }
            Definition::Typedef(_) | Definition::Const(_) => {}
        }
    }
    out
}

/// What changed from one schema to another.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Diff {
    /// Names (as [`names`] spells them) only in the newer schema.
    pub added: Vec<String>,
    /// Names only in the older schema.
    pub removed: Vec<String>,
    /// Definitions in both whose structure differs (source comments and file positions are
    /// ignored).
    pub changed: Vec<String>,
}

impl Diff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
}

/// A definition without the parts that do not affect the encoding.
fn structure(d: &Definition) -> serde_json::Value {
    let mut v = serde_json::to_value(d).expect("definitions serialize");
    if let Some(o) = v.as_object_mut() {
        o.remove("source");
        o.remove("file_index");
    }
    v
}

pub fn diff(old: &Schema, new: &Schema) -> Diff {
    let a = names(old);
    let b = names(new);
    let mut changed = Vec::new();
    for d in new.definitions() {
        if let Some(o) = old.get(d.name())
            && structure(o) != structure(d)
        {
            changed.push(rust_type_name(d.name()));
        }
    }
    changed.sort();
    Diff {
        added: b.difference(&a).cloned().collect(),
        removed: a.difference(&b).cloned().collect(),
        changed,
    }
}
