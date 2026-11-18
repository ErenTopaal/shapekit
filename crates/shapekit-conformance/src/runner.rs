//! `shapekit conformance run`: every selected entry through every selected testee, the
//! divergence matrix, and the known-failures verdict.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use shapekit_manifest::{Entry, EntryKind, Manifest, build as manifest_build};

use crate::compare::{self, FailureKind, Judged, Outcome, ResultClass, Verdict};
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
    /// How the check failed, where its result alone does not say: set on every `error` and on
    /// a `diverges-from-network` where the sdk crashed instead of answering (`panic`). A
    /// matrix written before the field existed reads with none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<FailureKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub expected: Value,
    pub actual: Value,
    pub source: String,
}

impl CheckRecord {
    /// The result with its kind of failure, as a known-failures line names it.
    pub fn outcome(&self) -> Outcome {
        Outcome {
            result: self.result,
            failure: self.failure,
        }
    }
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
    /// Every check that did not pass, apart from reference checks, and every check of an
    /// implementation-defined entry, so the answer each sdk gave stays in the matrix.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<CheckRecord>,
    /// Checks that agree with `expected_json` on the testee that rendered it (result
    /// `reference`): kept apart from `checks` and from passes, since they are not independent
    /// evidence. The cell's `result` treats them as passes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reference_checks: Vec<String>,
}

impl Cell {
    /// The class an entry is counted under in the summary: `reference` for a passing cell
    /// whose json checks are only the reference agreeing with itself.
    pub fn summary_class(&self) -> ResultClass {
        if self.result == ResultClass::Pass && !self.reference_checks.is_empty() {
            ResultClass::Reference
        } else {
            self.result
        }
    }

    /// The class an entry is shown under to people: like [`Cell::summary_class`], except that
    /// an entry whose checks all passed apart from ops the testee does not implement is
    /// `pass-some-unsupported`, not `unsupported`, which is kept for an entry no check of
    /// which ran. Failures rank as before.
    pub fn entry_class(&self) -> EntryClass {
        match self.summary_class() {
            ResultClass::Pass => EntryClass::Pass,
            ResultClass::Reference => EntryClass::Reference,
            ResultClass::Unsupported => {
                let ran = [ResultClass::Pass, ResultClass::Reference]
                    .iter()
                    .filter_map(|r| self.counts.get(r))
                    .sum::<usize>();
                if ran > 0 {
                    EntryClass::PassSomeUnsupported
                } else {
                    EntryClass::Unsupported
                }
            }
            ResultClass::DivergesFromPeers => EntryClass::DivergesFromPeers,
            ResultClass::DivergesFromNetwork => EntryClass::DivergesFromNetwork,
            ResultClass::Error => EntryClass::Error,
        }
    }
}

/// How an entry is counted for people: the result classes, with an entry that passed every
/// check its testee implements apart from one that ran nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryClass {
    Pass,
    /// Every check that ran passed, and the testee does not implement the others' ops.
    PassSomeUnsupported,
    Reference,
    /// No check ran: the testee implements none of the entry's ops, or the entry is newer
    /// than its protocol.
    Unsupported,
    DivergesFromPeers,
    DivergesFromNetwork,
    Error,
}

impl EntryClass {
    pub fn as_str(self) -> &'static str {
        match self {
            EntryClass::Pass => "pass",
            EntryClass::PassSomeUnsupported => "pass-some-unsupported",
            EntryClass::Reference => "reference",
            EntryClass::Unsupported => "unsupported",
            EntryClass::DivergesFromPeers => "diverges-from-peers",
            EntryClass::DivergesFromNetwork => "diverges-from-network",
            EntryClass::Error => "error",
        }
    }

    /// The words MATRIX.md and the run's output use.
    pub fn label(self) -> &'static str {
        match self {
            EntryClass::PassSomeUnsupported => "pass (some checks unsupported)",
            other => other.as_str(),
        }
    }
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
    /// Set on the testee whose library rendered the corpus's expected values: the rust testee
    /// at the generator's stellar-xdr version is the reference for `expected_json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_for: Option<String>,
}

/// What the reference testee is the reference for.
pub const REFERENCE_FOR: &str = "expected_json";

/// The package whose own XDR-JSON rendering the corpus generator writes as `expected_json`.
pub const REFERENCE_PACKAGE: &str = "stellar-xdr";

