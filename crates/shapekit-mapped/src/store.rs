//! Reads back what the target stored, through the queries of the mapping file.

use std::collections::{BTreeMap, HashMap};

use anyhow::{Context as _, Result, bail};
use postgres::fallible_iterator::FallibleIterator;
use postgres::types::{ToSql, Type};
use postgres::{Client, NoTls, Statement};
use shapekit_harness::protocol::{
    DumpColumn, DumpResult, DumpRow, DumpTable, ExportParticipant, ExportRecord, RawAddress,
    TxRecord, Unmapped,
};
use shapekit_oracle::network::Network;
use stellar_xdr::{
    AccountId, AlphaNum4, AlphaNum12, Asset, AssetCode4, AssetCode12, PublicKey, Uint256,
};

use crate::mapping::{Export, LookupKind, Mapping, OperationEncoding};
use crate::rules::{Lookups, Rules};
use crate::strkey;
use crate::value::{Row, Val};

/// Two connections: one streams the transactions of an export, the other reads each one's
/// rows while it does.
pub struct Store {
    main: Client,
    aux: Client,
}

/// How a column of a prepared statement is decoded.
#[derive(Debug, Clone, Copy)]
enum Decode {
    Bool,
    Int2,
    Int4,
    Int8,
    Oid,
    Float4,
    Float8,
    Text,
    Bytes,
    Json,
    /// Selected as `::text` by the wrapping query.
    AsText,
}

fn decode_of(t: &Type) -> Option<Decode> {
    Some(match *t {
        Type::BOOL => Decode::Bool,
        Type::INT2 => Decode::Int2,
        Type::INT4 => Decode::Int4,
        Type::INT8 => Decode::Int8,
        Type::OID => Decode::Oid,
        Type::FLOAT4 => Decode::Float4,
        Type::FLOAT8 => Decode::Float8,
        Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME | Type::UNKNOWN => Decode::Text,
        Type::BYTEA => Decode::Bytes,
        Type::JSON | Type::JSONB => Decode::Json,
        _ => return None,
    })
}

/// A query ready to run: the statement, its column names and decoders.
struct Prepared {
    stmt: Statement,
    names: Vec<String>,
    decode: Vec<Decode>,
}

/// A parameter value, converted to the type the statement wants.
#[derive(Debug, Clone)]
pub enum Param {
    Int(i64),
    Text(String),
    Bytes(Vec<u8>),
}

impl Param {
    fn of(v: &Val) -> Result<Param> {
        Ok(match v {
            Val::Int { v, .. } => Param::Int(*v),
            Val::Text(s) | Val::Other(s) => Param::Text(s.clone()),
            Val::Bytes(b) => Param::Bytes(b.clone()),
            other => bail!("{other:?} cannot be a query parameter"),
        })
    }

    fn for_type(&self, t: &Type) -> Result<Box<dyn ToSql + Sync>> {
        Ok(match (self, t) {
            (Param::Int(v), &Type::INT2) => Box::new(i16::try_from(*v)?),
            (Param::Int(v), &Type::INT4) => Box::new(i32::try_from(*v)?),
            (Param::Int(v), &Type::INT8) => Box::new(*v),
            (Param::Int(v), &Type::OID) => Box::new(u32::try_from(*v)?),
            (Param::Int(v), &Type::TEXT | &Type::VARCHAR) => Box::new(v.to_string()),
            (Param::Text(s), &Type::TEXT | &Type::VARCHAR | &Type::BPCHAR | &Type::NAME) => {
                Box::new(s.clone())
            }
            (Param::Text(s), &Type::INT8) => Box::new(s.parse::<i64>()?),
            (Param::Text(s), &Type::INT4) => Box::new(s.parse::<i32>()?),
            (Param::Bytes(b), &Type::BYTEA) => Box::new(b.clone()),
            (p, t) => bail!("a {p:?} parameter cannot be passed as {t}"),
        })
    }
}

