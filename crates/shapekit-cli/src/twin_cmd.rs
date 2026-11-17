//! `shapekit twin`: the coverage twin of stellar-core, its byte equality with the release
//! image, and the protocol gate list.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use shapekit_contracts::CoreRef;
use shapekit_contracts::gates::Comparison;
use shapekit_core_driver::config::CoreImage;
use shapekit_registry::Registry;
use shapekit_twin::gates;
use shapekit_twin::verify::{DEFAULT_CHAINS, SMOKE, VerifyOptions};

use crate::{manifest_cmd, pins};

/// The smoke chain's testnet canonical hash on the release image, as
/// measured on stellar-core 30.0.0 before the twin existed.
const SMOKE_TESTNET_RELEASE: &str =
    "926fbc8145c40b9d66f3c3a67a9177c22c131232dfaacb9ace37461be95bd818";

struct VerifyArgs {
    chains: Vec<String>,
    pins: PathBuf,
    twin_pins: PathBuf,
    corpus: PathBuf,
    registry: PathBuf,
    jobs: u16,
    out: PathBuf,
}

fn verify(a: VerifyArgs) -> Result<bool> {
    if a.out.exists() {
        bail!("{} already exists", a.out.display());
    }
    let release = CoreImage::from_pins(&a.pins)
        .with_context(|| format!("reading the core image pin from {}", a.pins.display()))?;
    let twin = CoreImage::from_pins(&a.twin_pins)
        .with_context(|| format!("reading the twin image pin from {}", a.twin_pins.display()))?;
    if !twin.local {
        bail!(
            "{} pins a registry image; the twin is a local build pinned by image_id",
            a.twin_pins.display()
        );
    }
    let registry = Registry::load(&a.registry)
        .with_context(|| format!("loading the registry in {}", a.registry.display()))?;
    let generated_with = manifest_cmd::generated_with(&pins::load(&a.pins)?)?;
    let chains = if a.chains.is_empty() {
        DEFAULT_CHAINS.iter().map(|c| c.to_string()).collect()
    } else {
        a.chains
    };
    let opts = VerifyOptions {
        release: release.clone(),
        twin: twin.clone(),
        chains,
        corpus: a.corpus,
        registry: &registry,
        generated_with,
        jobs: usize::from(a.jobs),
        out: a.out.clone(),
    };
    println!("release {}", release.reference());
    println!(
        "twin    {} ({}:{})",
        twin.reference(),
        twin.repository,
        twin.tag
    );
    let results = shapekit_twin::verify::verify(&opts)?;
    let mut ok = true;
    for c in &results {
        let corpus = match c.matches_corpus() {
            None => String::new(),
            Some(true) => ", corpus equal".into(),
            Some(false) => {
                ok = false;
                format!(
                    ", CORPUS DIFFERS ({})",
                    c.committed.as_deref().unwrap_or("")
                )
            }
        };
        ok &= c.equal();
        println!(
            "{} ({}): ledgers {} to {}, release {} ({:.1}s), twin {} ({:.1}s): {}{corpus}",
            c.chain,
            c.network.name(),
            c.twin.first_ledger,
            c.twin.last_ledger,
            c.release.canonical_stream_sha256,
            c.release.seconds,
            c.twin.canonical_stream_sha256,
            c.twin.seconds,
            if c.equal() { "equal" } else { "DIFFERS" },
        );
        if c.chain == SMOKE
            && c.network.name() == "testnet"
            && c.release.canonical_stream_sha256 != SMOKE_TESTNET_RELEASE
        {
            ok = false;
            println!(
                "smoke chain (testnet) on the release image is {}, expected {SMOKE_TESTNET_RELEASE}",
                c.release.canonical_stream_sha256
            );
        }
    }
    let equal = results.iter().filter(|c| c.equal()).count();
    let corpus: Vec<bool> = results.iter().filter_map(|c| c.matches_corpus()).collect();
    println!(
        "{equal} of {} comparisons equal, {} of {} corpus hashes equal; chains in {}",
        results.len(),
        corpus.iter().filter(|b| **b).count(),
        corpus.len(),
        a.out.display()
    );
    Ok(ok)
}

