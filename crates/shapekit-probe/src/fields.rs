//! The envelope's fields as the schema defines them: every field of every struct a
//! `TransactionEnvelope` can reach, named `<Struct>.<field>` with the names of the `.x` files.

use anyhow::Result;
use shapekit_schema::{Definition, Schema, Type};

/// The root every probed field is reached from.
pub const ROOT: &str = "TransactionEnvelope";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// `<Struct>.<field>`, `PaymentOp.amount`.
    pub path: String,
    pub structure: String,
    pub name: String,
    pub ty: Type,
}

/// Every field reachable from [`ROOT`], sorted by path.
pub fn envelope_fields(schema: &Schema) -> Result<Vec<Field>> {
    let reach = schema.graph().reachable(ROOT)?;
    let mut out = Vec::new();
    for name in reach {
        if let Some(Definition::Struct(s)) = schema.get(name) {
            for f in &s.fields {
                out.push(Field {
                    path: format!("{}.{}", s.name, f.name),
                    structure: s.name.clone(),
                    name: f.name.clone(),
                    ty: f.ty.clone(),
                });
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Structs XDR-JSON writes as one string (a strkey or a decimal number), whose fields have no
/// key of their own.
pub const STRING_SHAPED: &[&str] = &[
    "Int128Parts",
    "Int256Parts",
    "MuxedAccountMed25519",
    "MuxedContract",
    "MuxedEd25519Account",
    "SignerKeyEd25519SignedPayload",
    "UInt128Parts",
    "UInt256Parts",
];

/// A `.x` field name as XDR-JSON spells it: `seqNum` is `seq_num`, `offerID` is `offer_id`,
/// `ed25519SignedPayload` is `ed25519_signed_payload`.
pub fn json_name(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 4);
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            let prev = chars[i - 1];
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_ascii_lowercase());
            if prev.is_ascii_lowercase()
                || prev.is_ascii_digit()
                || (prev.is_ascii_uppercase() && next_lower)
            {
                out.push('_');
            }
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// A `.x` type name as stellar-xdr's Rust types spell it: the same words as [`json_name`],
/// each capitalised, so `SCNonceKey` is `ScNonceKey` and `ExtendFootprintTTLOp` is
/// `ExtendFootprintTtlOp`.
pub fn rust_name(name: &str) -> String {
    json_name(name)
        .split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pinned_envelope_has_203_fields() {
        let fields = envelope_fields(&Schema::pinned()).unwrap();
        assert_eq!(fields.len(), 203);
        assert!(fields.iter().any(|f| f.path == "PaymentOp.amount"));
        assert!(fields.iter().any(|f| f.path == "Transaction.memo"));
    }

    #[test]
    fn json_names_follow_the_xdr_json_spelling() {
        for (x, j) in [
            ("seqNum", "seq_num"),
            ("offerID", "offer_id"),
            ("liquidityPoolID", "liquidity_pool_id"),
            ("sourceAccountEd25519", "source_account_ed25519"),
            ("ed25519SignedPayload", "ed25519_signed_payload"),
            ("fee", "fee"),
        ] {
            assert_eq!(json_name(x), j);
        }
    }

    /// Every field name maps to the key stellar-xdr's serde writes, checked on each struct's
    /// default value; the structs XDR-JSON writes as one string are listed by name.
    #[test]
    fn every_field_name_is_a_key_of_its_xdr_json_object() {
        use stellar_xdr::{Type as XdrType, TypeVariant};
        let schema = Schema::pinned();
        let mut strings = std::collections::BTreeSet::new();
        let mut missing = Vec::new();
        for f in envelope_fields(&schema).unwrap() {
            let v: TypeVariant = rust_name(&f.structure)
                .parse()
                .unwrap_or_else(|_| panic!("{} has no stellar-xdr type", f.structure));
            match serde_json::to_value(XdrType::default(v)).unwrap() {
                serde_json::Value::Object(m) => {
                    let key = json_name(&f.name);
                    if !m.contains_key(&key) {
                        missing.push(format!("{} -> {key}", f.path));
                    }
                }
                _ => {
                    strings.insert(f.structure.clone());
                }
            }
        }
        assert!(missing.is_empty(), "{missing:#?}");
        assert_eq!(
            strings.into_iter().collect::<Vec<_>>(),
            STRING_SHAPED,
            "structs written as one string"
        );
    }
}