fn quote_ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn prepare(c: &mut Client, sql: &str) -> Result<Prepared> {
    let stmt = c
        .prepare(sql)
        .with_context(|| format!("preparing {sql:?}"))?;
    let names: Vec<String> = stmt.columns().iter().map(|c| c.name().to_owned()).collect();
    let decode: Vec<Option<Decode>> = stmt
        .columns()
        .iter()
        .map(|c| decode_of(c.type_()))
        .collect();
    if decode.iter().all(Option::is_some) {
        return Ok(Prepared {
            stmt,
            names,
            decode: decode.into_iter().map(Option::unwrap).collect(),
        });
    }
    // columns of other types (timestamps, numerics, uuids) are read in postgres's text form
    let mut seen = std::collections::BTreeSet::new();
    for n in &names {
        if !seen.insert(n) {
            bail!("{sql:?} returns two columns named {n}; name them apart");
        }
    }
    let exprs: Vec<String> = names
        .iter()
        .zip(&decode)
        .map(|(n, d)| match d {
            Some(_) => format!("q.{}", quote_ident(n)),
            None => format!("q.{}::text AS {}", quote_ident(n), quote_ident(n)),
        })
        .collect();
    let wrapped = format!("SELECT {} FROM ({sql}) AS q", exprs.join(", "));
    let stmt = c
        .prepare(&wrapped)
        .with_context(|| format!("preparing {wrapped:?}"))?;
    Ok(Prepared {
        stmt,
        names,
        decode: decode
            .into_iter()
            .map(|d| d.unwrap_or(Decode::AsText))
            .collect(),
    })
}

fn params(p: &Prepared, values: &[Param]) -> Result<Vec<Box<dyn ToSql + Sync>>> {
    let types = p.stmt.params();
    if types.len() != values.len() {
        bail!(
            "the query takes {} parameters, {} given",
            types.len(),
            values.len()
        );
    }
    values
        .iter()
        .zip(types)
        .map(|(v, t)| v.for_type(t))
        .collect()
}

fn read_row(p: &Prepared, r: &postgres::Row) -> Result<Row> {
    let mut out = BTreeMap::new();
    for (i, (name, d)) in p.names.iter().zip(&p.decode).enumerate() {
        let v = match d {
            Decode::Bool => r.try_get::<_, Option<bool>>(i)?.map(Val::Bool),
            Decode::Int2 => r.try_get::<_, Option<i16>>(i)?.map(|v| Val::Int {
                v: v.into(),
                wide: false,
            }),
            Decode::Int4 => r.try_get::<_, Option<i32>>(i)?.map(|v| Val::Int {
                v: v.into(),
                wide: false,
            }),
            Decode::Int8 => r
                .try_get::<_, Option<i64>>(i)?
                .map(|v| Val::Int { v, wide: true }),
            Decode::Oid => r.try_get::<_, Option<u32>>(i)?.map(|v| Val::Int {
                v: v.into(),
                wide: false,
            }),
            Decode::Float4 => r
                .try_get::<_, Option<f32>>(i)?
                .map(|v| Val::Float(v.into())),
            Decode::Float8 => r.try_get::<_, Option<f64>>(i)?.map(Val::Float),
            Decode::Text => r.try_get::<_, Option<String>>(i)?.map(Val::Text),
            Decode::Bytes => r.try_get::<_, Option<Vec<u8>>>(i)?.map(Val::Bytes),
            Decode::Json => r.try_get::<_, Option<serde_json::Value>>(i)?.map(Val::Json),
            Decode::AsText => r.try_get::<_, Option<String>>(i)?.map(Val::Other),
        };
        out.insert(name.clone(), v.unwrap_or(Val::Null));
    }
    Ok(Row(out))
}

fn query(c: &mut Client, sql: &str, values: &[Param]) -> Result<Vec<Row>> {
    let p = prepare(c, sql)?;
    let ps = params(&p, values)?;
    let refs: Vec<&(dyn ToSql + Sync)> = ps.iter().map(|b| b.as_ref()).collect();
    let rows = c
        .query(&p.stmt, &refs)
        .with_context(|| format!("running {sql:?}"))?;
    rows.iter().map(|r| read_row(&p, r)).collect()
}

