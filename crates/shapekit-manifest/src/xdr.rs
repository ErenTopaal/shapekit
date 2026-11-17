//! Decode, re-encode and XDR-JSON rendering on top of `stellar-xdr`.

use std::io::Cursor;
use std::str::FromStr;

use serde_json::Value;
use stellar_xdr::{Limited, Limits, Type, TypeVariant, WriteXdr};

use crate::ToolkitError;

/// Parses an XDR type name as `stellar-xdr` spells it, for example `LedgerCloseMeta`.
pub fn type_variant(type_name: &str) -> Result<TypeVariant, ToolkitError> {
    TypeVariant::from_str(type_name).map_err(|_| ToolkitError::UnknownType(type_name.to_string()))
}

/// The limits to hand `Type`'s own read and write so that `limits.depth` counts levels the
/// way a direct `<type>::read_xdr` and `<type>::write_xdr` do, which is how core and the host
/// decode.
///
/// Every arm of `Type::read_xdr` is `r.with_limited_depth(|r| <type>::read_xdr(r))`, and every
/// arm of `Type::write_xdr` writes the boxed value through `Box<T>::write_xdr`, which is
/// `w.with_limited_depth(|w| T::write_xdr(self, w))` (stellar-xdr 30.0.0,
/// src/generated/type_enum.rs and src/generated.rs). Either way the dispatch costs exactly one
/// level that a direct call does not. Core reads the type directly
/// (`TransactionEnvelope::read_xdr` in `can_parse_transaction`), so that level is handed back.
fn through_type(limits: Limits) -> Limits {
    Limits {
        depth: limits.depth.saturating_add(1),
        len: limits.len,
    }
}

/// Decodes exactly one value of `type_name`, rejecting trailing bytes, at the depth a direct
/// `<type>::read_xdr` would be given.
pub fn decode(type_name: &str, bytes: &[u8], limits: Limits) -> Result<Type, ToolkitError> {
    let variant = type_variant(type_name)?;
    let mut reader = Limited::new(Cursor::new(bytes), through_type(limits));
    Type::read_xdr_to_end(variant, &mut reader).map_err(ToolkitError::Decode)
}

/// Decodes under `limits`, re-encodes under the same limits and requires the bytes to be equal.
pub fn roundtrip_check(type_name: &str, bytes: &[u8], limits: Limits) -> Result<(), ToolkitError> {
    let value = decode(type_name, bytes, limits.clone())?;
    let again = value
        .to_xdr(through_type(limits))
        .map_err(ToolkitError::Encode)?;
    if again == bytes {
        return Ok(());
    }
    let first_difference = bytes
        .iter()
        .zip(&again)
        .position(|(a, b)| a != b)
        .unwrap_or(bytes.len().min(again.len()));
    Err(ToolkitError::RoundtripMismatch {
        original_len: bytes.len(),
        reencoded_len: again.len(),
        first_difference,
    })
}

/// The JSON value the crate's serde support writes for a decoded value. It is SEP-51 except
/// for fixed opaque fields and bare 64-bit values, which [`to_xdr_json`] puts right through
/// [`crate::sep51::normalize`].
pub fn json_value(value: &Type) -> Result<Value, ToolkitError> {
    serde_json::to_value(value).map_err(ToolkitError::Json)
}

/// SEP-51 XDR-JSON of one value: object keys sorted, two-space indent, trailing newline.
/// Sorting keeps the text independent of struct field order and of serde_json's map backend.
pub fn to_xdr_json(type_name: &str, bytes: &[u8], limits: Limits) -> Result<String, ToolkitError> {
    let value = decode(type_name, bytes, limits)?;
    let mut json = json_value(&value)?;
    crate::sep51::normalize(type_name, &mut json)?;
    Ok(render_json(&json))
}

/// Renders a JSON value the way every corpus JSON file is written.
pub fn render_json(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(&sorted(value))
        .expect("serializing a serde_json value cannot fail");
    text.push('\n');
    text
}

fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                out.insert(k.clone(), sorted(&map[k]));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

/// The XDR-JSON arm name of a transaction result, `tx_malformed` or `tx_failed` for example:
/// the bare string of a void arm, the single key of an arm with a value.
pub fn result_code(result: &stellar_xdr::TransactionResult) -> Result<String, ToolkitError> {
    match serde_json::to_value(&result.result).map_err(ToolkitError::Json)? {
        Value::String(s) => Ok(s),
        Value::Object(map) if map.len() == 1 => Ok(map.keys().next().cloned().unwrap_or_default()),
        other => Err(ToolkitError::Shape {
            path: "TransactionResult.result".into(),
            message: format!("expected a union arm, got {other}"),
        }),
    }
}
