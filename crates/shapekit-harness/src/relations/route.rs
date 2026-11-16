//! Route equivalence: when core holds two records identically except for whose they are
//! (two accounts funded the same way, one through a plain address and one through a muxed
//! one; a trustline opened by `ChangeTrust` and one opened through the asset contract), the
//! target must keep the same state for both. Core decides the equality, byte for byte, on
//! the entries' XDR with the holder zeroed; nothing is interpreted.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::known::Known;
use super::state::{CoreState, EntryKind, Mapping, row_key};
use crate::dump::Dump;

/// Records core holds identically except for their key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    pub entry: EntryKind,
    pub keys: Vec<Vec<String>>,
    /// Length of the compared XDR (the ledger entry with its holder zeroed).
    pub bytes: usize,
}

/// Every group of two or more equal records in `core`.
pub fn groups(core: &CoreState) -> Vec<Group> {
    let mut by: BTreeMap<(EntryKind, &Vec<u8>), Vec<Vec<String>>> = BTreeMap::new();
    for ((kind, key), bytes) in &core.unkeyed {
        by.entry((*kind, bytes)).or_default().push(key.clone());
    }
    by.into_iter()
        .filter(|(_, keys)| keys.len() > 1)
        .map(|((entry, bytes), keys)| Group {
            entry,
            keys,
            bytes: bytes.len(),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteDifference {
    pub table: String,
    pub entry: EntryKind,
    pub keys: Vec<Vec<String>>,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteReport {
    pub groups: Vec<Group>,
    /// Group and mapped table combinations compared.
    pub compared: u64,
    pub differences: Vec<RouteDifference>,
}

/// Compares, for every group and every mapped table of the group's kind, the rows the
/// target keeps for each key, without their key columns.
pub fn check(dump: &Dump, core: &CoreState, known: &Known, mapping: &Mapping) -> RouteReport {
    let mut r = RouteReport {
        groups: groups(core),
        ..RouteReport::default()
    };
    for g in &r.groups {
        for t in mapping.table.iter().filter(|t| t.entry == g.entry) {
            let Some(table) = dump.tables.get(&t.name) else {
                continue;
            };
            let key_cols: Vec<usize> = t
                .keys
                .iter()
                .filter_map(|k| table.column(&k.column))
                .collect();
            let mut stored: BTreeMap<Vec<String>, Vec<Vec<Option<String>>>> = BTreeMap::new();
            for row in &table.rows {
                let Some(key) = row_key(dump, table, row, &t.keys, known) else {
                    continue;
                };
                if g.keys.contains(&key) {
                    let rest: Vec<Option<String>> = row
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| !key_cols.contains(i))
                        .map(|(_, c)| c.clone())
                        .collect();
                    stored.entry(key).or_default().push(rest);
                }
            }
            r.compared += 1;
            let have: Vec<&Vec<String>> =
                g.keys.iter().filter(|k| stored.contains_key(*k)).collect();
            if have.is_empty() {
                continue;
            }
            if have.len() != g.keys.len() {
                let missing: Vec<String> = g
                    .keys
                    .iter()
                    .filter(|k| !stored.contains_key(*k))
                    .map(|k| k.join(" "))
                    .collect();
                r.differences.push(RouteDifference {
                    table: t.name.clone(),
                    entry: g.entry,
                    keys: g.keys.clone(),
                    detail: format!(
                        "rows for {} of the {} equal records; none for {}",
                        have.len(),
                        g.keys.len(),
                        missing.join(", ")
                    ),
                });
                continue;
            }
            let mut sets: Vec<Vec<Vec<Option<String>>>> = stored.into_values().collect();
            for s in &mut sets {
                s.sort();
            }
            if sets.windows(2).any(|w| w[0] != w[1]) {
                let names: Vec<&str> = table
                    .columns
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !key_cols.contains(i))
                    .map(|(_, c)| c.name.as_str())
                    .collect();
                let differing: Vec<&str> = names
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| {
                        let col = |s: &Vec<Vec<Option<String>>>| -> Vec<Option<String>> {
                            s.iter().map(|row| row[*i].clone()).collect()
                        };
                        sets.windows(2).any(|w| col(&w[0]) != col(&w[1]))
                    })
                    .map(|(_, n)| *n)
                    .collect();
                r.differences.push(RouteDifference {
                    table: t.name.clone(),
                    entry: g.entry,
                    keys: g.keys.clone(),
                    detail: format!("the rows differ in {}", differing.join(", ")),
                });
            }
        }
    }
    r
}