#[derive(clap::Subcommand)]
pub enum TwinCommand {
    /// How many of the `ordinary` unknown ranges (the explorer's targets) a set of runs
    /// reaches: `twin coverage` output directories or `explore gates` run counts.
    Targets {
        #[arg(long, default_value = "corpus/coverage/unknown-regions.json")]
        unknown: PathBuf,
        /// Region count files (`counts.json` of `twin coverage`, `<run>.counts.json` of
        /// `explore gates`); repeatable.
        #[arg(long = "counts", num_args = 1.., required = true)]
        counts: Vec<PathBuf>,
    },
    /// Mark each side of each protocol gate met when a measured chain takes it on the twin
    /// (the gate's branch counts in `twin coverage` output), with the chain as witness, and
    /// give every side left open the reason the data shows. Rewrites the gate list.
    GateWitnesses {
        /// `twin coverage` output directories of corpus chains.
        #[arg(long = "chain-dir", num_args = 1.., required = true)]
        chain_dirs: Vec<PathBuf>,
        /// A stellar-core checkout at the gate list's commit.
        #[arg(long)]
        core_src: PathBuf,
        #[arg(long, default_value = "twin/pins.toml")]
        twin_pins: PathBuf,
        #[arg(long, default_value = "registry/generated/gates.json")]
        gates: PathBuf,
        /// `explore-gate-sides.json` from `shapekit explore gates`: recipe reruns whose
        /// witnesses join the chains'.
        #[arg(long)]
        explore: Option<PathBuf>,
    },
    /// Sort the regions of a core coverage summary that no chain and no core test reaches
    /// by cause: defensive checks, diagnostics, configuration or test-only code, protocol
    /// sides no chain runs, and ordinary code no chain builds yet. Static rules first, then
    /// the hand labels file; each range records which decided it.
    Classify {
        /// A stellar-core checkout at the summary's commit.
        #[arg(long)]
        core_src: PathBuf,
        #[arg(long, default_value = "corpus/coverage/core-coverage.json")]
        coverage: PathBuf,
        #[arg(long, default_value = "registry/generated/gates.json")]
        gates: PathBuf,
        #[arg(long, default_value = "twin/unknown-labels.toml")]
        labels: PathBuf,
        #[arg(long, default_value = "corpus/coverage/unknown-regions.json")]
        out: PathBuf,
    },
    /// Run core's own test cases on the twin, one at a time at every protocol version, and
    /// keep the region counts of each (`<out>/tests/<n>/counts.json`, listed in
    /// `<out>/tests.tsv`).
    CoreTests {
        /// Catch2 test spec selecting the test cases.
        #[arg(long, default_value = "[tx]")]
        spec: String,
        #[arg(long, default_value = "twin/pins.toml")]
        twin_pins: PathBuf,
        /// Test cases run at once, each in a container of one cpu.
        #[arg(long, default_value_t = 3)]
        jobs: usize,
        /// Output directory; must not exist.
        #[arg(long)]
        out: PathBuf,
    },
    /// The core coverage summary over several `twin coverage` outputs, with every region no
    /// chain reaches classified by the core tests that reach it (`twin core-tests` output).
    RList {
        /// `twin coverage` output directories.
        #[arg(long = "chain-dir", num_args = 1.., required = true)]
        chain_dirs: Vec<PathBuf>,
        /// A `twin core-tests` output directory.
        #[arg(long)]
        tests: PathBuf,
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
        #[arg(long, default_value = "twin/pins.toml")]
        twin_pins: PathBuf,
        /// Where to write the summary (corpus/coverage/core-coverage.json).
        #[arg(long)]
        out: PathBuf,
    },
    /// Record one chain on the coverage twin and read which regions of core's
    /// transaction and ledger code it reaches: the core coverage summary
    /// (`core-coverage.json`) and the merged profile, written into `--out`.
    Coverage {
        /// `smoke` or a corpus chain id. The chain is recorded under both passphrases, as
        /// the corpus holds it, and the coverage is over both runs.
        #[arg(long)]
        chain: String,
        /// Pins naming the release image; their stack is recorded with recipe chains.
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
        /// Pins naming the twin image.
        #[arg(long, default_value = "twin/pins.toml")]
        twin_pins: PathBuf,
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
        #[arg(long, default_value = "registry")]
        registry: PathBuf,
        /// Print the regions overlapping these lines, `<file>:<from>-<to>` with the file
        /// as a path in core's repository.
        #[arg(long)]
        show: Vec<String>,
        /// Output directory; must not exist.
        #[arg(long)]
        out: PathBuf,
    },
    /// Record the same chains under both passphrases on the release image and on the
    /// coverage twin and compare their canonical stream hashes; corpus chains are also
    /// compared with the hash committed in the corpus. Exits 1 on any difference.
    Verify {
        /// Chains to compare: `smoke` or corpus chain ids. Defaults to the smoke chain,
        /// cap40.signed-payload-signer.p30, cap73.sac-trust.p30,
        /// classic.failed-tx.change-trust-native.p9 and cap67.unified-events.p22.
        #[arg(long = "chain")]
        chains: Vec<String>,
        /// Pins naming the release image.
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
        /// Pins naming the twin image.
        #[arg(long, default_value = "twin/pins.toml")]
        twin_pins: PathBuf,
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
        #[arg(long, default_value = "registry")]
        registry: PathBuf,
        /// Chains recorded at once on each image.
        #[arg(long, default_value_t = 2)]
        jobs: u16,
        /// Where the recorded chains are written; must not exist.
        #[arg(long)]
        out: PathBuf,
    },
    /// List every protocol version comparison in core's non-test C++ source
    /// (`protocolVersionIsBefore`, `StartsFrom`, `Equals`) with its function and protocol,
    /// both sides open. The checkout must be at the stellar-core commit pinned in the pins
    /// file; a pin by tag records the tag as the core version, a pin by bare commit (such
    /// as next/pins.toml) records the commit. Comparisons against a runtime value instead of a protocol constant are printed,
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
        TwinCommand::Targets { unknown, counts } => targets(&unknown, &counts),
        TwinCommand::GateWitnesses {
            chain_dirs,
            core_src,
            twin_pins,
            gates,
            explore,
        } => gate_witnesses(
            &chain_dirs,
            &core_src,
            &twin_pins,
            &gates,
            explore.as_deref(),
        ),
        TwinCommand::Classify {
            core_src,
            coverage,
            gates,
            labels,
            out,
        } => classify(&core_src, &coverage, &gates, &labels, &out),
        TwinCommand::CoreTests {
            spec,
            twin_pins,
            jobs,
            out,
        } => core_tests(&spec, &twin_pins, jobs, &out),
        TwinCommand::RList {
            chain_dirs,
            tests,
            pins,
            twin_pins,
            out,
        } => r_list(&chain_dirs, &tests, &pins, &twin_pins, &out),
        TwinCommand::Coverage {
            chain,
            pins,
            twin_pins,
            corpus,
            registry,
            show,
            out,
        } => coverage(CoverageArgs {
            chain,
            pins,
            twin_pins,
            corpus,
            registry,
            show,
            out,
        }),
        TwinCommand::Verify {
            chains,
            pins,
            twin_pins,
            corpus,
            registry,
            jobs,
            out,
        } => verify(VerifyArgs {
            chains,
            pins,
            twin_pins,
            corpus,
            registry,
            jobs,
            out,
        }),
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
    let commit = git
        .sha
        .clone()
        .with_context(|| "the stellar-core pin has no commit")?;
    // a pin by bare commit (an unreleased core, such as the next protocol's)
    // names the commit as its version, the form the contracts accept for it
    let version = match git.git_ref.as_deref() {
        None => commit.clone(),
        Some(r) => r
            .strip_prefix("refs/tags/")
            .with_context(|| format!("the stellar-core pin's ref {r} is not a tag"))?
            .to_owned(),
    };
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

struct CoverageArgs {
    chain: String,
    pins: PathBuf,
    twin_pins: PathBuf,
    corpus: PathBuf,
    registry: PathBuf,
    show: Vec<String>,
    out: PathBuf,
}

fn parse_show(s: &str) -> Result<(String, u32, u32)> {
    let (file, lines) = s
        .rsplit_once(':')
        .with_context(|| format!("--show {s:?} is not <file>:<from>-<to>"))?;
    let (a, b) = lines
        .split_once('-')
        .with_context(|| format!("--show {s:?} is not <file>:<from>-<to>"))?;
    Ok((file.to_owned(), a.parse()?, b.parse()?))
}

fn coverage(a: CoverageArgs) -> Result<bool> {
    use shapekit_gen::recipe::{self, Network, chain_id};
    use shapekit_twin::coverage as cov;
    use shapekit_twin::verify::{RecordSetup, record_all};

    let shows = a
        .show
        .iter()
        .map(|s| parse_show(s))
        .collect::<Result<Vec<_>>>()?;
    if a.out.exists() {
        bail!("{} already exists", a.out.display());
    }
    let mut twin = CoreImage::from_pins(&a.twin_pins)
        .with_context(|| format!("reading the twin image pin from {}", a.twin_pins.display()))?;
    if !twin.local {
        bail!(
            "{} pins a registry image; the twin is a local build pinned by image_id",
            a.twin_pins.display()
        );
    }
    let profiles = a.out.join("profiles");
    std::fs::create_dir_all(&profiles)
        .with_context(|| format!("creating {}", profiles.display()))?;
    let profiles = profiles.canonicalize()?;
    twin.profile_dir = Some(profiles.clone());

    let registry = Registry::load(&a.registry)
        .with_context(|| format!("loading the registry in {}", a.registry.display()))?;
    let setup = RecordSetup {
        registry: &registry,
        generated_with: manifest_cmd::generated_with(&pins::load(&a.pins)?)?,
        jobs: 1,
        networks: Network::BOTH.to_vec(),
    };
    let all = recipe::all();
    let recipes: Vec<&dyn recipe::Recipe> = if a.chain == SMOKE {
        Vec::new()
    } else {
        vec![
            all.iter()
                .find(|r| chain_id(r.as_ref()) == a.chain)
                .with_context(|| format!("no recipe records chain {}", a.chain))?
                .as_ref(),
        ]
    };
    let recorded = record_all(
        &twin,
        &recipes,
        a.chain == SMOKE,
        &a.out.join("chain"),
        &setup,
    )?;
    for network in Network::BOTH {
        let run = recorded
            .get(&(a.chain.clone(), network.name()))
            .with_context(|| format!("chain {} was not recorded", a.chain))?;
        println!(
            "{} ({}) on the twin: ledgers {} to {} in {:.1}s, canonical stream {}",
            a.chain,
            network.name(),
            run.first_ledger,
            run.last_ledger,
            run.seconds,
            run.canonical_stream_sha256
        );
        if a.chain != SMOKE {
            let committed = a
                .corpus
                .join("chains")
                .join(&a.chain)
                .join(network.name())
                .join("chain.json");
            let v: serde_json::Value = serde_json::from_slice(
                &std::fs::read(&committed)
                    .with_context(|| format!("reading {}", committed.display()))?,
            )?;
            if v["canonical_stream_sha256"].as_str() != Some(run.canonical_stream_sha256.as_str()) {
                bail!(
                    "the twin's chain differs from {} ({}); its coverage would not describe the corpus chain",
                    committed.display(),
                    v["canonical_stream_sha256"]
                );
            }
            println!("equal to the committed chain");
        }
    }

    let export = cov::merge_and_export(&twin, &profiles)?;
    let (counts, branches) = cov::read_export_with_branches(&export)?;
    // the export and the raw profiles are large and fully represented by the
    // merged profile and the summary
    std::fs::remove_file(&export)?;
    for e in std::fs::read_dir(&profiles)? {
        let e = e?;
        if e.file_name().to_string_lossy().ends_with(".profraw") {
            std::fs::remove_file(e.path())?;
        }
    }
    let mut core = pinned_core(&a.pins)?;
    core.image = Some(cov::image_ref(&twin));
    cov::write_counts(&a.out.join("counts.json"), &counts)?;
    cov::write_branches(&a.out.join("branches.json"), &branches)?;
    let summary = cov::summary(&counts, core, vec![format!("chain:{}", a.chain)], &[]);
    let file = a.out.join("core-coverage.json");
    shapekit_contracts::write(&file, &summary)?;
    let reached: u64 = summary.files.iter().map(|f| f.regions_reached).sum();
    let missed: u64 = summary.files.iter().map(|f| f.regions_unreached).sum();
    println!(
        "{} files of {}: {reached} of {} regions reached, {} unreached line ranges; written to {}",
        summary.files.len(),
        cov::MEASURED_DIRS.join(" and "),
        reached + missed,
        summary.unreached.len(),
        file.display()
    );
    for (f, from, to) in &shows {
        let spans = cov::spans_in(&counts, f, *from, *to);
        if spans.is_empty() {
            println!("{f}:{from}-{to}: no region");
        }
        for (s, c) in spans {
            println!(
                "{f}:{}:{}-{}:{} count {c}",
                s.line_start, s.column_start, s.line_end, s.column_end
            );
        }
    }
    Ok(true)
}

fn twin_image(path: &Path) -> Result<CoreImage> {
    let twin = CoreImage::from_pins(path)
        .with_context(|| format!("reading the twin image pin from {}", path.display()))?;
    if !twin.local {
        bail!(
            "{} pins a registry image; the twin is a local build pinned by image_id",
            path.display()
        );
    }
    Ok(twin)
}

fn core_tests(spec: &str, twin_pins: &Path, jobs: usize, out: &Path) -> Result<bool> {
    use shapekit_twin::coverage as cov;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    if out.exists() {
        bail!("{} already exists", out.display());
    }
    let twin = twin_image(twin_pins)?;
    let names = cov::list_core_tests(&twin, spec)?;
    std::fs::create_dir_all(out.join("tests"))?;
    println!("{} test cases match {spec}", names.len());
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<(usize, Result<cov::TestRun>)>> = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..jobs.max(1) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(name) = names.get(i) else { break };
                    let r = cov::run_core_test(&twin, name, &out.join("tests").join(i.to_string()));
                    if let Ok(t) = &r {
                        println!("{i}: {name}: exit {:?}, {:.0}s", t.exit_code, t.seconds);
                    }
                    results
                        .lock()
                        .expect("no panics hold the lock")
                        .push((i, r));
                }
            });
        }
    });
    let mut results = results.into_inner().expect("threads joined");
    results.sort_by_key(|(i, _)| *i);
    let mut tsv = String::from("index\texit\tseconds\tname\n");
    let mut ok = true;
    for (i, r) in &results {
        match r {
            Ok(t) => {
                ok &= t.exit_code == Some(0);
                tsv.push_str(&format!(
                    "{i}\t{}\t{:.1}\t{}\n",
                    t.exit_code.map_or("signal".into(), |c| c.to_string()),
                    t.seconds,
                    t.name
                ));
            }
            Err(e) => {
                ok = false;
                println!("{i}: {}: {e:#}", names[*i]);
                tsv.push_str(&format!("{i}\terror\t0\t{}\n", names[*i]));
            }
        }
    }
    std::fs::write(out.join("tests.tsv"), tsv)?;
    let failed = results
        .iter()
        .filter(|(_, r)| !matches!(r, Ok(t) if t.exit_code == Some(0)))
        .count();
    println!(
        "{} test cases run, {failed} did not exit 0; counts in {}",
        results.len(),
        out.join("tests").display()
    );
    Ok(ok)
}

