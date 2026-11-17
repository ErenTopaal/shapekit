//! `shapekit audit`: the law book over the public SEP-54 ledger lake, on a random sample of
//! the whole history or on every ledger since the last run.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use shapekit_contracts::AuditReport;
use shapekit_sentinel::sdk::SdkAudit;
use shapekit_sentinel::{Audit, Lake, Network, Tip};

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum NetworkArg {
    Pubnet,
    Testnet,
}

impl NetworkArg {
    fn network(self) -> Network {
        match self {
            NetworkArg::Pubnet => Network::Pubnet,
            NetworkArg::Testnet => Network::Testnet,
        }
    }
}

#[derive(clap::Args)]
pub struct LakeArgs {
    /// The network whose ledgers are checked.
    #[arg(long, value_enum)]
    network: NetworkArg,
    /// Read a SEP-54 lake from this directory instead of the public one.
    #[arg(long)]
    lake: Option<PathBuf>,
    /// Lake objects fetched at the same time.
    #[arg(long, default_value_t = 8)]
    jobs: usize,
}

#[derive(clap::Args)]
pub struct SdkArgs {
    /// Send every ledger through this SDK's testee (`rust`, `go` or `js`) instead of the
    /// in-process law book: `decode_encode` on the ledger and `tx_hash`, `balance_id` and
    /// `contract_id` on what it holds, each compared with the network's answer.
    #[arg(long, requires = "version")]
    sdk: Option<String>,
    /// The SDK version, as conformance/versions.toml lists it, or `current` for the version
    /// that file marks current.
    #[arg(long, requires = "sdk")]
    version: Option<String>,
    /// The versions file naming each testee's build and command.
    #[arg(long, default_value = "conformance/versions.toml")]
    versions: PathBuf,
    /// Use the testee as already built instead of running its build steps first.
    #[arg(long)]
    no_build: bool,
    /// Measure which SDK code the ledgers reach beyond what the committed corpus reaches.
    /// The SDK's coverage variant runs every corpus check first, its coverage at that point
    /// is the baseline, and every range a ledger reaches beyond it is reported with the first
    /// ledger that reached it.
    #[arg(long, value_enum, requires = "sdk")]
    coverage_baseline: Option<BaselineArg>,
    /// The manifests the baseline is measured over; repeat it to add the candidate lane
    /// (`candidate/manifest.json`) to the committed corpus.
    #[arg(long, default_value = "corpus/manifest.json")]
    manifest: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum BaselineArg {
    /// The committed corpus.
    Corpus,
}

#[derive(clap::Subcommand)]
pub enum AuditCommand {
    /// Check every law on ledgers drawn at random from the whole history. The same seed and
    /// bound always draw the same ledgers. Exits 1 when a law fails.
    Run {
        #[command(flatten)]
        lake: LakeArgs,
        /// How many distinct ledgers to draw.
        #[arg(long)]
        n: u32,
        /// The seed the ledgers are drawn with.
        #[arg(long)]
        seed: u32,
        /// Draw ledgers below this one (default: one past the newest ledger in the lake).
        #[arg(long)]
        below: Option<u32>,
        #[command(flatten)]
        sdk: SdkArgs,
        /// Where to write the audit report.
        #[arg(long)]
        out: PathBuf,
    },
    /// Check every law on every ledger after the cursor up to the newest one in the lake,
    /// then move the cursor to the last ledger checked. Consecutive runs leave no gap and
    /// check no ledger twice; the header chain is checked across runs too. Exits 1 when a
    /// law fails.
    Watch {
        #[command(flatten)]
        lake: LakeArgs,
        /// The cursor file: the last ledger checked and its hash. Read when it exists,
        /// written after every run that checked a ledger.
        #[arg(long)]
        cursor: PathBuf,
        /// Where to start when the cursor file does not exist: a ledger number, or `newest`
        /// for the newest ledger in the lake. Without it a missing cursor is an error, so a
        /// lost cursor never skips ledgers silently.
        #[arg(long)]
        start: Option<String>,
        /// Check at most this many ledgers, oldest first; the rest is left for the next run, so
        /// a cursor far behind the newest ledger catches up over several bounded runs.
        #[arg(long, default_value_t = 1000)]
        max_ledgers: u32,
        /// Where to write the audit report. Nothing is written when no ledger closed since
        /// the cursor.
        #[arg(long)]
        out: PathBuf,
    },
    /// Print the newest ledger in the lake. A workflow that runs several `audit run` jobs
    /// passes one past it to each as `--below`, so every job draws the same ledgers.
    Newest {
        #[command(flatten)]
        lake: LakeArgs,
    },
    /// Print an audit report as Markdown, for a CI job summary.
    Summary {
        #[arg(long)]
        report: PathBuf,
    },
}

/// The watch cursor as stored between runs.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    network: String,
    ledger: u32,
    /// The header hash of `ledger`, hex.
    hash: String,
}

