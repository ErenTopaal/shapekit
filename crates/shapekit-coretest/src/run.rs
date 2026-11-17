//! Runs core's `[tx]` tests in the pinned test image, each test case in its own container, and
//! summarises the dumps.
//!
//! One container per test case, never one process for many: core's memory grows across test
//! cases in one process (`scripts/check-lcm.sh` saw the oom killer at about 6 GB), and a
//! separate container gives each case its own network namespace for the ports tests open.
//! At most `shards` containers run at once, each pulling the next job from a shared queue, so
//! a long case does not hold back the cases queued behind it.
//!
//! A case named in [`Options::split`] runs as one job per protocol version (`--version N`
//! instead of `--all-versions`), in its own container each. One process for all 31 versions of
//! `pathpayment` outgrows 2 GB; one version at a time does not, and the jobs spread over the
//! shards. core reseeds its random generators only when a test case starts, so a single-version
//! run could in principle diverge from the all-versions run; the short hashes in core's baseline
//! decide, and `coretest convert` checks every one. A split case may only record metas inside
//! core's version wrapper: a part holding a meta of any other version is an error, since that
//! meta would be recorded once per part.
//!
//! Output layout under `out`:
//!
//! ```text
//! names.txt               the test cases, one per line, in run order
//! dump/NNN.txt            --debug-test-tx-meta of case NNN (a split case's parts, in version
//!                         order)
//! baseline/NNN/*.json     --record-test-tx-meta of case NNN (core's baseline format),
//!                         baseline/NNN/vVV/*.json for each part of a split case
//! log/NNN.log             stdout and stderr of the container
//! run.json                the report
//! ```

use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use crate::dump::Reader;
use crate::pins::Image;
use crate::{Error, RNG_SEED, Result, TX_TAG};