fn r_list(
    chain_dirs: &[PathBuf],
    tests: &Path,
    pins_path: &Path,
    twin_pins: &Path,
    out: &Path,
) -> Result<bool> {
    use shapekit_twin::coverage as cov;

    let twin = twin_image(twin_pins)?;
    let mut chains = Vec::new();
    let mut chain_counts = Vec::new();
    for d in chain_dirs {
        let summary: shapekit_contracts::CoreCoverage =
            shapekit_contracts::load(&d.join("core-coverage.json"))?;
        if summary.core.image.as_ref().map(|i| &i.digest) != Some(&twin.digest) {
            bail!(
                "{} was measured on another image than {}",
                d.display(),
                twin.digest
            );
        }
        chains.extend(summary.chains);
        chain_counts.push(cov::read_counts(&d.join("counts.json"))?);
    }
    chains.sort();
    chains.dedup();
    let merged = cov::merge_counts(&chain_counts);

    let tsv = std::fs::read_to_string(tests.join("tests.tsv"))
        .with_context(|| format!("reading {}", tests.join("tests.tsv").display()))?;
    let mut test_counts = Vec::new();
    for line in tsv.lines().skip(1) {
        let cols: Vec<&str> = line.splitn(4, '\t').collect();
        let [i, exit, _, name] = cols.as_slice() else {
            bail!("malformed tests.tsv line {line:?}");
        };
        if *exit != "0" {
            bail!("core test {name} did not exit 0 ({exit}); its counts are not used");
        }
        test_counts.push((
            name.to_string(),
            cov::read_counts(&tests.join("tests").join(i).join("counts.json"))?,
        ));
    }
    let mut core = pinned_core(pins_path)?;
    core.image = Some(cov::image_ref(&twin));
    let summary = cov::summary(&merged, core, chains, &test_counts);
    shapekit_contracts::write(out, &summary)?;
    let reached: u64 = summary.files.iter().map(|f| f.regions_reached).sum();
    let missed: u64 = summary.files.iter().map(|f| f.regions_unreached).sum();
    let by_tests = summary
        .unreached
        .iter()
        .filter(|r| !r.core_tests.is_empty())
        .count();
    println!(
        "{} chains, {} core tests: {reached} of {} regions reached by a chain; {} unreached line ranges, {by_tests} of them reached by a core test, {} unknown; written to {}",
        summary.chains.len(),
        test_counts.len(),
        reached + missed,
        summary.unreached.len(),
        summary.unreached.len() - by_tests,
        out.display()
    );
    Ok(true)
}

