//! `shapekit site data`: the JSON the static site in `site/` renders. Everything is derived
//! from committed files only (the manifest, the coverage report, the findings record and its
//! committed inputs, the conformance tables, the registry and pins.toml), so CI can rebuild
//! it from a checkout. The commit is passed in, never read from git, so the output depends on
//! the files and the flags alone. See docs/site.md.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use shapekit_conformance::known;
use shapekit_conformance::reproduce::ReproductionsFile;
use shapekit_conformance::versions::VersionsFile;
use shapekit_findings::collect::Collection;
use shapekit_findings::inputs::{self, MatrixView};
use shapekit_findings::triage::{self, Triage};
use shapekit_manifest::Manifest;
use shapekit_registry::Registry;

use crate::pins;

pub const SITE_SCHEMA: u32 = 1;
const DEFAULT_REPO: &str = "ErenTopaal/shapekit";
/// The wallet-backend sightings `shapekit harness reproduce` checks; the site lists them next
/// to the committed run of the same target.
const HARNESS_REPRODUCTIONS: &str = "adapters/walletbackend/reproductions.toml";
/// The `describe.target` of runs the table above is about.
const HARNESS_REPRODUCTIONS_TARGET: &str = "wallet-backend";

#[derive(Subcommand)]
pub enum SiteCommand {
    /// Write the site's JSON data files from the committed corpus, coverage, findings,
    /// conformance tables, registry and pins.
    Data {
        /// The repository root; every input path is relative to it.
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// Directory the JSON files are written into; created when missing.
        #[arg(long)]
        out: PathBuf,
        /// The commit the files were read at, as a 7 to 40 character hex id. Recorded in
        /// meta.json; it is not read from git.
        #[arg(long)]
        commit: String,
        /// GitHub `owner/repo` the site links files into.
        #[arg(long, default_value = DEFAULT_REPO)]
        repo: String,
    },
}

pub fn run(cmd: SiteCommand) -> Result<bool> {
    match cmd {
        SiteCommand::Data {
            root,
            out,
            commit,
            repo,
        } => {
            let written = write_data(&root, &out, &commit, &repo)?;
            for (file, sha) in &written {
                println!("{file} {sha}");
            }
            println!("wrote {} files to {}", written.len(), out.display());
            Ok(true)
        }
    }
}

/// Every input file read, with its digest, in the order first read.
#[derive(Default)]
struct Sources {
    root: PathBuf,
    seen: Vec<SourceRef>,
}

#[derive(Serialize)]
struct SourceRef {
    path: String,
    sha256: String,
    bytes: usize,
}

impl Sources {
    fn read(&mut self, rel: &str) -> Result<Vec<u8>> {
        ensure!(
            shapekit_manifest::is_safe_relative(rel),
            "{rel} is not a relative path inside the repository"
        );
        let path = self.root.join(rel);
        let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        if !self.seen.iter().any(|s| s.path == rel) {
            self.seen.push(SourceRef {
                path: rel.to_owned(),
                sha256: shapekit_manifest::sha256_hex(&bytes),
                bytes: bytes.len(),
            });
        }
        Ok(bytes)
    }

    fn text(&mut self, rel: &str) -> Result<String> {
        String::from_utf8(self.read(rel)?).with_context(|| format!("{rel} is not utf-8"))
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
}

fn check_commit(commit: &str) -> Result<()> {
    let ok = (7..=40).contains(&commit.len())
        && commit
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    ensure!(
        ok,
        "--commit must be a 7 to 40 character lowercase hex commit id, got {commit:?}"
    );
    Ok(())
}

fn check_repo(repo: &str) -> Result<()> {
    let parts: Vec<&str> = repo.split('/').collect();
    let ok = parts.len() == 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        });
    ensure!(ok, "--repo must be owner/name, got {repo:?}");
    Ok(())
}

