//! `shapekit corpus promote`: moves an entry from the candidate lane into the corpus.
//!
//! An entry is promoted when the current version of every SDK in conformance/versions.toml
//! passes it, or when each SDK that fails it has a finding in findings/triage.toml that takes
//! the failure and names a drafted issue under findings/issues/. An entry cut from a chain
//! also needs that chain under corpus/chains, where the harness replays it from. The failing cells of a
//! promoted entry are added to conformance/known-failures/ with the finding's link, or its
//! draft while nothing is filed, so the corpus run stays green.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use clap::Args;
use shapekit_conformance::versions::VersionsFile;
use shapekit_findings::collect::conformance_matches;
use shapekit_findings::inputs::{CellView, MatrixView};
use shapekit_findings::triage::{FindingDef, Status, Triage};
use shapekit_manifest::{Entry, Manifest, build};
use shapekit_registry::Registry;

#[derive(Args)]
pub struct PromoteArgs {
    /// Id of the candidate entry.
    entry: String,
    /// divergence-matrix.json of a `shapekit conformance run --manifest
    /// candidate/manifest.json` that covers the entry; repeatable, one per run.
    #[arg(long = "matrix", required = true)]
    matrices: Vec<PathBuf>,
    /// Report the decision without moving anything.
    #[arg(long)]
    dry_run: bool,
    #[arg(long, default_value = "candidate")]
    candidate: PathBuf,
    #[arg(long, default_value = "corpus")]
    corpus: PathBuf,
    #[arg(long, default_value = "registry")]
    registry: PathBuf,
    #[arg(long, default_value = "conformance/versions.toml")]
    versions_file: PathBuf,
    #[arg(long, default_value = "conformance/known-failures")]
    known_failures: PathBuf,
    #[arg(long, default_value = "findings/triage.toml")]
    triage: PathBuf,
}

/// Why one SDK's failure is accepted: the finding that takes it and the issue a known-failures
/// line points at.
struct Accepted<'a> {
    sdk: String,
    version: String,
    cell: &'a CellView,
    finding: &'a FindingDef,
    issue: String,
}

pub fn run(args: PromoteArgs) -> Result<bool> {
    let root = std::env::current_dir().context("reading the working directory")?;
    let candidate_file = args.candidate.join("manifest.json");
    let corpus_file = args.corpus.join("manifest.json");
    let candidate = build::load(&candidate_file)
        .with_context(|| format!("loading {}", candidate_file.display()))?;
    let corpus =
        build::load(&corpus_file).with_context(|| format!("loading {}", corpus_file.display()))?;
    let Some(entry) = candidate.entries.iter().find(|e| e.id == args.entry) else {
        bail!("{} has no entry {}", candidate_file.display(), args.entry);
    };
    ensure!(
        !corpus.entries.iter().any(|e| e.id == args.entry),
        "{} already has an entry {}",
        corpus_file.display(),
        args.entry
    );
    let versions = VersionsFile::load(&args.versions_file)?;
    let triage = shapekit_findings::triage::load(&args.triage)?;
    let mut matrices = Vec::new();
    for path in &args.matrices {
        let (bytes, _) = shapekit_findings::inputs::read(path)?;
        let path = path.clone();
        let view = shapekit_findings::parse_on_big_stack(move || {
            shapekit_findings::inputs::matrix(&bytes, &path)
        })?;
        matrices.push(view);
    }

    let (accepted, mut refusals) = decide(&args.entry, &versions, &matrices, &triage, &root);
    // the harness replays an entry from its chain under corpus/chains, so an entry cut from a
    // chain moves only when the corpus holds that chain
    if let Some(chain) = &entry.source.chain
        && !args.corpus.join("chains").join(chain).is_dir()
    {
        refusals.push(format!(
            "its chain {chain} is not under {}, so the harness could not replay the entry",
            args.corpus.join("chains").display()
        ));
    }
    for a in &accepted {
        println!(
            "{}@{}: {} is finding {} ({})",
            a.sdk, a.version, a.cell.result, a.finding.id, a.issue
        );
    }
    if !refusals.is_empty() {
        for r in &refusals {
            println!("refused: {r}");
        }
        println!("{} stays in the candidate lane", args.entry);
        return Ok(false);
    }
    if args.dry_run {
        println!("{} can be promoted", args.entry);
        return Ok(true);
    }
    let registry = Registry::load(&args.registry)
        .with_context(|| format!("loading the registry in {}", args.registry.display()))?;
    let moved = entry_files(entry)?;
    promote(&args, &registry, &candidate, &corpus, &moved)?;
    for a in &accepted {
        add_known_failure(&args.known_failures, &args.entry, a)?;
    }
    println!(
        "promoted {} ({} files); rerun `shapekit oracle run`, `shapekit coverage` and the conformance run for the corpus",
        args.entry,
        moved.len()
    );
    Ok(true)
}

