//! `shapekit conformance seed-check`: seeds faults into an sdk's encoding and helper code with
//! the language's mutation tool, runs the corpus against each seeded build and counts how many
//! the corpus catches.
//!
//! A seeded fault counts as caught when the corpus run on the faulty build differs from the run
//! on the unmodified build: some entry's checks change result or answer, the run hangs, or the
//! testee no longer starts. Known failures are compared like everything else, so a fault that
//! only changes an answer the corpus already marks as wrong is caught too, while a fault whose
//! effect no entry observes is missed.
//!
//! The population is every fault the tool generates in the chosen source files; a seeded
//! sample of it is run. `probe` is the per-fault step each tool calls back into.

pub mod go;
pub mod js;
pub mod rust;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use shapekit_contracts::obligations::{
    Obligation, ObligationKind, ObligationLedger, ObligationStatus,
};

use crate::runner::{self, Cell, RunOptions, Selection};
use crate::versions::VersionsFile;

pub const REPORT_SCHEMA: u32 = 1;

/// Paths shared by every step, all absolute.
#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
    pub manifest: PathBuf,
    pub versions: PathBuf,
    pub known_dir: PathBuf,
}

impl Paths {
    pub fn new(
        root: &Path,
        manifest: &Path,
        versions: &Path,
        known_dir: &Path,
    ) -> Result<Paths> {
        let abs = |p: &Path| -> Result<PathBuf> {
            let p = if p.is_absolute() {
                p.to_path_buf()
            } else {
                root.join(p)
            };
            std::fs::canonicalize(&p).with_context(|| format!("resolving {}", p.display()))
        };
        Ok(Paths {
            root: abs(root)?,
            manifest: abs(manifest)?,
            versions: abs(versions)?,
            known_dir: abs(known_dir)?,
        })
    }
}

/// The cells of one sdk version in one corpus run, by entry id.
pub type Fingerprint = BTreeMap<String, Cell>;

/// Runs the whole corpus on one version with `command` as its testee and returns its cells.
pub fn corpus_cells(paths: &Paths, sdk: &str, version: &str, command: &[String]) -> Result<Fingerprint> {
    let outcome = runner::run(&RunOptions {
        root: paths.root.clone(),
        manifest: paths.manifest.clone(),
        versions: paths.versions.clone(),
        known_dir: paths.known_dir.clone(),
        sdks: vec![sdk.to_string()],
        only_versions: vec![version.to_string()],
        all_versions: false,
        selection: Selection::default(),
        build: false,
        out: None,
        passes: 1,
        command: Some(command.to_vec()),
    })?;
    let key = format!("{sdk}@{version}");
    let mut out = Fingerprint::new();
    for row in outcome.matrix.entries {
        let cell = row
            .cells
            .get(&key)
            .with_context(|| format!("{} has no cell for {key}", row.id))?;
        out.insert(row.id, cell.clone());
    }
    Ok(out)
}

/// What one faulty build did to the corpus run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "outcome")]
pub enum ProbeOutcome {
    /// Every entry gave the same results and answers as on the unmodified build.
    Unchanged,
    /// These entries changed.
    Changed { entries: Vec<String> },
    /// The corpus run did not complete; the message says where it stopped.
    Failed { message: String },
}

impl ProbeOutcome {
    pub fn caught(&self) -> bool {
        !matches!(self, ProbeOutcome::Unchanged)
    }
}

