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

use shapekit_process::error_text::chain as error_chain;

use crate::adapter::Adapter;
use crate::compare::{
    EntryVerdict, Reason, Stop, Verdict, compare_tx, stopped_verdict, verdict_of,
};
use crate::corpus::{ChainJob, EntryRef};
use crate::dockernet;
use crate::dump::{Dump, DumpSummary};
use crate::protocol::{
    AdvanceResult, Bootstrap, Capability, Command, Delivery, DeliveryMode, DescribeResult,
    ExportRecord, Health, Lake, LedgerWindow, Limits, RestartResult, StartParams, StartResult,
    StatusResult, StopResult, TxRecord, Unmapped, vocabulary_fields,
};
use crate::relations::carry::{self, CarryReport};
use crate::relations::known::Known;

/// Ledgers per history checkpoint: core's default, which every chain is recorded with.
pub const CHECKPOINT_FREQUENCY: u32 = 64;

#[derive(Debug, Clone)]
pub struct Timeouts {
    pub describe: Duration,
    pub start: Duration,
    pub advance: Duration,
    pub status: Duration,
    pub export: Duration,
    pub dump: Duration,
    pub stop: Duration,
    pub restart: Duration,
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
            // a status answer reads the target's store and its container state; on a host
            // with a load average near 200 the wallet-backend adapter took over 60 s
            status: Duration::from_secs(300),
            export: Duration::from_secs(10 * 60),
            dump: Duration::from_secs(10 * 60),
            restart: Duration::from_secs(5 * 60),
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
    /// Where adapter and target logs go, and the dumps.
    pub logs: PathBuf,
    /// Ask an adapter that offers `dump` for what the target stored, after the export, and
    /// keep it as `<logs>/<chain>.<network>.dump.json`.
    pub dump: bool,
    /// Restart the target's ingesting process once, after it ingested the ledger halfway
    /// between the bootstrap ledger and the chain's last. Needs the `restart` capability.
    pub restart: bool,
}

