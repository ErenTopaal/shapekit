//! The carry check: every cell of the dump is looked up among the identifiers and amounts the
//! chain gave the target, in each known encoding. Two outcomes are always errors, whatever
//! the target means by its columns:
//!
//! - **truncated**: the cell is a strict prefix of a known value's encoding and equals no
//!   known value. The target kept part of what it was given.
//! - **fabricated**: the cell is a well-formed address the chain never carried and that does
//!   not follow from it, or any other value in a column that otherwise holds carried
//!   addresses. The target wrote an identifier it could not have derived.
//!
//! Every exact match is counted per column and encoding: which column carries which kind of
//! value, the evidence the state comparison builds its column mapping from.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use stellar_strkey::Strkey;

use super::known::{Encoding, Known, strkey_bytes};
use crate::dump::Dump;

/// Shortest cell compared as a possible prefix: hex of 17 bytes, a version byte and half a
/// key, or 20 strkey characters. Shorter cells match prefixes of anything by chance.
const MIN_HEX_PREFIX: usize = 34;
const MIN_TEXT_PREFIX: usize = 20;

/// Problems listed in full; beyond this only counted.
pub const MAX_PROBLEMS: usize = 200;

/// Strkey version bytes (`version << 3`) of every strkey type: G, M, P, C, T, X, L, B and
/// the muxed contract.
const VERSION_BYTES: &[u8] = &[
    6 << 3,
    12 << 3,
    15 << 3,
    2 << 3,
    19 << 3,
    23 << 3,
    11 << 3,
    1 << 3,
    22 << 3,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProblemKind {
    Truncated,
    Fabricated,
}

impl ProblemKind {
    pub fn name(self) -> &'static str {
        match self {
            ProblemKind::Truncated => "truncated",
            ProblemKind::Fabricated => "fabricated",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Problem {
    pub kind: ProblemKind,
    /// `table.column`.
    pub column: String,
    /// The row's index in the dump's ordering of its table.
    pub row: usize,
    pub cell: String,
    /// For `truncated`, the known value whose encoding the cell starts, and that encoding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub of: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<Encoding>,
    /// For `fabricated`, the strkey the cell decodes to, when it decodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decodes_to: Option<String>,
}

/// One column holding known values in one encoding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Carried {
    pub column: String,
    /// `address` or `amount`.
    pub kind: String,
    pub encoding: Encoding,
    /// Cells of the column that equal a known value in this encoding.
    pub cells: u64,
    /// Distinct known values among them.
    pub values: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarryReport {
    /// Known addresses and amounts of the ledgers the target ingested.
    pub known_addresses: u64,
    pub known_amounts: u64,
    /// Known addresses found in the dump in any encoding.
    pub addresses_found: u64,
    pub cells: u64,
    pub truncated: u64,
    pub fabricated: u64,
    pub carried: Vec<Carried>,
    /// The first [`MAX_PROBLEMS`] problems, by table, column and row.
    pub problems: Vec<Problem>,
}

impl CarryReport {
    pub fn columns_with(&self, kind: ProblemKind) -> Vec<String> {
        let mut c: Vec<String> = self
            .problems
            .iter()
            .filter(|p| p.kind == kind)
            .map(|p| p.column.clone())
            .collect();
        c.sort();
        c.dedup();
        c
    }
}

/// Whether a cell has the shape of a stored address: strkey text, or hex starting with a
/// strkey version byte and at least as long as a version byte and a key.
fn address_shaped(cell: &str, bytea: bool) -> bool {
    if bytea {
        cell.len() >= 66
            && u8::from_str_radix(&cell[..2], 16).is_ok_and(|b| VERSION_BYTES.contains(&b))
    } else {
        Strkey::from_string(cell).is_ok()
    }
}

/// The strkey a versioned-hex or text cell decodes to, when it does.
fn decodes_to(cell: &str, bytea: bool) -> Option<String> {
    if !bytea {
        return Strkey::from_string(cell).ok().map(|_| cell.to_owned());
    }
    let bytes = hex::decode(cell).ok()?;
    let s = encode_strkey(&bytes);
    // round trip: a wrong length for the version byte fails the strict decode
    strkey_bytes(&s).filter(|b| *b == bytes).map(|_| s)
}

/// Strkey text of version byte and payload: base32 of the bytes and their CRC16-XModem.
fn encode_strkey(bytes: &[u8]) -> String {
    let mut crc: u16 = 0;
    for b in bytes {
        crc ^= u16::from(*b) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    let mut all = bytes.to_vec();
    all.extend_from_slice(&crc.to_le_bytes());
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for b in all {
        acc = (acc << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((acc >> bits) & 31) as usize] as char);
        }
        acc &= (1 << bits) - 1;
    }
    if bits > 0 {
        out.push(ALPHABET[((acc << (5 - bits)) & 31) as usize] as char);
    }
    out
}

pub fn check(dump: &Dump, known: &Known) -> CarryReport {
    let mut rep = CarryReport {
        known_addresses: known.addresses.len() as u64,
        known_amounts: known.amounts.len() as u64,
        ..CarryReport::default()
    };
    let mut carried: BTreeMap<(String, Encoding), (bool, u64, std::collections::BTreeSet<String>)> =
        BTreeMap::new();
    let mut found = std::collections::BTreeSet::new();
    let mut problems = Vec::new();
    for (name, table) in &dump.tables {
        for (ci, col) in table.columns.iter().enumerate() {
            let bytea = col.ty == "bytea";
            let column = format!("{name}.{}", col.name);
            let mut address_column = false;
            let mut unexplained = Vec::new();
            for (ri, row) in table.rows.iter().enumerate() {
                let Some(cell) = row[ci].as_deref() else {
                    continue;
                };
                rep.cells += 1;
                if let Some(form) = known.forms.get(cell)
                    // a hex column holding a decimal-looking string is still hex
                    && (form.encoding != Encoding::Decimal || !bytea)
                {
                    let e = carried.entry((column.clone(), form.encoding)).or_insert((
                        form.address,
                        0,
                        Default::default(),
                    ));
                    e.1 += 1;
                    e.2.insert(form.value.clone());
                    if form.address {
                        address_column = true;
                        found.insert(form.value.clone());
                    }
                    continue;
                }
                let min = if bytea {
                    MIN_HEX_PREFIX
                } else {
                    MIN_TEXT_PREFIX
                };
                if cell.len() >= min
                    && let Some((_, form)) = known
                        .forms
                        .range(cell.to_owned()..)
                        .next()
                        .filter(|(k, f)| k.starts_with(cell) && f.address)
                {
                    problems.push(Problem {
                        kind: ProblemKind::Truncated,
                        column: column.clone(),
                        row: ri,
                        cell: cell.to_owned(),
                        of: Some(form.value.clone()),
                        encoding: Some(form.encoding),
                        decodes_to: None,
                    });
                    continue;
                }
                unexplained.push((ri, cell));
            }
            for (ri, cell) in unexplained {
                let decoded = decodes_to(cell, bytea);
                let shaped = address_shaped(cell, bytea);
                // a strict strkey nobody gave the target, or anything unexplained where the
                // target otherwise keeps carried addresses
                if decoded.is_some() || (address_column && shaped) {
                    problems.push(Problem {
                        kind: ProblemKind::Fabricated,
                        column: column.clone(),
                        row: ri,
                        cell: cell.to_owned(),
                        of: None,
                        encoding: None,
                        decodes_to: decoded,
                    });
                }
            }
        }
    }
    rep.addresses_found = found.len() as u64;
    rep.truncated = problems
        .iter()
        .filter(|p| p.kind == ProblemKind::Truncated)
        .count() as u64;
    rep.fabricated = problems.len() as u64 - rep.truncated;
    problems.truncate(MAX_PROBLEMS);
    rep.problems = problems;
    rep.carried = carried
        .into_iter()
        .map(|((column, encoding), (address, cells, values))| Carried {
            column,
            kind: if address { "address" } else { "amount" }.into(),
            encoding,
            cells,
            values: values.len() as u64,
        })
        .collect();
    rep
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dump::Table;
    use crate::protocol::DumpColumn;

    const G: &str = "GB4WH2VTILAXT2GUDH4KI2ZA2GGYSLJJUYBH6LP42LXI4WNBRIHHDDVT";
    const P: &str = "PDDLGS6XTXODPNKK7OP4FHZSNGVJG3OE3GKGMYN47OP3PWM3SSTPCAAAAAAXMAAAAASL2";
    const M: &str = "MBUTAF6RSBU7HAFPBOEUDFS7CHACUA33KU2E2YQIQ7VA3WK6H55RSAAAAAAAAAAAAEV2Q";

    fn dump(rows: Vec<Option<String>>) -> Dump {
        let mut d = Dump::default();
        d.tables.insert(
            "t".into(),
            Table {
                columns: vec![DumpColumn {
                    name: "a".into(),
                    ty: "bytea".into(),
                }],
                rows: rows.into_iter().map(|r| vec![r]).collect(),
            },
        );
        d
    }

    fn known() -> Known {
        let mut k = Known::default();
        for a in [G, P, M] {
            k.add_address(a);
        }
        k
    }

    fn versioned(s: &str) -> String {
        hex::encode(strkey_bytes(s).unwrap())
    }

    #[test]
    fn the_encoder_round_trips() {
        for s in [G, P, M] {
            assert_eq!(encode_strkey(&strkey_bytes(s).unwrap()), s);
        }
    }

    #[test]
    fn whole_values_are_carried() {
        let r = check(
            &dump(vec![Some(versioned(G)), Some(versioned(P)), None]),
            &known(),
        );
        assert_eq!((r.truncated, r.fabricated), (0, 0), "{r:?}");
        assert_eq!(r.carried.len(), 1);
        assert_eq!(r.carried[0].cells, 2);
    }

    #[test]
    fn a_signed_payload_cut_to_its_key_is_truncated() {
        let cut = versioned(P)[..66].to_owned();
        let r = check(&dump(vec![Some(versioned(G)), Some(cut)]), &known());
        assert_eq!(r.truncated, 1, "{r:?}");
        assert_eq!(r.problems[0].of.as_deref(), Some(P));
    }

    #[test]
    fn a_muxed_account_without_its_id_is_truncated_and_its_base_is_carried() {
        let cut = versioned(M)[..66].to_owned();
        let base = known()
            .addresses
            .iter()
            .find(|a| a.starts_with('G') && *a != G)
            .cloned()
            .unwrap();
        let r = check(&dump(vec![Some(cut), Some(versioned(&base))]), &known());
        assert_eq!((r.truncated, r.fabricated), (1, 0), "{r:?}");
    }

    #[test]
    fn an_address_the_chain_never_carried_is_fabricated() {
        let mut bytes = strkey_bytes(G).unwrap();
        bytes[5] ^= 1;
        let other = hex::encode(&bytes);
        let r = check(&dump(vec![Some(versioned(G)), Some(other)]), &known());
        assert_eq!(r.fabricated, 1, "{r:?}");
        assert!(
            r.problems[0]
                .decodes_to
                .as_deref()
                .unwrap()
                .starts_with('G')
        );
    }

    #[test]
    fn unrelated_bytes_outside_address_columns_are_not_judged() {
        let r = check(&dump(vec![Some("30".repeat(40))]), &known());
        assert_eq!((r.truncated, r.fabricated), (0, 0), "{r:?}");
    }
}
