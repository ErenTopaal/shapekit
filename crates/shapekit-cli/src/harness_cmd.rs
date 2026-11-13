//! `shapekit harness`: replay chains into an indexer through an adapter and judge what it
//! stored against the meta oracle. See docs/adapter-interface.md.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use shapekit_harness::corpus::{Filter, select};
use shapekit_harness::expect;
use shapekit_harness::protocol::{DeliveryMode, sync_generated};
use shapekit_harness::report::{self, MemoryOptions};
use shapekit_harness::reproduce;
use shapekit_harness::run::{RunOptions, Timeouts};

#[derive(Subcommand)]
pub enum HarnessCommand {
    /// Replay the selected chains into the target behind an adapter and write
    /// harness-report.json and HARNESS.md.
    Run(RunArgs),
    /// Run a reproductions table: pairs of target builds over named chains, with the verdicts
    /// named entries must get.
    Reproduce(ReproduceArgs),
    /// Write an expectations file from a reviewed harness-report.json (--write), or compare a
    /// report with one (--check). `harness run --expect` does the comparison inline.
    Expect(ExpectArgs),
    /// Check, or with --write regenerate, schema/adapter-protocol.schema.json and the
    /// vocabulary section of docs/adapter-interface.md.
    Schema {
        #[arg(long)]
        write: bool,
        /// The repository root.
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
}

#[derive(Args, Clone)]
pub struct Common {
    #[arg(long, default_value = "corpus")]
    corpus: PathBuf,
    /// Delivery mode to ask the adapter for; its default when absent.
    #[arg(long, value_parser = ["sep54-filesystem", "sep54-s3", "rpc"])]
    delivery: Option<String>,
    /// Memory limit the adapter applies to the target, in bytes.
    #[arg(long, default_value_t = 4 * 1024 * 1024 * 1024)]
    memory_limit: u64,
    /// Seconds a target may go without ingesting a released ledger.
    #[arg(long, default_value_t = 120)]
    ledger_timeout: u64,
    /// Seconds from start until the target has ingested the bootstrap ledger.
    #[arg(long, default_value_t = 600)]
    bootstrap_timeout: u64,
    /// Seconds the adapter may take to answer start (it may build images).
    #[arg(long, default_value_t = 1800)]
    start_timeout: u64,
    /// Seconds the adapter may take to answer one status request. A late answer ends the
    /// chain, so this is sized for a heavily loaded host, not for a quiet one.
    #[arg(long, default_value_t = 300)]
    status_timeout: u64,
    /// Bind the replay server to 127.0.0.1 and create no docker network, for targets that
    /// run as host processes.
    #[arg(long)]
    no_docker: bool,
}

#[derive(Args)]
pub struct RunArgs {
    /// The adapter command line, split on whitespace; quote an argument with ' or ".
    #[arg(long)]
    adapter: String,
    /// Only entries of this family; repeatable.
    #[arg(long)]
    family: Vec<String>,
    /// Only this chain id; repeatable.
    #[arg(long)]
    chain: Vec<String>,
    #[arg(long, default_value = "testnet", value_parser = ["testnet", "mainnet", "both"])]
    network: String,
    #[arg(long, default_value = "harness-out")]
    out: PathBuf,
    /// Run the whole selection this many times in one process and fail when a later round's
    /// peak memory grows past the first round's.
    #[arg(long, default_value_t = 1)]
    repeat: u32,
    /// Allowed growth of a later round's peak resident memory, as a fraction of the first.
    #[arg(long, default_value_t = 0.1)]
    memory_slack: f64,
    /// Fail when any round's peak resident memory exceeds this many bytes.
    #[arg(long)]
    memory_budget: Option<u64>,
    /// Expected verdicts (see `harness expect`): fail when any entry's verdict or reason
    /// differs, an entry is missing or unlisted, or the target build is another one.
    #[arg(long)]
    expect: Option<PathBuf>,
    #[command(flatten)]
    common: Common,
}

#[derive(Args)]
pub struct ExpectArgs {
    /// The harness-report.json of a run.
    #[arg(long)]
    report: PathBuf,
    /// Write the report's verdicts as an expectations file.
    #[arg(long, conflicts_with = "check", required_unless_present = "check")]
    write: Option<PathBuf>,
    /// Compare the report with this expectations file.
    #[arg(long)]
    check: Option<PathBuf>,
}

fn read_report(path: &std::path::Path) -> Result<report::Report> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))
}

/// Prints every difference and whether the run met its expectations.
fn report_expectations(
    rep: &report::Report,
    want: &expect::Expectations,
    path: &std::path::Path,
) -> Result<bool> {
    let diffs = expect::compare(rep, want);
    for d in &diffs {
        println!("expectation: {d}");
    }
    println!(
        "expectations {}: {}",
        path.display(),
        if diffs.is_empty() {
            format!("met, {} entries", want.entries.len())
        } else {
            format!("FAILED, {} difference(s)", diffs.len())
        }
    );
    Ok(diffs.is_empty())
}

#[derive(Args)]
pub struct ReproduceArgs {
    #[arg(long)]
    table: PathBuf,
    /// Only the reproduction with this id; repeatable.
    #[arg(long)]
    only: Vec<String>,
    #[arg(long, default_value = "harness-reproduce")]
    out: PathBuf,
    #[command(flatten)]
    common: Common,
}

