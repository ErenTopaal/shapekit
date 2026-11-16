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
    /// Run the carry check on a dump a harness run kept: every cell is looked up among the
    /// identifiers and amounts the chain carried, and truncated or fabricated values are
    /// listed.
    Carry(CarryArgs),
    /// Compare the dumps two runs kept, chain by chain: delivery modes, a restart, the two
    /// network variants. Columns that differ between two runs of one configuration
    /// (--control) are left out; the network variants are translated through the
    /// identifiers core wrote differently in the two chains.
    Relate(RelateArgs),
    /// Compare the target's stored accounts, trustlines, asset contract balances and pools
    /// with core's final entries: infer the table mapping from runs, or check runs with it.
    #[command(subcommand)]
    State(StateCommand),
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
    /// Do not ask the adapter for a dump of the target's tables after the export.
    #[arg(long)]
    no_dump: bool,
    /// Restart the target's ingesting process once, halfway through each chain (needs an
    /// adapter that offers restart).
    #[arg(long)]
    restart: bool,
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

#[derive(Subcommand)]
pub enum StateCommand {
    /// Infer which table and column holds which entry field from the ordinary chains of the
    /// given runs (no entry mishandled or dropped, carry check clean), keep what holds in
    /// all of them, and write the mapping.
    Infer {
        /// A `harness run --out` directory whose run kept dumps; repeatable.
        #[arg(long, required = true)]
        run: Vec<PathBuf>,
        /// Only chains of this network.
        #[arg(long)]
        network: Option<String>,
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
        #[arg(long)]
        write: PathBuf,
    },
    /// Apply a mapping to every chain of the given runs and list where the stored state
    /// differs from core's, and where that disagrees with the oracle's verdicts.
    Check {
        #[arg(long, required = true)]
        run: Vec<PathBuf>,
        #[arg(long)]
        mapping: PathBuf,
        /// Only chains of this network.
        #[arg(long)]
        network: Option<String>,
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
        /// Write the per-chain results as JSON here.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Args)]
pub struct RelateArgs {
    /// The logs directory of the base run (where its `<chain>.<network>.dump.json` are).
    #[arg(long)]
    base: PathBuf,
    /// The logs directory of the run compared with it.
    #[arg(long)]
    other: PathBuf,
    /// The logs directory of a second run of the base configuration, to measure noise.
    #[arg(long)]
    control: Option<PathBuf>,
    #[arg(long, default_value = "corpus")]
    corpus: PathBuf,
    /// Write the reports as JSON here.
    #[arg(long)]
    out: Option<PathBuf>,
}

/// `<chain>.<network>.dump.json` files of a logs directory, by (chain, network).
fn dumps_in(
    dir: &std::path::Path,
) -> Result<std::collections::BTreeMap<(String, String), PathBuf>> {
    let mut out = std::collections::BTreeMap::new();
    for e in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let p = e?.path();
        let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(stem) = name.strip_suffix(".dump.json") else {
            continue;
        };
        let Some((chain, network)) = stem.rsplit_once('.') else {
            continue;
        };
        out.insert((chain.to_owned(), network.to_owned()), p.clone());
    }
    Ok(out)
}

#[derive(Args)]
pub struct CarryArgs {
    /// A `<chain>.<network>.dump.json` from a run's logs directory.
    #[arg(long)]
    dump: PathBuf,
    /// The chain directory, for example corpus/chains/<chain>/testnet.
    #[arg(long)]
    chain_dir: PathBuf,
    /// The last ledger the target ingested; the chain's last ledger when absent.
    #[arg(long)]
    through: Option<u32>,
    /// Write the full report as JSON here.
    #[arg(long)]
    out: Option<PathBuf>,
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
        dump: !c.no_dump,
        restart: c.restart,
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
        HarnessCommand::Carry(a) => {
            let dump = shapekit_harness::dump::Dump::read(&a.dump)?;
            let chain = shapekit_pack::chain::ChainFile::read(&a.chain_dir)
                .with_context(|| format!("reading {}", a.chain_dir.display()))?;
            let through = a.through.unwrap_or(chain.ledger_range.last);
            let known = shapekit_harness::relations::known::Known::from_chain(
                &a.chain_dir,
                &chain.passphrase,
                through,
            )?;
            let r = shapekit_harness::relations::carry::check(&dump, &known);
            println!(
                "known: {} addresses ({} found in the dump), {} amounts; {} cells",
                r.known_addresses, r.addresses_found, r.known_amounts, r.cells
            );
            for c in &r.carried {
                println!(
                    "carried: {} {} {}: {} cells, {} values",
                    c.column,
                    c.kind,
                    c.encoding.name(),
                    c.cells,
                    c.values
                );
            }
            for p in &r.problems {
                println!(
                    "{}: {} row {}: {}{}",
                    p.kind.name(),
                    p.column,
                    p.row,
                    p.cell,
                    p.of.as_deref()
                        .or(p.decodes_to.as_deref())
                        .map(|v| format!(" ({v})"))
                        .unwrap_or_default()
                );
            }
            println!("truncated: {}, fabricated: {}", r.truncated, r.fabricated);
            if let Some(out) = &a.out {
                let mut text = serde_json::to_string_pretty(&r)?;
                text.push('\n');
                std::fs::write(out, text).with_context(|| format!("writing {}", out.display()))?;
            }
            Ok(r.truncated == 0 && r.fabricated == 0)
        }
        HarnessCommand::State(c) => state(c),
        HarnessCommand::Relate(a) => {
            use shapekit_harness::dump::Dump;
            use shapekit_harness::relations::replay;
            let base = dumps_in(&a.base)?;
            let other = dumps_in(&a.other)?;
            let control = a.control.as_deref().map(dumps_in).transpose()?;
            if base.is_empty() {
                bail!("{} holds no dump", a.base.display());
            }
            let mut reports = Vec::new();
            for ((chain, net), path) in &base {
                // the same network first; otherwise the chain's other variant
                let Some(((_, onet), opath)) = other
                    .get_key_value(&(chain.clone(), net.clone()))
                    .or_else(|| other.iter().find(|((c, _), _)| c == chain))
                else {
                    println!("{chain} ({net}): no dump in {}", a.other.display());
                    reports.push(None);
                    continue;
                };
                let bd = Dump::read(path)?;
                let od = Dump::read(opath)?;
                let noise = match control
                    .as_ref()
                    .and_then(|c| c.get(&(chain.clone(), net.clone())))
                {
                    Some(cp) => replay::noise_columns(&bd, &Dump::read(cp)?),
                    None => Default::default(),
                };
                let tr = if onet == net {
                    replay::Translation::default()
                } else {
                    let dir = a.corpus.join("chains").join(chain);
                    replay::Translation::between_chains(&dir.join(net), &dir.join(onet))?
                };
                let mut r = replay::compare(&bd, &od, &tr, &noise);
                r.chain = chain.clone();
                r.base = format!("{} ({net})", a.base.display());
                r.other = format!("{} ({onet})", a.other.display());
                println!(
                    "{chain} ({net} vs {onet}): {}, {} tables, {} rows, noise columns: {}{}",
                    if r.equal { "same" } else { "DIFFERENT" },
                    r.tables,
                    r.rows,
                    if r.noise_columns.is_empty() {
                        "none".to_owned()
                    } else {
                        r.noise_columns.join(", ")
                    },
                    if r.translated {
                        format!(", {} translated values", r.translation_values)
                    } else {
                        String::new()
                    }
                );
                for d in &r.differences {
                    println!(
                        "  {}: {} rows only in base, {} only in other; columns {}",
                        d.table,
                        d.only_base,
                        d.only_other,
                        d.columns.join(", ")
                    );
                }
                for t in r
                    .tables_only_base
                    .iter()
                    .chain(&r.tables_only_other)
                    .chain(&r.column_mismatches)
                {
                    println!("  table {t} is not comparable");
                }
                reports.push(Some(r));
            }
            if let Some(out) = &a.out {
                let mut text = serde_json::to_string_pretty(&reports)?;
                text.push('\n');
                std::fs::write(out, text).with_context(|| format!("writing {}", out.display()))?;
            }
            Ok(reports.iter().all(|r| r.as_ref().is_some_and(|r| r.equal)))
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

fn load_runs(
    runs: &[PathBuf],
    corpus: &std::path::Path,
    network: Option<&str>,
) -> Result<Vec<shapekit_harness::relations::state::RunChain>> {
    let mut out = Vec::new();
    for r in runs {
        out.extend(
            shapekit_harness::relations::state::load_run(r, corpus)?
                .into_iter()
                .filter(|c| network.is_none_or(|n| c.network == n)),
        );
    }
    Ok(out)
}

fn state(c: StateCommand) -> Result<bool> {
    use shapekit_harness::relations::state;
    match c {
        StateCommand::Infer {
            run,
            network,
            corpus,
            write,
        } => {
            let chains = load_runs(&run, &corpus, network.as_deref())?;
            let ordinary: Vec<_> = chains.iter().filter(|c| c.ordinary).collect();
            if ordinary.is_empty() {
                bail!("the runs hold no ordinary chain with a dump");
            }
            let per: Vec<_> = ordinary
                .iter()
                .map(|c| state::infer(&c.dump, &c.core, &c.known))
                .collect();
            let (m, notes) = state::merge(&per);
            for n in &notes {
                println!("note: {n}");
            }
            for t in &m.table {
                println!(
                    "{} holds {} entries keyed by {}; {} mapped columns, {} ambiguous; {} chains, {} rows",
                    t.name,
                    t.entry.name(),
                    t.keys
                        .iter()
                        .map(|k| format!("{}={}", k.field, k.column))
                        .collect::<Vec<_>>()
                        .join(", "),
                    t.fields.len(),
                    t.ambiguous.len(),
                    t.chains,
                    t.rows
                );
            }
            let names: Vec<String> = ordinary
                .iter()
                .map(|c| format!("{} ({})", c.chain, c.network))
                .collect();
            let mut targets: Vec<&str> = ordinary.iter().map(|c| c.target.as_str()).collect();
            targets.sort();
            targets.dedup();
            let header = format!(
                "which stored table and column holds which core entry field, inferred by\n\
                 `shapekit harness state infer` on {} from {} ordinary chains of {} chains with a dump:\n{}",
                targets.join(", "),
                ordinary.len(),
                chains.len(),
                names.join("\n")
            );
            std::fs::write(&write, m.render(&header)?)
                .with_context(|| format!("writing {}", write.display()))?;
            println!(
                "wrote {} from {} ordinary chains",
                write.display(),
                ordinary.len()
            );
            Ok(true)
        }
        StateCommand::Check {
            run,
            mapping,
            network,
            corpus,
            out,
        } => {
            let m = state::Mapping::load(&mapping)?;
            let chains = load_runs(&run, &corpus, network.as_deref())?;
            let results: Vec<_> = chains.iter().map(|c| state::check_chain(c, &m)).collect();
            let (mut ord, mut ord_ok) = (0, 0);
            for r in &results {
                if r.ordinary {
                    ord += 1;
                    if r.state.ok {
                        ord_ok += 1;
                    }
                }
                println!(
                    "{} ({}): {} rows, {} fields compared, {} mismatched, {} stale, {} missing{}",
                    r.chain,
                    r.network,
                    r.state.rows,
                    r.state.fields_compared,
                    r.state.mismatches.len(),
                    r.state.stale.len(),
                    r.state.missing.len(),
                    r.disagreement
                        .as_deref()
                        .map(|d| format!("; disagreement: {d}"))
                        .unwrap_or_default()
                );
                for x in r.state.mismatches.iter().take(5) {
                    println!(
                        "  {} {} {} ({}): stored {:?}, core {}",
                        x.table,
                        x.key.join(" "),
                        x.column,
                        x.field,
                        x.stored,
                        x.core
                    );
                }
                for x in r.state.stale.iter().chain(&r.state.missing).take(5) {
                    println!("  {x}");
                }
                if !r.route.groups.is_empty() {
                    println!(
                        "  route: {} groups of records core holds equally ({}), {} table comparisons, {} differences",
                        r.route.groups.len(),
                        r.route
                            .groups
                            .iter()
                            .map(|g| format!(
                                "{} x{}, {} bytes",
                                g.entry.name(),
                                g.keys.len(),
                                g.bytes
                            ))
                            .collect::<Vec<_>>()
                            .join("; "),
                        r.route.compared,
                        r.route.differences.len()
                    );
                }
                for d in &r.route.differences {
                    println!(
                        "  route difference in {} for {}: {}",
                        d.table,
                        d.keys
                            .iter()
                            .map(|k| k.join(" "))
                            .collect::<Vec<_>>()
                            .join(" / "),
                        d.detail
                    );
                }
            }
            println!(
                "ordinary chains where the mapping holds: {ord_ok} of {ord}; chains: {}; disagreements with the oracle: {}",
                results.len(),
                results.iter().filter(|r| r.disagreement.is_some()).count()
            );
            if let Some(o) = &out {
                let mut text = serde_json::to_string_pretty(&results)?;
                text.push('\n');
                std::fs::write(o, text).with_context(|| format!("writing {}", o.display()))?;
            }
            Ok(ord_ok == ord)
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
