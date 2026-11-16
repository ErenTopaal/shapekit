//! Changing one field of an envelope. The envelope is edited as XDR-JSON: a field of a struct
//! XDR-JSON writes as an object is a key of that object, so the edit replaces one key. The
//! structs XDR-JSON writes as one string (muxed ids, 128 and 256 bit parts, signed payloads)
//! are read into their fields through the schema, edited there and written back, so the edit
//! is still one field.
//!
//! Replacement values come from the field's schema type: integers at their bounds and next to
//! the base value, other enum members, every other union arm with its smallest payload, the
//! smallest value of a struct, an absent or present optional, an empty or longer array. The
//! smallest values are the reference interpreter's (`shapekit_schema::vectors::Builder`), turned
//! into XDR-JSON by encoding them and decoding the bytes with stellar-xdr.

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use shapekit_schema::interp::{Interp, Value as IValue};
use shapekit_schema::names::case_name;
use shapekit_schema::vectors::Builder;
use shapekit_schema::{Definition, Schema, Type};
use stellar_xdr::{Limits, TypeVariant, WriteXdr};

use crate::fields::{json_name, rust_name};
use crate::values::{Replacement, replacements as primitive_replacements};

/// The stellar-xdr type variant of a schema definition.
pub fn variant(name: &str) -> Result<TypeVariant> {
    rust_name(name)
        .parse()
        .map_err(|_| anyhow::anyhow!("{name} has no stellar-xdr type"))
}

/// Typed conversions between XDR-JSON, the interpreter's values and stellar-xdr.
pub struct Typer<'s> {
    schema: &'s Schema,
    builder: Builder<'s>,
    interp: Interp<'s>,
}

impl<'s> Typer<'s> {
    pub fn new(schema: &'s Schema) -> Self {
        Self {
            schema,
            builder: Builder::new(schema),
            interp: Interp::new(schema),
        }
    }

