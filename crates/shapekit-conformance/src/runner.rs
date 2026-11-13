//! `shapekit conformance run`: every selected entry through every selected testee, the
//! divergence matrix, and the known-failures verdict.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shapekit_manifest::{Entry, EntryKind, Manifest, build as manifest_build};

use crate::compare::{self, Judged, ResultClass, Verdict};
use crate::known;
use crate::plan::{self, Check};
use crate::testee::Testee;
use crate::versions::{self, Target, VersionsFile};

/// Which entries a run covers. Every non-empty list must match; within a list any value may.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    pub ids: Vec<String>,
    pub kinds: Vec<EntryKind>,
    pub flags: Vec<String>,
    pub linked_issues: Vec<String>,
}

impl Selection {
    pub fn matches(&self, e: &Entry) -> bool {
        (self.ids.is_empty() || self.ids.contains(&e.id))
            && (self.kinds.is_empty() || self.kinds.contains(&e.kind))
            && (self.flags.is_empty() || self.flags.iter().any(|f| e.flags.contains(f)))
            && (self.linked_issues.is_empty()
                || self
                    .linked_issues
                    .iter()
                    .any(|l| e.linked_issues.contains(l)))
    }

    pub fn select<'m>(&self, manifest: &'m Manifest) -> Result<Vec<&'m Entry>> {
        for id in &self.ids {
            ensure!(
                manifest.entries.iter().any(|e| &e.id == id),
                "entry {id} is not in the manifest"
            );
        }
        Ok(manifest
            .entries
            .iter()
            .filter(|e| self.matches(e))
            .collect())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckRecord {
    pub check: String,
    pub op: String,
    pub result: ResultClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub expected: Value,
    pub actual: Value,
    pub source: String,
}

/// One entry on one testee.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    pub result: ResultClass,
    /// The known-failures issue covering this cell, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub known_issue: Option<String>,
    /// Check counts by result.
    pub counts: BTreeMap<ResultClass, usize>,
    /// Every check that did not pass.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<CheckRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryRow {
    pub id: String,
    pub kind: EntryKind,
    pub family: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub linked_issues: Vec<String>,
    /// Keyed by `<sdk>@<version>`.
    pub cells: BTreeMap<String, Cell>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TesteeInfo {
    pub key: String,
    pub sdk: String,
    pub package: String,
    pub version: String,
    pub components: BTreeMap<String, String>,
    pub ops: Vec<String>,
    pub decode_limits: String,
}

/// `divergence-matrix.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Matrix {
    pub schema_version: u32,
    pub corpus_version: String,
    /// Depth limit of every decode request but a whole ledger's (`xdr.envelope.depth`).
    pub decode_depth: u32,
    /// Depth limit of a `LedgerCloseMeta` decode request (`xdr.meta.depth`).
    pub ledger_decode_depth: u32,
    pub testees: Vec<TesteeInfo>,
    /// Entry counts by result per testee.
    pub summary: BTreeMap<String, BTreeMap<ResultClass, usize>>,
    pub entries: Vec<EntryRow>,
    /// Known-failures problems that fail the run, as text.
    pub known_failure_problems: Vec<String>,
}

pub const MATRIX_SCHEMA: u32 = 1;

/// Runs every check of `entry` on every testee and returns one cell per testee key.
pub fn evaluate_entry(
    entry: &Entry,
    corpus: &Path,
    testees: &mut [Testee],
) -> Result<BTreeMap<String, Cell>> {
    let checks = plan::checks(entry, corpus)?;
    let mut cells: Vec<Cell> = testees
        .iter()
        .map(|_| Cell {
            result: ResultClass::Pass,
            known_issue: None,
            counts: BTreeMap::new(),
            checks: Vec::new(),
        })
        .collect();
    for check in &checks {
        let verdicts = run_check(check, testees)?;
        for ((cell, v), t) in cells.iter_mut().zip(verdicts).zip(testees.iter()) {
            record(cell, check, beyond_protocol(entry, t.target(), v));
        }
    }
    if checks.is_empty() {
        // an entry that yields no check cannot pass; it means the runner has nothing to send
        for cell in &mut cells {
            record(
                cell,
                &Check {
                    name: "entry".into(),
                    op: "none".into(),
                    request: Default::default(),
                    expect: plan::Expect::Unmappable("the entry yields no checks".into()),
                    source: entry.id.clone(),
                },
                Verdict {
                    result: ResultClass::Error,
                    detail: Some("the entry yields no checks".into()),
                    actual: Value::Null,
                },
            );
        }
    }
    Ok(testees
        .iter()
        .map(|t| t.target().key())
        .zip(cells)
        .collect())
}