fn classify(
    core_src: &Path,
    coverage: &Path,
    gates_path: &Path,
    labels: &Path,
    out: &Path,
) -> Result<bool> {
    use shapekit_twin::classify as cl;

    let summary: shapekit_contracts::CoreCoverage = shapekit_contracts::load(coverage)?;
    let gate_list: shapekit_contracts::GateList = shapekit_contracts::load(gates_path)?;
    if gate_list.core.commit != summary.core.commit {
        bail!(
            "{} is for core {}, {} for core {}",
            gates_path.display(),
            gate_list.core.commit,
            coverage.display(),
            summary.core.commit
        );
    }
    let head = checkout_commit(core_src)?;
    if head != summary.core.commit {
        bail!(
            "{} is at {head}, the summary is for core {}",
            core_src.display(),
            summary.core.commit
        );
    }
    let scan = gates::scan_tree(core_src)?;
    let labels = cl::read_labels(labels)?;
    let result = cl::classify(core_src, &summary, &gate_list, &scan, &labels)?;
    let mut text = serde_json::to_string_pretty(&result)?;
    text.push('\n');
    std::fs::write(out, text).with_context(|| format!("writing {}", out.display()))?;
    println!(
        "{} unknown ranges, chains on protocols {:?}:",
        result.ranges.len(),
        result.chain_protocols
    );
    for c in &result.counts {
        println!(
            "  {}: {} ({} by rule, {} by hand, {} residual)",
            c.cause.as_str(),
            c.by_rule + c.by_hand + c.residual,
            c.by_rule,
            c.by_hand,
            c.residual
        );
    }
    let mut rules: std::collections::BTreeMap<&str, usize> = Default::default();
    for r in &result.ranges {
        *rules.entry(r.decided_by.as_str()).or_default() += 1;
    }
    for (k, v) in rules {
        println!("  decided by {k}: {v}");
    }
    println!("written to {}", out.display());
    Ok(true)
}

