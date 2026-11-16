//! Direct comparison with core's records: the final ledger entries core wrote into the meta
//! of the ledgers the target ingested, next to the rows the target keeps for them.
//!
//! Which table and column stands for which entry and field is not written by hand. It is
//! inferred from dumps (`infer`): key columns are the columns the carry check found carrying
//! account, contract or pool addresses, or ids of rows that render as an asset or a
//! contract; a field column is one whose every keyed row equals the same entry field. The
//! inference over several chains keeps only what holds in all of them (`merge`), and the
//! result is a mapping file that `check` applies to any chain.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use shapekit_oracle::decode::LcmStream;
use shapekit_oracle::network::Network;
use shapekit_oracle::state::LedgerState;
use shapekit_oracle::vocab::trustline_asset_id;
use stellar_xdr::{
    AccountEntryExt, AccountEntryExtensionV1Ext, ContractDataDurability, LedgerEntry,
    LedgerEntryData, LiquidityPoolEntryBody, ScVal, TrustLineEntryExt,
};

use super::known::Known;
use crate::dump::{Dump, Table};

/// The entry kinds compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryKind {
    /// Keys: `account`.
    Account,
    /// Keys: `account`, `asset` (`CODE:ISSUER` or `pool-share:L...`).
    Trustline,
    /// A Stellar Asset Contract balance entry. Keys: `holder`, `contract`.
    SacBalance,
    /// Keys: `pool`.
    LiquidityPool,
}

impl EntryKind {
    pub const ALL: [EntryKind; 4] = [
        EntryKind::Account,
        EntryKind::Trustline,
        EntryKind::SacBalance,
        EntryKind::LiquidityPool,
    ];

    pub fn name(self) -> &'static str {
        match self {
            EntryKind::Account => "account",
            EntryKind::Trustline => "trustline",
            EntryKind::SacBalance => "sac-balance",
            EntryKind::LiquidityPool => "liquidity-pool",
        }
    }

    fn key_fields(self) -> &'static [&'static str] {
        match self {
            EntryKind::Account => &["account"],
            EntryKind::Trustline => &["account", "asset"],
            EntryKind::SacBalance => &["holder", "contract"],
            EntryKind::LiquidityPool => &["pool"],
        }
    }
}

type Fields = BTreeMap<String, String>;

/// Core's final entries, as text the way a store writes them: integers in decimal, booleans
/// as `true` and `false`.
#[derive(Debug, Clone, Default)]
pub struct CoreState {
    pub entries: BTreeMap<(EntryKind, Vec<String>), Fields>,
    /// Each entry's XDR with the holder of the key (the account, the balance holder or
    /// the pool id) set to zero bytes: two entries with the same bytes here are records
    /// core holds identically except for whose they are.
    pub unkeyed: BTreeMap<(EntryKind, Vec<String>), Vec<u8>>,
}

fn flag(v: bool) -> String {
    if v { "true" } else { "false" }.into()
}

fn i128_text(p: &stellar_xdr::Int128Parts) -> String {
    let v = (i128::from(p.hi) << 64) | i128::from(p.lo);
    v.to_string()
}

/// The fields of a Stellar Asset Contract balance value: a map of `amount`, `authorized`
/// and `clawback` (the asset contract's `BalanceValue`).
fn sac_balance(key: &ScVal, val: &ScVal) -> Option<(String, Fields)> {
    let ScVal::Vec(Some(k)) = key else {
        return None;
    };
    let [ScVal::Symbol(s), ScVal::Address(holder)] = k.as_slice() else {
        return None;
    };
    if s.as_slice() != b"Balance" {
        return None;
    }
    let ScVal::Map(Some(m)) = val else {
        return None;
    };
    let mut f = Fields::new();
    for e in m.iter() {
        let ScVal::Symbol(name) = &e.key else {
            return None;
        };
        let text = match &e.val {
            ScVal::I128(p) => i128_text(p),
            ScVal::Bool(b) => flag(*b),
            _ => return None,
        };
        f.insert(String::from_utf8_lossy(name.as_slice()).into_owned(), text);
    }
    if !f.contains_key("amount") {
        return None;
    }
    Some((shapekit_oracle::addr::sc_address(holder).0, f))
}

