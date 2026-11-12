//! One harness run: every selected chain through the adapter, ledger by ledger, then the
//! export compared with the oracle files.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use shapekit_oracle::vocab::{ORACLE_VOCAB_VERSION, OracleFile};
use shapekit_pack::chain::ChainFile;
use shapekit_serve::{DEFAULT_CACHE_BYTES, ServeConfig};

use crate::adapter::Adapter;
use crate::compare::{
    EntryVerdict, Reason, Stop, Verdict, compare_tx, stopped_verdict, verdict_of,
};
use crate::corpus::{ChainJob, EntryRef};
use crate::dockernet;
use crate::protocol::{
    ADAPTER_PROTOCOL_VERSION, AdvanceResult, Bootstrap, Command, Delivery, DeliveryMode,
    DescribeResult, ExportRecord, Health, Lake, LedgerWindow, Limits, StartParams, StartResult,
    StatusResult, StopResult, TxRecord, Unmapped, vocabulary_fields,
};

/// Ledgers per history checkpoint: core's default, which every chain is recorded with.
pub const CHECKPOINT_FREQUENCY: u32 = 64;

#[derive(Debug, Clone)]
pub struct Timeouts {
    pub describe: Duration,
    pub start: Duration,
    pub advance: Duration,
    pub status: Duration,
    pub export: Duration,
    pub stop: Duration,
    /// From `start` until the target reports the bootstrap ledger ingested.
    pub bootstrap: Duration,
    /// How long a target may go without ingesting a released ledger.
    pub ledger: Duration,
    pub poll: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Timeouts {
            describe: Duration::from_secs(60),
            start: Duration::from_secs(30 * 60),
            advance: Duration::from_secs(120),
            status: Duration::from_secs(60),
            export: Duration::from_secs(10 * 60),
            stop: Duration::from_secs(5 * 60),
            bootstrap: Duration::from_secs(10 * 60),
            ledger: Duration::from_secs(120),
            poll: Duration::from_millis(500),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Program and arguments of the adapter.
    pub adapter: Vec<String>,
    /// The delivery mode to ask for; the adapter's default when `None`.
    pub delivery: Option<DeliveryMode>,
    pub memory_limit_bytes: u64,
    pub timeouts: Timeouts,
    /// Create an internal Docker network per chain and bind the replay server to its
    /// gateway. Without it the server binds to 127.0.0.1.
    pub docker: bool,
    /// Where adapter and target logs go.
    pub logs: PathBuf,
}

/// What happened to one chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChainResult {
    pub chain: String,
    pub network: String,
    pub protocol: u32,
    pub first_ledger: u32,
    pub last_ledger: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bootstrap: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<DeliveryMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_ingested: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<Stop>,
    /// Why the chain could not be run to the end, when it could not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_peak_memory_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub components: BTreeMap<String, String>,
    pub exported_records: u64,
    /// Rows the adapter could not tie to a transaction; the first 50 are kept.
    pub unattributed_rows: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unattributed_sample: Vec<Unmapped>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    pub adapter_exit: String,
    pub seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryRow {
    pub entry: String,
    pub family: String,
    pub chain: String,
    pub network: String,
    pub ledger: u32,
    #[serde(flatten)]
    pub result: EntryVerdict,
}

#[derive(Debug, Clone, Default)]
pub struct RoundResult {
    pub describe: Option<DescribeResult>,
    pub chains: Vec<ChainResult>,
    pub entries: Vec<EntryRow>,
}

static NETWORKS: AtomicU32 = AtomicU32::new(0);

fn rows(job: &ChainJob, f: impl Fn(&EntryRef) -> EntryVerdict) -> Vec<EntryRow> {
    job.entries
        .iter()
        .map(|e| EntryRow {
            entry: e.id.clone(),
            family: e.family.clone(),
            chain: job.chain.clone(),
            network: job.network.clone(),
            ledger: e.ledger,
            result: f(e),
        })
        .collect()
}

pub fn run_round(jobs: &[ChainJob], opts: &RunOptions) -> Result<RoundResult> {
    std::fs::create_dir_all(&opts.logs)
        .with_context(|| format!("creating {}", opts.logs.display()))?;
    let vocab = vocabulary_fields();
    let mut out = RoundResult::default();
    for job in jobs {
        let started = Instant::now();
        let (mut chain, entries, describe) = run_chain(job, opts, &vocab);
        chain.seconds = started.elapsed().as_secs_f64();
        if out.describe.is_none() {
            out.describe = describe;
        }
        eprintln!(
            "harness: {} ({}): {}",
            job.chain,
            job.network,
            summary_line(&entries, &chain)
        );
        out.chains.push(chain);
        out.entries.extend(entries);
    }
    Ok(out)
}

fn summary_line(entries: &[EntryRow], chain: &ChainResult) -> String {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for e in entries {
        *counts.entry(e.result.verdict.name()).or_default() += 1;
    }
    let mut s: Vec<String> = counts.iter().map(|(k, v)| format!("{v} {k}")).collect();
    if let Some(e) = &chain.error {
        s.push(format!("error: {e}"));
    }
    s.join(", ")
}

/// The chain's state while it runs, so every exit path can produce a result.
struct Ctx<'a> {
    job: &'a ChainJob,
    result: ChainResult,
}

