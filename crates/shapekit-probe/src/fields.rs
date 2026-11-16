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
}
