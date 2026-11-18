//! Loads and validates the shape, field, CAP and lint-rule registry under `registry/`.

mod check;
mod model;

use std::fmt;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

pub use model::*;

/// The only registry file format this build reads.
pub const SCHEMA_VERSION: u32 = 1;

/// Indexer-visible CAPs up to and including this protocol must be covered by a family.
pub const COVERED_THROUGH_PROTOCOL: u32 = 30;

/// The lint rule IDs the linter and the guide agree on.
pub const LINT_RULE_IDS: [&str; 3] = ["IDX001", "IDX002", "IDX003"];

pub const CAPS_FILE: &str = "caps.toml";

/// The `caps.toml` record of shapes that were in the XDR before the CAP process. It is not a
/// CAP: it tags corpus entries whose every witnessed arm predates every CAP, so no entry is
/// left untagged.
pub const PRE_CAP: &str = "PRE-CAP";
pub const FIELDS_FILE: &str = "fields.toml";
pub const LIMITS_FILE: &str = "caps-limits.toml";
pub const LINT_RULES_FILE: &str = "lint-rules.toml";
pub const FAMILIES_FILE: &str = "families.toml";

/// One thing wrong with the registry or a manifest, located well enough to fix it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Problem {
    pub location: String,
    pub message: String,
}

impl Problem {
    pub fn new(location: impl Into<String>, message: impl Into<String>) -> Self {
        Problem {
            location: location.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.location, self.message)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("reading {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("parsing {path}: {source}")]
    Parse {
        path: PathBuf,
        source: Box<toml::de::Error>,
    },
}

/// The parsed registry. `families` is `None` when `families.toml` does not exist yet,
/// which `check` reports as a problem rather than failing the load.
#[derive(Debug, Clone)]
pub struct Registry {
    pub caps: CapsFile,
    pub fields: FieldsFile,
    pub limits: LimitsFile,
    pub lint_rules: LintRulesFile,
    pub families: Option<FamiliesFile>,
}

impl Registry {
    pub fn load(dir: &Path) -> Result<Registry, LoadError> {
        let families_path = dir.join(FAMILIES_FILE);
        let families = if families_path.exists() {
            Some(load_file(&families_path)?)
        } else {
            None
        };
        Ok(Registry {
            caps: load_file(&dir.join(CAPS_FILE))?,
            fields: load_file(&dir.join(FIELDS_FILE))?,
            limits: load_file(&dir.join(LIMITS_FILE))?,
            lint_rules: load_file(&dir.join(LINT_RULES_FILE))?,
            families,
        })
    }

    /// Every consistency problem in the registry, in file order. Empty means clean.
    pub fn check(&self) -> Vec<Problem> {
        check::run(self)
    }

    pub fn cap(&self, id: &str) -> Option<&Cap> {
        self.caps.caps.iter().find(|c| c.id == id)
    }

    pub fn field(&self, id: &str) -> Option<&Field> {
        self.fields.fields.iter().find(|f| f.id == id)
    }

    pub fn family(&self, id: &str) -> Option<&Family> {
        self.families
            .as_ref()
            .and_then(|f| f.families.iter().find(|x| x.id == id))
    }
}

fn load_file<T: DeserializeOwned>(path: &Path) -> Result<T, LoadError> {
    let text = std::fs::read_to_string(path).map_err(|source| LoadError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| LoadError::Parse {
        path: path.to_path_buf(),
        source: Box::new(source),
    })
}
