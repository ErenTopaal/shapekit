//! The `shapekit` command line tool.

mod capture_cmd;
mod conformance_cmd;
mod contracts_cmd;
mod coverage_cmd;
mod findings_cmd;
mod fuzz_cmd;
mod gen_cmd;
mod golden_cmd;
mod guide_cmd;
mod harness_cmd;
mod laws_cmd;
mod manifest_cmd;
mod oracle_cmd;
mod pack_cmd;
mod pins;
mod promote_cmd;
mod registry_cmd;
mod schema_cmd;
mod serve_cmd;
mod site_cmd;
mod synth_cmd;
mod twin_cmd;
mod vectors_cmd;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "shapekit",
    version,
    about = "XDR shape corpus, replay harness and SDK conformance tooling for Stellar"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Work with pins.toml, the list of every pinned upstream reference.
    Pins {
        #[command(subcommand)]
        command: PinsCommand,
    },
    /// Work with the shape, field, cap and lint-rule registry.
    Registry {
        #[command(subcommand)]
        command: RegistryCommand,
    },
    /// Work with the corpus manifest.
    Manifest {
        #[command(subcommand)]
        command: ManifestCommand,
    },
    /// Run corpus entries through SDK testees: the divergence matrix, reproductions of
    /// upstream sightings, and issue drafting.
    Conformance {
        #[command(subcommand)]
        command: conformance_cmd::ConformanceCommand,
    },
    /// The meta oracle: what each ledger changed, read from LedgerCloseMeta only.
    Oracle {
        #[command(subcommand)]
        command: OracleCommand,
    },
    /// Import upstream corpus sources.
    Import {
        #[command(subcommand)]
        command: ImportCommand,
    },
    /// Checks over imported core goldens.
    Goldens {
        #[command(subcommand)]
        command: GoldensCommand,
    },
    /// Capture entries from public network history (online).
    Capture {
        #[command(subcommand)]
        command: CaptureCommand,
    },
    /// Record chains on a private standalone stellar-core (needs docker and
    /// the pinned core image).
    Gen {
        #[command(subcommand)]
        command: GenCommand,
    },
    /// Hand-crafted synthetic entries and the non-xdr vector kinds.
    Synth {
        #[command(subcommand)]
        command: SynthCommand,
    },
    /// The indexer-hardening guide: generated tables and their checks.
    Guide {
        #[command(subcommand)]
        command: guide_cmd::GuideCommand,
    },
    /// Chains in their committed form: SEP-54 lake, archive tar, verification.
    Pack {
        #[command(subcommand)]
        command: pack_cmd::PackCommand,
    },
    /// Replay one recorded chain over HTTP: the SEP-54 lake, the history archive and a
    /// read-only JSON-RPC endpoint shaped like stellar-rpc.
    Serve(serve_cmd::ServeArgs),
    /// Replay chains into an indexer through an adapter and judge what it stored against the
    /// meta oracle.
    Harness {
        #[command(subcommand)]
        command: harness_cmd::HarnessCommand,
    },
    /// Findings: every harness, conformance and linter failure mapped to a triaged root cause
    /// with its upstream status.
    Findings {
        #[command(subcommand)]
        command: findings_cmd::FindingsCommand,
    },
    /// Differential decode and re-encode fuzzing, stellar-xdr in process against the go
    /// testee, seeded from the corpus.
    Fuzz(fuzz_cmd::FuzzArgs),
    /// Per-CAP coverage of the corpus: writes corpus/coverage/coverage.json and COVERAGE.md.
    Coverage(coverage_cmd::CoverageArgs),
    /// The static site in site/: its JSON data, derived from committed files only.
    Site {
        #[command(subcommand)]
        command: site_cmd::SiteCommand,
    },
    /// The coverage twin of stellar-core: protocol gates.
    Twin {
        #[command(subcommand)]
        command: twin_cmd::TwinCommand,
    },
    /// The law book: laws every ledger satisfies, anchored on the network's own hashes.
    Laws {
        #[command(subcommand)]
        command: laws_cmd::LawsCommand,
    },
    /// Conformance vectors derived from the schema and the representation gap tables.
    Vectors {
        #[command(subcommand)]
        command: vectors_cmd::VectorsCommand,
    },
    /// The stellar-xdr definitions in schema/xdr/ as data.
    Schema {
        #[command(subcommand)]
        command: schema_cmd::SchemaCommand,
    },
    /// The data formats the generators and checks exchange, and their schemas.
    Contracts {
        #[command(subcommand)]
        command: contracts_cmd::ContractsCommand,
    },
    /// The corpus as a release asset.
    Corpus {
        #[command(subcommand)]
        command: coverage_cmd::CorpusCommand,
    },
}