/// `divergence-matrix.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Matrix {
    pub schema_version: u32,
    pub corpus_version: String,
    /// The stellar-xdr version that rendered every `expected_json` (the manifest's
    /// `generated_with.stellar_xdr`).
    #[serde(default)]
    pub generator_stellar_xdr: String,
    /// Depth limit of every decode request but a whole ledger's (`xdr.envelope.depth`).
    pub decode_depth: u32,
    /// Depth limit of a `LedgerCloseMeta` decode request (`xdr.meta.depth`).
    pub ledger_decode_depth: u32,
    pub testees: Vec<TesteeInfo>,
    /// Entry counts per testee, by `Cell::summary_class`. An entry is as bad as its worst
    /// check, so one op an sdk lacks makes the whole entry unsupported; `check_summary` and
    /// `op_summary` show what that hides.
    pub summary: BTreeMap<String, BTreeMap<ResultClass, usize>>,
    /// Entry counts per testee, by `Cell::entry_class`: as `summary`, with the entries that
    /// passed every check their testee implements counted apart from the unsupported ones.
    /// Absent in matrices written before it was recorded.
    #[serde(default)]
    pub entry_summary: BTreeMap<String, BTreeMap<EntryClass, usize>>,
    /// Check counts per testee, by result.
    #[serde(default)]
    pub check_summary: BTreeMap<String, BTreeMap<ResultClass, usize>>,
    /// Check counts per testee, by op and result.
    #[serde(default)]
    pub op_summary: BTreeMap<String, OpCounts>,
    pub entries: Vec<EntryRow>,
    /// Known-failures problems that fail the run, as text.
    pub known_failure_problems: Vec<String>,
}

pub const MATRIX_SCHEMA: u32 = 1;

/// One entry's cells, keyed by testee, and the names of all its checks.
pub struct EntryResult {
    pub cells: BTreeMap<String, Cell>,
    pub checks: Vec<String>,
    /// Check counts per testee, op and result.
    pub ops: BTreeMap<String, OpCounts>,
}

/// Check counts by op, then by result.
pub type OpCounts = BTreeMap<String, BTreeMap<ResultClass, usize>>;

/// Runs every check of `entry` on every testee. `reference` is the key of the testee that is
/// the reference for `expected_json`, when it is among them.
pub fn evaluate_entry(
    entry: &Entry,
    corpus: &Path,
    testees: &mut [Testee],
    reference: Option<&str>,
) -> Result<EntryResult> {
    let checks = plan::checks(entry, corpus)?;
    let mut cells: Vec<Cell> = testees
        .iter()
        .map(|_| Cell {
            result: ResultClass::Pass,
            known_issue: None,
            counts: BTreeMap::new(),
            checks: Vec::new(),
            reference_checks: Vec::new(),
        })
        .collect();
    let mut ops: Vec<OpCounts> = testees.iter().map(|_| OpCounts::new()).collect();
    let is_reference: Vec<bool> = testees
        .iter()
        .map(|t| reference == Some(t.target().key().as_str()))
        .collect();
    for check in &checks {
        let verdicts = run_check(check, testees)?;
        for ((((cell, v), t), &r), by_op) in cells
            .iter_mut()
            .zip(verdicts)
            .zip(testees.iter())
            .zip(&is_reference)
            .zip(ops.iter_mut())
        {
            let result = record(cell, check, beyond_protocol(entry, t.target(), v), r);
            *by_op
                .entry(check.op.clone())
                .or_default()
                .entry(result)
                .or_insert(0) += 1;
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
                    failure: Some(FailureKind::Unmappable),
                },
                false,
            );
        }
    }
    let names = if checks.is_empty() {
        vec!["entry".to_string()]
    } else {
        checks.iter().map(|c| c.name.clone()).collect()
    };
    let keys: Vec<String> = testees.iter().map(|t| t.target().key()).collect();
    Ok(EntryResult {
        cells: keys.iter().cloned().zip(cells).collect(),
        checks: names,
        ops: keys.into_iter().zip(ops).collect(),
    })
}

