//! Contract interfaces read from a wasm's `contractspecv0` section, and argument values
//! filled from them: each argument is generated from its declared type, user-defined types
//! included, with accounts, contracts and code hashes drawn from the episode.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use stellar_xdr::{Limited, Limits, ReadXdr, ScSpecEntry, ScSpecTypeDef, ScSpecUdtUnionCaseV0};

use crate::fill::Filler;

/// One function of a contract's interface.
#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub inputs: Vec<ScSpecTypeDef>,
}

/// A contract's interface.
#[derive(Debug, Clone, Default)]
pub struct Interface {
    pub functions: Vec<Function>,
    /// User-defined types by name.
    pub types: BTreeMap<String, ScSpecEntry>,
}

impl Interface {
    /// Reads the spec from `wasm`.
    pub fn of(wasm: &[u8]) -> Result<Self> {
        let section = custom_section(wasm, "contractspecv0")?
            .context("the wasm has no contractspecv0 section")?;
        let mut r = Limited::new(std::io::Cursor::new(section), Limits::none());
        let mut out = Interface::default();
        while (r.inner.position() as usize) < section.len() {
            let e = ScSpecEntry::read_xdr(&mut r).context("decoding a spec entry")?;
            match &e {
                ScSpecEntry::FunctionV0(f) => out.functions.push(Function {
                    name: f.name.to_utf8_string_lossy(),
                    inputs: f.inputs.iter().map(|i| i.type_.clone()).collect(),
                }),
                ScSpecEntry::UdtStructV0(s) => {
                    out.types.insert(s.name.to_utf8_string_lossy(), e.clone());
                }
                ScSpecEntry::UdtUnionV0(u) => {
                    out.types.insert(u.name.to_utf8_string_lossy(), e.clone());
                }
                ScSpecEntry::UdtEnumV0(u) => {
                    out.types.insert(u.name.to_utf8_string_lossy(), e.clone());
                }
                ScSpecEntry::UdtErrorEnumV0(u) => {
                    out.types.insert(u.name.to_utf8_string_lossy(), e.clone());
                }
                ScSpecEntry::EventV0(_) => {}
            }
        }
        Ok(out)
    }

    pub fn constructor(&self) -> Option<&Function> {
        self.functions.iter().find(|f| f.name == "__constructor")
    }
}

fn leb128(b: &[u8], mut i: usize) -> Option<(usize, usize)> {
    let mut v: usize = 0;
    let mut shift = 0;
    loop {
        let byte = *b.get(i)?;
        i += 1;
        v |= usize::from(byte & 0x7f).checked_shl(shift)?;
        if byte & 0x80 == 0 {
            return Some((v, i));
        }
        shift += 7;
    }
}

/// The body of the custom section `name`, if the module has one.
pub fn custom_section<'a>(wasm: &'a [u8], name: &str) -> Result<Option<&'a [u8]>> {
    if wasm.get(..4) != Some(b"\0asm".as_slice()) {
        bail!("not a wasm module");
    }
    let mut i = 8;
    while i < wasm.len() {
        let id = wasm[i];
        let Some((len, next)) = leb128(wasm, i + 1) else {
            bail!("a section length at byte {} does not parse", i + 1)
        };
        let Some(body) = wasm.get(next..next + len) else {
            bail!("a section runs past the end of the module")
        };
        if id == 0
            && let Some((name_len, at)) = leb128(body, 0)
            && body.get(at..at + name_len) == Some(name.as_bytes())
        {
            return Ok(Some(&body[at + name_len..]));
        }
        i = next + len;
    }
    Ok(None)
}

/// References the episode can name in arguments.
#[derive(Debug, Clone, Default)]
pub struct Names {
    /// `deploy` labels of contract instances.
    pub contracts: Vec<String>,
    /// `upload` labels of code.
    pub wasm: Vec<String>,
}