#[derive(Subcommand)]
enum SynthCommand {
    /// Write every synthetic entry and vector, each checked against its reference
    /// implementation, and remove synthetic files this build no longer produces.
    Build {
        /// The corpus directory.
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
        /// Directory holding the registry files.
        #[arg(long, default_value = "registry")]
        registry: PathBuf,
    },
}

#[derive(Subcommand)]
enum GenCommand {
    /// Print the family of every recipe `gen chains` can record, one per line, sorted and
    /// without repeats.
    Families,
    /// Record the smoke chain on a standalone stellar-core from the core image in pins.toml:
    /// an upgrade to the protocol in the `PROTOCOL` constant of shapekit-core-driver's smoke
    /// module, testnet's Soroban settings, four funded accounts, a payment, a trustline and one
    /// echo contract call.
    Smoke {
        /// Passphrase the chain runs under.
        #[arg(long, value_parser = ["testnet", "mainnet"])]
        network: String,
        /// Directory to write the chain into; must not exist.
        #[arg(long)]
        out: PathBuf,
        /// pins.toml, read for the core image.
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
    },
    /// Record the recipe chains, one per family and protocol, and install
    /// their chains and core-emitted entries into the corpus.
    Chains {
        /// Only the recipes of this family; repeatable. All when absent.
        #[arg(long)]
        family: Vec<String>,
        /// Passphrases to record under. A single network keeps the other
        /// network's committed variant of each entry.
        #[arg(long, default_value = "both", value_parser = ["testnet", "mainnet", "both"])]
        network: String,
        /// Chains recorded at once, each on its own core container.
        #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u16).range(1..=32))]
        jobs: u16,
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
        #[arg(long, default_value = "registry")]
        registry: PathBuf,
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
    },
    /// Record a chain with no transactions through --last, so its history archive holds
    /// checkpoints, and write it in the committed form.
    Checkpoints {
        #[arg(long, value_parser = ["testnet", "mainnet"])]
        network: String,
        #[arg(long, default_value_t = 30)]
        protocol: u32,
        /// Last ledger; checkpoints end at 63, 127, 191 and so on.
        #[arg(long, default_value_t = 127)]
        last: u32,
        /// Directory to write the chain into; must not exist.
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
    },
    /// Compare core's raw ledgers.xdr of every chain in two independent recordings: each
    /// ledger key's changes must come in the same order in both, and the recordings may
    /// differ only in the order between keys. This is the evidence the key-stable canonical
    /// form rests on. Record the second set with `gen chains --corpus <copy>`.
    VerifyRawOrder {
        /// The corpus holding the second recording.
        #[arg(long)]
        other: PathBuf,
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
    },
    /// Record the chain of a recipe IR file and compare it with the committed corpus: the
    /// chain's canonical hashes per passphrase and, with both, every entry it owns.
    RunIr {
        /// The recipe file, corpus/recipes/<family>/<name>.json.
        file: PathBuf,
        /// testnet, mainnet or both.
        #[arg(long, default_value = "both")]
        network: String,
        #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u16).range(1..=32))]
        jobs: u16,
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
        #[arg(long, default_value = "registry")]
        registry: PathBuf,
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
    },
    /// Regenerate the recipe chains under both passphrases into a temporary
    /// directory and compare canonical hashes and entry files with the corpus.
    VerifyRepro {
        /// Only the recipes of this family; repeatable. All when absent.
        #[arg(long)]
        family: Vec<String>,
        #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u16).range(1..=32))]
        jobs: u16,
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
        #[arg(long, default_value = "registry")]
        registry: PathBuf,
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
    },
}

#[derive(Subcommand)]
enum RegistryCommand {
    /// Check every registry file and the references between them.
    Check {
        /// Directory holding the registry files.
        #[arg(long, default_value = "registry")]
        dir: PathBuf,
    },
}

#[derive(Subcommand)]
enum ManifestCommand {
    /// Check the manifest against the registry and the corpus files.
    Validate {
        /// Path to the manifest; entry files are resolved next to it.
        #[arg(long, default_value = "corpus/manifest.json")]
        file: PathBuf,
        /// Directory holding the registry files.
        #[arg(long, default_value = "registry")]
        registry: PathBuf,
    },
    /// Rebuild corpus/manifest.json from the entry and vector files.
    Build {
        /// The corpus directory.
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
        /// pins.toml, read for the header.
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
        /// Version written into the header.
        #[arg(long, default_value = env!("CARGO_PKG_VERSION"))]
        corpus_version: String,
    },
    /// Write the manifest JSON Schema generated from the manifest types.
    Schema {
        #[arg(long, default_value = "schema/manifest.schema.json")]
        out: PathBuf,
    },
}

