//! `shapekit registry check`.

use std::path::Path;

use anyhow::{Context, Result};
use shapekit_coverage::ROOT_TYPE;
use shapekit_manifest::signature::FragmentChecker;
use shapekit_registry::{FAMILIES_FILE, Problem, Registry};

pub fn check(dir: &Path) -> Result<bool> {
    let registry = Registry::load(dir)
        .with_context(|| format!("loading the registry in {}", dir.display()))?;
    let mut problems = registry.check();
    // a fragment that misnames an arm, field, type or value matches nothing, silently; the
    // registry crate cannot see the xdr schema, so this check lives here
    let checker = FragmentChecker::new(ROOT_TYPE)
        .with_context(|| format!("loading the {ROOT_TYPE} schema"))?;
    for family in registry.families.iter().flat_map(|f| &f.families) {
        for fragment in &family.signature {
            if let Err(message) = checker.check(fragment) {
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
    // the pre-cap record is not a cap, so it is counted apart
    let pre_cap = registry
        .caps
        .caps
        .iter()
        .any(|c| c.id == shapekit_registry::PRE_CAP);
    let summary = format!(
        "{} caps{}, {} fields, {} limits, {} lint rules, {families} families",
        registry.caps.caps.len() - usize::from(pre_cap),
        if pre_cap { " and the pre-cap record" } else { "" },
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