impl CoreState {
    fn add(&mut self, e: &LedgerEntry) {
        let mut f = Fields::new();
        f.insert(
            "last_modified_ledger".into(),
            e.last_modified_ledger_seq.to_string(),
        );
        let (kind, key) = match &e.data {
            LedgerEntryData::Account(a) => {
                f.insert("balance".into(), a.balance.to_string());
                f.insert("seq_num".into(), a.seq_num.0.to_string());
                f.insert("num_sub_entries".into(), a.num_sub_entries.to_string());
                f.insert("flags".into(), a.flags.to_string());
                f.insert(
                    "home_domain".into(),
                    String::from_utf8_lossy(a.home_domain.as_slice()).into_owned(),
                );
                f.insert("master_weight".into(), a.thresholds.0[0].to_string());
                f.insert("low_threshold".into(), a.thresholds.0[1].to_string());
                f.insert("med_threshold".into(), a.thresholds.0[2].to_string());
                f.insert("high_threshold".into(), a.thresholds.0[3].to_string());
                f.insert("num_signers".into(), a.signers.len().to_string());
                let (mut buying, mut selling, mut sponsored, mut sponsoring) = (0, 0, 0, 0);
                if let AccountEntryExt::V1(v1) = &a.ext {
                    buying = v1.liabilities.buying;
                    selling = v1.liabilities.selling;
                    if let AccountEntryExtensionV1Ext::V2(v2) = &v1.ext {
                        sponsored = v2.num_sponsored;
                        sponsoring = v2.num_sponsoring;
                    }
                }
                f.insert("buying_liabilities".into(), buying.to_string());
                f.insert("selling_liabilities".into(), selling.to_string());
                f.insert("num_sponsored".into(), sponsored.to_string());
                f.insert("num_sponsoring".into(), sponsoring.to_string());
                (
                    EntryKind::Account,
                    vec![shapekit_oracle::addr::account(&a.account_id)],
                )
            }
            LedgerEntryData::Trustline(t) => {
                f.insert("balance".into(), t.balance.to_string());
                f.insert("limit".into(), t.limit.to_string());
                f.insert("flags".into(), t.flags.to_string());
                f.insert("authorized".into(), flag(t.flags & 1 != 0));
                f.insert(
                    "authorized_to_maintain_liabilities".into(),
                    flag(t.flags & 2 != 0),
                );
                f.insert("clawback_enabled".into(), flag(t.flags & 4 != 0));
                let (b, s) = match &t.ext {
                    TrustLineEntryExt::V0 => (0, 0),
                    TrustLineEntryExt::V1(v1) => (v1.liabilities.buying, v1.liabilities.selling),
                };
                f.insert("buying_liabilities".into(), b.to_string());
                f.insert("selling_liabilities".into(), s.to_string());
                (
                    EntryKind::Trustline,
                    vec![
                        shapekit_oracle::addr::account(&t.account_id),
                        trustline_asset_id(&t.asset),
                    ],
                )
            }
            LedgerEntryData::ContractData(d) => {
                if d.durability != ContractDataDurability::Persistent {
                    return;
                }
                let Some((holder, fields)) = sac_balance(&d.key, &d.val) else {
                    return;
                };
                f.extend(fields);
                (
                    EntryKind::SacBalance,
                    vec![holder, shapekit_oracle::addr::sc_address(&d.contract).0],
                )
            }
            LedgerEntryData::LiquidityPool(p) => {
                let LiquidityPoolEntryBody::LiquidityPoolConstantProduct(c) = &p.body;
                f.insert("reserve_a".into(), c.reserve_a.to_string());
                f.insert("reserve_b".into(), c.reserve_b.to_string());
                f.insert("total_pool_shares".into(), c.total_pool_shares.to_string());
                f.insert(
                    "pool_shares_trust_line_count".into(),
                    c.pool_shares_trust_line_count.to_string(),
                );
                (
                    EntryKind::LiquidityPool,
                    vec![shapekit_oracle::addr::pool(&p.liquidity_pool_id)],
                )
            }
            _ => return,
        };
        if let Some(b) = unkeyed_xdr(e) {
            self.unkeyed.insert((kind, key.clone()), b);
        }
        self.entries.insert((kind, key), f);
    }