/// The newest ledger protocol an image runs (`Config::CURRENT_LEDGER_PROTOCOL_VERSION`), which
/// `--all-versions` runs up to: the first `ledger protocol version: N` line of `stellar-core
/// version` (`main/StellarCoreVersion` and `runVersion` in stellar-core, printed before the
/// per-host lines of the soroban hosts). A vnext build answers the next protocol.
pub fn max_protocol(image: &Image) -> Result<u32> {
    let out = Command::new("docker")
        .args([
            "run",
            "--rm",
            "--network",
            "none",
            "--memory",
            "512m",
            "--cpus",
            "1",
        ])
        .arg(image.reference())
        .arg("version")
        .stderr(Stdio::null())
        .output()
        .map_err(|e| Error::Docker(format!("starting docker: {e}")))?;
    if !out.status.success() {
        return Err(Error::Docker(format!("version exited with {}", out.status)));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    parse_max_protocol(&text)
        .ok_or_else(|| Error::Docker(format!("version printed no ledger protocol: {text}")))
}

fn parse_max_protocol(text: &str) -> Option<u32> {
    text.lines()
        .find_map(|l| l.strip_prefix("ledger protocol version: "))
        .and_then(|v| v.trim().parse().ok())
}

/// Cases split by default: `create offer`, `pathpayment` and `revoke from pool` are killed by
/// the kernel at 1.5 GB as one process (`pathpayment` also at 2 GB), and `txresults` and
/// `archival meta` are the longest cases.
pub const SPLIT_BY_DEFAULT: &[&str] = &[
    "create offer",
    "pathpayment",
    "revoke from pool",
    "txresults",
    "archival meta",
];

/// Limits for one test container.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Limits {
    /// docker `--cpus`.
    pub cpus: String,
    /// docker `--memory`, also used as `--memory-swap` so a case cannot swap.
    pub memory: String,
    /// A run the kernel killed for memory runs once more with this much; `None` keeps the kill
    /// as the run's result. `pathpayment` and `revoke from pool` are killed at 1536m in some
    /// protocol runs of the vnext image and pass at 3g.
    pub retry_memory: Option<String>,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            cpus: "1".into(),
            memory: "1536m".into(),
            retry_memory: Some("3g".into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Options {
    pub image: Image,
    /// Containers running at once.
    pub shards: usize,
    pub limits: Limits,
    /// Output directory; must be absent or empty.
    pub out: PathBuf,
    /// Test cases to run; empty means every `[tx]` case the image lists.
    pub tests: Vec<String>,
    /// A report of an earlier run: cases start longest first by the seconds it recorded,
    /// cases it does not hold before all others. In list order when absent.
    pub order_from: Option<PathBuf>,
    /// Cases run as one job per protocol version.
    pub split: Vec<String>,
}

/// Sorts `names` longest first by the seconds `earlier` recorded; unknown names first.
pub fn order_by(names: &mut [String], earlier: &Report) {
    let seconds: BTreeMap<&str, f64> = earlier
        .tests
        .iter()
        .map(|t| (t.name.as_str(), t.seconds))
        .collect();
    let key = |n: &String| seconds.get(n.as_str()).copied().unwrap_or(f64::INFINITY);
    names.sort_by(|a, b| key(b).total_cmp(&key(a)));
}

/// What one test case produced.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TestRun {
    pub index: usize,
    pub name: String,
    /// Container exit code, `None` when docker itself failed.
    pub exit_code: Option<i32>,
    pub oom_killed: bool,
    pub seconds: f64,
    pub metas: u64,
    pub dump_bytes: u64,
    /// Metas per protocol version.
    pub protocols: BTreeMap<u32, u64>,
    /// Metas recorded outside the protocol version wrapper.
    pub unlabelled: u64,
    /// Metas per `TransactionMeta` arm.
    pub meta_versions: BTreeMap<u32, u64>,
    /// Why the case did not produce a usable dump, when it did not.
    pub error: Option<String>,
    /// Run as one job per protocol version.
    #[serde(default)]
    pub split: bool,
    /// The memory the run's last attempt had.
    #[serde(default)]
    pub memory: String,
    /// The kernel killed a first attempt for memory and the run was repeated at the retry
    /// limit; `oom_killed` says how the repeat ended. For a split case, how many parts.
    #[serde(default)]
    pub oom_retried: usize,
}

impl TestRun {
    pub fn passed(&self) -> bool {
        self.exit_code == Some(0) && self.error.is_none()
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Totals {
    pub tests: usize,
    pub passed: usize,
    pub tests_with_metas: usize,
    pub metas: u64,
    pub dump_bytes: u64,
    pub protocols: BTreeMap<u32, u64>,
    pub unlabelled: u64,
    pub meta_versions: BTreeMap<u32, u64>,
    pub wall_seconds: f64,
    pub test_seconds: f64,
    /// Runs whose last attempt the kernel killed for memory; each one fails the run.
    #[serde(default)]
    pub oom_killed: usize,
    /// Runs repeated at the retry limit after a memory kill (parts of split cases counted
    /// one by one).
    #[serde(default)]
    pub oom_retried: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Report {
    pub image: String,
    /// The newest protocol the image runs, the top of every `--all-versions` and split run.
    #[serde(default)]
    pub max_protocol: u32,
    pub rng_seed: u32,
    pub shards: usize,
    pub cpus: String,
    pub memory: String,
    #[serde(default)]
    pub retry_memory: Option<String>,
    pub totals: Totals,
    pub tests: Vec<TestRun>,
}

impl Report {
    pub fn passed(&self) -> bool {
        self.totals.passed == self.totals.tests
    }

    pub fn load(path: &Path) -> Result<Report> {
        let text = std::fs::read_to_string(path).map_err(|source| Error::Read {
            path: path.display().to_string(),
            source,
        })?;
        Ok(serde_json::from_str(&text)?)
    }
}

/// Characters a catch test spec carries literally (the same set core's `selftest-parallel`
/// accepts); a comma would split the spec, brackets would make it a tag.
pub fn check_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " -_():/".contains(c));
    if ok {
        Ok(())
    } else {
        Err(Error::TestName(name.to_owned()))
    }
}

/// The `[tx]` test cases the image holds.
pub fn list_tests(image: &Image) -> Result<Vec<String>> {
    let out = Command::new("docker")
        .args(["run", "--rm", "--network", "none"])
        .arg(image.reference())
        .args(["test", "--ll", "FATAL", "--list-test-names-only", TX_TAG])
        .stderr(Stdio::null())
        .output()
        .map_err(|e| Error::Docker(format!("starting docker: {e}")))?;
    if !out.status.success() {
        return Err(Error::Docker(format!(
            "listing tests exited with {}",
            out.status
        )));
    }
    let text = String::from_utf8(out.stdout)
        .map_err(|_| Error::Docker("test list is not utf-8".into()))?;
    let names: Vec<String> = text
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with("Warning: running non-release version"))
        .map(str::to_owned)
        .collect();
    if names.is_empty() {
        return Err(Error::Docker(format!("the image lists no {TX_TAG} tests")));
    }
    Ok(names)
}

fn create_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(|source| Error::Write {
        path: path.display().to_string(),
        source,
    })
}

fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes).map_err(|source| Error::Write {
        path: path.display().to_string(),
        source,
    })
}

/// Counts the records of one dump.
pub fn summarise_dump(path: &Path, run: &mut TestRun) -> Result<()> {
    let file = File::open(path).map_err(|source| Error::Read {
        path: path.display().to_string(),
        source,
    })?;
    run.dump_bytes = file
        .metadata()
        .map_err(|source| Error::Read {
            path: path.display().to_string(),
            source,
        })?
        .len();
    let reader = Reader::new(
        BufReader::with_capacity(1 << 20, file),
        path.display().to_string(),
    );
    for record in reader {
        let record = record?;
        run.metas += 1;
        match record.protocol() {
            Some(p) => *run.protocols.entry(p).or_default() += 1,
            None => run.unlabelled += 1,
        }
        let v = record.meta_version().ok_or_else(|| Error::Dump {
            path: path.display().to_string(),
            line: record.line,
            message: "record body does not start with a TransactionMeta arm".into(),
        })?;
        *run.meta_versions.entry(v).or_default() += 1;
    }
    Ok(())
}

/// A name no other container of this process has had: the state read back after the run must
/// be this job's own.
fn container_name(tag: &str) -> String {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::SeqCst);
    format!("shapekit-coretest-{}-{n}-{tag}", std::process::id())
}

/// One container: a whole case, or one protocol version of a split case.
#[derive(Debug, Clone, Copy)]
struct Job {
    case: usize,
    version: Option<u32>,
}

impl Job {
    fn tag(self) -> String {
        match self.version {
            None => format!("{:03}", self.case),
            Some(v) => format!("{:03}-v{v:02}", self.case),
        }
    }
}

/// One job, repeated once at the retry limit when the kernel killed it for memory.
fn run_job(opts: &Options, owner: &str, job: Job, name: &str) -> TestRun {
    let first = run_one(opts, owner, job, name, &opts.limits.memory);
    match &opts.limits.retry_memory {
        Some(more) if first.oom_killed => {
            if let Err(e) = clear_job(opts, job) {
                let mut r = first;
                r.error = Some(format!("clearing the killed attempt for a retry: {e}"));
                return r;
            }
            let mut again = run_one(opts, owner, job, name, more);
            again.oom_retried = 1;
            again.seconds += first.seconds;
            again
        }
        _ => first,
    }
}

/// Removes what an attempt of `job` left, so a retry starts from nothing.
fn clear_job(opts: &Options, job: Job) -> Result<()> {
    let tag = job.tag();
    let rm_dir = |p: PathBuf| -> Result<()> {
        if p.exists() {
            std::fs::remove_dir_all(&p).map_err(|source| Error::Write {
                path: p.display().to_string(),
                source,
            })?;
        }
        Ok(())
    };
    let rm_file = |p: PathBuf| -> Result<()> {
        if p.exists() {
            std::fs::remove_file(&p).map_err(|source| Error::Write {
                path: p.display().to_string(),
                source,
            })?;
        }
        Ok(())
    };
    rm_dir(opts.out.join("work").join(&tag))?;
    rm_dir(match job.version {
        None => opts.out.join("baseline").join(&tag),
        Some(v) => opts
            .out
            .join("baseline")
            .join(format!("{:03}", job.case))
            .join(format!("v{v:02}")),
    })?;
    rm_file(opts.out.join("dump").join(format!("{tag}.txt")))?;
    rm_file(opts.out.join("log").join(format!("{tag}.log")))
}

