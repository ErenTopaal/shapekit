//! Runs core's `[tx]` tests in the pinned test image, each test case in its own container, and
//! summarises the dumps.
//!
//! One container per test case, never one process for many: core's memory grows across test
//! cases in one process (`scripts/check-lcm.sh` saw the oom killer at about 6 GB), and a
//! separate container gives each case its own network namespace for the ports tests open.
//! At most `shards` containers run at once, each pulling the next case from a shared queue, so
//! a long case does not hold back the cases queued behind it.
//!
//! Output layout under `out`:
//!
//! ```text
//! names.txt               the test cases, one per line, in run order
//! dump/NNN.txt            --debug-test-tx-meta of case NNN
//! baseline/NNN/*.json     --record-test-tx-meta of case NNN (core's baseline format)
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

/// Limits for one test container.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Limits {
    /// docker `--cpus`.
    pub cpus: String,
    /// docker `--memory`, also used as `--memory-swap` so a case cannot swap.
    pub memory: String,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            cpus: "1".into(),
            memory: "1536m".into(),
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
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Report {
    pub image: String,
    pub rng_seed: u32,
    pub shards: usize,
    pub cpus: String,
    pub memory: String,
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

fn container_name(index: usize) -> String {
    format!("shapekit-coretest-{}-{index:03}", std::process::id())
}

fn run_one(opts: &Options, owner: &str, index: usize, name: &str) -> TestRun {
    let mut run = TestRun {
        index,
        name: name.to_owned(),
        ..TestRun::default()
    };
    let tag = format!("{index:03}");
    let work = opts.out.join("work").join(&tag);
    let baseline = opts.out.join("baseline").join(&tag);
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
        let container = container_name(index);
        let mount = |host: &Path, inside: &str| format!("{}:{inside}", host.display());
        let status = Command::new("docker")
            .args(["run", "--name", &container, "--network", "none"])
            .args(["--cpus", &opts.limits.cpus])
            .args(["--memory", &opts.limits.memory])
            .args(["--memory-swap", &opts.limits.memory])
            .args(["--user", owner, "-e", "HOME=/work", "-w", "/work"])
            .args(["-v", &mount(&work, "/work")])
            .args(["-v", &mount(&baseline, "/baseline")])
            .args(["-v", &mount(&opts.out.join("dump"), "/dump")])
            .arg(opts.image.reference())
            .args(["test", name, "--all-versions"])
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

    let started = Instant::now();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let results: Mutex<Vec<TestRun>> = Mutex::new(Vec::with_capacity(names.len()));
    std::thread::scope(|scope| {
        for _ in 0..opts.shards.min(names.len()) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(name) = names.get(i) else { break };
                    let run = run_one(&opts, &owner, i, name);
                    let n = done.fetch_add(1, Ordering::SeqCst) + 1;
                    progress(&run, n, names.len());
                    results.lock().unwrap_or_else(|p| p.into_inner()).push(run);
                }
            });
        }
    });
    let mut tests = results.into_inner().unwrap_or_else(|p| p.into_inner());
    tests.sort_by_key(|t| t.index);
    std::fs::remove_dir(out.join("work")).ok();

    let mut totals = Totals {
        tests: tests.len(),
        wall_seconds: started.elapsed().as_secs_f64(),
        ..Totals::default()
    };
    for t in &tests {
        totals.passed += usize::from(t.passed());
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
        rng_seed: RNG_SEED,
        shards: opts.shards,
        cpus: opts.limits.cpus.clone(),
        memory: opts.limits.memory.clone(),
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
            rng_seed: RNG_SEED,
            shards: 1,
            cpus: "1".into(),
            memory: "1g".into(),
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
        let mut names: Vec<String> = ["a", "b", "c", "new"].iter().map(|s| s.to_string()).collect();
        order_by(&mut names, &earlier);
        assert_eq!(names, ["new", "b", "a", "c"]);
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
