//! `shapekit synth build`.

use std::path::Path;

use anyhow::{Context, Result};
use shapekit_registry::Registry;

pub fn build(corpus: &Path, registry_dir: &Path) -> Result<bool> {
    let registry = Registry::load(registry_dir)
        .with_context(|| format!("loading the registry in {}", registry_dir.display()))?;
    let (synth, report) = shapekit_gen::synth::build(corpus, &registry)?;
    for (kind, n) in synth.counts() {
        println!("{kind}: {n}");
    }
    for id in &report.removed {
        println!("removed stale {id}");
    }
    println!(
        "wrote {} synthetic entries under {}; run `shapekit manifest build` next",
        report.written,
        corpus.display()
    );
    Ok(true)
}
