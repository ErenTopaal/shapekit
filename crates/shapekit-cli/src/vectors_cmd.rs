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
    /// Measure the format vectors with schema mutants: copies of the schema with one change
    /// each (adjacent fields swapped, a bound or size moved by one, an arm or member dropped or
    /// added, a field made optional or required). A vector kills a mutant when the reference
    /// interpreter over the mutant gives it another verdict or another value. Exits 1 when a
    /// mutant survives.
    MutateCheck {
        /// Also print every mutant with the vector that killed it.
        #[arg(long)]
        verbose: bool,
    },
    /// Write the format vectors of every schema definition as xdr entries of the
    /// format.xdr-grammar family under <out>/entries/format.xdr-grammar/: for each decision a
    /// decoder makes, an input it must accept per allowed choice and one it must refuse per
    /// disallowed one, each confirmed by the reference interpreter. Run `shapekit manifest
    /// build --corpus <out>` afterwards.
    Gen {
        #[arg(long, default_value = "candidate")]
        out: PathBuf,
        /// The schema history, for the protocol each definition first appears in.
        #[arg(long, default_value = "schema/xdr")]
        history: PathBuf,
        /// Compare with the files already under <out> instead of writing; exits 1 on a
        /// difference.
        #[arg(long)]
        check: bool,
    },
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
        VectorsCommand::MutateCheck { verbose } => mutate_check(verbose),
        VectorsCommand::Gen {
            out,
            history,
            check,
        } => gen_format(&out, &history, check),
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

const FORMAT_FAMILY: &str = "format.xdr-grammar";

/// A lowercase id slug from free text: runs of anything but a-z and 0-9 become one dash.
fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

fn gen_format(out: &Path, history_dir: &Path, check: bool) -> Result<bool> {
    use shapekit_schema::Definition;
    use shapekit_schema::names::rust_type_name;
    use shapekit_schema::vectors::{Builder, Expect};

    let schema = shapekit_schema::Schema::pinned();
    let history = shapekit_schema::history::History::load(history_dir)
        .with_context(|| format!("loading {}", history_dir.display()))?;
    let first_seen = history.first_seen();
    let protocol_of = |label: &str| -> Result<u32> {
        history
            .get(label)
            .and_then(|t| t.protocol)
            .with_context(|| format!("tag {label} names no protocol"))
    };
    let builder = Builder::new(&schema);

    let mut files: BTreeMap<PathBuf, Vec<u8>> = BTreeMap::new();
    let mut ids = std::collections::BTreeSet::new();
    let mut refused_by_stellar_xdr = Vec::new();
    let (mut points, mut vectors) = (std::collections::BTreeSet::new(), 0usize);
    for d in schema.definitions() {
        if matches!(d, Definition::Const(_)) {
            continue;
        }
        let ty = rust_type_name(d.name());
        let seen = first_seen
            .get(&ty)
            .with_context(|| format!("{ty} has no first-seen tag"))?;
        let protocol = protocol_of(seen)?;
        for v in builder.vectors(d.name())? {
            let mut id = format!(
                "{FORMAT_FAMILY}.{}",
                slug(v.obligation.trim_start_matches("format:"))
            );
            let mut n = 2;
            while ids.contains(&id) {
                id = format!(
                    "{FORMAT_FAMILY}.{}-{n}",
                    slug(v.obligation.trim_start_matches("format:"))
                );
                n += 1;
            }
            ids.insert(id.clone());
            points.insert(format!("{}{}", v.type_name, v.point));
            vectors += 1;

            let dir = format!("entries/{FORMAT_FAMILY}/{id}");
            let xdr_file = format!("{dir}/any.xdr");
            let mut variant = json!({
                "xdr_file": xdr_file,
                "xdr_sha256": shapekit_manifest::sha256_hex(&v.bytes),
            });
            let expected = match v.expect {
                Expect::MustAccept => {
                    let limits = shapekit_manifest::decode_limits(&v.type_name, v.bytes.len());
                    match shapekit_manifest::xdr::to_xdr_json(&v.type_name, &v.bytes, limits) {
                        Ok(text) => {
                            let json_file = format!("{dir}/any.expected.json");
                            variant["expected_json_file"] = json!(json_file);
                            variant["expected_json_sha256"] =
                                json!(shapekit_manifest::sha256_hex(text.as_bytes()));
                            files.insert(out.join(&json_file), text.into_bytes());
                        }
                        Err(err) => {
                            // a value RFC 4506 allows that stellar-xdr does not read is a
                            // finding, not a vector to commit with a guessed expectation
                            refused_by_stellar_xdr.push(format!("{id}: {err}"));
                            ids.remove(&id);
                            vectors -= 1;
                            continue;
                        }
                    }
                    json!({"decode": "must-accept", "roundtrip": true, "validity": "valid"})
                }
                Expect::MustReject => {
                    json!({"decode": "must-reject", "roundtrip": false, "validity": "not-emittable"})
                }
                Expect::ImplementationDefined => json!({
                    "decode": "implementation-defined",
                    "roundtrip": false,
                    "validity": "not-emittable",
                }),
            };
            let bool_word = v.expect == Expect::ImplementationDefined;
            let notes = if bool_word {
                format!(
                    "{}. Decoders differ on this word: stellar-xdr reads it as false, go-stellar-sdk v0.9.0 and @stellar/stellar-sdk 17.4.0 refuse it, and stellar-core's xdrpp reads it as true and re-encodes it as 1",
                    v.note
                )
            } else {
                format!("{} in the smallest value of {}", v.note, v.type_name)
            };
            let linked: Vec<&str> = if bool_word {
                vec![
                    "https://github.com/stellar/rs-stellar-xdr/issues/629",
                    "https://github.com/stellar/stellar-core/issues/5509",
                ]
            } else {
                Vec::new()
            };
            let entry = json!({
                "id": id,
                "kind": "xdr",
                "family": FORMAT_FAMILY,
                "caps": [],
                "protocol": {"introduced": protocol, "valid_min": protocol},
                "xdr_type": v.type_name,
                "source": {"class": "synthetic"},
                "synthetic": true,
                "passphrase_dependent": false,
                "variants": {"any": variant},
                "expected": expected,
                "flags": ["format-vector"],
                "fields_exercised": [],
                "linked_issues": linked,
                "notes": notes,
                "obligations": [v.obligation],
            });
            files.insert(
                out.join(format!("{dir}/entry.json")),
                render_json(&entry).into_bytes(),
            );
            files.insert(out.join(&xdr_file), v.bytes.clone());
        }
    }

    let root = out.join("entries").join(FORMAT_FAMILY);
    let mut same = true;
    if check {
        for (path, bytes) in &files {
            if std::fs::read(path).ok().as_deref() != Some(bytes.as_slice()) {
                println!("differs: {}", path.display());
                same = false;
            }
        }
        let on_disk = walk(&root)?;
        for p in on_disk.difference(&files.keys().cloned().collect()) {
            println!("not generated: {}", p.display());
            same = false;
        }
    } else {
        if root.exists() {
            std::fs::remove_dir_all(&root)
                .with_context(|| format!("clearing {}", root.display()))?;
        }
        for (path, bytes) in &files {
            std::fs::create_dir_all(path.parent().expect("an entry file has a directory"))?;
            std::fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))?;
        }
    }
    println!(
        "{} decision points, {vectors} vectors in {} entries {}",
        points.len(),
        ids.len(),
        if check { "checked" } else { "written" }
    );
    for r in &refused_by_stellar_xdr {
        println!("stellar-xdr refuses a value the interpreter accepts: {r}");
    }
    Ok(same && refused_by_stellar_xdr.is_empty())
}

