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
    /// The mapped adapter: speaks the adapter protocol on stdin and stdout for a PostgreSQL
    /// indexer described by a mapping file (docs/mapping-adapter.md). `harness run --mapping`
    /// starts it for you.
    Adapter(MappedAdapterArgs),
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
    /// Free field inertness: attempts that differ in one field core does not validate, and
    /// that core treated alike, must get the same verdict in a run.
    FreeField(FreeFieldArgs),
    /// Compare the target's stored accounts, trustlines, asset contract balances and pools
    /// with core's final entries: infer the table mapping from runs, or check runs with it.
    #[command(subcommand)]
    State(StateCommand),
    /// Check, or with --write regenerate, schema/adapter-protocol.schema.json and the
    /// vocabulary section of docs/adapter-interface.md.
    Schema {
        /// Write the schema and the vocabulary section instead of checking them.
        #[arg(long)]
        write: bool,
        /// The repository root.
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
}

#[derive(Args, Clone)]
pub struct Common {
    /// The corpus directory, holding manifest.json.
    #[arg(long, default_value = "corpus")]
    corpus: PathBuf,
    /// Delivery mode to ask the adapter for; its default when absent.
    #[arg(long, value_parser = ["sep54-filesystem", "sep54-s3", "sep54-gcs", "rpc"])]
    delivery: Option<String>,
    /// Memory limit the adapter applies to the target, in bytes.
    #[arg(long, default_value_t = 4 * 1024 * 1024 * 1024)]
    memory_limit: u64,
    /// Seconds a target may go without ingesting a released ledger.
    #[arg(long, default_value_t = 120)]
    ledger_timeout: u64,
    /// Seconds the target may go without ingesting a ledger while it bootstraps.
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
    #[command(flatten)]
    pub(crate) adapter: AdapterChoice,
    /// Only entries of this family; repeatable.
    #[arg(long)]
    pub(crate) family: Vec<String>,
    /// Only this chain id; repeatable.
    #[arg(long)]
    pub(crate) chain: Vec<String>,
    /// A chain outside the corpus, as `<name>=<dir>` where `<dir>` is laid out like
    /// `corpus/chains/<chain>/<network>`; repeatable. Run for its dump alone, with no entry to
    /// judge, from its first checkpoint; the corpus selection is not used.
    #[arg(long, value_name = "NAME=DIR")]
    pub(crate) chain_dir: Vec<String>,
    /// Which passphrase's recording of each chain to replay.
    #[arg(long, default_value = "testnet", value_parser = ["testnet", "mainnet", "both"])]
    pub(crate) network: String,
    /// Directory for harness-report.json, HARNESS.md and the logs.
    #[arg(long, default_value = "harness-out")]
    pub(crate) out: PathBuf,
    /// Run the whole selection this many times in one process and fail when a later round's
    /// peak memory grows past the first round's.
    #[arg(long, default_value_t = 1)]
    pub(crate) repeat: u32,
    /// Allowed growth of a later round's peak resident memory, as a fraction of the first.
    #[arg(long, default_value_t = 0.1)]
    pub(crate) memory_slack: f64,
    /// Fail when any round's peak resident memory exceeds this many bytes.
    #[arg(long)]
    pub(crate) memory_budget: Option<u64>,
    /// Expected verdicts (see `harness expect`): fail when any entry's verdict or reason
    /// differs, an entry is missing or unlisted, or the target build is another one.
    #[arg(long)]
    pub(crate) expect: Option<PathBuf>,
    #[command(flatten)]
    pub(crate) common: Common,
}

