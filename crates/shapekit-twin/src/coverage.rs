//! Branch coverage of core's C++ code for one chain, read from the coverage
//! twin.
//!
//! The chain is recorded on the twin with every container writing its LLVM
//! profile into one host directory ([`CoreImage::profile_dir`]); core writes
//! the profile when it exits, and the driver stops every node with SIGTERM,
//! which core handles by shutting down normally. The profiles are merged with
//! `llvm-profdata` and exported with `llvm-cov export` inside the twin image,
//! next to the binary they describe, and the export is reduced to the core
//! coverage contract.
//!
//! The measured code is the code that decides ledger contents: transaction
//! application (`src/transactions`) and ledger close and meta production
//! (`src/ledger`), without their tests. A region is a code region of the
//! coverage mapping (expansion, skipped and gap regions are left out); a
//! region compiled into several functions (templates, inline functions) is
//! reached when any of its copies is.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use shapekit_contracts::core_coverage::{CoreCoverage, FileCoverage, Region, RegionClass};
use shapekit_contracts::{Contract, CoreRef, ImageRef};
use shapekit_core_driver::config::CoreImage;

/// Directories of core's source the measurement covers.
pub const MEASURED_DIRS: [&str; 2] = ["src/transactions/", "src/ledger/"];

/// Where the twin image's binary sits.
pub const TWIN_BINARY: &str = "/usr/local/bin/stellar-core";

/// llvm-cov's region kind of plain code regions.
const CODE_REGION: u64 = 0;

#[derive(Deserialize)]
struct Export {
    data: Vec<ExportData>,
}

#[derive(Deserialize)]
struct ExportData {
    functions: Vec<ExportFunction>,
}

#[derive(Deserialize)]
struct ExportFunction {
    filenames: Vec<String>,
    /// `[line_start, column_start, line_end, column_end, count, file_id,
    /// expanded_file_id, kind]`.
    regions: Vec<Vec<u64>>,
    /// `[line_start, column_start, line_end, column_end, true_count,
    /// false_count, file_id, expanded_file_id, kind]`: one per condition.
    #[serde(default)]
    branches: Vec<Vec<u64>>,
}

/// Per file, every condition's span and how often it was true and false.
pub type Branches = BTreeMap<String, BTreeMap<Span, (u64, u64)>>;

/// One code region span and the most any copy of it ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Span {
    pub line_start: u32,
    pub column_start: u32,
    pub line_end: u32,
    pub column_end: u32,
}

/// Per file, every code region span and its count.
pub type Counts = BTreeMap<String, BTreeMap<Span, u64>>;

/// The path in core's repository for a filename in the coverage mapping.
/// The twin compiles in `/work/src`, with sources named relative to its
/// `src` directory or absolute.
pub fn repo_path(name: &str) -> Option<String> {
    let rel = name
        .strip_prefix("/work/src/")
        .map(str::to_owned)
        .or_else(|| {
            (!name.starts_with('/')).then(|| {
                let n = name.trim_start_matches("./");
                format!("src/{}", n.trim_start_matches("../src/"))
            })
        })?;
    Some(rel)
}

fn measured(path: &str) -> bool {
    MEASURED_DIRS.iter().any(|d| path.starts_with(d)) && !path.contains("/test/")
}

/// Reads an `llvm-cov export -format=text` file into region counts of the
/// measured files.
pub fn read_export(path: &Path) -> Result<Counts> {
    Ok(read_export_with_branches(path)?.0)
}

