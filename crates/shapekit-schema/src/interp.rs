//! A reference XDR codec that reads the schema as data. It shares no code with any SDK: every
//! decision is RFC 4506 applied to the definitions in [`Schema`], strictly. A bool is the enum
//! `{FALSE = 0, TRUE = 1}`, so any other word is refused; an enum word must name a member; a
//! union discriminant must select an arm; padding must be zero; a length must fit its bound and
//! the bytes left; and the input must be consumed exactly.

use crate::{Definition, Schema, Type};

/// A decoded value. Unions keep their discriminant word and the arm's value, enums their word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Int(i32),
    UnsignedInt(u32),
    Hyper(i64),
    UnsignedHyper(u64),
    /// The IEEE-754 bits; XDR float and double are carried, never interpreted.
    Float(u32),
    Double(u64),
    Bool(bool),
    Opaque(Vec<u8>),
    String(Vec<u8>),
    Array(Vec<Value>),
    Optional(Option<Box<Value>>),
    Struct(Vec<Value>),
    Union {
        discriminant: i64,
        arm: Option<Box<Value>>,
    },
    Enum(i32),
}

/// Why a decode stopped, and the byte offset it stopped at.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{kind} at byte {offset}")]
pub struct DecodeError {
    pub kind: DecodeErrorKind,
    pub offset: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DecodeErrorKind {
    #[error("the input ends early")]
    Truncated,
    #[error("{0} bytes are left after the value")]
    Trailing(usize),
    #[error("bool word {0} is neither 0 nor 1")]
    Bool(u32),
    #[error("optional flag {0} is neither 0 nor 1")]
    OptionalFlag(u32),
    #[error("{ty} has no member {value}")]
    EnumValue { ty: String, value: i32 },
    #[error("{ty} has no arm for discriminant {value}")]
    UnionArm { ty: String, value: i64 },
    #[error("length {len} is above the bound {max}")]
    Bound { len: u64, max: u64 },
    #[error("length {len} is more than the {left} bytes left")]
    Length { len: u64, left: usize },
    #[error("padding byte {0:#04x} is not zero")]
    Padding(u8),
    #[error("nesting is deeper than the limit {0}")]
    Depth(u32),
    #[error("{0} is not defined")]
    Undefined(String),
}

/// Why an encode failed: the value does not fit the type it is written as.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the value does not fit {0}")]
pub struct EncodeError(pub String);

/// The reference codec over one schema.
pub struct Interp<'a> {
    schema: &'a Schema,
    /// Most nested types a decode enters (definitions, arrays and optionals each count one).
    pub depth_limit: u32,
}

struct Reader<'b> {
    bytes: &'b [u8],
    at: usize,
    depth: u32,
}

impl Reader<'_> {
    fn fail(&self, kind: DecodeErrorKind) -> DecodeError {
        DecodeError {
            kind,
            offset: self.at,
        }
    }

    fn take(&mut self, n: usize) -> Result<&[u8], DecodeError> {
        if self.bytes.len() - self.at < n {
            return Err(self.fail(DecodeErrorKind::Truncated));
        }
        let s = &self.bytes[self.at..self.at + n];
        self.at += n;
        Ok(s)
    }

    fn word(&mut self) -> Result<u32, DecodeError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn hyper(&mut self) -> Result<u64, DecodeError> {
        let b = self.take(8)?;
        let mut a = [0; 8];
        a.copy_from_slice(b);
        Ok(u64::from_be_bytes(a))
    }

    /// `n` bytes, then the zero padding to the next multiple of four.
    fn padded(&mut self, n: usize) -> Result<Vec<u8>, DecodeError> {
        let out = self.take(n)?.to_vec();
        let pad = (4 - n % 4) % 4;
        let start = self.at;
        let p = self.take(pad)?;
        if let Some((i, &b)) = p.iter().enumerate().find(|(_, b)| **b != 0) {
            return Err(DecodeError {
                kind: DecodeErrorKind::Padding(b),
                offset: start + i,
            });
        }
        Ok(out)
    }

    fn length(&mut self, max: Option<u64>, unit: usize) -> Result<usize, DecodeError> {
        let start = self.at;
        let len = u64::from(self.word()?);
        if let Some(max) = max
            && len > max
        {
            return Err(DecodeError {
                kind: DecodeErrorKind::Bound { len, max },
                offset: start,
            });
        }
        // every element takes at least `unit` bytes, so a longer count cannot be satisfied
        let left = self.bytes.len() - self.at;
        if len.saturating_mul(unit as u64) > left as u64 {
            return Err(DecodeError {
                kind: DecodeErrorKind::Length { len, left },
                offset: start,
            });
        }
        Ok(len as usize)
    }
}

