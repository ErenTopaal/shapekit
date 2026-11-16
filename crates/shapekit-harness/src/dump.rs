//! What a target stored, as the adapter's `dump` returns it: every table with its columns
//! and rows of cell text. The relations (`crate::relations`) read it; the harness keeps it
//! next to the logs as `<chain>.<network>.dump.json`.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::adapter::Adapter;
use crate::protocol::DumpColumn;

pub const DUMP_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dump {
    pub schema: u32,
    pub tables: BTreeMap<String, Table>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Table {
    pub columns: Vec<DumpColumn>,
    pub rows: Vec<Vec<Option<String>>>,
}

impl Table {
    pub fn column(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.name == name)
    }
}

/// A short account of a dump for the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DumpSummary {
    /// The dump file, relative to the run's output directory.
    pub file: String,
    pub tables: u64,
    pub rows: u64,
}

impl Dump {
    /// Asks the adapter for every table (or only `tables`) and collects the answer.
    pub fn take(adapter: &mut Adapter, tables: Vec<String>, timeout: Duration) -> Result<Dump> {
        let mut dump = Dump {
            schema: DUMP_SCHEMA,
            tables: BTreeMap::new(),
        };
        let mut rows: Vec<crate::protocol::DumpRow> = Vec::new();
        adapter.dump(
            tables,
            timeout,
            &mut |t| {
                dump.tables.insert(
                    t.name,
                    Table {
                        columns: t.columns,
                        rows: Vec::new(),
                    },
                );
            },
            &mut |r| rows.push(r),
        )?;
        for r in rows {
            dump.tables
                .get_mut(&r.table)
                .expect("the adapter client checks a row's table came first")
                .rows
                .push(r.cells);
        }
        Ok(dump)
    }

    pub fn rows(&self) -> u64 {
        self.tables.values().map(|t| t.rows.len() as u64).sum()
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        let mut text = serde_json::to_string(self).context("rendering the dump")?;
        text.push('\n');
        std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
    }

    pub fn read(path: &Path) -> Result<Dump> {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let d: Dump = serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing {}", path.display()))?;
        if d.schema != DUMP_SCHEMA {
            anyhow::bail!(
                "{} is dump schema {}, this shapekit reads {DUMP_SCHEMA}",
                path.display(),
                d.schema
            );
        }
        Ok(d)
    }
}