/// Which testees vote in peer judgement: one per sdk, its `current` version when the run holds
/// it, else its last version in the run (versions.toml order, or the order of `--version`).
pub fn voters(targets: &[&Target]) -> Vec<bool> {
    let mut out = vec![false; targets.len()];
    let mut sdks: Vec<&str> = Vec::new();
    for t in targets {
        if !sdks.contains(&t.sdk.as_str()) {
            sdks.push(&t.sdk);
        }
    }
    for sdk in sdks {
        let mine: Vec<usize> = (0..targets.len())
            .filter(|&i| targets[i].sdk == sdk)
            .collect();
        let pick = mine
            .iter()
            .copied()
            .find(|&i| targets[i].current)
            .or_else(|| mine.last().copied());
        if let Some(i) = pick {
            out[i] = true;
        }
    }
    out
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
                failure: None,
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
    let targets: Vec<&Target> = testees.iter().map(Testee::target).collect();
    let mut decided = compare::judge_peers(&peers, &voters(&targets)).into_iter();
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

/// A divergence on an entry newer than the version knows becomes unsupported. Crashes are kept:
/// a panic or a dead process on unknown xdr is a robustness defect, not a missing feature.
pub fn beyond_protocol(entry: &Entry, target: &Target, v: Verdict) -> Verdict {
    let p = entry_protocol(entry);
    match target.max_protocol {
        Some(max)
            if p > max
                && !v.crashed()
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
                failure: None,
            }
        }
        _ => v,
    }
}

// a json check whose expected value is the reference testee's own rendering
fn is_reference_check(check: &Check) -> bool {
    match check.op.as_str() {
        "decode_json" => matches!(check.expect, plan::Expect::Json(_)),
        "encode_json" => true,
        _ => false,
    }
}

// records one verdict and returns the class it was counted under
fn record(cell: &mut Cell, check: &Check, v: Verdict, reference: bool) -> ResultClass {
    let result = if reference && v.result == ResultClass::Pass && is_reference_check(check) {
        cell.reference_checks.push(check.name.clone());
        ResultClass::Reference
    } else {
        v.result
    };
    *cell.counts.entry(result).or_insert(0) += 1;
    if result == ResultClass::Reference {
        return result;
    }
    cell.result = cell.result.max(result);
    if result != ResultClass::Pass || matches!(check.expect, plan::Expect::Peers) {
        cell.checks.push(CheckRecord {
            check: check.name.clone(),
            op: check.op.clone(),
            result,
            failure: v.failure,
            detail: v.detail,
            expected: compare::expected_view(&check.expect),
            actual: v.actual,
            source: check.source.clone(),
        });
    }
    result
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
    /// Starts the one selected version with this command line instead of its own, so a build
    /// of the sdk with a seeded fault runs under the version's settings and known failures.
    pub command: Option<Vec<String>>,
    /// Run the coverage builds of the testees and write what each one covered to
    /// `coverage.json` in `out` (see `versions::coverage_target`).
    pub coverage: bool,
}

pub struct RunOutcome {
    pub matrix: Matrix,
    pub ok: bool,
    /// Testee keys (`<sdk>@<version>`) a known-failures problem is about. A problem that names
    /// no version counts against every version of its sdk the run covered.
    pub failing: BTreeSet<String>,
    /// Peak resident set of the runner after each pass, KiB.
    pub peak_rss_kib: Vec<i64>,
}

/// Resolves the targets a run covers.
pub fn targets(file: &VersionsFile, opts: &RunOptions) -> Result<Vec<Target>> {
    let mut targets = select_targets(file, opts)?;
    if opts.coverage {
        targets = targets
            .iter()
            .map(versions::coverage_target)
            .collect::<Result<_>>()?;
    }
    if let Some(command) = &opts.command {
        ensure!(
            targets.len() == 1 && !command.is_empty(),
            "a replacement command needs exactly one selected version"
        );
        targets[0].command = command.clone();
    }
    Ok(targets)
}