/// One check on every testee, implementation-defined checks decided across them.
pub fn run_check(check: &Check, testees: &mut [Testee]) -> Result<Vec<Verdict>> {
    let mut judged = Vec::with_capacity(testees.len());
    for t in testees.iter_mut() {
        let j = if matches!(check.expect, plan::Expect::Unmappable(_)) {
            compare::judge(
                &check.expect,
                &Ok(crate::testee::Reply::Unsupported(String::new())),
            )
        } else if !t.supports(&check.op) {
            Judged::Done(Verdict {
                result: ResultClass::Unsupported,
                detail: Some(format!(
                    "{} does not implement {}",
                    t.target().key(),
                    check.op
                )),
                actual: Value::String("unsupported".into()),
            })
        } else {
            let answer = t.request(check.request.clone())?;
            compare::judge(&check.expect, &answer)
        };
        judged.push(j);
    }
    let peers: Vec<Option<Value>> = judged
        .iter()
        .map(|j| match j {
            Judged::Peer(v) => Some(v.clone()),
            Judged::Done(_) => None,
        })
        .collect();
    let mut decided = compare::judge_peers(&peers).into_iter();
    Ok(judged
        .into_iter()
        .map(|j| {
            let d = decided.next().flatten();
            match j {
                Judged::Done(v) => v,
                Judged::Peer(_) => d.expect("every peer outcome is decided"),
            }
        })
        .collect())
}

/// The protocol an entry's bytes belong to: where they were generated, or for vectors without
/// a protocol, the first protocol the shape is valid in.
pub fn entry_protocol(entry: &Entry) -> u32 {
    entry
        .protocol
        .generated_at
        .unwrap_or(entry.protocol.valid_min)
}

fn beyond_protocol(entry: &Entry, target: &Target, v: Verdict) -> Verdict {
    let p = entry_protocol(entry);
    match target.max_protocol {
        Some(max)
            if p > max
                && matches!(
                    v.result,
                    ResultClass::DivergesFromNetwork | ResultClass::DivergesFromPeers
                ) =>
        {
            Verdict {
                result: ResultClass::Unsupported,
                detail: Some(format!(
                    "the entry is from protocol {p} and {} knows protocols up to {max} ({}: {})",
                    target.key(),
                    v.result,
                    v.detail.unwrap_or_default()
                )),
                actual: v.actual,
            }
        }
        _ => v,
    }
}

fn record(cell: &mut Cell, check: &Check, v: Verdict) {
    *cell.counts.entry(v.result).or_insert(0) += 1;
    cell.result = cell.result.max(v.result);
    if v.result != ResultClass::Pass {
        cell.checks.push(CheckRecord {
            check: check.name.clone(),
            op: check.op.clone(),
            result: v.result,
            detail: v.detail,
            expected: compare::expected_view(&check.expect),
            actual: v.actual,
            source: check.source.clone(),
        });
    }
}

pub struct RunOptions {
    /// Repository root; testee commands and build steps are relative to it.
    pub root: PathBuf,
    pub manifest: PathBuf,
    pub versions: PathBuf,
    pub known_dir: PathBuf,
    pub sdks: Vec<String>,
    /// Exact versions; empty means the current version, unless `all_versions`.
    pub only_versions: Vec<String>,
    pub all_versions: bool,
    pub selection: Selection,
    pub build: bool,
    /// Where divergence-matrix.json and MATRIX.md go; nothing is written when `None`.
    pub out: Option<PathBuf>,
    /// How many times the selection is run in one invocation; results must not change.
    pub passes: u32,
}

pub struct RunOutcome {
    pub matrix: Matrix,
    pub ok: bool,
    /// Peak resident set of the runner after each pass, KiB.
    pub peak_rss_kib: Vec<i64>,
}

/// Resolves the targets a run covers.
pub fn targets(file: &VersionsFile, opts: &RunOptions) -> Result<Vec<Target>> {
    if opts.only_versions.is_empty() {
        return file.select(&opts.sdks, opts.all_versions);
    }
    ensure!(
        !opts.all_versions,
        "--version and --all-versions exclude each other"
    );
    ensure!(
        opts.sdks.len() == 1,
        "--version needs exactly one --sdk, since versions differ per sdk"
    );
    opts.only_versions
        .iter()
        .map(|v| file.target(&opts.sdks[0], v))
        .collect()
}

pub fn load_manifest(path: &Path) -> Result<(Manifest, PathBuf)> {
    let manifest =
        manifest_build::load(path).with_context(|| format!("loading {}", path.display()))?;
    let corpus = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    Ok((manifest, corpus))
}

/// Builds (when asked) and starts every target, in order.
pub fn start_all(root: &Path, targets: &[Target], build: bool) -> Result<Vec<Testee>> {
    if build {
        for t in targets {
            eprintln!("building {}", t.key());
            versions::build(root, t)?;
        }
    }
    targets.iter().map(|t| Testee::start(root, t)).collect()
}