/// An XDR-JSON `ScVal` of type `ty`, in recipe form.
pub fn value(
    f: &mut Filler<'_>,
    iface: &Interface,
    names: &Names,
    ty: &ScSpecTypeDef,
    depth: u32,
) -> Value {
    let small_u: u64 = *f.rng.pick(&[0, 1, 2, 7, 1000, u64::from(u32::MAX)]);
    match ty {
        ScSpecTypeDef::Val => f.def("SCVal"),
        ScSpecTypeDef::Bool => json!({ "bool": f.rng.chance(1, 2) }),
        ScSpecTypeDef::Void => json!("void"),
        ScSpecTypeDef::Error => f.def("SCVal"),
        ScSpecTypeDef::U32 => json!({ "u32": small_u.min(u64::from(u32::MAX)) }),
        ScSpecTypeDef::I32 => json!({ "i32": *f.rng.pick(&[0i32, 1, -1, i32::MAX, i32::MIN]) }),
        ScSpecTypeDef::U64 => json!({ "u64": small_u.to_string() }),
        ScSpecTypeDef::I64 => json!({ "i64": (*f.rng.pick(&[0i64, 1, -1, i64::MAX])).to_string() }),
        ScSpecTypeDef::Timepoint => json!({ "timepoint": small_u.to_string() }),
        ScSpecTypeDef::Duration => json!({ "duration": small_u.to_string() }),
        ScSpecTypeDef::U128 => json!({ "u128": f.def("UInt128Parts") }),
        ScSpecTypeDef::I128 => json!({ "i128": f.def("Int128Parts") }),
        ScSpecTypeDef::U256 => json!({ "u256": f.def("UInt256Parts") }),
        ScSpecTypeDef::I256 => json!({ "i256": f.def("Int256Parts") }),
        ScSpecTypeDef::Bytes => {
            let n = *f.rng.pick(&[0usize, 1, 32, 64]);
            json!({ "bytes": hex::encode(f.rng.bytes(n)) })
        }
        ScSpecTypeDef::String => json!({ "string": *f.rng.pick(&["", "tag", "shapekit"]) }),
        ScSpecTypeDef::Symbol => json!({ "symbol": *f.rng.pick(&["", "a", "tag"]) }),
        ScSpecTypeDef::Address | ScSpecTypeDef::MuxedAddress => {
            json!({ "address": address(f, names) })
        }
        ScSpecTypeDef::Option(o) => {
            if f.rng.chance(1, 3) {
                json!("void")
            } else {
                value(f, iface, names, &o.value_type, depth + 1)
            }
        }
        ScSpecTypeDef::Result(r) => value(f, iface, names, &r.ok_type, depth + 1),
        ScSpecTypeDef::Vec(v) => {
            let n = if depth > 3 {
                0
            } else {
                *f.rng.pick(&[0usize, 1, 1, 2, 3])
            };
            json!({ "vec": (0..n).map(|_| value(f, iface, names, &v.element_type, depth + 1)).collect::<Vec<_>>() })
        }
        ScSpecTypeDef::Map(m) => {
            let n = if depth > 3 {
                0
            } else {
                *f.rng.pick(&[0usize, 1, 2])
            };
            let entries: Vec<Value> = (0..n)
                .map(|_| {
                    json!({
                        "key": value(f, iface, names, &m.key_type, depth + 1),
                        "val": value(f, iface, names, &m.value_type, depth + 1),
                    })
                })
                .collect();
            json!({ "map": entries })
        }
        ScSpecTypeDef::Tuple(t) => json!({
            "vec": t.value_types.iter().map(|x| value(f, iface, names, x, depth + 1)).collect::<Vec<_>>()
        }),
        ScSpecTypeDef::BytesN(b) => {
            if b.n == 32 && !names.wasm.is_empty() && f.rng.chance(2, 3) {
                json!({ "bytes": format!("@wasm:{}", f.rng.pick(&names.wasm)) })
            } else {
                json!({ "bytes": hex::encode(f.rng.bytes(b.n as usize)) })
            }
        }
        ScSpecTypeDef::Udt(u) => {
            let name = u.name.to_utf8_string_lossy();
            match iface.types.get(&name) {
                Some(ScSpecEntry::UdtStructV0(s)) => {
                    // a contract struct is a map keyed by field name, keys in order
                    let mut fields: Vec<(String, &ScSpecTypeDef)> = s
                        .fields
                        .iter()
                        .map(|x| (x.name.to_utf8_string_lossy(), &x.type_))
                        .collect();
                    fields.sort_by(|a, b| a.0.cmp(&b.0));
                    let entries: Vec<Value> = fields
                        .into_iter()
                        .map(|(n, t)| json!({ "key": { "symbol": n }, "val": value(f, iface, names, t, depth + 1) }))
                        .collect();
                    json!({ "map": entries })
                }
                Some(ScSpecEntry::UdtUnionV0(un)) if !un.cases.is_empty() => {
                    let case = &un.cases[f.rng.below(un.cases.len())];
                    match case {
                        ScSpecUdtUnionCaseV0::VoidV0(c) => {
                            json!({ "vec": [ { "symbol": c.name.to_utf8_string_lossy() } ] })
                        }
                        ScSpecUdtUnionCaseV0::TupleV0(c) => {
                            let mut items =
                                vec![json!({ "symbol": c.name.to_utf8_string_lossy() })];
                            for t in c.type_.iter() {
                                items.push(value(f, iface, names, t, depth + 1));
                            }
                            json!({ "vec": items })
                        }
                    }
                }
                Some(ScSpecEntry::UdtEnumV0(e)) if !e.cases.is_empty() => {
                    json!({ "u32": e.cases[f.rng.below(e.cases.len())].value })
                }
                _ => f.def("SCVal"),
            }
        }
    }
}

/// An address argument: a role's account most of the time, else a deployed contract.
fn address(f: &mut Filler<'_>, names: &Names) -> Value {
    if !names.contracts.is_empty() && f.rng.chance(1, 3) {
        Value::String(format!("@contract:{}", f.rng.pick(&names.contracts)))
    } else {
        Value::String(format!("@account:{}", f.role()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_committed_contracts_have_readable_interfaces() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/wasm");
        let echo = Interface::of(&std::fs::read(root.join("echo.wasm")).unwrap()).unwrap();
        let names: Vec<&str> = echo.functions.iter().map(|f| f.name.as_str()).collect();
        assert!(
            names.contains(&"echo") && names.contains(&"emit"),
            "{names:?}"
        );
        let tree = Interface::of(&std::fs::read(root.join("auth_tree.wasm")).unwrap()).unwrap();
        assert!(tree.types.contains_key("Level"), "{:?}", tree.types.keys());
        let factory = Interface::of(&std::fs::read(root.join("factory.wasm")).unwrap()).unwrap();
        assert_eq!(factory.constructor().map(|c| c.inputs.len()), Some(1));
    }
}