/// Region counts of the measured files, and the condition counts of every
/// non-test source file (protocol gates sit outside the measured files too).
pub fn read_export_with_branches(path: &Path) -> Result<(Counts, Branches)> {
    let file = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let export: Export = serde_json::from_reader(std::io::BufReader::new(file))
        .with_context(|| format!("parsing {}", path.display()))?;
    let mut branches = Branches::new();
    for d in &export.data {
        for f in &d.functions {
            for b in &f.branches {
                let [ls, cs, le, ce, t, fls, file_id, _expanded, _kind] = b.as_slice() else {
                    bail!("branch {b:?} does not have the 9 fields of llvm-cov's export format");
                };
                let name = f.filenames.get(*file_id as usize).with_context(|| {
                    format!("branch {b:?} names file {file_id}, not in its function")
                })?;
                let Some(p) = repo_path(name) else { continue };
                if !p.starts_with("src/")
                    || p.contains("/test/")
                    || p.starts_with("src/protocol-curr/")
                    || p.starts_with("src/rust/")
                {
                    continue;
                }
                // llvm-cov writes i64::MAX for a count it cannot compute (seen
                // on switch branches in the generated XDR headers); such a
                // branch says nothing, so it is left out
                if *t >= i64::MAX as u64 || *fls >= i64::MAX as u64 {
                    continue;
                }
                let span = Span {
                    line_start: *ls as u32,
                    column_start: *cs as u32,
                    line_end: *le as u32,
                    column_end: *ce as u32,
                };
                let e = branches.entry(p).or_default().entry(span).or_insert((0, 0));
                e.0 = e.0.saturating_add(*t);
                e.1 = e.1.saturating_add(*fls);
            }
        }
    }
    let mut counts = Counts::new();
    for d in &export.data {
        for f in &d.functions {
            for r in &f.regions {
                let [ls, cs, le, ce, count, file_id, _expanded, kind] = r.as_slice() else {
                    bail!("region {r:?} does not have the 8 fields of llvm-cov's export format");
                };
                if *kind != CODE_REGION {
                    continue;
                }
                let name = f.filenames.get(*file_id as usize).with_context(|| {
                    format!("region {r:?} names file {file_id}, not in its function")
                })?;
                let Some(p) = repo_path(name) else { continue };
                if !measured(&p) {
                    continue;
                }
                let span = Span {
                    line_start: *ls as u32,
                    column_start: *cs as u32,
                    line_end: *le as u32,
                    column_end: *ce as u32,
                };
                let c = counts.entry(p).or_default().entry(span).or_insert(0);
                *c = (*c).max(*count);
            }
        }
    }
    Ok((counts, branches))
}

/// Writes branch counts as JSON: file to `[line_start, column_start,
/// line_end, column_end, true_count, false_count]` rows.
pub fn write_branches(path: &Path, branches: &Branches) -> Result<()> {
    let rows: BTreeMap<&String, Vec<[u64; 6]>> = branches
        .iter()
        .map(|(f, m)| {
            (
                f,
                m.iter()
                    .map(|(s, (t, fl))| {
                        [
                            u64::from(s.line_start),
                            u64::from(s.column_start),
                            u64::from(s.line_end),
                            u64::from(s.column_end),
                            *t,
                            *fl,
                        ]
                    })
                    .collect(),
            )
        })
        .collect();
    std::fs::write(path, serde_json::to_vec(&rows)?)
        .with_context(|| format!("writing {}", path.display()))
}

pub fn read_branches(path: &Path) -> Result<Branches> {
    let rows: BTreeMap<String, Vec<[u64; 6]>> = serde_json::from_slice(
        &std::fs::read(path).with_context(|| format!("reading {}", path.display()))?,
    )
    .with_context(|| format!("parsing {}", path.display()))?;
    Ok(rows
        .into_iter()
        .map(|(f, v)| {
            let m = v
                .into_iter()
                .map(|[a, b, c, d, t, fl]| {
                    (
                        Span {
                            line_start: a as u32,
                            column_start: b as u32,
                            line_end: c as u32,
                            column_end: d as u32,
                        },
                        (t, fl),
                    )
                })
                .collect();
            (f, m)
        })
        .collect())
}

