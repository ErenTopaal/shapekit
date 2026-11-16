//! `probe all`: every envelope field, each through its struct's carrier.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use shapekit_core_driver::config::NodeConfig;
use shapekit_schema::Schema;

use crate::carriers::{self, Carrier};
use crate::edit::Typer;
use crate::engine::{FieldRun, probe_field, verdict};
use crate::fields::{Field, envelope_fields};
use crate::world::{World, WorldKind};

/// Every carrier, by struct name. A struct with several carriers names each with an id.
pub fn all_carriers() -> Result<BTreeMap<&'static str, Vec<Carrier>>> {
    let mut out: BTreeMap<&'static str, Vec<Carrier>> = BTreeMap::new();
    for c in carriers::classic()
        .into_iter()
        .chain(crate::soroban::carriers())
    {
        out.entry(c.structure).or_default().push(c);
    }
    for (s, cs) in &out {
        let mut ids: Vec<&str> = cs.iter().map(|c| c.id).collect();
        ids.sort();
        ids.dedup();
        if cs.len() > 1 && (ids.len() != cs.len() || ids.contains(&"")) {
            bail!("the carriers of {s} need distinct ids");
        }
    }
    Ok(out)
}

/// The fields `only` selects (struct names or `Struct.field` paths), all when empty.
pub fn selected(schema: &Schema, only: &[String]) -> Result<Vec<Field>> {
    let fields = envelope_fields(schema)?;
    for o in only {
        if !fields.iter().any(|f| &f.path == o || &f.structure == o) {
            bail!("no envelope field or struct {o}");
        }
    }
    Ok(fields
        .into_iter()
        .filter(|f| only.is_empty() || only.iter().any(|o| o == &f.path || o == &f.structure))
        .collect())
}

/// Runs `fields`, starting one node per protocol the carriers need. `config` gives the node
/// config for a protocol; `progress` sees every finished field.
pub fn run(
    fields: &[Field],
    config: impl Fn(u32) -> NodeConfig,
    workdir: &Path,
    mut progress: impl FnMut(&FieldRun),
) -> Result<Vec<FieldRun>> {
    let schema = Schema::pinned();
    let typer = Typer::new(&schema);
    let carriers = all_carriers()?;
    let missing: Vec<&str> = fields
        .iter()
        .filter(|f| !carriers.contains_key(f.structure.as_str()))
        .map(|f| f.path.as_str())
        .collect();
    if !missing.is_empty() {
        bail!("no carrier for {} fields: {missing:?}", missing.len());
    }
    let mut worlds: BTreeMap<WorldKind, World> = BTreeMap::new();
    let mut out = Vec::with_capacity(fields.len());
    for f in fields {
        let mut merged: Option<FieldRun> = None;
        for c in &carriers[f.structure.as_str()] {
            if !worlds.contains_key(&c.world) {
                let dir = workdir.join(format!("{:?}", c.world).to_ascii_lowercase());
                std::fs::create_dir_all(&dir)?;
                let w = World::start(c.world, config(c.world.protocol()), &dir)
                    .with_context(|| format!("starting the {:?} node", c.world))?;
                worlds.insert(c.world, w);
            }
            let w = worlds.get_mut(&c.world).expect("inserted above");
            let r = probe_field(w, &typer, c, f)
                .with_context(|| format!("probing {} with carrier {:?}", f.path, c.id))?;
            merged = Some(match merged {
                None => r,
                Some(mut m) => {
                    m.verdict.attempts.extend(r.verdict.attempts);
                    m.evidence.extend(r.evidence);
                    m.verdict.verdict = verdict(&m.verdict.attempts);
                    m
                }
            });
        }
        let r = merged.expect("every field has a carrier");
        progress(&r);
        out.push(r);
    }
    Ok(out)
}
