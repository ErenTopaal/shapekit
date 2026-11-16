//! Other values for one field, chosen from the field's schema type and its value in the base
//! transaction. Each is a single change: the rest of the transaction stays as it was.

use serde_json::Value;
use shapekit_schema::{Definition, Schema, Type};

/// One replacement value and how it reads in a report.
#[derive(Debug, Clone, PartialEq)]
pub struct Replacement {
    pub name: String,
    pub value: Value,
}

fn rep(name: impl Into<String>, value: Value) -> Replacement {
    Replacement {
        name: name.into(),
        value,
    }
}

/// XDR-JSON writes 64-bit integers as decimal strings and narrower ones as numbers.
fn int_of(v: &Value) -> Option<i128> {
    match v {
        Value::Number(n) => n.as_i64().map(i128::from).or(n.as_u64().map(i128::from)),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn ints(base: &Value, lo: i128, hi: i128, as_string: bool) -> Vec<Replacement> {
    let enc = |n: i128| {
        if as_string {
            Value::String(n.to_string())
        } else {
            serde_json::json!(n as i64)
        }
    };
    let mut out = Vec::new();
    let b = int_of(base);
    let mut tried = Vec::new();
    for (name, n) in [("zero", 0), ("minimum", lo), ("maximum", hi)] {
        if Some(n) != b && !tried.contains(&n) {
            tried.push(n);
            out.push(rep(format!("{name} {n}"), enc(n)));
        }
    }
    if let Some(b) = b {
        if b < hi {
            out.push(rep(format!("one more, {}", b + 1), enc(b + 1)));
        }
        if b > lo {
            out.push(rep(format!("one less, {}", b - 1), enc(b - 1)));
        }
    }
    out
}

/// The replacements for a value of type `ty` whose base value is `base`.
pub fn replacements(schema: &Schema, ty: &Type, base: &Value) -> Vec<Replacement> {
    match schema.resolve(ty) {
        Type::Int => ints(base, i32::MIN.into(), i32::MAX.into(), false),
        Type::UnsignedInt => ints(base, 0, u32::MAX.into(), false),
        Type::Hyper => ints(base, i64::MIN.into(), i64::MAX.into(), true),
        Type::UnsignedHyper => ints(base, 0, u64::MAX.into(), true),
        Type::Bool => match base {
            Value::Bool(b) => vec![rep((!b).to_string(), Value::Bool(!b))],
            _ => Vec::new(),
        },
        Type::OpaqueFixed { .. } => match base.as_str() {
            Some(h) if !h.is_empty() => {
                let mut bytes = hex::decode(h).unwrap_or_default();
                let mut out = Vec::new();
                if let Some(first) = bytes.first_mut() {
                    *first ^= 0x01;
                    out.push(rep(
                        "first byte changed",
                        Value::String(hex::encode(&bytes)),
                    ));
                }
                let zero = "00".repeat(h.len() / 2);
                if zero != h {
                    out.push(rep("all zero", Value::String(zero)));
                }
                out
            }
            _ => Vec::new(),
        },
        Type::OpaqueVar { max_size } | Type::String { max_size } => {
            let text = matches!(schema.resolve(ty), Type::String { .. });
            let mut out = Vec::new();
            let cur = base.as_str().unwrap_or("");
            if !cur.is_empty() {
                out.push(rep("empty", Value::String(String::new())));
            }
            let max = max_size.unwrap_or(64).min(1024) as usize;
            let full = if text {
                "a".repeat(max)
            } else {
                "ab".repeat(max)
            };
            if full != cur {
                out.push(rep(
                    format!("{max} bytes, the maximum"),
                    Value::String(full),
                ));
            }
            if text {
                out.push(rep("bytes outside ascii", Value::String("\u{e9}".into())));
            }
            out
        }
        Type::Optional { .. } => {
            if base.is_null() {
                Vec::new()
            } else {
                vec![rep("absent", Value::Null)]
            }
        }
        Type::VarArray { .. } => match base.as_array() {
            Some(a) if !a.is_empty() => {
                let mut twice = a.clone();
                twice.push(a[0].clone());
                vec![
                    rep("empty", Value::Array(Vec::new())),
                    rep("first element repeated", Value::Array(twice)),
                ]
            }
            _ => Vec::new(),
        },
        Type::Ref { name } => match schema.get(name) {
            Some(Definition::Enum(e)) => e
                .members
                .iter()
                .map(|m| snake(&m.name, &e.member_prefix))
                .filter(|m| base.as_str() != Some(m.as_str()))
                .map(|m| rep(m.clone(), Value::String(m)))
                .collect(),
            _ => Vec::new(),
        },
        Type::Array { .. } | Type::Float | Type::Double => Vec::new(),
    }
}

/// An enum member as XDR-JSON names it: the shared prefix dropped, snake case.
pub fn snake(member: &str, prefix: &str) -> String {
    member
        .strip_prefix(prefix)
        .unwrap_or(member)
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(r: &[Replacement]) -> Vec<&str> {
        r.iter().map(|x| x.name.as_str()).collect()
    }

    #[test]
    fn integers_move_to_their_bounds_and_neighbours() {
        let s = Schema::pinned();
        let r = replacements(&s, &Type::UnsignedInt, &serde_json::json!(100));
        assert_eq!(
            names(&r),
            [
                "zero 0",
                "maximum 4294967295",
                "one more, 101",
                "one less, 99"
            ]
        );
        let r = replacements(&s, &Type::Hyper, &serde_json::json!("5"));
        assert_eq!(r[0].value, serde_json::json!("0"));
    }

    #[test]
    fn enums_take_their_other_members() {
        let s = Schema::pinned();
        let ty = Type::Ref {
            name: "MemoType".into(),
        };
        let r = replacements(&s, &ty, &serde_json::json!("none"));
        assert_eq!(names(&r), ["text", "id", "hash", "return"]);
    }

    #[test]
    fn optional_values_can_be_dropped() {
        let s = Schema::pinned();
        let ty = Type::Optional {
            element: Box::new(Type::UnsignedInt),
        };
        assert_eq!(
            names(&replacements(&s, &ty, &serde_json::json!(3))),
            ["absent"]
        );
        assert!(replacements(&s, &ty, &serde_json::Value::Null).is_empty());
    }
}