fn run_one(opts: &Options, owner: &str, job: Job, name: &str, memory: &str) -> TestRun {
    let index = job.case;
    let mut run = TestRun {
        index,
        name: match job.version {
            None => name.to_owned(),
            Some(v) => format!("{name} [protocol {v}]"),
        },
        split: job.version.is_some(),
        memory: memory.to_owned(),
        ..TestRun::default()
    };
    let tag = job.tag();
    let work = opts.out.join("work").join(&tag);
    let baseline = match job.version {
        None => opts.out.join("baseline").join(&tag),
        Some(v) => opts
            .out
            .join("baseline")
            .join(format!("{index:03}"))
            .join(format!("v{v:02}")),
    };
    let log = opts.out.join("log").join(format!("{tag}.log"));
    let dump = opts.out.join("dump").join(format!("{tag}.txt"));
    let started = Instant::now();
    let result = (|| -> Result<()> {
        create_dir(&work)?;
        create_dir(&baseline)?;
        let log_file = File::create(&log).map_err(|source| Error::Write {
            path: log.display().to_string(),
            source,
        })?;
        let log_err = log_file.try_clone().map_err(|source| Error::Write {
            path: log.display().to_string(),
            source,
        })?;
        let container = container_name(&tag);
        let mount = |host: &Path, inside: &str| format!("{}:{inside}", host.display());
        let status = Command::new("docker")
            .args(["run", "--name", &container, "--network", "none"])
            .args(["--cpus", &opts.limits.cpus])
            .args(["--memory", memory])
            .args(["--memory-swap", memory])
            .args(["--user", owner, "-e", "HOME=/work", "-w", "/work"])
            .args(["-v", &mount(&work, "/work")])
            .args(["-v", &mount(&baseline, "/baseline")])
            .args(["-v", &mount(&opts.out.join("dump"), "/dump")])
            .arg(opts.image.reference())
            .args(["test", name])
            .args(match job.version {
                None => vec!["--all-versions".to_owned()],
                Some(v) => vec!["--version".to_owned(), v.to_string()],
            })
            .args(["--rng-seed", &RNG_SEED.to_string()])
            .args(["--ll", "fatal", "-r", "simple", "--disable-dots"])
            .args(["--record-test-tx-meta", "/baseline"])
            .args(["--debug-test-tx-meta", &format!("/dump/{tag}.txt")])
            .stdout(log_file)
            .stderr(log_err)
            .status()
            .map_err(|e| Error::Docker(format!("starting docker: {e}")))?;
        let inspect = Command::new("docker")
            .args([
                "inspect",
                "-f",
                "{{.State.ExitCode}} {{.State.OOMKilled}}",
                &container,
            ])
            .output();
        let removed = Command::new("docker")
            .args(["rm", "-f", "-v", &container])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let inspect = inspect.map_err(|e| Error::Docker(format!("inspecting {container}: {e}")))?;
        let state = String::from_utf8_lossy(&inspect.stdout).trim().to_owned();
        let mut parts = state.split(' ');
        run.exit_code = parts.next().and_then(|c| c.parse().ok());
        run.oom_killed = parts.next() == Some("true");
        if run.exit_code.is_none() {
            return Err(Error::Docker(format!(
                "container {container} left no state (docker run {status})"
            )));
        }
        match removed {
            Ok(s) if s.success() => {}
            other => {
                return Err(Error::Docker(format!("removing {container}: {other:?}")));
            }
        }
        if run.exit_code != Some(0) {
            return Ok(());
        }
        summarise_dump(&dump, &mut run)?;
        if let Some(v) = job.version {
            let foreign = run.unlabelled
                + run
                    .protocols
                    .iter()
                    .filter(|(p, _)| **p != v)
                    .map(|(_, n)| n)
                    .sum::<u64>();
            if foreign > 0 {
                return Err(Error::Dump {
                    path: dump.display().to_string(),
                    line: 0,
                    message: format!(
                        "{foreign} metas of the protocol {v} run are outside that version; this case cannot be split"
                    ),
                });
            }
        }
        std::fs::remove_dir_all(&work).map_err(|source| Error::Write {
            path: work.display().to_string(),
            source,
        })
    })();
    run.seconds = started.elapsed().as_secs_f64();
    if let Err(e) = result {
        run.error = Some(e.to_string());
    }
    run
}