#[derive(Subcommand)]
pub enum StateCommand {
    /// Route equivalence over a recipe pair (corpus/recipes/explore.pairs/<pair>): the
    /// target must keep the same rows for every entry core holds identically at the end of
    /// both chains.
    Pair {
        /// The run of chain `a`, run with --chain-dir.
        #[arg(long)]
        a: PathBuf,
        /// The run of chain `b`.
        #[arg(long)]
        b: PathBuf,
        /// The pair's `pair.json`.
        #[arg(long)]
        pair: PathBuf,
        /// The table mapping `harness state infer` wrote.
        #[arg(long)]
        mapping: PathBuf,
        /// Which network's chain of each run, when a run holds both.
        #[arg(long, default_value = "testnet", value_parser = ["testnet", "mainnet"])]
        network: String,
        /// The corpus directory, holding manifest.json.
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
        /// Write the result as JSON here.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Infer which table and column holds which entry field from the ordinary chains of the
    /// given runs (no entry mishandled or dropped, carry check clean), keep what holds in
    /// all of them, and write the mapping.
    Infer {
        /// A `harness run --out` directory whose run kept dumps; repeatable.
        #[arg(long, required = true)]
        run: Vec<PathBuf>,
        /// Only chains of this network.
        #[arg(long, value_parser = ["testnet", "mainnet"])]
        network: Option<String>,
        /// The corpus directory, holding manifest.json.
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
        /// Where to write the inferred mapping.
        #[arg(long)]
        write: PathBuf,
    },
    /// Apply a mapping to every chain of the given runs and list where the stored state
    /// differs from core's, and where that disagrees with the oracle's verdicts.
    Check {
        /// A `harness run --out` directory whose run kept dumps; repeatable.
        #[arg(long, required = true)]
        run: Vec<PathBuf>,
        /// The table mapping `harness state infer` wrote.
        #[arg(long)]
        mapping: PathBuf,
        /// Core entries a table leaves out on purpose, each with the target's source line.
        #[arg(long)]
        exclusions: Option<PathBuf>,
        /// Only chains of this network.
        #[arg(long, value_parser = ["testnet", "mainnet"])]
        network: Option<String>,
        /// The corpus directory, holding manifest.json.
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
        /// Write the per-chain results as JSON here.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Args)]
pub struct FreeFieldArgs {
    /// A `harness run --out` directory whose run judged the attempts' entries.
    #[arg(long)]
    run: PathBuf,
    /// The not-validated fields of this table, with their witness entries.
    #[arg(long)]
    validation: Option<PathBuf>,
    /// Hand-written groups over corpus entries.
    #[arg(long)]
    groups: Option<PathBuf>,
    /// Write the report as JSON here.
    #[arg(long)]
    out: Option<PathBuf>,
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
    /// The corpus directory, holding manifest.json.
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
    /// The reproductions table, for example adapters/walletbackend/reproductions.toml.
    #[arg(long)]
    table: PathBuf,
    /// Run every case through the mapped adapter with this mapping file instead of the
    /// table's adapter command; each case's commit is passed as `--var commit=<commit>`.
    #[arg(long)]
    mapping: Option<PathBuf>,
    /// A mapping variable for every case, as NAME=VALUE; repeatable.
    #[arg(long = "var", value_name = "NAME=VALUE", requires = "mapping")]
    vars: Vec<String>,
    /// docker's --cpus for every container the mapped adapter starts.
    #[arg(long, requires = "mapping")]
    cpus: Option<String>,
    /// The buildx builder the mapped adapter builds images with.
    #[arg(long, requires = "mapping")]
    builder: Option<String>,
    /// Only the reproduction with this id; repeatable.
    #[arg(long)]
    only: Vec<String>,
    /// Directory for one harness run per reproduction side and the summary.
    #[arg(long, default_value = "harness-reproduce")]
    out: PathBuf,
    #[command(flatten)]
    common: Common,
}

/// Which adapter a run talks to: a program, or the mapped adapter with a mapping file.
#[derive(Args, Clone)]
pub struct AdapterChoice {
    /// The adapter command line, split on whitespace; quote an argument with ' or ".
    #[arg(long, required_unless_present = "mapping", conflicts_with = "mapping")]
    pub(crate) adapter: Option<String>,
    /// Connect a PostgreSQL indexer through a mapping file instead of an adapter program
    /// (docs/mapping-adapter.md), for example adapters/walletbackend/adapter.toml.
    #[arg(long)]
    pub(crate) mapping: Option<PathBuf>,
    /// A variable the mapping file defines, as NAME=VALUE; repeatable.
    #[arg(long = "var", value_name = "NAME=VALUE", requires = "mapping")]
    pub(crate) vars: Vec<String>,
    /// docker's --cpus for every container the mapped adapter starts.
    #[arg(long, requires = "mapping")]
    pub(crate) cpus: Option<String>,
    /// The buildx builder the mapped adapter builds images with.
    #[arg(long, requires = "mapping")]
    pub(crate) builder: Option<String>,
}

impl AdapterChoice {
    pub(crate) fn command(&self) -> Result<Vec<String>> {
        match (&self.adapter, &self.mapping) {
            (Some(a), None) => split_command(a),
            (None, Some(m)) => {
                mapped_command(m, &self.vars, self.cpus.as_deref(), self.builder.as_deref())
            }
            _ => bail!("give exactly one of --adapter and --mapping"),
        }
    }
}

/// The command line that runs this binary's mapped adapter on `mapping`. The mapping is
/// checked here, so a broken file fails before any chain runs.
fn mapped_command(
    mapping: &std::path::Path,
    vars: &[String],
    cpus: Option<&str>,
    builder: Option<&str>,
) -> Result<Vec<String>> {
    shapekit_mapped::mapping::Mapping::load(mapping)?;
    let exe = std::env::current_exe().context("locating the shapekit binary")?;
    let path = std::fs::canonicalize(mapping)
        .with_context(|| format!("resolving {}", mapping.display()))?;
    let mut cmd = vec![
        exe.to_string_lossy().into_owned(),
        "harness".into(),
        "adapter".into(),
        "--mapping".into(),
        path.to_string_lossy().into_owned(),
    ];
    for v in vars {
        if !v.contains('=') {
            bail!("--var {v:?} is not NAME=VALUE");
        }
        cmd.push("--var".into());
        cmd.push(v.clone());
    }
    if let Some(c) = cpus {
        cmd.extend(["--cpus".into(), c.into()]);
    }
    if let Some(b) = builder {
        cmd.extend(["--builder".into(), b.into()]);
    }
    Ok(cmd)
}

#[derive(Args)]
pub struct MappedAdapterArgs {
    /// The mapping file.
    #[arg(long)]
    mapping: PathBuf,
    /// A variable the mapping file defines, as NAME=VALUE; repeatable.
    #[arg(long = "var", value_name = "NAME=VALUE")]
    vars: Vec<String>,
    /// docker's --cpus for every container the adapter starts; no limit when absent.
    #[arg(long)]
    cpus: Option<String>,
    /// The buildx builder to build images with; the current one when absent.
    #[arg(long)]
    builder: Option<String>,
}

fn serve_mapped(a: MappedAdapterArgs) -> Result<bool> {
    use shapekit_mapped::adapter::{Adapter, Options};
    let mapping = shapekit_mapped::mapping::Mapping::load(&a.mapping)?;
    let mut vars = std::collections::BTreeMap::new();
    for v in &a.vars {
        let (k, val) = v
            .split_once('=')
            .with_context(|| format!("--var {v:?} is not NAME=VALUE"))?;
        vars.insert(k.to_owned(), val.to_owned());
    }
    let mut adapter = Adapter::new(Options {
        mapping,
        vars,
        cpus: a.cpus,
        builder: a.builder,
    })?;
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut out = std::io::stdout().lock();
    shapekit_mapped::serve::serve(&mut adapter, &mut input, &mut out)?;
    Ok(true)
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

/// What `check indexer`, `harness run` and `harness reproduce` say on a host where the docker
/// delivery network cannot work.
pub const NEEDS_LINUX_DOCKER: &str = "check indexer needs a Linux docker host: the isolated delivery network is not reachable from macOS (docker runs in a VM)";

// docker on macOS runs in a vm: the gateway of the --internal network, where the replay server
// binds, and the containers' addresses, which the adapters connect to, exist only inside it
fn docker_host(docker: bool, macos: bool) -> Result<()> {
    if docker && macos {
        bail!(
            "{NEEDS_LINUX_DOCKER}; --no-docker runs without that network, for a target that runs as host processes"
        );
    }
    Ok(())
}

/// Whether this build's host runs docker in a VM, out of reach of the delivery network.
pub fn docker_in_a_vm() -> bool {
    cfg!(target_os = "macos")
}

fn linux_docker_host(c: &Common) -> Result<()> {
    docker_host(!c.no_docker, docker_in_a_vm())
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
        HarnessCommand::FreeField(a) => {
            use shapekit_harness::relations::free_field::{Groups, check};
            let mut groups = Groups::default();
            if let Some(p) = &a.validation {
                groups.group.extend(Groups::from_validation(p)?.group);
            }
            if let Some(p) = &a.groups {
                groups.group.extend(Groups::load(p)?.group);
            }
            if groups.group.is_empty() {
                bail!("no groups: give --validation, --groups or both");
            }
            let rep = read_report(&a.run.join(report::REPORT_FILE))?;
            let r = check(&rep.entries, &groups);
            println!(
                "{} groups, {} attempts judged, {} not in the run, {} sets of alike attempts compared, {} differences",
                r.groups,
                r.judged,
                r.not_run,
                r.compared,
                r.differences.len()
            );
            for d in &r.differences {
                println!("{} ({}, core: {}):", d.field, d.network, d.outcome);
                for (label, j) in &d.attempts {
                    println!(
                        "  {label}: {}{}{}",
                        j.verdict,
                        j.reason
                            .as_deref()
                            .map(|x| format!(" ({x})"))
                            .unwrap_or_default(),
                        if j.problems.is_empty() {
                            String::new()
                        } else {
                            format!(", {}", j.problems.join(", "))
                        }
                    );
                }
            }
            if let Some(o) = &a.out {
                let mut text = serde_json::to_string_pretty(&r)?;
                text.push('\n');
                std::fs::write(o, text).with_context(|| format!("writing {}", o.display()))?;
            }
            free_field_verdict(&r)
        }
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
            linux_docker_host(&a.common)?;
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
            let sel = if a.chain_dir.is_empty() {
                select(
                    &a.common.corpus,
                    &Filter {
                        families: &a.family,
                        chains: &a.chain,
                        networks: &networks,
                    },
                )?
            } else {
                let mut sel = shapekit_harness::corpus::Selection {
                    manifest_sha256: "none: chains named with --chain-dir".into(),
                    ..Default::default()
                };
                for c in &a.chain_dir {
                    let (name, dir) = c
                        .split_once('=')
                        .with_context(|| format!("--chain-dir {c:?} is not NAME=DIR"))?;
                    let dir = PathBuf::from(dir);
                    let chain = shapekit_pack::chain::ChainFile::read(&dir)
                        .with_context(|| format!("reading {}", dir.display()))?;
                    let network = match chain.passphrase.as_str() {
                        "Test SDF Network ; September 2015" => "testnet",
                        "Public Global Stellar Network ; September 2015" => "mainnet",
                        other => bail!(
                            "{}: passphrase {other:?} is neither testnet's nor mainnet's",
                            dir.display()
                        ),
                    };
                    sel.jobs.push(shapekit_harness::corpus::ChainJob {
                        chain: name.to_owned(),
                        network: network.to_owned(),
                        dir,
                        entries: Vec::new(),
                        external: true,
                    });
                }
                sel
            };
            if sel.jobs.is_empty() {
                bail!("the selection has no chain with an evaluable entry");
            }
            let opts = options(&a.common, a.adapter.command()?, a.out.join("logs"))?;
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
        HarnessCommand::Adapter(a) => serve_mapped(a),
        HarnessCommand::Reproduce(a) => {
            linux_docker_host(&a.common)?;
            let mut table = reproduce::load(&a.table)?;
            if let Some(m) = &a.mapping {
                let mut vars = a.vars.clone();
                vars.push("commit={commit}".into());
                table.adapter.command =
                    mapped_command(m, &vars, a.cpus.as_deref(), a.builder.as_deref())?;
            }
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
        StateCommand::Pair {
            a,
            b,
            pair,
            mapping,
            network,
            corpus,
            out,
        } => {
            use shapekit_harness::relations::route;
            let m = state::Mapping::load(&mapping)?;
            let one = |run: &PathBuf| -> Result<state::RunChain> {
                let mut cs: Vec<_> = state::load_run(run, &corpus)?
                    .into_iter()
                    .filter(|c| c.network == network)
                    .collect();
                if cs.len() != 1 {
                    bail!(
                        "{} holds {} {network} chains with a dump, the pair needs one",
                        run.display(),
                        cs.len()
                    );
                }
                Ok(cs.remove(0))
            };
            let (ca, cb) = (one(&a)?, one(&b)?);
            let keys = pair_keys(&pair)?;
            let results = route::pair_check(&ca, &cb, &keys, &m);
            let mut ok = true;
            for r in &results {
                println!(
                    "{} {}: core {}",
                    r.entry.name(),
                    r.key.join(" "),
                    if r.core_equal { "equal" } else { "DIFFERENT" }
                );
                ok &= r.core_equal && r.changes_equal;
                println!(
                    "  exported changes about it: {} ({} after a, {} after b)",
                    if r.changes_equal { "same" } else { "DIFFERENT" },
                    r.changes_a.len(),
                    r.changes_b.len()
                );
                if !r.changes_equal {
                    println!("    a: {:?}\n    b: {:?}", r.changes_a, r.changes_b);
                }
                for t in &r.tables {
                    ok &= t.equal;
                    println!(
                        "  {}: {} ({} row(s) after a, {} after b)",
                        t.table,
                        if t.equal { "same" } else { "DIFFERENT" },
                        t.rows_a.len(),
                        t.rows_b.len()
                    );
                    if !t.equal {
                        println!("    a: {:?}\n    b: {:?}", t.rows_a, t.rows_b);
                    }
                }
            }
            if let Some(o) = &out {
                let mut text = serde_json::to_string_pretty(&results)?;
                text.push('\n');
                std::fs::write(o, text).with_context(|| format!("writing {}", o.display()))?;
            }
            Ok(ok)
        }
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
            exclusions,
            network,
            corpus,
            out,
        } => {
            let m = state::Mapping::load(&mapping)?;
            let chains = load_runs(&run, &corpus, network.as_deref())?;
            let x = exclusions
                .as_deref()
                .map(state::Exclusions::load)
                .transpose()?
                .unwrap_or_default();
            let results: Vec<_> = chains
                .iter()
                .map(|c| state::check_chain(c, &m, &x))
                .collect();
            for r in &results {
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
                for (why, n) in &r.state.excluded {
                    println!("  excluded {n}: {why}");
                }
                if !r.problems_outside_state.is_empty() {
                    println!(
                        "  the oracle's problems are in facts this comparison does not read: {}",
                        r.problems_outside_state.join(", ")
                    );
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
            let t = StateTally::of(&results);
            println!(
                "ordinary chains where the mapping holds: {} of {}; ordinary chains that compared no field: {}; chains: {}; disagreements with the oracle: {}",
                t.held,
                t.held + t.failed,
                t.empty,
                results.len(),
                results.iter().filter(|r| r.disagreement.is_some()).count()
            );
            if let Some(o) = &out {
                let mut text = serde_json::to_string_pretty(&results)?;
                text.push('\n');
                std::fs::write(o, text).with_context(|| format!("writing {}", o.display()))?;
            }
            t.verdict(network.as_deref())
        }
    }
}

/// The verdict of `harness free-field`: no difference among no compared sets says nothing
/// about the groups, so a run that compared none is an error, not a pass.
fn free_field_verdict(
    r: &shapekit_harness::relations::free_field::FreeFieldReport,
) -> Result<bool> {
    if r.compared == 0 {
        bail!("no set of alike attempts was compared, so the run says nothing about the groups");
    }
    Ok(r.differences.is_empty())
}

/// The ledger keys `harness state pair` compares, read from the pair file's `entries`. A
/// file listing none is an error: comparing nothing would pass without showing anything.
fn pair_keys(
    pair: &std::path::Path,
) -> Result<Vec<(shapekit_harness::relations::state::EntryKind, Vec<String>)>> {
    use shapekit_harness::relations::route;
    let text =
        std::fs::read_to_string(pair).with_context(|| format!("reading {}", pair.display()))?;
    let v: serde_json::Value =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", pair.display()))?;
    let keys = v
        .get("entries")
        .and_then(|e| e.as_array())
        .with_context(|| format!("{} has no entries", pair.display()))?
        .iter()
        .map(|e| route::pair_key(e.get("key").unwrap_or(&serde_json::Value::Null)))
        .collect::<Result<Vec<_>>>()?;
    if keys.is_empty() {
        bail!(
            "{} lists no entries, so there is nothing to compare",
            pair.display()
        );
    }
    Ok(keys)
}

/// The ordinary chains of `harness state check`, by what their comparison showed.
#[derive(Debug, Default, PartialEq, Eq)]
struct StateTally {
    /// Ordinary chains in the runs.
    ordinary: usize,
    /// The mapping held and at least one field was compared.
    held: usize,
    /// The stored state differs from core's.
    failed: usize,
    /// The mapping held only because it compared no field (a mapping that maps nothing of
    /// the chain): not evidence that it holds, so not counted as compared.
    empty: usize,
}

impl StateTally {
    fn of(results: &[shapekit_harness::relations::state::ChainState]) -> StateTally {
        let mut t = StateTally::default();
        for r in results.iter().filter(|r| r.ordinary) {
            t.ordinary += 1;
            if !r.state.ok {
                t.failed += 1;
            } else if r.state.fields_compared == 0 {
                t.empty += 1;
            } else {
                t.held += 1;
            }
        }
        t
    }

    /// Fails on any ordinary chain whose state differs; holds only when at least one
    /// ordinary chain compared a field.
    fn verdict(&self, network: Option<&str>) -> Result<bool> {
        if self.failed > 0 {
            return Ok(false);
        }
        let on = network.map(|n| format!(" on {n}")).unwrap_or_default();
        if self.ordinary == 0 {
            bail!("the runs hold no ordinary chain with a dump{on}, so nothing was compared");
        }
        if self.held == 0 {
            bail!(
                "the {} ordinary chains with a dump{on} compared no field, so nothing was compared",
                self.ordinary
            );
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        NEEDS_LINUX_DOCKER, StateTally, docker_host, docker_in_a_vm, free_field_verdict, pair_keys,
        split_command,
    };
    use shapekit_harness::relations::free_field::FreeFieldReport;
    use shapekit_harness::relations::state::{ChainState, StateReport};

    fn chain(ordinary: bool, fields_compared: u64, ok: bool) -> ChainState {
        ChainState {
            chain: "c".into(),
            network: "testnet".into(),
            ordinary,
            verdicts: Default::default(),
            state: StateReport {
                fields_compared,
                ok,
                ..StateReport::default()
            },
            route: Default::default(),
            disagreement: None,
            problems_outside_state: Vec::new(),
        }
    }

    #[test]
    fn state_check_needs_a_chain_that_compared_a_field() {
        let err = |results: &[ChainState]| {
            StateTally::of(results)
                .verdict(Some("testnet"))
                .unwrap_err()
                .to_string()
        };
        // no chain, or only chains that are not ordinary
        assert!(err(&[]).contains("no ordinary chain with a dump on testnet"));
        assert!(err(&[chain(false, 9, true)]).contains("no ordinary chain"));
        // a mapping that maps nothing of the only ordinary chain holds on it vacuously
        let t = StateTally::of(&[chain(true, 0, true)]);
        assert_eq!((t.held, t.empty), (0, 1));
        assert!(err(&[chain(true, 0, true)]).contains("compared no field"));
        // one chain that compared fields is enough; an empty one beside it is not counted
        let both = [chain(true, 0, true), chain(true, 3, true)];
        assert_eq!(StateTally::of(&both).held, 1);
        assert!(StateTally::of(&both).verdict(None).unwrap());
        // a difference fails, whether or not a field was compared
        for bad in [chain(true, 3, false), chain(true, 0, false)] {
            assert!(!StateTally::of(&[bad]).verdict(None).unwrap());
        }
    }

    #[test]
    fn state_pair_needs_an_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let pair = tmp.path().join("pair.json");
        std::fs::write(&pair, r#"{"entries":[]}"#).unwrap();
        let e = pair_keys(&pair).unwrap_err().to_string();
        assert!(e.contains("lists no entries"), "{e}");
        std::fs::write(&pair, r#"{}"#).unwrap();
        let e = pair_keys(&pair).unwrap_err().to_string();
        assert!(e.contains("has no entries"), "{e}");
    }

    #[test]
    fn free_field_needs_a_compared_set() {
        let none = FreeFieldReport::default();
        let e = free_field_verdict(&none).unwrap_err().to_string();
        assert!(e.contains("no set of alike attempts was compared"), "{e}");
        let one = FreeFieldReport {
            compared: 1,
            ..FreeFieldReport::default()
        };
        assert!(free_field_verdict(&one).unwrap());
    }

    #[test]
    fn the_docker_delivery_network_needs_a_linux_host() {
        assert!(docker_host(true, false).is_ok());
        assert!(docker_host(false, true).is_ok());
        let e = docker_host(true, true).unwrap_err().to_string();
        assert!(e.starts_with(NEEDS_LINUX_DOCKER), "{e}");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn on_macos_a_docker_run_stops_before_it_starts() {
        assert!(docker_in_a_vm());
        assert!(docker_host(true, docker_in_a_vm()).is_err());
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn off_macos_a_docker_run_goes_ahead() {
        assert!(!docker_in_a_vm());
        assert!(docker_host(true, docker_in_a_vm()).is_ok());
    }

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
