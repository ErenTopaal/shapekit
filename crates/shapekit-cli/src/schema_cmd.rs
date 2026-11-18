//! `shapekit schema`: the stellar-xdr definitions in schema/xdr/ as data.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use shapekit_schema::Schema;
use shapekit_schema::fragment::Fragment;
use shapekit_schema::history::History;

#[derive(clap::Subcommand)]
pub enum SchemaCommand {
    /// Count definitions, union cases, enum values and struct fields, over the whole schema or
    /// over what one root type reaches, and list the recursive groups.
    Stats {
        /// Count only the definitions this type reaches, itself included.
        #[arg(long)]
        root: Option<String>,
        /// A definitions file written by scripts/fetch-xdr-definitions.sh (default: the pinned
        /// stellar-xdr tag, built into this binary).
        #[arg(long)]
        schema: Option<PathBuf>,
        /// Print JSON instead of text.
        #[arg(long)]
        json: bool,
    },
    /// List what changed between two tags of schema/xdr/: names added and removed, and
    /// definitions whose structure changed.
    Diff {
        /// The older tag, for example v28.0.
        a: String,
        /// The newer tag, for example v30.0.
        b: String,
        /// The directory holding <tag>/xdr.json files.
        #[arg(long, default_value = "schema/xdr")]
        dir: PathBuf,
    },
    /// The first tag of schema/xdr/ each name appears in: definitions, struct fields, union arms
    /// and enum values, spelled as signatures spell them (Type.field, Type:arm, Type=value).
    FirstSeen {
        /// Only names that start with this text.
        #[arg(long)]
        prefix: Option<String>,
        /// The directory holding <tag>/xdr.json files.
        #[arg(long, default_value = "schema/xdr")]
        dir: PathBuf,
    },
    /// Date each family's signature fragments in the schema history and compare the tag they
    /// all appear from with the family's protocol_introduced in families.toml.
    Families {
        /// Directory holding the registry files, including families.toml.
        #[arg(long, default_value = "registry")]
        registry: PathBuf,
        /// The directory holding <tag>/xdr.json files.
        #[arg(long, default_value = "schema/xdr")]
        dir: PathBuf,
    },
}

pub fn run(command: SchemaCommand) -> Result<bool> {
    match command {
        SchemaCommand::Stats { root, schema, json } => {
            stats(root.as_deref(), schema.as_deref(), json)
        }
        SchemaCommand::Diff { a, b, dir } => diff(&dir, &a, &b),
        SchemaCommand::FirstSeen { prefix, dir } => first_seen(&dir, prefix.as_deref()),
        SchemaCommand::Families { registry, dir } => families(&registry, &dir),
    }
}

fn load(path: Option<&Path>) -> Result<Schema> {
    match path {
        Some(p) => Schema::load(p).with_context(|| format!("loading {}", p.display())),
        None => Ok(Schema::pinned()),
    }
}

