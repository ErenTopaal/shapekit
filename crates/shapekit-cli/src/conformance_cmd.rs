//! `shapekit conformance run|crosscheck|reproduce|sync-versions|seed-check|draft-issues|file-issues`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Subcommand;
use shapekit_conformance::{crosscheck, issues, reproduce, runner, seed, sync};
use shapekit_manifest::EntryKind;

// requests and responses with deep XDR-JSON are parsed and dropped recursively
const STACK_BYTES: usize = 512 << 20;

#[derive(Subcommand)]
pub enum ConformanceCommand {
    /// Run corpus entries through SDK testees and write the divergence matrix.
    Run {
        /// SDK to run, a key of versions.toml such as rust, go or js; repeatable. All SDKs in
        /// versions.toml when absent.
        #[arg(long = "sdk")]
        sdks: Vec<String>,
        /// Exact SDK version to run; repeatable, needs exactly one --sdk.
        #[arg(long = "version")]
        versions: Vec<String>,
        /// Run every version in versions.toml instead of the current one.
        #[arg(long)]
        all_versions: bool,
        /// Entry id to run; repeatable. Every entry when absent.
        #[arg(long = "entry")]
        entries: Vec<String>,
        /// Entry kind to run; repeatable.
        #[arg(long = "kind", value_parser = parse_kind)]
        kinds: Vec<EntryKind>,
        /// Run only entries carrying one of these flags; repeatable.
        #[arg(long = "flag")]
        flags: Vec<String>,
        /// Run only entries linked to one of these issues; repeatable.
        #[arg(long = "linked-issue")]
        linked_issues: Vec<String>,
        /// Skip the build steps in versions.toml (the testees must already be built).
        #[arg(long)]
        no_build: bool,
        /// Run every distinct TransactionMeta of a dataset directory (index.jsonl and meta/,
        /// as `shapekit coretest convert` writes them) instead of the corpus manifest. The
        /// dataset run reads no manifest or known-failures list, runs once and writes its own
        /// obligations, so those options cannot be given with it.
        #[arg(long, conflicts_with_all = ["entries", "kinds", "flags", "linked_issues", "coverage", "obligations", "passes", "manifest", "known_failures"])]
        dataset: Option<PathBuf>,
        /// With --dataset: the candidate lane failing metas are written to.
        #[arg(long, default_value = "candidate")]
        candidate: PathBuf,
        /// Run the coverage builds of the testees and write what each covered to
        /// coverage.json next to the matrix.
        #[arg(long)]
        coverage: bool,
        /// With --all-versions over the whole corpus: write each sdk's `sdk-version` obligations
        /// to conformance/sdk-versions/<sdk>.obligations.json.
        #[arg(long, requires = "all_versions", conflicts_with_all = ["entries", "kinds", "flags", "linked_issues", "coverage", "versions"])]
        obligations: bool,
        /// Run the selection this many times in one invocation and require equal results.
        #[arg(long, default_value_t = 1)]
        passes: u32,
        /// The corpus manifest; entry files are resolved next to it.
        #[arg(long, default_value = "corpus/manifest.json")]
        manifest: PathBuf,
        /// The SDK versions and how to build and start each testee.
        #[arg(long, default_value = "conformance/versions.toml")]
        versions_file: PathBuf,
        /// Directory of <sdk>.txt files listing the known failures.
        #[arg(long, default_value = "conformance/known-failures")]
        known_failures: PathBuf,
        /// Directory for divergence-matrix.json and MATRIX.md.
        #[arg(long, default_value = "target/conformance")]
        out: PathBuf,
    },
    /// Compare every expected_json (stellar-xdr's own rendering) with independent decoders.
    Crosscheck {
        /// SDK whose current version decodes; repeatable. go and js when absent.
        #[arg(long = "sdk")]
        sdks: Vec<String>,
        /// Entry id to check; repeatable.
        #[arg(long = "entry")]
        entries: Vec<String>,
        /// Skip the build steps in versions.toml (the testees must already be built).
        #[arg(long)]
        no_build: bool,
        /// The corpus manifest; entry files are resolved next to it.
        #[arg(long, default_value = "corpus/manifest.json")]
        manifest: PathBuf,
        /// The SDK versions and how to build and start each testee.
        #[arg(long, default_value = "conformance/versions.toml")]
        versions_file: PathBuf,
        /// Directory of <sdk>.txt files listing the known failures.
        #[arg(long, default_value = "conformance/known-failures")]
        known_failures: PathBuf,
        /// Directory for crosscheck.json and CROSSCHECK.md.
        #[arg(long, default_value = "target/conformance/crosscheck")]
        out: PathBuf,
    },
    /// Check every sighting in reproductions.toml fails before its fix and passes after it.
    Reproduce {
        /// Only rows for this SDK; repeatable.
        #[arg(long = "sdk")]
        sdks: Vec<String>,
        /// Skip the build steps in versions.toml (the testees must already be built).
        #[arg(long)]
        no_build: bool,
        /// The corpus manifest; entry files are resolved next to it.
        #[arg(long, default_value = "corpus/manifest.json")]
        manifest: PathBuf,
        /// The SDK versions and how to build and start each testee.
        #[arg(long, default_value = "conformance/versions.toml")]
        versions_file: PathBuf,
        /// The upstream sightings, each with the SDK versions before and after its fix.
        #[arg(long, default_value = "conformance/reproductions.toml")]
        reproductions: PathBuf,
    },
    /// Derive the SDK version matrix and the testee version files from the package registries.
    SyncVersions {
        /// Write nothing; fail when versions.toml or the testee files are not up to date.
        #[arg(long)]
        check: bool,
        /// With --check: on drift, also write the report as markdown to this file.
        #[arg(long, requires = "check")]
        report: Option<PathBuf>,
        /// The SDK versions and how to build and start each testee.
        #[arg(long, default_value = "conformance/versions.toml")]
        versions_file: PathBuf,
        /// The upstream sightings, each with the SDK versions before and after its fix.
        #[arg(long, default_value = "conformance/reproductions.toml")]
        reproductions: PathBuf,
        /// Directory of <sdk>.txt files listing the known failures.
        #[arg(long, default_value = "conformance/known-failures")]
        known_failures: PathBuf,
    },
    /// Seed faults into an SDK with its language's mutation tool and count how many the
    /// corpus catches.
    SeedCheck {
        /// SDK to seed: rust, go or js.
        #[arg(long)]
        sdk: String,
        /// SDK version; the current one in versions.toml when absent.
        #[arg(long)]
        version: Option<String>,
        /// How many of the generated faults to run.
        #[arg(long, default_value_t = 100)]
        sample: usize,
        /// Seed of the sample.
        #[arg(long, default_value_t = 20261116)]
        seed: u64,
        /// Faults run at the same time, where the tool can run several.
        #[arg(long, default_value_t = 2)]
        jobs: usize,
        /// Skip the testee build steps in versions.toml.
        #[arg(long)]
        no_build: bool,
        /// Work directory for sdk copies, tools and builds.
        #[arg(long, default_value = "target/seed-check")]
        work: PathBuf,
        /// Where <sdk>.json, <sdk>.obligations.json and SEED-CHECK.md go.
        #[arg(long, default_value = "conformance/seed-check")]
        out: PathBuf,
        /// Also run these candidate lane entries in every probe (with --candidate-flag): a
        /// fault only they catch counts as caught by the lane, apart from the corpus.
        #[arg(long)]
        candidate: Option<PathBuf>,
        /// Flag the candidate lane entries must carry; repeatable.
        #[arg(long = "candidate-flag")]
        candidate_flags: Vec<String>,
        /// The corpus manifest; entry files are resolved next to it.
        #[arg(long, default_value = "corpus/manifest.json")]
        manifest: PathBuf,
        /// The SDK versions and how to build and start each testee.
        #[arg(long, default_value = "conformance/versions.toml")]
        versions_file: PathBuf,
        /// Directory of <sdk>.txt files listing the known failures.
        #[arg(long, default_value = "conformance/known-failures")]
        known_failures: PathBuf,
    },
    /// One seeded fault: run the corpus on a build and compare it with the unmodified one.
    /// Called back by the mutation tools during seed-check; exits 0 when nothing changed.
    #[command(hide = true)]
    SeedProbe {
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        manifest: PathBuf,
        /// Also run these candidate lane entries in every probe (with --candidate-flag): a
        /// fault only they catch counts as caught by the lane, apart from the corpus.
        #[arg(long)]
        candidate: Option<PathBuf>,
        /// Flag the candidate lane entries must carry; repeatable.
        #[arg(long = "candidate-flag")]
        candidate_flags: Vec<String>,
        #[arg(long)]
        versions_file: PathBuf,
        #[arg(long)]
        known_failures: PathBuf,
        #[arg(long)]
        sdk: String,
        #[arg(long)]
        version: String,
        /// The testee command line, one argument per flag.
        #[arg(long = "command", required = true)]
        command: Vec<String>,
        #[arg(long)]
        baseline: PathBuf,
        /// Record file.
        #[arg(long, conflicts_with = "record_dir")]
        record: Option<PathBuf>,
        /// Record directory; the file is named by the environment variable --label-env.
        #[arg(long, requires = "label_env")]
        record_dir: Option<PathBuf>,
        /// Record directory whose records are numbered in the order the probes run:
        /// `<n>.json`, n the count of records already there.
        #[arg(long, conflicts_with_all = ["record", "record_dir"])]
        record_next: Option<PathBuf>,
        #[arg(long)]
        label_env: Option<String>,
    },
    /// go-mutesting's exec command during seed-check.
    #[command(hide = true)]
    SeedGoExec {
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        manifest: PathBuf,
        /// Also run these candidate lane entries in every probe (with --candidate-flag): a
        /// fault only they catch counts as caught by the lane, apart from the corpus.
        #[arg(long)]
        candidate: Option<PathBuf>,
        /// Flag the candidate lane entries must carry; repeatable.
        #[arg(long = "candidate-flag")]
        candidate_flags: Vec<String>,
        #[arg(long)]
        versions_file: PathBuf,
        #[arg(long)]
        known_failures: PathBuf,
        #[arg(long)]
        setup: PathBuf,
    },
    /// Write one issue body per SDK and diverging entry from a divergence matrix.
    DraftIssues {
        /// The divergence matrix of a `conformance run`.
        #[arg(long, default_value = "target/conformance/divergence-matrix.json")]
        matrix: PathBuf,
        /// The corpus manifest; entry files are resolved next to it.
        #[arg(long, default_value = "corpus/manifest.json")]
        manifest: PathBuf,
        /// The SDK versions and how to build and start each testee.
        #[arg(long, default_value = "conformance/versions.toml")]
        versions_file: PathBuf,
        /// The known-failures lists of every sdk: a draft names as siblings the sdks whose list
        /// holds a divergence on its entry, beside those diverging in the matrix.
        #[arg(long, default_value = "conformance/known-failures")]
        known_failures: PathBuf,
        /// Findings triages whose findings name drafts under --out by `draft`; a draft carries
        /// its finding's title and cause. Repeatable.
        #[arg(
            long = "triage",
            default_values = ["findings/triage.toml", "candidate/findings/triage.toml"]
        )]
        triages: Vec<PathBuf>,
        /// Directory the issue bodies are written into.
        #[arg(long, default_value = "findings/issues")]
        out: PathBuf,
        /// Write only the drafts a finding of the triages names, and remove the other drafts
        /// of entries the matrix ran.
        #[arg(long)]
        named_only: bool,
    },
    /// Open the drafted issues upstream with gh. Refuses to run without --confirm.
    FileIssues {
        /// Directory holding the drafted issue bodies (`conformance draft-issues --out`).
        #[arg(long, default_value = "findings/issues")]
        issues: PathBuf,
        /// Only these drafted files; repeatable.
        #[arg(long = "only")]
        only: Vec<String>,
        /// Confirms that this batch of drafts was reviewed and is to be filed upstream.
        #[arg(long)]
        confirm: bool,
    },
}