/// Writes the data files and returns each file name with the sha256 of what was written.
pub fn write_data(
    root: &Path,
    out: &Path,
    commit: &str,
    repo: &str,
) -> Result<Vec<(String, String)>> {
    check_commit(commit)?;
    check_repo(repo)?;
    let mut src = Sources {
        root: root.to_path_buf(),
        ..Sources::default()
    };

    let manifest: Manifest = serde_json::from_slice(&src.read("corpus/manifest.json")?)
        .context("parsing corpus/manifest.json")?;
    let coverage: CoverageView =
        serde_json::from_slice(&src.read("corpus/coverage/coverage.json")?)
            .context("parsing corpus/coverage/coverage.json")?;
    ensure!(
        coverage.corpus_version == manifest.corpus_version,
        "coverage.json is of corpus {} but the manifest is {}",
        coverage.corpus_version,
        manifest.corpus_version
    );

    // the registry loader reads its own files; hash the ones it reads so the record is whole
    let registry_files = registry_files(&src.path("registry"))?;
    for f in &registry_files {
        src.read(f)?;
    }
    let registry = Registry::load(&src.path("registry"))
        .map_err(|e| anyhow::anyhow!("loading registry/: {e}"))?;
    let pins = pins::parse(&src.text("pins.toml")?).context("parsing pins.toml")?;

    src.read("findings/triage.toml")?;
    let triage: Triage = triage::load(&src.path("findings/triage.toml"))?;
    let findings: Collection = serde_json::from_slice(&src.read("findings/findings.json")?)
        .context("parsing findings/findings.json")?;

    src.read("conformance/versions.toml")?;
    let versions = VersionsFile::load(&src.path("conformance/versions.toml"))?;
    src.read("conformance/reproductions.toml")?;
    let reproductions = ReproductionsFile::load(&src.path("conformance/reproductions.toml"))?;

    let protocol_sha = pins
        .git
        .iter()
        .find(|g| g.name == "stellar-protocol")
        .and_then(|g| g.sha.clone())
        .context("pins.toml has no stellar-protocol commit")?;

    let blockers = blocker_meanings(&src.text("corpus/coverage/COVERAGE.md")?)?;
    let coverage_out = coverage_json(&coverage, &registry, &protocol_sha, &blockers)?;
    let entries_out = entries_json(&manifest);
    let conformance_out = conformance_json(&mut src, &triage, &versions, &reproductions)?;
    let harness_out = harness_json(&mut src, &triage)?;
    let findings_out = findings_json(&findings);

    let files: Vec<(&str, Value)> = vec![
        ("coverage.json", coverage_out),
        ("entries.json", entries_out),
        ("conformance.json", conformance_out),
        ("harness.json", harness_out),
        ("findings.json", findings_out),
    ];

    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    let mut written = Vec::new();
    for (name, value) in &files {
        let sha = write_json(out, name, value)?;
        written.push(((*name).to_owned(), sha));
    }
    let meta = meta_json(&manifest, &pins, &src, commit, repo, &written);
    let sha = write_json(out, "meta.json", &meta)?;
    written.push(("meta.json".to_owned(), sha));
    Ok(written)
}

fn write_json(out: &Path, name: &str, value: &Value) -> Result<String> {
    let mut text = serde_json::to_string(value)?;
    text.push('\n');
    let path = out.join(name);
    std::fs::write(&path, &text).with_context(|| format!("writing {}", path.display()))?;
    Ok(shapekit_manifest::sha256_hex(text.as_bytes()))
}

fn registry_files(dir: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for e in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let e = e?;
        let name = e.file_name().to_string_lossy().into_owned();
        if name.ends_with(".toml") && e.file_type()?.is_file() {
            names.push(format!("registry/{name}"));
        }
    }
    names.sort();
    ensure!(!names.is_empty(), "registry/ holds no toml files");
    Ok(names)
}

// the parts of corpus/coverage/coverage.json the site shows. the coverage crate's own types
// only serialize (they borrow static names), so the report is read back through these

#[derive(Deserialize)]
struct CoverageView {
    corpus_version: String,
    stellar_xdr: String,
    root_type: String,
    summary: Value,
    caps: Vec<CapRowView>,
    arms: ArmsView,
    families: Vec<FamilyRowView>,
    fields: Vec<FieldRowView>,
    /// The obligation ledger by kind.
    obligations: Vec<ObligationRowView>,
}

#[derive(Deserialize)]
struct ObligationRowView {
    kind: String,
    met: usize,
    unmeetable: usize,
    open: usize,
}

#[derive(Deserialize)]
struct CapRowView {
    id: String,
    title: String,
    #[serde(default)]
    protocol: Option<u32>,
    status: String,
    indexer_visible: bool,
    #[serde(default)]
    not_visible_reason: Option<String>,
    families: Vec<String>,
    entries: usize,
    core_emitted_entries: usize,
    entries_by_source_class: BTreeMap<String, usize>,
    entries_by_variant: BTreeMap<String, usize>,
    protocols_core_emitted: Vec<u32>,
    protocols_all: Vec<u32>,
    #[serde(default)]
    uncovered: Option<ReasonView>,
}

#[derive(Deserialize, Serialize)]
struct ReasonView {
    blocker: String,
    reason: String,
}

