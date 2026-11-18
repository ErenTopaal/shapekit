//! `conformance/known-failures/<sdk>.txt`.
//!
//! One line per known failure:
//!
//! ```text
//! <entry-id> <version|*> <result> <issue> <check>[, <check>...]
//! ```
//!
//! The first four fields are separated by whitespace; everything after the issue is the list of
//! check names, separated by commas, each written exactly as the matrix names it
//! (`testnet/decode_json`, `case 3/strkey_decode`). `<version>` is an exact version from
//! versions.toml, or `*` for every version, which is only right when every version fails the
//! same checks with the same result. `<result>` is what every listed check gives, with the kind
//! of failure where there is one: `diverges-from-network` for a wrong answer,
//! `diverges-from-network:panic` for a crash instead of an answer, and `error:<kind>` for an
//! error, the kind one of `bad-request`, `internal`, `timeout`, `exited`, `protocol` and
//! `unmappable` (see `compare::Outcome`). A line matches only its own kind, so a known wrong
//! answer does not cover a crash on the same check, nor a known bad request a timeout. Checks of
//! one entry that fail with different results go on separate lines. `<issue>` links the
//! upstream issue, or the drafted issue file under `findings/issues/` until it is filed.
//!
//! A `#` at the start of a line or after whitespace starts a comment, so an issue url such as
//! `https://github.com/o/r/pull/1#issuecomment-2` keeps its fragment.
//!
//! In a run, every check is judged on its own: a failing check listed with its result does not
//! fail the run; a failing check no line lists fails it, even when other checks of the same
//! entry are listed; a listed check that now has another result, passes, is unsupported or is
//! not a check of the entry fails it too, until the line is changed. Lines for entries or
//! versions outside the run are not checked, except that every line must name an entry that
//! exists and a version versions.toml lists.
//!
//! `diverges-from-peers` cannot be listed: implementation-defined checks are recorded as passes
//! with the answer kept (see `compare::judge_peers`), so no check ever has that result.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};

use crate::compare::{Outcome, ResultClass};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownLine {
    pub entry: String,
    /// `None` for `*`.
    pub version: Option<String>,
    pub result: Outcome,
    pub issue: String,
    /// The check names the line covers; never empty.
    pub checks: BTreeSet<String>,
    pub line: usize,
}

impl KnownLine {
    pub fn covers(&self, entry: &str, version: &str) -> bool {
        self.entry == entry && self.version.as_deref().is_none_or(|v| v == version)
    }