/// The asset a lookup row names, and its asset contract id.
fn credit_asset(code: &str, issuer: &str) -> Result<Asset> {
    if code.is_empty() || code.len() > 12 || !code.bytes().all(|b| b.is_ascii_alphanumeric()) {
        bail!("asset code {code:?} is not 1 to 12 letters and digits");
    }
    let key = stellar_strkey::ed25519::PublicKey::from_string(issuer)
        .map_err(|e| anyhow::anyhow!("issuer {issuer:?}: {e}"))?;
    let issuer = AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(key.0)));
    let mut padded = [0u8; 12];
    padded[..code.len()].copy_from_slice(code.as_bytes());
    Ok(if code.len() <= 4 {
        let mut c = [0u8; 4];
        c.copy_from_slice(&padded[..4]);
        Asset::CreditAlphanum4(AlphaNum4 {
            asset_code: AssetCode4(c),
            issuer,
        })
    } else {
        Asset::CreditAlphanum12(AlphaNum12 {
            asset_code: AssetCode12(padded),
            issuer,
        })
    })
}

impl Store {
    pub fn connect(url: &str) -> Result<Store> {
        let main = Client::connect(url, NoTls).with_context(|| format!("connecting to {url}"))?;
        let aux = Client::connect(url, NoTls).with_context(|| format!("connecting to {url}"))?;
        Ok(Store { main, aux })
    }

    pub fn execute(&mut self, sql: &str) -> Result<()> {
        self.main
            .batch_execute(sql)
            .with_context(|| format!("running {sql:?}"))
    }

    /// The target's durable cursor: the first column of the first row, `None` for no row,
    /// NULL or 0.
    pub fn cursor(&mut self, sql: &str) -> Result<Option<u32>> {
        let rows = query(&mut self.main, sql, &[])?;
        let Some(r) = rows.first() else {
            return Ok(None);
        };
        let Some((name, v)) = r.0.iter().next() else {
            bail!("the cursor query returns no column");
        };
        let n: u64 = match v {
            Val::Null => return Ok(None),
            Val::Int { v, .. } => u64::try_from(*v).with_context(|| format!("{name} is {v}"))?,
            Val::Text(s) | Val::Other(s) => s
                .trim()
                .parse()
                .with_context(|| format!("{name} is {s:?}"))?,
            other => bail!("{name} is {other:?}, not a ledger number"),
        };
        if n == 0 {
            return Ok(None);
        }
        Ok(Some(
            u32::try_from(n).with_context(|| format!("{name} is {n}"))?,
        ))
    }

    pub fn lookups(&mut self, m: &Mapping, passphrase: &str) -> Result<Lookups> {
        let net = Network::new(passphrase);
        let mut out = Lookups::new();
        for l in &m.export.lookup {
            let mut ids = HashMap::new();
            match l.kind {
                LookupKind::AssetContract => {
                    if l.native {
                        ids.insert(net.asset_contract_id(&Asset::Native), "native".to_owned());
                    }
                    for r in query(&mut self.main, &l.query, &[])
                        .with_context(|| format!("lookup {}", l.name))?
                    {
                        let (Some(code), Some(issuer)) =
                            (r.get("code").text(), r.get("issuer").text())
                        else {
                            bail!(
                                "lookup {}: a row without text code and issuer: {:?}",
                                l.name,
                                r
                            );
                        };
                        let a = credit_asset(code, issuer)
                            .with_context(|| format!("lookup {}", l.name))?;
                        ids.insert(net.asset_contract_id(&a), format!("{code}:{issuer}"));
                    }
                }
            }
            out.insert(l.name.clone(), ids);
        }
        Ok(out)
    }

    /// Streams one transaction record per stored transaction in ledgers `from` to `to`, then
    /// one unattributed record per ledger holding rows of no stored transaction.
    pub fn export(
        &mut self,
        m: &Mapping,
        passphrase: &str,
        from: u32,
        to: u32,
        emit: &mut dyn FnMut(ExportRecord) -> Result<()>,
    ) -> Result<u64> {
        let rules = Rules::new(m, self.lookups(m, passphrase)?);
        let ex = &m.export;
        let allowed = m.forms(&ex.participant.forms)?;
        let p = prepare(&mut self.main, &ex.transactions)?;
        let ps = params(&p, &[Param::Int(from.into()), Param::Int(to.into())])?;
        let refs: Vec<&dyn ToSql> = ps.iter().map(|b| b.as_ref() as &dyn ToSql).collect();
        let mut it = self
            .main
            .query_raw(&p.stmt, refs)
            .context("reading transactions")?;
        let mut n = 0u64;
        while let Some(r) = it.next().context("reading transactions")? {
            let t = read_row(&p, &r)?;
            let rec = transaction(&mut self.aux, &rules, ex, &allowed, &t)?;
            emit(ExportRecord::Transaction(rec))?;
            n += 1;
        }
        drop(it);
        n += self.unattributed(&rules, m, from, to, emit)?;
        Ok(n)
    }
}

