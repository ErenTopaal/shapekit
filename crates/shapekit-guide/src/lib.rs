//! The indexer-hardening guide (`docs/guide/indexer-hardening.mdx`) carries four generated
//! regions between `{/* shapekit:begin <name> */}` and `{/* shapekit:end <name> */}` lines:
//! the field table from `registry/fields.toml`, the corpus entries that exercise each field
//! from `corpus/manifest.json`, the caps table from `registry/caps-limits.toml`, and the
//! checklist from `registry/lint-rules.toml` plus `registry/guide-checklist.toml`.
//!
//! `apply` rewrites those regions; `check` reports regions that differ from what the registry
//! and manifest render to, fields core does not fully validate that no corpus entry exercises,
//! and corpus entries naming a field that is not in the field table.

pub mod checklist;
mod mdx;
mod render;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use shapekit_manifest::{Entry, Manifest};
use shapekit_registry::{Problem, Registry, ValidatedByCore};

pub use checklist::{CHECKLIST_FILE, ChecklistFile, ChecklistItem};
pub use render::entry_path;

pub const FIELD_TABLE: &str = "field-table";
pub const CORPUS_BY_FIELD: &str = "corpus-by-field";
pub const CAPS_TABLE: &str = "caps-table";
pub const CHECKLIST: &str = "checklist";

/// Every generated region, in the order they appear in the guide.
pub const REGIONS: [&str; 4] = [FIELD_TABLE, CORPUS_BY_FIELD, CAPS_TABLE, CHECKLIST];

/// The anchor of the guide heading the corpus region sits under; field table rows link to it.
pub const CORPUS_ANCHOR: &str = "corpus-entries-by-field";

/// How corpus entries are linked from the guide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorpusLinks {
    /// A path from the guide's directory to the corpus directory, for the copy in this repo.
    Relative(String),
    /// An absolute URL of the corpus directory, for a copy published elsewhere.
    Url(String),
    /// Entry ids as plain code, for a copy that has no corpus to link to.
    Plain,
}

/// What the generated regions are rendered from.
pub struct Sources<'a> {
    pub registry: &'a Registry,
    pub manifest: &'a Manifest,
    pub checklist: &'a ChecklistFile,
    pub links: &'a CorpusLinks,
}

#[derive(Debug, thiserror::Error)]
pub enum GuideError {
    #[error("reading {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("parsing {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("guide markers: {0}")]
    Markers(String),
    #[error("cannot render: {0}")]
    Unrenderable(String),
    #[error("corpus entry: {0}")]
    EntryFile(String),
}

/// A field that core does not fully validate and that no corpus entry exercises.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uncovered {
    pub field: String,
    pub validated_by_core: ValidatedByCore,
}

/// A corpus entry naming a field that the guide's field table does not list.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnknownField {
    pub entry: String,
    pub field: String,
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    /// Regions whose text in the guide differs from the rendered text.
    pub stale: Vec<&'static str>,
    pub uncovered: Vec<Uncovered>,
    pub unknown: Vec<UnknownField>,
    pub checklist: Vec<Problem>,
    /// Field ids found in the guide's field table.
    pub table_fields: usize,
    /// Fields that at least one corpus entry exercises.
    pub exercised_fields: usize,
}

impl Report {
    pub fn passed(&self) -> bool {
        self.stale.is_empty()
            && self.uncovered.is_empty()
            && self.unknown.is_empty()
            && self.checklist.is_empty()
    }
}

/// Corpus entries per field id, in manifest order (sorted by entry id).
pub fn entries_by_field(manifest: &Manifest) -> BTreeMap<&str, Vec<&Entry>> {
    let mut out: BTreeMap<&str, Vec<&Entry>> = BTreeMap::new();
    for e in &manifest.entries {
        let mut seen = BTreeSet::new();
        for f in &e.fields_exercised {
            if seen.insert(f.as_str()) {
                out.entry(f.as_str()).or_default().push(e);
            }
        }
    }
    out
}

