//! Aggregates `corpus/entries/**/entry.json` and `corpus/vectors/**/*.json` into the manifest.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::model::*;
use crate::validate::is_safe_relative;
use crate::{sha256_hex, xdr};

pub const ENTRIES_DIR: &str = "entries";
pub const VECTORS_DIR: &str = "vectors";
pub const ENTRY_FILE: &str = "entry.json";

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("walking {path}: {source}")]
    Walk {
        path: PathBuf,
        source: walkdir::Error,
    },
    #[error("reading {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("parsing {path}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("entry {id} is defined in both {first} and {second}")]
    DuplicateId {
        id: String,
        first: String,
        second: String,
    },
    #[error("{path}: {message}")]
    Invalid { path: String, message: String },
}

/// Reads every entry and vector file under `corpus` and returns the manifest, entries sorted
/// by id. Files are not hashed against the entries here; `validate` does that. A directory
/// under `entries/` whose name starts with a dot is not read: the tools stage new entries
/// in such directories (`.staged-*`, `.golden-import-*`) before they move them into place,
/// and one left by an interrupted run is not a committed entry.
pub fn build(
    corpus: &Path,
    corpus_version: &str,
    generated_with: GeneratedWith,
) -> Result<Manifest, BuildError> {
    let mut entries: BTreeMap<String, (String, Entry)> = BTreeMap::new();

    for file in json_files(&corpus.join(ENTRIES_DIR), true)? {
        if file.file_name().and_then(|n| n.to_str()) != Some(ENTRY_FILE) {
            continue;
        }
        let rel = relative(corpus, &file)?;
        let entry: Entry = read_json(&file)?;
        let dir = rel.rsplit_once('/').map_or("", |(d, _)| d).to_string();
        for (name, v) in entry.variants.iter() {
            let files = [
                Some(&v.xdr_file),
                v.expected_json_file.as_ref(),
                v.oracle_file.as_ref(),
            ];
            for path in files.into_iter().flatten() {
                if !is_safe_relative(path) || !path.starts_with(&format!("{dir}/")) {
                    return Err(BuildError::Invalid {
                        path: rel,
                        message: format!("{name} file {path} is not inside {dir}/"),
                    });
                }
            }
        }
        if entry.vector.is_some() {
            return Err(BuildError::Invalid {
                path: rel,
                message: "an entry.json cannot name a vector file".into(),
            });
        }
        insert(&mut entries, rel, entry)?;
    }

    for file in json_files(&corpus.join(VECTORS_DIR), false)? {
        let rel = relative(corpus, &file)?;
        let bytes = std::fs::read(&file).map_err(|source| BuildError::Read {
            path: file.clone(),
            source,
        })?;
        let vf: VectorFile =
            serde_json::from_slice(&bytes).map_err(|source| BuildError::Parse {
                path: file.clone(),
                source,
            })?;
        let mut entry = vf.entry;
        if entry.vector.is_some() {
            return Err(BuildError::Invalid {
                path: rel,
                message: "the entry inside a vector file cannot name a vector file".into(),
            });
        }
        entry.vector = Some(FileRef {
            path: rel.clone(),
            sha256: sha256_hex(&bytes),
        });
        insert(&mut entries, rel, entry)?;
    }

    let entries: Vec<Entry> = entries.into_values().map(|(_, e)| e).collect();
    let protocol_tiers = entries
        .iter()
        .filter_map(|e| e.protocol.generated_at)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(Manifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        corpus_version: corpus_version.to_string(),
        generated_with,
        protocol_tiers,
        entries,
    })
}

/// The manifest as written to disk: sorted keys, two-space indent, trailing newline.
pub fn render(manifest: &Manifest) -> String {
    xdr::render_json(&serde_json::to_value(manifest).expect("the manifest is plain json"))
}

pub fn load(path: &Path) -> Result<Manifest, BuildError> {
    read_json(path)
}

fn insert(
    entries: &mut BTreeMap<String, (String, Entry)>,
    rel: String,
    entry: Entry,
) -> Result<(), BuildError> {
    if let Some((first, _)) = entries.get(&entry.id) {
        return Err(BuildError::DuplicateId {
            id: entry.id.clone(),
            first: first.clone(),
            second: rel,
        });
    }
    entries.insert(entry.id.clone(), (rel, entry));
    Ok(())
}

/// Every `.json` file under `dir`, at any depth; with `skip_staging`, none inside a directory
/// whose name starts with a dot.
fn json_files(dir: &Path, skip_staging: bool) -> Result<Vec<PathBuf>, BuildError> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    let walk = WalkDir::new(dir).sort_by_file_name().into_iter();
    for item in walk.filter_entry(|e| {
        !(skip_staging
            && e.depth() > 0
            && e.file_type().is_dir()
            && e.file_name().to_string_lossy().starts_with('.'))
    }) {
        let item = item.map_err(|source| BuildError::Walk {
            path: dir.to_path_buf(),
            source,
        })?;
        if item.file_type().is_file() && item.path().extension().is_some_and(|x| x == "json") {
            files.push(item.into_path());
        }
    }
    Ok(files)
}

fn relative(corpus: &Path, file: &Path) -> Result<String, BuildError> {
    let rel = file.strip_prefix(corpus).map_err(|_| BuildError::Invalid {
        path: file.display().to_string(),
        message: "is outside the corpus directory".into(),
    })?;
    let parts: Option<Vec<&str>> = rel.components().map(|c| c.as_os_str().to_str()).collect();
    parts
        .map(|p| p.join("/"))
        .ok_or_else(|| BuildError::Invalid {
            path: file.display().to_string(),
            message: "path is not valid UTF-8".into(),
        })
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, BuildError> {
    let bytes = std::fs::read(path).map_err(|source| BuildError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| BuildError::Parse {
        path: path.to_path_buf(),
        source,
    })
}
