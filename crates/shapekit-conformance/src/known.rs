//! `conformance/known-failures/<sdk>.txt`.
//!
//! One line per known failure: `<entry-id> <version> <result> <issue>`, separated by
//! whitespace, `#` starts a comment. `<version>` is an exact version from versions.toml or `*`
//! for every version. `<result>` is the result the entry has (`diverges-from-network`,
//! `diverges-from-peers` or `error`). `<issue>` links the upstream issue, or the drafted issue
//! file under `findings/issues/` until it is filed.
//!
//! In a run: a failing entry with a matching line does not fail the run; a failing entry
//! without one fails it; a line whose entry now has another result, passes, or is unsupported
//! fails it too until the line is changed or removed. Lines for entries or versions outside
//! the run are not checked, except that every line must name an entry that exists.
//!
//! A `diverges-from-peers` result exists only when other sdks answer the same check, so a run
//! of one sdk cannot reproduce it: there such a line is not reported for passing, and it is
//! checked in full whenever the run holds more than one sdk.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::compare::ResultClass;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownLine {
    pub entry: String,
    /// `None` for `*`.
    pub version: Option<String>,
    pub result: ResultClass,
    pub issue: String,
    pub line: usize,
}

impl KnownLine {
    pub fn covers(&self, entry: &str, version: &str) -> bool {
        self.entry == entry && self.version.as_deref().is_none_or(|v| v == version)
    }
}

pub fn path(dir: &Path, sdk: &str) -> std::path::PathBuf {
    dir.join(format!("{sdk}.txt"))
}

/// Reads an sdk's list. A missing file is an empty list: an sdk with no known failures needs
/// no file.
pub fn load(dir: &Path, sdk: &str) -> Result<Vec<KnownLine>> {
    let file = path(dir, sdk);
    let text = match std::fs::read_to_string(&file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", file.display())),
    };
    parse(&text).with_context(|| format!("parsing {}", file.display()))
}

pub fn parse(text: &str) -> Result<Vec<KnownLine>> {
    let mut out: Vec<KnownLine> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let content = raw.split('#').next().unwrap_or("").trim();
        if content.is_empty() {
            continue;
        }
        let parts: Vec<&str> = content.split_whitespace().collect();
        let [entry, version, result, issue] = parts[..] else {
            bail!(
                "line {line}: expected `<entry-id> <version|*> <result> <issue>`, got {content:?}"
            );
        };
        let result = match ResultClass::parse(result) {
            Some(r) if r.is_failure() => r,
            Some(r) => bail!("line {line}: {r} is not a failure and cannot be listed"),
            None => bail!("line {line}: unknown result {result:?}"),
        };
        let version = (version != "*").then(|| version.to_string());
        if let Some(prev) = out.iter().find(|k| {
            k.entry == entry && (k.version.is_none() || version.is_none() || k.version == version)
        }) {
            bail!(
                "line {line}: {entry} overlaps line {} for the same version",
                prev.line
            );
        }
        out.push(KnownLine {
            entry: entry.to_string(),
            version,
            result,
            issue: issue.to_string(),
            line,
        });
    }
    Ok(out)
}

/// Why a run fails because of its known-failures list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    Unlisted {
        entry: String,
        version: String,
        result: ResultClass,
    },
    WrongResult {
        entry: String,
        version: String,
        listed: ResultClass,
        actual: ResultClass,
        line: usize,
    },
    NoLongerFails {
        entry: String,
        version: String,
        actual: ResultClass,
        line: usize,
    },
    UnknownEntry {
        entry: String,
        line: usize,
    },
    UnknownVersion {
        entry: String,
        version: String,
        line: usize,
    },
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Problem::Unlisted {
                entry,
                version,
                result,
            } => write!(
                f,
                "{entry} on {version}: {result} and not in the known-failures list"
            ),
            Problem::WrongResult {
                entry,
                version,
                listed,
                actual,
                line,
            } => write!(
                f,
                "{entry} on {version}: listed as {listed} (line {line}) but is {actual}"
            ),
            Problem::NoLongerFails {
                entry,
                version,
                actual,
                line,
            } => write!(
                f,
                "{entry} on {version}: listed as a failure (line {line}) but is {actual}; remove the line"
            ),
            Problem::UnknownEntry { entry, line } => {
                write!(f, "line {line}: {entry} is not in the manifest")
            }
            Problem::UnknownVersion {
                entry,
                version,
                line,
            } => write!(
                f,
                "line {line}: {entry} names version {version}, which versions.toml does not list"
            ),
        }
    }
}

/// Checks one sdk's results against its list. `results` holds every (entry, version) the run
/// covered; `all_entries` is every id in the manifest; `all_versions` every version of the sdk
/// in versions.toml; `other_sdks` says whether the run also held other sdks to compare with.
pub fn evaluate(
    lines: &[KnownLine],
    results: &BTreeMap<(String, String), ResultClass>,
    all_entries: &BTreeSet<String>,
    all_versions: &BTreeSet<String>,
    other_sdks: bool,
) -> Vec<Problem> {
    let mut problems = Vec::new();
    for l in lines {
        if !all_entries.contains(&l.entry) {
            problems.push(Problem::UnknownEntry {
                entry: l.entry.clone(),
                line: l.line,
            });
        }
        if let Some(v) = &l.version
            && !all_versions.contains(v)
        {
            problems.push(Problem::UnknownVersion {
                entry: l.entry.clone(),
                version: v.clone(),
                line: l.line,
            });
        }
    }
    for ((entry, version), &actual) in results {
        let line = lines.iter().find(|l| l.covers(entry, version));
        match (line, actual.is_failure()) {
            (None, true) => problems.push(Problem::Unlisted {
                entry: entry.clone(),
                version: version.clone(),
                result: actual,
            }),
            (None, false) => {}
            (Some(l), true) if l.result == actual => {}
            (Some(l), true) => problems.push(Problem::WrongResult {
                entry: entry.clone(),
                version: version.clone(),
                listed: l.result,
                actual,
                line: l.line,
            }),
            // without other sdks there were no peers to diverge from
            (Some(l), false) if l.result == ResultClass::DivergesFromPeers && !other_sdks => {}
            (Some(l), false) => problems.push(Problem::NoLongerFails {
                entry: entry.clone(),
                version: version.clone(),
                actual,
                line: l.line,
            }),
        }
    }
    problems
}

/// The issue a known-failures line gives for an (entry, version), if any.
pub fn issue_for<'a>(lines: &'a [KnownLine], entry: &str, version: &str) -> Option<&'a str> {
    lines
        .iter()
        .find(|l| l.covers(entry, version))
        .map(|l| l.issue.as_str())
}