    pub fn schema(&self) -> &'s Schema {
        self.schema
    }

    /// XDR-JSON of an interpreter value of definition `name`.
    pub fn json_of(&self, name: &str, v: &IValue) -> Result<Value> {
        let bytes = self
            .interp
            .encode(name, v)
            .map_err(|e| anyhow::anyhow!("encoding a {name}: {e}"))?;
        let t = stellar_xdr::Type::from_xdr(variant(name)?, &bytes, Limits::none())
            .with_context(|| format!("stellar-xdr reading the {name} the interpreter wrote"))?;
        Ok(serde_json::to_value(&t)?)
    }

    /// A replacement holding `v` of definition `name`, with its XDR when XDR-JSON cannot write
    /// it back.
    fn rep(&self, name: &str, label: String, v: &IValue) -> Result<Replacement> {
        let bytes = self
            .interp
            .encode(name, v)
            .map_err(|e| anyhow::anyhow!("encoding a {name}: {e}"))?;
        let value = self.json_of(name, v)?;
        let back = self.xdr_of(name, &value).ok();
        Ok(Replacement {
            name: label,
            value,
            xdr: (back.as_deref() != Some(bytes.as_slice())).then_some(bytes),
        })
    }

    /// The interpreter value of a replacement of type `ty`.
    fn value_of_rep(&self, ty: &Type, r: &Replacement) -> Result<IValue> {
        match (&r.xdr, self.schema.resolve(ty)) {
            (Some(bytes), Type::Ref { name }) => self
                .interp
                .decode(name, bytes)
                .map_err(|e| anyhow::anyhow!("decoding a {name}: {e}")),
            (Some(_), t) => bail!("raw XDR replacement of a {t:?}"),
            (None, _) => self.value_of_type(ty, &r.value),
        }
    }

    /// The interpreter value of an XDR-JSON value of definition `name`.
    pub fn value_of(&self, name: &str, json: &Value) -> Result<IValue> {
        let bytes = self.xdr_of(name, json)?;
        self.interp
            .decode(name, &bytes)
            .map_err(|e| anyhow::anyhow!("interpreter decoding a {name}: {e}"))
    }

    /// The XDR bytes of an XDR-JSON value of definition `name`.
    pub fn xdr_of(&self, name: &str, json: &Value) -> Result<Vec<u8>> {
        let t = stellar_xdr::Type::from_json(variant(name)?, json.to_string().as_bytes())
            .with_context(|| format!("reading {json} as a {name}"))?;
        Ok(t.to_xdr(Limits::none())?)
    }

    /// The smallest value of a type, `None` for a type with no finite value.
    pub fn smallest(&self, ty: &Type) -> Option<IValue> {
        Some(match ty {
            Type::Int => IValue::Int(0),
            Type::UnsignedInt => IValue::UnsignedInt(0),
            Type::Hyper => IValue::Hyper(0),
            Type::UnsignedHyper => IValue::UnsignedHyper(0),
            Type::Float => IValue::Float(0),
            Type::Double => IValue::Double(0),
            Type::Bool => IValue::Bool(false),
            Type::OpaqueFixed { size } => IValue::Opaque(vec![0; *size as usize]),
            Type::OpaqueVar { .. } => IValue::Opaque(Vec::new()),
            Type::String { .. } => IValue::String(Vec::new()),
            Type::Array { element, count } => {
                IValue::Array(vec![self.smallest(element)?; *count as usize])
            }
            Type::VarArray { .. } => IValue::Array(Vec::new()),
            Type::Optional { .. } => IValue::Optional(None),
            Type::Ref { name } => self.builder.smallest(name)?.clone(),
        })
    }

    /// XDR-JSON of a value of any type: a definition through stellar-xdr, a primitive as
    /// XDR-JSON writes it (64-bit integers as decimal strings, opaque as hex).
    pub fn json_of_type(&self, ty: &Type, v: &IValue) -> Result<Value> {
        // a named type of bytes has its own XDR-JSON form (ContractID is a C strkey); named
        // integers are written like the bare integer inside a struct
        if let Type::Ref { name } = ty
            && self.bytes_like(ty)
        {
            return self.json_of(name, v);
        }
        let ty = self.schema.resolve(ty);
        Ok(match (ty, v) {
            (Type::Ref { name }, _) => self.json_of(name, v)?,
            (_, IValue::Int(n)) => json!(n),
            (_, IValue::UnsignedInt(n)) => json!(n),
            (_, IValue::Hyper(n)) => json!(n.to_string()),
            (_, IValue::UnsignedHyper(n)) => json!(n.to_string()),
            (_, IValue::Bool(b)) => json!(b),
            (_, IValue::Opaque(b)) => json!(hex::encode(b)),
            (Type::String { .. }, IValue::String(b)) => {
                // XDR-JSON escapes the bytes of a string; stellar-xdr reads its own escaping
                let s = stellar_xdr::StringM::<{ u32::MAX }>::try_from(b.clone())?;
                serde_json::to_value(&s)?
            }
            (Type::Optional { element }, IValue::Optional(o)) => match o {
                None => Value::Null,
                Some(inner) => self.json_of_type(element, inner)?,
            },
            (
                Type::Array { element, .. } | Type::VarArray { element, .. },
                IValue::Array(items),
            ) => Value::Array(
                items
                    .iter()
                    .map(|i| self.json_of_type(element, i))
                    .collect::<Result<_>>()?,
            ),
            (t, v) => bail!("no XDR-JSON for a {t:?} holding {v:?}"),
        })
    }

    /// The interpreter value of XDR-JSON of a primitive type, the inverse of
    /// [`Self::json_of_type`] for the field types of the string-shaped structs.
    pub fn value_of_type(&self, ty: &Type, json: &Value) -> Result<IValue> {
        if let Type::Ref { name } = ty
            && self.bytes_like(ty)
        {
            return self.value_of(name, json);
        }
        let ty = self.schema.resolve(ty);
        let int = |j: &Value| -> Result<i128> {
            match j {
                Value::Number(n) => n
                    .as_i64()
                    .map(i128::from)
                    .or(n.as_u64().map(i128::from))
                    .context("not an integer"),
                Value::String(s) => Ok(s.parse()?),
                other => bail!("{other} is not an integer"),
            }
        };
        Ok(match ty {
            Type::Ref { name } => self.value_of(name, json)?,
            Type::Int => IValue::Int(int(json)?.try_into()?),
            Type::UnsignedInt => IValue::UnsignedInt(int(json)?.try_into()?),
            Type::Hyper => IValue::Hyper(int(json)?.try_into()?),
            Type::UnsignedHyper => IValue::UnsignedHyper(int(json)?.try_into()?),
            Type::OpaqueFixed { .. } | Type::OpaqueVar { .. } => IValue::Opaque(hex::decode(
                json.as_str().context("opaque is a hex string")?,
            )?),
            Type::Bool => IValue::Bool(json.as_bool().context("not a bool")?),
            Type::Optional { element } => IValue::Optional(match json {
                Value::Null => None,
                j => Some(Box::new(self.value_of_type(element, j)?)),
            }),
            Type::Array { element, .. } | Type::VarArray { element, .. } => IValue::Array(
                json.as_array()
                    .context("not an array")?
                    .iter()
                    .map(|j| self.value_of_type(element, j))
                    .collect::<Result<_>>()?,
            ),
            Type::String { .. } => {
                let s: stellar_xdr::StringM<{ u32::MAX }> = serde_json::from_value(json.clone())?;
                IValue::String(s.to_vec())
            }
            other => bail!("no XDR-JSON reader for a {other:?}"),
        })
    }

    /// The fields of a struct XDR-JSON writes as one string, keyed by their `.x` names.
    pub fn view(&self, structure: &str, json: &Value) -> Result<serde_json::Map<String, Value>> {
        let Some(Definition::Struct(s)) = self.schema.get(structure) else {
            bail!("{structure} is not a struct");
        };
        let IValue::Struct(vals) = self.value_of(structure, json)? else {
            bail!("{structure} did not decode as a struct");
        };
        let mut out = serde_json::Map::new();
        for (f, v) in s.fields.iter().zip(&vals) {
            out.insert(f.name.clone(), self.json_of_type(&f.ty, v)?);
        }
        Ok(out)
    }

    /// The XDR-JSON string of a string-shaped struct from its fields.
    pub fn unview(
        &self,
        structure: &str,
        fields: &serde_json::Map<String, Value>,
    ) -> Result<Value> {
        let Some(Definition::Struct(s)) = self.schema.get(structure) else {
            bail!("{structure} is not a struct");
        };
        let vals = s
            .fields
            .iter()
            .map(|f| {
                let j = fields
                    .get(&f.name)
                    .with_context(|| format!("{structure} view has no {}", f.name))?;
                self.value_of_type(&f.ty, j)
            })
            .collect::<Result<Vec<_>>>()?;
        self.json_of(structure, &IValue::Struct(vals))
    }

    /// The byte offset, inside the encoding of `v` of type `ty`, of the value the XDR-JSON
    /// pointer segments `segs` lead to, and that value with its type. Struct segments are
    /// field keys, union segments are case names, array segments are indexes, and an optional
    /// takes no segment.
    pub fn offset(&self, ty: &Type, v: &IValue, segs: &[&str]) -> Result<(usize, Type, IValue)> {
        let Some((seg, rest)) = segs.split_first() else {
            return Ok((0, ty.clone(), v.clone()));
        };
        let size = |t: &Type, x: &IValue| -> Result<usize> {
            Ok(self
                .interp
                .encode_as(t, x)
                .map_err(|e| anyhow::anyhow!("encoding a {t:?}: {e}"))?
                .len())
        };
        let shift = |n: usize, r: (usize, Type, IValue)| (n + r.0, r.1, r.2);
        match (self.schema.resolve(ty), v) {
            (Type::Ref { name }, IValue::Struct(vals)) => {
                let Some(Definition::Struct(st)) = self.schema.get(name) else {
                    bail!("{name} is not a struct");
                };
                let mut off = 0;
                for (f, x) in st.fields.iter().zip(vals) {
                    if key(&f.name) == *seg {
                        return Ok(shift(off, self.offset(&f.ty, x, rest)?));
                    }
                    off += size(&f.ty, x)?;
                }
                bail!("{name} has no field {seg}")
            }
            (Type::Ref { name }, IValue::Union { discriminant, arm }) => {
                let Some(Definition::Union(u)) = self.schema.get(name) else {
                    bail!("{name} is not a union");
                };
                let a = u.arm(*discriminant).context("a value outside its union")?;
                let case = a
                    .cases
                    .iter()
                    .find(|c| c.value == *discriminant)
                    .expect("the arm holds the discriminant");
                ensure!(
                    case_name(self.schema, u, case) == *seg,
                    "{name}: the pointer names arm {seg}, the value takes {}",
                    case_name(self.schema, u, case)
                );
                match (&a.ty, arm) {
                    (Some(t), Some(x)) => Ok(shift(4, self.offset(t, x, rest)?)),
                    _ => bail!("{name}: arm {seg} holds no value to point into"),
                }
            }
            (Type::VarArray { element, .. }, IValue::Array(items))
            | (Type::Array { element, .. }, IValue::Array(items)) => {
                let i: usize = seg.parse().with_context(|| format!("{seg} is no index"))?;
                let mut off = if matches!(self.schema.resolve(ty), Type::VarArray { .. }) {
                    4
                } else {
                    0
                };
                for x in items.iter().take(i) {
                    off += size(element, x)?;
                }
                let x = items.get(i).with_context(|| format!("no element {i}"))?;
                Ok(shift(off, self.offset(element, x, rest)?))
            }
            (Type::Optional { element }, IValue::Optional(Some(x))) => {
                Ok(shift(4, self.offset(element, x, segs)?))
            }
            (t, x) => bail!("cannot point into a {t:?} holding {x:?} with {seg}"),
        }
    }

    /// The offset of a JSON pointer inside an encoded `TransactionEnvelope`, with the value
    /// there and its type.
    pub fn envelope_offset(&self, bytes: &[u8], pointer: &str) -> Result<(usize, Type, IValue)> {
        let v = self
            .interp
            .decode("TransactionEnvelope", bytes)
            .map_err(|e| anyhow::anyhow!("decoding the envelope: {e}"))?;
        let segs: Vec<&str> = pointer.split('/').skip(1).collect();
        let (off, ty, v) = self.offset(
            &Type::Ref {
                name: "TransactionEnvelope".into(),
            },
            &v,
            &segs,
        )?;
        // an optional is written like its value
        match (self.schema.resolve(&ty).clone(), v) {
            (Type::Optional { element }, IValue::Optional(Some(x))) => Ok((off + 4, *element, *x)),
            (t, v) => Ok((off, t, v)),
        }
    }

    /// A typedef whose value is bytes: opaque or a string.
    fn bytes_like(&self, ty: &Type) -> bool {
        matches!(ty, Type::Ref { .. })
            && matches!(
                self.schema.resolve(ty),
                Type::OpaqueFixed { .. } | Type::OpaqueVar { .. } | Type::String { .. }
            )
    }

    /// The single-field replacements for a field of type `ty` whose value is `base`.
    pub fn replacements(&self, ty: &Type, base: &Value) -> Result<Vec<Replacement>> {
        let resolved = self.schema.resolve(ty);
        // a typedef of a primitive can have its own XDR-JSON form (PoolID is an L strkey):
        // the replacements are made on the primitive and written back through the typedef
        if let (Type::Ref { name }, true) = (ty, self.bytes_like(ty)) {
            let v = self.value_of(name, base)?;
            let pj = self.json_of_type(resolved, &v)?;
            return self
                .replacements(resolved, &pj)?
                .into_iter()
                .map(|r| {
                    let v = self.value_of_type(resolved, &r.value)?;
                    self.rep(name, r.name, &v)
                })
                .collect();
        }
        let mut out = primitive_replacements(self.schema, ty, base);
        match resolved {
            Type::Ref { name } => match self.schema.get(name) {
                Some(Definition::Union(u)) => {
                    let IValue::Union { discriminant, arm } = self.value_of(name, base)? else {
                        bail!("{name} did not decode as a union");
                    };
                    // the arm the base takes, with each other value of its payload
                    if let (Some(payload), Some(t)) =
                        (arm, u.arm(discriminant).and_then(|a| a.ty.as_ref()))
                    {
                        let pj = self.json_of_type(t, &payload)?;
                        for r in self.replacements(t, &pj)? {
                            let v = self.value_of_rep(t, &r)?;
                            let v = IValue::Union {
                                discriminant,
                                arm: Some(Box::new(v)),
                            };
                            out.push(self.rep(name, format!("same arm, {}", r.name), &v)?);
                        }
                    }
                    for a in &u.arms {
                        let arm = match &a.ty {
                            None => None,
                            Some(t) => match self.smallest(t) {
                                Some(v) => Some(Box::new(v)),
                                None => continue,
                            },
                        };
                        for c in &a.cases {
                            if c.value == discriminant {
                                continue;
                            }
                            let v = IValue::Union {
                                discriminant: c.value,
                                arm: arm.clone(),
                            };
                            out.push(self.rep(
                                name,
                                format!("arm {}, smallest", case_name(self.schema, u, c)),
                                &v,
                            )?);
                        }
                    }
                }
                Some(Definition::Struct(_)) => {
                    let v = self
                        .smallest(resolved)
                        .with_context(|| format!("{name} has no smallest value"))?;
                    let r = self.rep(name, "smallest value".into(), &v)?;
                    if &r.value != base {
                        out.push(r);
                    }
                }
                _ => {}
            },
            Type::Optional { element } if base.is_null() => {
                let v = self
                    .smallest(element)
                    .context("an optional of a type with no value")?;
                out.push(Replacement {
                    name: "present, smallest".into(),
                    value: self.json_of_type(element, &v)?,
                    xdr: None,
                });
            }
            Type::VarArray { element, .. } if base.as_array().is_some_and(|a| a.is_empty()) => {
                let v = self
                    .smallest(element)
                    .context("an array of a type with no value")?;
                out.push(Replacement {
                    name: "one element, smallest".into(),
                    value: Value::Array(vec![self.json_of_type(element, &v)?]),
                    xdr: None,
                });
            }
            _ => {}
        }
        Ok(out)
    }
}

