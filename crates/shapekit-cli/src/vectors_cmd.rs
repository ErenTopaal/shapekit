//! `shapekit vectors`: conformance vectors derived from the schema and the representation gap
//! tables, written to the candidate lane.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use base64::Engine;
use serde_json::{Value, json};
use shapekit_manifest::xdr::render_json;
use shapekit_schema::gaps::{GapVector, IdExpect, IdInput, IdKind, MapValue, Table};
use stellar_xdr::{Limits, ScMap, ScMapEntry, ScString, ScSymbol, ScVal, WriteXdr};

#[derive(clap::Subcommand)]
pub enum VectorsCommand {
    /// Turn the vectors of registry/gaps/<language>.toml into conformance entries: one entry
    /// per gap row and id kind, under <out>/vectors/. Run `shapekit manifest build --corpus
    /// <out>` afterwards.
    Gaps {
        #[arg(long, default_value = "registry")]
        registry: PathBuf,
        #[arg(long, default_value = "candidate")]
        out: PathBuf,
        /// Compare with the files already under <out> instead of writing; exits 1 on a
        /// difference.
        #[arg(long)]
        check: bool,
    },
}

pub fn run(command: VectorsCommand) -> Result<bool> {
    match command {
        VectorsCommand::Gaps {
            registry,
            out,
            check,
        } => gaps(&registry, &out, check),
    }
}