fn run_chain(
    job: &ChainJob,
    opts: &RunOptions,
    vocab: &BTreeMap<String, Vec<String>>,
) -> (ChainResult, Vec<EntryRow>, Option<DescribeResult>) {
    let mut ctx = Ctx {
        job,
        result: ChainResult {
            chain: job.chain.clone(),
            network: job.network.clone(),
            protocol: 0,
            first_ledger: 0,
            last_ledger: 0,
            bootstrap: None,
            delivery: None,
            last_ingested: None,
            stop: None,
            error: None,
            target_peak_memory_bytes: None,
            components: BTreeMap::new(),
            exported_records: 0,
            unattributed_rows: 0,
            unattributed_sample: Vec::new(),
            notes: Vec::new(),
            adapter_exit: String::new(),
            seconds: 0.0,
        },
    };
    let mut describe = None;
    let entries = match drive(&mut ctx, opts, vocab, &mut describe) {
        Ok(entries) => entries,
        Err(e) => {
            let msg = format!("{e:#}");
            ctx.result.error = Some(msg.clone());
            rows(job, |_| {
                EntryVerdict::simple(
                    Verdict::NotEvaluated,
                    None,
                    format!("the chain could not be run: {msg}"),
                )
            })
        }
    };
    (ctx.result, entries, describe)
}

fn log_name(job: &ChainJob, what: &str) -> String {
    format!("{}.{}.{what}.log", job.chain, job.network)
}

fn drive(
    ctx: &mut Ctx<'_>,
    opts: &RunOptions,
    vocab: &BTreeMap<String, Vec<String>>,
    describe_out: &mut Option<DescribeResult>,
) -> Result<Vec<EntryRow>> {
    let job = ctx.job;
    let chain =
        ChainFile::read(&job.dir).with_context(|| format!("reading {}", job.dir.display()))?;
    ctx.result.protocol = chain.protocol;
    ctx.result.first_ledger = chain.ledger_range.first;
    ctx.result.last_ledger = chain.ledger_range.last;

    let mut oracles = Vec::with_capacity(job.entries.len());
    for e in &job.entries {
        let bytes = std::fs::read(&e.oracle_file)
            .with_context(|| format!("reading {}", e.oracle_file.display()))?;
        let file: OracleFile = serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing {}", e.oracle_file.display()))?;
        if file.oracle_vocab_version != ORACLE_VOCAB_VERSION {
            bail!(
                "{} is vocabulary version {}, the harness speaks {ORACLE_VOCAB_VERSION}; run `shapekit oracle run`",
                e.oracle_file.display(),
                file.oracle_vocab_version
            );
        }
        oracles.push(file);
    }
    let wanted: HashSet<String> = oracles
        .iter()
        .flat_map(|f| f.ledgers.iter())
        .flat_map(|l| l.transactions.iter().map(|t| t.hash.clone()))
        .collect();

    let mut adapter = Adapter::spawn(&opts.adapter, &opts.logs.join(log_name(job, "adapter")))?;
    let outcome = drive_adapter(ctx, opts, &chain, &wanted, &mut adapter, describe_out);
    ctx.result.adapter_exit = adapter.finish(Duration::from_secs(30));
    let (stop, records) = outcome?;
    let describe = describe_out.as_ref().expect("set before any ledger ran");

    if !(describe.protocols.min..=describe.protocols.max).contains(&chain.protocol) {
        return Ok(rows(job, |_| {
            EntryVerdict::simple(
                Verdict::OutOfScope,
                None,
                format!(
                    "chain protocol {} is outside the target's protocols {} to {}",
                    chain.protocol, describe.protocols.min, describe.protocols.max
                ),
            )
        }));
    }
    let bootstrap = ctx
        .result
        .bootstrap
        .ok_or_else(|| anyhow!("the chain ran without a bootstrap ledger"))?;
    let first_at_stop = stop.as_ref().and_then(|s| {
        job.entries
            .iter()
            .find(|e| e.ledger == s.ledger)
            .map(|e| e.id.clone())
    });
    let mut out = Vec::with_capacity(job.entries.len());
    for (e, oracle) in job.entries.iter().zip(oracles) {
        let result = if e.ledger <= bootstrap {
            EntryVerdict::simple(
                Verdict::NotEvaluated,
                None,
                format!(
                    "ledger {} is at or before the bootstrap ledger {bootstrap}",
                    e.ledger
                ),
            )
        } else if let Some(s) = stop.as_ref().filter(|s| e.ledger >= s.ledger) {
            stopped_verdict(e.ledger, s, first_at_stop.as_deref())
        } else {
            let mut comparisons = Vec::new();
            for l in &oracle.ledgers {
                for tx in &l.transactions {
                    comparisons.push(compare_tx(
                        tx,
                        records.get(&tx.hash),
                        &describe.claims,
                        vocab,
                    ));
                }
            }
            verdict_of(comparisons)
        };
        out.push(EntryRow {
            entry: e.id.clone(),
            family: e.family.clone(),
            chain: job.chain.clone(),
            network: job.network.clone(),
            ledger: e.ledger,
            result,
        });
    }
    Ok(out)
}