fn transaction(
    aux: &mut Client,
    rules: &Rules<'_>,
    ex: &Export,
    allowed: &[String],
    t: &Row,
) -> Result<TxRecord> {
    let hash = match t.get(&ex.transaction.hash) {
        Val::Bytes(b) => hex::encode(b),
        Val::Text(s) => s.to_lowercase(),
        other => bail!("transaction hash {other:?} is neither bytes nor text"),
    };
    let ledger = t
        .get(&ex.transaction.ledger)
        .int()
        .and_then(|v| u32::try_from(v).ok())
        .with_context(|| format!("transaction {hash} has no ledger number"))?;
    let id = Param::of(t.get(&ex.transaction.id))
        .with_context(|| format!("transaction {hash}: {}", ex.transaction.id))?;
    let mut participants = Vec::new();
    for r in query(aux, &ex.participants, std::slice::from_ref(&id))
        .with_context(|| format!("reading participants of {hash}"))?
    {
        // NULL decodes as no bytes, which reports why it is not an address
        let b = match r.get(&ex.participant.address) {
            Val::Bytes(b) => b.as_slice(),
            Val::Null => &[],
            other => bail!(
                "participant of {hash}: {} is {other:?}, not stored bytes",
                ex.participant.address
            ),
        };
        let mut raw = RawAddress {
            column: ex.participant.address.clone(),
            hex: hex::encode(b),
            strkey: None,
            error: None,
        };
        match strkey::decode_stored(b, allowed) {
            Ok(s) => participants.push(ExportParticipant {
                address: Some(s),
                mux_ids: vec![],
                raw: None,
            }),
            Err(e) => {
                raw.error = Some(e);
                participants.push(ExportParticipant {
                    address: None,
                    mux_ids: vec![],
                    raw: Some(raw),
                });
            }
        }
    }
    let rows = query(aux, &ex.rows, std::slice::from_ref(&id))
        .with_context(|| format!("reading the rows of {hash}"))?;
    let mapped = rules.map_transaction(rows);
    Ok(TxRecord {
        ledger,
        hash,
        participants,
        changes: mapped.changes,
        unmapped: mapped.unmapped,
    })
}

impl Store {
    fn unattributed(
        &mut self,
        rules: &Rules<'_>,
        m: &Mapping,
        from: u32,
        to: u32,
        emit: &mut dyn FnMut(ExportRecord) -> Result<()>,
    ) -> Result<u64> {
        let ex = &m.export;
        let mut lists: Vec<Vec<(u32, Unmapped)>> = vec![Vec::new(); ex.unattributed.len()];
        let mut lo = from;
        while lo <= to {
            let hi = to.min(lo.saturating_add(ex.unattributed_window - 1));
            for (si, src) in ex.unattributed.iter().enumerate() {
                let rows = query(
                    &mut self.main,
                    &src.query,
                    &[Param::Int(lo.into()), Param::Int(hi.into())],
                )
                .with_context(|| {
                    format!(
                        "reading unattributed {} rows of ledgers {lo} to {hi}",
                        src.table
                    )
                })?;
                for r in rows {
                    let v = r.get(&src.ledger.column).int().with_context(|| {
                        format!(
                            "an unattributed {} row has no {}",
                            src.table, src.ledger.column
                        )
                    })?;
                    let l = match src.ledger.encoding {
                        Some(OperationEncoding::Toid) => v >> 32,
                        _ => v,
                    };
                    let mut u = rules.unmapped(&r, src.note.clone());
                    u.table = src.table.clone();
                    u.category = label(&r, &src.category);
                    u.reason = label(&r, &src.reason);
                    lists[si].push((u32::try_from(l).context("ledger out of range")?, u));
                }
            }
            if hi == to {
                break;
            }
            lo = hi + 1;
        }
        let mut by_ledger: BTreeMap<u32, Vec<Unmapped>> = BTreeMap::new();
        for list in lists {
            for (l, u) in list {
                by_ledger.entry(l).or_default().push(u);
            }
        }
        let mut n = 0;
        for (l, unmapped) in by_ledger {
            emit(ExportRecord::Unattributed {
                ledger: Some(l),
                unmapped,
            })?;
            n += 1;
        }
        Ok(n)
    }