pub fn compare(baseline: &Fingerprint, now: &Fingerprint) -> ProbeOutcome {
    let mut entries: Vec<String> = Vec::new();
    for (id, cell) in now {
        if baseline.get(id) != Some(cell) {
            entries.push(id.clone());
        }
    }
    for id in baseline.keys() {
        if !now.contains_key(id) {
            entries.push(id.clone());
        }
    }
    entries.sort();
    entries.dedup();
    if entries.is_empty() {
        ProbeOutcome::Unchanged
    } else {
        ProbeOutcome::Changed { entries }
    }
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

/// The per-fault step: runs the corpus on the build `command` starts and compares it with the
/// baseline. The outcome goes to `record`; the caller maps it to its tool's exit codes.
pub fn probe(
    paths: &Paths,
    sdk: &str,
    version: &str,
    command: &[String],
    baseline: &Path,
    record: &Path,
) -> Result<ProbeOutcome> {
    let base: Fingerprint = read_json(baseline)?;
    let outcome = match corpus_cells(paths, sdk, version, command) {
        Ok(now) => compare(&base, &now),
        Err(e) => ProbeOutcome::Failed {
            message: format!("{e:#}"),
        },
    };
    write_json(record, &outcome)?;
    Ok(outcome)
}

/// The unmodified build's cells, the reference every fault is compared with. The run must not
/// fail: a baseline that cannot run makes every fault look caught.
pub fn write_baseline(paths: &Paths, sdk: &str, version: &str, command: &[String], out: &Path) -> Result<()> {
    let cells = corpus_cells(paths, sdk, version, command)
        .context("running the corpus on the unmodified build")?;
    write_json(out, &cells)
}

/// A seeded, reproducible sample of `k` distinct indices below `n`, in ascending order.
pub fn sample(n: usize, k: usize, seed: u64) -> Vec<usize> {
    // splitmix64, then a partial fisher-yates shuffle
    let mut state = seed;
    let mut next = || {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    };
    let k = k.min(n);
    let mut idx: Vec<usize> = (0..n).collect();
    for i in 0..k {
        let span = (n - i) as u64;
        let j = i + (next() % span) as usize;
        idx.swap(i, j);
    }
    let mut out = idx[..k].to_vec();
    out.sort_unstable();
    out
}

/// What became of one seeded fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// The corpus run changed.
    Caught,
    /// The corpus run did not finish within the tool's timeout.
    Timeout,
    /// The corpus run was the same as on the unmodified build.
    Missed,
    /// The faulty code did not build, so there was nothing to run.
    Unviable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fault {
    /// Stable within one sdk version and tool version: file, position and what the tool did.
    pub key: String,
    pub file: String,
    pub line: u32,
    pub mutation: String,
    pub status: Status,
    /// Entries whose results changed, sorted; at most `WITNESS_LIMIT` of them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<String>,
    /// How many entries changed in all.
    #[serde(default)]
    pub entries_changed: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// How many changed entries a fault lists.
pub const WITNESS_LIMIT: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealFault {
    pub issue: String,
    pub reproductions: Vec<String>,
    pub caught: bool,
}

/// `conformance/seed-check/<sdk>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub schema: u32,
    pub sdk: String,
    pub package: String,
    pub version: String,
    pub tool: Tool,
    /// The source files faults are seeded into, as globs relative to the package root.
    pub scope: Vec<String>,
    /// Every fault the tool generates in the scope.
    pub population: usize,
    pub seed: u64,
    pub sample: usize,
    pub counts: BTreeMap<Status, usize>,
    pub faults: Vec<Fault>,
    /// Released versions with a fixed defect, from reproductions.toml, and whether the
    /// corpus catches each.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub real_faults: Vec<RealFault>,
}

impl Report {
    /// Caught or timed out, out of the faults that built.
    pub fn rate(&self) -> (usize, usize) {
        let get = |s| self.counts.get(&s).copied().unwrap_or(0);
        let caught = get(Status::Caught) + get(Status::Timeout);
        (caught, caught + get(Status::Missed))
    }
}

pub fn counts(faults: &[Fault]) -> BTreeMap<Status, usize> {
    let mut out = BTreeMap::new();
    for s in [Status::Caught, Status::Timeout, Status::Missed, Status::Unviable] {
        out.insert(s, 0);
    }
    for f in faults {
        *out.entry(f.status).or_insert(0) += 1;
    }
    out
}

/// A fault's outcome as the report keeps it.
pub fn fault_from(key: String, file: String, line: u32, mutation: String, status: Status, probe: Option<&ProbeOutcome>) -> Fault {
    let (entries, changed, note) = match probe {
        Some(ProbeOutcome::Changed { entries }) => (
            entries.iter().take(WITNESS_LIMIT).cloned().collect(),
            entries.len(),
            None,
        ),
        Some(ProbeOutcome::Failed { message }) => (
            Vec::new(),
            0,
            Some(format!("the corpus run stopped: {message}")),
        ),
        _ => (Vec::new(), 0, None),
    };
    Fault {
        key,
        file,
        line,
        mutation,
        status,
        entries,
        entries_changed: changed,
        note,
    }
}

/// One `seed` obligation per sampled fault that built. A caught fault is met by the entries
/// that changed; a fault that stopped the run before any entry, timed out or was missed stays
/// open with what is known.
pub fn obligations(report: &Report) -> ObligationLedger {
    let mut out = Vec::new();
    for f in &report.faults {
        if f.status == Status::Unviable {
            continue;
        }
        let id = Obligation::id_for(
            ObligationKind::Seed,
            &format!("{}@{}:{}", report.sdk, report.version, f.key),
        );
        let (status, witnesses, evidence) = match f.status {
            Status::Caught if !f.entries.is_empty() => (
                ObligationStatus::Met,
                f.entries.iter().map(|e| format!("entry:{e}")).collect(),
                Some(format!(
                    "{}; {} entries change",
                    f.mutation, f.entries_changed
                )),
            ),
            _ => (
                ObligationStatus::Open,
                Vec::new(),
                Some(match (&f.status, &f.note) {
                    (_, Some(n)) => format!("{}; {n}", f.mutation),
                    (Status::Timeout, None) => format!("{}; the corpus run timed out", f.mutation),
                    _ => format!("{}; no entry changes", f.mutation),
                }),
            ),
        };
        out.push(Obligation {
            id,
            kind: ObligationKind::Seed,
            status,
            witnesses,
            evidence,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out.dedup_by(|a, b| a.id == b.id);
    ObligationLedger {
        schema_version: 1,
        obligations: out,
    }
}

pub fn render_md(reports: &[Report]) -> String {
    let mut out = String::from(
        "# Seeded faults\n\n\
         Written by `shapekit conformance seed-check`. Each sdk's mutation tool seeds faults \
         into the sdk's encoding and helper code; a sample of them is run against the corpus. \
         A fault is caught when the corpus run on the faulty build differs from the run on the \
         unmodified build.\n\n\
         | sdk | version | tool | faults generated | sampled | caught | timed out | missed | did not build | caught of those that built |\n\
         |---|---|---|---|---|---|---|---|---|---|\n",
    );
    for r in reports {
        let g = |s| r.counts.get(&s).copied().unwrap_or(0);
        let (c, n) = r.rate();
        let pct = if n == 0 {
            "none built".to_string()
        } else {
            format!("{c} of {n} ({:.1}%)", 100.0 * c as f64 / n as f64)
        };
        out.push_str(&format!(
            "| {} | {} | {} {} | {} | {} | {} | {} | {} | {} | {pct} |\n",
            r.sdk,
            r.version,
            r.tool.name,
            r.tool.version,
            r.population,
            r.sample,
            g(Status::Caught),
            g(Status::Timeout),
            g(Status::Missed),
            g(Status::Unviable)
        ));
    }
    for r in reports {
        if !r.real_faults.is_empty() {
            out.push_str(&format!(
                "\n## Released {} faults\n\n| issue | reproductions | caught |\n|---|---|---|\n",
                r.sdk
            ));
            for f in &r.real_faults {
                out.push_str(&format!(
                    "| {} | {} | {} |\n",
                    f.issue,
                    f.reproductions.join(", "),
                    if f.caught { "yes" } else { "no" }
                ));
            }
        }
        let missed: Vec<&Fault> = r
            .faults
            .iter()
            .filter(|f| f.status == Status::Missed)
            .collect();
        out.push_str(&format!(
            "\n## Missed in {} {}\n\n",
            r.sdk, r.version
        ));
        if missed.is_empty() {
            out.push_str("None.\n");
        } else {
            out.push_str("| file | line | mutation |\n|---|---|---|\n");
            for f in missed {
                out.push_str(&format!(
                    "| {} | {} | {} |\n",
                    f.file,
                    f.line,
                    f.mutation.replace('|', "\\|").replace('\n', " ")
                ));
            }
        }
    }
    out
}

/// Runs a command to completion, failing on a non-zero exit.
pub fn run(argv: &[String], dir: &Path, env: &[(String, String)]) -> Result<()> {
    ensure!(!argv.is_empty(), "empty command");
    let status = Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(dir)
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdout(std::io::stderr())
        .status()
        .with_context(|| format!("starting {}", argv.join(" ")))?;
    ensure!(
        status.success(),
        "`{}` in {} exited with {status}",
        argv.join(" "),
        dir.display()
    );
    Ok(())
}

/// Runs a command and returns its stdout.
pub fn output(argv: &[String], dir: &Path, env: &[(String, String)]) -> Result<String> {
    ensure!(!argv.is_empty(), "empty command");
    let out = Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(dir)
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stderr(std::process::Stdio::inherit())
        .output()
        .with_context(|| format!("starting {}", argv.join(" ")))?;
    ensure!(
        out.status.success(),
        "`{}` in {} exited with {}",
        argv.join(" "),
        dir.display(),
        out.status
    );
    String::from_utf8(out.stdout).context("command output is not utf-8")
}

pub fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// Copies a directory tree, following no symlinks out of it.
pub fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    run(
        &[
            "cp".to_string(),
            "-R".to_string(),
            from.display().to_string(),
            to.display().to_string(),
        ],
        Path::new("/"),
        &[],
    )
}

/// Everything a driver needs.
pub struct Context {
    pub paths: Paths,
    pub sdk: String,
    pub version: String,
    pub package: String,
    pub work: PathBuf,
    pub sample: usize,
    pub seed: u64,
    pub build: bool,
    pub jobs: usize,
    /// This program, for the per-fault callbacks.
    pub exe: PathBuf,
}

impl Context {
    pub fn new(
        paths: Paths,
        sdk: &str,
        version: Option<&str>,
        work: &Path,
        sample: usize,
        seed: u64,
        build: bool,
        jobs: usize,
    ) -> Result<Context> {
        let file = VersionsFile::load(&paths.versions)?;
        let s = file.sdk(sdk)?;
        let version = version.unwrap_or(&s.current).to_string();
        file.target(sdk, &version)?;
        ensure!(sample > 0, "the sample must hold at least one fault");
        let work = if work.is_absolute() {
            work.to_path_buf()
        } else {
            paths.root.join(work)
        }
        .join(sdk);
        Ok(Context {
            sdk: sdk.to_string(),
            package: s.package.clone(),
            version,
            work,
            sample,
            seed,
            build,
            jobs: jobs.max(1),
            exe: std::env::current_exe().context("locating this program")?,
            paths,
        })
    }

    /// Where the mutation tools are installed, kept across runs.
    pub fn tools(&self) -> PathBuf {
        self.work
            .parent()
            .map_or_else(|| self.work.join("tools"), |p| p.join("tools"))
    }

    /// A fresh work directory.
    pub fn fresh_work(&self) -> Result<()> {
        if self.work.exists() {
            std::fs::remove_dir_all(&self.work)
                .with_context(|| format!("removing {}", self.work.display()))?;
        }
        std::fs::create_dir_all(&self.work)
            .with_context(|| format!("creating {}", self.work.display()))
    }

    /// The argv of a `seed-probe` callback for one fault, recording into `record`.
    pub fn probe_argv(&self, command: &[String], baseline: &Path) -> Vec<String> {
        let mut v = vec![
            self.exe.display().to_string(),
            "conformance".to_string(),
            "seed-probe".to_string(),
            "--root".to_string(),
            self.paths.root.display().to_string(),
            "--manifest".to_string(),
            self.paths.manifest.display().to_string(),
            "--versions-file".to_string(),
            self.paths.versions.display().to_string(),
            "--known-failures".to_string(),
            self.paths.known_dir.display().to_string(),
            "--sdk".to_string(),
            self.sdk.clone(),
            "--version".to_string(),
            self.version.clone(),
            "--baseline".to_string(),
            baseline.display().to_string(),
        ];
        for c in command {
            v.push("--command".to_string());
            v.push(c.clone());
        }
        v
    }
}

/// Runs one sdk's seed check end to end.
pub fn check(ctx: &Context) -> Result<Report> {
    match ctx.sdk.as_str() {
        "js" => js::check(ctx),
        "go" => go::check(ctx),
        "rust" => rust::check(ctx),
        other => bail!("seed-check knows rust, go and js, not {other}"),
    }
}

/// The issues behind the reproductions of `sdk`, each caught when every one of its rows holds.
pub fn real_faults(paths: &Paths, sdk: &str, build: bool) -> Result<Vec<RealFault>> {
    let repro_path = paths.root.join("conformance/reproductions.toml");
    let cells = crate::reproduce::run(&crate::reproduce::ReproduceOptions {
        root: paths.root.clone(),
        manifest: paths.manifest.clone(),
        versions: paths.versions.clone(),
        reproductions: repro_path.clone(),
        sdks: vec![sdk.to_string()],
        build,
    })?;
    let file = crate::reproduce::ReproductionsFile::load(&repro_path)?;
    let mut by_issue: BTreeMap<String, (Vec<String>, bool)> = BTreeMap::new();
    for r in file.reproductions.iter().filter(|r| r.sdk == sdk) {
        let holds = cells
            .iter()
            .filter(|c| c.reproduction == r.id)
            .all(crate::reproduce::Cell::holds);
        let e = by_issue.entry(r.link.clone()).or_insert((Vec::new(), true));
        e.0.push(r.id.clone());
        e.1 &= holds;
    }
    Ok(by_issue
        .into_iter()
        .map(|(issue, (reproductions, caught))| RealFault {
            issue,
            reproductions,
            caught,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::ResultClass;

    #[test]
    fn samples_are_distinct_sorted_and_reproducible() {
        let a = sample(1000, 50, 7);
        assert_eq!(a.len(), 50);
        assert!(a.windows(2).all(|w| w[0] < w[1]));
        assert!(a.iter().all(|&i| i < 1000));
        assert_eq!(a, sample(1000, 50, 7));
        assert_ne!(a, sample(1000, 50, 8));
        assert_eq!(sample(3, 10, 1), vec![0, 1, 2]);
    }

    fn cell(result: ResultClass) -> Cell {
        Cell {
            result,
            known_issue: None,
            counts: BTreeMap::from([(result, 1)]),
            checks: Vec::new(),
            reference_checks: Vec::new(),
        }
    }

    #[test]
    fn a_change_in_any_entry_is_caught() {
        let base: Fingerprint = BTreeMap::from([
            ("a".to_string(), cell(ResultClass::Pass)),
            ("b".to_string(), cell(ResultClass::Pass)),
        ]);
        assert_eq!(compare(&base, &base.clone()), ProbeOutcome::Unchanged);
        let mut now = base.clone();
        now.insert("b".to_string(), cell(ResultClass::DivergesFromNetwork));
        assert_eq!(
            compare(&base, &now),
            ProbeOutcome::Changed {
                entries: vec!["b".to_string()]
            }
        );
        now.remove("a");
        assert_eq!(
            compare(&base, &now),
            ProbeOutcome::Changed {
                entries: vec!["a".to_string(), "b".to_string()]
            }
        );
    }

    #[test]
    fn obligations_follow_the_outcome() {
        let mk = |key: &str, status, entries: Vec<&str>, note: Option<&str>| Fault {
            key: key.to_string(),
            file: "f.js".to_string(),
            line: 1,
            mutation: "x".to_string(),
            status,
            entries: entries.iter().map(|s| s.to_string()).collect(),
            entries_changed: entries.len(),
            note: note.map(str::to_string),
        };
        let report = Report {
            schema: REPORT_SCHEMA,
            sdk: "js".to_string(),
            package: "p".to_string(),
            version: "1.0.0".to_string(),
            tool: Tool {
                name: "t".to_string(),
                version: "1".to_string(),
            },
            scope: vec![],
            population: 10,
            seed: 1,
            sample: 4,
            counts: BTreeMap::new(),
            faults: vec![
                mk("a", Status::Caught, vec!["e1"], None),
                mk("b", Status::Missed, vec![], None),
                mk("c", Status::Unviable, vec![], None),
                mk("d", Status::Caught, vec![], Some("did not start")),
            ],
            real_faults: vec![],
        };
        let ledger = obligations(&report);
        let got: Vec<(&str, ObligationStatus, usize)> = ledger
            .obligations
            .iter()
            .map(|o| (o.id.as_str(), o.status, o.witnesses.len()))
            .collect();
        assert_eq!(
            got,
            [
                ("seed:js@1.0.0:a", ObligationStatus::Met, 1),
                ("seed:js@1.0.0:b", ObligationStatus::Open, 0),
                ("seed:js@1.0.0:d", ObligationStatus::Open, 0),
            ]
        );
        use shapekit_contracts::Contract as _;
        assert!(ledger.problems().is_empty(), "{:?}", ledger.problems());
    }
}