/// The newest checkpoint strictly before `before`.
pub fn bootstrap_checkpoint(checkpoints: &[u32], before: u32) -> Option<u32> {
    checkpoints.iter().copied().filter(|c| *c < before).max()
}

type Records = HashMap<String, TxRecord>;

/// Talks to the adapter from `describe` to `stop`. Returns where the target stopped, if it
/// did, and the exported records of the entries' transactions.
fn drive_adapter(
    ctx: &mut Ctx<'_>,
    opts: &RunOptions,
    chain: &ChainFile,
    wanted: &HashSet<String>,
    adapter: &mut Adapter,
    describe_out: &mut Option<DescribeResult>,
) -> Result<(Option<Stop>, Records)> {
    let job = ctx.job;
    let t = &opts.timeouts;
    let describe: DescribeResult = adapter.call(
        Command::Describe {
            protocol: ADAPTER_PROTOCOL_VERSION,
        },
        t.describe,
    )?;
    if describe.protocol != ADAPTER_PROTOCOL_VERSION {
        bail!(
            "the adapter speaks protocol {}, the harness {ADAPTER_PROTOCOL_VERSION}",
            describe.protocol
        );
    }
    if describe.vocab_version != ORACLE_VOCAB_VERSION {
        bail!(
            "the adapter maps to vocabulary version {}, the oracle files are {ORACLE_VOCAB_VERSION}",
            describe.vocab_version
        );
    }
    for c in &describe.claims.changes {
        if !vocabulary_fields().contains_key(&c.kind) {
            bail!(
                "the adapter claims {:?}, which is not a vocabulary kind",
                c.kind
            );
        }
    }
    *describe_out = Some(describe.clone());
    if !(describe.protocols.min..=describe.protocols.max).contains(&chain.protocol) {
        adapter.call::<StopResult>(Command::Stop, t.stop)?;
        return Ok((None, Records::new()));
    }
    let mode = opts.delivery.unwrap_or(describe.default_delivery);
    if !describe.delivery_modes.contains(&mode) {
        bail!(
            "the adapter does not offer delivery mode {} (it offers {:?})",
            mode.name(),
            describe.delivery_modes
        );
    }
    ctx.result.delivery = Some(mode);

    let first_entry = job
        .entries
        .first()
        .map(|e| e.ledger)
        .ok_or_else(|| anyhow!("the chain has no entry to evaluate"))?;
    let bootstrap = match describe.bootstrap {
        Bootstrap::Checkpoint => {
            bootstrap_checkpoint(&chain.checkpoints, first_entry).ok_or_else(|| {
                anyhow!(
                    "the target bootstraps from a history checkpoint, and the chain has none before its first entry ledger {first_entry} (checkpoints {:?}); regenerate it with `shapekit gen chains`, which records ledgers through the first checkpoint before the recipe",
                    chain.checkpoints
                )
            })?
        }
        Bootstrap::Ledger => chain.ledger_range.first,
    };
    ctx.result.bootstrap = Some(bootstrap);
    let last = chain.ledger_range.last;

    let mut net = if opts.docker {
        let n = NETWORKS.fetch_add(1, Ordering::SeqCst);
        Some(dockernet::Network::create(&format!(
            "shapekit-harness-{}-{n}",
            std::process::id()
        ))?)
    } else {
        None
    };
    let host = net
        .as_ref()
        .map_or(IpAddr::V4(Ipv4Addr::LOCALHOST), |n| n.gateway);
    let server = shapekit_serve::start(ServeConfig {
        chain_dir: job.dir.clone(),
        listen: SocketAddr::new(host, 0),
        head: Some(bootstrap),
        cache_bytes: DEFAULT_CACHE_BYTES,
        log_requests: false,
    })
    .with_context(|| format!("serving {}", job.dir.display()))?;
    let base = server.url();
    let workdir = tempfile::Builder::new()
        .prefix("shapekit-harness-")
        .tempdir()
        .context("creating the adapter work directory")?;
    let lake_dir = job.dir.join(&chain.lake.dir);
    let lake_path = std::fs::canonicalize(&lake_dir)
        .with_context(|| format!("resolving {}", lake_dir.display()))?;

    let params = StartParams {
        chain: job.chain.clone(),
        network: job.network.clone(),
        network_passphrase: chain.passphrase.clone(),
        protocol: chain.protocol,
        delivery: Delivery {
            mode,
            rpc_url: format!("{base}/rpc"),
            archive_url: format!("{base}/archive"),
            lake: Lake {
                url: format!("{base}/ledgers"),
                path: lake_path.display().to_string(),
                ledgers_per_batch: chain.lake.ledgers_per_batch,
                batches_per_partition: chain.lake.batches_per_partition,
            },
        },
        ledgers: LedgerWindow {
            bootstrap,
            last,
            checkpoint_frequency: CHECKPOINT_FREQUENCY,
        },
        limits: Limits {
            memory_bytes: opts.memory_limit_bytes,
        },
        docker_network: net.as_ref().map(|n| n.name.clone()),
        workdir: workdir.path().display().to_string(),
    };
    let control = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("building the control client")?;

    let ingest = (|| -> Result<(Option<Stop>, Records)> {
        let started: StartResult = adapter.call(Command::Start(Box::new(params)), t.start)?;
        ctx.result.components = started.components;
        let mut stop = wait_for(ctx, adapter, bootstrap, t.bootstrap, opts)?;
        if stop.is_none() {
            for ledger in bootstrap + 1..=last {
                set_head(&control, &base, ledger)?;
                let a: AdvanceResult =
                    adapter.call(Command::Advance { head: ledger }, t.advance)?;
                if a.head != ledger {
                    bail!("advance to {ledger} answered head {}", a.head);
                }
                stop = wait_for(ctx, adapter, ledger, t.ledger, opts)?;
                if stop.is_some() {
                    break;
                }
            }
        }
        let mut records = Records::new();
        if let Some(through) = ctx.result.last_ingested.filter(|l| *l > bootstrap) {
            records = export(ctx, adapter, wanted, bootstrap + 1, through, t.export)?;
        }
        Ok((stop, records))
    })();

    // always try to stop the target, and keep its logs, whatever happened above
    let stopped = adapter.call::<StopResult>(Command::Stop, t.stop);
    let logs_path = opts.logs.join(log_name(job, "target"));
    let stop_error = match stopped {
        Ok(s) => std::fs::write(&logs_path, s.logs)
            .with_context(|| format!("writing {}", logs_path.display()))
            .err(),
        Err(e) => Some(anyhow!(e).context("stopping the target")),
    };
    let server_stopped = server.stop();
    let net_removed = net.as_mut().map(dockernet::Network::remove).transpose();
    let workdir_removed = workdir.close();

    let out = ingest?;
    if let Some(e) = stop_error {
        return Err(e);
    }
    server_stopped.context("stopping the replay server")?;
    net_removed.context("removing the docker network")?;
    workdir_removed.context("removing the adapter work directory")?;
    Ok(out)
}