#[derive(Deserialize)]
struct ArmsView {
    reachable: usize,
    covered: Vec<Value>,
    uncovered: Vec<UncoveredArmView>,
}

#[derive(Deserialize)]
struct UncoveredArmView {
    arm: String,
    non_core_entries: Vec<String>,
    refused_by_core: Vec<String>,
    #[serde(default)]
    uncovered: Option<ReasonView>,
}

#[derive(Deserialize)]
struct FamilyRowView {
    id: String,
    title: String,
    caps: Vec<String>,
    entries_by_source_class: BTreeMap<String, usize>,
    missing_source_classes: Vec<String>,
    fragments: Vec<FragmentRowView>,
    planned: Vec<Value>,
}

#[derive(Deserialize)]
struct FragmentRowView {
    fragment: String,
    covered: bool,
    core_emitted: Vec<String>,
    non_core_entries: Vec<String>,
    refused_by_core: Vec<String>,
    #[serde(default)]
    uncovered: Option<ReasonView>,
}

#[derive(Deserialize)]
struct FieldRowView {
    id: String,
    xdr_path: String,
    validated_by_core: String,
    entries: Vec<String>,
    core_emitted_entries: usize,
}

/// The blocker table of the committed COVERAGE.md (`| `name` | meaning |` rows under
/// `## Blockers`), so the site explains each blocker in the words the report uses.
fn blocker_meanings(markdown: &str) -> Result<Vec<(String, String)>> {
    let mut rows = Vec::new();
    let mut inside = false;
    for line in markdown.lines() {
        if line.starts_with("## ") {
            if inside {
                break;
            }
            inside = line.trim() == "## Blockers";
            continue;
        }
        if !inside {
            continue;
        }
        let Some(row) = line.strip_prefix("| `") else {
            continue;
        };
        let Some((name, rest)) = row.split_once("` | ") else {
            bail!("corpus/coverage/COVERAGE.md: blocker row {line:?} is not | `name` | meaning |");
        };
        let meaning = rest.trim_end().trim_end_matches('|').trim_end();
        ensure!(
            !name.is_empty() && !meaning.is_empty(),
            "corpus/coverage/COVERAGE.md: blocker row {line:?} has an empty cell"
        );
        rows.push((name.to_owned(), meaning.to_owned()));
    }
    ensure!(
        !rows.is_empty(),
        "corpus/coverage/COVERAGE.md has no ## Blockers table"
    );
    Ok(rows)
}