    pub fn covers_check(&self, entry: &str, version: &str, check: &str) -> bool {
        self.covers(entry, version) && self.checks.contains(check)
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

/// The line without its comment: a `#` counts only at the start or after whitespace, since
/// urls carry `#` inside a token.
pub fn strip_comment(raw: &str) -> &str {
    let mut prev_space = true;
    for (i, c) in raw.char_indices() {
        if c == '#' && prev_space {
            return &raw[..i];
        }
        prev_space = c.is_whitespace();
    }
    raw
}

// the next whitespace-separated token and the text after it
fn token(text: &str) -> Option<(&str, &str)> {
    let text = text.trim_start();
    if text.is_empty() {
        return None;
    }
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    Some((&text[..end], &text[end..]))
}

pub fn parse(text: &str) -> Result<Vec<KnownLine>> {
    const FORMAT: &str = "`<entry-id> <version|*> <result> <issue> <check>[, <check>...]`";
    let mut out: Vec<KnownLine> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let content = strip_comment(raw).trim();
        if content.is_empty() {
            continue;
        }
        let mut rest = content;
        let mut fields = Vec::with_capacity(4);
        for _ in 0..4 {
            let Some((t, r)) = token(rest) else {
                bail!("line {line}: expected {FORMAT}, got {content:?}");
            };
            fields.push(t);
            rest = r;
        }
        let [entry, version, result, issue] = fields[..] else {
            unreachable!("four fields were read");
        };
        let mut checks = BTreeSet::new();
        for c in rest.split(',') {
            let c = c.split_whitespace().collect::<Vec<_>>().join(" ");
            ensure!(
                !c.is_empty(),
                "line {line}: expected {FORMAT}; the check list is missing or has an empty item"
            );
            ensure!(checks.insert(c.clone()), "line {line}: lists {c:?} twice");
        }
        let result = match Outcome::parse(result) {
            Ok(r) if r.result == ResultClass::DivergesFromPeers => bail!(
                "line {line}: no check is diverges-from-peers: implementation-defined checks are recorded as passes"
            ),
            Ok(r) if r.is_failure() => r,
            Ok(r) => bail!("line {line}: {r} is not a failure and cannot be listed"),
            Err(why) => bail!("line {line}: {why}"),
        };
        let version = (version != "*").then(|| version.to_string());
        if let Some((prev, shared)) = out.iter().find_map(|k| {
            let same_version = k.version.is_none() || version.is_none() || k.version == version;
            let shared = k.checks.intersection(&checks).next();
            (k.entry == entry && same_version)
                .then_some(shared)
                .flatten()
                .map(|c| (k, c))
        }) {
            bail!(
                "line {line}: {entry} {shared:?} is already listed on line {} for the same version",
                prev.line
            );
        }
        out.push(KnownLine {
            entry: entry.to_string(),
            version,
            result,
            issue: issue.to_string(),
            checks,
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
        check: String,
        result: Outcome,
    },
    WrongResult {
        entry: String,
        version: String,
        check: String,
        listed: Outcome,
        actual: Outcome,
        line: usize,
    },
    NoLongerFails {
        entry: String,
        version: String,
        check: String,
        actual: Outcome,
        line: usize,
    },
    UnknownCheck {
        entry: String,
        version: String,
        check: String,
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

impl Problem {
    /// The sdk version the problem is about; `None` for a line that names no version.
    pub fn version(&self) -> Option<&str> {
        match self {
            Problem::Unlisted { version, .. }
            | Problem::WrongResult { version, .. }
            | Problem::NoLongerFails { version, .. }
            | Problem::UnknownCheck { version, .. }
            | Problem::UnknownVersion { version, .. } => Some(version),
            Problem::UnknownEntry { .. } => None,
        }
    }
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Problem::Unlisted {
                entry,
                version,
                check,
                result,
            } => write!(
                f,
                "{entry} on {version}: `{check}` is {result} and not in the known-failures list"
            ),
            Problem::WrongResult {
                entry,
                version,
                check,
                listed,
                actual,
                line,
            } => write!(
                f,
                "{entry} on {version}: `{check}` is listed as {listed} (line {line}) but is {actual}"
            ),
            Problem::NoLongerFails {
                entry,
                version,
                check,
                actual,
                line,
            } => write!(
                f,
                "{entry} on {version}: `{check}` is listed as a failure (line {line}) but is {actual}; remove it from the line"
            ),
            Problem::UnknownCheck {
                entry,
                version,
                check,
                line,
            } => write!(
                f,
                "{entry} on {version}: line {line} lists `{check}`, which is not a check of this entry"
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

/// Every check of one entry on one version, by name, with its result and kind of failure
/// (passes included).
pub type CellChecks = BTreeMap<String, Outcome>;

/// Checks one sdk's results against its list. `results` holds every (entry, version) the run
/// covered with all of its checks; `all_entries` is every id in the manifest; `all_versions`
/// every version of the sdk in versions.toml.
pub fn evaluate(
    lines: &[KnownLine],
    results: &BTreeMap<(String, String), CellChecks>,
    all_entries: &BTreeSet<String>,
    all_versions: &BTreeSet<String>,
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
    for ((entry, version), checks) in results {
        let here: Vec<&KnownLine> = lines.iter().filter(|l| l.covers(entry, version)).collect();
        for (check, &actual) in checks {
            let line = here.iter().find(|l| l.checks.contains(check));
            let at = || (entry.clone(), version.clone(), check.clone());
            match (line, actual.is_failure()) {
                (None, true) => {
                    let (entry, version, check) = at();
                    problems.push(Problem::Unlisted {
                        entry,
                        version,
                        check,
                        result: actual,
                    });
                }
                (None, false) => {}
                (Some(l), true) if l.result == actual => {}
                (Some(l), true) => {
                    let (entry, version, check) = at();
                    problems.push(Problem::WrongResult {
                        entry,
                        version,
                        check,
                        listed: l.result,
                        actual,
                        line: l.line,
                    });
                }
                (Some(l), false) => {
                    let (entry, version, check) = at();
                    problems.push(Problem::NoLongerFails {
                        entry,
                        version,
                        check,
                        actual,
                        line: l.line,
                    });
                }
            }
        }
        for l in &here {
            for c in l.checks.iter().filter(|c| !checks.contains_key(*c)) {
                problems.push(Problem::UnknownCheck {
                    entry: entry.clone(),
                    version: version.clone(),
                    check: c.clone(),
                    line: l.line,
                });
            }
        }
    }
    problems
}

/// The issues of the lines that cover any of an (entry, version)'s failing checks, joined,
/// or `None` when no line covers one.
pub fn issue_for<'a>(
    lines: &[KnownLine],
    entry: &str,
    version: &str,
    failing: impl IntoIterator<Item = &'a str>,
) -> Option<String> {
    let mut issues: Vec<&str> = Vec::new();
    for check in failing {
        for l in lines
            .iter()
            .filter(|l| l.covers_check(entry, version, check))
        {
            if !issues.contains(&l.issue.as_str()) {
                issues.push(&l.issue);
            }
        }
    }
    (!issues.is_empty()).then(|| issues.join(", "))
}
