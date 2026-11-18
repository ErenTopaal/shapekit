//! Replays the contract invocations of one recorded chain directory (the
//! `chain.json` and `ledgers.xdr` a recording writes) through the linked
//! `soroban-env-host` and compares each result with the chain's meta.
//!
//! usage: shapekit-host-cov <chain dir>
//!
//! Each invocation gets a verdict: verified (the host did what core
//! recorded: same return value, ledger writes, contract events and success
//! hash, or the same failure code), differs, unverified (the host ran but
//! core's result code cannot confirm it ran there too), not run by core, or
//! skipped (inputs the replay cannot rebuild exactly). Exits 0 only if at
//! least one invocation ran on the host and was compared, and none differs. Built with `-C instrument-coverage`
//! by `twin/host-cov.sh`, the run also leaves the host's coverage profile.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use shapekit_core_driver::meta::read_all;
use shapekit_twin::host_cov::{CoreResult, HOST_VERSION, Verdict, passes, replay};
use stellar_xdr::{Limits, WriteXdr};

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool> {
    let mut args = std::env::args_os().skip(1);
    let (Some(dir), None) = (args.next(), args.next()) else {
        bail!("usage: shapekit-host-cov <chain dir>");
    };
    let dir = PathBuf::from(dir);
    let summary: serde_json::Value = serde_json::from_slice(
        &std::fs::read(dir.join("chain.json")).context("reading chain.json")?,
    )
    .context("parsing chain.json")?;
    let nid_hex = summary["network_id"]
        .as_str()
        .context("chain.json has no network_id")?;
    let network_id: [u8; 32] = hex::decode(nid_hex)
        .context("network_id is not hex")?
        .try_into()
        .map_err(|_| anyhow::anyhow!("network_id is not 32 bytes"))?;
    let stream = summary["stream_file"]
        .as_str()
        .context("chain.json has no stream_file")?;
    let frames = read_all(&dir.join(stream)).context("reading the meta stream")?;
    let lcms: Vec<_> = frames.into_iter().map(|f| f.meta).collect();

    let checks = replay(&lcms, network_id)?;
    if checks.is_empty() {
        println!("no contract invocation in {}", dir.display());
        return Ok(false);
    }
    let mut counts: std::collections::BTreeMap<&str, usize> = Default::default();
    for c in &checks {
        let (word, why) = match &c.verdict {
            Verdict::Verified => ("verified", String::new()),
            Verdict::Differs(w) => ("DIFFERS", w.clone()),
            Verdict::Unverified(w) => ("unverified", w.clone()),
            Verdict::NotRunByCore => ("not run by core", String::new()),
            Verdict::Skipped(w) => ("skipped", w.clone()),
        };
        *counts.entry(word).or_default() += 1;
        let host = match &c.host {
            Some(h) => format!(
                "host {HOST_VERSION} {}, cpu {} mem {}{}",
                if h.success { "success" } else { "failure" },
                h.cpu_insns,
                h.mem_bytes,
                h.error
                    .as_deref()
                    .map(|e| format!(", {e}"))
                    .unwrap_or_default()
            ),
            None => "host not run".into(),
        };
        let ret = match c.host.as_ref().and_then(|h| h.return_value.as_ref()) {
            Some(v) => format!(", return {}", hex::encode(v.to_xdr(Limits::none())?)),
            None => String::new(),
        };
        let core = match &c.core {
            CoreResult::Success(h) => format!("success {}", hex::encode(h)),
            other => format!("{other:?}"),
        };
        println!(
            "ledger {} tx {} {}: core {core}, {host}{ret}: {word}{}",
            c.ledger,
            c.tx_index,
            hex::encode(c.tx_hash),
            if why.is_empty() {
                String::new()
            } else {
                format!(" ({why})")
            }
        );
        if matches!(c.verdict, Verdict::Differs(_))
            && let Some(h) = &c.host
        {
            println!(
                "  host return {:?}\n  meta return {:?}",
                h.return_value, c.meta_return
            );
            println!(
                "  host writes {:?}\n  meta writes {:?}",
                h.writes, c.meta_writes
            );
            println!(
                "  host events {:?}\n  meta events {:?}",
                h.events, c.meta_events
            );
        }
    }
    let summary: Vec<String> = counts.iter().map(|(k, v)| format!("{v} {k}")).collect();
    println!("{} invocations: {}", checks.len(), summary.join(", "));
    Ok(passes(&checks))
}
