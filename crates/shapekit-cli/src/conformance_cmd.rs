//! `shapekit conformance run|reproduce|draft-issues|file-issues`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Subcommand;
use shapekit_conformance::{issues, reproduce, runner};
use shapekit_manifest::EntryKind;

// requests and responses with deep XDR-JSON are parsed and dropped recursively
const STACK_BYTES: usize = 512 << 20;

#[derive(Subcommand)]
pub enum ConformanceCommand {
    /// Run corpus entries through SDK testees and write the divergence matrix.
    Run {
        /// SDK to run (rust, go, js); repeatable. All SDKs in versions.toml when absent.
        #[arg(long = "sdk")]
        sdks: Vec<String>,
        /// Exact SDK version to run; repeatable, needs exactly one --sdk.
        #[arg(long = "version")]
        versions: Vec<String>,
        /// Run every version in versions.toml instead of the current one.
        #[arg(long)]
        all_versions: bool,
        /// Entry id to run; repeatable.
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
        /// Run the selection this many times in one invocation and require equal results.
        #[arg(long, default_value_t = 1)]
        passes: u32,
        #[arg(long, default_value = "corpus/manifest.json")]
        manifest: PathBuf,
        #[arg(long, default_value = "conformance/versions.toml")]
        versions_file: PathBuf,
        #[arg(long, default_value = "conformance/known-failures")]
        known_failures: PathBuf,
        /// Directory for divergence-matrix.json and MATRIX.md.
        #[arg(long, default_value = "target/conformance")]
        out: PathBuf,
    },
    /// Check every sighting in reproductions.toml fails before its fix and passes after it.
    Reproduce {
        /// Only rows for this SDK; repeatable.
        #[arg(long = "sdk")]
        sdks: Vec<String>,
        #[arg(long)]
        no_build: bool,
        #[arg(long, default_value = "corpus/manifest.json")]
        manifest: PathBuf,
        #[arg(long, default_value = "conformance/versions.toml")]
        versions_file: PathBuf,
        #[arg(long, default_value = "conformance/reproductions.toml")]
        reproductions: PathBuf,
    },
    /// Write one issue body per SDK and diverging entry from a divergence matrix.
    DraftIssues {
        #[arg(long, default_value = "target/conformance/divergence-matrix.json")]
        matrix: PathBuf,
        #[arg(long, default_value = "corpus/manifest.json")]
        manifest: PathBuf,
        #[arg(long, default_value = "conformance/versions.toml")]
        versions_file: PathBuf,
        #[arg(long, default_value = "findings/issues")]
        out: PathBuf,
    },
    /// Open the drafted issues upstream with gh. Needs the repo owner's approval per batch.
    FileIssues {
        #[arg(long, default_value = "findings/issues")]
        issues: PathBuf,
        /// Only these drafted files; repeatable.
        #[arg(long = "only")]
        only: Vec<String>,
        /// Confirms the repo owner approved filing this batch upstream.
        #[arg(long = "i-have-owner-approval")]
        owner_approval: bool,
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
            passes,
            manifest,
            versions_file,
            known_failures,
            out,
        } => {
            let outcome = runner::run(&runner::RunOptions {
                root,
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
            })?;
            for (key, counts) in &outcome.matrix.summary {
                let parts: Vec<String> = counts.iter().map(|(r, n)| format!("{r} {n}")).collect();
                println!("{key}: {}", parts.join(", "));
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
            Ok(outcome.ok)
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
        ConformanceCommand::DraftIssues {
            matrix,
            manifest,
            versions_file,
            out,
        } => {
            let report = issues::draft(&issues::DraftOptions {
                matrix,
                manifest,
                versions: versions_file,
                out: out.clone(),
            })?;
            for f in &report.removed {
                println!("removed stale draft {}", out.join(f).display());
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
            owner_approval,
        } => {
            let filed = issues::file(&issues::FileOptions {
                issues: dir,
                only,
                owner_approval,
            })?;
            println!("{} issues recorded in filed.json", filed.len());
            Ok(true)
        }
    }
}