fn coverage_json(
    c: &CoverageView,
    reg: &Registry,
    protocol_sha: &str,
    blockers: &[(String, String)],
) -> Result<Value> {
    // every blocker a reason names must be explained, or the page would show a bare word
    let named = c
        .caps
        .iter()
        .filter_map(|r| r.uncovered.as_ref())
        .chain(c.arms.uncovered.iter().filter_map(|a| a.uncovered.as_ref()))
        .chain(
            c.families
                .iter()
                .flat_map(|f| f.fragments.iter().filter_map(|x| x.uncovered.as_ref())),
        );
    for r in named {
        ensure!(
            blockers.iter().any(|(b, _)| *b == r.blocker),
            "blocker {} is not in the ## Blockers table of corpus/coverage/COVERAGE.md",
            r.blocker
        );
    }
    let mut caps = Vec::new();
    for row in &c.caps {
        let cap = reg.cap(&row.id).with_context(|| {
            format!("coverage names {} but registry/caps.toml does not", row.id)
        })?;
        caps.push(json!({
            "id": row.id,
            "title": row.title,
            "protocol": row.protocol,
            "status": row.status,
            "indexer_visible": row.indexer_visible,
            "not_visible_reason": row.not_visible_reason,
            "families": row.families,
            "entries": row.entries,
            "core_emitted_entries": row.core_emitted_entries,
            "entries_by_source_class": row.entries_by_source_class,
            "entries_by_variant": row.entries_by_variant,
            "protocols_core_emitted": row.protocols_core_emitted,
            "protocols_all": row.protocols_all,
            "uncovered": row.uncovered,
            "xdr_additions": cap.xdr_additions,
            "text_url": format!(
                "https://github.com/stellar/stellar-protocol/blob/{protocol_sha}/{}",
                cap.source.path
            ),
        }));
    }
    let mut families = Vec::new();
    for row in &c.families {
        let fam = reg.family(&row.id).with_context(|| {
            format!(
                "coverage names family {} but registry/families.toml does not",
                row.id
            )
        })?;
        let fragments: Vec<Value> = row
            .fragments
            .iter()
            .map(|f| {
                json!({
                    "fragment": f.fragment,
                    "covered": f.covered,
                    "core_emitted": f.core_emitted.len(),
                    "non_core": f.non_core_entries.len(),
                    "refused_by_core": f.refused_by_core.len(),
                    "uncovered": f.uncovered,
                })
            })
            .collect();
        families.push(json!({
            "id": row.id,
            "title": row.title,
            "caps": row.caps,
            "priority": fam.priority,
            "protocol_introduced": fam.protocol_introduced,
            "protocol_valid": { "min": fam.protocol_valid.min, "max": fam.protocol_valid.max },
            "linked_issues": fam.linked_issues,
            "entries_by_source_class": row.entries_by_source_class,
            "missing_source_classes": row.missing_source_classes,
            "fragments": fragments,
            "planned": row.planned.len(),
        }));
    }
    let uncovered_arms: Vec<Value> = c
        .arms
        .uncovered
        .iter()
        .map(|a| {
            json!({
                "arm": a.arm,
                "non_core": a.non_core_entries.len(),
                "refused_by_core": a.refused_by_core.len(),
                "uncovered": a.uncovered,
            })
        })
        .collect();
    let fields: Vec<Value> = c
        .fields
        .iter()
        .map(|f| {
            json!({
                "id": f.id,
                "xdr_path": f.xdr_path,
                "validated_by_core": f.validated_by_core,
                "entries": f.entries.len(),
                "core_emitted_entries": f.core_emitted_entries,
            })
        })
        .collect();
    Ok(json!({
        "schema": SITE_SCHEMA,
        "corpus_version": c.corpus_version,
        "stellar_xdr": c.stellar_xdr,
        "root_type": c.root_type,
        "summary": c.summary,
        "caps": caps,
        "families": families,
        "arms": {
            "reachable": c.arms.reachable,
            "covered": c.arms.covered.len(),
            "uncovered": uncovered_arms,
        },
        "fields": fields,
        "obligations": c
            .obligations
            .iter()
            .map(|o| json!({ "kind": o.kind, "met": o.met, "unmeetable": o.unmeetable, "open": o.open }))
            .collect::<Vec<_>>(),
        "blockers": blockers.iter().map(|(b, m)| json!({ "blocker": b, "meaning": m })).collect::<Vec<_>>(),
    }))
}

fn entries_json(m: &Manifest) -> Value {
    let entries: Vec<Value> = m
        .entries
        .iter()
        .map(|e| {
            let mut files: Vec<Value> = e
                .variants
                .iter()
                .map(|(name, v)| json!({ "variant": name, "path": format!("corpus/{}", v.xdr_file) }))
                .collect();
            if let Some(v) = &e.vector {
                files.push(json!({ "variant": "vector", "path": format!("corpus/{}", v.path) }));
            }
            json!({
                "id": e.id,
                "family": e.family,
                "kind": e.kind,
                "caps": e.caps,
                "protocol": e.protocol,
                "source": e.source.class,
                "synthetic": e.synthetic,
                "xdr_type": e.xdr_type,
                "expected": e.expected,
                "flags": e.flags,
                "linked_issues": e.linked_issues,
                "files": files,
            })
        })
        .collect();
    json!({
        "schema": SITE_SCHEMA,
        "corpus_version": m.corpus_version,
        "entries": entries,
    })
}

/// Check results that count as a failure (docs/testee-protocol.md, "How the runner uses the
/// ops"); `unsupported` and `reference` do not.
fn is_failing_check(result: &str) -> bool {
    matches!(
        result,
        "error" | "diverges-from-network" | "diverges-from-peers"
    )
}

/// What the committed matrix shows for one reproduction row at one version.
fn observe(
    row: &shapekit_conformance::reproduce::Reproduction,
    m: &MatrixView,
    version: &str,
) -> &'static str {
    let key = format!("{}@{}", row.sdk, version);
    if !m.testees.iter().any(|t| t.key == key) {
        return "not-in-matrix";
    }
    if !row.probes.is_empty() || row.select.is_some() {
        // probes and selections are resolved by the runner, not stored in the matrix
        return "not-in-matrix";
    }
    let failing = |entry: &str, check: &str| -> bool {
        m.entries
            .iter()
            .find(|e| e.id == entry)
            .and_then(|e| e.cells.get(&key))
            .is_some_and(|c| c.checks.iter().any(|k| k.check == check))
    };
    let named_all = row.must_diverge.iter().all(|d| match &d.entry {
        Some(entry) => d.checks.iter().all(|c| failing(entry, c)),
        None => false,
    });
    let counted_any = row.entries.iter().any(|entry| {
        m.entries
            .iter()
            .find(|e| &e.id == entry)
            .and_then(|e| e.cells.get(&key))
            .is_some_and(|c| {
                c.checks
                    .iter()
                    .any(|k| row.ops.is_empty() || row.ops.contains(&k.op))
            })
    });
    match (named_all, counted_any) {
        (true, _) => "named-checks-diverge",
        (false, false) => "no-counted-check-fails",
        (false, true) => "diverges-elsewhere",
    }
}