/// The core coverage contract over region counts: per file the reached and
/// unreached spans, and every unreached line range once. An unreached range
/// is `reachable-by-core-tests` when one of `tests` (test name and its
/// counts on the same binary) reaches a span in it, naming those tests, and
/// `unknown` otherwise.
pub fn summary(
    counts: &Counts,
    core: CoreRef,
    chains: Vec<String>,
    tests: &[(String, Counts)],
) -> CoreCoverage {
    let mut files = Vec::new();
    let mut unreached = Vec::new();
    for (path, spans) in counts {
        let reached = spans.values().filter(|c| **c > 0).count() as u64;
        let missed = spans.len() as u64 - reached;
        files.push(FileCoverage {
            path: path.clone(),
            regions_reached: reached,
            regions_unreached: missed,
        });
        let mut ranges: BTreeMap<(u32, u32), std::collections::BTreeSet<String>> = BTreeMap::new();
        for (span, c) in spans {
            if *c > 0 {
                continue;
            }
            let names = ranges.entry((span.line_start, span.line_end)).or_default();
            for (test, tc) in tests {
                if tc
                    .get(path)
                    .and_then(|m| m.get(span))
                    .is_some_and(|n| *n > 0)
                {
                    names.insert(test.clone());
                }
            }
        }
        for ((a, b), names) in ranges {
            unreached.push(Region {
                file: path.clone(),
                line_start: a,
                line_end: b,
                class: if names.is_empty() {
                    RegionClass::Unknown
                } else {
                    RegionClass::ReachableByCoreTests
                },
                core_tests: names.into_iter().collect(),
            });
        }
    }
    CoreCoverage {
        schema_version: <CoreCoverage as Contract>::VERSION,
        core,
        chains,
        files,
        unreached,
    }
}

/// Counts of several runs of the same binary as one: a span's count is the
/// sum of its counts.
pub fn merge_counts<'a>(all: impl IntoIterator<Item = &'a Counts>) -> Counts {
    let mut out = Counts::new();
    for c in all {
        for (file, spans) in c {
            let f = out.entry(file.clone()).or_default();
            for (s, n) in spans {
                *f.entry(*s).or_insert(0) += n;
            }
        }
    }
    out
}

/// Writes counts as JSON: file to `[line_start, column_start, line_end,
/// column_end, count]` rows.
pub fn write_counts(path: &Path, counts: &Counts) -> Result<()> {
    let rows: BTreeMap<&String, Vec<[u64; 5]>> = counts
        .iter()
        .map(|(f, m)| {
            (
                f,
                m.iter()
                    .map(|(s, c)| {
                        [
                            u64::from(s.line_start),
                            u64::from(s.column_start),
                            u64::from(s.line_end),
                            u64::from(s.column_end),
                            *c,
                        ]
                    })
                    .collect(),
            )
        })
        .collect();
    std::fs::write(path, serde_json::to_vec(&rows)?)
        .with_context(|| format!("writing {}", path.display()))
}

pub fn read_counts(path: &Path) -> Result<Counts> {
    let rows: BTreeMap<String, Vec<[u64; 5]>> = serde_json::from_slice(
        &std::fs::read(path).with_context(|| format!("reading {}", path.display()))?,
    )
    .with_context(|| format!("parsing {}", path.display()))?;
    Ok(rows
        .into_iter()
        .map(|(f, v)| {
            let m = v
                .into_iter()
                .map(|[a, b, c, d, n]| {
                    (
                        Span {
                            line_start: a as u32,
                            column_start: b as u32,
                            line_end: c as u32,
                            column_end: d as u32,
                        },
                        n,
                    )
                })
                .collect();
            (f, m)
        })
        .collect())
}

/// The spans of `file` overlapping lines `from..=to`, with their counts.
pub fn spans_in(counts: &Counts, file: &str, from: u32, to: u32) -> Vec<(Span, u64)> {
    counts
        .get(file)
        .map(|m| {
            m.iter()
                .filter(|(s, _)| s.line_start <= to && s.line_end >= from)
                .map(|(s, c)| (*s, *c))
                .collect()
        })
        .unwrap_or_default()
}