/// The JSON key of a `.x` field name inside its struct's XDR-JSON object.
pub fn key(field: &str) -> String {
    json_name(field)
}

/// Sets the value at a JSON pointer.
pub fn set(doc: &mut Value, pointer: &str, value: Value) -> Result<()> {
    let slot = doc
        .pointer_mut(pointer)
        .with_context(|| format!("nothing at {pointer}"))?;
    *slot = value;
    Ok(())
}

pub fn get<'a>(doc: &'a Value, pointer: &str) -> Result<&'a Value> {
    doc.pointer(pointer)
        .with_context(|| format!("nothing at {pointer}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_fields_take_every_other_arm() {
        let s = Schema::pinned();
        let t = Typer::new(&s);
        let ty = Type::Ref {
            name: "Memo".into(),
        };
        let r = t.replacements(&ty, &json!("none")).unwrap();
        let names: Vec<_> = r.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "arm text, smallest",
                "arm id, smallest",
                "arm hash, smallest",
                "arm return, smallest"
            ]
        );
        assert_eq!(r[1].value, json!({"id": "0"}));
    }

    #[test]
    fn string_shaped_structs_open_into_their_fields() {
        let s = Schema::pinned();
        let t = Typer::new(&s);
        let m = stellar_xdr::MuxedAccount::MuxedEd25519(stellar_xdr::MuxedAccountMed25519 {
            id: 7,
            ed25519: stellar_xdr::Uint256([3; 32]),
        });
        let j = serde_json::to_value(&m).unwrap();
        let mut v = t.view("MuxedAccountMed25519", &j).unwrap();
        assert_eq!(v["id"], json!("7"));
        v.insert("id".into(), json!("8"));
        let back = t.unview("MuxedAccountMed25519", &v).unwrap();
        let m2: stellar_xdr::MuxedAccount = serde_json::from_value(back).unwrap();
        match m2 {
            stellar_xdr::MuxedAccount::MuxedEd25519(x) => assert_eq!(x.id, 8),
            other => panic!("{other:?}"),
        }
        let i = stellar_xdr::ScVal::I128(stellar_xdr::Int128Parts { hi: -1, lo: 5 });
        let ij = serde_json::to_value(&i).unwrap();
        let parts = &ij["i128"];
        let v = t.view("Int128Parts", parts).unwrap();
        assert_eq!(v["hi"], json!("-1"));
        assert_eq!(v["lo"], json!("5"));
    }

    #[test]
    fn empty_arrays_and_absent_optionals_get_a_value() {
        let s = Schema::pinned();
        let t = Typer::new(&s);
        let ty = Type::VarArray {
            element: Box::new(Type::Ref {
                name: "Asset".into(),
            }),
            max_count: Some(5),
        };
        let r = t.replacements(&ty, &json!([])).unwrap();
        assert_eq!(r[0].value, json!(["native"]));
    }
}
