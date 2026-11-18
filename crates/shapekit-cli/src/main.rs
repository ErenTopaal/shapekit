//! The `shapekit` command line tool.

mod audit_cmd;
mod capture_cmd;
mod check_cmd;
mod conformance_cmd;
mod contracts_cmd;
mod coretest_cmd;
mod coverage_cmd;
mod doctor_cmd;
mod error_text;
mod explore_cmd;
mod findings_cmd;
mod fuzz_cmd;
mod gen_cmd;
mod golden_cmd;
mod guide_cmd;
mod guided_fuzz_cmd;
mod harness_cmd;
mod init_cmd;
mod laws_cmd;
mod lint_cmd;
mod manifest_cmd;
mod native_fuzz_cmd;
mod oracle_cmd;
mod pack_cmd;
mod pins;
mod probe_cmd;
mod promote_cmd;
mod registry_cmd;
mod report_cmd;
mod schema_cmd;
mod serve_cmd;
mod site_cmd;
mod synth_cmd;
mod twin_cmd;
mod vectors_cmd;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand};

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
    /// Run the corpus through SDK testees or replay it into an indexer.
    Check {
        #[command(subcommand)]
        command: check_cmd::CheckCommand,
    },
    /// Run indexerlint over a Go indexer built on go-stellar-sdk.
    Lint(lint_cmd::LintArgs),
    /// Write a self-contained HTML report of a harness or conformance run: failures grouped
    /// by finding, each with its smallest entry, the guide section and a reproduce command.
    Report(report_cmd::ReportArgs),
    /// Write a skeleton adapter (for an indexer) or testee (for an SDK) that speaks the protocol.
    Init {
        #[command(subcommand)]
        command: init_cmd::InitCommand,
    },
    /// Check that docker, the toolchains, disk space and network access are there for check
    /// sdk, check indexer and lint, and say how to install what is missing.
    Doctor(doctor_cmd::DoctorArgs),
    /// List the internal commands: the corpus generator, its checks and the maintenance tools.
    Dev,
    /// Work with pins.toml, the list of every pinned upstream reference.
    #[command(hide = true)]
    Pins {
        #[command(subcommand)]
        command: PinsCommand,
    },
    /// Work with the shape, field, cap and lint-rule registry.
    #[command(hide = true)]
    Registry {
        #[command(subcommand)]
        command: RegistryCommand,
    },
    /// Work with the corpus manifest.
    #[command(hide = true)]
    Manifest {
        #[command(subcommand)]
        command: ManifestCommand,
    },
    /// Run corpus entries through SDK testees: the divergence matrix, reproductions of
    /// upstream sightings, and issue drafting.
    #[command(hide = true)]
    Conformance {
        #[command(subcommand)]
        command: conformance_cmd::ConformanceCommand,
    },
    /// The meta oracle: what each ledger changed, read from LedgerCloseMeta only.
    #[command(hide = true)]
    Oracle {
        #[command(subcommand)]
        command: OracleCommand,
    },
    /// Import upstream corpus sources.
    #[command(hide = true)]
    Import {
        #[command(subcommand)]
        command: ImportCommand,
    },
    /// Checks over imported core goldens.
    #[command(hide = true)]
    Goldens {
        #[command(subcommand)]
        command: GoldensCommand,
    },
    /// Capture entries from public network history (online).
    #[command(hide = true)]
    Capture {
        #[command(subcommand)]
        command: CaptureCommand,
    },
    /// Record chains on a private standalone stellar-core (needs docker and
    /// the pinned core image).
    #[command(hide = true)]
    Gen {
        #[command(subcommand)]
        command: GenCommand,
    },
    /// Hand-crafted synthetic entries and the non-xdr vector kinds.
    #[command(hide = true)]
    Synth {
        #[command(subcommand)]
        command: SynthCommand,
    },
    /// The indexer-hardening guide: generated tables and their checks.
    #[command(hide = true)]
    Guide {
        #[command(subcommand)]
        command: guide_cmd::GuideCommand,
    },
    /// Chains in their committed form: SEP-54 lake, archive tar, verification.
    #[command(hide = true)]
    Pack {
        #[command(subcommand)]
        command: pack_cmd::PackCommand,
    },
    /// Replay one recorded chain over HTTP: the SEP-54 lake, the history archive and a
    /// read-only JSON-RPC endpoint shaped like stellar-rpc.
    Serve(serve_cmd::ServeArgs),
    /// Replay chains into an indexer through an adapter and judge what it stored against the
    /// meta oracle.
    #[command(hide = true)]
    Harness {
        #[command(subcommand)]
        command: harness_cmd::HarnessCommand,
    },
    /// Findings: every harness, conformance and linter failure mapped to a triaged root cause
    /// with its upstream status.
    #[command(hide = true)]
    Findings {
        #[command(subcommand)]
        command: findings_cmd::FindingsCommand,
    },
    /// Differential decode and re-encode fuzzing, stellar-xdr in process against the go
    /// testee, seeded from the corpus.
    #[command(hide = true)]
    Fuzz(fuzz_cmd::FuzzArgs),
    /// Each sdk's own coverage-guided fuzzer on its decoder, checked by the encoding laws.
    #[command(hide = true)]
    NativeFuzz(native_fuzz_cmd::NativeFuzzArgs),
    /// The rust, go and javascript testees fuzzed together, steered by their coverage.
    #[command(hide = true)]
    GuidedFuzz(guided_fuzz_cmd::GuidedFuzzArgs),
    /// Per-CAP coverage of the corpus: writes corpus/coverage/coverage.json and COVERAGE.md.
    #[command(hide = true)]
    Coverage(coverage_cmd::CoverageArgs),
    /// The static site in site/: its JSON data, derived from committed files only.
    #[command(hide = true)]
    Site {
        #[command(subcommand)]
        command: site_cmd::SiteCommand,
    },
    /// The coverage twin of stellar-core: byte equality with the release image, core coverage
    /// of a chain, protocol gates.
    #[command(hide = true)]
    Twin {
        #[command(subcommand)]
        command: twin_cmd::TwinCommand,
    },
    /// The recipe explorer: recipes found by filling transactions from the schema and the
    /// chain's state, kept when core's ledgers show something new.
    #[command(hide = true)]
    Explore {
        #[command(subcommand)]
        command: explore_cmd::ExploreCommand,
    },
    /// The law book: laws every ledger satisfies, anchored on the network's own hashes.
    #[command(hide = true)]
    Laws {
        #[command(subcommand)]
        command: laws_cmd::LawsCommand,
    },
    /// Conformance vectors derived from the schema and the representation gap tables.
    #[command(hide = true)]
    Vectors {
        #[command(subcommand)]
        command: vectors_cmd::VectorsCommand,
    },
    /// The law book over the public ledger lake: random audits of the history and a watch
    /// over every new ledger.
    #[command(hide = true)]
    Audit {
        #[command(subcommand)]
        command: audit_cmd::AuditCommand,
    },
    /// The stellar-xdr definitions in schema/xdr/ as data.
    #[command(hide = true)]
    Schema {
        #[command(subcommand)]
        command: schema_cmd::SchemaCommand,
    },
    /// The data formats the generators and checks exchange, and their schemas.
    #[command(hide = true)]
    Contracts {
        #[command(subcommand)]
        command: contracts_cmd::ContractsCommand,
    },
    /// The corpus as a release asset.
    #[command(hide = true)]
    Corpus {
        #[command(subcommand)]
        command: coverage_cmd::CorpusCommand,
    },
    /// Which envelope fields core validates: single-field changes of a valid transaction,
    /// submitted to a standalone core.
    #[command(hide = true)]
    Probe {
        #[command(subcommand)]
        command: probe_cmd::ProbeCommand,
    },
    /// stellar-core's own [tx] tests at every protocol version and the metas they apply.
    #[command(hide = true)]
    Coretest {
        #[command(subcommand)]
        command: coretest_cmd::CoretestCommand,
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
    /// without repeats: the recipes of this build and the chains of the corpus's
    /// recipes/chains.toml.
    Families {
        /// The corpus directory, holding manifest.json.
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
    },
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
        /// The corpus directory, holding manifest.json.
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
        /// Directory holding the registry files.
        #[arg(long, default_value = "registry")]
        registry: PathBuf,
        /// The pins file, read for the core image and the pinned sources.
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
    },
    /// Record a chain with no transactions through --last, so its history archive holds
    /// checkpoints, and write it in the committed form.
    Checkpoints {
        /// Passphrase the chain runs under.
        #[arg(long, value_parser = ["testnet", "mainnet"])]
        network: String,
        /// Protocol the chain is upgraded to after genesis.
        #[arg(long, default_value_t = 30)]
        protocol: u32,
        /// Last ledger; checkpoints end at 63, 127, 191 and so on.
        #[arg(long, default_value_t = 127)]
        last: u32,
        /// Directory to write the chain into; must not exist.
        #[arg(long)]
        out: PathBuf,
        /// The pins file, read for the core image and the pinned sources.
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
        /// The corpus directory, holding manifest.json.
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
    },
    /// Record the chain of a recipe IR file and compare it with the committed corpus: the
    /// chain's canonical hashes per passphrase and, with both, every entry it owns.
    RunIr {
        /// The recipe file, corpus/recipes/<family>/<name>.json.
        file: PathBuf,
        /// testnet, mainnet or both.
        #[arg(long, default_value = "both", value_parser = ["testnet", "mainnet", "both"])]
        network: String,
        /// Chains recorded at once, each on its own core container.
        #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u16).range(1..=32))]
        jobs: u16,
        /// The corpus directory, holding manifest.json.
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
        /// Directory holding the registry files.
        #[arg(long, default_value = "registry")]
        registry: PathBuf,
        /// The pins file, read for the core image and the pinned sources.
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
    },
    /// Regenerate the recipe chains under both passphrases into a temporary
    /// directory and compare canonical hashes and entry files with the corpus.
    VerifyRepro {
        /// Only the recipes of this family; repeatable. All when absent.
        #[arg(long)]
        family: Vec<String>,
        /// Chains recorded at once, each on its own core container.
        #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u16).range(1..=32))]
        jobs: u16,
        /// The corpus directory, holding manifest.json.
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
        /// Directory holding the registry files.
        #[arg(long, default_value = "registry")]
        registry: PathBuf,
        /// The pins file, read for the core image and the pinned sources.
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
    /// Rewrite every entry's expected XDR-JSON file from its XDR file as `to_xdr_json` renders
    /// it now, and the sha256 in the entry file. Run `manifest build` afterwards.
    Rerender {
        /// The corpus directory (corpus or candidate).
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
        /// List the files that would change without writing them.
        #[arg(long)]
        dry_run: bool,
    },
    /// Write the manifest JSON Schema generated from the manifest types.
    Schema {
        /// Where to write the schema.
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
        /// The stellar-core tag the checkout is at.
        #[arg(long, default_value = "v30.0.0")]
        tag: String,
        /// Golden tier to read. test-lcm-next is the next protocol's meta and belongs in the
        /// next/ rehearsal tree (--corpus next), never in the release corpus.
        #[arg(long, default_value = "test-lcm-current", value_parser = ["test-lcm-current", "test-lcm-next"])]
        tier: String,
        /// The corpus directory, holding manifest.json.
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
        /// Directory holding the registry files.
        #[arg(long, default_value = "registry")]
        registry: PathBuf,
    },
}

