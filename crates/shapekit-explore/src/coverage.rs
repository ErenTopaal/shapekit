//! Core's region counts from the twin's export, over all of core's non-test source rather
//! than only the directories the core coverage summary measures: protocol gates sit in
//! `src/herder`, `src/bucket` and `src/main` too.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use shapekit_twin::coverage::{Counts, Span, repo_path};

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
    regions: Vec<Vec<u64>>,
}

/// llvm-cov's kind of plain code regions.
const CODE_REGION: u64 = 0;

/// Whether a repository path is core's own non-test source.
pub fn counted(path: &str) -> bool {
    path.starts_with("src/") && !path.contains("/test/") && !path.starts_with("src/rust/")
}

/// Reads an `llvm-cov export` file into region counts of core's non-test source; a region
/// compiled into several functions counts as its most-run copy.
pub fn read_export_all(path: &Path) -> Result<Counts> {
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
                let Some(name) = f.filenames.get(*file_id as usize) else {
                    bail!("region {r:?} names file {file_id}, not in its function");
                };
                let Some(p) = repo_path(name) else { continue };
                if !counted(&p) {
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

/// The count of the innermost region holding position `line:column` of `file`, which is how
/// often the code at that position ran. `None` when no region holds it.
pub fn count_at(counts: &Counts, file: &str, line: u32, column: u32) -> Option<u64> {
    let spans = counts.get(file)?;
    let holds = |s: &Span| {
        (s.line_start, s.column_start) <= (line, column)
            && (line, column) < (s.line_end, s.column_end)
    };
    spans
        .iter()
        .filter(|(s, _)| holds(s))
        // the innermost region starts last and, among those, ends first
        .max_by_key(|(s, _)| {
            (
                s.line_start,
                s.column_start,
                std::cmp::Reverse((s.line_end, s.column_end)),
            )
        })
        .map(|(_, c)| *c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(a: u32, b: u32, c: u32, d: u32) -> Span {
        Span {
            line_start: a,
            column_start: b,
            line_end: c,
            column_end: d,
        }
    }

    #[test]
    fn the_innermost_region_decides() {
        let mut counts = Counts::new();
        let f = counts.entry("src/x.cpp".into()).or_default();
        f.insert(span(10, 1, 50, 2), 7);
        f.insert(span(20, 5, 22, 6), 0);
        assert_eq!(count_at(&counts, "src/x.cpp", 21, 9), Some(0));
        assert_eq!(count_at(&counts, "src/x.cpp", 30, 1), Some(7));
        assert_eq!(count_at(&counts, "src/x.cpp", 60, 1), None);
        assert_eq!(count_at(&counts, "src/y.cpp", 21, 9), None);
        assert!(counted("src/herder/Upgrades.cpp"));
        assert!(!counted("src/transactions/test/TxTests.cpp"));
    }
}