/// Splits a matrix that spans sdks by testee and adds each sdk's cells to its own matrix,
/// or makes them that sdk's matrix when there is none. A cell both runs failed keeps the
/// per-sdk result and gains the checks it lacked.
fn merge_mixed(
    matrices: &mut BTreeMap<String, (String, MatrixView)>,
    rel: &str,
    m: MatrixView,
) -> Result<()> {
    let sdk_of: BTreeMap<String, String> = m
        .testees
        .iter()
        .map(|t| (t.key.clone(), t.sdk.clone()))
        .collect();
    for t in &m.testees {
        let (_, own) = matrices.entry(t.sdk.clone()).or_insert_with(|| {
            (
                rel.to_owned(),
                MatrixView {
                    corpus_version: m.corpus_version.clone(),
                    testees: Vec::new(),
                    entries: Vec::new(),
                    known_failure_problems: m.known_failure_problems.clone(),
                },
            )
        });
        if !own.testees.iter().any(|o| o.key == t.key) {
            own.testees.push(t.clone());
        }
    }
    for e in m.entries {
        for (key, cell) in e.cells {
            let Some(sdk) = sdk_of.get(&key) else {
                bail!("{rel}: cell {key} of {} has no testee", e.id);
            };
            let (_, own) = matrices
                .get_mut(sdk)
                .expect("every testee sdk was inserted above");
            let entry = match own.entries.iter().position(|o| o.id == e.id) {
                Some(i) => &mut own.entries[i],
                None => {
                    own.entries.push(inputs::MatrixEntry {
                        id: e.id.clone(),
                        cells: BTreeMap::new(),
                    });
                    own.entries.last_mut().expect("just pushed")
                }
            };
            match entry.cells.get_mut(&key) {
                Some(have) => {
                    for c in cell.checks {
                        if !have.checks.contains(&c) {
                            have.checks.push(c);
                        }
                    }
                    if have.known_issue.is_none() {
                        have.known_issue = cell.known_issue;
                    }
                }
                None => {
                    entry.cells.insert(key, cell);
                }
            }
        }
    }
    Ok(())
}

