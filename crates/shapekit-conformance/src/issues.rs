//! `shapekit conformance draft-issues` and `file-issues`.
//!
//! Drafting writes one issue body per sdk and diverging entry into `findings/issues/`. Filing
//! opens them upstream with the `gh` CLI. Filing opens public issues in other repositories, so
//! it refuses to run without `--confirm`, which states that the batch was reviewed.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;

use crate::compare::ResultClass;
use crate::plan;
use crate::runner::{self, CheckRecord, Matrix};
use crate::versions::VersionsFile;

// drafts link the corpus and docs at the release that holds them, so the links do not move
const SHAPEKIT_URL: &str = "https://github.com/ErenTopaal/shapekit";

/// The git tag of the shapekit release that carries corpus `version`.
pub fn release_tag(version: &str) -> String {
    format!("v{version}")
}

// a check's source starts with a corpus path (`corpus/... case 3`, `corpus/...: decode = ...`,
// `corpus/..., envelope 0 ...`); that path becomes a link, the rest stays text
fn link_source(source: &str, tag: &str) -> String {
    if !source.starts_with("corpus/") {
        return source.to_string();
    }
    let end = source.find([' ', ':', ',']).unwrap_or(source.len());
    let (path, rest) = source.split_at(end);
    format!("[`{path}`]({SHAPEKIT_URL}/blob/{tag}/{path}){rest}")
}

// sources naming cases of one file (`<path> case <n>`) become one line, `<path> cases 2, 13`
fn group_sources(sources: &[&str]) -> Vec<String> {
    let mut cases: BTreeMap<&str, BTreeSet<u64>> = BTreeMap::new();
    let mut other: BTreeSet<&str> = BTreeSet::new();
    for s in sources {
        match s
            .rsplit_once(" case ")
            .and_then(|(p, n)| n.parse::<u64>().ok().map(|n| (p, n)))
        {
            Some((path, n)) => {
                cases.entry(path).or_default().insert(n);
            }
            None => {
                other.insert(s);
            }
        }
    }
    let mut out: Vec<String> = cases
        .into_iter()
        .map(|(path, ns)| {
            let list: Vec<String> = ns.iter().map(u64::to_string).collect();
            let word = if list.len() == 1 { "case" } else { "cases" };
            format!("{path} {word} {}", list.join(", "))
        })
        .collect();
    out.extend(other.into_iter().map(str::to_string));
    out.sort();
    out
}

/// The marker every drafted body carries; filing searches upstream for it to avoid duplicates.
pub const MARKER: &str = "shapekit vector";
const FILED: &str = "filed.json";
const SHOWN_CHECKS: usize = 5;

pub struct DraftOptions {
    pub matrix: PathBuf,
    pub manifest: PathBuf,
    pub versions: PathBuf,
    pub out: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub file: String,
    pub sdk: String,
    pub entry: String,
    pub repo: String,
    pub title: String,
    pub body: String,
}

pub fn file_name(sdk: &str, entry: &str) -> String {
    format!("{sdk}-{entry}.md")
}