/// Every SDK's current version must pass, or its failure must be a triaged finding with a
/// drafted issue. Returns the accepted failures and the reasons to refuse.
fn decide<'a>(
    entry: &str,
    versions: &VersionsFile,
    matrices: &'a [MatrixView],
    triage: &'a Triage,
    root: &Path,
) -> (Vec<Accepted<'a>>, Vec<String>) {
    let mut accepted = Vec::new();
    let mut refusals = Vec::new();
    for (sdk, def) in &versions.sdk {
        let key = format!("{sdk}@{}", def.current);
        let cell = matrices
            .iter()
            .filter_map(|m| m.entries.iter().find(|e| e.id == entry))
            .find_map(|e| e.cells.get(&key));
        let Some(cell) = cell else {
            refusals.push(format!(
                "no matrix has a result for {entry} on {key}; run `shapekit conformance run --manifest candidate/manifest.json --entry {entry} --sdk {sdk}`"
            ));
            continue;
        };
        if !cell.is_failure() {
            continue;
        }
        // the first finding whose matcher takes the failure owns it, as in findings collect
        let finding = triage.findings.iter().find(|f| {
            f.conformance
                .iter()
                .any(|m| conformance_matches(m, sdk, &def.current, entry))
        });
        let Some(finding) = finding else {
            refusals.push(format!(
                "{key} answers {} and no finding in findings/triage.toml takes it",
                cell.result
            ));
            continue;
        };
        let Some(draft) = finding.draft.as_deref() else {
            refusals.push(format!(
                "{key} answers {}; finding {} takes it but names no draft under findings/issues/",
                cell.result, finding.id
            ));
            continue;
        };
        if !draft.starts_with("findings/issues/") || !root.join(draft).is_file() {
            refusals.push(format!(
                "{key} answers {}; finding {} names draft {draft}, which is not a file under findings/issues/",
                cell.result, finding.id
            ));
            continue;
        }
        let issue = match (finding.status, &finding.link) {
            (Status::Filed | Status::DuplicateOf, Some(link)) => link.clone(),
            _ => draft.to_owned(),
        };
        accepted.push(Accepted {
            sdk: sdk.clone(),
            version: def.current.clone(),
            cell,
            finding,
            issue,
        });
    }
    (accepted, refusals)
}