fn conformance_json(
    src: &mut Sources,
    triage: &Triage,
    versions: &VersionsFile,
    reproductions: &ReproductionsFile,
) -> Result<Value> {
    let mut matrices: BTreeMap<String, (String, MatrixView)> = BTreeMap::new();
    let mut mixed: Vec<(String, MatrixView)> = Vec::new();
    for rel in &triage.inputs.matrices {
        let bytes = src.read(rel)?;
        let path = src.path(rel);
        let m = shapekit_findings::parse_on_big_stack(move || inputs::matrix(&bytes, &path))?;
        let sdks: Vec<&str> = m.testees.iter().map(|t| t.sdk.as_str()).collect();
        let Some(sdk) = sdks.first().map(|s| (*s).to_owned()) else {
            bail!("{rel} has no testees");
        };
        if sdks.iter().any(|s| *s != sdk) {
            mixed.push((rel.clone(), m));
            continue;
        }
        ensure!(
            !matrices.contains_key(&sdk),
            "two matrices for sdk {sdk} in findings/triage.toml"
        );
        matrices.insert(sdk, (rel.clone(), m));
    }
    // a run over every sdk at once (the references-disagree run) holds cells a per-sdk run
    // cannot produce, so each sdk's share of it joins that sdk's own matrix
    for (rel, m) in mixed {
        merge_mixed(&mut matrices, &rel, m)?;
    }

    let mut sdks = Vec::new();
    for (name, sdk) in &versions.sdk {
        let known_rel = format!("conformance/known-failures/{name}.txt");
        let known = if src.path(&known_rel).exists() {
            src.read(&known_rel)?;
            known::load(&src.path("conformance/known-failures"), name)?
        } else {
            Vec::new()
        };
        let matrix = matrices.get(name);
        let mut vers = Vec::new();
        for v in &sdk.versions {
            let key = format!("{name}@{}", v.version);
            let mut cells_by_result: BTreeMap<String, usize> = BTreeMap::new();
            let mut checks_by_op: BTreeMap<String, usize> = BTreeMap::new();
            let mut checks_by_result: BTreeMap<String, usize> = BTreeMap::new();
            let mut cells_known = 0usize;
            let in_matrix = matrix.is_some_and(|(_, m)| m.testees.iter().any(|t| t.key == key));
            if let Some((_, m)) = matrix {
                for e in &m.entries {
                    if let Some(c) = e.cells.get(&key) {
                        *cells_by_result.entry(c.result.clone()).or_default() += 1;
                        if c.known_issue.is_some() {
                            cells_known += 1;
                        }
                        for k in &c.checks {
                            // a failing cell also lists checks the version could not answer
                            // (`unsupported`); those are not failures, so only the per-result
                            // count holds them
                            if is_failing_check(&k.result) {
                                *checks_by_op.entry(k.op.clone()).or_default() += 1;
                            }
                            *checks_by_result.entry(k.result.clone()).or_default() += 1;
                        }
                    }
                }
            }
            let known_lines = known
                .iter()
                .filter(|l| l.version.as_deref().is_none_or(|lv| lv == v.version))
                .count();
            vers.push(json!({
                "version": v.version,
                "key": key,
                "current": v.version == sdk.current,
                "max_protocol": v.max_protocol,
                "in_matrix": in_matrix,
                "failing_cells_by_result": cells_by_result,
                "failing_cells_with_known_issue": cells_known,
                "failing_checks_by_op": checks_by_op,
                "checks_by_result": checks_by_result,
                "known_failure_lines": known_lines,
            }));
        }
        let entries: Vec<Value> = match matrix {
            Some((_, m)) => m
                .entries
                .iter()
                .map(|e| {
                    let cells: BTreeMap<&String, Value> = e
                        .cells
                        .iter()
                        .map(|(k, c)| {
                            (
                                k,
                                json!({
                                    "result": c.result,
                                    "known_issue": c.known_issue,
                                    "checks": c.checks.iter().map(|ch| json!({
                                        "check": ch.check,
                                        "op": ch.op,
                                        "result": ch.result,
                                        "detail": ch.detail,
                                    })).collect::<Vec<_>>(),
                                }),
                            )
                        })
                        .collect();
                    json!({ "id": e.id, "cells": cells })
                })
                .collect(),
            None => Vec::new(),
        };
        sdks.push(json!({
            "sdk": name,
            "package": sdk.package,
            "current": sdk.current,
            "issues_repo": sdk.issues_repo,
            "matrix": matrix.map(|(rel, _)| rel),
            "corpus_version": matrix.map(|(_, m)| &m.corpus_version),
            "known_failure_problems": matrix.map(|(_, m)| &m.known_failure_problems),
            "known_failures_file": if known.is_empty() { None } else { Some(format!("conformance/known-failures/{name}.txt")) },
            "known_failure_lines": known.len(),
            "versions": vers,
            "entries": entries,
        }));
    }

    let mut rows = Vec::new();
    for r in &reproductions.reproductions {
        let matrix = matrices.get(&r.sdk).map(|(_, m)| m);
        let observed = |vs: &[String]| -> BTreeMap<String, &'static str> {
            vs.iter()
                .map(|v| {
                    (
                        v.clone(),
                        matrix.map_or("not-in-matrix", |m| observe(r, m, v)),
                    )
                })
                .collect()
        };
        rows.push(json!({
            "id": r.id,
            "sighting": r.sighting,
            "link": r.link,
            "sdk": r.sdk,
            "entries": r.entries,
            "ops": r.ops,
            "probes": r.probes.iter().map(|p| &p.name).collect::<Vec<_>>(),
            "must_diverge": r.must_diverge.iter().map(|d| json!({
                "entry": d.entry,
                "probe": d.probe,
                "checks": d.checks,
            })).collect::<Vec<_>>(),
            "fail": observed(&r.fail),
            "pass": observed(&r.pass),
        }));
    }
    Ok(json!({
        "schema": SITE_SCHEMA,
        "sdks": sdks,
        "reproductions": rows,
    }))
}