/// Merges the profiles in `dir` and exports them, inside the twin image,
/// into `dir/export.json`. Returns the export's path.
pub fn merge_and_export(image: &CoreImage, dir: &Path) -> Result<PathBuf> {
    let mut profiles = 0;
    for e in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let e = e.with_context(|| format!("reading {}", dir.display()))?;
        if e.file_name().to_string_lossy().ends_with(".profraw") {
            profiles += 1;
        }
    }
    if profiles == 0 {
        bail!(
            "no profile in {}: the twin wrote none, so core did not exit normally or the image is not a coverage build",
            dir.display()
        );
    }
    let user = host_user(dir)?;
    let script = format!(
        "set -e; llvm-profdata-20 merge -sparse -o /profiles/merged.profdata /profiles/*.profraw; \
         llvm-cov-20 export -format=text -skip-expansions -instr-profile=/profiles/merged.profdata {TWIN_BINARY} > /profiles/export.json"
    );
    let out = Command::new("docker")
        .args([
            "run",
            "--rm",
            "--network",
            "none",
            "--memory",
            "4g",
            "--cpus",
            "2",
            "--user",
        ])
        .arg(&user)
        .arg("-v")
        .arg(profiles_mount(dir)?)
        .args(["--entrypoint", "sh"])
        .arg(image.reference())
        .args(["-c", &script])
        .output()
        .context("running docker")?;
    if !out.status.success() {
        bail!(
            "merging and exporting the profiles failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(dir.join("export.json"))
}

/// The bind mount of `dir` at `/profiles`. Docker reads a source that is not an absolute
/// path as a volume name, so the source is the directory's absolute path, with symbolic links
/// resolved so the mount names the directory itself.
fn profiles_mount(dir: &Path) -> Result<String> {
    let real = dir
        .canonicalize()
        .with_context(|| format!("resolving {}", dir.display()))?;
    let real = real
        .to_str()
        .with_context(|| format!("{} is not UTF-8", real.display()))?;
    if real.contains(':') {
        bail!("{real} holds a colon, which a docker bind mount cannot name");
    }
    Ok(format!("{real}:/profiles"))
}

fn host_user(dir: &Path) -> Result<String> {
    use std::os::unix::fs::MetadataExt;
    let m = std::fs::metadata(dir).with_context(|| format!("reading {}", dir.display()))?;
    Ok(format!("{}:{}", m.uid(), m.gid()))
}

/// Names of core's test cases matching a Catch2 test spec, from the twin.
pub fn list_core_tests(image: &CoreImage, spec: &str) -> Result<Vec<String>> {
    let out = Command::new("docker")
        .args([
            "run",
            "--rm",
            "--network",
            "none",
            "--memory",
            "1g",
            "--cpus",
            "1",
        ])
        .arg(image.reference())
        .args(["test", "--list-test-names-only", spec])
        .output()
        .context("running docker")?;
    if !out.status.success() {
        bail!(
            "listing core tests failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let names: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect();
    if names.is_empty() {
        bail!("no core test matches {spec}");
    }
    Ok(names)
}

/// How one core test ran on the twin.
#[derive(Debug, Clone)]
pub struct TestRun {
    pub name: String,
    pub exit_code: Option<i32>,
    pub seconds: f64,
    pub counts: Counts,
}

/// Runs one core test case at every protocol version on the twin, in `dir`
/// (created, emptied of everything but `counts.json` afterwards), and reads
/// its region counts. The arguments are those core's own test dumps use.
pub fn run_core_test(image: &CoreImage, name: &str, dir: &Path) -> Result<TestRun> {
    let work = dir.join("work");
    let profiles = dir.join("profiles");
    std::fs::create_dir_all(&work)?;
    std::fs::create_dir_all(&profiles)?;
    let (work, profiles) = (work.canonicalize()?, profiles.canonicalize()?);
    let started = std::time::Instant::now();
    let out = Command::new("docker")
        .args([
            "run",
            "--rm",
            "--network",
            "none",
            "--memory",
            "1536m",
            "--cpus",
            "1",
            "--user",
        ])
        .arg(host_user(dir)?)
        .arg("-v")
        .arg(format!("{}:/data", work.display()))
        .args(["-w", "/data"])
        .arg("-v")
        .arg(format!("{}:/profiles", profiles.display()))
        .args(["-e", "LLVM_PROFILE_FILE=/profiles/test-%p.profraw"])
        .arg(image.reference())
        .args([
            "test",
            name,
            "--all-versions",
            "--rng-seed",
            "12345",
            "--ll",
            "fatal",
            "-r",
            "simple",
            "--disable-dots",
        ])
        .output()
        .context("running docker")?;
    let seconds = started.elapsed().as_secs_f64();
    std::fs::write(dir.join("test.log"), [out.stdout, out.stderr].concat())?;
    std::fs::remove_dir_all(&work)?;
    let export = merge_and_export(image, &profiles)?;
    let counts = read_export(&export)?;
    write_counts(&dir.join("counts.json"), &counts)?;
    std::fs::remove_dir_all(&profiles)?;
    Ok(TestRun {
        name: name.to_owned(),
        exit_code: out.status.code(),
        seconds,
        counts,
    })
}

/// The image reference the contract records for a twin image.
pub fn image_ref(image: &CoreImage) -> ImageRef {
    ImageRef {
        repository: image.repository.clone(),
        tag: image.tag.clone(),
        digest: image.digest.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_profiles_mount_names_the_resolved_absolute_directory() {
        let base = tempfile::tempdir().unwrap();
        let real = base.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = base.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let want = format!("{}:/profiles", real.canonicalize().unwrap().display());
        assert_eq!(profiles_mount(&link).unwrap(), want);
        // a relative path resolves against the working directory
        let cwd = std::env::current_dir().unwrap();
        let rel = pathdiff(&real, &cwd);
        assert_eq!(profiles_mount(&rel).unwrap(), want);
        assert!(profiles_mount(&base.path().join("missing")).is_err());
    }

    /// `to` relative to `from`, through `..` components.
    fn pathdiff(to: &Path, from: &Path) -> PathBuf {
        let (to, from) = (to.canonicalize().unwrap(), from.canonicalize().unwrap());
        let common = to
            .components()
            .zip(from.components())
            .take_while(|(a, b)| a == b)
            .count();
        let mut out = PathBuf::new();
        for _ in from.components().skip(common) {
            out.push("..");
        }
        for c in to.components().skip(common) {
            out.push(c);
        }
        out
    }

    #[test]
    fn repository_paths() {
        assert_eq!(
            repo_path("/work/src/src/ledger/LedgerManagerImpl.cpp").as_deref(),
            Some("src/ledger/LedgerManagerImpl.cpp")
        );
        assert_eq!(
            repo_path("transactions/SetOptionsOpFrame.cpp").as_deref(),
            Some("src/transactions/SetOptionsOpFrame.cpp")
        );
        assert_eq!(repo_path("/usr/include/c++/v1/vector"), None);
        assert!(!measured("src/transactions/test/PaymentTests.cpp"));
        assert!(measured("src/ledger/LedgerTxn.cpp"));
        assert!(!measured("src/herder/Upgrades.cpp"));
    }

    #[test]
    fn a_region_counts_as_reached_when_any_copy_ran() {
        let span = |a, b| Span {
            line_start: a,
            column_start: 1,
            line_end: b,
            column_end: 2,
        };
        let mut counts = Counts::new();
        let f = counts.entry("src/transactions/A.cpp".into()).or_default();
        f.insert(span(10, 12), 3);
        f.insert(span(14, 15), 0);
        let core = CoreRef {
            version: "v30.0.0".into(),
            commit: "0e49751aa9ddf892302cca02cea7f3f5d97df9f1".into(),
            image: None,
        };
        let mut t = Counts::new();
        t.entry("src/transactions/A.cpp".into())
            .or_default()
            .insert(span(14, 15), 2);
        let s = summary(&counts, core.clone(), vec!["chain:x".into()], &[]);
        assert_eq!(s.files[0].regions_reached, 1);
        assert_eq!(s.files[0].regions_unreached, 1);
        assert_eq!(s.unreached.len(), 1);
        assert_eq!(
            (s.unreached[0].line_start, s.unreached[0].line_end),
            (14, 15)
        );
        assert!(s.problems().is_empty(), "{:?}", s.problems());
        assert_eq!(spans_in(&counts, "src/transactions/A.cpp", 11, 11).len(), 1);
        let with_tests = summary(
            &counts,
            core,
            vec!["chain:x".into()],
            &[("[tx] t".into(), t)],
        );
        assert_eq!(
            with_tests.unreached[0].class,
            RegionClass::ReachableByCoreTests
        );
        assert_eq!(
            with_tests.unreached[0].core_tests,
            vec!["[tx] t".to_owned()]
        );
        assert!(with_tests.problems().is_empty());
        let dir = tempfile::tempdir().unwrap();
        write_counts(&dir.path().join("c.json"), &counts).unwrap();
        assert_eq!(read_counts(&dir.path().join("c.json")).unwrap(), counts);
        assert_eq!(
            merge_counts([&counts, &counts])["src/transactions/A.cpp"][&span(10, 12)],
            6
        );
    }
}