    /// Core's entries after applying the chain's ledgers up to `through` to genesis.
    pub fn from_chain(chain_dir: &Path, passphrase: &str, through: u32) -> Result<CoreState> {
        let net = Network::new(passphrase);
        let mut state = LedgerState::genesis(&net);
        let path = chain_dir.join("ledgers.xdr");
        for l in LcmStream::open(&path).with_context(|| format!("opening {}", path.display()))? {
            let l = l.with_context(|| format!("reading {}", path.display()))?;
            if shapekit_pack::lake::seq_of(&l.meta) > through {
                break;
            }
            state
                .apply_ledger(&l.meta, &net)
                .with_context(|| format!("applying {}", path.display()))?;
        }
        if state.head_seq() != Some(through) {
            bail!(
                "{} ends at ledger {:?}, the target ingested through {through}",
                path.display(),
                state.head_seq()
            );
        }
        let mut s = CoreState::default();
        for (_, e) in state.iter() {
            s.add(e);
        }
        Ok(s)
    }
}

fn zero_account() -> stellar_xdr::AccountId {
    stellar_xdr::AccountId(stellar_xdr::PublicKey::PublicKeyTypeEd25519(
        stellar_xdr::Uint256([0; 32]),
    ))
}

/// The entry's XDR with its holder zeroed, for the kinds compared.
fn unkeyed_xdr(e: &LedgerEntry) -> Option<Vec<u8>> {
    use stellar_xdr::WriteXdr;
    let mut e = e.clone();
    match &mut e.data {
        LedgerEntryData::Account(a) => a.account_id = zero_account(),
        LedgerEntryData::Trustline(t) => t.account_id = zero_account(),
        LedgerEntryData::LiquidityPool(p) => {
            p.liquidity_pool_id = stellar_xdr::PoolId(stellar_xdr::Hash([0; 32]))
        }
        LedgerEntryData::ContractData(d) => {
            let ScVal::Vec(Some(v)) = &mut d.key else {
                return None;
            };
            let mut items: Vec<ScVal> = v.to_vec();
            if let Some(ScVal::Address(a)) = items.get_mut(1) {
                *a = stellar_xdr::ScAddress::Account(zero_account());
            }
            d.key = ScVal::Vec(Some(items.try_into().ok()?));
        }
        _ => return None,
    }
    e.to_xdr(stellar_xdr::Limits::none()).ok()
}

