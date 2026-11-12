//! The `shapekit` command line tool.

mod manifest_cmd;
mod pins;
mod registry_cmd;

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