/// One case from the runs of its protocol versions: the parts' dumps concatenated in version
/// order into the case's dump, which is then summarised as a whole.
fn join_parts(out: &Path, case: usize, name: &str, parts: Vec<TestRun>, max: u32) -> TestRun {
    let mut run = TestRun {
        index: case,
        name: name.to_owned(),
        split: true,
        ..TestRun::default()
    };
    run.seconds = parts.iter().map(|p| p.seconds).sum();
    run.oom_killed = parts.iter().any(|p| p.oom_killed);
    run.oom_retried = parts.iter().map(|p| p.oom_retried).sum();
    let mut memories: Vec<&str> = parts.iter().map(|p| p.memory.as_str()).collect();
    memories.sort();
    memories.dedup();
    run.memory = memories.join(", ");
    let failed: Vec<&TestRun> = parts.iter().filter(|p| !p.passed()).collect();
    if let Some(first) = failed.first() {
        run.exit_code = first.exit_code;
        run.error = Some(format!(
            "{} of {} protocol runs failed, first {}: exit {:?}{}",
            failed.len(),
            parts.len(),
            first.name,
            first.exit_code,
            first
                .error
                .as_deref()
                .map(|e| format!(", {e}"))
                .unwrap_or_default()
        ));
        return run;
    }
    run.exit_code = Some(0);
    let joined = (|| -> Result<()> {
        let path = out.join("dump").join(format!("{case:03}.txt"));
        let mut target = File::create(&path).map_err(|source| Error::Write {
            path: path.display().to_string(),
            source,
        })?;
        for v in 0..=max {
            let part = out.join("dump").join(format!("{case:03}-v{v:02}.txt"));
            let mut src = File::open(&part).map_err(|source| Error::Read {
                path: part.display().to_string(),
                source,
            })?;
            std::io::copy(&mut src, &mut target).map_err(|source| Error::Write {
                path: path.display().to_string(),
                source,
            })?;
            std::fs::remove_file(&part).map_err(|source| Error::Write {
                path: part.display().to_string(),
                source,
            })?;
        }
        drop(target);
        summarise_dump(&path, &mut run)
    })();
    if let Err(e) = joined {
        run.error = Some(e.to_string());
    }
    run
}

