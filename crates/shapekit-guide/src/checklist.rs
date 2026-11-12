//! `registry/guide-checklist.toml`: the language-neutral checklist items the guide adds after
//! the three lint rules. Each item names the field rows and caps it rests on, so a renamed or
//! removed registry row fails `guide check` instead of leaving a dangling reference.

use std::path::Path;

use serde::Deserialize;
use shapekit_registry::{Problem, Registry};

use crate::GuideError;

pub const CHECKLIST_FILE: &str = "guide-checklist.toml";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChecklistFile {
    pub schema_version: u32,
    #[serde(default, rename = "item")]
    pub items: Vec<ChecklistItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChecklistItem {
    pub id: String,
    pub title: String,
    pub text: String,
    /// Ids of `fields.toml` rows the item applies to.
    #[serde(default)]
    pub fields: Vec<String>,
    /// Ids of `caps-limits.toml` limits the item applies to.
    #[serde(default)]
    pub limits: Vec<String>,
    /// Upstream sources for anything the item states that the registry rows do not cover.
    #[serde(default)]
    pub links: Vec<Link>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub label: String,
    pub url: String,
}

impl ChecklistFile {
    pub fn load(path: &Path) -> Result<ChecklistFile, GuideError> {
        let text = std::fs::read_to_string(path).map_err(|source| GuideError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        toml::from_str(&text).map_err(|source| GuideError::Parse {
            path: path.to_path_buf(),
            message: source.to_string(),
        })
    }

    /// Every item reference the registry cannot resolve, plus duplicate ids and empty text.
    pub fn problems(&self, registry: &Registry) -> Vec<Problem> {
        let mut out = Vec::new();
        if self.schema_version != shapekit_registry::SCHEMA_VERSION {
            out.push(Problem::new(
                CHECKLIST_FILE,
                format!(
                    "schema_version {} is not {}",
                    self.schema_version,
                    shapekit_registry::SCHEMA_VERSION
                ),
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        for item in &self.items {
            let loc = format!("{CHECKLIST_FILE} {}", item.id);
            if !seen.insert(item.id.as_str()) {
                out.push(Problem::new(&loc, "duplicate id"));
            }
            if item.title.trim().is_empty() || item.text.trim().is_empty() {
                out.push(Problem::new(&loc, "title and text must not be empty"));
            }
            if item.fields.is_empty() && item.limits.is_empty() && item.links.is_empty() {
                out.push(Problem::new(
                    &loc,
                    "names no field, limit or link to rest on",
                ));
            }
            for f in &item.fields {
                if registry.field(f).is_none() {
                    out.push(Problem::new(&loc, format!("unknown field {f}")));
                }
            }
            for l in &item.limits {
                if !registry.limits.limits.iter().any(|x| &x.id == l) {
                    out.push(Problem::new(&loc, format!("unknown limit {l}")));
                }
            }
            for link in &item.links {
                if !link.url.starts_with("https://") {
                    out.push(Problem::new(
                        &loc,
                        format!("link {} is not an https url", link.url),
                    ));
                }
            }
        }
        out
    }
}