/// The family an entry of each vector kind belongs to, with the fields its existing entries
/// of the same kind exercise.
fn family_of(kind: &Kind) -> (&'static str, &'static [&'static str]) {
    match kind {
        Kind::Id(IdKind::Muxed) => (
            "cap27.muxed-accounts",
            &["op.payment.destination.mux_id", "tx.source_account.mux_id"],
        ),
        Kind::Id(IdKind::Offer) => ("cap03.offer-liabilities", &["op.manage_offer.offer_id"]),
        Kind::Native => (
            "soroban.scval-shapes",
            &["op.invoke_host_function.invoke_contract.args"],
        ),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Id(IdKind),
    Native,
}

fn scval(v: &MapValue) -> Result<ScVal> {
    Ok(match v {
        MapValue::U32(n) => ScVal::U32(*n),
        MapValue::Symbol(s) => ScVal::Symbol(ScSymbol(s.as_bytes().to_vec().try_into()?)),
        MapValue::StringHex(h) => ScVal::String(ScString(
            hex::decode(h)
                .with_context(|| format!("{h} is not hex"))?
                .try_into()?,
        )),
    })
}

/// The order soroban-env-host keeps map keys in, for the key kinds the tables use: by ScVal
/// type first, then u32 by value and strings and symbols by their bytes (host/comparison.rs,
/// `Compare<ScVal> for Budget`, in soroban-env-host 30.0.0).
fn host_order(a: &ScVal, b: &ScVal) -> Result<std::cmp::Ordering> {
    let rank = |v: &ScVal| v.discriminant() as i32;
    Ok(match (a, b) {
        (ScVal::U32(x), ScVal::U32(y)) => x.cmp(y),
        (ScVal::String(x), ScVal::String(y)) => x.0.as_slice().cmp(y.0.as_slice()),
        (ScVal::Symbol(x), ScVal::Symbol(y)) => x.0.as_slice().cmp(y.0.as_slice()),
        _ if rank(a) != rank(b) => rank(a).cmp(&rank(b)),
        _ => bail!("no host order between {a:?} and {b:?} here"),
    })
}

fn gaps(registry: &Path, out: &Path, check: bool) -> Result<bool> {
    let families_path = registry.join("families.toml");
    let families: toml::Table = std::fs::read_to_string(&families_path)
        .with_context(|| format!("reading {}", families_path.display()))?
        .parse()
        .with_context(|| format!("parsing {}", families_path.display()))?;
    let family = |id: &str| -> Result<(Vec<String>, i64)> {
        let f = families["family"]
            .as_array()
            .and_then(|a| a.iter().find(|f| f["id"].as_str() == Some(id)))
            .with_context(|| format!("families.toml has no family {id}"))?;
        let caps = f["caps"]
            .as_array()
            .context("a family without caps")?
            .iter()
            .filter_map(|c| c.as_str().map(str::to_string))
            .collect();
        let introduced = f["protocol_introduced"]
            .as_integer()
            .context("a family without protocol_introduced")?;
        Ok((caps, introduced))
    };

    let dir = registry.join("gaps");
    let mut tables = Vec::new();
    for e in std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
        let p = e?.path();
        if p.extension().is_some_and(|x| x == "toml") {
            let t = Table::load(&p)?;
            if p.file_stem().and_then(|s| s.to_str()) != Some(t.language.as_str()) {
                bail!("{} says language {}", p.display(), t.language);
            }
            tables.push(t);
        }
    }
    tables.sort_by(|a, b| a.language.cmp(&b.language));

    let mut files: BTreeMap<PathBuf, String> = BTreeMap::new();
    for t in &tables {
        for g in &t.gaps {
            let mut by_kind: BTreeMap<Kind, Vec<Value>> = BTreeMap::new();
            let mut any_either: BTreeMap<Kind, bool> = BTreeMap::new();
            for v in &g.vectors {
                match v {
                    GapVector::IdParse {
                        id_kind,
                        input,
                        expect,
                        id,
                        reason,
                    } => {
                        let input = match input {
                            IdInput::Text(s) => json!({"text": s}),
                            IdInput::Number(n) => json!({"number": n}),
                        };
                        let expected = match expect {
                            IdExpect::Reject => json!({"reject": reason}),
                            IdExpect::IdOrReject => {
                                *any_either.entry(Kind::Id(*id_kind)).or_default() = true;
                                json!({"either": [{"id": id}, {"reject": reason}]})
                            }
                        };
                        by_kind.entry(Kind::Id(*id_kind)).or_default().push(json!({
                            "op": "id_parse",
                            "kind": id_kind.as_str(),
                            "input": input,
                            "expected": expected,
                        }));
                    }
                    GapVector::ToNative { map, collides_as } => {
                        let entries = map
                            .iter()
                            .map(|e| {
                                Ok(ScMapEntry {
                                    key: scval(&e.key)?,
                                    val: scval(&e.val)?,
                                })
                            })
                            .collect::<Result<Vec<_>>>()?;
                        for w in entries.windows(2) {
                            if host_order(&w[0].key, &w[1].key)? != std::cmp::Ordering::Less {
                                bail!(
                                    "{} gap {}: map keys are not in the host's strictly ascending order",
                                    t.language,
                                    g.id
                                );
                            }
                        }
                        let n = entries.len();
                        let value = ScVal::Map(Some(ScMap(entries.try_into()?)));
                        let xdr = base64::engine::general_purpose::STANDARD
                            .encode(value.to_xdr(Limits::none())?);
                        by_kind.entry(Kind::Native).or_default().push(json!({
                            "op": "to_native",
                            "value": {"json": serde_json::to_value(&value)?, "xdr": xdr},
                            "expected": {
                                "rule": "no-entry-lost",
                                "entry_count": n,
                                "collides_as": collides_as,
                            },
                        }));
                    }
                }
            }
            for (kind, cases) in by_kind {
                let (fam, fields) = family_of(&kind);
                let (caps, introduced) = family(fam)?;
                let (vector_kind, expected) = match kind {
                    Kind::Native => (
                        "native-projection",
                        json!({"decode": "must-accept", "roundtrip": true, "validity": "valid"}),
                    ),
                    Kind::Id(_) if any_either.get(&kind).copied().unwrap_or(false) => (
                        "textual-id",
                        json!({"decode": "implementation-defined", "roundtrip": false, "validity": "valid"}),
                    ),
                    Kind::Id(_) => (
                        "textual-id",
                        json!({"decode": "must-reject", "roundtrip": false, "validity": "not-emittable"}),
                    ),
                };
                let suffix = match kind {
                    Kind::Id(k) => format!("-{}", k.as_str()),
                    Kind::Native => String::new(),
                };
                let id = format!("{fam}.gap-{}-{}{suffix}", t.language, g.id);
                let issues: Vec<&String> = g
                    .sources
                    .iter()
                    .filter(|s| s.starts_with("https://github.com/") && s.contains("/issues/"))
                    .collect();
                let notes = format!(
                    "representation gap {} of the {} table (registry/gaps/{}.toml): {} xdr {} as {}. {}. {}",
                    g.id,
                    t.language,
                    t.language,
                    t.sdk,
                    g.xdr.join(", "),
                    g.native,
                    g.loss,
                    g.sdk_note
                );
                let entry = json!({
                    "id": id,
                    "kind": vector_kind,
                    "family": fam,
                    "caps": caps,
                    "protocol": {"introduced": introduced, "valid_min": introduced},
                    "source": {"class": "synthetic"},
                    "synthetic": true,
                    "passphrase_dependent": false,
                    "expected": expected,
                    "flags": ["representation-gap"],
                    "fields_exercised": fields,
                    "linked_issues": issues,
                    "notes": notes,
                    "obligations": [t.obligation(g)],
                });
                let path = out
                    .join("vectors")
                    .join(vector_kind)
                    .join(format!("{id}.json"));
                files.insert(path, render_json(&json!({"entry": entry, "cases": cases})));
            }
        }
    }

    let mut same = true;
    for (path, text) in &files {
        if check {
            let on_disk = std::fs::read_to_string(path).unwrap_or_default();
            if on_disk != *text {
                println!("differs: {}", path.display());
                same = false;
            }
        } else {
            std::fs::create_dir_all(path.parent().expect("a vectors path has a parent"))?;
            std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))?;
        }
    }
    let rows: usize = tables.iter().map(|t| t.gaps.len()).sum();
    let without: usize = tables
        .iter()
        .flat_map(|t| &t.gaps)
        .filter(|g| g.vectors.is_empty())
        .count();
    println!(
        "{} tables, {rows} gap rows ({without} without a vector), {} entries {}",
        tables.len(),
        files.len(),
        if check { "checked" } else { "written" }
    );
    Ok(same)
}
