//! `shapekit coverage` and `shapekit corpus pack`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(clap::Args)]
pub struct CoverageArgs {
    /// Fail instead of writing when a gap has no reason or the committed files are stale.
    #[arg(long)]
    check: bool,
    /// The corpus directory, holding manifest.json.
    #[arg(long, default_value = "corpus")]
    corpus: PathBuf,
    /// Directory holding the registry files.
    #[arg(long, default_value = "registry")]
    registry: PathBuf,
}

#[derive(Subcommand)]
pub enum CorpusCommand {
    /// Write corpus-<version>.tar.zst and SHA256SUMS from the committed corpus, byte for byte
    /// the same on every run.
    Pack {
        /// The corpus directory, holding manifest.json.
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
        /// Directory holding the registry files.
        #[arg(long, default_value = "registry")]
        registry: PathBuf,
        /// Directory for the archive and SHA256SUMS.
        #[arg(long, default_value = "dist")]
        out: PathBuf,
    },
    /// Move entries from the candidate lane into the corpus, with the chains they are cut
    /// from: the current versions of rust, go and js pass each, or a main finding with a
    /// drafted issue takes the failure, and every other failure is attributed to a finding,
    /// a candidate finding or a record of candidate/finding-candidates.toml.
    Promote(crate::promote_cmd::PromoteArgs),
}

pub fn coverage(args: CoverageArgs) -> Result<bool> {
    let (report, problems) =
        shapekit_coverage::run(&args.corpus, &args.registry).context("computing coverage")?;
    let rendered = shapekit_coverage::render(&report);
    let s = &report.summary;
    let summary = format!(
        "{} entries ({} core-emitted); {} of {} indexer-visible caps core-emitted, {} with a reason; {} of {} arms covered; {} of {} fragments covered; {} of {} not fully validated fields exercised",
        s.entries,
        s.core_emitted_entries,
        s.caps_indexer_visible_core_emitted,
        s.caps_indexer_visible,
        s.caps_indexer_visible_with_reason,
        s.arms_covered,
        s.arms_reachable,
        s.fragments_covered,
        s.fragments,
        s.fields_not_fully_validated_exercised,
        s.fields_not_fully_validated,
    );
    let mut problems = problems;
    if args.check {
        problems.extend(shapekit_coverage::stale(&args.corpus, &rendered));
    } else {
        shapekit_coverage::write(&args.corpus, &rendered).context("writing coverage")?;
    }
    for p in &problems {
        println!("{p}");
    }
    if problems.is_empty() {
        println!("coverage ok: {summary}");
        Ok(true)
    } else {
        println!("{} coverage problems: {summary}", problems.len());
        Ok(false)
    }
}

pub fn corpus(command: CorpusCommand) -> Result<bool> {
    match command {
        CorpusCommand::Pack {
            corpus,
            registry,
            out,
        } => pack(&corpus, &registry, &out),
        CorpusCommand::Promote(args) => crate::promote_cmd::run(args),
    }
}

fn pack(corpus: &Path, registry: &Path, out: &Path) -> Result<bool> {
    // a release must carry the coverage of exactly the entries it ships
    let (report, problems) =
        shapekit_coverage::run(corpus, registry).context("computing coverage")?;
    let mut problems = problems;
    problems.extend(shapekit_coverage::stale(
        corpus,
        &shapekit_coverage::render(&report),
    ));
    if !problems.is_empty() {
        for p in &problems {
            println!("{p}");
        }
        println!(
            "{} coverage problems, not packing; run shapekit coverage --check",
            problems.len()
        );
        return Ok(false);
    }
    let manifest = shapekit_coverage::load_manifest(corpus)?;
    let r = shapekit_coverage::pack::pack(corpus, &manifest, out).context("packing the corpus")?;
    println!(
        "{}: {} files, {} bytes, {} compressed, sha256 {}",
        r.archive.display(),
        r.files,
        r.uncompressed_bytes,
        r.compressed_bytes,
        r.sha256
    );
    println!("{}", r.sums.display());
    Ok(true)
}
