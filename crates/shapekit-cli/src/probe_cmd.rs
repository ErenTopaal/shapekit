//! `shapekit probe`: which envelope fields core validates, asked of core itself.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use shapekit_contracts::validation::ProbeAttempt;
use shapekit_core_driver::config::CoreImage;
use shapekit_core_driver::smoke::{self, MAINNET_PASSPHRASE, TESTNET_PASSPHRASE};
use shapekit_probe::{Probe, variants};

#[derive(Subcommand)]
pub enum ProbeCommand {
    /// Submit single-field changes of one valid payment to a standalone core and print core's
    /// answer to each: refused at submission, applied and failed, or applied and successful.
    Run {
        /// Only the changes of this registry/fields.toml field (repeatable); all when absent.
        #[arg(long)]
        field: Vec<String>,
        /// testnet or mainnet passphrase.
        #[arg(long, default_value = "testnet")]
        network: String,
        /// Also write the attempts as JSON.
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
    },
}

pub fn run(command: ProbeCommand) -> Result<bool> {
    match command {
        ProbeCommand::Run {
            field,
            network,
            out,
            pins,
        } => probe_run(&field, &network, out.as_deref(), &pins),
    }
}

#[derive(serde::Serialize)]
struct Row<'a> {
    field: &'a str,
    #[serde(flatten)]
    attempt: &'a ProbeAttempt,
}

fn probe_run(fields: &[String], network: &str, out: Option<&Path>, pins: &Path) -> Result<bool> {
    let passphrase = match network {
        "testnet" => TESTNET_PASSPHRASE,
        "mainnet" => MAINNET_PASSPHRASE,
        other => bail!("unknown network {other:?}, expected testnet or mainnet"),
    };
    let all = variants::payment();
    for f in fields {
        if !all.iter().any(|v| v.field == f) {
            bail!("no probe changes field {f}");
        }
    }
    let chosen: Vec<_> = all
        .iter()
        .filter(|v| fields.is_empty() || fields.iter().any(|f| f == v.field))
        .collect();
    let image = CoreImage::from_pins(pins)
        .with_context(|| format!("reading the core image pin from {}", pins.display()))?;
    let mut cfg = smoke::node_config(passphrase, image);
    cfg.soroban_high_limits = false;
    // whatever core applies must pass every invariant, or the node stops
    cfg.invariant_checks = true;
    let work = tempfile::Builder::new()
        .prefix("shapekit-probe-")
        .tempdir()
        .context("creating the node workdir")?;
    let mut probe = Probe::start(cfg, work.path(), chosen.len())?;
    let started = std::time::Instant::now();
    let mut attempts = Vec::with_capacity(chosen.len());
    for v in &chosen {
        let (a, _) = probe.attempt(v)?;
        attempts.push((v.field, a));
    }
    let seconds = started.elapsed().as_secs_f64();
    drop(probe);
    println!("{} attempts in {seconds:.2} s", attempts.len());
    for (field, a) in &attempts {
        println!(
            "{field:<32} {:<34} {:<10} {:<24} {:?}",
            a.variant,
            format!("{:?}", a.submission).to_lowercase(),
            a.result_code.as_deref().unwrap_or("-"),
            a.applied
        );
    }
    if let Some(out) = out {
        let rows: Vec<Row> = attempts
            .iter()
            .map(|(field, attempt)| Row { field, attempt })
            .collect();
        let mut text = serde_json::to_string_pretty(&rows)?;
        text.push('\n');
        std::fs::write(out, text).with_context(|| format!("writing {}", out.display()))?;
    }
    Ok(true)
}