fn gate_witnesses(
    chain_dirs: &[PathBuf],
    core_src: &Path,
    twin_pins: &Path,
    gates_path: &Path,
    explore: Option<&Path>,
) -> Result<bool> {
    use shapekit_twin::coverage as cov;
    use shapekit_twin::gate_sides;

    let twin = twin_image(twin_pins)?;
    let list: shapekit_contracts::GateList = shapekit_contracts::load(gates_path)?;
    let head = checkout_commit(core_src)?;
    if head != list.core.commit {
        bail!(
            "{} is at {head}, {} is for core {}",
            core_src.display(),
            gates_path.display(),
            list.core.commit
        );
    }
    let mut chains = Vec::new();
    for d in chain_dirs {
        let summary: shapekit_contracts::CoreCoverage =
            shapekit_contracts::load(&d.join("core-coverage.json"))?;
        if summary.core.image.as_ref().map(|i| &i.digest) != Some(&twin.digest) {
            bail!(
                "{} was measured on another image than {}",
                d.display(),
                twin.digest
            );
        }
        if summary.core.commit != list.core.commit {
            bail!(
                "{} was measured on core {}",
                d.display(),
                summary.core.commit
            );
        }
        let [chain] = summary.chains.as_slice() else {
            bail!(
                "{} measures {} chains, expected one",
                d.display(),
                summary.chains.len()
            );
        };
        let protocol: u32 = chain
            .rsplit_once(".p")
            .and_then(|(_, n)| n.parse().ok())
            .with_context(|| format!("{chain} names no protocol"))?;
        chains.push((
            chain.clone(),
            protocol,
            cov::read_branches(&d.join("branches.json"))?,
        ));
    }
    let mut sources: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    let mut line_of = |file: &str, line: u32| -> Option<String> {
        if !sources.contains_key(file) {
            let text = std::fs::read_to_string(core_src.join(file)).ok()?;
            sources.insert(file.to_owned(), text.lines().map(str::to_owned).collect());
        }
        sources[file].get(line as usize - 1).cloned()
    };
    let mut lines: std::collections::BTreeMap<(String, u32), Option<String>> = Default::default();
    for g in &list.gates {
        lines.insert((g.file.clone(), g.line), line_of(&g.file, g.line));
    }
    let explore = match explore {
        Some(p) => {
            let e: gate_sides::ExploreSides = serde_json::from_slice(
                &std::fs::read(p).with_context(|| format!("reading {}", p.display()))?,
            )
            .with_context(|| format!("parsing {}", p.display()))?;
            if e.schema_version != 1 {
                bail!(
                    "{} has schema_version {}, expected 1",
                    p.display(),
                    e.schema_version
                );
            }
            if e.twin_image != twin.digest {
                bail!(
                    "{} was run on {}, the twin is {}",
                    p.display(),
                    e.twin_image,
                    twin.digest
                );
            }
            Some(e)
        }
        None => None,
    };
    let w = gate_sides::witness(&list, &chains, explore.as_ref(), |f, l| {
        lines.get(&(f.to_owned(), l)).cloned().flatten()
    });
    shapekit_contracts::write(gates_path, &w.list)?;
    let open_by: std::collections::BTreeMap<String, usize> = w
        .list
        .gates
        .iter()
        .flat_map(|g| &g.sides)
        .filter(|s| s.status == shapekit_contracts::ObligationStatus::Open)
        .filter_map(|s| s.evidence.as_deref())
        .fold(Default::default(), |mut m, e| {
            let kind = if e.starts_with("the gate's result") {
                "no branch region"
            } else if e.starts_with("needs a chain at") {
                "needs a protocol no chain runs"
            } else if e.starts_with("no chain evaluates") {
                "condition never evaluated"
            } else {
                "condition evaluated, side never taken"
            };
            *m.entry(kind.to_owned()).or_default() += 1;
            m
        });
    println!(
        "{} gates, {} sides: {} met ({} by recipe reruns only), {} open; {} gates have no branch region",
        w.list.gates.len(),
        w.met + w.open,
        w.met,
        w.met_by_reruns_only,
        w.open,
        w.no_branch
    );
    for c in &w.conflicts {
        println!(
            "  CONFLICT {}:{}:{} {} side: {} claims it, {} evaluated the condition and never took it",
            c.file,
            c.line,
            c.column,
            c.branch.as_str(),
            c.run,
            c.chain
        );
    }
    for (k, v) in open_by {
        println!("  open, {k}: {v}");
    }
    println!("written to {}", gates_path.display());
    Ok(true)
}