/// The entry's files relative to the manifest directory: its entry directory for an xdr entry,
/// its vector file otherwise.
fn entry_files(entry: &Entry) -> Result<Vec<String>> {
    if let Some(v) = &entry.vector {
        return Ok(vec![v.path.clone()]);
    }
    let first = entry
        .variants
        .iter()
        .next()
        .with_context(|| format!("entry {} has neither variants nor a vector", entry.id))?
        .1;
    let dir = first
        .xdr_file
        .rsplit_once('/')
        .map(|(d, _)| d.to_owned())
        .with_context(|| format!("{} is not inside an entry directory", first.xdr_file))?;
    Ok(vec![dir])
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    if from.is_file() {
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::copy(from, to)
            .with_context(|| format!("copying {} to {}", from.display(), to.display()))?;
        return Ok(());
    }
    for item in walkdir::WalkDir::new(from).sort_by_file_name() {
        let item = item.with_context(|| format!("walking {}", from.display()))?;
        let target = to.join(
            item.path()
                .strip_prefix(from)
                .expect("inside the walked tree"),
        );
        if item.file_type().is_dir() {
            std::fs::create_dir_all(&target)
                .with_context(|| format!("creating {}", target.display()))?;
        } else {
            std::fs::copy(item.path(), &target)
                .with_context(|| format!("copying {}", item.path().display()))?;
        }
    }
    Ok(())
}

fn remove(path: &Path) -> Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
    .with_context(|| format!("removing {}", path.display()))
}

/// Copies the files into the corpus, rebuilds and validates its manifest, and only then
/// removes them from the candidate lane; a corpus manifest with problems undoes the copy.
fn promote(
    args: &PromoteArgs,
    registry: &Registry,
    candidate: &Manifest,
    corpus: &Manifest,
    files: &[String],
) -> Result<()> {
    for rel in files {
        let to = args.corpus.join(rel);
        ensure!(!to.exists(), "{} already exists", to.display());
    }
    for rel in files {
        copy_tree(&args.candidate.join(rel), &args.corpus.join(rel))?;
    }
    let rebuilt = build::build(
        &args.corpus,
        &corpus.corpus_version,
        corpus.generated_with.clone(),
    );
    let problems = match &rebuilt {
        Ok(m) => shapekit_manifest::validate(m, registry, &args.corpus)
            .iter()
            .map(ToString::to_string)
            .collect(),
        Err(e) => vec![e.to_string()],
    };
    if !problems.is_empty() {
        for rel in files {
            remove(&args.corpus.join(rel))?;
        }
        bail!(
            "the corpus manifest would not validate, nothing moved:\n{}",
            problems.join("\n")
        );
    }
    let rebuilt = rebuilt.expect("checked above");
    write_manifest(&args.corpus, &rebuilt)?;
    for rel in files {
        remove(&args.candidate.join(rel))?;
    }
    let rest = build::build(
        &args.candidate,
        &candidate.corpus_version,
        candidate.generated_with.clone(),
    )
    .with_context(|| format!("rebuilding {}", args.candidate.display()))?;
    write_manifest(&args.candidate, &rest)
}

fn write_manifest(dir: &Path, manifest: &Manifest) -> Result<()> {
    let path = dir.join("manifest.json");
    std::fs::write(&path, build::render(manifest))
        .with_context(|| format!("writing {}", path.display()))
}

/// Appends `<entry> <version> <result> <issue> <checks>` for the accepted failure, under a
/// comment naming its finding.
fn add_known_failure(dir: &Path, entry: &str, a: &Accepted<'_>) -> Result<()> {
    let path = shapekit_conformance::known::path(dir, &a.sdk);
    let mut text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let checks: BTreeSet<&str> = a
        .cell
        .checks
        .iter()
        .filter(|c| c.result != "pass")
        .map(|c| c.check.as_str())
        .collect();
    ensure!(
        !checks.is_empty(),
        "{}@{} fails {entry} but the matrix lists no failing check",
        a.sdk,
        a.version
    );
    if !text.ends_with('\n') && !text.is_empty() {
        text.push('\n');
    }
    let _ = writeln!(text, "# {}: {}", a.finding.id, a.finding.title);
    let _ = writeln!(
        text,
        "{entry} {} {} {} {}",
        a.version,
        a.cell.result,
        a.issue,
        checks.into_iter().collect::<Vec<_>>().join(", ")
    );
    shapekit_conformance::known::parse(&text)
        .with_context(|| format!("the new line would break {}", path.display()))?;
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
}
