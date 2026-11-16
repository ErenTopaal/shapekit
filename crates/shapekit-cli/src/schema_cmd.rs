//! `shapekit schema`: the stellar-xdr definitions in schema/xdr/ as data.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use shapekit_schema::Schema;

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
}

pub fn run(command: SchemaCommand) -> Result<bool> {
    match command {
        SchemaCommand::Stats { root, schema, json } => {
            stats(root.as_deref(), schema.as_deref(), json)
        }
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