fn set_head(client: &reqwest::blocking::Client, base: &str, ledger: u32) -> Result<()> {
    let resp = client
        .post(format!("{base}/control/head"))
        .header("content-type", "application/json")
        .body(format!("{{\"ledger\": {ledger}}}"))
        .send()
        .with_context(|| format!("moving the replay head to {ledger}"))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        bail!("moving the replay head to {ledger}: {status} {body}");
    }
    Ok(())
}

/// Polls `status` until the target has ingested `ledger`, died, or made no progress for
/// `patience`. Returns where it stopped, if it did.
fn wait_for(
    ctx: &mut Ctx<'_>,
    adapter: &mut Adapter,
    ledger: u32,
    patience: Duration,
    opts: &RunOptions,
) -> Result<Option<Stop>> {
    let t = &opts.timeouts;
    let mut last_progress = Instant::now();
    loop {
        let st: StatusResult = adapter.call(Command::Status, t.status)?;
        if let Some(p) = st.peak_memory_bytes {
            let peak = ctx.result.target_peak_memory_bytes.get_or_insert(0);
            *peak = (*peak).max(p);
        }
        if st.last_ingested_ledger > ctx.result.last_ingested {
            ctx.result.last_ingested = st.last_ingested_ledger;
            last_progress = Instant::now();
        }
        if ctx.result.last_ingested.is_some_and(|l| l >= ledger) {
            return Ok(None);
        }
        let stuck_at = ctx
            .result
            .last_ingested
            .map_or(ledger, |l| (l + 1).min(ledger));
        let error = st.last_error.clone();
        let stop = |reason, message: String| {
            Some(Stop {
                ledger: stuck_at,
                reason,
                message,
            })
        };
        match st.health {
            Health::Crashed => {
                return Ok(stop(
                    Reason::Crashed,
                    error.unwrap_or_else(|| "the ingest process exited".into()),
                ));
            }
            Health::Oom => {
                return Ok(stop(
                    Reason::Oom,
                    error.unwrap_or_else(|| {
                        format!(
                            "killed at its memory limit of {} bytes",
                            opts.memory_limit_bytes
                        )
                    }),
                ));
            }
            Health::Starting | Health::Running => {}
        }
        if last_progress.elapsed() >= patience {
            return Ok(match error {
                Some(e) => stop(
                    Reason::Halted,
                    format!("no progress for {patience:?}; last error: {e}"),
                ),
                None => stop(
                    Reason::Timeout,
                    format!("no progress for {patience:?} and no error reported"),
                ),
            });
        }
        std::thread::sleep(t.poll);
    }
}

