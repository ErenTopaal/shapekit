//! `shapekit import goldens` and `shapekit goldens agree`.

use std::path::Path;

use anyhow::{Context, Result};
use shapekit_gen::golden;
use shapekit_manifest::build;
use shapekit_registry::Registry;

pub fn import(core_src: &Path, tag: &str, corpus: &Path, registry_dir: &Path) -> Result<bool> {
    let registry = Registry::load(registry_dir)
        .with_context(|| format!("loading the registry in {}", registry_dir.display()))?;
    let commit = golden::checkout_commit(core_src, tag)?;
    let mut set = golden::scan(core_src, tag, &commit)?;
    println!(
        "indexed {} files ({} ledgers) in {} directories of {} at {tag} ({commit})",
        set.files.len(),
        set.lcms.len(),
        set.dirs.len(),
        golden::TIER
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
            "not imported: {} fragment {} (smallest carrier {} record {}, {} bytes)",
            n.family, n.fragment, n.path, n.record, n.bytes
        );
    }
    let families = registry.families.as_ref().map_or(0, |f| f.families.len());
    println!(
        "imported {total} entries for {} of {families} families; wrote {}",
        report.written.len(),
        report.index_path.display()
    );
    Ok(true)
}

pub fn verify(core_src: &Path, tag: &str, index: &Path) -> Result<bool> {
    let commit = golden::checkout_commit(core_src, tag)?;
    let set = golden::scan(core_src, tag, &commit)?;
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
    let report = golden::agree(&manifest, &registry, root)?;
    for p in &report.pairs {
        let verdict = if p.shared_fragments.is_empty() {
            "not comparable (no common fragment)"
        } else if p.op_mismatches.is_empty() {
            "agree"
        } else {
            "DISAGREE"
        };
        println!(
            "{}: {} vs {} [{}]: {verdict}",
            p.family,
            p.golden,
            p.other,
            p.other_class.as_str()
        );
        if !p.shared_fragments.is_empty() {
            println!("  shared fragments: {}", p.shared_fragments.join(", "));
            println!("  operations compared: {}", list(&p.compared_ops));
        }
        for m in &p.op_mismatches {
            println!(
                "  {}: golden {{{}}} vs {{{}}}",
                m.op,
                m.golden.iter().cloned().collect::<Vec<_>>().join(", "),
                m.other.iter().cloned().collect::<Vec<_>>().join(", ")
            );
        }
    }
    for f in &report.no_shared_fragment {
        println!("{f}: no comparable pair, the golden and other entries share no family fragment");
    }
    if !report.golden_only_families.is_empty() {
        println!(
            "families with golden entries but no local-core or history entry: {}",
            report.golden_only_families.join(", ")
        );
    }
    if report.pairs.is_empty() {
        println!("no pairs: no family has both a golden entry and a local-core or history entry");
        return Ok(true);
    }
    let failures = report.failures();
    println!(
        "{} pairs, {} comparable, {failures} failures",
        report.pairs.len(),
        report
            .pairs
            .iter()
            .filter(|p| !p.shared_fragments.is_empty())
            .count()
    );
    Ok(failures == 0)
}

fn list(items: &[String]) -> String {
    if items.is_empty() {
        "none (no shared fragment names an operation both carry)".to_string()
    } else {
        items.join(", ")
    }
}
