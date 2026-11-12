//! The `shapekit` command line tool.

mod pins;

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