fn select_targets(file: &VersionsFile, opts: &RunOptions) -> Result<Vec<Target>> {
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

/// Loads the manifest and refuses any file reference that leaves the corpus, before a testee
/// starts. The bytes are checked against their sha256 when each entry's checks are built.
pub fn load_manifest(path: &Path) -> Result<(Manifest, PathBuf)> {
    let manifest =
        manifest_build::load(path).with_context(|| format!("loading {}", path.display()))?;
    let corpus = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    for e in &manifest.entries {
        let files = e
            .variants
            .iter()
            .flat_map(|(_, v)| std::iter::once(&v.xdr_file).chain(&v.expected_json_file))
            .chain(e.vector.iter().map(|f| &f.path));
        for f in files {
            ensure!(
                shapekit_manifest::is_safe_relative(f),
                "{}: {f:?} in {} is not a path inside the corpus",
                e.id,
                path.display()
            );
        }
    }
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

    let reference = targets
        .iter()
        .find(|t| {
            t.package == REFERENCE_PACKAGE && t.version == manifest.generated_with.stellar_xdr
        })
        .map(Target::key);

    let mut rows: Vec<EntryRow> = Vec::new();
    let mut check_names: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut op_summary: BTreeMap<String, OpCounts> = BTreeMap::new();
    let mut peak_rss_kib = Vec::new();
    for pass in 0..opts.passes {
        let mut this_pass = Vec::with_capacity(selected.len());
        for entry in &selected {
            let result = evaluate_entry(entry, &corpus, &mut testees, reference.as_deref())
                .with_context(|| format!("running {}", entry.id))?;
            check_names.insert(entry.id.clone(), result.checks);
            if pass == 0 {
                for (key, by_op) in result.ops {
                    let sum = op_summary.entry(key).or_default();
                    for (op, counts) in by_op {
                        for (r, n) in counts {
                            *sum.entry(op.clone()).or_default().entry(r).or_insert(0) += n;
                        }
                    }
                }
            }
            this_pass.push(EntryRow {
                id: entry.id.clone(),
                kind: entry.kind,
                family: entry.family.clone(),
                linked_issues: entry.linked_issues.clone(),
                cells: result.cells,
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

    if opts.coverage {
        let out = opts
            .out
            .as_ref()
            .context("a coverage run needs an output directory")?;
        write_coverage(out, &mut testees)?;
    }

    let testee_info: Vec<TesteeInfo> = testees
        .iter()
        .map(|t| {
            let h = t.hello();
            let key = t.target().key();
            TesteeInfo {
                reference_for: (reference.as_deref() == Some(key.as_str()))
                    .then(|| REFERENCE_FOR.to_string()),
                key,
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
    let mut failing: BTreeSet<String> = BTreeSet::new();
    let sdks: BTreeSet<&str> = targets.iter().map(|t| t.sdk.as_str()).collect();
    for sdk in sdks {
        let lines = known::load(&opts.known_dir, sdk)?;
        let mut results = BTreeMap::new();
        for row in &mut rows {
            for t in targets.iter().filter(|t| t.sdk == sdk) {
                let cell = row
                    .cells
                    .get_mut(&t.key())
                    .expect("every row has a cell per target");
                let checks = cell_checks(&check_names[&row.id], cell);
                cell.known_issue = known::issue_for(
                    &lines,
                    &row.id,
                    &t.version,
                    checks
                        .iter()
                        .filter(|(_, o)| o.is_failure())
                        .map(|(c, _)| c.as_str()),
                );
                results.insert((row.id.clone(), t.version.clone()), checks);
            }
        }
        let all_versions: BTreeSet<String> = file
            .sdk(sdk)?
            .versions
            .iter()
            .map(|v| v.version.clone())
            .collect();
        for p in known::evaluate(&lines, &results, &all_ids, &all_versions) {
            match p.version() {
                Some(v) => {
                    failing.insert(format!("{sdk}@{v}"));
                }
                None => failing.extend(targets.iter().filter(|t| t.sdk == sdk).map(Target::key)),
            }
            problems.push(format!(
                "{}: {p}",
                known::path(&opts.known_dir, sdk).display()
            ));
        }
    }

    let mut summary: BTreeMap<String, BTreeMap<ResultClass, usize>> = BTreeMap::new();
    let mut entry_summary: BTreeMap<String, BTreeMap<EntryClass, usize>> = BTreeMap::new();
    for row in &rows {
        for (key, cell) in &row.cells {
            *summary
                .entry(key.clone())
                .or_default()
                .entry(cell.summary_class())
                .or_insert(0) += 1;
            *entry_summary
                .entry(key.clone())
                .or_default()
                .entry(cell.entry_class())
                .or_insert(0) += 1;
        }
    }
    let matrix = Matrix {
        schema_version: MATRIX_SCHEMA,
        corpus_version: manifest.corpus_version.clone(),
        generator_stellar_xdr: manifest.generated_with.stellar_xdr.clone(),
        decode_depth: shapekit_manifest::ENVELOPE_DEPTH,
        ledger_decode_depth: shapekit_manifest::META_DEPTH,
        testees: testee_info,
        summary,
        entry_summary,
        check_summary: op_summary
            .iter()
            .map(|(key, by_op)| {
                let mut total: BTreeMap<ResultClass, usize> = BTreeMap::new();
                for counts in by_op.values() {
                    for (r, n) in counts {
                        *total.entry(*r).or_insert(0) += n;
                    }
                }
                (key.clone(), total)
            })
            .collect(),
        op_summary,
        entries: rows,
        known_failure_problems: problems,
    };
    if let Some(out) = &opts.out {
        write_outputs(out, &matrix)?;
    }
    Ok(RunOutcome {
        ok: matrix.known_failure_problems.is_empty(),
        failing,
        matrix,
        peak_rss_kib,
    })
}

/// Every check of a cell by name with its result and kind of failure: the recorded non-passes
/// and references, and a pass for every other check the entry has.
pub fn cell_checks(names: &[String], cell: &Cell) -> known::CellChecks {
    let mut out: known::CellChecks = names
        .iter()
        .map(|n| (n.clone(), Outcome::of(ResultClass::Pass)))
        .collect();
    for c in &cell.checks {
        out.insert(c.check.clone(), c.outcome());
    }
    for c in &cell.reference_checks {
        out.insert(c.clone(), Outcome::of(ResultClass::Reference));
    }
    out
}

fn peak_rss() -> Result<i64> {
    use nix::sys::resource::{UsageWho, getrusage};
    let usage = getrusage(UsageWho::RUSAGE_SELF).context("getrusage")?;
    // ru_maxrss is in kilobytes on linux and in bytes on macos (getrusage(2) of each)
    let per_kib = if cfg!(target_os = "macos") { 1024 } else { 1 };
    Ok(usage.max_rss() / per_kib)
}

/// What each testee covered over the whole run, from one `coverage_snapshot` at its end.
/// Counters live in the testee process, so a testee the runner had to restart lost what its
/// earlier processes covered; `processes` says how many there were.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageRecord {
    pub tool: String,
    pub processes: u32,
    pub ranges: Vec<String>,
    /// Ranges the tool reports as never run, for a tool that leaves some ranges out of both
    /// lists (see `coverage_snapshot` in docs/testee-protocol.md).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uncovered: Option<Vec<String>>,
}

fn write_coverage(dir: &Path, testees: &mut [Testee]) -> Result<()> {
    let mut out: BTreeMap<String, CoverageRecord> = BTreeMap::new();
    for t in testees.iter_mut() {
        let key = t.target().key();
        let reply = t
            .request(Map::from_iter([(
                "op".to_string(),
                Value::from("coverage_snapshot"),
            )]))?
            .map_err(|f| anyhow::anyhow!("{key}: coverage_snapshot failed: {f}"))?;
        let crate::testee::Reply::Ok(result) = reply else {
            bail!("{key}: coverage_snapshot was answered with {reply:?}");
        };
        let tool = result["tool"]
            .as_str()
            .with_context(|| format!("{key}: coverage_snapshot has no tool"))?
            .to_string();
        let ranges: Vec<String> = serde_json::from_value(result["ranges"].clone())
            .with_context(|| format!("{key}: coverage_snapshot ranges"))?;
        let uncovered: Option<Vec<String>> = match result.get("uncovered") {
            None => None,
            Some(v) => Some(
                serde_json::from_value(v.clone())
                    .with_context(|| format!("{key}: coverage_snapshot uncovered"))?,
            ),
        };
        out.insert(
            key,
            CoverageRecord {
                tool,
                processes: t.starts,
                ranges,
                uncovered,
            },
        );
    }
    std::fs::create_dir_all(dir)?;
    let mut text = serde_json::to_string_pretty(&out)?;
    text.push('\n');
    std::fs::write(dir.join("coverage.json"), text)
        .with_context(|| format!("writing {}", dir.join("coverage.json").display()))
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
