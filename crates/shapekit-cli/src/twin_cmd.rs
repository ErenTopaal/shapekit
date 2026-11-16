//! `shapekit twin`: the coverage twin of stellar-core and the protocol gate list.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use shapekit_contracts::CoreRef;
use shapekit_contracts::gates::Comparison;
use shapekit_twin::gates;

use crate::pins;

#[derive(clap::Subcommand)]
pub enum TwinCommand {
    /// List every protocol version comparison in core's non-test C++ source
    /// (`protocolVersionIsBefore`, `StartsFrom`, `Equals`) with its function and protocol,
    /// both sides open. The checkout must be at the stellar-core commit pinned in the pins
    /// file. Comparisons against a runtime value instead of a protocol constant are printed,
    /// not listed.
    Gates {
        /// A stellar-core checkout.
        #[arg(long)]
        core_src: PathBuf,
        /// Where to write the gate list (registry/generated/gates.json).
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
    },
}

pub fn run(command: TwinCommand) -> Result<bool> {
    match command {
        TwinCommand::Gates {
            core_src,
            out,
            pins,
        } => gates(&core_src, &out, &pins),
    }
}

/// The pinned core release: tag and commit of the `stellar-core` git pin.
fn pinned_core(pins_path: &Path) -> Result<CoreRef> {
    let p = pins::load(pins_path)?;
    let git = p
        .git
        .iter()
        .find(|g| g.name == "stellar-core")
        .with_context(|| format!("{} has no [[git]] stellar-core", pins_path.display()))?;
    let version = git
        .git_ref
        .as_deref()
        .and_then(|r| r.strip_prefix("refs/tags/"))
        .with_context(|| "the stellar-core pin names no tag")?
        .to_owned();
    let commit = git
        .sha
        .clone()
        .with_context(|| "the stellar-core pin has no commit")?;
    Ok(CoreRef {
        version,
        commit,
        image: None,
    })
}

fn checkout_commit(dir: &Path) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .with_context(|| format!("running git in {}", dir.display()))?;
    if !out.status.success() {
        bail!(
            "{} is not a git checkout: {}",
            dir.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn gates(core_src: &Path, out: &Path, pins_path: &Path) -> Result<bool> {
    let core = pinned_core(pins_path)?;
    let head = checkout_commit(core_src)?;
    if head != core.commit {
        bail!(
            "{} is at {head}, {} pins stellar-core {} ({})",
            core_src.display(),
            pins_path.display(),
            core.version,
            core.commit
        );
    }
    let scan = gates::scan_tree(core_src)?;
    let (list, runtime) = gates::gate_list(&scan, core);
    shapekit_contracts::write(out, &list)?;

    let in_tx = list
        .gates
        .iter()
        .filter(|g| g.file.starts_with("src/transactions/"))
        .count();
    let count = |c: Comparison| list.gates.iter().filter(|g| g.comparison == c).count();
    let lo = list.gates.iter().map(|g| g.version).min();
    let hi = list.gates.iter().map(|g| g.version).max();
    println!(
        "{} files scanned, {} matches of the gate functions",
        scan.files_scanned,
        scan.gates.len() + scan.declarations.len()
    );
    for (file, line) in &scan.declarations {
        println!("  declaration or definition, not a gate: {file}:{line}");
    }
    println!(
        "{} calls, {} against a protocol constant written to {}, {} against a runtime value:",
        scan.gates.len(),
        list.gates.len(),
        out.display(),
        runtime.len()
    );
    for g in &runtime {
        println!(
            "  {}:{}:{} {} compares {} with {}",
            g.file, g.line, g.column, g.function, g.version_arg, g.protocol_arg
        );
    }
    println!(
        "listed gates: {in_tx} in src/transactions, {} is-before, {} starts-from, {} equals, protocols V_{} to V_{}, {} only built with BUILD_TESTS",
        count(Comparison::IsBefore),
        count(Comparison::StartsFrom),
        count(Comparison::Equals),
        lo.map_or("-".into(), |v| v.to_string()),
        hi.map_or("-".into(), |v| v.to_string()),
        scan.gates
            .iter()
            .filter(|g| g.build_tests_only && g.protocol.is_some())
            .count()
    );
    Ok(true)
}
