//! Stored values as the rules read them.

use std::collections::BTreeMap;

use serde_json::Value;

/// One stored cell.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Null,
    /// `wide` for 64-bit columns, rendered as decimal strings in unmapped rows so ids past
    /// 2^53 stay exact.
    Int {
        v: i64,
        wide: bool,
    },
    Float(f64),
    Bool(bool),
    Text(String),
    Bytes(Vec<u8>),
    Json(Value),
    /// A type the rules do not read, in the store's own text form.
    Other(String),
}

impl Val {
    pub fn int(&self) -> Option<i64> {
        match self {
            Val::Int { v, .. } => Some(*v),
            _ => None,
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Val::Text(s) => Some(s),
            _ => None,
        }
    }

    pub fn bytes(&self) -> Option<&[u8]> {
        match self {
            Val::Bytes(b) => Some(b),
            _ => None,
        }
    }

    /// The value as a condition compares it: text as it is, integers in decimal, booleans
    /// as `true` and `false`. NULL and bytes compare as nothing.
    pub fn condition_text(&self) -> Option<String> {
        match self {
            Val::Text(s) | Val::Other(s) => Some(s.clone()),
            Val::Int { v, .. } => Some(v.to_string()),
            Val::Bool(b) => Some(b.to_string()),
            _ => None,
        }
    }

    /// The value in an unmapped row: bytes as lowercase hex, 64-bit integers as decimal
    /// strings, JSON as stored.
    pub fn to_json(&self) -> Value {
        match self {
            Val::Null => Value::Null,
            Val::Int { v, wide: true } => Value::String(v.to_string()),
            Val::Int { v, wide: false } => Value::from(*v),
            Val::Float(f) => serde_json::Number::from_f64(*f)
                .map(Value::Number)
                .unwrap_or(Value::Null),
            Val::Bool(b) => Value::Bool(*b),
            Val::Text(s) | Val::Other(s) => Value::String(s.clone()),
            Val::Bytes(b) => Value::String(hex::encode(b)),
            Val::Json(j) => j.clone(),
        }
    }
}

/// One stored row by column name. A column the row lacks reads as NULL.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Row(pub BTreeMap<String, Val>);

static NULL: Val = Val::Null;

impl Row {
    pub fn get(&self, col: &str) -> &Val {
        self.0.get(col).unwrap_or(&NULL)
    }

    pub fn to_json(&self) -> serde_json::Map<String, Value> {
        self.0
            .iter()
            .map(|(k, v)| (k.clone(), v.to_json()))
            .collect()
    }
}