pub fn run(opts: &RunOptions) -> Result<RunOutcome> {
    ensure!(opts.passes >= 1, "passes must be at least 1");
    let file = VersionsFile::load(&opts.versions)?;
    let targets = targets(&file, opts)?;
    let (manifest, corpus) = load_manifest(&opts.manifest)?;
    let selected = opts.selection.select(&manifest)?;
    ensure!(!selected.is_empty(), "the selection matches no entry");
    let mut testees = start_all(&opts.root, &targets, opts.build)?;

    let mut rows: Vec<EntryRow> = Vec::new();
    let mut peak_rss_kib = Vec::new();
    for pass in 0..opts.passes {
        let mut this_pass = Vec::with_capacity(selected.len());
        for entry in &selected {
            let cells = evaluate_entry(entry, &corpus, &mut testees)
                .with_context(|| format!("running {}", entry.id))?;
            this_pass.push(EntryRow {
                id: entry.id.clone(),
                kind: entry.kind,
                family: entry.family.clone(),
                linked_issues: entry.linked_issues.clone(),
                cells,
            });
        }
        peak_rss_kib.push(peak_rss()?);
        if pass > 0 {
            ensure!(
                this_pass == rows,
                "pass {} gave different results from pass 1",
                pass + 1
            );
        }
        rows = this_pass;
    }

    let testee_info: Vec<TesteeInfo> = testees
        .iter()
        .map(|t| {
            let h = t.hello();
            TesteeInfo {
                key: t.target().key(),
                sdk: t.target().sdk.clone(),
                package: h.sdk.clone(),
                version: h.sdk_version.clone(),
                components: h.components.clone(),
                ops: h.ops.clone(),
                decode_limits: h.decode_limits.clone(),
            }
        })
        .collect();
    drop(testees);

    let all_ids: BTreeSet<String> = manifest.entries.iter().map(|e| e.id.clone()).collect();
    let mut problems = Vec::new();
    let sdks: BTreeSet<&str> = targets.iter().map(|t| t.sdk.as_str()).collect();
    let other_sdks = sdks.len() > 1;
    for sdk in sdks {
        let lines = known::load(&opts.known_dir, sdk)?;
        let mut results = BTreeMap::new();
        for row in &mut rows {
            for t in targets.iter().filter(|t| t.sdk == sdk) {
                let cell = row
                    .cells
                    .get_mut(&t.key())
                    .expect("every row has a cell per target");
                cell.known_issue = known::issue_for(&lines, &row.id, &t.version).map(String::from);
                results.insert((row.id.clone(), t.version.clone()), cell.result);
            }
        }
        let all_versions: BTreeSet<String> = file
            .sdk(sdk)?
            .versions
            .iter()
            .map(|v| v.version.clone())
            .collect();
        for p in known::evaluate(&lines, &results, &all_ids, &all_versions, other_sdks) {
            problems.push(format!(
                "{}: {p}",
                known::path(&opts.known_dir, sdk).display()
            ));
        }
    }

    let mut summary: BTreeMap<String, BTreeMap<ResultClass, usize>> = BTreeMap::new();
    for row in &rows {
        for (key, cell) in &row.cells {
            *summary
                .entry(key.clone())
                .or_default()
                .entry(cell.result)
                .or_insert(0) += 1;
        }
    }
    let matrix = Matrix {
        schema_version: MATRIX_SCHEMA,
        corpus_version: manifest.corpus_version.clone(),
        decode_depth: shapekit_manifest::ENVELOPE_DEPTH,
        ledger_decode_depth: shapekit_manifest::META_DEPTH,
        testees: testee_info,
        summary,
        entries: rows,
        known_failure_problems: problems,
    };
    if let Some(out) = &opts.out {
        write_outputs(out, &matrix)?;
    }
    Ok(RunOutcome {
        ok: matrix.known_failure_problems.is_empty(),
        matrix,
        peak_rss_kib,
    })
}

fn peak_rss() -> Result<i64> {
    use nix::sys::resource::{UsageWho, getrusage};
    let usage = getrusage(UsageWho::RUSAGE_SELF).context("getrusage")?;
    Ok(usage.max_rss())
}

pub fn write_outputs(dir: &Path, matrix: &Matrix) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let json = shapekit_manifest::xdr::render_json(
        &serde_json::to_value(matrix).context("rendering the matrix")?,
    );
    let json_path = dir.join("divergence-matrix.json");
    std::fs::write(&json_path, json).with_context(|| format!("writing {}", json_path.display()))?;
    let md_path = dir.join("MATRIX.md");
    std::fs::write(&md_path, crate::matrix_md::render(matrix))
        .with_context(|| format!("writing {}", md_path.display()))?;
    Ok(())
}

pub fn load_matrix(path: &Path) -> Result<Matrix> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let value =
        crate::testee::parse_deep(&text).with_context(|| format!("parsing {}", path.display()))?;
    serde_json::from_value(value).with_context(|| format!("reading {}", path.display()))
}