#[derive(Subcommand)]
enum ImportCommand {
    /// Index stellar-core's --check-lcm goldens and import the smallest golden ledger per
    /// family fragment as core-golden entries.
    Goldens {
        /// A git checkout of stellar-core at the tag, holding the --tier directory.
        #[arg(long)]
        core_src: PathBuf,
        #[arg(long, default_value = "v30.0.0")]
        tag: String,
        /// Golden tier to read. test-lcm-next is the next protocol's meta and belongs in the
        /// next/ rehearsal tree (--corpus next), never in the release corpus.
        #[arg(long, default_value = "test-lcm-current", value_parser = ["test-lcm-current", "test-lcm-next"])]
        tier: String,
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
        #[arg(long, default_value = "registry")]
        registry: PathBuf,
    },
}

#[derive(Subcommand)]
enum GoldensCommand {
    /// Rescan a stellar-core checkout and require it to match the committed golden index.
    Verify {
        #[arg(long)]
        core_src: PathBuf,
        #[arg(long, default_value = "v30.0.0")]
        tag: String,
        #[arg(long, default_value = "test-lcm-current", value_parser = ["test-lcm-current", "test-lcm-next"])]
        tier: String,
        #[arg(long, default_value = "corpus/goldens/core-v30.0.0/index.json")]
        index: PathBuf,
    },
    /// Structural agreement between golden entries and local-core or history entries.
    Agree {
        #[arg(long, default_value = "corpus/manifest.json")]
        file: PathBuf,
        #[arg(long, default_value = "registry")]
        registry: PathBuf,
    },
}

#[derive(Subcommand)]
enum CaptureCommand {
    /// Capture one ledger's LedgerCloseMeta as a testnet-history or mainnet-history entry.
    History {
        #[arg(long, value_enum)]
        network: capture_cmd::NetworkArg,
        #[arg(long)]
        ledger: u32,
        /// A transaction hash the ledger must contain; recorded with its apply index.
        #[arg(long)]
        tx: Option<String>,
        /// Store the whole ledger, or only the --tx transaction's envelope and result meta.
        #[arg(long, value_enum, default_value = "ledger")]
        slice: capture_cmd::SliceArg,
        /// Registry family; the ledger must match one of its signature fragments.
        #[arg(long)]
        family: String,
        /// Entry slug, giving the id <family>.<slug>.
        #[arg(long)]
        slug: String,
        /// Where the ledger comes from; there is no fallback between the two.
        #[arg(long, value_enum, default_value = "lake")]
        via: capture_cmd::Via,
        /// Data lake prefix, for a testnet that has since been reset.
        #[arg(long)]
        lake_prefix: Option<String>,
        /// stellar-rpc endpoint for --via rpc.
        #[arg(long)]
        rpc_url: Option<String>,
        /// Upstream issue or pull request this capture reproduces (repeatable).
        #[arg(long = "linked-issue")]
        linked_issues: Vec<String>,
        /// Extra provenance text appended to the entry notes.
        #[arg(long)]
        note: Option<String>,
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
        #[arg(long, default_value = "registry")]
        registry: PathBuf,
    },
}

#[derive(Subcommand)]
enum OracleCommand {
    /// Write <variant>.oracle.json next to every eligible corpus entry and record it.
    Run {
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
    },
    /// Apply a record-marked LedgerCloseMeta stream to a ledger state, checking state
    /// consistency, conservation of lumens and interpretation on every ledger.
    Check {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        passphrase: String,
        #[arg(long, value_enum, default_value = "genesis")]
        start: oracle_cmd::StartState,
    },
    /// Print the oracle facts for a LedgerCloseMeta file.
    Interpret {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        passphrase: String,
        /// The file is a record-marked stream of ledgers rather than one bare value.
        #[arg(long)]
        framed: bool,
    },
}