fn walk(dir: &Path) -> Result<std::collections::BTreeSet<PathBuf>> {
    let mut out = std::collections::BTreeSet::new();
    if !dir.exists() {
        return Ok(out);
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d)? {
            let p = e?.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.insert(p);
            }
        }
    }
    Ok(out)
}

fn mutate_check(verbose: bool) -> Result<bool> {
    use shapekit_schema::Definition;
    use shapekit_schema::vectors::Builder;

    let schema = shapekit_schema::Schema::pinned();
    let builder = Builder::new(&schema);
    let names: Vec<&str> = schema
        .definitions()
        .iter()
        .filter(|d| !matches!(d, Definition::Const(_)))
        .map(|d| d.name())
        .collect();
    let mut vectors = BTreeMap::new();
    for n in &names {
        vectors.insert(n.to_string(), builder.vectors(n)?);
    }
    let mutants = shapekit_schema::mutate::mutants(&schema, &names);
    let outcomes = shapekit_schema::mutate::check(&schema, &mutants, &vectors);
    let mut by_op: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for o in &outcomes {
        let e = by_op.entry(o.operator).or_default();
        e.0 += 1;
        if o.killed_by.is_some() {
            e.1 += 1;
        }
    }
    let killed = outcomes.iter().filter(|o| o.killed_by.is_some()).count();
    println!(
        "{} mutants, {killed} killed, {} survive",
        outcomes.len(),
        outcomes.len() - killed
    );
    for (op, (n, k)) in &by_op {
        println!("  {op:<14} {n} mutants, {k} killed");
    }
    for o in &outcomes {
        match &o.killed_by {
            None => println!("survives: {}", o.description),
            Some(v) if verbose => println!("killed: {} by {v}", o.description),
            Some(_) => {}
        }
    }
    Ok(killed == outcomes.len())
}
