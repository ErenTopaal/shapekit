//! The reference interpreter against stellar-xdr on every corpus file a decoder must accept:
//! both decode it, both give the same value tree, and both write the same bytes back.

use std::path::PathBuf;
use std::str::FromStr;

use serde_json::Value as Json;
use shapekit_schema::interp::{Interp, Value};
use shapekit_schema::names::{case_name, field_name, member_name, rust_type_name};
use shapekit_schema::{Definition, Schema, Type};
use stellar_xdr::{Limits, TypeVariant, WriteXdr};

fn corpus() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
}

struct Cmp<'a> {
    schema: &'a Schema,
    interp: Interp<'a>,
}

impl Cmp<'_> {
    /// Compares one interpreter value of `ty` with stellar-xdr's XDR-JSON of the same value.
    fn same(&self, ty: &Type, v: &Value, j: &Json, path: &str) -> Result<(), String> {
        let bad = |what: &str| Err(format!("{path}: {what}: ours {v:?}, stellar-xdr {j}"));
        match (ty, v) {
            (Type::Ref { name }, _) => self.same_definition(name, v, j, path),
            (Type::Int, Value::Int(i)) => num(j, &i.to_string())
                .then_some(())
                .ok_or(())
                .or_else(|_| bad("int")),
            (Type::UnsignedInt, Value::UnsignedInt(i)) => num(j, &i.to_string())
                .then_some(())
                .ok_or(())
                .or_else(|_| bad("uint")),
            (Type::Hyper, Value::Hyper(i)) => num(j, &i.to_string())
                .then_some(())
                .ok_or(())
                .or_else(|_| bad("hyper")),
            (Type::UnsignedHyper, Value::UnsignedHyper(i)) => num(j, &i.to_string())
                .then_some(())
                .ok_or(())
                .or_else(|_| bad("uhyper")),
            (Type::Bool, Value::Bool(b)) => (j.as_bool() == Some(*b))
                .then_some(())
                .ok_or(())
                .or_else(|_| bad("bool")),
            (Type::OpaqueFixed { .. } | Type::OpaqueVar { .. }, Value::Opaque(b)) => {
                // hex, or for a few fixed arrays (Curve25519 keys, HMAC) a list of numbers
                let listed = j.as_array().map(|a| {
                    a.iter()
                        .map(|n| n.as_u64().and_then(|n| u8::try_from(n).ok()))
                        .collect::<Option<Vec<u8>>>()
                });
                (j.as_str().and_then(|s| hex::decode(s).ok()).as_deref() == Some(b.as_slice())
                    || listed.flatten().as_deref() == Some(b.as_slice()))
                .then_some(())
                .ok_or(())
                .or_else(|_| bad("opaque"))
            }
            (Type::String { .. }, Value::String(b)) => {
                if j.as_str().and_then(unescape).as_deref() == Some(b.as_slice()) {
                    Ok(())
                } else {
                    bad("string")
                }
            }
            (Type::Optional { element }, Value::Optional(o)) => match o {
                None => j
                    .is_null()
                    .then_some(())
                    .ok_or(())
                    .or_else(|_| bad("absent optional")),
                Some(inner) => self.same(element, inner, j, path),
            },
            (Type::Array { element, .. } | Type::VarArray { element, .. }, Value::Array(items)) => {
                let Some(js) = j.as_array() else {
                    return bad("array");
                };
                if js.len() != items.len() {
                    return bad("array length");
                }
                for (i, (v, j)) in items.iter().zip(js).enumerate() {
                    self.same(element, v, j, &format!("{path}[{i}]"))?;
                }
                Ok(())
            }
            _ => bad("type and value disagree"),
        }
    }

    fn same_definition(&self, name: &str, v: &Value, j: &Json, path: &str) -> Result<(), String> {
        let def = self
            .schema
            .get(name)
            .ok_or(format!("{path}: {name} undefined"))?;
        // types XDR-JSON writes as one string (strkeys, 128-bit numbers, asset codes): compare
        // the bytes stellar-xdr reads that string back to with the interpreter's bytes
        if let Json::String(_) = j
            && matches!(def, Definition::Struct(_) | Definition::Union(_))
        {
            return self.same_bytes(name, v, j, path);
        }
        match (def, v) {
            (Definition::Typedef(t), _) => match (j, self.same(&t.ty, v, j, path)) {
                // a typedef written as a strkey (ContractId, PoolId) or as an escaped string
                (Json::String(_), Err(_)) => self.same_bytes(name, v, j, path),
                (_, r) => r,
            },
            (Definition::Struct(s), Value::Struct(fields)) => {
                let Some(obj) = j.as_object() else {
                    return Err(format!("{path}: struct {name} is not an object: {j}"));
                };
                if obj.len() != s.fields.len() {
                    return Err(format!("{path}: {name} has {} keys in json", obj.len()));
                }
                for (f, v) in s.fields.iter().zip(fields) {
                    let key = field_name(&f.name);
                    let jv = obj
                        .get(&key)
                        .ok_or(format!("{path}: {name} json lacks {key}"))?;
                    self.same(&f.ty, v, jv, &format!("{path}.{key}"))?;
                }
                Ok(())
            }
            (Definition::Enum(e), Value::Enum(w)) => {
                let m = e
                    .members
                    .iter()
                    .find(|m| m.value == i64::from(*w))
                    .ok_or(format!("{path}: no member {w}"))?;
                let want = member_name(&m.name, &e.member_prefix);
                (j.as_str() == Some(want.as_str()))
                    .then_some(())
                    .ok_or(format!("{path}: enum {want} vs {j}"))
            }
            (Definition::Union(u), Value::Union { discriminant, arm }) => {
                let a = u.arm(*discriminant).ok_or(format!("{path}: no arm"))?;
                let case = a
                    .cases
                    .iter()
                    .find(|c| c.value == *discriminant)
                    .expect("the arm holds the case");
                let arm_name = case_name(self.schema, u, case);
                match (&a.ty, arm) {
                    (None, None) => (j.as_str() == Some(arm_name.as_str()))
                        .then_some(())
                        .ok_or(format!("{path}: void arm {arm_name} vs {j}")),
                    (Some(t), Some(v)) => {
                        let inner = j
                            .as_object()
                            .filter(|o| o.len() == 1)
                            .and_then(|o| o.get(&arm_name))
                            .ok_or(format!("{path}: arm {arm_name} vs {j}"))?;
                        self.same(t, v, inner, &format!("{path}:{arm_name}"))
                    }
                    _ => Err(format!("{path}: arm shape")),
                }
            }
            _ => Err(format!("{path}: {name} and value disagree")),
        }
    }

    fn same_bytes(&self, name: &str, v: &Value, j: &Json, path: &str) -> Result<(), String> {
        // stellar-xdr writes a signed-payload signer with an empty payload, which has no strkey,
        // as <INVALID:G...:EMPTY_PAYLOAD>; that marker cannot be read back, so check its parts
        if let Some(marker) = j.as_str().and_then(|s| s.strip_prefix("<INVALID:")) {
            let account = marker
                .strip_suffix(":EMPTY_PAYLOAD>")
                .ok_or(format!("{path}: unknown marker {j}"))?;
            let key = stellar_strkey::ed25519::PublicKey::from_string(account)
                .map_err(|e| format!("{path}: {account}: {e}"))?;
            // the marker stands for the signer key union or for its payload struct alone
            let inner = match v {
                Value::Union {
                    arm: Some(inner), ..
                } => inner.as_ref(),
                Value::Struct(_) => v,
                _ => return Err(format!("{path}: {name} is not a signed-payload signer")),
            };
            return match inner {
                Value::Struct(f)
                    if f.len() == 2
                        && f[0] == Value::Opaque(key.0.to_vec())
                        && f[1] == Value::Opaque(Vec::new()) =>
                {
                    Ok(())
                }
                other => Err(format!(
                    "{path}: {other:?} is not the empty payload of {account}"
                )),
            };
        }
        let ours = self
            .interp
            .encode(name, v)
            .map_err(|e| format!("{path}: {e}"))?;
        let variant = TypeVariant::from_str(&rust_type_name(name))
            .map_err(|e| format!("{path}: no type variant for {name}: {e}"))?;
        let theirs = stellar_xdr::Type::from_json(variant, j.to_string().as_bytes())
            .and_then(|t| t.to_xdr(Limits::none()))
            .map_err(|e| format!("{path}: stellar-xdr reads {j} as {name}: {e}"))?;
        (ours == theirs)
            .then_some(())
            .ok_or(format!("{path}: {name} {j} encodes differently"))
    }
}

