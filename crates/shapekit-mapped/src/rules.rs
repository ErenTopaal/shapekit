//! Turns one transaction's stored rows into vocabulary changes, by the passes and cases of
//! the mapping's `[export]` section.
//!
//! Rows are sorted by `export.order` and split into operations by `export.operation`. Each
//! pass takes, in order, the rows of the operation no earlier pass took that match its
//! `select`; each row goes to the first case that matches it. A case either reports the row as
//! unmapped or builds one change from it (with a partner row or an absorbed row when the case
//! says so). Rows left after the last pass are unmapped, never dropped.

use std::collections::{BTreeMap, HashMap};

use base64::Engine as _;
use serde_json::{Map, Value};
use shapekit_harness::protocol::{ExportChange, RawAddress, RowRef, Unmapped};

use crate::mapping::{
    Case, Condition, Export, Field, JsonAs, Level, Mapping, OperationEncoding, Pass,
};
use crate::strkey;
use crate::value::{Row, Val};

/// Stellar Asset Contract ids by lookup name, each to the vocabulary's asset name.
pub type Lookups = BTreeMap<String, HashMap<[u8; 32], String>>;

/// The changes and unmapped rows of one transaction.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct TxMapping {
    pub changes: Vec<ExportChange>,
    pub unmapped: Vec<Unmapped>,
}

/// The `[export]` rules of a mapping, with the lookups read from the store.
pub struct Rules<'a> {
    mapping: &'a Mapping,
    export: &'a Export,
    lookups: Lookups,
}

/// What a field read gave: a value, nothing (the field stays absent), or a reason the row
/// cannot be mapped.
enum Read {
    Set(Value),
    Absent,
    Fail(String),
}

#[derive(Default)]
struct Built {
    fields: Map<String, Value>,
    raw: BTreeMap<String, RawAddress>,
}

const LEFTOVER: &str = "no pass of the mapping takes this row";

