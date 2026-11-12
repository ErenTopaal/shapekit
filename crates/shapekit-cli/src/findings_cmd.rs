//! `shapekit findings collect|check`. The format and rules are documented on the shapekit-findings crate.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use shapekit_findings::collect::{Collection, collect};
use shapekit_findings::{check, render, triage};

#[derive(Subcommand)]
pub enum FindingsCommand {
    /// Map every failure in the triage's inputs to a finding and write findings.json and
    /// FINDINGS.md.
    Collect(Common),
    /// Fail while any failure has no finding, or any finding has no upstream link or recorded
    /// decision, or findings.json is not what collect would write now.
    Check(Common),
}

#[derive(Args)]
pub struct Common {
    #[arg(long, default_value = "findings/triage.toml")]
    triage: PathBuf,
    /// The repository root; input paths in the triage are relative to it.
    #[arg(long, default_value = ".")]
    root: PathBuf,
    /// Where findings.json and FINDINGS.md live.
    #[arg(long, default_value = "findings")]
    out: PathBuf,
}

fn fresh(c: &Common) -> Result<Collection> {
    let t = triage::load(&c.triage)?;
    collect(&c.root, &t)
}

pub fn run(cmd: FindingsCommand) -> Result<bool> {
    match cmd {
        FindingsCommand::Collect(c) => {
            let col = fresh(&c)?;
            std::fs::create_dir_all(&c.out)
                .with_context(|| format!("creating {}", c.out.display()))?;
            let json = c.out.join("findings.json");
            let mut text = serde_json::to_string_pretty(&col)?;
            text.push('\n');
            std::fs::write(&json, text).with_context(|| format!("writing {}", json.display()))?;
            let md = c.out.join("FINDINGS.md");
            std::fs::write(&md, render::markdown(&col))
                .with_context(|| format!("writing {}", md.display()))?;
            for (comp, by) in &col.summary.by_component {
                let parts: Vec<String> = by.iter().map(|(k, v)| format!("{k} {v}")).collect();
                println!("{comp}: {}", parts.join(", "));
            }
            println!(
                "{} findings, {} failures, {} unmapped",
                col.summary.findings, col.summary.failures, col.summary.unmapped
            );
            println!("wrote {} and {}", json.display(), md.display());
            Ok(true)
        }
        FindingsCommand::Check(c) => {
            let col = fresh(&c)?;
            let path = c.out.join("findings.json");
            let written: Option<Collection> = match std::fs::read(&path) {
                Ok(b) => Some(
                    serde_json::from_slice(&b)
                        .with_context(|| format!("parsing {}", path.display()))?,
                ),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
            };
            let out = check::check(&c.root, &col, written.as_ref());
            for p in &out.problems {
                println!("{p}");
            }
            println!(
                "{} findings, {} failures, {} unmapped; {} of {} findings pending filing (status draft){}",
                col.summary.findings,
                col.summary.failures,
                col.summary.unmapped,
                out.pending.len(),
                col.summary.findings,
                if out.pending.is_empty() {
                    String::new()
                } else {
                    format!(": {}", out.pending.join(", "))
                }
            );
            println!(
                "findings check: {}",
                if out.ok {
                    "ok".to_string()
                } else {
                    format!("FAILED, {} problem(s)", out.problems.len())
                }
            );
            Ok(out.ok)
        }
    }
}
