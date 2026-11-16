//! Format vectors: for every decision a decoder makes while reading a definition, one input it
//! must accept for each allowed choice and one it must refuse for each disallowed one. Each
//! vector is a whole value of the definition, built around its smallest valid value, and is
//! checked against the reference interpreter before it is returned, so the expectation is the
//! interpreter's verdict and never typed in.

use std::collections::BTreeMap;

use crate::interp::{DecodeErrorKind, Interp, Value};
use crate::names::{case_name, field_name, member_name, rust_type_name};
use crate::{Definition, Schema, Type};

/// What a decoder must do with a vector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Expect {
    MustAccept,
    MustReject,
    /// RFC 4506 refuses the input, but decoders are known to differ (a bool word of 2, which
    /// stellar-xdr reads as false); every decoder must at least agree with the others.
    ImplementationDefined,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Vector {
    /// The `format` obligation this vector meets: `format:<Type><point>:<choice>`.
    pub obligation: String,
    /// The definition the bytes encode, by its stellar-xdr crate name.
    pub type_name: String,
    #[serde(with = "hex_bytes")]
    pub bytes: Vec<u8>,
    pub expect: Expect,
    /// The decision point inside the type, as a signature path suffix (empty for the type's
    /// own discriminant or member word).
    pub point: String,
    pub note: String,
}

