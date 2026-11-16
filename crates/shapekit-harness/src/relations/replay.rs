//! Replay invariance: the same chain delivered another way (`rpc` or a SEP-54 lake), with a
//! restart in the middle, or under the other network passphrase must leave the target with
//! the same tables. Two dumps are compared as multisets of rows, table by table.
//!
//! Two kinds of difference are allowed, and both are measured rather than assumed:
//!
//! - **noise columns**: columns that differ between two runs of the same configuration
//!   (a control pair), such as ingestion timestamps. They are blanked on both sides and
//!   listed in the report.
//! - **network translation**: when the two dumps come from the testnet and mainnet variants
//!   of a chain, every address and hash core itself wrote differently in the two chains
//!   (walking both chains' ledgers in step) is translated, in every known encoding, before
//!   comparing. Numbers are not translated.
//!   Anything the target derives from those identifiers in a form outside the known
//!   encodings stays a difference.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shapekit_oracle::decode::LcmStream;
use shapekit_oracle::network::Network;
use stellar_strkey::Strkey;
use stellar_xdr::Asset;

use super::known::{encodings_of, is_strkey};
use crate::dump::Dump;

/// Rows shown per side and table when they differ.
pub const MAX_SAMPLES: usize = 5;

/// Cell text in one dump to the cell text the other dump should hold for it.
#[derive(Debug, Clone, Default)]
pub struct Translation {
    pub map: HashMap<String, String>,
    /// Values the ledgers wrote as two different counterparts; kept out of `map`.
    pub conflicts: BTreeSet<String>,
    /// Places where the two chains' ledgers have different shapes (a network setting
    /// written as another arm, a list of another length). Nothing under them is paired.
    pub shape_differences: Vec<String>,
}