    /// Streams every base table of the public schema (or the named ones), each header then
    /// its rows ordered by every column, as cell text.
    pub fn dump(
        &mut self,
        only: &[String],
        table: &mut dyn FnMut(DumpTable) -> Result<()>,
        row: &mut dyn FnMut(DumpRow) -> Result<()>,
    ) -> Result<DumpResult, DumpError> {
        let mut res = DumpResult { tables: 0, rows: 0 };
        let names: Vec<String> = self
            .main
            .query(
                "SELECT table_name::text FROM information_schema.tables \
                 WHERE table_schema = 'public' AND table_type = 'BASE TABLE' ORDER BY table_name",
                &[],
            )
            .map_err(|e| DumpError::Store(anyhow::Error::new(e).context("listing tables")))?
            .iter()
            .map(|r| r.get(0))
            .collect();
        let names = if only.is_empty() {
            names
        } else {
            for n in only {
                if !names.contains(n) {
                    return Err(DumpError::UnknownTable(n.clone()));
                }
            }
            only.to_vec()
        };
        for name in names {
            let cols = self
                .main
                .query(
                    "SELECT a.attname::text, format_type(a.atttypid, a.atttypmod), a.atttypid = 'bytea'::regtype \
                     FROM pg_attribute a WHERE a.attrelid = to_regclass(quote_ident('public') || '.' || quote_ident($1)) \
                     AND a.attnum > 0 AND NOT a.attisdropped ORDER BY a.attnum",
                    &[&name],
                )
                .map_err(|e| {
                    DumpError::Store(anyhow::Error::new(e).context(format!("reading the columns of {name}")))
                })?;
            let columns: Vec<DumpColumn> = cols
                .iter()
                .map(|r| DumpColumn {
                    name: r.get(0),
                    ty: r.get(1),
                })
                .collect();
            let bytea: Vec<bool> = cols.iter().map(|r| r.get(2)).collect();
            table(DumpTable {
                name: name.clone(),
                columns: columns.clone(),
            })
            .map_err(DumpError::Store)?;
            res.tables += 1;
            let exprs: Vec<String> = columns
                .iter()
                .zip(&bytea)
                .map(|(c, b)| {
                    if *b {
                        format!("encode({}, 'hex')", quote_ident(&c.name))
                    } else {
                        format!("{}::text", quote_ident(&c.name))
                    }
                })
                .collect();
            let mut sql = format!(
                "SELECT {} FROM {}.{}",
                exprs.join(", "),
                quote_ident("public"),
                quote_ident(&name)
            );
            if !columns.is_empty() {
                let order: Vec<String> = (1..=columns.len()).map(|i| i.to_string()).collect();
                sql += &format!(" ORDER BY {}", order.join(", "));
            }
            let mut it = self
                .main
                .query_raw(&sql, std::iter::empty::<&dyn ToSql>())
                .map_err(|e| {
                    DumpError::Store(anyhow::Error::new(e).context(format!("reading {name}")))
                })?;
            while let Some(r) = it.next().map_err(|e| {
                DumpError::Store(anyhow::Error::new(e).context(format!("reading {name}")))
            })? {
                let cells: Vec<Option<String>> = (0..columns.len()).map(|i| r.get(i)).collect();
                row(DumpRow {
                    table: name.clone(),
                    cells,
                })
                .map_err(DumpError::Store)?;
                res.rows += 1;
            }
        }
        Ok(res)
    }
}

/// A dump failure: a table the request names that the store lacks is the harness's error,
/// anything else the target's.
#[derive(Debug)]
pub enum DumpError {
    UnknownTable(String),
    Store(anyhow::Error),
}

fn label(r: &Row, col: &Option<String>) -> Option<String> {
    col.as_ref()
        .and_then(|c| r.get(c).text().map(str::to_owned))
        .filter(|s| !s.is_empty())
}