fn parse_kind(s: &str) -> std::result::Result<EntryKind, String> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).map_err(|_| {
        format!("unknown kind {s}; one of xdr, ordering, strkey, textual-id, native-projection")
    })
}

pub fn run(cmd: ConformanceCommand) -> Result<bool> {
    std::thread::Builder::new()
        .name("conformance".into())
        .stack_size(STACK_BYTES)
        .spawn(move || dispatch(cmd))
        .context("starting the conformance thread")?
        .join()
        .map_err(|_| anyhow::anyhow!("the conformance thread panicked"))?
}

fn dispatch(cmd: ConformanceCommand) -> Result<bool> {
    let root = std::env::current_dir().context("reading the working directory")?;
    match cmd {
        ConformanceCommand::Run {
            sdks,
            versions,
            all_versions,
            entries,
            kinds,
            flags,
            linked_issues,
            no_build,
            dataset,
            candidate,
            coverage,
            obligations,
            passes,
            manifest,
            versions_file,
            known_failures,
            out,
        } => {
            if let Some(dataset) = dataset {
                let report = shapekit_conformance::dataset::run(
                    &shapekit_conformance::dataset::DatasetOptions {
                        root: root.clone(),
                        dataset,
                        versions: versions_file,
                        sdks,
                        only_versions: versions,
                        all_versions,
                        build: !no_build,
                        out: out.clone(),
                        candidate: root.join(candidate),
                    },
                )?;
                print!("{}", shapekit_conformance::dataset::render(&report));
                println!(
                    "wrote {}, DATASET.md and core-test-sections.obligations.json",
                    out.join("dataset-report.json").display()
                );
                return Ok(true);
            }
            let versions_path = versions_file.clone();
            let run_sdks = sdks.clone();
            let outcome = runner::run(&runner::RunOptions {
                root: root.clone(),
                manifest,
                versions: versions_file,
                known_dir: known_failures,
                sdks,
                only_versions: versions,
                all_versions,
                selection: runner::Selection {
                    ids: entries,
                    kinds,
                    flags,
                    linked_issues,
                },
                build: !no_build,
                out: Some(out.clone()),
                passes,
                command: None,
                coverage,
            })?;
            // entries count as their worst check, so the check totals go next to them
            let line = |counts: &std::collections::BTreeMap<_, usize>| {
                counts
                    .iter()
                    .map(|(r, n)| format!("{r} {n}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            for (key, counts) in &outcome.matrix.entry_summary {
                let checks = outcome
                    .matrix
                    .check_summary
                    .get(key)
                    .map(line)
                    .unwrap_or_default();
                let entries = counts
                    .iter()
                    .map(|(c, n)| format!("{} {n}", c.label()))
                    .collect::<Vec<_>>()
                    .join(", ");
                println!("{key}: entries {entries}; checks {checks}");
            }
            for (i, kib) in outcome.peak_rss_kib.iter().enumerate() {
                println!("pass {}: runner peak rss {kib} KiB", i + 1);
            }
            for p in &outcome.matrix.known_failure_problems {
                println!("known-failures: {p}");
            }
            println!(
                "wrote {} and {}",
                out.join("divergence-matrix.json").display(),
                out.join("MATRIX.md").display()
            );
            if obligations {
                let file = shapekit_conformance::versions::VersionsFile::load(&versions_path)?;
                let sdks: Vec<String> = if run_sdks.is_empty() {
                    file.sdk.keys().cloned().collect()
                } else {
                    run_sdks
                };
                for sdk in &sdks {
                    let ledger =
                        shapekit_conformance::sdk_versions::obligations(&outcome, &file, sdk)?;
                    let path = root.join(shapekit_conformance::sdk_versions::fragment_path(sdk));
                    if let Some(dir) = path.parent() {
                        std::fs::create_dir_all(dir)?;
                    }
                    seed::write_json(&path, &ledger)?;
                    let met = ledger
                        .obligations
                        .iter()
                        .filter(|o| {
                            o.status == shapekit_contracts::obligations::ObligationStatus::Met
                        })
                        .count();
                    println!(
                        "{sdk}: {met} of {} versions met; wrote {}",
                        ledger.obligations.len(),
                        path.display()
                    );
                }
            }
            Ok(outcome.ok)
        }
        ConformanceCommand::Crosscheck {
            sdks,
            entries,
            no_build,
            manifest,
            versions_file,
            known_failures,
            out,
        } => {
            let report = crosscheck::run(&crosscheck::CrosscheckOptions {
                root,
                manifest,
                versions: versions_file,
                known_dir: known_failures,
                sdks,
                selection: runner::Selection {
                    ids: entries,
                    ..Default::default()
                },
                build: !no_build,
                out: Some(out.clone()),
            })?;
            println!(
                "{} decode_json checks against {}",
                report.checks,
                report.decoders.join(", ")
            );
            for (n, c) in &report.confirmed_by {
                println!("confirmed by {n} of {}: {c}", report.decoders.len());
            }
            for d in &report.disagreements {
                println!("{} `{}`: {}", d.entry, d.check, d.class.as_str());
            }
            println!(
                "wrote {} and {}",
                out.join("crosscheck.json").display(),
                out.join("CROSSCHECK.md").display()
            );
            let problems = report.problems();
            for p in &problems {
                eprintln!("crosscheck fails: {p}");
            }
            Ok(problems.is_empty())
        }
        ConformanceCommand::Reproduce {
            sdks,
            no_build,
            manifest,
            versions_file,
            reproductions,
        } => {
            let cells = reproduce::run(&reproduce::ReproduceOptions {
                root,
                manifest,
                versions: versions_file,
                reproductions,
                sdks,
                build: !no_build,
            })?;
            print!("{}", reproduce::render(&cells));
            Ok(cells.iter().all(reproduce::Cell::holds))
        }
        ConformanceCommand::SyncVersions {
            check,
            report,
            versions_file,
            reproductions,
            known_failures,
        } => {
            let (old, plans) = sync::discover(&sync::SyncOptions {
                root: root.clone(),
                versions: versions_file.clone(),
                reproductions,
                known_dir: known_failures,
            })?;
            print!("{}", sync::summary(&plans));
            let rendered = sync::render(&old, &plans)?;
            if check {
                let drift = sync::drift(&root, &versions_file, &old, &plans, &rendered)?;
                if drift.is_empty() {
                    println!(
                        "{} and the testee files are up to date",
                        versions_file.display()
                    );
                    return Ok(true);
                }
                let mut md =
                    String::from("`shapekit conformance sync-versions --check` found:\n\n");
                for l in &drift.lines {
                    md.push_str(&format!("- {l}\n"));
                }
                if drift.file_changed {
                    md.push_str(&format!(
                        "- {} is not what sync-versions writes\n",
                        versions_file.display()
                    ));
                }
                md.push_str("\nRun `shapekit conformance sync-versions` to update them.\n");
                print!("\n{md}");
                if let Some(path) = report {
                    std::fs::write(&path, &md)
                        .with_context(|| format!("writing {}", path.display()))?;
                }
                return Ok(false);
            }
            for line in sync::apply(&root, &versions_file, &old, &plans, &rendered)? {
                println!("{line}");
            }
            Ok(true)
        }
        ConformanceCommand::SeedCheck {
            sdk,
            version,
            sample,
            seed,
            jobs,
            no_build,
            work,
            out,
            candidate,
            candidate_flags,
            manifest,
            versions_file,
            known_failures,
        } => {
            let paths = seed::Paths::new(&root, &manifest, &versions_file, &known_failures)?
                .with_lane(candidate.as_deref(), &candidate_flags)?;
            let ctx = seed::Context::new(
                paths,
                &sdk,
                version.as_deref(),
                &work,
                seed::Settings {
                    sample,
                    seed,
                    build: !no_build,
                    jobs,
                },
            )?;
            let mut report = seed::check(&ctx)?;
            report.lane = ctx.lane_note()?;
            let out = root.join(out);
            let unclassified = seed::classify(&mut report, &out.join("classes"))?;
            seed::write_json(&out.join(format!("{sdk}.json")), &report)?;
            seed::write_json(
                &out.join(format!("{sdk}.obligations.json")),
                &seed::obligations(&report),
            )?;
            let mut reports = Vec::new();
            for s in ["rust", "go", "js"] {
                let p = out.join(format!("{s}.json"));
                if p.is_file() {
                    reports.push(seed::read_json::<seed::Report>(&p)?);
                }
            }
            std::fs::write(out.join("SEED-CHECK.md"), seed::render_md(&reports))
                .context("writing SEED-CHECK.md")?;
            let (caught, built) = report.rate();
            println!(
                "{sdk} {}: {} faults generated by {} {}, {} sampled, {caught} of {built} that built caught",
                report.version,
                report.population,
                report.tool.name,
                report.tool.version,
                report.sample
            );
            for (s, n) in &report.counts {
                println!("  {s:?}: {n}");
            }
            let (alone, _) = report.corpus_rate();
            println!("  caught by the corpus alone: {alone}");
            for (class, n) in report.class_counts() {
                println!("  missed by the corpus alone, {class}: {n}");
            }
            if unclassified > 0 {
                println!(
                    "  {unclassified} faults the corpus alone misses have no class in {}",
                    out.join("classes").display()
                );
            }
            for f in &report.real_faults {
                println!(
                    "  released fault {}: {}",
                    f.issue,
                    if f.caught { "caught" } else { "not caught" }
                );
            }
            println!("wrote {}", out.display());
            Ok(true)
        }
        ConformanceCommand::SeedProbe {
            root: probe_root,
            candidate,
            candidate_flags,
            manifest,
            versions_file,
            known_failures,
            sdk,
            version,
            command,
            baseline,
            record,
            record_dir,
            record_next,
            label_env,
        } => {
            let paths = seed::Paths::new(&probe_root, &manifest, &versions_file, &known_failures)?
                .with_lane(candidate.as_deref(), &candidate_flags)?;
            let record = match (record, record_dir, label_env) {
                _ if record_next.is_some() => {
                    let dir = record_next.expect("checked");
                    std::fs::create_dir_all(&dir)?;
                    let n = std::fs::read_dir(&dir)?
                        .filter_map(|e| e.ok())
                        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
                        .count();
                    dir.join(format!("{n}.json"))
                }
                (Some(r), _, _) => r,
                (None, Some(dir), Some(var)) => {
                    // stryker's dry run sets no mutant; it must find the build unchanged
                    let label = std::env::var(&var).ok().filter(|v| !v.is_empty());
                    dir.join(format!("{}.json", label.as_deref().unwrap_or("unmodified")))
                }
                _ => anyhow::bail!(
                    "seed-probe needs --record, --record-next, or --record-dir with --label-env"
                ),
            };
            let outcome = seed::probe(&paths, &sdk, &version, &command, &baseline, &record)?;
            Ok(!outcome.caught())
        }
        ConformanceCommand::SeedGoExec {
            root: exec_root,
            candidate,
            candidate_flags,
            manifest,
            versions_file,
            known_failures,
            setup,
        } => {
            let paths = seed::Paths::new(&exec_root, &manifest, &versions_file, &known_failures)?
                .with_lane(candidate.as_deref(), &candidate_flags)?;
            let code = seed::go::exec(&paths, &setup)?;
            std::process::exit(code);
        }
        ConformanceCommand::DraftIssues {
            matrix,
            manifest,
            versions_file,
            known_failures,
            triages,
            out,
            named_only,
        } => {
            let triaged = draft_triage(&root, &triages, &out)?;
            let report = issues::draft(&issues::DraftOptions {
                matrix,
                manifest,
                versions: versions_file,
                known_failures,
                out: out.clone(),
                causes: triaged.causes,
                sibling_matrices: triaged.matrices,
                claims: triaged.claims,
                findings: triaged.findings,
                named_only,
            })?;
            for f in &report.removed {
                println!("removed stale draft {}", out.join(f).display());
            }
            for (f, why) in &report.kept {
                println!("kept {}: {why}", out.join(f).display());
            }
            for f in &report.written {
                println!("wrote {}", out.join(f).display());
            }
            println!("{} drafts", report.written.len());
            Ok(true)
        }
        ConformanceCommand::FileIssues {
            issues: dir,
            only,
            confirm,
        } => {
            let filed = issues::file(&issues::FileOptions {
                issues: dir,
                only,
                confirm,
            })?;
            println!("{} issues recorded in filed.json", filed.len());
            Ok(true)
        }
    }
}

/// What the findings triages tell the drafts: the cause of every draft under `out` by file name,
/// the matrices of every sdk, and which finding takes each divergence.
struct Triaged {
    causes: std::collections::BTreeMap<String, issues::Cause>,
    matrices: Vec<PathBuf>,
    claims: std::collections::BTreeMap<(String, String), std::collections::BTreeSet<String>>,
    findings: std::collections::BTreeMap<String, issues::Claim>,
}

/// Reads every triage and collects its findings over its inputs, as `findings collect` does,
/// so a draft cites what the triage says now rather than a findings.json written earlier.
fn draft_triage(root: &Path, triages: &[PathBuf], out: &Path) -> Result<Triaged> {
    let normal = |p: &Path| -> PathBuf {
        p.components()
            .filter(|c| !matches!(c, std::path::Component::CurDir))
            .collect()
    };
    let out = normal(out);
    let mut t = Triaged {
        causes: Default::default(),
        matrices: Vec::new(),
        claims: Default::default(),
        findings: Default::default(),
    };
    for path in triages {
        let triage = shapekit_findings::triage::load(path)?;
        t.matrices
            .extend(triage.inputs.matrices.iter().map(|m| root.join(m)));
        let collection = shapekit_findings::collect::collect(root, &triage)
            .with_context(|| format!("collecting the findings of {}", path.display()))?;
        for r in &collection.findings {
            t.findings.insert(
                r.id.clone(),
                issues::Claim {
                    title: r.title.clone(),
                    component: r.component.clone(),
                    status: r.status.name().to_string(),
                    link: r.link.clone(),
                },
            );
            for o in r.occurrences.iter().filter(|o| o.source == "conformance") {
                let Some(entry) = &o.entry else { continue };
                let sdk = o.on.split_once('@').map_or(o.on.as_str(), |(s, _)| s);
                t.claims
                    .entry((sdk.to_string(), entry.clone()))
                    .or_default()
                    .insert(r.id.clone());
            }
        }
        for f in &triage.findings {
            let Some(draft) = &f.draft else { continue };
            let draft = normal(Path::new(draft));
            if draft.parent() != Some(out.as_path()) {
                continue;
            }
            let Some(name) = draft.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let record = collection.findings.iter().find(|r| r.id == f.id);
            // the draft's sdk is the conformance matcher's whose name starts the file name
            let sdk = f
                .conformance
                .iter()
                .map(|m| m.sdk.as_str())
                .filter(|s| name.starts_with(&format!("{s}-")))
                .max_by_key(|s| s.len());
            let affected = record.and_then(|r| {
                r.affected
                    .iter()
                    .find(|a| Some(a.on.as_str()) == sdk)
                    .cloned()
            });
            let entries: Vec<String> = match (record, sdk) {
                (Some(r), Some(sdk)) => {
                    let mut es: Vec<String> = r
                        .occurrences
                        .iter()
                        .filter(|o| o.source == "conformance")
                        .filter(|o| o.on.split_once('@').map(|(s, _)| s) == Some(sdk))
                        .filter_map(|o| o.entry.clone())
                        .collect();
                    es.sort();
                    es.dedup();
                    es
                }
                _ => Vec::new(),
            };
            // a matcher of the sdk without check patterns takes every check
            let own: Vec<_> = f
                .conformance
                .iter()
                .filter(|m| Some(m.sdk.as_str()) == sdk)
                .collect();
            let checks: Vec<String> = if own.iter().any(|m| m.checks.is_empty()) {
                Vec::new()
            } else {
                own.iter().flat_map(|m| m.checks.iter().cloned()).collect()
            };
            let cause = issues::Cause {
                finding: f.id.clone(),
                title: f.title.clone(),
                cause: f.cause.clone(),
                entries,
                tested: affected
                    .as_ref()
                    .map(|a| a.tested.clone())
                    .unwrap_or_default(),
                failing: affected.map(|a| a.failing).unwrap_or_default(),
                checks,
            };
            if let Some(other) = t.causes.insert(name.to_owned(), cause) {
                anyhow::bail!(
                    "findings {} and {} both name the draft {}",
                    other.finding,
                    f.id,
                    draft.display()
                );
            }
        }
    }
    Ok(t)
}