/// Every draft the matrix calls for, in file-name order.
pub fn drafts(
    matrix: &Matrix,
    manifest: &shapekit_manifest::Manifest,
    corpus: &Path,
    versions: &VersionsFile,
) -> Result<Vec<Draft>> {
    let mut out = Vec::new();
    for row in &matrix.entries {
        // sdk -> versions that diverge, with their failing checks
        let mut by_sdk: BTreeMap<String, Vec<(String, &CheckRecord)>> = BTreeMap::new();
        for t in &matrix.testees {
            let Some(cell) = row.cells.get(&t.key) else {
                continue;
            };
            if !matches!(
                cell.result,
                ResultClass::DivergesFromNetwork | ResultClass::DivergesFromPeers
            ) {
                continue;
            }
            let list = by_sdk.entry(t.sdk.clone()).or_default();
            for ch in cell.checks.iter().filter(|c| {
                matches!(
                    c.result,
                    ResultClass::DivergesFromNetwork | ResultClass::DivergesFromPeers
                )
            }) {
                list.push((t.version.clone(), ch));
            }
        }
        if by_sdk.is_empty() {
            continue;
        }
        let entry = manifest
            .entries
            .iter()
            .find(|e| e.id == row.id)
            .with_context(|| format!("matrix entry {} is not in the manifest", row.id))?;
        let checks = plan::checks(entry, corpus)?;
        let affected: BTreeSet<&String> = by_sdk.keys().collect();
        for (sdk, failing) in &by_sdk {
            let sdk_conf = versions.sdk(sdk)?;
            let ops: BTreeSet<&str> = failing.iter().map(|(_, c)| c.op.as_str()).collect();
            let repos: BTreeSet<&str> = ops
                .iter()
                .map(|op| {
                    sdk_conf
                        .issues_repo_by_op
                        .get(*op)
                        .map(String::as_str)
                        .unwrap_or(sdk_conf.issues_repo.as_str())
                })
                .collect();
            ensure!(
                repos.len() == 1,
                "{} on {sdk}: failing ops {ops:?} belong to different repositories {repos:?}",
                row.id
            );
            let repo = repos.into_iter().next().expect("one repo").to_string();
            let mut vers: Vec<&str> = failing.iter().map(|(v, _)| v.as_str()).collect();
            vers.sort_unstable();
            vers.dedup();
            let first = failing[0].1;
            let title = format!(
                "{} {}: {} ({})",
                sdk_conf.package,
                vers.join(", "),
                first
                    .detail
                    .as_deref()
                    .unwrap_or(first.result.as_str())
                    .trim_end_matches('.'),
                row.id
            );
            let tag = release_tag(&matrix.corpus_version);
            let mut body = String::new();
            let _ = writeln!(body, "## What happens\n");
            let against = if first.result == ResultClass::DivergesFromPeers {
                "the other Stellar SDKs (the protocol leaves this case to the implementation, so the SDKs are compared with each other)"
            } else {
                "the Stellar network"
            };
            let _ = writeln!(
                body,
                "On the input below, `{}` {} answers differently from {} in {} {}.\n",
                sdk_conf.package,
                vers.join(", "),
                against,
                failing.len(),
                if failing.len() == 1 {
                    "check"
                } else {
                    "checks"
                }
            );
            let _ = writeln!(
                body,
                "A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol]({SHAPEKIT_URL}/blob/{tag}/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.\n"
            );
            let _ = writeln!(body, "## Input\n");
            let _ = writeln!(
                body,
                "Corpus entry `{}` (family `{}`) of the [shapekit]({SHAPEKIT_URL}) corpus {}.\n",
                row.id, entry.family, matrix.corpus_version
            );
            // the same check failing on several versions is shown once, with its versions
            let mut by_check: Vec<(&CheckRecord, Vec<&str>)> = Vec::new();
            for (version, ch) in failing {
                match by_check
                    .iter_mut()
                    .find(|(c, _)| c.check == ch.check && c.actual == ch.actual)
                {
                    Some((_, vs)) => vs.push(version.as_str()),
                    None => by_check.push((ch, vec![version.as_str()])),
                }
            }
            for (ch, on) in by_check.iter().take(SHOWN_CHECKS) {
                let request = checks
                    .iter()
                    .find(|c| c.name == ch.check)
                    .map(|c| crate::compare::short(&Value::Object(c.request.clone())))
                    .unwrap_or(Value::Null);
                let _ = writeln!(
                    body,
                    "### `{}` on {}\n\n{}\n\n```json\n{}\n```\n\nExpected: `{}`\n\nActual: `{}`\n",
                    ch.check,
                    on.join(", "),
                    ch.detail.as_deref().unwrap_or(ch.result.as_str()),
                    block(&request)?,
                    inline(&ch.expected),
                    inline(&ch.actual)
                );
            }
            if by_check.len() > SHOWN_CHECKS {
                let _ = writeln!(
                    body,
                    "{} more checks fail; the command below lists every one.\n",
                    by_check.len() - SHOWN_CHECKS
                );
            }
            let _ = writeln!(body, "## Reproduce\n");
            let _ = writeln!(
                body,
                "From a checkout of {SHAPEKIT_URL} at tag `{tag}`:\n\n```sh\nshapekit conformance run --sdk {sdk} --version {} --entry {}\n```\n",
                vers[0], row.id
            );
            let _ = writeln!(body, "## Where the expected answers come from\n");
            let _ = writeln!(body, "The expected answers are recorded in:\n");
            let sources: Vec<&str> = failing.iter().map(|(_, c)| c.source.as_str()).collect();
            for s in group_sources(&sources) {
                let _ = writeln!(body, "- {}", link_source(&s, &tag));
            }
            if let Some(notes) = &entry.notes {
                let _ = writeln!(
                    body,
                    "\nHow the corpus entry was built and checked: {notes}"
                );
            }
            if !entry.linked_issues.is_empty() {
                let _ = writeln!(body, "\nRelated: {}", entry.linked_issues.join(", "));
            }
            let _ = writeln!(body, "\n## Sibling issues\n");
            let mut siblings = Vec::new();
            for s in affected.iter().filter(|s| *s != &sdk) {
                let conf = versions.sdk(s)?;
                siblings.push(format!(
                    "- `{}` ({}) diverges on the same entry.",
                    conf.package, conf.issues_repo
                ));
            }
            if siblings.is_empty() {
                let _ = writeln!(
                    body,
                    "None: of the SDKs tested, only this one diverges on this entry."
                );
            } else {
                for s in siblings {
                    let _ = writeln!(body, "{s}");
                }
            }
            let _ = writeln!(body, "\n{MARKER}: `{}`", row.id);
            out.push(Draft {
                file: file_name(sdk, &row.id),
                sdk: sdk.clone(),
                entry: row.id.clone(),
                repo,
                title,
                body,
            });
        }
    }
    out.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(out)
}

