//! `shapekit guide table` and `shapekit guide check`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use shapekit_guide::{ChecklistFile, CorpusLinks, Sources};
use shapekit_registry::{Registry, ValidatedByCore};

#[derive(Subcommand)]
pub enum GuideCommand {
    /// Render the field table, corpus links, caps table and checklist into the marked
    /// regions of the guide.
    Table(GuideArgs),
    /// Fail when a rendered region differs from the guide, when a field core does not fully
    /// validate has no corpus entry, or when a corpus entry names a field the table lacks.
    Check(GuideArgs),
}

#[derive(Args)]
pub struct GuideArgs {
    /// The guide.
    #[arg(long, default_value = "docs/guide/indexer-hardening.mdx")]
    file: PathBuf,
    /// Directory holding the registry files, including guide-checklist.toml.
    #[arg(long, default_value = "registry")]
    registry: PathBuf,
    /// The corpus manifest; entry links are made relative to its directory.
    #[arg(long, default_value = "corpus/manifest.json")]
    manifest: PathBuf,
    /// Link corpus entries under this URL of the corpus directory instead of by a relative
    /// path, for a copy of the guide published outside this repository.
    #[arg(long, conflicts_with = "plain_entries")]
    corpus_url: Option<String>,
    /// Write corpus entry ids without links.
    #[arg(long)]
    plain_entries: bool,
}

pub fn run(command: GuideCommand) -> Result<bool> {
    match command {
        GuideCommand::Table(args) => table(&args),
        GuideCommand::Check(args) => check(&args),
    }
}

struct Loaded {
    registry: Registry,
    manifest: shapekit_manifest::Manifest,
    checklist: ChecklistFile,
    links: CorpusLinks,
    doc: String,
}

fn load(args: &GuideArgs) -> Result<Loaded> {
    let registry = Registry::load(&args.registry)
        .with_context(|| format!("loading the registry in {}", args.registry.display()))?;
    let manifest = shapekit_manifest::build::load(&args.manifest)
        .with_context(|| format!("loading {}", args.manifest.display()))?;
    let checklist = ChecklistFile::load(&args.registry.join(shapekit_guide::CHECKLIST_FILE))?;
    let doc = std::fs::read_to_string(&args.file)
        .with_context(|| format!("reading {}", args.file.display()))?;
    let links = match (&args.corpus_url, args.plain_entries) {
        (Some(url), _) => {
            if !url.starts_with("https://") {
                bail!("--corpus-url must be an https url, got {url}");
            }
            CorpusLinks::Url(url.clone())
        }
        (None, true) => CorpusLinks::Plain,
        (None, false) => CorpusLinks::Relative(relative_corpus(&args.file, &args.manifest)?),
    };
    Ok(Loaded {
        registry,
        manifest,
        checklist,
        links,
        doc,
    })
}

fn relative_corpus(guide: &Path, manifest: &Path) -> Result<String> {
    let guide_dir = absolute_parent(guide)?;
    let corpus_dir = absolute_parent(manifest)?;
    shapekit_guide::relative_dir(&guide_dir, &corpus_dir).with_context(|| {
        format!(
            "no relative path from {} to {}",
            guide_dir.display(),
            corpus_dir.display()
        )
    })
}

fn absolute_parent(path: &Path) -> Result<PathBuf> {
    let abs =
        std::fs::canonicalize(path).with_context(|| format!("resolving {}", path.display()))?;
    abs.parent()
        .map(Path::to_path_buf)
        .with_context(|| format!("{} has no parent directory", abs.display()))
}

fn table(args: &GuideArgs) -> Result<bool> {
    let l = load(args)?;
    let problems = l.checklist.problems(&l.registry);
    if !problems.is_empty() {
        for p in &problems {
            println!("{p}");
        }
        println!("guide not written: {} checklist problems", problems.len());
        return Ok(false);
    }
    let src = Sources {
        registry: &l.registry,
        manifest: &l.manifest,
        checklist: &l.checklist,
        links: &l.links,
    };
    let out = shapekit_guide::apply(&l.doc, &src)?;
    if out == l.doc {
        println!("{}: regions already current", args.file.display());
    } else {
        std::fs::write(&args.file, &out)
            .with_context(|| format!("writing {}", args.file.display()))?;
        println!("{}: regions rewritten", args.file.display());
    }
    Ok(true)
}

fn check(args: &GuideArgs) -> Result<bool> {
    let l = load(args)?;
    let src = Sources {
        registry: &l.registry,
        manifest: &l.manifest,
        checklist: &l.checklist,
        links: &l.links,
    };
    let report = shapekit_guide::check(&l.doc, &src)?;
    for name in &report.stale {
        println!("stale region {name}: run shapekit guide table");
    }
    for u in &report.uncovered {
        let word = match u.validated_by_core {
            ValidatedByCore::No => "not validated",
            ValidatedByCore::Partial => "partly validated",
            ValidatedByCore::Yes => "validated",
        };
        println!("no corpus entry exercises {} ({word} by core)", u.field);
    }
    for u in &report.unknown {
        println!(
            "corpus entry {} names field {}, which is not in the field table",
            u.entry, u.field
        );
    }
    for p in &report.checklist {
        println!("{p}");
    }
    let summary = format!(
        "{} fields in the table, {} exercised by {} corpus entries; {} stale regions, {} uncovered fields, {} unknown field references, {} checklist problems",
        report.table_fields,
        report.exercised_fields,
        l.manifest.entries.len(),
        report.stale.len(),
        report.uncovered.len(),
        report.unknown.len(),
        report.checklist.len(),
    );
    if report.passed() {
        println!("guide ok: {summary}");
    } else {
        println!("guide check failed: {summary}");
    }
    Ok(report.passed())
}