impl<'a> Interp<'a> {
    pub fn new(schema: &'a Schema) -> Interp<'a> {
        Interp {
            schema,
            depth_limit: 1000,
        }
    }

    /// Decodes exactly one value of the named type from all of `bytes`.
    pub fn decode(&self, type_name: &str, bytes: &[u8]) -> Result<Value, DecodeError> {
        let mut r = Reader {
            bytes,
            at: 0,
            depth: 0,
        };
        let ty = Type::Ref {
            name: type_name.to_string(),
        };
        let v = self.read(&ty, &mut r)?;
        if r.at != bytes.len() {
            return Err(r.fail(DecodeErrorKind::Trailing(bytes.len() - r.at)));
        }
        Ok(v)
    }

    fn nested<T>(
        &self,
        r: &mut Reader<'_>,
        f: impl FnOnce(&mut Reader<'_>) -> Result<T, DecodeError>,
    ) -> Result<T, DecodeError> {
        if r.depth >= self.depth_limit {
            return Err(r.fail(DecodeErrorKind::Depth(self.depth_limit)));
        }
        r.depth += 1;
        let out = f(r);
        r.depth -= 1;
        out
    }

    /// The smallest encoding of a value of `ty`, used to refuse counts no input can satisfy.
    fn min_size(&self, ty: &Type) -> usize {
        match self.schema.resolve(ty) {
            Type::Hyper | Type::UnsignedHyper | Type::Double => 8,
            Type::OpaqueFixed { size } => (*size as usize).div_ceil(4) * 4,
            Type::Array { element, count } => self.min_size(element) * *count as usize,
            Type::Ref { name } => match self.schema.get(name) {
                Some(Definition::Struct(s)) => {
                    s.fields.iter().map(|f| self.min_size_shallow(&f.ty)).sum()
                }
                _ => 4,
            },
            _ => 4,
        }
    }

    /// One level of [`Self::min_size`] for struct fields, so recursive types terminate.
    fn min_size_shallow(&self, ty: &Type) -> usize {
        match self.schema.resolve(ty) {
            Type::Hyper | Type::UnsignedHyper | Type::Double => 8,
            Type::OpaqueFixed { size } => (*size as usize).div_ceil(4) * 4,
            _ => 4,
        }
    }

    fn read(&self, ty: &Type, r: &mut Reader<'_>) -> Result<Value, DecodeError> {
        Ok(match ty {
            Type::Int => Value::Int(r.word()? as i32),
            Type::UnsignedInt => Value::UnsignedInt(r.word()?),
            Type::Hyper => Value::Hyper(r.hyper()? as i64),
            Type::UnsignedHyper => Value::UnsignedHyper(r.hyper()?),
            Type::Float => Value::Float(r.word()?),
            Type::Double => Value::Double(r.hyper()?),
            Type::Bool => {
                let start = r.at;
                match r.word()? {
                    0 => Value::Bool(false),
                    1 => Value::Bool(true),
                    w => {
                        return Err(DecodeError {
                            kind: DecodeErrorKind::Bool(w),
                            offset: start,
                        });
                    }
                }
            }
            Type::OpaqueFixed { size } => Value::Opaque(r.padded(*size as usize)?),
            Type::OpaqueVar { max_size } => {
                let n = r.length(*max_size, 1)?;
                Value::Opaque(r.padded(n)?)
            }
            Type::String { max_size } => {
                let n = r.length(*max_size, 1)?;
                Value::String(r.padded(n)?)
            }
            Type::Array { element, count } => self.nested(r, |r| {
                (0..*count)
                    .map(|_| self.read(element, r))
                    .collect::<Result<Vec<_>, _>>()
                    .map(Value::Array)
            })?,
            Type::VarArray { element, max_count } => self.nested(r, |r| {
                let n = r.length(*max_count, self.min_size(element))?;
                (0..n)
                    .map(|_| self.read(element, r))
                    .collect::<Result<Vec<_>, _>>()
                    .map(Value::Array)
            })?,
            Type::Optional { element } => self.nested(r, |r| {
                let start = r.at;
                match r.word()? {
                    0 => Ok(Value::Optional(None)),
                    1 => Ok(Value::Optional(Some(Box::new(self.read(element, r)?)))),
                    w => Err(DecodeError {
                        kind: DecodeErrorKind::OptionalFlag(w),
                        offset: start,
                    }),
                }
            })?,
            Type::Ref { name } => self.nested(r, |r| self.read_definition(name, r))?,
        })
    }

    fn read_definition(&self, name: &str, r: &mut Reader<'_>) -> Result<Value, DecodeError> {
        let Some(def) = self.schema.get(name) else {
            return Err(r.fail(DecodeErrorKind::Undefined(name.to_string())));
        };
        match def {
            Definition::Typedef(t) => self.read(&t.ty, r),
            Definition::Struct(s) => s
                .fields
                .iter()
                .map(|f| self.read(&f.ty, r))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Struct),
            Definition::Enum(e) => {
                let start = r.at;
                let v = r.word()? as i32;
                if !e.has_value(i64::from(v)) {
                    return Err(DecodeError {
                        kind: DecodeErrorKind::EnumValue {
                            ty: name.to_string(),
                            value: v,
                        },
                        offset: start,
                    });
                }
                Ok(Value::Enum(v))
            }
            Definition::Union(u) => {
                let start = r.at;
                let d = self.read(&u.discriminant.ty, r)?;
                let value = discriminant_word(&d);
                let Some(arm) = u.arm(value) else {
                    return Err(DecodeError {
                        kind: DecodeErrorKind::UnionArm {
                            ty: name.to_string(),
                            value,
                        },
                        offset: start,
                    });
                };
                let arm = match &arm.ty {
                    Some(t) => Some(Box::new(self.read(t, r)?)),
                    None => None,
                };
                Ok(Value::Union {
                    discriminant: value,
                    arm,
                })
            }
            Definition::Const(_) => Err(r.fail(DecodeErrorKind::Undefined(name.to_string()))),
        }
    }

    /// Encodes a value as the named type.
    pub fn encode(&self, type_name: &str, v: &Value) -> Result<Vec<u8>, EncodeError> {
        let mut out = Vec::new();
        self.write(
            &Type::Ref {
                name: type_name.to_string(),
            },
            v,
            &mut out,
        )?;
        Ok(out)
    }

    /// Encodes a value as a type written inline, such as a struct field's.
    pub fn encode_as(&self, ty: &Type, v: &Value) -> Result<Vec<u8>, EncodeError> {
        let mut out = Vec::new();
        self.write(ty, v, &mut out)?;
        Ok(out)
    }

    fn write(&self, ty: &Type, v: &Value, out: &mut Vec<u8>) -> Result<(), EncodeError> {
        let misfit = || EncodeError(format!("{ty:?}"));
        let padded = |b: &[u8], out: &mut Vec<u8>| {
            out.extend_from_slice(b);
            out.resize(out.len() + (4 - b.len() % 4) % 4, 0);
        };
        let length = |n: usize, max: Option<u64>, out: &mut Vec<u8>| {
            let n = u32::try_from(n).map_err(|_| misfit())?;
            if max.is_some_and(|m| u64::from(n) > m) {
                return Err(misfit());
            }
            out.extend_from_slice(&n.to_be_bytes());
            Ok(())
        };
        match (ty, v) {
            (Type::Int, Value::Int(i)) => out.extend_from_slice(&i.to_be_bytes()),
            (Type::UnsignedInt, Value::UnsignedInt(i)) => out.extend_from_slice(&i.to_be_bytes()),
            (Type::Hyper, Value::Hyper(i)) => out.extend_from_slice(&i.to_be_bytes()),
            (Type::UnsignedHyper, Value::UnsignedHyper(i)) => {
                out.extend_from_slice(&i.to_be_bytes())
            }
            (Type::Float, Value::Float(b)) => out.extend_from_slice(&b.to_be_bytes()),
            (Type::Double, Value::Double(b)) => out.extend_from_slice(&b.to_be_bytes()),
            (Type::Bool, Value::Bool(b)) => out.extend_from_slice(&u32::from(*b).to_be_bytes()),
            (Type::OpaqueFixed { size }, Value::Opaque(b)) => {
                if b.len() as u64 != *size {
                    return Err(misfit());
                }
                padded(b, out);
            }
            (Type::OpaqueVar { max_size }, Value::Opaque(b))
            | (Type::String { max_size }, Value::String(b)) => {
                length(b.len(), *max_size, out)?;
                padded(b, out);
            }
            (Type::Array { element, count }, Value::Array(items)) => {
                if items.len() as u64 != *count {
                    return Err(misfit());
                }
                for i in items {
                    self.write(element, i, out)?;
                }
            }
            (Type::VarArray { element, max_count }, Value::Array(items)) => {
                length(items.len(), *max_count, out)?;
                for i in items {
                    self.write(element, i, out)?;
                }
            }
            (Type::Optional { element }, Value::Optional(o)) => match o {
                None => out.extend_from_slice(&0u32.to_be_bytes()),
                Some(inner) => {
                    out.extend_from_slice(&1u32.to_be_bytes());
                    self.write(element, inner, out)?;
                }
            },
            (Type::Ref { name }, v) => {
                let def = self.schema.get(name).ok_or_else(misfit)?;
                match (def, v) {
                    (Definition::Typedef(t), v) => self.write(&t.ty, v, out)?,
                    (Definition::Struct(s), Value::Struct(fields)) => {
                        if fields.len() != s.fields.len() {
                            return Err(misfit());
                        }
                        for (f, v) in s.fields.iter().zip(fields) {
                            self.write(&f.ty, v, out)?;
                        }
                    }
                    (Definition::Enum(e), Value::Enum(w)) => {
                        if !e.has_value(i64::from(*w)) {
                            return Err(misfit());
                        }
                        out.extend_from_slice(&w.to_be_bytes());
                    }
                    (Definition::Union(u), Value::Union { discriminant, arm }) => {
                        let a = u.arm(*discriminant).ok_or_else(misfit)?;
                        let d = discriminant_value(&u.discriminant.ty, self.schema, *discriminant)
                            .ok_or_else(misfit)?;
                        self.write(&u.discriminant.ty, &d, out)?;
                        match (&a.ty, arm) {
                            (None, None) => {}
                            (Some(t), Some(v)) => self.write(t, v, out)?,
                            _ => return Err(misfit()),
                        }
                    }
                    _ => return Err(misfit()),
                }
            }
            _ => return Err(misfit()),
        }
        Ok(())
    }
}

/// The word a decoded discriminant carries.
fn discriminant_word(v: &Value) -> i64 {
    match v {
        Value::Int(i) | Value::Enum(i) => i64::from(*i),
        Value::UnsignedInt(u) => i64::from(*u),
        Value::Bool(b) => i64::from(*b),
        _ => unreachable!("a discriminant is int, unsigned int, bool or an enum"),
    }
}

/// The discriminant value to write for a word, in the discriminant's own type.
fn discriminant_value(ty: &Type, schema: &Schema, word: i64) -> Option<Value> {
    match schema.resolve(ty) {
        Type::Int => i32::try_from(word).ok().map(Value::Int),
        Type::UnsignedInt => u32::try_from(word).ok().map(Value::UnsignedInt),
        Type::Bool => match word {
            0 => Some(Value::Bool(false)),
            1 => Some(Value::Bool(true)),
            _ => None,
        },
        Type::Ref { .. } => i32::try_from(word).ok().map(Value::Enum),
        _ => None,
    }
}