impl<'a> Rules<'a> {
    pub fn new(m: &'a Mapping, lookups: Lookups) -> Rules<'a> {
        Rules {
            mapping: m,
            export: &m.export,
            lookups,
        }
    }

    /// The operation of a row: `None` for the transaction's own rows, else the 0-based index.
    fn operation(&self, r: &Row) -> (i64, Option<i64>) {
        let v = r.get(&self.export.operation.column).int();
        match self.export.operation.encoding {
            OperationEncoding::Toid => {
                let id = v.unwrap_or(0);
                if id == 0 {
                    (0, None)
                } else {
                    (id, Some((id & 0xfff) - 1))
                }
            }
            OperationEncoding::Index => match v {
                Some(i) => (i, Some(i)),
                None => (i64::MIN, None),
            },
        }
    }

    pub fn map_transaction(&self, mut rows: Vec<Row>) -> TxMapping {
        let order = &self.export.order;
        rows.sort_by(|a, b| {
            for c in order {
                let x = a.get(c).int().unwrap_or(0);
                let y = b.get(c).int().unwrap_or(0);
                if x != y {
                    return x.cmp(&y);
                }
            }
            std::cmp::Ordering::Equal
        });
        let mut out = TxMapping::default();
        let mut start = 0;
        while start < rows.len() {
            let (key, op) = self.operation(&rows[start]);
            let mut end = start;
            while end < rows.len() && self.operation(&rows[end]).0 == key {
                end += 1;
            }
            self.map_operation(op, &rows[start..end], &mut out);
            start = end;
        }
        out
    }

    fn map_operation(&self, op: Option<i64>, rows: &[Row], out: &mut TxMapping) {
        let mut used = vec![false; rows.len()];
        for pass in &self.export.pass {
            self.run_pass(pass, op, rows, &mut used, out);
        }
        for (i, r) in rows.iter().enumerate() {
            if !used[i] {
                out.unmapped.push(self.unmapped(r, LEFTOVER.to_owned()));
            }
        }
    }

    fn run_pass(
        &self,
        pass: &Pass,
        op: Option<i64>,
        rows: &[Row],
        used: &mut [bool],
        out: &mut TxMapping,
    ) {
        let seq: Vec<usize> = (0..rows.len())
            .filter(|&i| !used[i] && matches(&pass.select, &rows[i]))
            .collect();
        let mut taken = vec![false; seq.len()];
        let mut merged: Vec<ExportChange> = Vec::new();
        let mut merge_keys: HashMap<(usize, Vec<String>), ()> = HashMap::new();
        for k in 0..seq.len() {
            let i = seq[k];
            if taken[k] || used[i] {
                continue;
            }
            let r = &rows[i];
            let Some((ci, case)) = pass
                .cases
                .iter()
                .enumerate()
                .find(|(_, c)| case_matches(c, op, r))
            else {
                continue;
            };
            used[i] = true;
            if let Some(note) = &case.unmapped {
                out.unmapped.push(self.unmapped(r, render_note(note, r)));
                continue;
            }
            let merge_key = (!case.merge.is_empty())
                .then(|| (ci, case.merge.iter().map(|c| same_key(r.get(c))).collect()));
            if let Some(key) = &merge_key
                && merge_keys.contains_key(key)
            {
                continue;
            }
            let mut b = Built::default();
            if let Err(note) = self.read_fields(&case.fields, r, &mut b) {
                out.unmapped.push(self.unmapped(r, note));
                continue;
            }
            if let Some(abs) = &case.absorb {
                let j = (i + 1..rows.len()).find(|&j| {
                    !used[j]
                        && matches(&abs.select, &rows[j])
                        && abs
                            .same
                            .iter()
                            .all(|c| same_key(rows[j].get(c)) == same_key(r.get(c)))
                });
                if let Some(j) = j {
                    if let Err(note) = self.read_fields(&abs.fields, &rows[j], &mut b) {
                        out.unmapped.push(self.unmapped(r, note));
                        continue;
                    }
                    used[j] = true;
                }
            }
            if let Some(pair) = &case.pair {
                let partner = (k + 1 < seq.len()).then_some(k + 1).filter(|&n| {
                    let p = &rows[seq[n]];
                    let (Some(mine), Some(theirs)) =
                        (r.get(&pair.next).int(), p.get(&pair.next).int())
                    else {
                        return false;
                    };
                    !taken[n]
                        && !used[seq[n]]
                        && theirs == mine.wrapping_add(1)
                        && matches(&pair.select, p)
                        && pair
                            .same
                            .iter()
                            .all(|c| same_key(p.get(c)) == same_key(r.get(c)))
                });
                let res = match partner {
                    Some(n) => self.read_fields(&case.paired, &rows[seq[n]], &mut b),
                    None => self.read_fields(&case.unpaired, r, &mut b),
                };
                if let Err(note) = res {
                    out.unmapped.push(self.unmapped(r, note));
                    continue;
                }
                if let Some(n) = partner {
                    taken[n] = true;
                    used[seq[n]] = true;
                }
            }
            let kind = case.kind.clone().expect("validated: a case has a kind");
            let change = self.change(kind, b, op, r);
            match merge_key {
                Some(key) => {
                    merge_keys.insert(key, ());
                    merged.push(change);
                }
                None => out.changes.push(change),
            }
        }
        out.changes.extend(merged);
    }

    fn change(&self, kind: String, mut b: Built, op: Option<i64>, r: &Row) -> ExportChange {
        match op {
            Some(idx) => {
                b.fields.insert("scope".into(), Value::from("operation"));
                b.fields.insert("op_index".into(), Value::from(idx));
            }
            None => {
                b.fields
                    .insert("scope".into(), Value::from("fee-processing"));
            }
        }
        ExportChange {
            kind,
            raw: b.raw,
            source: Some(self.row_ref(r)),
            fields: b.fields,
        }
    }

    fn label(&self, r: &Row, col: &Option<String>) -> Option<String> {
        col.as_ref()
            .and_then(|c| r.get(c).text().map(str::to_owned))
            .filter(|s| !s.is_empty())
    }

    fn row_ref(&self, r: &Row) -> RowRef {
        RowRef {
            table: self.export.table.clone(),
            category: self.label(r, &self.export.category),
            reason: self.label(r, &self.export.reason),
        }
    }

    pub fn unmapped(&self, r: &Row, note: String) -> Unmapped {
        Unmapped {
            table: self.export.table.clone(),
            category: self.label(r, &self.export.category),
            reason: self.label(r, &self.export.reason),
            row: r.to_json(),
            note,
        }
    }

    fn read_fields(
        &self,
        fields: &BTreeMap<String, Field>,
        r: &Row,
        b: &mut Built,
    ) -> Result<(), String> {
        for (name, f) in fields {
            match self.read(name, f, r, b) {
                Read::Set(v) => {
                    b.fields.insert(name.clone(), v);
                }
                Read::Absent => {}
                Read::Fail(note) => return Err(note),
            }
        }
        Ok(())
    }

    fn read(&self, name: &str, f: &Field, r: &Row, b: &mut Built) -> Read {
        let fail = |default: String| Read::Fail(f.note.clone().unwrap_or(default));
        if let Some(col) = &f.address {
            let Some(bytes) = r.get(col).bytes() else {
                return Read::Absent;
            };
            let allowed = match f.forms.as_ref().map(|x| self.mapping.forms(x)) {
                Some(Ok(a)) => a,
                _ => unreachable!("validated: an address field has known forms"),
            };
            let mut raw = RawAddress {
                column: col.clone(),
                hex: hex::encode(bytes),
                strkey: None,
                error: None,
            };
            let v = match strkey::decode_stored(bytes, &allowed) {
                Ok(s) => {
                    raw.strkey = Some(s.clone());
                    Value::String(s)
                }
                Err(e) => {
                    raw.error = Some(e);
                    Value::Null
                }
            };
            b.raw.insert(name.to_owned(), raw);
            return Read::Set(v);
        }
        if let Some(col) = &f.int {
            return match r.get(col).int() {
                Some(v) => Read::Set(Value::from(v)),
                None => Read::Absent,
            };
        }
        if let Some(col) = &f.text {
            let s = match r.get(col).text() {
                Some(s) => s.to_owned(),
                None => match &f.default {
                    Some(d) => d.clone(),
                    None => return Read::Absent,
                },
            };
            if f.skip_empty && s.is_empty() {
                return Read::Absent;
            }
            return Read::Set(match &f.typed {
                Some(t) => serde_json::json!({"type": t, "value": s}),
                None => Value::String(s),
            });
        }
        if let Some(col) = &f.equals {
            let want = toml_text(f.value.as_ref().expect("validated: equals has a value"));
            return Read::Set(Value::Bool(
                r.get(col).condition_text().as_deref() == Some(want.as_str()),
            ));
        }
        if let Some(v) = &f.value {
            return Read::Set(toml_json(v));
        }
        if let Some(col) = &f.stroops {
            let Some(s) = r.get(col).text() else {
                return Read::Absent;
            };
            return match stroops(s) {
                Some(v) => Read::Set(Value::String(v)),
                None => fail(format!("{col} is not an amount with at most 7 decimals")),
            };
        }
        if let Some(col) = &f.negate {
            let s = match r.get(col).text() {
                Some(s) => s.to_owned(),
                None => match &f.default {
                    Some(d) => d.clone(),
                    None => return Read::Absent,
                },
            };
            return match negate(&s) {
                Some(v) => Read::Set(Value::String(v)),
                None => fail(format!("{col} is not a decimal integer")),
            };
        }
        if let Some(col) = &f.text_hex {
            return match r.get(col).text() {
                Some(s) => Read::Set(Value::String(hex::encode(s.as_bytes()))),
                None => fail(format!("{col} is null")),
            };
        }
        if let Some(col) = &f.json {
            let path = f.path.as_deref().expect("validated: json has a path");
            let at = json_at(r.get(col), path);
            return match f.json_as.expect("validated: json has as") {
                JsonAs::String => match at {
                    Some(Value::String(s)) => Read::Set(Value::String(s.clone())),
                    _ => match &f.default {
                        Some(d) => Read::Set(Value::String(d.clone())),
                        None => Read::Absent,
                    },
                },
                JsonAs::Base64Hex => match at {
                    None => Read::Absent,
                    Some(Value::String(s)) => {
                        match base64::engine::general_purpose::STANDARD.decode(s) {
                            Ok(raw) => Read::Set(Value::String(hex::encode(raw))),
                            Err(e) => fail(format!("{col}.{path} is not base64: {e}")),
                        }
                    }
                    Some(_) => fail(format!("{col}.{path} is not a base64 string")),
                },
                JsonAs::Uint32 => match at.and_then(uint32_of) {
                    Some(v) => Read::Set(Value::from(v)),
                    None => Read::Absent,
                },
            };
        }
        if let Some(col) = &f.bits {
            let Some(v) = r.get(col).int() else {
                return Read::Absent;
            };
            let mut out: u32 = 0;
            for (bit, to) in &f.map {
                let bit: u32 = bit.parse().expect("validated: bits are numbers");
                if v & (1i64 << bit) != 0 {
                    out |= to;
                }
            }
            return Read::Set(Value::from(out));
        }
        if f.asset.is_some() || f.pool.is_some() {
            return match self.asset(f, r) {
                Ok(a) => Read::Set(Value::String(a)),
                Err(note) => fail(note),
            };
        }
        unreachable!("validated: every field has a source")
    }

    /// A pool id (hex text) as `pool-share:<L strkey>`, or an asset contract address as the
    /// asset the lookup names.
    fn asset(&self, f: &Field, r: &Row) -> Result<String, String> {
        if let Some(pcol) = &f.pool
            && let Some(lp) = r.get(pcol).text()
            && !lp.is_empty()
        {
            return match hex::decode(lp) {
                Ok(id) if id.len() == 32 => Ok(format!(
                    "pool-share:{}",
                    strkey::encode(strkey::version_byte("L").expect("L is a letter"), &id)
                )),
                _ => Err(format!("{pcol} is not a 32-byte hex pool id")),
            };
        }
        let col = f.asset.as_deref().unwrap_or_default();
        let Some(tok) = r.get(col).bytes() else {
            return Err("row names neither a token nor a liquidity pool".into());
        };
        if tok.len() != 33 {
            return Err(format!(
                "{col} holds {} bytes, not a 33-byte contract address",
                tok.len()
            ));
        }
        let mut id = [0u8; 32];
        id.copy_from_slice(&tok[1..]);
        let name = f.lookup.as_deref().unwrap_or_default();
        self.lookups
            .get(name)
            .and_then(|m| m.get(&id))
            .cloned()
            .ok_or_else(|| format!("{col} is not the asset contract of any asset in lookup {name}"))
    }
}

fn matches(cond: &Condition, r: &Row) -> bool {
    cond.iter().all(|(col, want)| {
        r.get(col)
            .condition_text()
            .is_some_and(|v| want.contains(&v))
    })
}

fn version_in(r: &Row, col: &str, letters: &[String]) -> Option<bool> {
    let b = r.get(col).bytes().filter(|b| !b.is_empty())?;
    Some(
        letters
            .iter()
            .any(|l| strkey::version_byte(l) == Some(b[0])),
    )
}

fn case_matches(c: &Case, op: Option<i64>, r: &Row) -> bool {
    let level_ok = match c.level {
        None => true,
        Some(Level::Transaction) => op.is_none(),
        Some(Level::Operation) => op.is_some(),
    };
    level_ok
        && matches(&c.when, r)
        && c.when_version
            .iter()
            .all(|(col, l)| version_in(r, col, l) == Some(true))
        && c.when_version_not
            .iter()
            .all(|(col, l)| version_in(r, col, l) == Some(false))
}

/// How `same`, `merge` and pairing compare cells: bytes by their hex, text as it is, NULL as
/// the empty string.
fn same_key(v: &Val) -> String {
    match v {
        Val::Null => String::new(),
        Val::Bytes(b) => hex::encode(b),
        other => other.condition_text().unwrap_or_default(),
    }
}

/// `{<column>:version}` in a note is the column's first byte, as `0x..`.
fn render_note(note: &str, r: &Row) -> String {
    let mut out = String::new();
    let mut rest = note;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        match tail.find('}') {
            Some(end) if tail[1..end].ends_with(":version") => {
                let col = &tail[1..end - ":version".len()];
                match r.get(col).bytes().and_then(|b| b.first()) {
                    Some(v) => out.push_str(&format!("0x{v:02x}")),
                    None => out.push_str("none"),
                }
                rest = &tail[end + 1..];
            }
            _ => {
                out.push('{');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn json_at<'v>(v: &'v Val, path: &str) -> Option<&'v Value> {
    let Val::Json(j) = v else { return None };
    let mut cur = j;
    for key in path.split('.') {
        cur = cur.as_object()?.get(key)?;
    }
    Some(cur)
}

fn uint32_of(v: &Value) -> Option<u32> {
    match v {
        Value::Number(n) => {
            if let Some(u) = n.as_u64() {
                return u32::try_from(u).ok();
            }
            let f = n.as_f64()?;
            (f >= 0.0 && f <= f64::from(u32::MAX) && f.fract() == 0.0).then_some(f as u32)
        }
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn toml_json(v: &toml::Value) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

fn toml_text(v: &toml::Value) -> String {
    match v {
        toml::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// A decimal amount with at most 7 fraction digits as the exact integer number of stroops.
pub fn stroops(v: &str) -> Option<String> {
    let (whole, frac) = v.split_once('.').unwrap_or((v, ""));
    if whole.is_empty() || frac.len() > 7 {
        return None;
    }
    if !whole
        .bytes()
        .chain(frac.bytes())
        .all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let digits = format!("{whole}{frac}{}", "0".repeat(7 - frac.len()));
    Some(canonical_digits(&digits))
}

fn canonical_digits(d: &str) -> String {
    let t = d.trim_start_matches('0');
    if t.is_empty() { "0".into() } else { t.into() }
}

/// The negation of a decimal integer with an optional sign, in canonical form.
pub fn negate(s: &str) -> Option<String> {
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let d = canonical_digits(digits);
    Some(if d == "0" || neg { d } else { format!("-{d}") })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts() {
        for (i, o) in [
            ("922337203685.4775807", "9223372036854775807"),
            ("1", "10000000"),
            ("0.5", "5000000"),
            ("12.0000001", "120000001"),
        ] {
            assert_eq!(stroops(i).as_deref(), Some(o));
        }
        for bad in ["", "-1", "1.00000001", "1e5", ".5"] {
            assert_eq!(stroops(bad), None, "{bad}");
        }
        assert_eq!(negate("100").as_deref(), Some("-100"));
        assert_eq!(negate("-7").as_deref(), Some("7"));
        assert_eq!(negate("0").as_deref(), Some("0"));
        assert_eq!(negate(""), None);
        assert_eq!(negate("1x"), None);
    }
}