fn harness_json(src: &mut Sources, triage: &Triage) -> Result<Value> {
    let mut runs = Vec::new();
    for rel in &triage.inputs.harness {
        let bytes = src.read(rel)?;
        let r = inputs::harness_report(&bytes, &src.path(rel))?;
        // per family and network, the verdict counts
        let mut by_family: BTreeMap<(String, String), BTreeMap<&'static str, usize>> =
            BTreeMap::new();
        let mut by_network: BTreeMap<String, BTreeMap<&'static str, usize>> = BTreeMap::new();
        let mut problems = Vec::new();
        // every replayed entry's verdict per network, so a page can say "passed" and not only
        // list failures
        let mut verdicts: BTreeMap<&str, BTreeMap<&str, Value>> = BTreeMap::new();
        for e in &r.entries {
            let v = e.result.verdict.name();
            verdicts.entry(e.entry.as_str()).or_default().insert(
                e.network.as_str(),
                json!({ "verdict": v, "reason": e.result.reason.map(|x| x.name()) }),
            );
            *by_family
                .entry((e.family.clone(), e.network.clone()))
                .or_default()
                .entry(v)
                .or_default() += 1;
            *by_network
                .entry(e.network.clone())
                .or_default()
                .entry(v)
                .or_default() += 1;
            if !matches!(v, "pass" | "out-of-scope") {
                problems.push(json!({
                    "entry": e.entry,
                    "family": e.family,
                    "chain": e.chain,
                    "network": e.network,
                    "ledger": e.ledger,
                    "verdict": v,
                    "reason": e.result.reason.map(|x| x.name()),
                    "detail": e.result.detail,
                    "problems": e.result.problems.iter().map(|p| json!({
                        "kind": p.kind,
                        "fact": p.fact,
                        "detail": p.detail,
                    })).collect::<Vec<_>>(),
                }));
            }
        }
        let families: Vec<Value> = by_family
            .into_iter()
            .map(|((family, network), counts)| {
                json!({ "family": family, "network": network, "verdicts": counts })
            })
            .collect();
        let chain_errors: Vec<Value> = r
            .chains
            .iter()
            .filter_map(|c| {
                c.error
                    .as_ref()
                    .map(|err| json!({ "chain": c.chain, "network": c.network, "error": err }))
            })
            .collect();
        let mut chains_by_network: BTreeMap<&str, usize> = BTreeMap::new();
        for c in &r.chains {
            *chains_by_network.entry(c.network.as_str()).or_default() += 1;
        }
        let mut components: BTreeMap<String, String> = BTreeMap::new();
        for c in &r.chains {
            for (k, v) in &c.components {
                components.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        let describe = r.describe.as_ref().map(|d| {
            json!({
                "adapter": d.adapter,
                "adapter_version": d.adapter_version,
                "target": d.target,
                "target_version": d.target_version,
                "default_delivery": d.default_delivery,
                "protocols": { "min": d.protocols.min, "max": d.protocols.max },
            })
        });
        runs.push(json!({
            "report": rel,
            "ok": r.ok(),
            "shapekit_version": r.shapekit_version,
            "manifest_sha256": r.manifest_sha256,
            "describe": describe,
            "components": components,
            "totals": r.totals,
            "chains": r.chains.len(),
            "chains_by_network": chains_by_network,
            "distinct_entries": verdicts.len(),
            "chain_errors": chain_errors,
            "rounds": r.rounds.iter().map(|x| json!({
                "round": x.round,
                "peak_rss_bytes": x.peak_rss_bytes,
                "seconds": x.seconds,
                "same_verdicts_as_first": x.same_verdicts_as_first,
            })).collect::<Vec<_>>(),
            "memory": r.memory,
            "target_memory": r.target_memory,
            "by_network": by_network,
            "families": families,
            "problems": problems,
            "verdicts": verdicts,
        }));
    }
    let reproductions = harness_reproductions(src, &runs)?;
    let mut lint = Vec::new();
    for l in &triage.inputs.lint {
        let bytes = src.read(&l.result)?;
        let res = inputs::lint(&bytes, &src.path(&l.result))?;
        lint.push(json!({
            "target": l.target,
            "version": l.version,
            "result": l.result,
            "command": l.command,
            "findings": res.findings,
            "suppressed": res.suppressed.len(),
            "baselined": res.baselined.len(),
        }));
    }
    Ok(json!({
        "schema": SITE_SCHEMA,
        "runs": runs,
        "reproductions": reproductions,
        "lint": lint,
    }))
}

/// The wallet-backend reproduction table, each expectation with the verdict the committed
/// run at the pinned commit gave the same entry. The table's own pre-fix and fixed commits
/// are run by `shapekit harness reproduce` in the nightly workflow, which commits no result,
/// so the committed run is the only observed verdict the site can show.
fn harness_reproductions(src: &mut Sources, runs: &[Value]) -> Result<Value> {
    src.read(HARNESS_REPRODUCTIONS)?;
    let table = shapekit_harness::reproduce::load(&src.path(HARNESS_REPRODUCTIONS))?;
    let pinned: Vec<&Value> = runs
        .iter()
        .filter(|r| r["describe"]["target"] == HARNESS_REPRODUCTIONS_TARGET)
        .collect();
    let rows: Vec<Value> = table
        .reproduction
        .iter()
        .map(|r| {
            let cases: Vec<Value> = r
                .case
                .iter()
                .map(|c| {
                    let expect: Vec<Value> = c
                        .expect
                        .iter()
                        .map(|x| {
                            let at_pin: Vec<Value> = pinned
                                .iter()
                                .filter_map(|run| {
                                    let v = &run["verdicts"][&x.entry][&r.network];
                                    (!v.is_null()).then(|| {
                                        json!({
                                            "report": run["report"],
                                            "target_version": run["describe"]["target_version"],
                                            "verdict": v["verdict"],
                                            "reason": v["reason"],
                                        })
                                    })
                                })
                                .collect();
                            json!({
                                "entry": x.entry,
                                "verdict": x.verdict,
                                "reason": x.reason,
                                "at_pin": at_pin,
                            })
                        })
                        .collect();
                    json!({ "label": c.label, "commit": c.commit, "expect": expect })
                })
                .collect();
            json!({
                "id": r.id,
                "issue": r.issue,
                "chain": r.chain,
                "network": r.network,
                "notes": r.notes,
                "cases": cases,
            })
        })
        .collect();
    Ok(json!({ "table": HARNESS_REPRODUCTIONS, "rows": rows }))
}

fn findings_json(c: &Collection) -> Value {
    let findings: Vec<Value> = c
        .findings
        .iter()
        .map(|f| {
            let mut by_source: BTreeMap<&str, usize> = BTreeMap::new();
            for o in &f.occurrences {
                *by_source.entry(o.source.as_str()).or_default() += 1;
            }
            json!({
                "id": f.id,
                "title": f.title,
                "component": f.component,
                "severity": f.severity,
                "status": f.status,
                "caps": f.caps,
                "minimal_entry": f.minimal_entry,
                "reproduce": f.reproduce,
                "link": f.link,
                "reference": f.reference,
                "reason": f.reason,
                "draft": f.draft,
                "cause": f.cause,
                "siblings": f.siblings,
                "related": f.related,
                "entries": f.entries,
                "occurrences": f.occurrences.len(),
                "occurrences_by_source": by_source,
            })
        })
        .collect();
    json!({
        "schema": SITE_SCHEMA,
        "summary": c.summary,
        "inputs": c.inputs,
        "unmapped": c.unmapped.len(),
        "input_problems": c.input_problems,
        "findings": findings,
    })
}

fn meta_json(
    m: &Manifest,
    p: &pins::Pins,
    src: &Sources,
    commit: &str,
    repo: &str,
    outputs: &[(String, String)],
) -> Value {
    let git: Vec<Value> = p
        .git
        .iter()
        .filter(|g| g.url.starts_with("https://github.com/stellar/"))
        .map(|g| json!({ "name": g.name, "url": g.url, "ref": g.git_ref, "sha": g.sha, "purpose": g.purpose }))
        .collect();
    json!({
        "schema": SITE_SCHEMA,
        "commit": commit,
        "repo": repo,
        "repo_url": format!("https://github.com/{repo}"),
        "shapekit_version": env!("CARGO_PKG_VERSION"),
        "corpus": {
            "version": m.corpus_version,
            "entries": m.entries.len(),
            "protocol_tiers": m.protocol_tiers,
            "generated_with": m.generated_with,
        },
        "pins": {
            "toolchains": p.toolchains.iter().map(|t| json!({ "name": format!("{:?}", t.name).to_lowercase(), "version": t.version })).collect::<Vec<_>>(),
            "images": p.images.iter().map(|i| json!({ "name": i.name, "repository": i.repository, "tag": i.tag, "digest": i.digest })).collect::<Vec<_>>(),
            "git": git,
            "npm": p.npm.iter().map(|n| json!({ "name": n.name, "version": n.version, "purpose": n.purpose })).collect::<Vec<_>>(),
            "gomod": p.gomod.iter().map(|g| json!({ "module": g.module, "version": g.version, "purpose": g.purpose })).collect::<Vec<_>>(),
            "crates": p.crates.iter().filter(|c| c.name.starts_with("stellar-") || c.name.starts_with("soroban-")).map(|c| json!({ "name": c.name, "version": c.version, "purpose": c.purpose })).collect::<Vec<_>>(),
        },
        "sources": src.seen,
        "outputs": outputs.iter().map(|(f, s)| json!({ "file": f, "sha256": s })).collect::<Vec<_>>(),
    })
}
