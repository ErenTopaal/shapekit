//! Signature fragments (the grammar of `registry/families.toml`, see
//! `crates/shapekit-manifest/src/signature.rs`) checked against a schema: can some value of
//! some type in this schema produce the fragment? This is what dates a fragment across the
//! schema history; the manifest crate's own checker answers the same question for the
//! pinned crate only.

use crate::names::{case_name, field_name, member_name, rust_type_name};
use crate::{Definition, Schema, Type};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Segment {
    Field(String),
    Arm(String),
    Value(String),
    Element,
    Bucket(Bucket),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bucket {
    Empty,
    One,
    Many,
    Max,
    None,
}

/// A parsed fragment: an optional anchor type (by its Rust name) and the segments after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fragment {
    pub anchor: Option<String>,
    pub segments: Vec<Segment>,
}

const DELIMITERS: [char; 5] = ['.', ':', '=', '[', '#'];

#[derive(Debug, thiserror::Error)]
#[error("fragment {fragment:?}: {reason}")]
pub struct ParseError {
    pub fragment: String,
    pub reason: String,
}

impl Fragment {
    pub fn parse(text: &str) -> Result<Fragment, ParseError> {
        let err = |reason: &str| ParseError {
            fragment: text.to_string(),
            reason: reason.to_string(),
        };
        let first = text.find(DELIMITERS).unwrap_or(text.len());
        let anchor = (first > 0).then(|| text[..first].to_string());
        if anchor.is_some() && first == text.len() {
            return Err(err("an anchored fragment needs at least one segment"));
        }
        let mut rest = &text[first..];
        let mut segments = Vec::new();
        while let Some(d) = rest.chars().next() {
            rest = &rest[1..];
            if d == '[' {
                rest = rest.strip_prefix(']').ok_or_else(|| err("[ without ]"))?;
                segments.push(Segment::Element);
                continue;
            }
            let end = rest.find(DELIMITERS).unwrap_or(rest.len());
            let name = &rest[..end];
            rest = &rest[end..];
            if name.is_empty() {
                return Err(err("empty segment name"));
            }
            segments.push(match d {
                '.' => Segment::Field(name.to_string()),
                ':' => Segment::Arm(name.to_string()),
                '=' => Segment::Value(name.to_string()),
                '#' => Segment::Bucket(match name {
                    "empty" => Bucket::Empty,
                    "one" => Bucket::One,
                    "many" => Bucket::Many,
                    "max" => Bucket::Max,
                    "none" => Bucket::None,
                    _ => return Err(err("unknown length bucket")),
                }),
                _ => unreachable!("find stops only at a delimiter"),
            });
        }
        Ok(Fragment { anchor, segments })
    }

    /// Whether some value of this schema produces the fragment: an anchored fragment from its
    /// anchor type, a suffix fragment from any type.
    pub fn producible(&self, schema: &Schema) -> bool {
        let starts: Vec<Type> = match &self.anchor {
            Some(a) => schema
                .definitions()
                .iter()
                .filter(|d| !matches!(d, Definition::Const(_)) && rust_type_name(d.name()) == *a)
                .map(|d| Type::Ref {
                    name: d.name().to_string(),
                })
                .collect(),
            None => every_type(schema),
        };
        starts.iter().any(|t| walk(schema, t, &self.segments))
    }
}

/// Every type node of the schema: each definition and each type written inside one.
fn every_type(schema: &Schema) -> Vec<Type> {
    fn nested(t: &Type, out: &mut Vec<Type>) {
        out.push(t.clone());
        if let Type::Array { element, .. }
        | Type::VarArray { element, .. }
        | Type::Optional { element } = t
        {
            nested(element, out);
        }
    }
    let mut out = Vec::new();
    for d in schema.definitions() {
        if matches!(d, Definition::Const(_)) {
            continue;
        }
        out.push(Type::Ref {
            name: d.name().to_string(),
        });
        for t in d.component_types() {
            nested(t, &mut out);
        }
    }
    out
}

/// The type behind typedefs and optionals: XDR-JSON writes a present optional as its value.
fn concrete<'a>(schema: &'a Schema, mut t: &'a Type) -> &'a Type {
    loop {
        t = schema.resolve(t);
        match t {
            Type::Optional { element } => t = element,
            _ => return t,
        }
    }
}

fn walk(schema: &Schema, t: &Type, segments: &[Segment]) -> bool {
    let Some((seg, rest)) = segments.split_first() else {
        return true;
    };
    if let Segment::Bucket(b) = seg {
        return rest.is_empty() && bucket(schema, t, *b);
    }
    let t = concrete(schema, t);
    match seg {
        Segment::Element => match t {
            Type::Array { element, .. } | Type::VarArray { element, .. } => {
                walk(schema, element, rest)
            }
            _ => false,
        },
        Segment::Field(name) => {
            let Type::Ref { name: def } = t else {
                return false;
            };
            let Some(Definition::Struct(s)) = schema.get(def) else {
                return false;
            };
            s.fields
                .iter()
                .filter(|f| field_name(&f.name) == *name)
                .any(|f| walk(schema, &f.ty, rest))
        }
        Segment::Arm(name) => {
            let Type::Ref { name: def } = t else {
                return false;
            };
            let Some(Definition::Union(u)) = schema.get(def) else {
                return false;
            };
            u.arms.iter().any(|arm| {
                arm.cases.iter().any(|c| case_name(schema, u, c) == *name)
                    && match &arm.ty {
                        Some(ty) => walk(schema, ty, rest),
                        None => rest.is_empty(),
                    }
            })
        }
        Segment::Value(name) => {
            let Type::Ref { name: def } = t else {
                return false;
            };
            let Some(Definition::Enum(e)) = schema.get(def) else {
                return false;
            };
            rest.is_empty()
                && e.members
                    .iter()
                    .any(|m| member_name(&m.name, &e.member_prefix) == *name)
        }
        Segment::Bucket(_) => unreachable!("handled above"),
    }
}

/// Whether a value of `t` can fall in the length bucket: `#none` needs an optional on the way,
/// the others a variable-length array, opaque or string, `#max` a declared bound.
fn bucket<'a>(schema: &'a Schema, mut t: &'a Type, b: Bucket) -> bool {
    loop {
        t = schema.resolve(t);
        match t {
            Type::Optional { element } => {
                if b == Bucket::None {
                    return true;
                }
                t = element;
            }
            Type::VarArray { max_count: max, .. }
            | Type::OpaqueVar { max_size: max }
            | Type::String { max_size: max } => {
                // as the manifest's checker: any length bucket fits a variable-length value,
                // `#max` only a bounded one
                return match b {
                    Bucket::Empty | Bucket::One | Bucket::Many => true,
                    Bucket::Max => max.is_some(),
                    Bucket::None => false,
                };
            }
            _ => return false,
        }
    }
}
