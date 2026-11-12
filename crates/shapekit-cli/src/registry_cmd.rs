//! `shapekit registry check`.

use std::path::Path;

use anyhow::{Context, Result};
use shapekit_manifest::signature::check_fragment;
use shapekit_registry::{FAMILIES_FILE, Problem, Registry};

pub fn check(dir: &Path) -> Result<bool> {
    let registry = Registry::load(dir)
        .with_context(|| format!("loading the registry in {}", dir.display()))?;
    let mut problems = registry.check();
    // fragments must be written in the signature grammar, or they silently match nothing
    for family in registry.families.iter().flat_map(|f| &f.families) {
        for fragment in &family.signature {
            if let Err(message) = check_fragment(fragment) {
                problems.push(Problem::new(
                    format!("{FAMILIES_FILE} {}", family.id),
                    format!("signature {message}"),
                ));
            }
        }
    }
    for p in &problems {
        println!("{p}");
    }
    let families = registry.families.as_ref().map_or(0, |f| f.families.len());
    let summary = format!(
        "{} caps, {} fields, {} limits, {} lint rules, {families} families",
        registry.caps.caps.len(),
        registry.fields.fields.len(),
        registry.limits.limits.len(),
        registry.lint_rules.rules.len(),
    );
    if problems.is_empty() {
        println!("registry ok: {summary}");
        Ok(true)
    } else {
        println!("{} problems in {summary}", problems.len());
        Ok(false)
    }
}