fn targets(unknown: &Path, counts: &[PathBuf]) -> Result<bool> {
    use shapekit_twin::classify::{Cause, UnknownRegions};
    use shapekit_twin::coverage as cov;

    let u: UnknownRegions = serde_json::from_slice(
        &std::fs::read(unknown).with_context(|| format!("reading {}", unknown.display()))?,
    )
    .with_context(|| format!("parsing {}", unknown.display()))?;
    let runs: Vec<(String, cov::Counts)> = counts
        .iter()
        .map(|p| Ok((p.display().to_string(), cov::read_counts(p)?)))
        .collect::<Result<_>>()?;
    let targets: Vec<_> = u
        .ranges
        .iter()
        .filter(|r| r.cause == Cause::Ordinary)
        .collect();
    let mut reached = 0;
    let mut by_run: std::collections::BTreeMap<&str, usize> = Default::default();
    for t in &targets {
        let mut hit = false;
        for (name, c) in &runs {
            // a line range is unreached when one of its spans did not run, so
            // it is reached only when every span with these lines ran
            let any = c.get(&t.file).is_some_and(|spans| {
                let mut of_range = spans
                    .iter()
                    .filter(|(s, _)| s.line_start == t.line_start && s.line_end == t.line_end)
                    .peekable();
                of_range.peek().is_some() && of_range.all(|(_, n)| *n > 0)
            });
            if any {
                hit = true;
                *by_run.entry(name.as_str()).or_default() += 1;
            }
        }
        if hit {
            reached += 1;
            println!("reached {}:{}-{}", t.file, t.line_start, t.line_end);
        }
    }
    for (name, n) in &by_run {
        println!("  {n} targets reached by {name}");
    }
    println!(
        "{reached} of {} ordinary target ranges reached by {} runs",
        targets.len(),
        runs.len()
    );
    Ok(true)
}