#[derive(Subcommand)]
enum GoldensCommand {
    /// Rescan a stellar-core checkout and require it to match the committed golden index.
    Verify {
        /// A git checkout of stellar-core at the tag, holding the --tier directory.
        #[arg(long)]
        core_src: PathBuf,
        /// The stellar-core tag the checkout is at.
        #[arg(long, default_value = "v30.0.0")]
        tag: String,
        /// Golden tier to rescan.
        #[arg(long, default_value = "test-lcm-current", value_parser = ["test-lcm-current", "test-lcm-next"])]
        tier: String,
        /// The committed golden index to compare with.
        #[arg(long, default_value = "corpus/goldens/core-v30.0.0/index.json")]
        index: PathBuf,
    },
    /// Structural agreement between golden entries and local-core or history entries.
    Agree {
        /// The corpus manifest.
        #[arg(long, default_value = "corpus/manifest.json")]
        file: PathBuf,
        /// Directory holding the registry files.
        #[arg(long, default_value = "registry")]
        registry: PathBuf,
    },
}

#[derive(Subcommand)]
enum CaptureCommand {
    /// Capture one ledger's LedgerCloseMeta as a testnet-history or mainnet-history entry.
    History {
        /// The network whose history the ledger comes from.
        #[arg(long, value_enum)]
        network: capture_cmd::NetworkArg,
        /// The ledger sequence to capture.
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
        /// The corpus directory, holding manifest.json.
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
        /// Directory holding the registry files.
        #[arg(long, default_value = "registry")]
        registry: PathBuf,
    },
}