/// The bytes of a SEP-51 escaped string: \0 \t \n \r \\ and \xNN escapes, every other
/// character as its UTF-8 bytes.
fn unescape(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            let mut buf = [0; 4];
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            continue;
        }
        match chars.next()? {
            '0' => out.push(0),
            't' => out.push(b'\t'),
            'n' => out.push(b'\n'),
            'r' => out.push(b'\r'),
            '\\' => out.push(b'\\'),
            'x' => {
                let hex: String = chars.by_ref().take(2).collect();
                out.push(u8::from_str_radix(&hex, 16).ok()?);
            }
            _ => return None,
        }
    }
    Some(out)
}

fn short(s: &str) -> String {
    s.chars().take(600).collect()
}

fn num(j: &Json, want: &str) -> bool {
    match j {
        Json::Number(n) => n.to_string() == want,
        Json::String(s) => s == want,
        _ => false,
    }
}

#[test]
fn interpreter_agrees_with_stellar_xdr_on_every_accepted_corpus_file() {
    // the corpus holds values nested hundreds of levels deep, which both decoders and the
    // comparison walk recursively
    std::thread::Builder::new()
        .stack_size(1 << 30)
        .spawn(agree)
        .unwrap()
        .join()
        .unwrap();
}

fn agree() {
    let schema = Schema::pinned();
    // stellar-xdr reads with Limits::none() here, so the interpreter reads without its limit
    let mut interp = Interp::new(&schema);
    interp.depth_limit = u32::MAX;
    let cmp = Cmp {
        schema: &schema,
        interp,
    };
    let m: Json =
        serde_json::from_slice(&std::fs::read(corpus().join("manifest.json")).unwrap()).unwrap();
    let mut files = 0;
    for e in m["entries"].as_array().unwrap() {
        if e["kind"] != "xdr" || e["expected"]["decode"] != "must-accept" {
            continue;
        }
        let ty = e["xdr_type"].as_str().unwrap();
        // corpus types are named as the stellar-xdr crate names them
        let xdr_name = schema
            .definitions()
            .iter()
            .find(|d| rust_type_name(d.name()) == ty)
            .map(|d| d.name().to_string())
            .unwrap_or_else(|| panic!("{ty} is not in the schema"));
        let variant = TypeVariant::from_str(ty).unwrap();
        for (vname, v) in e["variants"].as_object().unwrap() {
            let id = format!("{} {vname}", e["id"].as_str().unwrap());
            let bytes = std::fs::read(corpus().join(v["xdr_file"].as_str().unwrap())).unwrap();
            let ours = cmp
                .interp
                .decode(&xdr_name, &bytes)
                .unwrap_or_else(|err| panic!("{id}: the interpreter refuses it: {err}"));
            assert_eq!(
                cmp.interp.encode(&xdr_name, &ours).unwrap(),
                bytes,
                "{id}: re-encoding"
            );
            let theirs = stellar_xdr::Type::read_xdr_to_end(
                variant,
                &mut stellar_xdr::Limited::new(bytes.as_slice(), Limits::none()),
            )
            .unwrap_or_else(|err| panic!("{id}: stellar-xdr refuses it: {err}"));
            assert_eq!(
                theirs.to_xdr(Limits::none()).unwrap(),
                bytes,
                "{id}: stellar-xdr re-encoding"
            );
            // stellar_xdr::Type serializes as the value it holds
            let j = serde_json::to_value(&theirs).unwrap();
            cmp.same_definition(&xdr_name, &ours, &j, ty)
                .unwrap_or_else(|err| panic!("{id}: {}", short(&err)));
            files += 1;
        }
    }
    assert_eq!(files, 1111);
}