// one-line json for an issue body; long values are cut, the source file holds them whole
fn inline(v: &Value) -> String {
    const MAX: usize = 600;
    let text = v.to_string();
    if text.len() <= MAX {
        return text;
    }
    let mut cut = MAX;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!(
        "{}... ({} chars, see the source below)",
        &text[..cut],
        text.len()
    )
}

// a request for a code block: pretty when short, one cut line when deep or long
fn block(v: &Value) -> Result<String> {
    const MAX: usize = 2000;
    let compact = v.to_string();
    if compact.len() > MAX {
        let mut cut = MAX;
        while !compact.is_char_boundary(cut) {
            cut -= 1;
        }
        return Ok(format!(
            "{}... ({} chars; the corpus file holds the whole value)",
            &compact[..cut],
            compact.len()
        ));
    }
    let pretty = serde_json::to_string_pretty(v).context("rendering a request")?;
    Ok(if pretty.lines().count() > 60 {
        compact
    } else {
        pretty
    })
}

/// The file text: a header naming repository and title, then the body.
pub fn render(d: &Draft) -> String {
    format!(
        "<!-- repo: {} -->\n# {}\n\n{}",
        d.repo,
        d.title.replace('\n', " "),
        d.body
    )
}

/// Reads back a drafted file: repository, title, body and vector id.
pub fn parse(text: &str) -> Result<(String, String, String, String)> {
    let mut lines = text.splitn(3, '\n');
    let repo = lines
        .next()
        .and_then(|l| l.strip_prefix("<!-- repo: "))
        .and_then(|l| l.strip_suffix(" -->"))
        .context("first line is not <!-- repo: owner/name -->")?;
    let title = lines
        .next()
        .and_then(|l| l.strip_prefix("# "))
        .context("second line is not the # title")?;
    let body = lines.next().unwrap_or("").trim_start_matches('\n');
    let marker = format!("{MARKER}: `");
    let vector = body
        .lines()
        .rev()
        .find_map(|l| l.strip_prefix(&marker).and_then(|r| r.strip_suffix('`')))
        .context("the body has no vector marker line")?;
    Ok((
        repo.to_string(),
        title.to_string(),
        body.to_string(),
        vector.to_string(),
    ))
}

pub struct DraftReport {
    pub written: Vec<String>,
    pub removed: Vec<String>,
    /// Drafts left as they are, with the reason: the matrix does not cover every version they
    /// name, or the file was not written by this command.
    pub kept: Vec<(String, String)>,
}

/// The versions a drafted title names: `<package> <v1>, <v2>: <what> (<entry>)`.
pub fn title_versions(title: &str, package: &str) -> Option<BTreeSet<String>> {
    let (versions, _) = title
        .strip_prefix(package)?
        .strip_prefix(' ')?
        .split_once(": ")?;
    let set: BTreeSet<String> = versions.split(", ").map(str::to_string).collect();
    (!set.is_empty() && set.iter().all(|v| !v.is_empty() && !v.contains(' '))).then_some(set)
}

/// What to do with an existing file in the drafts directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Existing {
    /// Not a draft of an (sdk, entry) the matrix ran: leave it alone and say nothing.
    Outside,
    /// The matrix covers every version the draft names: replace or remove it.
    Covered,
    /// Leave it alone, for this reason.
    Keep(String),
}