/// The body of every region, each framed by one blank line on either side.
pub fn render(src: &Sources) -> Result<Vec<(&'static str, String)>, GuideError> {
    let by_field = entries_by_field(src.manifest);
    let bodies = [
        (FIELD_TABLE, render::field_table(src, &by_field)?),
        (CORPUS_BY_FIELD, render::corpus_by_field(src, &by_field)?),
        (CAPS_TABLE, render::caps_table(src)?),
        (CHECKLIST, render::checklist(src)?),
    ];
    Ok(bodies
        .into_iter()
        .map(|(name, body)| (name, format!("\n{body}\n")))
        .collect())
}

/// The guide with every region replaced by its rendered text.
pub fn apply(doc: &str, src: &Sources) -> Result<String, GuideError> {
    mdx::splice(doc, &render(src)?)
}

pub fn check(doc: &str, src: &Sources) -> Result<Report, GuideError> {
    let current = mdx::extract(doc, &REGIONS)?;
    let mut report = Report::default();
    for (name, body) in render(src)? {
        if current.get(name) != Some(&body) {
            report.stale.push(name);
        }
    }

    let by_field = entries_by_field(src.manifest);
    for f in &src.registry.fields.fields {
        if f.validated_by_core != ValidatedByCore::Yes && !by_field.contains_key(f.id.as_str()) {
            report.uncovered.push(Uncovered {
                field: f.id.clone(),
                validated_by_core: f.validated_by_core,
            });
        }
    }
    report.exercised_fields = src
        .registry
        .fields
        .fields
        .iter()
        .filter(|f| by_field.contains_key(f.id.as_str()))
        .count();

    // the table in the file, not the registry, is what a reader sees, so field ids come from it
    let table = table_field_ids(&current[FIELD_TABLE]);
    report.table_fields = table.len();
    for e in &src.manifest.entries {
        for f in &e.fields_exercised {
            if !table.contains(f.as_str()) {
                report.unknown.push(UnknownField {
                    entry: e.id.clone(),
                    field: f.clone(),
                });
            }
        }
    }
    report.unknown.sort();
    report.unknown.dedup();

    report.checklist = src.checklist.problems(src.registry);
    Ok(report)
}

/// The id in the first cell of every body row of the field table: the first code span.
fn table_field_ids(region: &str) -> BTreeSet<&str> {
    region
        .lines()
        .filter_map(|l| l.strip_prefix("| `"))
        .filter_map(|rest| rest.split_once('`').map(|(id, _)| id))
        .collect()
}

/// A forward-slash path from directory `from` to directory `to`, both absolute.
pub fn relative_dir(from: &std::path::Path, to: &std::path::Path) -> Option<String> {
    use std::path::Component;
    let from: Vec<Component> = from.components().collect();
    let to: Vec<Component> = to.components().collect();
    if from.first() != to.first() {
        return None;
    }
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec!["..".to_string(); from.len() - common];
    for c in &to[common..] {
        parts.push(c.as_os_str().to_str()?.to_string());
    }
    if parts.is_empty() {
        Some(".".to_string())
    } else {
        Some(parts.join("/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn relative_dirs() {
        assert_eq!(
            relative_dir(Path::new("/r/docs/guide"), Path::new("/r/corpus")).unwrap(),
            "../../corpus"
        );
        assert_eq!(
            relative_dir(Path::new("/r"), Path::new("/r/corpus")).unwrap(),
            "corpus"
        );
        assert_eq!(relative_dir(Path::new("/r"), Path::new("/r")).unwrap(), ".");
    }

    #[test]
    fn reads_ids_from_table_rows() {
        let region = "\n| Field | x |\n| --- | --- |\n| `a.b`<br />`X` | y |\n| `c` | z |\n";
        let ids = table_field_ids(region);
        assert_eq!(ids.into_iter().collect::<Vec<_>>(), ["a.b", "c"]);
    }
}