fn candidate() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../candidate")
}

#[test]
fn interpreter_agrees_with_stellar_xdr_on_every_accepted_format_vector() {
    std::thread::Builder::new()
        .stack_size(1 << 30)
        .spawn(agree_format)
        .unwrap()
        .join()
        .unwrap();
}

/// Every format vector of the candidate lane that a decoder must accept: stellar-xdr reads it
/// under the corpus limit for its type, the interpreter reads it, and the two give the same
/// value tree. The recursion-depth vectors past the limit must be refused by stellar-xdr on
/// depth alone, and read by both one level higher.
fn agree_format() {
    let schema = Schema::pinned();
    let mut interp = Interp::new(&schema);
    // RFC 4506 sets no depth limit; the limit under test is stellar-xdr's
    interp.depth_limit = u32::MAX;
    let cmp = Cmp {
        schema: &schema,
        interp,
    };
    let m: Json =
        serde_json::from_slice(&std::fs::read(candidate().join("manifest.json")).unwrap()).unwrap();
    let (mut accepted, mut deep, mut past) = (0, 0, 0);
    for e in m["entries"].as_array().unwrap() {
        if e["family"] != "format.xdr-grammar" {
            continue;
        }
        let id = e["id"].as_str().unwrap();
        let depth_vector = id.contains("-depth-");
        let decode = e["expected"]["decode"].as_str().unwrap();
        if decode != "must-accept" && !depth_vector {
            continue;
        }
        let ty = e["xdr_type"].as_str().unwrap();
        let xdr_name = schema
            .definitions()
            .iter()
            .find(|d| rust_type_name(d.name()) == ty)
            .map(|d| d.name().to_string())
            .unwrap_or_else(|| panic!("{ty} is not in the schema"));
        let file = e["variants"]["any"]["xdr_file"].as_str().unwrap();
        let bytes = std::fs::read(candidate().join(file)).unwrap();
        let limits = shapekit_manifest::decode_limits(ty, bytes.len());
        let read = |limits: Limits| shapekit_manifest::xdr::decode(ty, &bytes, limits);
        let theirs = if decode == "must-accept" {
            read(limits).unwrap_or_else(|err| panic!("{id}: stellar-xdr refuses it: {err}"))
        } else {
            match read(limits.clone()) {
                Err(shapekit_manifest::ToolkitError::Decode(
                    stellar_xdr::Error::DepthLimitExceeded,
                )) => {}
                other => panic!("{id}: stellar-xdr does not stop on depth: {other:?}"),
            }
            past += 1;
            read(Limits {
                depth: limits.depth + 1,
                len: limits.len,
            })
            .unwrap_or_else(|err| panic!("{id}: stellar-xdr refuses it one level higher: {err}"))
        };
        let ours = cmp
            .interp
            .decode(&xdr_name, &bytes)
            .unwrap_or_else(|err| panic!("{id}: the interpreter refuses it: {err}"));
        assert_eq!(cmp.interp.encode(&xdr_name, &ours).unwrap(), bytes, "{id}");
        let j = serde_json::to_value(&theirs).unwrap();
        cmp.same_definition(&xdr_name, &ours, &j, ty)
            .unwrap_or_else(|err| panic!("{id}: {}", short(&err)));
        accepted += 1;
        if depth_vector {
            deep += 1;
        }
    }
    eprintln!(
        "{accepted} format vectors agree, {deep} of them recursion-depth vectors, {past} past the limit"
    );
    assert_eq!((deep, past), (45, 15));
}