fn open(args: &LakeArgs) -> Result<Lake> {
    let network = args.network.network();
    match &args.lake {
        Some(dir) => Lake::dir(dir.clone(), network),
        None => Lake::public(network),
    }
}

pub fn run(command: AuditCommand) -> Result<bool> {
    match command {
        AuditCommand::Run {
            lake,
            n,
            seed,
            below,
            sdk,
            out,
        } => {
            ensure!(n > 0, "--n has to be at least 1");
            let started = Instant::now();
            let l = open(&lake)?;
            let below = match below {
                Some(b) => b,
                None => l
                    .newest()?
                    .checked_add(1)
                    .context("the lake ends at the last possible ledger")?,
            };
            println!(
                "drawing {n} ledgers below {below} with seed {seed} from {}",
                l.location()
            );
            let testee = match (&sdk.sdk, &sdk.version) {
                (Some(name), Some(version)) => {
                    let root = std::env::current_dir().context("reading the current directory")?;
                    let target = shapekit_sentinel::sdk::start(
                        &root,
                        &sdk.versions,
                        name,
                        version,
                        sdk.coverage_baseline.is_some(),
                        !sdk.no_build,
                    )?;
                    let mut t = SdkAudit::new(&root, &target, l.network.passphrase())?;
                    println!("checking through the {} testee", target.key());
                    if let Some(BaselineArg::Corpus) = sdk.coverage_baseline {
                        let sent = t.corpus_baseline(&sdk.manifest)?;
                        let c = t.coverage_delta().context("the baseline was not taken")?;
                        println!(
                            "baseline {}: {} checks reach {} ranges",
                            c.baseline, sent, c.baseline_ranges
                        );
                    }
                    Some(t)
                }
                _ => None,
            };
            let audit = shapekit_sentinel::random_audit(&l, n, seed, below, lake.jobs, testee)?;
            finish(&audit, &l, Some(u64::from(seed)), &out, started)
        }
        AuditCommand::Watch {
            lake,
            cursor,
            start,
            max_ledgers,
            out,
        } => {
            let started = Instant::now();
            let l = open(&lake)?;
            let network = l.network;
            let tip = read_cursor(&cursor, network)?;
            let newest = l.newest()?;
            let start = match (&tip, start.as_deref()) {
                (Some(t), Some(s)) => bail!(
                    "{} holds a cursor at ledger {}; --start {s} would move it, so remove one of the two",
                    cursor.display(),
                    t.ledger
                ),
                (Some(_), None) => None,
                (None, Some("newest")) => Some(newest),
                (None, Some(s)) => Some(
                    s.parse::<u32>()
                        .with_context(|| format!("--start {s} is neither a ledger nor newest"))?,
                ),
                (None, None) => bail!(
                    "{} does not exist. The sentinel does not guess where it stopped: restore the cursor, or give --start with the first ledger to check (--start newest skips everything before the newest ledger)",
                    cursor.display()
                ),
            };
            let Some(window) = shapekit_sentinel::window(tip.as_ref(), start, newest, max_ledgers)?
            else {
                println!(
                    "no ledger closed after {} (newest in the lake: {newest})",
                    tip.as_ref().map(|t| t.ledger).unwrap_or(newest)
                );
                return Ok(true);
            };
            match &tip {
                Some(t) => println!(
                    "cursor at {}, checking {} to {} of {} in {}",
                    t.ledger,
                    window.first,
                    window.last,
                    newest,
                    l.location()
                ),
                None => println!(
                    "no cursor, checking {} to {} of {} in {}",
                    window.first,
                    window.last,
                    newest,
                    l.location()
                ),
            }
            // ledgers that closed after the cursor and before this run's own share: a backlog
            // left by runs that did not happen or did not finish, filled oldest first
            let behind = newest - window.first + 1;
            let audit = shapekit_sentinel::watch(&l, tip, window, lake.jobs)?;
            let held = finish(&audit, &l, None, &out, started)?;
            let last = audit.tip().context("the window checked no ledger")?.clone();
            let left = newest - last.ledger;
            println!(
                "backlog: {behind} ledgers were behind the newest ({newest}); this run checked {}, {left} are left for the next run",
                audit.ledgers().len()
            );
            write_atomic(
                &cursor,
                &serde_json::to_vec_pretty(&Cursor {
                    network: network.name().to_string(),
                    ledger: last.ledger,
                    hash: last.hash.clone(),
                })?,
            )?;
            println!("cursor moved to {}", last.ledger);
            Ok(held)
        }
        AuditCommand::Newest { lake } => {
            println!("{}", open(&lake)?.newest()?);
            Ok(true)
        }
        AuditCommand::Summary { report } => {
            let r: AuditReport = shapekit_contracts::load(&report)?;
            print!("{}", shapekit_sentinel::summary::markdown(&r));
            Ok(true)
        }
    }
}