/// Runs every requested case and writes `run.json`.
pub fn run(opts: &Options, progress: &(dyn Fn(&TestRun, usize, usize) + Sync)) -> Result<Report> {
    if opts.shards == 0 {
        return Err(Error::Docker("at least one shard is needed".into()));
    }
    if opts.out.exists() {
        let mut entries = std::fs::read_dir(&opts.out).map_err(|source| Error::Read {
            path: opts.out.display().to_string(),
            source,
        })?;
        if entries.next().is_some() {
            return Err(Error::Write {
                path: opts.out.display().to_string(),
                source: std::io::Error::other("output directory is not empty"),
            });
        }
    }
    for sub in ["dump", "baseline", "log", "work"] {
        create_dir(&opts.out.join(sub))?;
    }
    let out = std::fs::canonicalize(&opts.out).map_err(|source| Error::Read {
        path: opts.out.display().to_string(),
        source,
    })?;
    let opts = Options {
        out: out.clone(),
        ..opts.clone()
    };
    let meta = std::fs::metadata(&out).map_err(|source| Error::Read {
        path: out.display().to_string(),
        source,
    })?;
    let owner = format!("{}:{}", meta.uid(), meta.gid());

    let mut names = if opts.tests.is_empty() {
        list_tests(&opts.image)?
    } else {
        opts.tests.clone()
    };
    if let Some(path) = &opts.order_from {
        order_by(&mut names, &Report::load(path)?);
    }
    for name in &names {
        check_name(name)?;
    }
    write(&out.join("names.txt"), (names.join("\n") + "\n").as_bytes())?;

    for name in &opts.split {
        if !names.contains(name) {
            return Err(Error::TestName(format!(
                "{name} (asked to split, but not run)"
            )));
        }
    }
    let max = max_protocol(&opts.image)?;
    let jobs: Vec<Job> = names
        .iter()
        .enumerate()
        .flat_map(|(case, name)| {
            if opts.split.contains(name) {
                (0..=max)
                    .map(|v| Job {
                        case,
                        version: Some(v),
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![Job {
                    case,
                    version: None,
                }]
            }
        })
        .collect();

    let started = Instant::now();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let results: Mutex<Vec<(usize, TestRun)>> = Mutex::new(Vec::with_capacity(jobs.len()));
    std::thread::scope(|scope| {
        for _ in 0..opts.shards.min(jobs.len()) {
            scope.spawn(|| {
                loop {
                    let j = next.fetch_add(1, Ordering::SeqCst);
                    let Some(job) = jobs.get(j).copied() else {
                        break;
                    };
                    let run = run_job(&opts, &owner, job, &names[job.case]);
                    let n = done.fetch_add(1, Ordering::SeqCst) + 1;
                    progress(&run, n, jobs.len());
                    results
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .push((j, run));
                }
            });
        }
    });
    let mut parts = results.into_inner().unwrap_or_else(|p| p.into_inner());
    parts.sort_by_key(|(j, _)| *j);
    let mut tests = Vec::with_capacity(names.len());
    let mut i = 0;
    while i < parts.len() {
        let case = jobs[parts[i].0].case;
        let end = parts[i..]
            .iter()
            .position(|(j, _)| jobs[*j].case != case)
            .map_or(parts.len(), |p| i + p);
        let group: Vec<TestRun> = parts[i..end].iter().map(|(_, r)| r.clone()).collect();
        tests.push(if group.len() == 1 && !group[0].split {
            group.into_iter().next().expect("one part")
        } else {
            join_parts(&out, case, &names[case], group, max)
        });
        i = end;
    }
    std::fs::remove_dir(out.join("work")).ok();

    let mut totals = Totals {
        tests: tests.len(),
        wall_seconds: started.elapsed().as_secs_f64(),
        ..Totals::default()
    };
    for t in &tests {
        totals.passed += usize::from(t.passed());
        totals.oom_killed += usize::from(t.oom_killed);
        totals.oom_retried += t.oom_retried;
        totals.tests_with_metas += usize::from(t.metas > 0);
        totals.metas += t.metas;
        totals.dump_bytes += t.dump_bytes;
        totals.unlabelled += t.unlabelled;
        totals.test_seconds += t.seconds;
        for (p, n) in &t.protocols {
            *totals.protocols.entry(*p).or_default() += n;
        }
        for (v, n) in &t.meta_versions {
            *totals.meta_versions.entry(*v).or_default() += n;
        }
    }
    let report = Report {
        image: opts.image.reference(),
        max_protocol: max,
        rng_seed: RNG_SEED,
        shards: opts.shards,
        cpus: opts.limits.cpus.clone(),
        memory: opts.limits.memory.clone(),
        retry_memory: opts.limits.retry_memory.clone(),
        totals,
        tests,
    };
    let mut text = serde_json::to_string_pretty(&report)?;
    text.push('\n');
    write(&out.join("run.json"), text.as_bytes())?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_names_are_checked_like_selftest_parallel() {
        check_name("bump sequence").unwrap();
        check_name("change trust (pool share): flags/x").unwrap();
        assert!(check_name("a,b").is_err());
        assert!(check_name("[tx]").is_err());
        assert!(check_name("").is_err());
    }

    #[test]
    fn earlier_seconds_order_longest_first_unknown_first() {
        let earlier = Report {
            image: String::new(),
            max_protocol: 30,
            rng_seed: RNG_SEED,
            shards: 1,
            cpus: "1".into(),
            memory: "1g".into(),
            retry_memory: None,
            totals: Totals::default(),
            tests: [("a", 5.0), ("b", 50.0), ("c", 1.0)]
                .iter()
                .map(|(n, s)| TestRun {
                    name: (*n).into(),
                    seconds: *s,
                    ..TestRun::default()
                })
                .collect(),
        };
        let mut names: Vec<String> = ["a", "b", "c", "new"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        order_by(&mut names, &earlier);
        assert_eq!(names, ["new", "b", "a", "c"]);
    }

    #[test]
    fn reads_the_top_protocol_line_of_version() {
        let v31 = "stellar-core 30.0.1.vnext (362dbc51)\nledger protocol version: 31\nrust version: x\nsoroban-env-host versions: \n    host[0]:\n        ledger protocol version: 21\n";
        assert_eq!(parse_max_protocol(v31), Some(31));
        assert_eq!(parse_max_protocol("stellar-core 30.0.0\n"), None);
    }

    #[test]
    fn summarises_a_dump() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("d.txt");
        std::fs::write(
            &path,
            "=== \"A\" : t|protocol version 3 ===\n{\n    \"TransactionMeta\": {\n        \"v\": 2,\n        \"v2\": {}\n    }\n}\n\
=== \"A\" : t|protocol version 3 ===\n{\n    \"TransactionMeta\": {\n        \"v\": 2,\n        \"v2\": {}\n    }\n}\n\
=== \"A\" : t ===\n{\n    \"TransactionMeta\": {\n        \"v\": 4,\n        \"v4\": {}\n    }\n}\n",
        )
        .unwrap();
        let mut run = TestRun::default();
        summarise_dump(&path, &mut run).unwrap();
        assert_eq!(run.metas, 3);
        assert_eq!(run.protocols, BTreeMap::from([(3, 2)]));
        assert_eq!(run.unlabelled, 1);
        assert_eq!(run.meta_versions, BTreeMap::from([(2, 2), (4, 1)]));
    }
}