#[derive(Subcommand)]
enum OracleCommand {
    /// Write <variant>.oracle.json next to every eligible corpus entry and record it.
    Run {
        /// The corpus directory, holding manifest.json.
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
    },
    /// Apply a record-marked LedgerCloseMeta stream to a ledger state, checking state
    /// consistency, conservation of lumens and interpretation on every ledger.
    Check {
        /// The record-marked LedgerCloseMeta stream.
        #[arg(long)]
        file: PathBuf,
        /// The network passphrase the ledgers were closed under.
        #[arg(long)]
        passphrase: String,
        /// What the ledger state was before the first ledger of the file.
        #[arg(long, value_enum, default_value = "genesis")]
        start: oracle_cmd::StartState,
    },
    /// Print the oracle facts for a LedgerCloseMeta file.
    Interpret {
        /// The LedgerCloseMeta file.
        #[arg(long)]
        file: PathBuf,
        /// The network passphrase the ledger was closed under.
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

/// The hidden top-level commands with their one-line help, the text `shapekit dev` prints.
fn dev_listing() -> String {
    let cli = Cli::command();
    let mut hidden: Vec<_> = cli.get_subcommands().filter(|c| c.is_hide_set()).collect();
    hidden.sort_by_key(|c| c.get_name());
    let width = hidden.iter().map(|c| c.get_name().len()).max().unwrap_or(0);
    let mut out = String::from(
        "Internal commands: the corpus generator, its checks and the maintenance tools.\n\
         They are left out of `shapekit --help`; run them as `shapekit <command>`, and see\n\
         `shapekit <command> --help` for each.\n\n",
    );
    for c in hidden {
        let about = c.get_about().map(|a| a.to_string()).unwrap_or_default();
        let first = about.split_once(". ").map_or(about.as_str(), |(f, _)| f);
        let first = first.trim_end_matches('.');
        out.push_str(&format!("  {:width$}  {first}\n", c.get_name()));
    }
    out
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
            ManifestCommand::Rerender { corpus, dry_run } => {
                manifest_cmd::rerender(&corpus, dry_run)
            }
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
            command: GenCommand::Families { corpus },
        } => gen_cmd::families(&corpus),
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
        Command::Report(args) => report_cmd::run(args),
        Command::Fuzz(args) => fuzz_cmd::run(args),
        Command::NativeFuzz(args) => native_fuzz_cmd::run(args),
        Command::GuidedFuzz(args) => guided_fuzz_cmd::run(args),
        Command::Coverage(args) => coverage_cmd::coverage(args),
        Command::Corpus { command } => coverage_cmd::corpus(command),
        Command::Contracts { command } => contracts_cmd::run(command),
        Command::Site { command } => site_cmd::run(command),
        Command::Probe { command } => probe_cmd::run(command),
        Command::Coretest { command } => coretest_cmd::run(command),
        Command::Schema { command } => schema_cmd::run(command),
        Command::Laws { command } => laws_cmd::run(command),
        Command::Twin { command } => twin_cmd::run(command),
        Command::Explore { command } => explore_cmd::run(command),
        Command::Vectors { command } => vectors_cmd::run(command),
        Command::Audit { command } => audit_cmd::run(command),
        Command::Check { command } => check_cmd::run(command),
        Command::Lint(args) => lint_cmd::run(args),
        Command::Init { command } => init_cmd::run(command),
        Command::Doctor(args) => doctor_cmd::run(args),
        Command::Dev => {
            print!("{}", dev_listing());
            Ok(true)
        }
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("error: {}", error_text::chain(err.as_ref()));
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_is_consistent() {
        Cli::command().debug_assert();
    }

    // zero bytes would reach a modulo by zero in the generator
    #[test]
    fn arbitrary_byte_bounds_are_positive() {
        for cmd in ["fuzz", "guided-fuzz"] {
            let parses = |n: &str| {
                Cli::try_parse_from(["shapekit", cmd, "--max-arbitrary-bytes", n]).is_ok()
            };
            assert!(parses("1"), "{cmd}");
            assert!(!parses("0"), "{cmd}");
        }
    }

    // a misspelled network would select no chain and pass
    #[test]
    fn network_options_take_only_known_networks() {
        let parses = |args: &[&str]| Cli::try_parse_from(args.iter().copied()).is_ok();
        let base: [&[&str]; 4] = [
            &[
                "shapekit",
                "harness",
                "state",
                "check",
                "--run",
                "r",
                "--mapping",
                "m",
            ],
            &[
                "shapekit", "harness", "state", "infer", "--run", "r", "--write", "w",
            ],
            &[
                "shapekit",
                "harness",
                "state",
                "pair",
                "--a",
                "a",
                "--b",
                "b",
                "--pair",
                "p",
                "--mapping",
                "m",
            ],
            &["shapekit", "gen", "run-ir", "f.json"],
        ];
        for args in base {
            assert!(
                parses(&[args, &["--network", "mainnet"]].concat()),
                "{args:?}"
            );
            assert!(
                !parses(&[args, &["--network", "testnte"]].concat()),
                "{args:?}"
            );
        }
    }

    // the user commands are the whole of `shapekit --help`; every internal command keeps its
    // path, so scripts and workflows that call it still parse
    #[test]
    fn visible_and_internal_commands() {
        let cli = Cli::command();
        let visible: Vec<_> = cli
            .get_subcommands()
            .filter(|c| !c.is_hide_set())
            .map(|c| c.get_name())
            .collect();
        assert_eq!(
            visible,
            ["check", "lint", "report", "init", "doctor", "dev", "serve"]
        );
        let internal = [
            "twin",
            "explore",
            "probe",
            "coretest",
            "gen",
            "laws",
            "contracts",
            "schema",
            "vectors",
            "synth",
            "import",
            "capture",
            "goldens",
            "oracle",
            "pack",
            "site",
            "registry",
            "manifest",
            "audit",
            "fuzz",
            "native-fuzz",
            "guided-fuzz",
            "pins",
            "corpus",
            "coverage",
            "guide",
            "findings",
            "conformance",
            "harness",
        ];
        let listing = dev_listing();
        for name in internal {
            let c = cli
                .find_subcommand(name)
                .unwrap_or_else(|| panic!("{name} is gone"));
            assert!(c.is_hide_set(), "{name} is listed in --help");
            assert!(
                listing.contains(&format!("  {name} ")),
                "{name} missing from dev"
            );
        }
        assert_eq!(
            cli.get_subcommands().filter(|c| c.is_hide_set()).count(),
            internal.len()
        );
        for argv in [
            &["shapekit", "registry", "check"][..],
            &[
                "shapekit",
                "conformance",
                "run",
                "--sdk",
                "rust",
                "--no-build",
            ],
            &["shapekit", "harness", "run", "--adapter", "a"],
            &["shapekit", "check", "sdk", "--sdk", "go", "--all-versions"],
            &[
                "shapekit",
                "check",
                "indexer",
                "--adapter",
                "a b",
                "--chain",
                "c",
            ],
            &["shapekit", "lint", "dir", "--baseline", "b.json", "--json"],
            &["shapekit", "dev"],
        ] {
            Cli::try_parse_from(argv).unwrap_or_else(|e| panic!("{argv:?}: {e}"));
        }
    }

    // a dataset run ignores these options, so giving them with --dataset is refused rather
    // than silently dropped
    #[test]
    fn dataset_runs_refuse_the_options_they_ignore() {
        let base = ["shapekit", "conformance", "run", "--dataset", "d"];
        Cli::try_parse_from(base).unwrap_or_else(|e| panic!("{e}"));
        for extra in [
            &["--obligations", "--all-versions"][..],
            &["--passes", "2"],
            &["--manifest", "m.json"],
            &["--known-failures", "k"],
        ] {
            let argv: Vec<&str> = base.iter().chain(extra).copied().collect();
            let err = Cli::try_parse_from(&argv).err().unwrap_or_else(|| {
                panic!("{argv:?} parses");
            });
            assert_eq!(
                err.kind(),
                clap::error::ErrorKind::ArgumentConflict,
                "{argv:?}: {err}"
            );
        }
    }
}
