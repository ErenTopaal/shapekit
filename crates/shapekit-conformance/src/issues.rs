//! `shapekit conformance draft-issues` and `file-issues`.
//!
//! Drafting writes one issue body per sdk and diverging entry into `findings/issues/`. Filing
//! opens them upstream with the `gh` CLI. Filing is outward-facing, so it refuses to run
//! without `--i-have-owner-approval`, which the repo owner gives per batch.

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
            let mut body = String::new();
            let _ = writeln!(body, "## What happens\n");
            let _ = writeln!(
                body,
                "`{}` {} answers {} of the checks of corpus entry `{}` differently from {}.\n",
                sdk_conf.package,
                vers.join(", "),
                failing.len(),
                row.id,
                if first.result == ResultClass::DivergesFromPeers {
                    "the other SDKs (the network leaves this case to the implementation)"
                } else {
                    "the Stellar network"
                }
            );
            let _ = writeln!(body, "## Minimal vector\n");
            let _ = writeln!(
                body,
                "Entry `{}` (kind {:?}, family `{}`) of the shapekit corpus {}. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).\n",
                row.id, entry.kind, entry.family, matrix.corpus_version
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
                    "{} more failing checks are listed in `divergence-matrix.json`.\n",
                    by_check.len() - SHOWN_CHECKS
                );
            }
            let _ = writeln!(body, "## Reproduce\n");
            let _ = writeln!(
                body,
                "```sh\nshapekit conformance run --sdk {sdk} --version {} --entry {}\n```\n",
                vers[0], row.id
            );
            let _ = writeln!(body, "## Expected value and its source\n");
            let mut sources: Vec<&str> = failing.iter().map(|(_, c)| c.source.as_str()).collect();
            sources.sort_unstable();
            sources.dedup();
            for s in sources {
                let _ = writeln!(body, "- {s}");
            }
            if let Some(notes) = &entry.notes {
                let _ = writeln!(body, "\nEntry notes: {notes}");
            }
            if !entry.linked_issues.is_empty() {
                let _ = writeln!(body, "\nRelated: {}", entry.linked_issues.join(", "));
            }
            let _ = writeln!(body, "\n## Sibling issues\n");
            let siblings: Vec<String> = affected
                .iter()
                .filter(|s| *s != &sdk)
                .map(|s| format!("- {s}: `findings/issues/{}`", file_name(s, &row.id)))
                .collect();
            if siblings.is_empty() {
                let _ = writeln!(body, "None: only this SDK diverges on this entry.");
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
}

pub fn draft(opts: &DraftOptions) -> Result<DraftReport> {
    let matrix = runner::load_matrix(&opts.matrix)?;
    let (manifest, corpus) = runner::load_manifest(&opts.manifest)?;
    let versions = VersionsFile::load(&opts.versions)?;
    let drafts = drafts(&matrix, &manifest, &corpus, &versions)?;
    std::fs::create_dir_all(&opts.out)
        .with_context(|| format!("creating {}", opts.out.display()))?;
    let keep: BTreeSet<&str> = drafts.iter().map(|d| d.file.as_str()).collect();
    // a matrix run for one sdk says nothing about another sdk's drafts, so only those sdks are pruned
    let prefixes: BTreeSet<String> = matrix
        .testees
        .iter()
        .map(|t| format!("{}-", t.sdk))
        .collect();
    let mut removed = Vec::new();
    for item in
        std::fs::read_dir(&opts.out).with_context(|| format!("listing {}", opts.out.display()))?
    {
        let item = item.with_context(|| format!("listing {}", opts.out.display()))?;
        let name = item.file_name().to_string_lossy().to_string();
        if !name.ends_with(".md")
            || keep.contains(name.as_str())
            || !prefixes.iter().any(|p| name.starts_with(p.as_str()))
        {
            continue;
        }
        let text = std::fs::read_to_string(item.path())
            .with_context(|| format!("reading {}", item.path().display()))?;
        // only drafts this command wrote are replaced; hand-written findings stay
        if parse(&text).is_ok() {
            std::fs::remove_file(item.path())
                .with_context(|| format!("removing {}", item.path().display()))?;
            removed.push(name);
        }
    }
    let mut written = Vec::new();
    for d in &drafts {
        let path = opts.out.join(&d.file);
        std::fs::write(&path, render(d)).with_context(|| format!("writing {}", path.display()))?;
        written.push(d.file.clone());
    }
    Ok(DraftReport { written, removed })
}

pub struct FileOptions {
    pub issues: PathBuf,
    /// Only files whose name is listed; all drafts when empty.
    pub only: Vec<String>,
    pub owner_approval: bool,
}

/// Opens every drafted issue that has no upstream issue yet and records the links in
/// `filed.json`. Never runs without the owner's approval flag.
pub fn file(opts: &FileOptions) -> Result<BTreeMap<String, String>> {
    if !opts.owner_approval {
        bail!(
            "file-issues opens public issues in upstream repositories; it runs only with --i-have-owner-approval, given by the repo owner for this batch"
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
        let url = match existing(&repo, &vector)? {
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

fn existing(repo: &str, vector: &str) -> Result<Option<String>> {
    let query = format!("\"{MARKER}: `{vector}`\" in:body");
    let text = gh(&[
        "issue", "list", "--repo", repo, "--state", "all", "--search", &query, "--json", "url",
        "--limit", "5",
    ])?;
    let found: Vec<Value> = serde_json::from_str(&text).context("parsing gh issue list")?;
    Ok(found
        .first()
        .and_then(|v| v.get("url"))
        .and_then(Value::as_str)
        .map(str::to_string))
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
