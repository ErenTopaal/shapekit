//! The names the `stellar-xdr` crate and XDR-JSON (SEP-51) give to definitions, fields, union
//! arms and enum values, derived from the XDR names the same way the rs-stellar-xdr generator
//! derives them (`xdr-generator-rust/generator/src/naming.rs`): heck for type and field names,
//! the enum's member prefix stripped for arm and value names, and serde's `snake_case` rename
//! for the JSON spelling of a Rust variant.

use heck::{ToSnakeCase, ToUpperCamelCase};

use crate::{Case, Definition, Schema, Type, Union};

/// The Rust type name of a definition: `SCVal` is `ScVal`, the overlay `Error` is `SError`.
pub fn rust_type_name(xdr: &str) -> String {
    let escaped = match xdr {
        "type" => "type_",
        "Error" => "SError",
        other => other,
    };
    escaped.to_upper_camel_case()
}

/// The XDR-JSON key of a struct field: `extraSigners` is `extra_signers`.
pub fn field_name(xdr: &str) -> String {
    xdr.to_snake_case()
}

/// serde's `rename_all = "snake_case"` for a Rust variant name.
fn serde_snake(variant: &str) -> String {
    let mut out = String::new();
    for (i, c) in variant.char_indices() {
        if c.is_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The generator's prefix stripping: a member name that would start with a digit after the
/// prefix keeps the prefix's first character.
fn strip_prefix(name: &str, prefix: &str) -> String {
    if prefix.is_empty() || !name.starts_with(prefix) {
        return name.to_string();
    }
    let stripped = &name[prefix.len()..];
    if stripped.starts_with(|c: char| c.is_ascii_digit())
        && let Some(first) = prefix.chars().next()
    {
        return format!("{first}{stripped}");
    }
    stripped.to_string()
}

/// The XDR-JSON name of an enum member: the member prefix of its enum stripped, then the Rust
/// variant name in `snake_case` (`SIGNER_KEY_TYPE_ED25519_SIGNED_PAYLOAD` is
/// `ed25519_signed_payload`).
pub fn member_name(member: &str, member_prefix: &str) -> String {
    serde_snake(&strip_prefix(member, member_prefix).to_upper_camel_case())
}

/// The XDR-JSON name of a union case: the named case's enum member, or `v<n>` for a literal.
pub fn case_name(schema: &Schema, union: &Union, case: &Case) -> String {
    match &case.name {
        Some(name) => {
            let prefix = match &union.discriminant.ty {
                Type::Ref { name } => match schema.get(name) {
                    Some(Definition::Enum(e)) => e.member_prefix.as_str(),
                    _ => "",
                },
                _ => "",
            };
            member_name(name, prefix)
        }
        None => format!("v{}", case.value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_the_generator() {
        assert_eq!(rust_type_name("SCVal"), "ScVal");
        assert_eq!(rust_type_name("SCPQuorumSet"), "ScpQuorumSet");
        assert_eq!(rust_type_name("Error"), "SError");
        assert_eq!(rust_type_name("uint32"), "Uint32");
        assert_eq!(field_name("signerSponsoringIDs"), "signer_sponsoring_i_ds");
        assert_eq!(field_name("type"), "type");
        assert_eq!(
            member_name("SIGNER_KEY_TYPE_ED25519_SIGNED_PAYLOAD", "SIGNER_KEY_TYPE_"),
            "ed25519_signed_payload"
        );
        assert_eq!(
            member_name("txFEE_BUMP_INNER_SUCCESS", ""),
            "tx_fee_bump_inner_success"
        );
    }
}