/// Splits a command line on whitespace, honouring ' and " quotes. No escapes, no expansion.
pub fn split_command(s: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote = None;
    let mut started = false;
    for ch in s.chars() {
        match (quote, ch) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => {
                quote = Some(ch);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            (None, c) => {
                cur.push(c);
                started = true;
            }
        }
    }
    if quote.is_some() {
        bail!("unbalanced quote in {s:?}");
    }
    if started {
        out.push(cur);
    }
    if out.is_empty() {
        bail!("the adapter command is empty");
    }
    Ok(out)
}

fn options(c: &Common, adapter: Vec<String>, logs: PathBuf) -> Result<RunOptions> {
    let delivery = c
        .delivery
        .as_deref()
        .map(|d| DeliveryMode::parse(d).with_context(|| format!("unknown delivery {d}")))
        .transpose()?;
    Ok(RunOptions {
        adapter,
        delivery,
        memory_limit_bytes: c.memory_limit,
        timeouts: Timeouts {
            ledger: Duration::from_secs(c.ledger_timeout),
            bootstrap: Duration::from_secs(c.bootstrap_timeout),
            start: Duration::from_secs(c.start_timeout),
            status: Duration::from_secs(c.status_timeout),
            ..Timeouts::default()
        },
        docker: !c.no_docker,
        logs,
    })
}

pub fn run(cmd: HarnessCommand) -> Result<bool> {
    match cmd {
        HarnessCommand::Expect(a) => {
            let rep = read_report(&a.report)?;
            if let Some(path) = a.check {
                return report_expectations(&rep, &expect::load(&path)?, &path);
            }
            let path = a.write.expect("clap requires --write or --check");
            let e = expect::from_report(&rep)?;
            let header = format!(
                "expected verdicts of `shapekit harness run --expect` on this selection at {} {}.\n\
                 regenerate after reviewing a run's HARNESS.md:\n\
                 shapekit harness expect --report <out>/harness-report.json --write <this file>",
                e.target, e.target_version
            );
            std::fs::write(&path, expect::render(&e, &header)?)
                .with_context(|| format!("writing {}", path.display()))?;
            println!("wrote {} ({} entries)", path.display(), e.entries.len());
            Ok(true)
        }
        HarnessCommand::Schema { write, root } => {
            let stale = sync_generated(&root, write)?;
            for s in &stale {
                println!("{} {s}", if write { "wrote" } else { "stale" });
            }
            Ok(write || stale.is_empty())
        }
        HarnessCommand::Run(a) => {
            // a broken expectations file should fail before a long run, not after it
            let want = a
                .expect
                .as_deref()
                .map(|p| expect::load(p).map(|w| (w, p.to_path_buf())))
                .transpose()?;
            let networks: Vec<String> = match a.network.as_str() {
                "both" => vec!["testnet".into(), "mainnet".into()],
                n => vec![n.into()],
            };
            let sel = select(
                &a.common.corpus,
                &Filter {
                    families: &a.family,
                    chains: &a.chain,
                    networks: &networks,
                },
            )?;
            if sel.jobs.is_empty() {
                bail!("the selection has no chain with an evaluable entry");
            }
            let opts = options(&a.common, split_command(&a.adapter)?, a.out.join("logs"))?;
            let rep = report::run(
                &sel,
                &opts,
                &a.out,
                &MemoryOptions {
                    rounds: a.repeat,
                    slack_fraction: a.memory_slack,
                    slack_min_bytes: 16 * 1024 * 1024,
                    budget_bytes: a.memory_budget,
                },
            )?;
            let (json, md) = report::write(&rep, &a.out)?;
            for (k, v) in &rep.totals.by_verdict {
                println!("{k}: {v}");
            }
            println!("{}", rep.memory.detail);
            if let Some(t) = &rep.target_memory {
                println!("target: {}", t.detail);
            }
            for c in rep.chains.iter().filter(|c| c.error.is_some()) {
                println!(
                    "chain {} ({}) did not run: {}",
                    c.chain,
                    c.network,
                    c.error.as_deref().unwrap_or("")
                );
            }
            println!("wrote {} and {}", json.display(), md.display());
            let met = match &want {
                Some((w, path)) => report_expectations(&rep, w, path)?,
                None => true,
            };
            Ok(rep.ok() && met)
        }
        HarnessCommand::Reproduce(a) => {
            let table = reproduce::load(&a.table)?;
            let base = options(&a.common, Vec::new(), a.out.join("logs"))?;
            let results = reproduce::run(&table, &a.only, &a.common.corpus, &base, &a.out)?;
            reproduce::write(&results, &a.out)?;
            for c in &results {
                println!(
                    "{} {} ({}): {}",
                    c.reproduction,
                    c.label,
                    c.commit,
                    if c.holds { "holds" } else { "does not hold" }
                );
            }
            println!("wrote {}", a.out.join("REPRODUCTIONS.md").display());
            Ok(!results.is_empty() && results.iter().all(|c| c.holds))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::split_command;

    #[test]
    fn splits_with_quotes() {
        assert_eq!(
            split_command("go run ./x --name 'a b' \"\"").unwrap(),
            ["go", "run", "./x", "--name", "a b", ""]
        );
        assert!(split_command("a 'b").is_err());
        assert!(split_command("   ").is_err());
    }
}
