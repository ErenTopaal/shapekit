//! `probe all`: every envelope field, each through its struct's carrier.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use shapekit_core_driver::config::NodeConfig;
use shapekit_schema::Schema;

use crate::carriers::{self, Carrier};
use crate::edit::Typer;
use shapekit_contracts::validation::Verdict;

use crate::engine::{FieldRun, probe_field};
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

/// The directory a world's chain is recorded in: `<chains>/<family>.<chain>/<network>`.
pub fn chain_dir(chains: &Path, kind: WorldKind) -> std::path::PathBuf {
    chains
        .join(format!("{}.{}", crate::witness::FAMILY, kind.chain()))
        .join("testnet")
}

fn finish(w: World, kind: WorldKind, chains: Option<&Path>) -> Result<()> {
    match chains {
        Some(dir) => w.finish(&chain_dir(dir, kind)),
        // dropping the world stops its node
        None => Ok(()),
    }
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
    chains: Option<&Path>,
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
    // every job of one world runs before the next world starts: a standalone node left idle
    // lets its virtual clock run ahead of the close times the driver gives it
    // (ApplicationImpl::manualClose refuses a close time before its clock's now)
    let mut jobs: Vec<(usize, usize, &Carrier)> = Vec::new();
    for (fi, f) in fields.iter().enumerate() {
        for (ci, c) in carriers[f.structure.as_str()].iter().enumerate() {
            jobs.push((fi, ci, c));
        }
    }
    jobs.sort_by_key(|(fi, ci, c)| (c.world, *fi, *ci));
    let mut runs: BTreeMap<(usize, usize), FieldRun> = BTreeMap::new();
    let mut current: Option<(WorldKind, World)> = None;
    for (fi, ci, c) in jobs {
        if current.as_ref().is_none_or(|(k, _)| *k != c.world) {
            // the previous world is done: its chain is recorded, or the node just stopped
            if let Some((k, w)) = current.take() {
                finish(w, k, chains)?;
            }
            let dir = workdir.join(format!("{:?}", c.world).to_ascii_lowercase());
            std::fs::create_dir_all(&dir)?;
            let w = World::start(c.world, config(c.world.protocol()), &dir)
                .with_context(|| format!("starting the {:?} node", c.world))?;
            current = Some((c.world, w));
        }
        let (_, w) = current.as_mut().expect("started above");
        let f = &fields[fi];
        let r = probe_field(w, &typer, c, f)
            .with_context(|| format!("probing {} with carrier {:?}", f.path, c.id))?;
        progress(&r);
        runs.insert((fi, ci), r);
    }
    if let Some((k, w)) = current.take() {
        finish(w, k, chains)?;
    }
    let mut out = Vec::with_capacity(fields.len());
    let mut it = runs.into_iter().peekable();
    while let Some(((fi, _), first)) = it.next() {
        // each carrier has its own base, so the verdict is decided per carrier: the field is
        // validated when some carrier's values end in more than one way
        let mut m = first;
        while let Some(((next, _), _)) = it.peek() {
            if *next != fi {
                break;
            }
            let (_, r) = it.next().expect("peeked");
            if r.verdict.verdict == Verdict::Validated {
                m.verdict.verdict = Verdict::Validated;
            }
            m.verdict.attempts.extend(r.verdict.attempts);
            m.evidence.extend(r.evidence);
        }
        out.push(m);
    }
    Ok(out)
}