fn export(
    ctx: &mut Ctx<'_>,
    adapter: &mut Adapter,
    wanted: &HashSet<String>,
    from: u32,
    to: u32,
    timeout: Duration,
) -> Result<Records> {
    let mut records = Records::new();
    let mut notes = Vec::new();
    let result = &mut ctx.result;
    let seen = adapter.export(from, to, timeout, &mut |rec| match rec {
        ExportRecord::Transaction(tx) => {
            if tx.ledger < from || tx.ledger > to {
                notes.push(format!(
                    "export of {from} to {to} returned transaction {} of ledger {}",
                    tx.hash, tx.ledger
                ));
            }
            if wanted.contains(&tx.hash) {
                if records.contains_key(&tx.hash) {
                    notes.push(format!("transaction {} was exported twice", tx.hash));
                }
                records.insert(tx.hash.clone(), tx);
            }
        }
        ExportRecord::Unattributed { unmapped, .. } => {
            result.unattributed_rows += unmapped.len() as u64;
            let room = 50usize.saturating_sub(result.unattributed_sample.len());
            result
                .unattributed_sample
                .extend(unmapped.into_iter().take(room));
        }
    })?;
    ctx.result.exported_records = seen;
    ctx.result.notes.extend(notes);
    Ok(records)
}

/// Every verdict of a round, for comparing repeated rounds.
pub fn verdict_digest(entries: &[EntryRow]) -> Vec<(String, String, Verdict, Option<Reason>)> {
    entries
        .iter()
        .map(|e| {
            (
                e.entry.clone(),
                e.network.clone(),
                e.result.verdict,
                e.result.reason,
            )
        })
        .collect()
}

/// Path of a log written by a round.
pub fn logs_dir(out: &Path, round: u32, rounds: u32) -> PathBuf {
    if rounds > 1 {
        out.join("logs").join(format!("round-{round}"))
    } else {
        out.join("logs")
    }
}