/// Decides about one existing file. `covered` maps (sdk, entry) to the versions the matrix ran.
pub fn existing_draft(
    name: &str,
    text: &str,
    versions: &VersionsFile,
    covered: &BTreeMap<(String, String), BTreeSet<String>>,
) -> Existing {
    let Ok((_, title, _, vector)) = parse(text) else {
        return Existing::Keep("not written by draft-issues".into());
    };
    let Some((sdk, _)) = name.split_once('-') else {
        return Existing::Outside;
    };
    if name != file_name(sdk, &vector) {
        return Existing::Outside;
    }
    let Some(ran) = covered.get(&(sdk.to_string(), vector.clone())) else {
        return Existing::Outside;
    };
    let Ok(conf) = versions.sdk(sdk) else {
        return Existing::Outside;
    };
    match title_versions(&title, &conf.package) {
        None => Existing::Keep("its title names no versions to compare with the matrix".into()),
        Some(named) => {
            let outside: Vec<&str> = named
                .iter()
                .filter(|v| !ran.contains(*v))
                .map(String::as_str)
                .collect();
            if outside.is_empty() {
                Existing::Covered
            } else {
                Existing::Keep(format!(
                    "it names {}, which the matrix did not run",
                    outside.join(", ")
                ))
            }
        }
    }
}

/// Writes the matrix's drafts. Only drafts of (sdk, entry, version) cells the matrix covers are
/// replaced or removed: a draft naming a version outside the matrix is kept as it is, and a new
/// draft for it is not written, so a narrow run never shrinks a draft of a wider one.
pub fn draft(opts: &DraftOptions) -> Result<DraftReport> {
    let matrix = runner::load_matrix(&opts.matrix)?;
    let (manifest, corpus) = runner::load_manifest(&opts.manifest)?;
    let versions = VersionsFile::load(&opts.versions)?;
    let drafts = drafts(&matrix, &manifest, &corpus, &versions)?;
    std::fs::create_dir_all(&opts.out)
        .with_context(|| format!("creating {}", opts.out.display()))?;
    let mut covered: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for row in &matrix.entries {
        for t in matrix
            .testees
            .iter()
            .filter(|t| row.cells.contains_key(&t.key))
        {
            covered
                .entry((t.sdk.clone(), row.id.clone()))
                .or_default()
                .insert(t.version.clone());
        }
    }
    let fresh: BTreeSet<&str> = drafts.iter().map(|d| d.file.as_str()).collect();
    let mut removed = Vec::new();
    let mut kept = Vec::new();
    let mut names = Vec::new();
    for item in
        std::fs::read_dir(&opts.out).with_context(|| format!("listing {}", opts.out.display()))?
    {
        let item = item.with_context(|| format!("listing {}", opts.out.display()))?;
        let name = item.file_name().to_string_lossy().to_string();
        if name.ends_with(".md") {
            names.push(name);
        }
    }
    names.sort();
    for name in names {
        let path = opts.out.join(&name);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        match existing_draft(&name, &text, &versions, &covered) {
            Existing::Outside => {}
            Existing::Covered if fresh.contains(name.as_str()) => {}
            Existing::Covered => {
                std::fs::remove_file(&path)
                    .with_context(|| format!("removing {}", path.display()))?;
                removed.push(name);
            }
            // a hand-written file is only reported when a draft would have replaced it
            Existing::Keep(why) if fresh.contains(name.as_str()) || parse(&text).is_ok() => {
                kept.push((name, why));
            }
            Existing::Keep(_) => {}
        }
    }
    let mut written = Vec::new();
    for d in &drafts {
        if kept.iter().any(|(k, _)| k == &d.file) {
            continue;
        }
        let path = opts.out.join(&d.file);
        std::fs::write(&path, render(d)).with_context(|| format!("writing {}", path.display()))?;
        written.push(d.file.clone());
    }
    Ok(DraftReport {
        written,
        removed,
        kept,
    })
}

pub struct FileOptions {
    pub issues: PathBuf,
    /// Only files whose name is listed; all drafts when empty.
    pub only: Vec<String>,
    /// Set by `--confirm`: the batch was reviewed and is to be filed upstream.
    pub confirm: bool,
}

