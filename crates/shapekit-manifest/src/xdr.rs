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

/// Decodes exactly one value of `type_name`, rejecting trailing bytes.
pub fn decode(type_name: &str, bytes: &[u8], limits: Limits) -> Result<Type, ToolkitError> {
    let variant = type_variant(type_name)?;
    let mut reader = Limited::new(Cursor::new(bytes), limits);
    Type::read_xdr_to_end(variant, &mut reader).map_err(ToolkitError::Decode)
}

/// Decodes under `limits`, re-encodes under the same limits and requires the bytes to be equal.
pub fn roundtrip_check(type_name: &str, bytes: &[u8], limits: Limits) -> Result<(), ToolkitError> {
    let value = decode(type_name, bytes, limits.clone())?;
    let again = value.to_xdr(limits).map_err(ToolkitError::Encode)?;
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

/// The SEP-51 XDR-JSON value of a decoded value, through the crate's serde support.
pub fn json_value(value: &Type) -> Result<Value, ToolkitError> {
    serde_json::to_value(value).map_err(ToolkitError::Json)
}

/// SEP-51 XDR-JSON of one value: object keys sorted, two-space indent, trailing newline.
/// Sorting keeps the text independent of struct field order and of serde_json's map backend.
pub fn to_xdr_json(type_name: &str, bytes: &[u8], limits: Limits) -> Result<String, ToolkitError> {
    let value = decode(type_name, bytes, limits)?;
    let json = json_value(&value)?;
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
