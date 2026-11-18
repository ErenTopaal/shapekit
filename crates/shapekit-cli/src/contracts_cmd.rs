//! `shapekit contracts`: the data formats the generators and checks exchange.

use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ContractsCommand {
    /// Compare schema/*.schema.json of every contract with the schema generated from its
    /// types, or with --write regenerate them.
    Schema {
        /// Write the generated schemas instead of comparing.
        #[arg(long, conflicts_with = "check")]
        write: bool,
        /// Compare only, the default; fails when a committed schema is stale or missing.
        #[arg(long)]
        check: bool,
        /// Directory holding the committed schemas.
        #[arg(long, default_value = "schema")]
        dir: PathBuf,
    },
    /// Load a data file as one contract: its schema version, its fields and the checks the
    /// schema cannot express.
    Validate {
        /// The contract the file is checked as.
        #[arg(long, value_parser = contract_names())]
        contract: String,
        /// The data file to check.
        #[arg(long)]
        file: PathBuf,
    },
}

fn contract_names() -> Vec<&'static str> {
    shapekit_contracts::all().iter().map(|c| c.name).collect()
}

pub fn run(command: ContractsCommand) -> Result<bool> {
    match command {
        ContractsCommand::Schema { write, dir, .. } => {
            let stale = shapekit_contracts::sync_schemas(&dir, write)?;
            for path in &stale {
                if write {
                    println!("wrote {}", path.display());
                } else {
                    println!("stale: {}", path.display());
                }
            }
            let n = shapekit_contracts::all().len();
            if write || stale.is_empty() {
                println!("contract schemas ok: {n} files in {}", dir.display());
                Ok(true)
            } else {
                println!("run `shapekit contracts schema --write`");
                Ok(false)
            }
        }
        ContractsCommand::Validate { contract, file } => {
            let Some(c) = shapekit_contracts::all()
                .into_iter()
                .find(|c| c.name == contract)
            else {
                bail!("no contract named {contract}");
            };
            match (c.validate)(&file) {
                Ok(()) => {
                    println!("{} ok as {contract}", file.display());
                    Ok(true)
                }
                Err(e) => {
                    println!("{e}");
                    Ok(false)
                }
            }
        }
    }
}
