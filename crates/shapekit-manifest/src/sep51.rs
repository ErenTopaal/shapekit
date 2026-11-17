//! The two places where `stellar-xdr`'s serde rendering departs from SEP-51 v2.0.1
//! (stellar-protocol `ecosystem/sep-0051.md` at commit ced008c3, pinned in pins.toml as
//! `stellar-protocol`), and the walk that puts them right before an expected file is written.
//!
//! - Fixed-length opaque data: "The XDR fixed-length opaque data type ([RFC 4506 Section 4.9])
//!   are represented as a hexadecimal string." (lines 204-207). stellar-xdr 30.0.0 writes a
//!   field declared `opaque name[N]` with no typedef as a plain `[u8; N]`, which serde writes as
//!   an array of numbers (`src/generated/curve25519_public.rs:24-26`, `Curve25519Public.key`).
//!   Named hash typedefs (`Hash`, `Uint256`) carry their own hex rendering and are not touched.
//! - 64-bit integers: hyper and unsigned hyper map "to JSON strings, with the number base10
//!   encoded as a simple integer" (lines 106-109 and 140-143). stellar-xdr writes struct fields
//!   of those types through `NumberOrString` (`src/generated/account_entry.rs:50-54`), but the
//!   bare typedefs `Int64`, `Uint64`, `TimePoint` and `Duration` are plain `i64`/`u64`
//!   (`src/generated/int64.rs:10`, `uint64.rs:10`), so a value of one of those types on its own,
//!   or an element of an array of them, is written as a JSON number.
//! - Enum names: an enum value "is represented in JSON with a string, derived from the
//!   name-identifier that corresponds to its value. The string is the name-identifier modified
//!   to be snake_case, and truncated removing any shared prefix if there are multiple
//!   identifiers in the enum." (lines 384-387). Where removing the prefix leaves a name that
//!   starts with a digit, stellar-xdr keeps the prefix's first character so that the Rust
//!   variant is an identifier (`BINARY_FUSE_FILTER_8_BIT` is `b8_bit`; rs-stellar-xdr's
//!   xdr-generator-rust naming, mirrored in `shapekit_schema::names`); SEP-51 has no such
//!   exception, so the value is `8_bit`. `BinaryFuseFilterType` is the only enum of the pinned
//!   schema this applies to, and no union switches on it.
//!
//! The walk follows the pinned schema alongside the JSON value and changes only a value whose
//! JSON shape is the plain serde shape of its schema type: an array of byte numbers where the
//! schema says fixed or variable opaque, a JSON number where it says hyper or unsigned hyper.
//! A value stellar-xdr renders its own way (a strkey, an asset code, a 128-bit integer string)
//! has another JSON shape than its schema type and is left as it is.

use serde_json::Value;
use shapekit_schema::names::{case_name, field_name, member_name, rust_type_name};
use shapekit_schema::{Definition, Schema, Type};

use crate::ToolkitError;

/// Rewrites `value`, the serde rendering of one value of the stellar-xdr type `type_name`,
/// into SEP-51's forms for fixed opaque data and 64-bit integers. Returns how many values it
/// changed.
pub fn normalize(type_name: &str, value: &mut Value) -> Result<usize, ToolkitError> {
    let schema = schema();
    let def = schema
        .definitions()
        .iter()
        .find(|d| !matches!(d, Definition::Const(_)) && rust_type_name(d.name()) == type_name)
        .ok_or_else(|| ToolkitError::UnknownType(type_name.to_string()))?;
    let mut changed = 0;
    walk_def(schema, def, value, &mut changed);
    Ok(changed)
}

fn schema() -> &'static Schema {
    static SCHEMA: std::sync::OnceLock<Schema> = std::sync::OnceLock::new();
    SCHEMA.get_or_init(Schema::pinned)
}

