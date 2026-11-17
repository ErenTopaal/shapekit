//! Per-CAP coverage of the corpus (`corpus/coverage/`) and the deterministic release archive
//! of the committed corpus.
//!
//! Coverage counts only core-emitted entries as covering a union arm or a family fragment:
//! core goldens, local core and public network history, never synthetic bytes or envelopes core
//! refused. Every gap must carry a one-line reason in `registry/uncovered.toml`.

pub mod compute;
pub mod evidence;
pub mod ledger;
mod markdown;
pub mod pack;
pub mod uncovered;

use std::path::{Path, PathBuf};

use shapekit_manifest::{Manifest, Problem};
use shapekit_registry::Registry;

pub use compute::{Coverage, EntryShape, ROOT_TYPE, compute, entry_shapes, is_core_emitted};
pub use uncovered::{Blocker, UNCOVERED_FILE, UncoveredFile};

/// Where the report lives, relative to the corpus directory.
pub const COVERAGE_DIR: &str = "coverage";
pub const COVERAGE_JSON: &str = "coverage.json";
pub const COVERAGE_MD: &str = "COVERAGE.md";

#[derive(Debug, thiserror::Error)]
pub enum CoverageError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("parsing {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("{0}")]
    Entry(String),
    #[error("{0}")]
    Pack(String),
}

/// The two report files, rendered.
pub struct Rendered {
    pub json: String,
    pub markdown: String,
    /// `obligations.json`, when the report carries the merged ledger.
    pub ledger: Option<String>,
}

/// The report over the corpus and registry on disk, and every problem `--check` reports
/// other than staleness.
pub fn run(corpus: &Path, registry_dir: &Path) -> Result<(Coverage, Vec<Problem>), CoverageError> {
    let manifest = load_manifest(corpus)?;
    let registry = Registry::load(registry_dir).map_err(|e| CoverageError::Parse {
        path: registry_dir.to_path_buf(),
        message: e.to_string(),
    })?;
    let uncovered = UncoveredFile::load(registry_dir)?;
    let shapes = entry_shapes(&manifest, corpus)?;
    let (mut report, mut problems) = compute(&manifest, &registry, &uncovered, &shapes)?;
    // the ledger's other sources sit next to the corpus directory, at the repository root
    let root = corpus
        .canonicalize()
        .map_err(|source| CoverageError::Io {
            path: corpus.to_path_buf(),
            source,
        })?
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    if let Some(candidates) = evidence::candidate_manifest(&root)? {
        let dir = root.join(evidence::CANDIDATE_MANIFEST);
        let dir = dir.parent().unwrap_or(&root);
        let (rows, evidence_problems) = evidence::compare(&report, &candidates, dir)?;
        report.uncovered_evidence = rows;
        problems.extend(evidence_problems);
    }
    let (ledger, ledger_problems) = ledger::build(&root, corpus, registry_dir)?;
    report.obligations = ledger::rows(&ledger);
    problems.extend(ledger::gate_evidence_problems(&ledger));
    report.ledger = Some(ledger);
    problems.extend(ledger_problems);
    Ok((report, problems))
}

pub fn load_manifest(corpus: &Path) -> Result<Manifest, CoverageError> {
    let path = corpus.join("manifest.json");
    shapekit_manifest::build::load(&path).map_err(|e| CoverageError::Parse {
        path,
        message: e.to_string(),
    })
}

pub fn render(coverage: &Coverage) -> Rendered {
    let value = serde_json::to_value(coverage).expect("the report is plain data");
    Rendered {
        json: shapekit_manifest::xdr::render_json(&value),
        markdown: markdown::render(coverage),
        ledger: coverage.ledger.as_ref().map(ledger::render),
    }
}

/// Writes both files under `<corpus>/coverage/`.
pub fn write(corpus: &Path, rendered: &Rendered) -> Result<(), CoverageError> {
    let dir = corpus.join(COVERAGE_DIR);
    std::fs::create_dir_all(&dir).map_err(|source| CoverageError::Io {
        path: dir.clone(),
        source,
    })?;
    let files = [
        Some((COVERAGE_JSON, &rendered.json)),
        Some((COVERAGE_MD, &rendered.markdown)),
        rendered.ledger.as_ref().map(|l| (ledger::LEDGER_JSON, l)),
    ];
    for (name, text) in files.into_iter().flatten() {
        let path = dir.join(name);
        std::fs::write(&path, text).map_err(|source| CoverageError::Io { path, source })?;
    }
    Ok(())
}

/// A problem for each report file on disk that differs from `rendered`.
pub fn stale(corpus: &Path, rendered: &Rendered) -> Vec<Problem> {
    let dir = corpus.join(COVERAGE_DIR);
    let mut out = Vec::new();
    let files = [
        Some((COVERAGE_JSON, &rendered.json)),
        Some((COVERAGE_MD, &rendered.markdown)),
        rendered.ledger.as_ref().map(|l| (ledger::LEDGER_JSON, l)),
    ];
    for (name, text) in files.into_iter().flatten() {
        let path = dir.join(name);
        match std::fs::read_to_string(&path) {
            Ok(current) if &current == text => {}
            Ok(_) => out.push(Problem::new(
                path.display().to_string(),
                "stale, run shapekit coverage",
            )),
            Err(e) => out.push(Problem::new(
                path.display().to_string(),
                format!("cannot read ({e}), run shapekit coverage"),
            )),
        }
    }
    out
}