#[derive(Subcommand)]
enum PinsCommand {
    /// Check every pin against its live upstream source.
    Verify {
        /// Path to the pins file.
        #[arg(long, default_value = "pins.toml")]
        file: PathBuf,
        /// Number of checks to run at once.
        #[arg(long, default_value_t = 8, value_parser = clap::value_parser!(u16).range(1..=64))]
        jobs: u16,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Pins {
            command: PinsCommand::Verify { file, jobs },
        } => pins::verify::run(&file, usize::from(jobs)),
        Command::Registry {
            command: RegistryCommand::Check { dir },
        } => registry_cmd::check(&dir),
        Command::Manifest { command } => match command {
            ManifestCommand::Validate { file, registry } => {
                manifest_cmd::validate_file(&file, &registry)
            }
            ManifestCommand::Build {
                corpus,
                pins,
                corpus_version,
            } => manifest_cmd::build_file(&corpus, &pins, &corpus_version),
            ManifestCommand::Schema { out } => manifest_cmd::write_schema(&out),
        },
        Command::Oracle { command } => match command {
            OracleCommand::Run { corpus } => oracle_cmd::run(&corpus),
            OracleCommand::Check {
                file,
                passphrase,
                start,
            } => oracle_cmd::check(&file, &passphrase, start),
            OracleCommand::Interpret {
                file,
                passphrase,
                framed,
            } => oracle_cmd::interpret(&file, &passphrase, framed),
        },
        Command::Gen {
            command: GenCommand::Families,
        } => gen_cmd::families(),
        Command::Gen {
            command: GenCommand::Smoke { network, out, pins },
        } => gen_cmd::smoke(&network, &out, &pins),
        Command::Gen {
            command:
                GenCommand::Checkpoints {
                    network,
                    protocol,
                    last,
                    out,
                    pins,
                },
        } => gen_cmd::checkpoints(&network, protocol, last, &out, &pins),
        Command::Gen {
            command:
                GenCommand::Chains {
                    family,
                    network,
                    jobs,
                    corpus,
                    registry,
                    pins,
                },
        } => gen_cmd::chains(
            &family,
            &network,
            jobs,
            &gen_cmd::GenPaths {
                corpus: &corpus,
                registry: &registry,
                pins: &pins,
            },
        ),
        Command::Gen {
            command: GenCommand::VerifyRawOrder { other, corpus },
        } => gen_cmd::verify_raw_order(&corpus, &other),
        Command::Gen {
            command:
                GenCommand::RunIr {
                    file,
                    network,
                    jobs,
                    corpus,
                    registry,
                    pins,
                },
        } => gen_cmd::run_ir(
            &file,
            &network,
            jobs,
            &gen_cmd::GenPaths {
                corpus: &corpus,
                registry: &registry,
                pins: &pins,
            },
        ),
        Command::Gen {
            command:
                GenCommand::VerifyRepro {
                    family,
                    jobs,
                    corpus,
                    registry,
                    pins,
                },
        } => gen_cmd::verify_repro(
            &family,
            jobs,
            &gen_cmd::GenPaths {
                corpus: &corpus,
                registry: &registry,
                pins: &pins,
            },
        ),
        Command::Synth {
            command: SynthCommand::Build { corpus, registry },
        } => synth_cmd::build(&corpus, &registry),
        Command::Import {
            command:
                ImportCommand::Goldens {
                    core_src,
                    tag,
                    tier,
                    corpus,
                    registry,
                },
        } => golden_cmd::import(&core_src, &tag, &tier, &corpus, &registry),
        Command::Goldens {
            command: GoldensCommand::Agree { file, registry },
        } => golden_cmd::agree(&file, &registry),
        Command::Goldens {
            command:
                GoldensCommand::Verify {
                    core_src,
                    tag,
                    tier,
                    index,
                },
        } => golden_cmd::verify(&core_src, &tag, &tier, &index),
        Command::Capture {
            command:
                CaptureCommand::History {
                    network,
                    ledger,
                    tx,
                    slice,
                    family,
                    slug,
                    via,
                    lake_prefix,
                    rpc_url,
                    linked_issues,
                    note,
                    corpus,
                    registry,
                },
        } => capture_cmd::run(capture_cmd::Args {
            network,
            ledger,
            tx: tx.as_deref(),
            slice,
            family: &family,
            slug: &slug,
            via,
            lake_prefix: lake_prefix.as_deref(),
            rpc_url: rpc_url.as_deref(),
            linked_issues,
            note: note.as_deref(),
            corpus: &corpus,
            registry: &registry,
        }),
        Command::Conformance { command } => conformance_cmd::run(command),
        Command::Guide { command } => guide_cmd::run(command),
        Command::Pack { command } => pack_cmd::run(command),
        Command::Serve(args) => serve_cmd::run(args),
        Command::Harness { command } => harness_cmd::run(command),
        Command::Findings { command } => findings_cmd::run(command),
        Command::Fuzz(args) => fuzz_cmd::run(args),
        Command::Coverage(args) => coverage_cmd::coverage(args),
        Command::Corpus { command } => coverage_cmd::corpus(command),
        Command::Contracts { command } => contracts_cmd::run(command),
        Command::Site { command } => site_cmd::run(command),
        Command::Schema { command } => schema_cmd::run(command),
        Command::Laws { command } => laws_cmd::run(command),
        Command::Twin { command } => twin_cmd::run(command),
        Command::Vectors { command } => vectors_cmd::run(command),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(2)
        }
    }
}