/// Opens every drafted issue that has no upstream issue yet and records the links in
/// `filed.json`. Never runs without the confirmation flag.
pub fn file(opts: &FileOptions) -> Result<BTreeMap<String, String>> {
    if !opts.confirm {
        bail!(
            "file-issues opens public issues in upstream repositories; it runs only with --confirm, which confirms that this batch of drafts was reviewed and is to be filed upstream"
        );
    }
    let filed_path = opts.issues.join(FILED);
    let mut filed: BTreeMap<String, String> = match std::fs::read_to_string(&filed_path) {
        Ok(t) => {
            serde_json::from_str(&t).with_context(|| format!("parsing {}", filed_path.display()))?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
        Err(e) => return Err(e).with_context(|| format!("reading {}", filed_path.display())),
    };
    let mut names: Vec<String> = Vec::new();
    for item in std::fs::read_dir(&opts.issues)
        .with_context(|| format!("listing {}", opts.issues.display()))?
    {
        let name = item
            .with_context(|| format!("listing {}", opts.issues.display()))?
            .file_name()
            .to_string_lossy()
            .to_string();
        if name.ends_with(".md") && (opts.only.is_empty() || opts.only.contains(&name)) {
            names.push(name);
        }
    }
    names.sort();
    for name in &opts.only {
        ensure!(names.contains(name), "{name} is not a drafted issue");
    }
    for name in names {
        if filed.contains_key(&name) {
            continue;
        }
        let path = opts.issues.join(&name);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let (repo, title, body, vector) =
            parse(&text).with_context(|| format!("reading the draft {}", path.display()))?;
        // the title starts with the package, as `drafts` writes it
        let package = title.split_whitespace().next().unwrap_or("");
        let url = match existing(&repo, &vector, package)? {
            Some(url) => url,
            None => create(&repo, &title, &body)?,
        };
        println!("{name}: {url}");
        filed.insert(name, url);
        // written after every issue so a failure part way keeps the links already made
        std::fs::write(
            &filed_path,
            serde_json::to_string_pretty(&filed).context("rendering filed.json")? + "\n",
        )
        .with_context(|| format!("writing {}", filed_path.display()))?;
    }
    Ok(filed)
}

fn gh(args: &[&str]) -> Result<String> {
    let out = Command::new("gh")
        .args(args)
        .output()
        .with_context(|| format!("starting gh {}", args.join(" ")))?;
    ensure!(
        out.status.success(),
        "gh {} exited with {}: {}",
        args.join(" "),
        out.status,
        String::from_utf8_lossy(&out.stderr).trim()
    );
    String::from_utf8(out.stdout).context("gh printed non-utf-8 output")
}

// github search ignores punctuation, so `a.b` also finds `a.b-c`: the hits are only candidates
fn existing(repo: &str, vector: &str, package: &str) -> Result<Option<String>> {
    let query = format!("\"{MARKER}: `{vector}`\" in:body");
    let text = gh(&[
        "issue", "list", "--repo", repo, "--state", "all", "--search", &query, "--json",
        "url,body", "--limit", "50",
    ])?;
    let found: Vec<Value> = serde_json::from_str(&text).context("parsing gh issue list")?;
    Ok(matching_issue(&found, vector, package))
}

/// The first issue (`{url, body}` as gh lists them) whose body holds the exact marker line of
/// `vector` and names `package` the way drafted bodies do (`` `<package>` ``).
pub fn matching_issue(found: &[Value], vector: &str, package: &str) -> Option<String> {
    let marker = format!("{MARKER}: `{vector}`");
    let named = format!("`{package}`");
    found
        .iter()
        .find(|issue| {
            issue
                .get("body")
                .and_then(Value::as_str)
                .is_some_and(|b| b.lines().any(|l| l.trim() == marker) && b.contains(&named))
        })
        .and_then(|issue| issue.get("url"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn create(repo: &str, title: &str, body: &str) -> Result<String> {
    let mut tmp = tempfile::NamedTempFile::new().context("creating the body file")?;
    std::io::Write::write_all(&mut tmp, body.as_bytes()).context("writing the body file")?;
    let path = tmp.path().to_string_lossy().to_string();
    let out = gh(&[
        "issue",
        "create",
        "--repo",
        repo,
        "--title",
        title,
        "--body-file",
        &path,
    ])?;
    let url = out.trim().lines().last().unwrap_or("").to_string();
    ensure!(
        url.starts_with("https://"),
        "gh issue create printed no issue url: {out}"
    );
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::link_source;

    #[test]
    fn the_link_ends_where_the_corpus_path_ends() {
        let url = "https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/a/b.xdr";
        for (source, rest) in [
            ("corpus/a/b.xdr case 3", " case 3"),
            (
                "corpus/a/b.xdr: decode = must-accept",
                ": decode = must-accept",
            ),
            (
                "corpus/a/b.xdr, envelope 0 of the set",
                ", envelope 0 of the set",
            ),
            ("corpus/a/b.xdr", ""),
        ] {
            assert_eq!(
                link_source(source, "v0.2.0"),
                format!("[`corpus/a/b.xdr`]({url}){rest}")
            );
        }
        assert_eq!(
            link_source("stellar-core case 1", "v0.2.0"),
            "stellar-core case 1"
        );
    }
}