/// A hash or id core writes as 32 or more bytes of lowercase hex. Shorter values (numbers,
/// codes) are too common to translate: the same text means other things elsewhere.
fn is_hash(s: &str) -> bool {
    s.len() >= 64
        && s.len().is_multiple_of(2)
        && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn strkey_text(s: &Strkey) -> String {
    s.to_string().to_string()
}

impl Translation {
    fn pair(&mut self, a: &str, b: &str) {
        if self.conflicts.contains(a) {
            return;
        }
        match self.map.get(a) {
            Some(x) if x != b => {
                self.map.remove(a);
                self.conflicts.insert(a.to_owned());
            }
            Some(_) => {}
            None => {
                self.map.insert(a.to_owned(), b.to_owned());
            }
        }
    }

    fn pair_addresses(&mut self, a: &str, b: &str) {
        let (ea, eb) = (encodings_of(a), encodings_of(b));
        for ((ta, ka), (tb, kb)) in ea.iter().zip(&eb) {
            if ka == kb {
                self.pair(ta, tb);
            }
        }
    }

    fn pair_assets(&mut self, a: &Asset, b: &Asset, na: &Network, nb: &Network) {
        let ca = Strkey::Contract(stellar_strkey::Contract(na.asset_contract_id(a)));
        let cb = Strkey::Contract(stellar_strkey::Contract(nb.asset_contract_id(b)));
        self.pair_addresses(&strkey_text(&ca), &strkey_text(&cb));
    }

    fn walk(
        &mut self,
        a: &Value,
        b: &Value,
        path: &mut Vec<String>,
        nets: (&Network, &Network),
    ) -> Result<()> {
        match (a, b) {
            (Value::String(x), Value::String(y)) => {
                if x != y {
                    if is_strkey(x) && is_strkey(y) {
                        self.pair_addresses(x, y);
                    } else if is_hash(x) && is_hash(y) {
                        self.pair(x, y);
                    }
                }
            }
            (Value::Array(x), Value::Array(y)) => {
                if x.len() != y.len() {
                    self.shape_differences.push(format!(
                        "{}: {} and {} items",
                        path.join("/"),
                        x.len(),
                        y.len()
                    ));
                    return Ok(());
                }
                for (i, (p, q)) in x.iter().zip(y).enumerate() {
                    path.push(i.to_string());
                    self.walk(p, q, path, nets)?;
                    path.pop();
                }
            }
            (Value::Object(x), Value::Object(y)) => {
                if x.len() != y.len() || x.keys().any(|k| !y.contains_key(k)) {
                    self.shape_differences.push(path.join("/"));
                    return Ok(());
                }
                if x.len() == 1
                    && (x.contains_key("credit_alphanum4") || x.contains_key("credit_alphanum12"))
                    && let (Ok(p), Ok(q)) = (
                        serde_json::from_value::<Asset>(a.clone()),
                        serde_json::from_value::<Asset>(b.clone()),
                    )
                {
                    self.pair_assets(&p, &q, nets.0, nets.1);
                }
                for (k, p) in x {
                    path.push(k.clone());
                    self.walk(p, &y[k], path, nets)?;
                    path.pop();
                }
            }
            _ if a == b => {}
            _ => self.shape_differences.push(path.join("/")),
        }
        Ok(())
    }

    /// What core wrote differently in the two variants of one recipe chain, ledger by
    /// ledger, plus the passphrase-derived values (genesis root, native asset contract).
    pub fn between_chains(a_dir: &Path, b_dir: &Path) -> Result<Translation> {
        let ca = shapekit_pack::chain::ChainFile::read(a_dir)
            .with_context(|| format!("reading {}", a_dir.display()))?;
        let cb = shapekit_pack::chain::ChainFile::read(b_dir)
            .with_context(|| format!("reading {}", b_dir.display()))?;
        let (na, nb) = (Network::new(&ca.passphrase), Network::new(&cb.passphrase));
        let mut t = Translation::default();
        t.pair_addresses(
            &shapekit_oracle::addr::account(&na.root_account()),
            &shapekit_oracle::addr::account(&nb.root_account()),
        );
        t.pair_assets(&Asset::Native, &Asset::Native, &na, &nb);
        let open = |d: &Path| {
            let p = d.join("ledgers.xdr");
            LcmStream::open(&p).with_context(|| format!("opening {}", p.display()))
        };
        let (mut sa, mut sb) = (open(a_dir)?, open(b_dir)?);
        loop {
            match (sa.next(), sb.next()) {
                (None, None) => break,
                (Some(x), Some(y)) => {
                    let (x, y) = (x?, y?);
                    let seq = shapekit_pack::lake::seq_of(&x.meta);
                    if seq != shapekit_pack::lake::seq_of(&y.meta) {
                        bail!("the chains' ledgers fall out of step at ledger {seq}");
                    }
                    let (va, vb) = (
                        serde_json::to_value(&x.meta)?,
                        serde_json::to_value(&y.meta)?,
                    );
                    t.walk(&va, &vb, &mut vec![format!("ledger {seq}")], (&na, &nb))?;
                }
                _ => bail!("the chains have different numbers of ledgers"),
            }
        }
        Ok(t)
    }

    fn apply(&self, cell: &str) -> String {
        self.map
            .get(cell)
            .cloned()
            .unwrap_or_else(|| cell.to_owned())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableDiff {
    pub table: String,
    pub rows_base: u64,
    pub rows_other: u64,
    pub only_base: u64,
    pub only_other: u64,
    /// Columns whose multiset of (translated) values differs between the two sides.
    pub columns: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub samples_only_base: Vec<Vec<Option<String>>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub samples_only_other: Vec<Vec<Option<String>>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayReport {
    pub chain: String,
    pub base: String,
    pub other: String,
    /// Whether the network translation was applied, and how many values it maps.
    pub translated: bool,
    pub translation_values: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub translation_conflicts: Vec<String>,
    /// The first places where the two chains differ in shape (nothing under them is
    /// translated).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shape_differences: Vec<String>,
    /// `table.column`, blanked on both sides (from the control pair).
    pub noise_columns: Vec<String>,
    pub tables: u64,
    pub rows: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tables_only_base: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tables_only_other: Vec<String>,
    /// Tables whose columns differ; their rows are not compared.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub column_mismatches: Vec<String>,
    /// Tables whose rows differ.
    pub differences: Vec<TableDiff>,
    pub equal: bool,
}

/// Columns differing between two dumps of the same configuration.
pub fn noise_columns(a: &Dump, b: &Dump) -> BTreeSet<String> {
    let r = compare(a, b, &Translation::default(), &BTreeSet::new());
    r.differences
        .iter()
        .flat_map(|d| d.columns.iter().cloned())
        .collect()
}

fn multiset<T: Ord + Clone>(v: impl Iterator<Item = T>) -> BTreeMap<T, u64> {
    let mut m = BTreeMap::new();
    for x in v {
        *m.entry(x).or_insert(0) += 1;
    }
    m
}

/// Compares `other` with `base` after translating `base`'s cells and blanking `noise`.
pub fn compare(
    base: &Dump,
    other: &Dump,
    tr: &Translation,
    noise: &BTreeSet<String>,
) -> ReplayReport {
    let mut r = ReplayReport {
        translated: !tr.map.is_empty(),
        translation_values: tr.map.len() as u64,
        translation_conflicts: tr.conflicts.iter().take(20).cloned().collect(),
        shape_differences: tr.shape_differences.iter().take(20).cloned().collect(),
        noise_columns: noise.iter().cloned().collect(),
        ..ReplayReport::default()
    };
    r.tables_only_base = base
        .tables
        .keys()
        .filter(|k| !other.tables.contains_key(*k))
        .cloned()
        .collect();
    r.tables_only_other = other
        .tables
        .keys()
        .filter(|k| !base.tables.contains_key(*k))
        .cloned()
        .collect();
    for (name, tb) in &base.tables {
        let Some(to) = other.tables.get(name) else {
            continue;
        };
        r.tables += 1;
        if tb.columns != to.columns {
            r.column_mismatches.push(name.clone());
            continue;
        }
        let blank: Vec<bool> = tb
            .columns
            .iter()
            .map(|c| noise.contains(&format!("{name}.{}", c.name)))
            .collect();
        let norm = |row: &Vec<Option<String>>, translate: bool| -> Vec<Option<String>> {
            row.iter()
                .zip(&blank)
                .map(|(c, b)| {
                    if *b {
                        None
                    } else if translate {
                        c.as_deref().map(|x| tr.apply(x))
                    } else {
                        c.clone()
                    }
                })
                .collect()
        };
        let rb = multiset(tb.rows.iter().map(|x| norm(x, true)));
        let ro = multiset(to.rows.iter().map(|x| norm(x, false)));
        r.rows += tb.rows.len() as u64;
        let mut d = TableDiff {
            table: name.clone(),
            rows_base: tb.rows.len() as u64,
            rows_other: to.rows.len() as u64,
            ..TableDiff::default()
        };
        for (row, n) in &rb {
            let m = ro.get(row).copied().unwrap_or(0);
            if *n > m {
                d.only_base += n - m;
                if d.samples_only_base.len() < MAX_SAMPLES {
                    d.samples_only_base.push(row.clone());
                }
            }
        }
        for (row, n) in &ro {
            let m = rb.get(row).copied().unwrap_or(0);
            if *n > m {
                d.only_other += n - m;
                if d.samples_only_other.len() < MAX_SAMPLES {
                    d.samples_only_other.push(row.clone());
                }
            }
        }
        if d.only_base == 0 && d.only_other == 0 {
            continue;
        }
        for (i, c) in tb.columns.iter().enumerate() {
            let cb = multiset(
                rb.iter()
                    .flat_map(|(row, n)| std::iter::repeat_n(row[i].clone(), *n as usize)),
            );
            let co = multiset(
                ro.iter()
                    .flat_map(|(row, n)| std::iter::repeat_n(row[i].clone(), *n as usize)),
            );
            if cb != co {
                d.columns.push(format!("{name}.{}", c.name));
            }
        }
        r.differences.push(d);
    }
    r.equal = r.differences.is_empty()
        && r.column_mismatches.is_empty()
        && r.tables_only_base.is_empty()
        && r.tables_only_other.is_empty();
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dump::Table;
    use crate::protocol::DumpColumn;

    fn dump(rows: &[[&str; 2]]) -> Dump {
        let mut d = Dump::default();
        d.tables.insert(
            "t".into(),
            Table {
                columns: vec![
                    DumpColumn {
                        name: "id".into(),
                        ty: "bytea".into(),
                    },
                    DumpColumn {
                        name: "at".into(),
                        ty: "text".into(),
                    },
                ],
                rows: rows
                    .iter()
                    .map(|r| r.iter().map(|c| Some((*c).to_owned())).collect())
                    .collect(),
            },
        );
        d
    }

    #[test]
    fn order_does_not_matter_and_noise_is_measured() {
        let a = dump(&[["01", "t1"], ["02", "t2"]]);
        let b = dump(&[["02", "t9"], ["01", "t8"]]);
        let noise = noise_columns(&a, &b);
        assert_eq!(noise, BTreeSet::from(["t.at".to_owned()]));
        assert!(compare(&a, &b, &Translation::default(), &noise).equal);
    }

    #[test]
    fn a_missing_row_is_a_difference() {
        let a = dump(&[["01", "t"], ["02", "t"]]);
        let b = dump(&[["01", "t"]]);
        let r = compare(&a, &b, &Translation::default(), &BTreeSet::new());
        assert!(!r.equal);
        assert_eq!(r.differences[0].only_base, 1);
        assert!(r.differences[0].columns.contains(&"t.id".to_owned()));
    }

    #[test]
    fn translated_values_compare_equal() {
        let a = dump(&[["aa", "t"]]);
        let b = dump(&[["bb", "t"]]);
        let mut tr = Translation::default();
        tr.pair("aa", "bb");
        assert!(compare(&a, &b, &tr, &BTreeSet::new()).equal);
    }

    #[test]
    fn a_value_with_two_counterparts_is_not_translated() {
        let mut tr = Translation::default();
        tr.pair("aa", "bb");
        tr.pair("aa", "cc");
        assert!(tr.map.is_empty());
        assert!(tr.conflicts.contains("aa"));
    }

    #[test]
    fn the_two_variants_of_a_chain_translate_into_each_other() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/chains/cap40.signed-payload-signer.p28");
        let t = Translation::between_chains(&root.join("testnet"), &root.join("mainnet")).unwrap();
        assert!(!t.map.is_empty());
        let na = Network::new("Test SDF Network ; September 2015");
        let root_g = shapekit_oracle::addr::account(&na.root_account());
        assert!(
            t.map.contains_key(&root_g),
            "the genesis root is translated"
        );
    }
}