fn walk_def(schema: &Schema, def: &Definition, value: &mut Value, changed: &mut usize) {
    match def {
        Definition::Typedef(t) => walk_type(schema, &t.ty, value, changed),
        Definition::Struct(s) => {
            if let Value::Object(map) = value {
                for f in &s.fields {
                    if let Some(v) = map.get_mut(&field_name(&f.name)) {
                        walk_type(schema, &f.ty, v, changed);
                    }
                }
            }
        }
        Definition::Union(u) => {
            // a void arm is a bare string; an arm with a value is an object of one member
            let Value::Object(map) = value else {
                return;
            };
            if map.len() != 1 {
                return;
            }
            let Some((key, v)) = map.iter_mut().next() else {
                return;
            };
            let arm = u
                .arms
                .iter()
                .find(|a| a.cases.iter().any(|c| case_name(schema, u, c) == *key));
            if let Some(ty) = arm.and_then(|a| a.ty.as_ref()) {
                walk_type(schema, ty, v, changed);
            }
        }
        Definition::Enum(e) => {
            let Value::String(s) = value else {
                return;
            };
            for m in &e.members {
                let Some(stripped) = m.name.strip_prefix(e.member_prefix.as_str()) else {
                    continue;
                };
                if !e.member_prefix.is_empty()
                    && stripped.starts_with(|c: char| c.is_ascii_digit())
                    && *s == member_name(&m.name, &e.member_prefix)
                {
                    *value = Value::String(member_name(stripped, ""));
                    *changed += 1;
                    return;
                }
            }
        }
        Definition::Const(_) => {}
    }
}

fn walk_type(schema: &Schema, ty: &Type, value: &mut Value, changed: &mut usize) {
    match ty {
        Type::Hyper | Type::UnsignedHyper => {
            if let Value::Number(n) = value {
                *value = Value::String(n.to_string());
                *changed += 1;
            }
        }
        Type::OpaqueFixed { .. } | Type::OpaqueVar { .. } => {
            if let Some(bytes) = byte_array(value) {
                *value = Value::String(hex::encode(bytes));
                *changed += 1;
            }
        }
        Type::Ref { name } => {
            if let Some(def) = schema.get(name) {
                walk_def(schema, def, value, changed);
            }
        }
        Type::Array { element, .. } | Type::VarArray { element, .. } => {
            if let Value::Array(items) = value {
                for item in items {
                    walk_type(schema, element, item, changed);
                }
            }
        }
        Type::Optional { element } if !value.is_null() => {
            walk_type(schema, element, value, changed);
        }
        _ => {}
    }
}

/// The bytes of a JSON array whose every element is an integer from 0 to 255.
fn byte_array(value: &Value) -> Option<Vec<u8>> {
    let Value::Array(items) = value else {
        return None;
    };
    items
        .iter()
        .map(|i| i.as_u64().and_then(|n| u8::try_from(n).ok()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fixed_opaque_fields_become_hex() {
        let mut v = json!({ "key": [1, 2, 255] });
        assert_eq!(normalize("Curve25519Public", &mut v).unwrap(), 1);
        assert_eq!(v, json!({ "key": "0102ff" }));
    }

    #[test]
    fn bare_64_bit_values_become_strings() {
        for t in ["Int64", "Uint64", "TimePoint", "Duration"] {
            let mut v = json!(18446744073709551615u64);
            if t == "Int64" {
                v = json!(-9223372036854775808i64);
            }
            let text = v.to_string();
            assert_eq!(normalize(t, &mut v).unwrap(), 1, "{t}");
            assert_eq!(v, Value::String(text), "{t}");
        }
    }

    #[test]
    fn strings_and_custom_renderings_are_left_alone() {
        // already a string (NumberOrString), a strkey for a struct, a hash in hex
        let mut v = json!({ "balance": "5", "account_id": "GAAA", "seq_num": "7" });
        assert_eq!(normalize("AccountEntry", &mut v).unwrap(), 0);
        let mut h = json!("00ff");
        assert_eq!(normalize("Hash", &mut h).unwrap(), 0);
    }

    #[test]
    fn enum_names_keep_no_letter_of_the_removed_prefix() {
        let mut v = json!("b16_bit");
        assert_eq!(normalize("BinaryFuseFilterType", &mut v).unwrap(), 1);
        assert_eq!(v, json!("16_bit"));
        let mut other = json!("credit_alphanum4");
        assert_eq!(normalize("AssetType", &mut other).unwrap(), 0);
    }

    #[test]
    fn union_arms_and_nested_types_are_followed() {
        // PeerAddress.ip is a union whose ipv4 arm is opaque[4]
        let mut v = json!({ "ip": { "i_pv4": [127, 0, 0, 1] }, "port": 11625, "num_failures": 0 });
        assert_eq!(normalize("PeerAddress", &mut v).unwrap(), 1);
        assert_eq!(v["ip"], json!({ "i_pv4": "7f000001" }));
    }
}