fn read_cursor(path: &Path, network: Network) -> Result<Option<Tip>> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let c: Cursor =
        serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))?;
    ensure!(
        c.network == network.name(),
        "{} is a cursor for {}, not {}",
        path.display(),
        c.network,
        network.name()
    );
    ensure!(
        c.hash.len() == 64
            && c.hash
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
        "{}: the hash is not 32 bytes of lowercase hex",
        path.display()
    );
    Ok(Some(Tip {
        ledger: c.ledger,
        hash: c.hash,
    }))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("renaming to {}", path.display()))
}

fn finish(
    audit: &Audit,
    lake: &Lake,
    seed: Option<u64>,
    out: &Path,
    started: Instant,
) -> Result<bool> {
    let report = audit.report(seed);
    if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    shapekit_contracts::write(out, &report)?;
    let r = audit.laws();
    println!(
        "{} ledgers, {} protocol versions, {} transactions",
        report.ledgers.len(),
        report.protocols.len(),
        r.transactions
    );
    println!(
        "{} bytes fetched, {} bytes decompressed, {} retried requests, {:.1} s",
        audit.compressed_bytes,
        audit.raw_bytes,
        lake.retries(),
        started.elapsed().as_secs_f64()
    );
    match &report.envelopes {
        Some(e) => println!(
            "envelopes: {} v0, {} v1, {} fee bump",
            e.v0, e.v1, e.fee_bump
        ),
        None => println!(
            "envelopes in the tx sets ({}) do not add up to the applied transactions",
            r.envelopes.values().sum::<u64>()
        ),
    }
    for l in &report.laws {
        println!(
            "  {:<22} {:>10} checked {:>6} violated",
            l.law, l.checked, l.violations
        );
    }
    if audit.sdk().is_none() {
        for (law, n) in audit.undecided() {
            println!("  {:<22} {n:>10} undecided", law.as_str());
        }
    }
    let mut errors = 0;
    if let Some(sdk) = audit.sdk() {
        println!(
            "through {} {}, testee started {} times",
            sdk.sdk_ref().sdk,
            sdk.sdk_ref().version,
            sdk.starts()
        );
        for (op, n) in &sdk.unsupported {
            println!("  {op:<22} {n:>10} unsupported");
        }
        errors = sdk.errors.len();
        for (ledger, e) in sdk.errors.iter().take(20) {
            println!("  error, ledger {ledger}: {e}");
        }
        if errors > 0 {
            println!("{errors} checks ended in an error; none is counted as a pass");
        }
        if let Some(c) = &report.coverage {
            println!(
                "coverage: baseline {} ranges, with the ledgers {}, {} new",
                c.baseline_ranges,
                c.ranges,
                c.new.len()
            );
            for b in &c.new {
                println!("  new {} first reached by ledger {}", b.location, b.ledger);
            }
        }
    }
    let total = report.total_violations();
    println!("{total} violations; report written to {}", out.display());
    Ok(total == 0 && errors == 0)
}