fn stats(root: Option<&str>, path: Option<&Path>, json: bool) -> Result<bool> {
    let schema = load(path)?;
    let graph = schema.graph();
    let s = graph.stats(root)?;
    let groups = graph.recursive_groups();
    let label = match path {
        Some(p) => p.display().to_string(),
        None => format!("stellar-xdr {}", shapekit_schema::PINNED_TAG),
    };
    if json {
        let out = serde_json::json!({
            "schema": label,
            "root": root,
            "stats": s,
            "recursive_groups": groups,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(true);
    }
    match root {
        Some(r) => println!("{label}, reachable from {r}"),
        None => println!("{label}, every definition"),
    }
    let kinds: Vec<String> = s.kinds.iter().map(|(k, n)| format!("{n} {k}")).collect();
    println!("  {} definitions: {}", s.types, kinds.join(", "));
    println!("  {} union cases", s.union_cases);
    println!("  {} enum values", s.enum_values);
    println!("  {} struct fields", s.struct_fields);
    println!("  {} recursive groups (whole schema)", groups.len());
    for g in &groups {
        println!("    {}", g.join(", "));
    }
    Ok(true)
}

fn history(dir: &Path) -> Result<History> {
    History::load(dir).with_context(|| format!("loading the schemas under {}", dir.display()))
}

fn diff(dir: &Path, a: &str, b: &str) -> Result<bool> {
    let h = history(dir)?;
    let (Some(old), Some(new)) = (h.get(a), h.get(b)) else {
        bail!("{} has no {a} or no {b}", dir.display());
    };
    let d = shapekit_schema::history::diff(&old.schema, &new.schema);
    println!(
        "{a} to {b}: {} added, {} removed, {} definitions changed",
        d.added.len(),
        d.removed.len(),
        d.changed.len()
    );
    for n in &d.added {
        println!("  + {n}");
    }
    for n in &d.removed {
        println!("  - {n}");
    }
    for n in &d.changed {
        println!("  ~ {n}");
    }
    Ok(true)
}

fn first_seen(dir: &Path, prefix: Option<&str>) -> Result<bool> {
    let h = history(dir)?;
    for (name, tag) in h.first_seen() {
        if prefix.is_none_or(|p| name.starts_with(p)) {
            println!("{tag}\t{name}");
        }
    }
    Ok(true)
}

/// Dates each family's fragments and compares the tag they all exist from with the family's
/// protocol_introduced. A family introduced at or before the oldest tag in the history can only
/// be checked as "present in the oldest tag".
fn families(registry: &Path, dir: &Path) -> Result<bool> {
    let h = history(dir)?;
    let oldest = h.tags.first().context("the history is empty")?;
    let path = registry.join("families.toml");
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let table: toml::Table = text
        .parse()
        .with_context(|| format!("parsing {}", path.display()))?;
    let Some(fams) = table.get("family").and_then(|f| f.as_array()) else {
        bail!("{} has no [[family]] tables", path.display());
    };
    let (mut agree, mut before_history, mut differ) = (0, 0, 0);
    for f in fams {
        let id = f
            .get("id")
            .and_then(|v| v.as_str())
            .context("a family without an id")?;
        let introduced = f
            .get("protocol_introduced")
            .and_then(|v| v.as_integer())
            .with_context(|| format!("{id} has no protocol_introduced"))?;
        let frags = f
            .get("signature")
            .and_then(|v| v.as_array())
            .with_context(|| format!("{id} has no signature"))?;
        // the first tag each fragment is producible in, and the tag every fragment is
        let mut all_from: Option<usize> = Some(0);
        let mut latest = Vec::new();
        for frag in frags {
            let text = frag
                .as_str()
                .with_context(|| format!("{id}: a fragment is not text"))?;
            let parsed = Fragment::parse(text)?;
            let first = h.tags.iter().position(|t| parsed.producible(&t.schema));
            match (first, all_from) {
                (Some(i), Some(a)) => {
                    if i > a {
                        latest.clear();
                    }
                    if i >= a {
                        latest.push(text);
                    }
                    all_from = Some(a.max(i));
                }
                (None, _) => {
                    all_from = None;
                    latest = vec![text];
                }
                _ => {}
            }
        }
        let Some(i) = all_from else {
            differ += 1;
            println!(
                "{id}: introduced {introduced}; never producible in the history: {}",
                latest.join(", ")
            );
            continue;
        };
        let tag = &h.tags[i];
        let verdict = match tag.protocol {
            _ if i == 0 && introduced <= i64::from(oldest.protocol.unwrap_or(0)) => {
                before_history += 1;
                format!(
                    "present in the oldest tag {}, which is at or after protocol {introduced}",
                    tag.label
                )
            }
            Some(p) if i64::from(p) == introduced => {
                agree += 1;
                format!("agrees: every fragment from {}", tag.label)
            }
            _ => {
                differ += 1;
                format!(
                    "DIFFERS: every fragment only from {} (protocol {}); the last to appear: {}",
                    tag.label,
                    tag.protocol.map_or("none".to_string(), |p| p.to_string()),
                    latest.join(", ")
                )
            }
        };
        println!("{id}: introduced {introduced}; {verdict}");
    }
    println!(
        "{} families: {agree} agree, {before_history} introduced at or before the oldest tag {}, {differ} differ",
        fams.len(),
        oldest.label
    );
    Ok(true)
}