/// What happened to one chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChainResult {
    pub chain: String,
    pub network: String,
    /// The chain directory, when it is not `<corpus>/chains/<chain>/<network>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<PathBuf>,
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
    /// The ledger after which the target's ingesting process was restarted (`--restart`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restarted_after: Option<u32>,
    /// The target's stored tables, when the adapter offers `dump` and the target ingested a
    /// ledger after the bootstrap ledger.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dump: Option<DumpSummary>,
    /// The carry check over the dump (see `relations::carry`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carry: Option<CarryReport>,
    /// Why the dump could not be taken or checked. The verdicts stand; the run is not ok.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dump_error: Option<String>,
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
            dir: job.external.then(|| job.dir.clone()),
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
            restarted_after: None,
            dump: None,
            carry: None,
            dump_error: None,
            notes: Vec::new(),
            adapter_exit: String::new(),
            seconds: 0.0,
        },
    };
    let mut describe = None;
    let entries = match drive(&mut ctx, opts, vocab, &mut describe) {
        Ok(entries) => entries,
        Err(e) => {
            let msg = error_chain(e.as_ref());
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
        if let Some(want) = &e.oracle_sha256 {
            let got = shapekit_manifest::sha256_hex(&bytes);
            if &got != want {
                bail!(
                    "{} has sha256 {got}, the manifest records {want}; run `shapekit oracle run` and `shapekit manifest build`",
                    e.oracle_file.display()
                );
            }
        }
        let file: OracleFile = serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing {}", e.oracle_file.display()))?;
        if file.oracle_vocab_version != ORACLE_VOCAB_VERSION {
            bail!(
                "{} is vocabulary version {}, the harness speaks {ORACLE_VOCAB_VERSION}; run `shapekit oracle run`",
                e.oracle_file.display(),
                file.oracle_vocab_version
            );
        }
        if let Some(tx) = file
            .ledgers
            .iter()
            .flat_map(|l| &l.transactions)
            .find(|t| t.events.is_none())
        {
            bail!(
                "{} does not say which events the meta of {} can carry; run `shapekit oracle run`",
                e.oracle_file.display(),
                tx.hash
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
    ctx.result.notes.extend(adapter.take_late_answers());
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

/// Checkpoints that do not fall on [`CHECKPOINT_FREQUENCY`] boundaries. The target is told
/// that frequency, so a chain recorded with another one would be read at the wrong ledgers.
pub fn off_frequency_checkpoints(checkpoints: &[u32]) -> Vec<u32> {
    checkpoints
        .iter()
        .copied()
        .filter(|c| (u64::from(*c) + 1) % u64::from(CHECKPOINT_FREQUENCY) != 0)
        .collect()
}

/// Whether `hash` is a transaction hash as the protocol writes it: 64 lowercase hex digits.
pub fn is_protocol_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
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
    let describe = adapter.describe(t.describe)?;
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
    if opts.restart && !describe.offers(Capability::Restart) {
        bail!("--restart needs an adapter that offers the restart capability");
    }

    // a chain run for its dump alone (no entries) starts as early as the target can: after
    // its first checkpoint
    let first_entry = match job.entries.first() {
        Some(e) => e.ledger,
        None => chain
            .checkpoints
            .iter()
            .min()
            .map_or(chain.ledger_range.last, |c| c + 1),
    };
    let off = off_frequency_checkpoints(&chain.checkpoints);
    if !off.is_empty() {
        bail!(
            "chain checkpoints {off:?} are not on the {CHECKPOINT_FREQUENCY}-ledger boundaries the target is told to expect"
        );
    }
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
    let workdir = shapekit_process::scratch::scratch_dir("shapekit-harness-")
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
        let restart_after = opts
            .restart
            .then(|| bootstrap + last.saturating_sub(bootstrap) / 2);
        if stop.is_none() {
            if restart_after == Some(bootstrap) {
                restart(ctx, adapter, bootstrap, t.restart)?;
            }
            for ledger in bootstrap + 1..=last {
                set_head(&control, &base, ledger)?;
                check_served(&control, &base, ledger)?;
                let a: AdvanceResult =
                    adapter.call(Command::Advance { head: ledger }, t.advance)?;
                if a.head != ledger {
                    bail!("advance to {ledger} answered head {}", a.head);
                }
                stop = wait_for(ctx, adapter, ledger, t.ledger, opts)?;
                if stop.is_some() {
                    break;
                }
                if restart_after == Some(ledger) {
                    restart(ctx, adapter, ledger, t.restart)?;
                }
            }
        }
        let mut records = Records::new();
        if let Some(through) = ctx.result.last_ingested.filter(|l| *l > bootstrap) {
            records = export(ctx, adapter, wanted, bootstrap + 1, through, t.export)?;
            if job.external {
                let path = opts
                    .logs
                    .join(format!("{}.{}.export.json", job.chain, job.network));
                let mut sorted: Vec<&TxRecord> = records.values().collect();
                sorted.sort_by(|a, b| (a.ledger, &a.hash).cmp(&(b.ledger, &b.hash)));
                let mut text = serde_json::to_string(&sorted).context("rendering the export")?;
                text.push('\n');
                std::fs::write(&path, text)
                    .with_context(|| format!("writing {}", path.display()))?;
            }
            if opts.dump && describe.offers(Capability::Dump) {
                take_dump(ctx, adapter, opts, chain, through);
            }
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

    // a cleanup failure is reported even when ingestion already failed
    let mut cleanup = Vec::new();
    if let Some(e) = stop_error {
        cleanup.push(error_chain(e.as_ref()));
    }
    if let Err(e) = server_stopped {
        cleanup.push(format!("stopping the replay server: {e}"));
    }
    if let Err(e) = net_removed {
        cleanup.push(format!(
            "removing the docker network: {}",
            error_chain(e.as_ref())
        ));
    }
    if let Err(e) = workdir_removed {
        cleanup.push(format!("removing the adapter work directory: {e}"));
    }
    match (ingest, cleanup.is_empty()) {
        (Ok(out), true) => Ok(out),
        (Ok(_), false) => bail!("{}", cleanup.join("; ")),
        (Err(e), true) => Err(e),
        (Err(e), false) => Err(e.context(format!("and then: {}", cleanup.join("; ")))),
    }
}

/// Takes and writes the dump. A failure is kept on the chain, not raised: the verdicts do
/// not depend on the dump, and the report is not ok while a dump is missing.
fn take_dump(
    ctx: &mut Ctx<'_>,
    adapter: &mut Adapter,
    opts: &RunOptions,
    chain: &ChainFile,
    through: u32,
) {
    let name = format!("{}.{}.dump.json", ctx.job.chain, ctx.job.network);
    let path = opts.logs.join(&name);
    let taken = Dump::take(adapter, Vec::new(), opts.timeouts.dump)
        .and_then(|d| d.write(&path).map(|()| d));
    match taken {
        Ok(d) => {
            ctx.result.dump = Some(DumpSummary {
                file: name,
                tables: d.tables.len() as u64,
                rows: d.rows(),
            });
            match Known::from_chain(&ctx.job.dir, &chain.passphrase, through) {
                Ok(known) => ctx.result.carry = Some(carry::check(&d, &known)),
                Err(e) => {
                    ctx.result.dump_error = Some(format!(
                        "reading the chain's values: {}",
                        error_chain(e.as_ref())
                    ))
                }
            }
        }
        Err(e) => ctx.result.dump_error = Some(error_chain(e.as_ref())),
    }
}

fn restart(ctx: &mut Ctx<'_>, adapter: &mut Adapter, after: u32, timeout: Duration) -> Result<()> {
    adapter.call::<RestartResult>(Command::Restart, timeout)?;
    ctx.result.restarted_after = Some(after);
    Ok(())
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

/// Asks the replay server for the ledger the target is about to read, so a ledger the
/// server cannot serve is a harness error and never counted against the target.
fn check_served(client: &reqwest::blocking::Client, base: &str, ledger: u32) -> Result<()> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getLedgers",
        "params": {"startLedger": ledger, "pagination": {"limit": 1}},
    });
    let resp = client
        .post(format!("{base}/rpc"))
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .with_context(|| format!("asking the replay server for ledger {ledger}"))?;
    let status = resp.status();
    let text = resp.text().with_context(|| {
        format!("reading the replay server's answer for ledger {ledger} (http {status})")
    })?;
    let v: serde_json::Value = serde_json::from_str(&text).with_context(|| {
        let shown: String = text.chars().take(300).collect();
        format!("the replay server's answer for ledger {ledger} (http {status}): {shown}")
    })?;
    if let Some(e) = v.get("error") {
        bail!("the replay server cannot serve ledger {ledger}: {e}");
    }
    let got = v
        .pointer("/result/ledgers/0/sequence")
        .and_then(|s| s.as_u64());
    if got != Some(u64::from(ledger)) {
        bail!("the replay server answered getLedgers for {ledger} with {v}");
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
            // the adapter is meant to enforce the limit; a peak past it means it did not,
            // and the target is judged as if it had been killed there
            if p > opts.memory_limit_bytes {
                let at = ctx
                    .result
                    .last_ingested
                    .map_or(ledger, |l| (l + 1).min(ledger));
                return Ok(Some(Stop {
                    ledger: at,
                    reason: Reason::Oom,
                    message: format!(
                        "peak memory {p} bytes exceeds the limit of {} bytes",
                        opts.memory_limit_bytes
                    ),
                }));
            }
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
    let ctx_external = ctx.job.external;
    let mut records = Records::new();
    let mut notes = Vec::new();
    let mut bad_hashes = Vec::new();
    let result = &mut ctx.result;
    let seen = adapter.export(from, to, timeout, &mut |rec| match rec {
        ExportRecord::Transaction(tx) => {
            // an uppercase or short hash would match nothing and read as a dropped
            // transaction, blaming the target's data for a protocol mistake
            if !is_protocol_hash(&tx.hash) {
                bad_hashes.push(tx.hash.clone());
                return;
            }
            if tx.ledger < from || tx.ledger > to {
                notes.push(format!(
                    "export of {from} to {to} returned transaction {} of ledger {}",
                    tx.hash, tx.ledger
                ));
            }
            // a chain run for its dump alone keeps every record: there is no oracle to pick by
            if wanted.contains(&tx.hash) || ctx_external {
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
    if !bad_hashes.is_empty() {
        let shown: Vec<&str> = bad_hashes.iter().take(3).map(String::as_str).collect();
        bail!(
            "protocol violation answering export: {} record(s) with a hash that is not 64 lowercase hex digits, for example {shown:?}",
            bad_hashes.len()
        );
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoints_must_sit_on_the_frequency() {
        assert!(off_frequency_checkpoints(&[63, 127, 191]).is_empty());
        assert_eq!(off_frequency_checkpoints(&[63, 100, 127, 31]), [100, 31]);
    }

    #[test]
    fn only_lowercase_full_hashes_are_protocol_hashes() {
        let h = "ab".repeat(32);
        assert!(is_protocol_hash(&h));
        assert!(!is_protocol_hash(&h.to_uppercase()));
        assert!(!is_protocol_hash(&h[..62]));
        assert!(!is_protocol_hash(&format!("{}zz", &h[..62])));
    }
}