/// How a key column's cell becomes a key value: the cell itself (decoded through the known
/// addresses when it is an address in a known encoding), or the row of another table the
/// cell names, rendered with a format such as `{code}:{issuer}`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Via {
    pub table: String,
    pub id: String,
    pub format: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct KeyColumn {
    /// The entry's key field, for example `account`.
    pub field: String,
    pub column: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<Via>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableMapping {
    pub name: String,
    pub entry: EntryKind,
    pub keys: Vec<KeyColumn>,
    /// Column to the entry field it holds.
    pub fields: BTreeMap<String, String>,
    /// Columns that held more than one field in every chain inferred from, with the
    /// candidates. Not compared.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ambiguous: BTreeMap<String, Vec<String>>,
    /// Chains the mapping was inferred from, and keyed rows it held on.
    pub chains: u64,
    pub rows: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mapping {
    #[serde(default)]
    pub table: Vec<TableMapping>,
}

impl Mapping {
    pub fn load(path: &Path) -> Result<Mapping> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn render(&self, header: &str) -> Result<String> {
        let mut s: String = header.lines().map(|l| format!("# {l}\n")).collect();
        s.push('\n');
        s.push_str(&toml::to_string(self).context("rendering the mapping")?);
        Ok(s)
    }
}

fn render(format: &str, table: &Table, row: &[Option<String>], known: &Known) -> Option<String> {
    let mut out = String::new();
    let mut rest = format;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let close = rest[open..].find('}')? + open;
        let col = table.column(&rest[open + 1..close])?;
        out.push_str(&resolve_cell(row[col].as_deref()?, known));
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// A cell as the value it stands for: a known address in any encoding becomes its strkey.
fn resolve_cell(cell: &str, known: &Known) -> String {
    match known.forms.get(cell) {
        Some(f) if f.address => f.value.clone(),
        _ => cell.to_owned(),
    }
}

/// Resolves the key columns of one row.
pub(crate) fn row_key(
    dump: &Dump,
    table: &Table,
    row: &[Option<String>],
    keys: &[KeyColumn],
    known: &Known,
) -> Option<Vec<String>> {
    keys.iter()
        .map(|k| {
            let cell = row[table.column(&k.column)?].as_deref()?;
            match &k.via {
                None => Some(resolve_cell(cell, known)),
                Some(v) => {
                    let t = dump.tables.get(&v.table)?;
                    let id = t.column(&v.id)?;
                    let r = t.rows.iter().find(|r| r[id].as_deref() == Some(cell))?;
                    render(&v.format, t, r, known)
                }
            }
        })
        .collect()
}

/// One table's inferred mapping on one chain.
#[derive(Debug, Clone)]
struct Inferred {
    entry: EntryKind,
    keys: Vec<KeyColumn>,
    rows: u64,
    /// Column to the fields it equals on every keyed row.
    candidates: BTreeMap<String, BTreeSet<String>>,
}

/// Formats a lookup table can render its rows with: each column alone and each ordered
/// pair joined by `:`.
fn formats(t: &Table) -> Vec<String> {
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    let mut out: Vec<String> = names.iter().map(|n| format!("{{{n}}}")).collect();
    for a in &names {
        for b in &names {
            if a != b {
                out.push(format!("{{{a}}}:{{{b}}}"));
            }
        }
    }
    out
}

/// Every way a column's cells can resolve to key values of `field`, judged by how many
/// cells land on a key value core has for that field.
fn key_options(
    dump: &Dump,
    table: &Table,
    ci: usize,
    known: &Known,
    values: &BTreeSet<String>,
) -> Vec<(Option<Via>, u64)> {
    let mut out = Vec::new();
    let cells: Vec<&str> = table.rows.iter().filter_map(|r| r[ci].as_deref()).collect();
    if cells.is_empty() {
        return out;
    }
    let direct = cells
        .iter()
        .filter(|c| values.contains(&resolve_cell(c, known)))
        .count() as u64;
    if direct > 0 {
        out.push((None, direct));
    }
    for (vname, vt) in &dump.tables {
        for (idi, idc) in vt.columns.iter().enumerate() {
            let ids: BTreeMap<&str, &Vec<Option<String>>> = vt
                .rows
                .iter()
                .filter_map(|r| r[idi].as_deref().map(|i| (i, r)))
                .collect();
            if ids.len() != vt.rows.len() || !cells.iter().all(|c| ids.contains_key(c)) {
                continue;
            }
            for f in formats(vt) {
                let hits = cells
                    .iter()
                    .filter(|c| render(&f, vt, ids[*c], known).is_some_and(|v| values.contains(&v)))
                    .count() as u64;
                if hits > 0 {
                    out.push((
                        Some(Via {
                            table: vname.clone(),
                            id: idc.name.clone(),
                            format: f,
                        }),
                        hits,
                    ));
                }
            }
        }
    }
    out
}

fn infer_table(dump: &Dump, table: &Table, core: &CoreState, known: &Known) -> Option<Inferred> {
    let mut best: Option<Inferred> = None;
    for kind in EntryKind::ALL {
        let fields = kind.key_fields();
        // key values core has, per key field
        let per_field: Vec<BTreeSet<String>> = (0..fields.len())
            .map(|i| {
                core.entries
                    .keys()
                    .filter(|(k, _)| *k == kind)
                    .map(|(_, key)| key[i].clone())
                    .collect()
            })
            .collect();
        if per_field[0].is_empty() {
            continue;
        }
        // the best option for each key field, each from a different column
        let mut keys = Vec::new();
        let mut used = BTreeSet::new();
        for (fi, field) in fields.iter().enumerate() {
            let mut choice: Option<(KeyColumn, u64)> = None;
            for (ci, c) in table.columns.iter().enumerate() {
                if used.contains(&ci) {
                    continue;
                }
                for (via, hits) in key_options(dump, table, ci, known, &per_field[fi]) {
                    if choice.as_ref().is_none_or(|(_, h)| hits > *h) {
                        choice = Some((
                            KeyColumn {
                                field: (*field).to_owned(),
                                column: c.name.clone(),
                                via,
                            },
                            hits,
                        ));
                    }
                }
            }
            let (k, _) = choice?;
            used.insert(table.column(&k.column).expect("chosen from the table"));
            keys.push(k);
        }
        let mut rows = 0u64;
        let mut candidates: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let key_cols: BTreeSet<&str> = keys.iter().map(|k| k.column.as_str()).collect();
        for row in &table.rows {
            let Some(key) = row_key(dump, table, row, &keys, known) else {
                continue;
            };
            let Some(f) = core.entries.get(&(kind, key)) else {
                continue;
            };
            for (ci, c) in table.columns.iter().enumerate() {
                if key_cols.contains(c.name.as_str()) {
                    continue;
                }
                let matching: BTreeSet<String> = match row[ci].as_deref() {
                    Some(cell) => f
                        .iter()
                        .filter(|(_, v)| *v == cell)
                        .map(|(k, _)| k.clone())
                        .collect(),
                    None => BTreeSet::new(),
                };
                match candidates.get_mut(&c.name) {
                    Some(s) => s.retain(|x| matching.contains(x)),
                    None if rows == 0 => {
                        candidates.insert(c.name.clone(), matching);
                    }
                    None => {}
                }
            }
            rows += 1;
        }
        if rows == 0 {
            continue;
        }
        candidates.retain(|_, s| !s.is_empty());
        // a table that only refers to entries (history rows naming an account) holds no
        // state to compare
        if candidates.is_empty() {
            continue;
        }
        if best.as_ref().is_none_or(|b| rows > b.rows) {
            best = Some(Inferred {
                entry: kind,
                keys,
                rows,
                candidates,
            });
        }
    }
    best
}

/// One chain's dump next to core's entries: per table, the entry kind and key columns that
/// match the most rows, and the fields each other column equals on every one of them.
pub fn infer(dump: &Dump, core: &CoreState, known: &Known) -> Mapping {
    let mut m = Mapping::default();
    for (name, table) in &dump.tables {
        if let Some(i) = infer_table(dump, table, core, known) {
            m.table.push(TableMapping {
                name: name.clone(),
                entry: i.entry,
                keys: i.keys,
                fields: BTreeMap::new(),
                ambiguous: i
                    .candidates
                    .into_iter()
                    .map(|(c, s)| (c, s.into_iter().collect()))
                    .collect(),
                chains: 1,
                rows: i.rows,
            });
        }
    }
    m
}

/// Keeps what holds in every chain: a table's entry kind and keys must agree everywhere it
/// was inferred (a table that disagrees is dropped and named in the returned notes), and a
/// column keeps only the fields it equalled in every chain. A column left with one field
/// is mapped; with several, the one named like the column is taken when there is one, and
/// otherwise the column stays ambiguous.
pub fn merge(per_chain: &[Mapping]) -> (Mapping, Vec<String>) {
    let mut by_table: BTreeMap<String, Vec<&TableMapping>> = BTreeMap::new();
    for m in per_chain {
        for t in &m.table {
            by_table.entry(t.name.clone()).or_default().push(t);
        }
    }
    let mut notes = Vec::new();
    let mut out = Mapping::default();
    for (name, ts) in by_table {
        let first = ts[0];
        if ts
            .iter()
            .any(|t| t.entry != first.entry || t.keys != first.keys)
        {
            let ways: BTreeSet<String> = ts
                .iter()
                .map(|t| format!("{} keyed by {:?}", t.entry.name(), t.keys))
                .collect();
            notes.push(format!(
                "{name}: chains disagree on what it holds: {}",
                ways.into_iter().collect::<Vec<_>>().join("; ")
            ));
            continue;
        }
        let mut cand: BTreeMap<String, BTreeSet<String>> = first
            .ambiguous
            .iter()
            .map(|(c, v)| (c.clone(), v.iter().cloned().collect()))
            .collect();
        for t in &ts[1..] {
            cand.retain(|c, s| {
                let other: BTreeSet<String> =
                    t.ambiguous.get(c).into_iter().flatten().cloned().collect();
                s.retain(|x| other.contains(x));
                !s.is_empty()
            });
        }
        let mut fields = BTreeMap::new();
        let mut ambiguous = BTreeMap::new();
        for (c, s) in cand {
            if s.len() == 1 {
                fields.insert(c, s.into_iter().next().expect("one field"));
            } else if let Some(same) = s.iter().find(|f| norm(f) == norm(&c)) {
                fields.insert(c.clone(), same.clone());
            } else {
                ambiguous.insert(c, s.into_iter().collect());
            }
        }
        if fields.is_empty() && ambiguous.is_empty() {
            notes.push(format!(
                "{name}: no column equals the same {} field in every chain",
                first.entry.name()
            ));
            continue;
        }
        out.table.push(TableMapping {
            name,
            entry: first.entry,
            keys: first.keys.clone(),
            fields,
            ambiguous,
            chains: ts.len() as u64,
            rows: ts.iter().map(|t| t.rows).sum(),
        });
    }
    (out, notes)
}

fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mismatch {
    pub table: String,
    pub key: Vec<String>,
    pub column: String,
    pub field: String,
    pub stored: Option<String>,
    pub core: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateReport {
    /// Rows compared with a core entry.
    pub rows: u64,
    pub fields_compared: u64,
    pub mismatches: Vec<Mismatch>,
    /// Rows whose key core has no entry for at the last ingested ledger (removed entries
    /// still stored, or keys core never had): `table: key`.
    pub stale: Vec<String>,
    /// Core entries of a mapped table's kind with no row: `table: key`.
    pub missing: Vec<String>,
    /// Rows whose key columns do not resolve.
    pub unresolved: u64,
    pub ok: bool,
}

/// Applies `mapping` to one chain's dump.
pub fn check(dump: &Dump, core: &CoreState, known: &Known, mapping: &Mapping) -> StateReport {
    let mut r = StateReport::default();
    for t in &mapping.table {
        let Some(table) = dump.tables.get(&t.name) else {
            continue;
        };
        let mut seen = BTreeSet::new();
        for row in &table.rows {
            let Some(key) = row_key(dump, table, row, &t.keys, known) else {
                r.unresolved += 1;
                continue;
            };
            seen.insert(key.clone());
            let Some(f) = core.entries.get(&(t.entry, key.clone())) else {
                r.stale.push(format!("{}: {}", t.name, key.join(" ")));
                continue;
            };
            r.rows += 1;
            for (col, field) in &t.fields {
                let Some(ci) = table.column(col) else {
                    continue;
                };
                let Some(want) = f.get(field) else {
                    continue;
                };
                r.fields_compared += 1;
                if row[ci].as_deref() != Some(want.as_str()) {
                    r.mismatches.push(Mismatch {
                        table: t.name.clone(),
                        key: key.clone(),
                        column: col.clone(),
                        field: field.clone(),
                        stored: row[ci].clone(),
                        core: want.clone(),
                    });
                }
            }
        }
        for (kind, key) in core.entries.keys() {
            if *kind == t.entry && !seen.contains(key) {
                r.missing.push(format!("{}: {}", t.name, key.join(" ")));
            }
        }
    }
    r.ok = r.mismatches.is_empty() && r.stale.is_empty() && r.missing.is_empty();
    r
}

/// One chain of a harness run, with what the state comparison needs.
pub struct RunChain {
    pub chain: String,
    pub network: String,
    pub dump: Dump,
    pub core: CoreState,
    pub known: Known,
    /// Verdict counts of the chain's entries in the run.
    pub verdicts: BTreeMap<String, u64>,
    /// No entry mishandled or dropped, the chain ran without an error and the carry check
    /// found nothing: the chains the mapping is inferred on.
    pub ordinary: bool,
}

/// Every chain of the run in `out` (a `harness run --out` directory) that kept a dump.
pub fn load_run(out: &Path, corpus: &Path) -> Result<Vec<RunChain>> {
    let path = out.join(crate::report::REPORT_FILE);
    let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let rep: crate::report::Report =
        serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))?;
    let logs = crate::run::logs_dir(out, 1, rep.rounds.len() as u32);
    let mut chains = Vec::new();
    for c in &rep.chains {
        let (Some(d), Some(through)) = (&c.dump, c.last_ingested) else {
            continue;
        };
        let dir = corpus.join("chains").join(&c.chain).join(&c.network);
        let chain = shapekit_pack::chain::ChainFile::read(&dir)
            .with_context(|| format!("reading {}", dir.display()))?;
        let mut verdicts = BTreeMap::new();
        for e in rep
            .entries
            .iter()
            .filter(|e| e.chain == c.chain && e.network == c.network)
        {
            *verdicts
                .entry(e.result.verdict.name().to_owned())
                .or_insert(0) += 1;
        }
        let carry_clean = c
            .carry
            .as_ref()
            .is_some_and(|x| x.truncated == 0 && x.fabricated == 0);
        let ordinary = c.error.is_none()
            && carry_clean
            && !verdicts.contains_key("mishandled")
            && !verdicts.contains_key("dropped")
            && verdicts.contains_key("pass");
        chains.push(RunChain {
            chain: c.chain.clone(),
            network: c.network.clone(),
            dump: Dump::read(&logs.join(&d.file))?,
            core: CoreState::from_chain(&dir, &chain.passphrase, through)?,
            known: Known::from_chain(&dir, &chain.passphrase, through)?,
            verdicts,
            ordinary,
        });
    }
    Ok(chains)
}

/// The state comparison of one chain, next to the oracle's verdicts on it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainState {
    pub chain: String,
    pub network: String,
    pub ordinary: bool,
    pub verdicts: BTreeMap<String, u64>,
    pub state: StateReport,
    /// Records core holds equally, and whether the target does too.
    pub route: super::route::RouteReport,
    /// Where the state comparison and the oracle's verdicts tell different stories.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disagreement: Option<String>,
}

pub fn check_chain(c: &RunChain, mapping: &Mapping) -> ChainState {
    let state = check(&c.dump, &c.core, &c.known, mapping);
    let oracle_problem =
        c.verdicts.contains_key("mishandled") || c.verdicts.contains_key("dropped");
    let disagreement = match (state.ok, oracle_problem) {
        (false, false) => Some(format!(
            "the stored state differs from core's ({} mismatched fields, {} stale rows, {} missing rows) where the oracle finds no problem",
            state.mismatches.len(),
            state.stale.len(),
            state.missing.len()
        )),
        (true, true) => Some(
            "the oracle finds entries mishandled or dropped, the stored state of the mapped tables equals core's"
                .into(),
        ),
        _ => None,
    };
    ChainState {
        chain: c.chain.clone(),
        network: c.network.clone(),
        ordinary: c.ordinary,
        verdicts: c.verdicts.clone(),
        route: super::route::check(&c.dump, &c.core, &c.known, mapping),
        state,
        disagreement,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dump::Table;
    use crate::protocol::DumpColumn;
    use crate::relations::known::strkey_bytes;

    const G: &str = "GB4WH2VTILAXT2GUDH4KI2ZA2GGYSLJJUYBH6LP42LXI4WNBRIHHDDVT";

    fn setup(balance: &str) -> (Dump, CoreState, Known) {
        let mut core = CoreState::default();
        let mut f = Fields::new();
        f.insert("balance".into(), "500".into());
        f.insert("num_sub_entries".into(), "0".into());
        f.insert("flags".into(), "0".into());
        core.entries.insert((EntryKind::Account, vec![G.into()]), f);
        let mut known = Known::default();
        known.add_address(G);
        let col = |n: &str, t: &str| DumpColumn {
            name: n.into(),
            ty: t.into(),
        };
        let mut dump = Dump::default();
        dump.tables.insert(
            "balances".into(),
            Table {
                columns: vec![
                    col("account_id", "bytea"),
                    col("balance", "bigint"),
                    col("seen", "text"),
                ],
                rows: vec![vec![
                    Some(hex::encode(strkey_bytes(G).unwrap())),
                    Some(balance.into()),
                    Some("x".into()),
                ]],
            },
        );
        dump.tables.insert(
            "history".into(),
            Table {
                columns: vec![col("account_id", "bytea"), col("note", "text")],
                rows: vec![vec![
                    Some(hex::encode(strkey_bytes(G).unwrap())),
                    Some("y".into()),
                ]],
            },
        );
        (dump, core, known)
    }

    #[test]
    fn a_balance_column_is_inferred_and_history_tables_are_left_out() {
        let (dump, core, known) = setup("500");
        let (m, notes) = merge(&[infer(&dump, &core, &known)]);
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(m.table.len(), 1, "{m:?}");
        let t = &m.table[0];
        assert_eq!((t.name.as_str(), t.entry), ("balances", EntryKind::Account));
        assert_eq!(t.fields.get("balance").map(String::as_str), Some("balance"));
        assert!(!t.fields.contains_key("seen"));
        assert!(check(&dump, &core, &known, &m).ok);
    }

    #[test]
    fn a_stored_value_that_differs_from_core_is_a_mismatch() {
        let (good, core, known) = setup("500");
        let (m, _) = merge(&[infer(&good, &core, &known)]);
        let (bad, _, _) = setup("499");
        let r = check(&bad, &core, &known, &m);
        assert_eq!(r.mismatches.len(), 1, "{r:?}");
        assert_eq!(r.mismatches[0].core, "500");
    }

    #[test]
    fn merge_keeps_only_fields_that_hold_in_every_chain() {
        let (d1, core, known) = setup("500");
        let mut a = infer(&d1, &core, &known);
        let b = a.clone();
        // pretend the second chain's balance column matched nothing
        a.table[0].ambiguous.remove("balance");
        let (m, _) = merge(&[a, b]);
        assert!(m.table.is_empty() || !m.table[0].fields.contains_key("balance"));
    }

    #[test]
    fn core_state_of_a_recorded_chain_holds_its_accounts() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/chains/cap40.signed-payload-signer.p28/testnet");
        let chain = shapekit_pack::chain::ChainFile::read(&dir).unwrap();
        let s = CoreState::from_chain(&dir, &chain.passphrase, chain.ledger_range.last).unwrap();
        let accounts = s
            .entries
            .keys()
            .filter(|(k, _)| *k == EntryKind::Account)
            .count();
        assert!(accounts >= 2, "{accounts}");
    }
}
