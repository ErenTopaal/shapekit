//! `shapekit import goldens` and `shapekit goldens agree`.

use std::path::Path;

use anyhow::{Context, Result};
use shapekit_gen::golden;
use shapekit_manifest::build;
use shapekit_registry::Registry;

pub fn import(
    core_src: &Path,
    tag: &str,
    tier: &str,
    corpus: &Path,
    registry_dir: &Path,
) -> Result<bool> {
    let registry = Registry::load(registry_dir)
        .with_context(|| format!("loading the registry in {}", registry_dir.display()))?;
    let commit = golden::checkout_commit_tier(core_src, tier, tag)?;
    let mut set = golden::scan_tier(core_src, tier, tag, &commit)?;
    println!(
        "indexed {} files ({} ledgers) in {} directories of {} at {tag} ({commit})",
        set.files.len(),
        set.lcms.len(),
        set.dirs.len(),
        set.tier
    );
    let report = golden::import(&mut set, &registry, corpus)?;
    if !report.removed.is_empty() {
        println!(
            "removed {} earlier core-golden entries",
            report.removed.len()
        );
    }
    let mut total = 0;
    for (family, ids) in &report.written {
        total += ids.len();
        println!("{family}: {}", ids.join(", "));
    }
    for n in &report.not_imported {
        println!(
            "not imported: {} fragment {} (smallest carrier {} record {}, {} bytes): {}",
            n.family, n.fragment, n.path, n.record, n.bytes, n.reason
        );
    }
    let families = registry.families.as_ref().map_or(0, |f| f.families.len());
    println!(
        "imported {total} entries for {} of {families} families; wrote {}",
        report.written.len(),
        report.index_path.display()
    );
    // the import rewrites every golden entry.json without its oracle reference, so the oracle
    // runs here; otherwise a re-import of an unchanged set would not be a no-op
    let oracle = shapekit_oracle::corpus::run(corpus)
        .with_context(|| format!("running the oracle over {}", corpus.display()))?;
    let golden_oracles = oracle
        .written
        .iter()
        .filter(|(id, _, _)| report.written.values().flatten().any(|w| w == id))
        .count();
    println!(
        "oracle: {golden_oracles} golden oracle files written ({} in the corpus)",
        oracle.written.len()
    );
    Ok(true)
}

pub fn verify(core_src: &Path, tag: &str, tier: &str, index: &Path) -> Result<bool> {
    let commit = golden::checkout_commit_tier(core_src, tier, tag)?;
    let set = golden::scan_tier(core_src, tier, tag, &commit)?;
    let committed: serde_json::Value = serde_json::from_slice(
        &std::fs::read(index).with_context(|| format!("reading {}", index.display()))?,
    )
    .with_context(|| format!("parsing {}", index.display()))?;
    let problems = golden::verify_index(&set, &committed);
    for p in &problems {
        println!("{p}");
    }
    if problems.is_empty() {
        println!(
            "{} files and {} index files at {tag} ({commit}) match {}",
            set.files.len(),
            set.dirs.len(),
            index.display()
        );
    }
    Ok(problems.is_empty())
}

pub fn agree(manifest_file: &Path, registry_dir: &Path) -> Result<bool> {
    let registry = Registry::load(registry_dir)
        .with_context(|| format!("loading the registry in {}", registry_dir.display()))?;
    let manifest = build::load(manifest_file)
        .with_context(|| format!("loading {}", manifest_file.display()))?;
    let root = manifest_file.parent().unwrap_or(Path::new("."));
    let exceptions = golden::load_exceptions(&root.join(golden::EXCEPTIONS_FILE))?;
    let report = golden::agree(&manifest, &registry, root, &exceptions)?;
    for f in &report.families {
        println!(
            "{}: {} golden, {} local-core or history entries; operations in both: {}",
            f.family,
            f.goldens.len(),
            f.others.len(),
            if f.shared_ops.is_empty() {
                "none".to_string()
            } else {
                f.shared_ops.join(", ")
            }
        );
        if !f.shared_fragments.is_empty() {
            println!("  shared fragments: {}", f.shared_fragments.join(", "));
        }
        for c in &f.combinations {
            let verdict = match (&c.matched_by.first(), &c.excepted) {
                (Some(by), _) => format!("matched by {by}"),
                (None, Some(reason)) => format!("excepted: {reason}"),
                (None, None) => "DISAGREE: no local-core or history entry shows it".to_string(),
            };
            println!(
                "  {} {} {{{}}}: {verdict}",
                c.golden,
                c.op,
                c.kinds.join(", ")
            );
        }
        for (op, kinds, by) in &f.local_only {
            println!("  info: only {by} shows {op} {{{}}}", kinds.join(", "));
        }
    }
    if !report.golden_only_families.is_empty() {
        println!(
            "families with golden entries but no local-core or history entry: {}",
            report.golden_only_families.join(", ")
        );
    }
    for x in &report.stale_exceptions {
        println!(
            "info: exception for {} {} is now matched and can be removed",
            x.golden, x.op
        );
    }
    for x in &report.inapplicable_exceptions {
        println!(
            "info: exception for {} {} is not compared (no local-core or history entry of its family performs {}), so it excuses nothing today",
            x.golden, x.op, x.op
        );
    }
    for x in &report.unknown_exceptions {
        println!(
            "DISAGREE: exception for {} {} {{{}}} names no golden combination",
            x.golden,
            x.op,
            x.kinds.join(", ")
        );
    }
    if report.families.is_empty() {
        println!("no pairs: no family has both a golden entry and a local-core or history entry");
        return Ok(report.failures() == 0);
    }
    let checked: usize = report.families.iter().map(|f| f.combinations.len()).sum();
    let failures = report.failures();
    println!(
        "{} families compared, {checked} golden combinations checked, {failures} failures",
        report.families.len()
    );
    Ok(failures == 0)
}
