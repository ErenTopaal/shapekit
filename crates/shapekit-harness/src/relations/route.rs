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

/// One entry of a recipe pair (`candidate/recipes/explore.pairs/<pair>/pair.json`): two
/// chains from the same start that reach it in two ways.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairEntry {
    pub entry: EntryKind,
    pub key: Vec<String>,
    /// Core holds the entry identically at the end of both chains (its XDR compared).
    pub core_equal: bool,
    /// The tables of the entry's kind the mapping names, each with whether the target's rows
    /// for the key are the same in both runs.
    pub tables: Vec<PairTable>,
    /// The changes the target exported about the entry in each run (kind and fields, without
    /// scope and operation index), and whether they are the same. Empty when the runs kept
    /// no export.
    pub changes_a: Vec<String>,
    pub changes_b: Vec<String>,
    pub changes_equal: bool,
}

/// Vocabulary kinds that change an entry of the kind, and the field naming the entry's
/// asset when the kind has one.
fn entry_kinds(kind: EntryKind) -> &'static [&'static str] {
    match kind {
        EntryKind::Account => &[
            "account-created",
            "account-removed",
            "account-flags",
            "thresholds",
            "home-domain",
            "inflation-destination",
            "sequence",
            "signer-added",
            "signer-removed",
            "signer-weight",
        ],
        EntryKind::Trustline => &[
            "trustline-created",
            "trustline-removed",
            "trustline-limit",
            "trustline-flags",
        ],
        EntryKind::SacBalance | EntryKind::LiquidityPool => &[],
    }
}

/// The exported changes about the entry: of a kind that changes it, naming its account and,
/// for a trustline, its asset.
fn changes_about(c: &super::state::RunChain, kind: EntryKind, key: &[String]) -> Vec<String> {
    let kinds = entry_kinds(kind);
    let mut out: Vec<String> = c
        .export
        .iter()
        .flat_map(|t| &t.changes)
        .filter(|ch| kinds.contains(&ch.kind.as_str()))
        .filter(|ch| {
            ch.fields.get("account").and_then(|v| v.as_str()) == key.first().map(String::as_str)
        })
        .filter(|ch| {
            kind != EntryKind::Trustline
                || ch.fields.get("asset").and_then(|v| v.as_str()) == key.get(1).map(String::as_str)
        })
        .map(|ch| {
            let mut f = ch.fields.clone();
            f.remove("scope");
            f.remove("op_index");
            format!("{} {}", ch.kind, serde_json::Value::Object(f))
        })
        .collect();
    out.sort();
    out
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairTable {
    pub table: String,
    pub rows_a: Vec<Vec<Option<String>>>,
    pub rows_b: Vec<Vec<Option<String>>>,
    pub equal: bool,
}

/// The key of a `LedgerKey` as `pair.json` writes it, in the form [`CoreState`] keys
/// entries by. Accounts and trustlines, the kinds recipe pairs compare.
pub fn pair_key(v: &serde_json::Value) -> anyhow::Result<(EntryKind, Vec<String>)> {
    let key: stellar_xdr::LedgerKey = serde_json::from_value(v.clone())
        .map_err(|e| anyhow::anyhow!("{v} is not a ledger key: {e}"))?;
    Ok(match key {
        stellar_xdr::LedgerKey::Account(a) => (
            EntryKind::Account,
            vec![shapekit_oracle::addr::account(&a.account_id)],
        ),
        stellar_xdr::LedgerKey::Trustline(t) => (
            EntryKind::Trustline,
            vec![
                shapekit_oracle::addr::account(&t.account_id),
                shapekit_oracle::vocab::trustline_asset_id(&t.asset),
            ],
        ),
        other => anyhow::bail!(
            "a pair entry of kind {:?} is not compared",
            other.discriminant()
        ),
    })
}

fn rows_for(
    c: &super::state::RunChain,
    table: &super::state::TableMapping,
    key: &[String],
) -> Vec<Vec<Option<String>>> {
    let Some(t) = c.dump.tables.get(&table.name) else {
        return Vec::new();
    };
    let mut rows: Vec<Vec<Option<String>>> = t
        .rows
        .iter()
        .filter(|r| row_key(&c.dump, t, r, &table.keys, &c.known).as_deref() == Some(key))
        .cloned()
        .collect();
    rows.sort();
    rows
}

/// Compares, for every entry of a pair, the rows the target keeps for it after chain `a`
/// and after chain `b`.
pub fn pair_check(
    a: &super::state::RunChain,
    b: &super::state::RunChain,
    entries: &[(EntryKind, Vec<String>)],
    mapping: &Mapping,
) -> Vec<PairEntry> {
    entries
        .iter()
        .map(|(kind, key)| {
            let ck = (*kind, key.clone());
            let core_equal = a.core.unkeyed.contains_key(&ck)
                && a.core.unkeyed.get(&ck) == b.core.unkeyed.get(&ck);
            let tables = mapping
                .table
                .iter()
                .filter(|t| t.entry == *kind)
                .map(|t| {
                    let (ra, rb) = (rows_for(a, t, key), rows_for(b, t, key));
                    PairTable {
                        table: t.name.clone(),
                        equal: ra == rb,
                        rows_a: ra,
                        rows_b: rb,
                    }
                })
                .collect();
            let (changes_a, changes_b) =
                (changes_about(a, *kind, key), changes_about(b, *kind, key));
            PairEntry {
                entry: *kind,
                key: key.clone(),
                core_equal,
                tables,
                changes_equal: changes_a == changes_b,
                changes_a,
                changes_b,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_keys_read_accounts_and_trustlines() {
        let a = serde_json::json!({"account": {"account_id": "GB62AEPS2IU6LFQZ4QMJV3PTRRSYCTP35DSCRMLQYMDWQGMPVNM3LPSU"}});
        assert_eq!(pair_key(&a).unwrap().0, EntryKind::Account);
        let t = serde_json::json!({"trustline": {
            "account_id": "GB62AEPS2IU6LFQZ4QMJV3PTRRSYCTP35DSCRMLQYMDWQGMPVNM3LPSU",
            "asset": {"credit_alphanum4": {"asset_code": "USD", "issuer": "GC54HSZ6KTA7TFBWWS36OD4J65R4QTUZODULVRFG5HNFVCVWLO3VZJ5Z"}}}});
        let (kind, key) = pair_key(&t).unwrap();
        assert_eq!(kind, EntryKind::Trustline);
        assert_eq!(
            key[1],
            "USD:GC54HSZ6KTA7TFBWWS36OD4J65R4QTUZODULVRFG5HNFVCVWLO3VZJ5Z"
        );
        let o = serde_json::json!({"ttl": {"key_hash": "00".repeat(32)}});
        assert!(pair_key(&o).is_err());
    }
}
