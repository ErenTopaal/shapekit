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

use anyhow::{Context, Result, bail};
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
    fn value_of_type(&self, ty: &Type, json: &Value) -> Result<IValue> {
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
            other => bail!("no field of a string-shaped struct has type {other:?}"),
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

    /// The single-field replacements for a field of type `ty` whose value is `base`.
    pub fn replacements(&self, ty: &Type, base: &Value) -> Result<Vec<Replacement>> {
        let resolved = self.schema.resolve(ty);
        let mut out = primitive_replacements(self.schema, ty, base);
        match resolved {
            Type::Ref { name } => match self.schema.get(name) {
                Some(Definition::Union(u)) => {
                    let IValue::Union { discriminant, .. } = self.value_of(name, base)? else {
                        bail!("{name} did not decode as a union");
                    };
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
                            out.push(Replacement {
                                name: format!("arm {}, smallest", case_name(self.schema, u, c)),
                                value: self.json_of(name, &v)?,
                            });
                        }
                    }
                }
                Some(Definition::Struct(_)) => {
                    let v = self
                        .smallest(resolved)
                        .with_context(|| format!("{name} has no smallest value"))?;
                    let j = self.json_of(name, &v)?;
                    if &j != base {
                        out.push(Replacement {
                            name: "smallest value".into(),
                            value: j,
                        });
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
                });
            }
            Type::VarArray { element, .. } if base.as_array().is_some_and(|a| a.is_empty()) => {
                let v = self
                    .smallest(element)
                    .context("an array of a type with no value")?;
                out.push(Replacement {
                    name: "one element, smallest".into(),
                    value: Value::Array(vec![self.json_of_type(element, &v)?]),
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
