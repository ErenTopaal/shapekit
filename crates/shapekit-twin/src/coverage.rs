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
}

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
    let file = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let export: Export = serde_json::from_reader(std::io::BufReader::new(file))
        .with_context(|| format!("parsing {}", path.display()))?;
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
    Ok(counts)
}

/// The core coverage contract over region counts: per file the reached and
/// unreached spans, and every unreached line range once. All unreached
/// regions are `unknown` until core's own tests are measured.
pub fn summary(counts: &Counts, core: CoreRef, chains: Vec<String>) -> CoreCoverage {
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
        let mut ranges: Vec<(u32, u32)> = spans
            .iter()
            .filter(|(_, c)| **c == 0)
            .map(|(s, _)| (s.line_start, s.line_end))
            .collect();
        ranges.sort();
        ranges.dedup();
        for (a, b) in ranges {
            unreached.push(Region {
                file: path.clone(),
                line_start: a,
                line_end: b,
                class: RegionClass::Unknown,
                core_tests: Vec::new(),
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
    let profiles = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".profraw"))
        .count();
    if profiles == 0 {
        bail!(
            "no profile in {}: the twin wrote none, so core did not exit normally or the image is not a coverage build",
            dir.display()
        );
    }
    let user = {
        use std::os::unix::fs::MetadataExt;
        let m = std::fs::metadata(dir)?;
        format!("{}:{}", m.uid(), m.gid())
    };
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
        .arg(format!("{}:/profiles", dir.display()))
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
        let s = summary(&counts, core, vec!["chain:x".into()]);
        assert_eq!(s.files[0].regions_reached, 1);
        assert_eq!(s.files[0].regions_unreached, 1);
        assert_eq!(s.unreached.len(), 1);
        assert_eq!(
            (s.unreached[0].line_start, s.unreached[0].line_end),
            (14, 15)
        );
        assert!(s.problems().is_empty(), "{:?}", s.problems());
        assert_eq!(spans_in(&counts, "src/transactions/A.cpp", 11, 11).len(), 1);
    }
}