mod hex_bytes {
    pub fn serialize<S: serde::Serializer>(b: &[u8], s: S) -> Result<S::Ok, S::Error> {
        let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
        s.serialize_str(&hex)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VectorError {
    #[error("{0} has no finite value")]
    Infinite(String),
    #[error("vector {obligation}: the interpreter says {got}, the vector expects {want:?}")]
    Verdict {
        obligation: String,
        got: String,
        want: Expect,
    },
    #[error("encoding the smallest value of {0}: {1}")]
    Encode(String, String),
}

/// Smallest values per definition, computed once.
pub struct Builder<'a> {
    schema: &'a Schema,
    interp: Interp<'a>,
    smallest: BTreeMap<String, Value>,
}

impl<'a> Builder<'a> {
    /// Computes the smallest value of every definition by fixed point: a definition gets a
    /// value once every type it needs has one, so recursive definitions take their
    /// non-recursive arms.
    pub fn new(schema: &'a Schema) -> Builder<'a> {
        let mut b = Builder {
            schema,
            interp: Interp::new(schema),
            smallest: BTreeMap::new(),
        };
        loop {
            let mut progress = false;
            for d in schema.definitions() {
                if matches!(d, Definition::Const(_)) || b.smallest.contains_key(d.name()) {
                    continue;
                }
                if let Some(v) = b.smallest_definition(d) {
                    b.smallest.insert(d.name().to_string(), v);
                    progress = true;
                }
            }
            if !progress {
                break;
            }
        }
        b
    }

    pub fn smallest(&self, name: &str) -> Option<&Value> {
        self.smallest.get(name)
    }

    fn smallest_type(&self, ty: &Type) -> Option<Value> {
        Some(match ty {
            Type::Int => Value::Int(0),
            Type::UnsignedInt => Value::UnsignedInt(0),
            Type::Hyper => Value::Hyper(0),
            Type::UnsignedHyper => Value::UnsignedHyper(0),
            Type::Float => Value::Float(0),
            Type::Double => Value::Double(0),
            Type::Bool => Value::Bool(false),
            Type::OpaqueFixed { size } => Value::Opaque(vec![0; *size as usize]),
            Type::OpaqueVar { .. } => Value::Opaque(Vec::new()),
            Type::String { .. } => Value::String(Vec::new()),
            Type::Array { element, count } => {
                let e = self.smallest_type(element)?;
                Value::Array(vec![e; *count as usize])
            }
            Type::VarArray { .. } => Value::Array(Vec::new()),
            Type::Optional { .. } => Value::Optional(None),
            Type::Ref { name } => self.smallest.get(name)?.clone(),
        })
    }

    fn smallest_definition(&self, d: &Definition) -> Option<Value> {
        match d {
            Definition::Typedef(t) => self.smallest_type(&t.ty),
            Definition::Struct(s) => s
                .fields
                .iter()
                .map(|f| self.smallest_type(&f.ty))
                .collect::<Option<Vec<_>>>()
                .map(Value::Struct),
            Definition::Enum(e) => e.members.first().map(|m| Value::Enum(m.value as i32)),
            Definition::Union(u) => {
                // the first arm whose value exists, preferring void arms
                let mut best: Option<(usize, Value)> = None;
                for a in &u.arms {
                    let Some(c) = a.cases.first() else { continue };
                    let arm = match &a.ty {
                        None => None,
                        Some(t) => match self.smallest_type(t) {
                            Some(v) => Some(Box::new(v)),
                            None => continue,
                        },
                    };
                    let v = Value::Union {
                        discriminant: c.value,
                        arm,
                    };
                    let size = self.size(d.name(), &v);
                    if best.as_ref().is_none_or(|(s, _)| size < *s) {
                        best = Some((size, v));
                    }
                }
                best.map(|(_, v)| v)
            }
            Definition::Const(_) => None,
        }
    }

    fn size(&self, name: &str, v: &Value) -> usize {
        self.interp.encode(name, v).map_or(usize::MAX, |b| b.len())
    }

    fn encode(&self, name: &str, v: &Value) -> Result<Vec<u8>, VectorError> {
        self.interp
            .encode(name, v)
            .map_err(|e| VectorError::Encode(name.to_string(), e.to_string()))
    }

    /// The vectors of one definition: its own decision (arms, members, the discriminant word)
    /// and the decisions in its direct components (optional flags, lengths, bounds, padding,
    /// bools), each inside the definition's smallest value.
    pub fn vectors(&self, name: &str) -> Result<Vec<Vector>, VectorError> {
        let def = self
            .schema
            .get(name)
            .ok_or_else(|| VectorError::Infinite(name.to_string()))?;
        let base = self
            .smallest
            .get(name)
            .ok_or_else(|| VectorError::Infinite(name.to_string()))?
            .clone();
        let ty = rust_type_name(name);
        let mut out = Vec::new();
        let mut push =
            |point: String, choice: String, bytes: Vec<u8>, expect: Expect, note: String| {
                out.push(Vector {
                    obligation: format!("format:{ty}{point}:{choice}"),
                    type_name: ty.clone(),
                    bytes,
                    expect,
                    point,
                    note,
                });
            };
        let base_bytes = self.encode(name, &base)?;
        match def {
            Definition::Enum(e) => {
                for m in &e.members {
                    let v = Value::Enum(m.value as i32);
                    let n = member_name(&m.name, &e.member_prefix);
                    push(
                        String::new(),
                        format!("={n}"),
                        self.encode(name, &v)?,
                        Expect::MustAccept,
                        format!("member {} ({})", m.name, m.value),
                    );
                }
                let bad = absent_word(e.members.iter().map(|m| m.value));
                push(
                    String::new(),
                    "=undefined".into(),
                    (bad as i32).to_be_bytes().to_vec(),
                    Expect::MustReject,
                    format!("word {bad} names no member"),
                );
            }
            Definition::Union(u) => {
                for a in &u.arms {
                    for c in &a.cases {
                        let arm = match &a.ty {
                            None => None,
                            Some(t) => match self.smallest_type(t) {
                                Some(v) => Some(Box::new(v)),
                                None => continue,
                            },
                        };
                        let v = Value::Union {
                            discriminant: c.value,
                            arm,
                        };
                        let n = case_name(self.schema, u, c);
                        push(
                            String::new(),
                            format!(":{n}"),
                            self.encode(name, &v)?,
                            Expect::MustAccept,
                            format!("arm for discriminant {}", c.value),
                        );
                    }
                }
                // a discriminant word no arm takes: for an enum discriminant, prefer a member
                // without an arm, which tells "unknown member" apart from "no arm"
                let arms: Vec<i64> = u.case_values().collect();
                let members: Vec<i64> = match &u.discriminant.ty {
                    Type::Ref { name } => match self.schema.get(name) {
                        Some(Definition::Enum(e)) => e.members.iter().map(|m| m.value).collect(),
                        _ => Vec::new(),
                    },
                    _ => Vec::new(),
                };
                if let Some(m) = members.iter().find(|m| !arms.contains(m)) {
                    push(
                        String::new(),
                        "#member-without-arm".into(),
                        discriminant_bytes(*m),
                        Expect::MustReject,
                        format!("discriminant {m} is a member with no arm"),
                    );
                }
                let bad = absent_word(arms.iter().copied().chain(members.iter().copied()));
                push(
                    String::new(),
                    "#undefined-discriminant".into(),
                    discriminant_bytes(bad),
                    Expect::MustReject,
                    format!("discriminant {bad} names nothing"),
                );
                // decisions in an arm's own inline type (an optional, a length, a bool), inside
                // the union taking that arm
                for a in &u.arms {
                    let (Some(ty), Some(c)) = (&a.ty, a.cases.first()) else {
                        continue;
                    };
                    if matches!(ty, Type::Ref { .. }) {
                        continue;
                    }
                    let n = case_name(self.schema, u, c);
                    let head = discriminant_bytes(c.value);
                    for (choice, val, expect, note) in self.component_choices(ty) {
                        let bytes = match val {
                            Choice::Value(v) => self.encode(
                                name,
                                &Value::Union {
                                    discriminant: c.value,
                                    arm: Some(Box::new(v)),
                                },
                            )?,
                            Choice::Patch(p) => [head.clone(), p].concat(),
                        };
                        push(format!(":{n}"), choice, bytes, expect, note);
                    }
                }
            }
            Definition::Struct(s) => {
                let Value::Struct(fields) = &base else {
                    unreachable!("a struct's value")
                };
                for (i, f) in s.fields.iter().enumerate() {
                    let key = format!(".{}", field_name(&f.name));
                    for (choice, val, expect, note) in self.component_choices(&f.ty) {
                        let mut fs = fields.clone();
                        match val {
                            Choice::Value(v) => {
                                fs[i] = v;
                                push(
                                    key.clone(),
                                    choice,
                                    self.encode(name, &Value::Struct(fs))?,
                                    expect,
                                    note,
                                );
                            }
                            Choice::Patch(patch) => {
                                // splice raw bytes in place of the field's smallest encoding
                                let mut prefix = Vec::new();
                                for (pf, pv) in s.fields.iter().zip(fields).take(i) {
                                    prefix.extend(self.encode_type(&pf.ty, pv)?);
                                }
                                let field_len = self.encode_type(&f.ty, &fields[i])?.len();
                                let mut bytes = prefix.clone();
                                bytes.extend_from_slice(&patch);
                                bytes.extend_from_slice(&base_bytes[prefix.len() + field_len..]);
                                push(key.clone(), choice, bytes, expect, note);
                            }
                        }
                    }
                }
            }
            Definition::Typedef(t) => {
                for (choice, val, expect, note) in self.component_choices(&t.ty) {
                    let bytes = match val {
                        Choice::Value(v) => self.encode(name, &v)?,
                        Choice::Patch(p) => p,
                    };
                    push(String::new(), choice, bytes, expect, note);
                }
            }
            Definition::Const(_) => {}
        }
        // the whole value cut short and with a word left over
        if !base_bytes.is_empty() {
            push(
                String::new(),
                "#truncated".into(),
                base_bytes[..base_bytes.len() - 1].to_vec(),
                Expect::MustReject,
                "the smallest value without its last byte".into(),
            );
        }
        let mut trailing = base_bytes.clone();
        trailing.extend_from_slice(&[0; 4]);
        push(
            String::new(),
            "#trailing".into(),
            trailing,
            Expect::MustReject,
            "the smallest value and one more zero word".into(),
        );

        for v in &out {
            self.confirm(name, v)?;
        }
        Ok(out)
    }

    fn encode_type(&self, ty: &Type, v: &Value) -> Result<Vec<u8>, VectorError> {
        self.interp
            .encode_as(ty, v)
            .map_err(|e| VectorError::Encode(format!("{ty:?}"), e.to_string()))
    }

    /// The decision points of a component written inline: its own word or length, never the
    /// decisions of a definition it names (those vectors belong to that definition).
    fn component_choices(&self, ty: &Type) -> Vec<(String, Choice, Expect, String)> {
        let mut out = Vec::new();
        match ty {
            Type::Bool => {
                out.push((
                    "#true".into(),
                    Choice::Value(Value::Bool(true)),
                    Expect::MustAccept,
                    "bool word 1".into(),
                ));
                out.push((
                    "#bool-2".into(),
                    Choice::Patch(2u32.to_be_bytes().to_vec()),
                    Expect::ImplementationDefined,
                    "bool word 2, which RFC 4506 does not define".into(),
                ));
            }
            Type::Optional { element } => {
                if let Some(v) = self.smallest_type(element) {
                    out.push((
                        "#some".into(),
                        Choice::Value(Value::Optional(Some(Box::new(v)))),
                        Expect::MustAccept,
                        "optional present".into(),
                    ));
                }
                out.push((
                    "#flag-2".into(),
                    Choice::Patch(2u32.to_be_bytes().to_vec()),
                    Expect::MustReject,
                    "optional flag 2".into(),
                ));
            }
            Type::OpaqueFixed { size } if size % 4 != 0 => {
                let mut p = vec![0u8; *size as usize];
                p.resize(size.div_ceil(4) as usize * 4, 0);
                *p.last_mut().expect("padding exists") = 1;
                out.push((
                    "#nonzero-padding".into(),
                    Choice::Patch(p),
                    Expect::MustReject,
                    "a padding byte of 1".into(),
                ));
            }
            Type::OpaqueVar { max_size } | Type::String { max_size } => {
                let is_string = matches!(ty, Type::String { .. });
                let wrap = |b: Vec<u8>| {
                    if is_string {
                        Value::String(b)
                    } else {
                        Value::Opaque(b)
                    }
                };
                out.push((
                    "#one".into(),
                    Choice::Value(wrap(vec![0x61])),
                    Expect::MustAccept,
                    "length 1".into(),
                ));
                if let Some(max) = max_size {
                    out.push((
                        "#max".into(),
                        Choice::Value(wrap(vec![0x61; *max as usize])),
                        Expect::MustAccept,
                        format!("length {max}, the bound"),
                    ));
                    let mut p = (*max as u32 + 1).to_be_bytes().to_vec();
                    p.extend(vec![0x61; (*max as usize + 1).div_ceil(4) * 4]);
                    out.push((
                        "#over-max".into(),
                        Choice::Patch(p),
                        Expect::MustReject,
                        format!("length {}, one past the bound", max + 1),
                    ));
                }
                let mut p = 1u32.to_be_bytes().to_vec();
                p.extend([0x61, 0, 0, 1]);
                out.push((
                    "#nonzero-padding".into(),
                    Choice::Patch(p),
                    Expect::MustReject,
                    "length 1 with a padding byte of 1".into(),
                ));
                out.push((
                    "#past-end".into(),
                    Choice::Patch(0x7fff_fff0u32.to_be_bytes().to_vec()),
                    Expect::MustReject,
                    "a length longer than the input".into(),
                ));
            }
            Type::VarArray { element, max_count } => {
                if let Some(e) = self.smallest_type(element) {
                    out.push((
                        "#one".into(),
                        Choice::Value(Value::Array(vec![e.clone()])),
                        Expect::MustAccept,
                        "one element".into(),
                    ));
                    if let Some(max) = max_count
                        && *max <= 256
                    {
                        out.push((
                            "#max".into(),
                            Choice::Value(Value::Array(vec![e.clone(); *max as usize])),
                            Expect::MustAccept,
                            format!("{max} elements, the bound"),
                        ));
                        let mut p = (*max as u32 + 1).to_be_bytes().to_vec();
                        if let Ok(eb) = self.encode_type(element, &e) {
                            for _ in 0..=*max {
                                p.extend_from_slice(&eb);
                            }
                            out.push((
                                "#over-max".into(),
                                Choice::Patch(p),
                                Expect::MustReject,
                                format!("{} elements, one past the bound", max + 1),
                            ));
                        }
                    }
                }
                out.push((
                    "#past-end".into(),
                    Choice::Patch(0x7fff_fff0u32.to_be_bytes().to_vec()),
                    Expect::MustReject,
                    "a count longer than the input".into(),
                ));
            }
            _ => {}
        }
        out
    }

    fn confirm(&self, name: &str, v: &Vector) -> Result<(), VectorError> {
        let got = self.interp.decode(name, &v.bytes);
        let ok = match (&got, v.expect) {
            (Ok(_), Expect::MustAccept) => true,
            (Err(_), Expect::MustReject) => true,
            (Err(e), Expect::ImplementationDefined) => matches!(e.kind, DecodeErrorKind::Bool(2)),
            _ => false,
        };
        if ok {
            Ok(())
        } else {
            Err(VectorError::Verdict {
                obligation: v.obligation.clone(),
                got: match got {
                    Ok(_) => "accept".into(),
                    Err(e) => e.to_string(),
                },
                want: v.expect,
            })
        }
    }
}

enum Choice {
    Value(Value),
    Patch(Vec<u8>),
}

fn absent_word(taken: impl Iterator<Item = i64>) -> i64 {
    let taken: Vec<i64> = taken.collect();
    (0..)
        .find(|w| !taken.contains(w))
        .expect("some word is free")
}

fn discriminant_bytes(word: i64) -> Vec<u8> {
    (word as i32).to_be_bytes().to_vec()
}
