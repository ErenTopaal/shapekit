//! Structural shape signatures and the union-arm denominator, both driven by the JSON Schema
//! that `stellar-xdr` generates for a type.
//!
//! # Signature format
//!
//! A signature is a set of paths. Every path starts at the root type name and is built from
//! these segments, the same names XDR-JSON (SEP-51) uses:
//!
//! - `.name` a struct field, for example `TransactionEnvelope:tx.tx.fee`.
//! - `:arm` the arm a union takes, void arms included, for example `...memo:none`.
//! - `=value` the value of an XDR enum, for example `...type_=contract`.
//! - `[]` an element of an array; indices are collapsed, so all elements share one path.
//! - `#empty`, `#one`, `#many`, `#max` the length bucket of a variable-length array, opaque or
//!   string, on its own path next to the array's path. `#max` means the length equals the
//!   bound declared in the XDR (`signatures<20>` holding 20) and wins over `#one` when the bound
//!   is 1. Unbounded (`<>`) values never reach `#max`. String lengths are counted in bytes after
//!   undoing the XDR-JSON escapes.
//! - `#none` an optional value that is absent.
//!
//! Leaf values (numbers, booleans, hashes, keys, string contents) are dropped. Every prefix of
//! a path is also in the set, so `A.b.c` implies `A.b` and `A`.
//!
//! Unions that XDR-JSON renders as a strkey or a plain string (`MuxedAccount`, `SignerKey`,
//! `ScAddress`, `AssetCode`, `PublicKey` and its typedefs `AccountId` and `NodeId`,
//! `ClaimableBalanceId`) still contribute their arm, recovered by parsing the string with the
//! crate's own parser. They contribute nothing below the arm, except that a signed-payload
//! signer adds `:ed25519_signed_payload.payload` and its length bucket against the 64-byte bound.
//! An empty signed payload, which XDR-JSON renders as `<INVALID:G...:EMPTY_PAYLOAD>`, gives
//! `#empty`.
//!
//! Besides the rooted paths, every union arm and enum value taken is also recorded anchored at
//! its type: `SignerKey:ed25519_signed_payload`, `OperationBody:set_options`,
//! `ContractEventType=contract`. Nested union types use the crate's Rust names
//! (`OperationBody`, `TransactionExt`).
//!
//! A real example from testnet ledger 5694308, which holds a payment and Soroban invocations:
//!
//! ```text
//! LedgerCloseMeta:v2.tx_set:v1.phases[]:v0[]:txset_comp_txs_maybe_discounted_fee.txs[]:tx.tx.operations[].body:payment.destination:ed25519
//! ```
//!
//! # Fragments
//!
//! `families.toml` signature fragments are matched by [`fragment_matches`]: a fragment matches
//! when it is a member of the signature, or when it starts with a segment delimiter
//! (`.`, `:`, `=`, `[`, `#`) and some path ends with it. So both `SignerKey:ed25519_signed_payload`
//! and `.body:set_options.signer.key:ed25519_signed_payload` are valid fragments.

use std::collections::BTreeSet;
use std::str::FromStr;

use serde_json::{Map, Value};
use stellar_xdr::{
    AssetCode, ClaimableBalanceId, Limits, MuxedAccount, PublicKey, ScAddress, SignerKey, Type,
    TypeVariant,
};

use crate::{ToolkitError, xdr};

const SIGNED_PAYLOAD_MAX: usize = 64;
const DELIMITERS: [char; 5] = ['.', ':', '=', '[', '#'];
const BUCKETS: [&str; 5] = ["empty", "one", "many", "max", "none"];

/// The JSON Schema of one XDR type and every definition it references.
pub struct XdrSchema {
    root_name: String,
    root: Value,
    defs: Map<String, Value>,
}

impl XdrSchema {
    pub fn for_type(type_name: &str) -> Result<XdrSchema, ToolkitError> {
        let variant = xdr::type_variant(type_name)?;
        Self::for_variant(variant)
    }

    pub fn for_variant(variant: TypeVariant) -> Result<XdrSchema, ToolkitError> {
        let generator = stellar_xdr::schemars::settings_draft201909().into_generator();
        let mut root =
            serde_json::to_value(variant.json_schema(generator)).map_err(ToolkitError::Json)?;
        let defs = match root.as_object_mut().and_then(|o| o.remove("definitions")) {
            Some(Value::Object(defs)) => defs,
            Some(_) => return Err(schema_err("definitions is not an object")),
            None => Map::new(),
        };
        Ok(XdrSchema {
            root_name: variant.name().to_string(),
            root,
            defs,
        })
    }

    // follows $ref chains; the returned name is the last definition reached
    fn resolve<'a>(
        &'a self,
        mut node: &'a Value,
        mut name: Option<&'a str>,
    ) -> Result<(&'a Value, Option<&'a str>), ToolkitError> {
        let mut hops = 0;
        while let Some(r) = node.get("$ref").and_then(Value::as_str) {
            let def = r
                .strip_prefix("#/definitions/")
                .ok_or_else(|| schema_err(format!("unsupported $ref {r}")))?;
            node = if def == self.root_name && !self.defs.contains_key(def) {
                &self.root
            } else {
                self.defs
                    .get(def)
                    .ok_or_else(|| schema_err(format!("missing definition {def}")))?
            };
            name = Some(def);
            hops += 1;
            if hops > 64 {
                return Err(schema_err(format!("$ref cycle through {def}")));
            }
        }
        Ok((node, name))
    }

    /// The structural signature of a decoded value of this schema's type.
    pub fn signature(&self, value: &Value) -> Result<BTreeSet<String>, ToolkitError> {
        let mut out = BTreeSet::new();
        let mut path = self.root_name.clone();
        self.walk(
            value,
            &self.root,
            Some(&self.root_name),
            &mut path,
            &mut out,
        )?;
        Ok(out)
    }

    fn walk(
        &self,
        value: &Value,
        node: &Value,
        name: Option<&str>,
        path: &mut String,
        out: &mut BTreeSet<String>,
    ) -> Result<(), ToolkitError> {
        let (node, name) = self.resolve(node, name)?;
        out.insert(path.clone());

        if let (Some(def), Value::String(text)) = (name, value)
            && let Some(union) = StringUnion::from_def(def)
        {
            return union.walk(text, path, out);
        }

        if let Some(options) = node.get("anyOf").and_then(Value::as_array) {
            if value.is_null() {
                out.insert(format!("{path}#none"));
                return Ok(());
            }
            let branch = options
                .iter()
                .find(|o| o.get("type").and_then(Value::as_str) != Some("null"))
                .ok_or_else(|| schema_err(format!("{path}: optional with no value branch")))?;
            return self.walk(value, branch, None, path, out);
        }

        if let Some(branches) = node.get("oneOf").and_then(Value::as_array) {
            return self.walk_union(value, branches, name, path, out);
        }

        if let Some(values) = node.get("enum").and_then(Value::as_array) {
            let Value::String(v) = value else {
                return Err(shape_err(path, "enum value is not a string"));
            };
            if !values.iter().any(|x| x.as_str() == Some(v)) {
                return Err(shape_err(path, &format!("{v} is not a value of this enum")));
            }
            let sep = if is_xdr_enum(node) { '=' } else { ':' };
            record_choice(name, sep, v, path, out);
            return Ok(());
        }

        match type_of(node) {
            Some("object") => self.walk_struct(value, node, path, out),
            Some("array") => self.walk_array(value, node, path, out),
            Some("string") => walk_string(value, node, path, out),
            _ => Ok(()),
        }
    }

    fn walk_union(
        &self,
        value: &Value,
        branches: &[Value],
        name: Option<&str>,
        path: &mut String,
        out: &mut BTreeSet<String>,
    ) -> Result<(), ToolkitError> {
        match value {
            Value::String(arm) => {
                let known = branches.iter().any(|b| {
                    b.get("enum")
                        .and_then(Value::as_array)
                        .is_some_and(|e| e.iter().any(|x| x.as_str() == Some(arm)))
                });
                if !known {
                    return Err(shape_err(
                        path,
                        &format!("{arm} is not a void arm of this union"),
                    ));
                }
                record_choice(name, ':', arm, path, out);
                Ok(())
            }
            Value::Object(map) if map.len() == 1 => {
                let (arm, inner) = map.iter().next().expect("map has one entry");
                let arm_schema = branches
                    .iter()
                    .find_map(|b| b.get("properties").and_then(|p| p.get(arm)))
                    .ok_or_else(|| {
                        shape_err(path, &format!("{arm} is not an arm of this union"))
                    })?;
                record_choice(name, ':', arm, path, out);
                let len = path.len();
                path.push(':');
                path.push_str(arm);
                let result = self.walk(inner, arm_schema, None, path, out);
                path.truncate(len);
                result
            }
            _ => Err(shape_err(
                path,
                "union value is neither an arm name nor a one-key object",
            )),
        }
    }

    fn walk_struct(
        &self,
        value: &Value,
        node: &Value,
        path: &mut String,
        out: &mut BTreeSet<String>,
    ) -> Result<(), ToolkitError> {
        let Value::Object(map) = value else {
            return Err(shape_err(path, "struct value is not an object"));
        };
        let props = node.get("properties").and_then(Value::as_object);
        let mut keys: Vec<&String> = map.keys().collect();
        keys.sort();
        for key in keys {
            let field_schema = props
                .and_then(|p| p.get(key))
                .ok_or_else(|| shape_err(path, &format!("unknown field {key}")))?;
            let len = path.len();
            path.push('.');
            path.push_str(key);
            let result = self.walk(&map[key], field_schema, None, path, out);
            path.truncate(len);
            result?;
        }
        Ok(())
    }

    fn walk_array(
        &self,
        value: &Value,
        node: &Value,
        path: &mut String,
        out: &mut BTreeSet<String>,
    ) -> Result<(), ToolkitError> {
        let Value::Array(items) = value else {
            return Err(shape_err(path, "array value is not an array"));
        };
        let min = node.get("minItems").and_then(Value::as_u64);
        let max = node.get("maxItems").and_then(Value::as_u64);
        if min.is_none() || min != max {
            out.insert(format!("{path}#{}", bucket(items.len() as u64, max)));
        }
        let item_schema = node
            .get("items")
            .ok_or_else(|| schema_err(format!("{path}: array without items")))?;
        let len = path.len();
        path.push_str("[]");
        let result = items
            .iter()
            .try_for_each(|item| self.walk(item, item_schema, None, path, out));
        path.truncate(len);
        result
    }

    /// Every union arm reachable from this schema's root, written `Type:arm`.
    pub fn reachable_arms(&self) -> Result<BTreeSet<String>, ToolkitError> {
        let mut out = BTreeSet::new();
        let mut seen = BTreeSet::new();
        self.collect_arms(&self.root, Some(&self.root_name), &mut seen, &mut out)?;
        Ok(out)
    }

    fn collect_arms<'a>(
        &'a self,
        node: &'a Value,
        name: Option<&'a str>,
        seen: &mut BTreeSet<&'a str>,
        out: &mut BTreeSet<String>,
    ) -> Result<(), ToolkitError> {
        let mut node = node;
        let mut name = name;
        while let Some(r) = node.get("$ref").and_then(Value::as_str) {
            let def = r
                .strip_prefix("#/definitions/")
                .ok_or_else(|| schema_err(format!("unsupported $ref {r}")))?;
            if !seen.insert(def) {
                return Ok(());
            }
            node = if def == self.root_name && !self.defs.contains_key(def) {
                &self.root
            } else {
                self.defs
                    .get(def)
                    .ok_or_else(|| schema_err(format!("missing definition {def}")))?
            };
            name = Some(def);
        }

        if let Some(def) = name
            && let Some(union) = StringUnion::from_def(def)
        {
            for arm in union.arms() {
                out.insert(format!("{}:{arm}", union.type_name()));
            }
            return Ok(());
        }

        if let Some(branches) = node.get("oneOf").and_then(Value::as_array) {
            let def = name.ok_or_else(|| schema_err("union schema with no definition name"))?;
            for b in branches {
                if let Some(values) = b.get("enum").and_then(Value::as_array) {
                    for v in values.iter().filter_map(Value::as_str) {
                        out.insert(format!("{def}:{v}"));
                    }
                }
                if let Some(props) = b.get("properties").and_then(Value::as_object) {
                    for (arm, schema) in props {
                        out.insert(format!("{def}:{arm}"));
                        self.collect_arms(schema, None, seen, out)?;
                    }
                }
            }
            return Ok(());
        }

        if let Some(values) = node.get("enum").and_then(Value::as_array) {
            if !is_xdr_enum(node) {
                let def = name.ok_or_else(|| schema_err("union schema with no definition name"))?;
                for v in values.iter().filter_map(Value::as_str) {
                    out.insert(format!("{def}:{v}"));
                }
            }
            return Ok(());
        }

        for key in ["anyOf", "allOf"] {
            if let Some(options) = node.get(key).and_then(Value::as_array) {
                for o in options {
                    self.collect_arms(o, None, seen, out)?;
                }
            }
        }
        if let Some(items) = node.get("items") {
            self.collect_arms(items, None, seen, out)?;
        }
        if let Some(props) = node.get("properties").and_then(Value::as_object) {
            for (key, schema) in props {
                // the schema generator adds a $schema string property to object roots
                if key != "$schema" {
                    self.collect_arms(schema, None, seen, out)?;
                }
            }
        }
        Ok(())
    }
}

/// Decodes `bytes` as `type_name` under `limits` and returns its signature.
pub fn shape_signature(
    type_name: &str,
    bytes: &[u8],
    limits: Limits,
) -> Result<BTreeSet<String>, ToolkitError> {
    let value = xdr::decode(type_name, bytes, limits)?;
    signature_of(&value)
}

/// The signature of an already decoded value.
pub fn signature_of(value: &Type) -> Result<BTreeSet<String>, ToolkitError> {
    let schema = XdrSchema::for_variant(value.variant())?;
    schema.signature(&xdr::json_value(value)?)
}

/// Every union arm reachable from `root_type`, the coverage denominator.
pub fn reachable_arms(root_type: &str) -> Result<BTreeSet<String>, ToolkitError> {
    XdrSchema::for_type(root_type)?.reachable_arms()
}

/// Whether a `families.toml` fragment occurs in a signature. See the module docs.
pub fn fragment_matches(signature: &BTreeSet<String>, fragment: &str) -> bool {
    if signature.contains(fragment) {
        return true;
    }
    fragment.starts_with(DELIMITERS) && signature.iter().any(|p| p.ends_with(fragment))
}

/// Checks that a fragment is written in the signature grammar, so a typo cannot silently
/// match nothing.
pub fn check_fragment(fragment: &str) -> Result<(), String> {
    if fragment.is_empty() {
        return Err("fragment is empty".into());
    }
    let mut rest = fragment;
    if !rest.starts_with(DELIMITERS) {
        let end = rest.find(DELIMITERS).unwrap_or(rest.len());
        let head = &rest[..end];
        if !head.starts_with(|c: char| c.is_ascii_uppercase()) || !is_ident(head) {
            return Err(format!(
                "{fragment}: must start with a type name or a delimiter"
            ));
        }
        rest = &rest[end..];
    }
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("[]") {
            rest = r;
            continue;
        }
        let delim = rest.chars().next().expect("rest is not empty");
        if !DELIMITERS.contains(&delim) || delim == '[' {
            return Err(format!("{fragment}: unexpected {delim:?}"));
        }
        let body = &rest[1..];
        let end = body.find(DELIMITERS).unwrap_or(body.len());
        let word = &body[..end];
        if delim == '#' {
            if !BUCKETS.contains(&word) {
                return Err(format!(
                    "{fragment}: #{word} is not one of #{}",
                    BUCKETS.join(", #")
                ));
            }
            if end != body.len() {
                return Err(format!("{fragment}: a #bucket must end the fragment"));
            }
        } else if !is_ident(word) {
            return Err(format!("{fragment}: {delim}{word} is not a name"));
        }
        rest = &body[end..];
    }
    Ok(())
}

fn is_ident(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn record_choice(
    def: Option<&str>,
    sep: char,
    choice: &str,
    path: &str,
    out: &mut BTreeSet<String>,
) {
    out.insert(format!("{path}{sep}{choice}"));
    if let Some(def) = def {
        out.insert(format!("{def}{sep}{choice}"));
    }
}

fn bucket(len: u64, max: Option<u64>) -> &'static str {
    if len == 0 {
        "empty"
    } else if Some(len) == max {
        "max"
    } else if len == 1 {
        "one"
    } else {
        "many"
    }
}

fn type_of(node: &Value) -> Option<&str> {
    match node.get("type") {
        Some(Value::String(t)) => Some(t),
        Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).find(|t| *t != "null"),
        _ => None,
    }
}

// the generator documents every definition as "X is an XDR Enum defined as: ..."
fn is_xdr_enum(node: &Value) -> bool {
    node.get("description")
        .and_then(Value::as_str)
        .is_some_and(|d| d.contains(" is an XDR Enum "))
}

fn walk_string(
    value: &Value,
    node: &Value,
    path: &str,
    out: &mut BTreeSet<String>,
) -> Result<(), ToolkitError> {
    let Value::String(text) = value else {
        // 64-bit integers are strings in XDR-JSON but other leaves may not be
        return Ok(());
    };
    let max_len = node.get("maxLength").and_then(Value::as_u64);
    let min_len = node.get("minLength").and_then(Value::as_u64);
    let hex = node.get("contentEncoding").and_then(Value::as_str) == Some("hex");
    if hex {
        if min_len.is_some() && min_len == max_len {
            return Ok(());
        }
        let bytes = (text.len() / 2) as u64;
        out.insert(format!("{path}#{}", bucket(bytes, max_len.map(|m| m / 2))));
    } else if let Some(max) = max_len {
        let bytes = escaped_byte_len(text)
            .ok_or_else(|| shape_err(path, "string is not valid XDR-JSON escaped text"))?;
        out.insert(format!("{path}#{}", bucket(bytes as u64, Some(max))));
    }
    Ok(())
}

/// Byte length of an XDR string as XDR-JSON renders it: printable ASCII as is, and
/// `\\`, `\0`, `\t`, `\n`, `\r`, `\xNN` escapes for everything else.
pub fn escaped_byte_len(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut n = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => match bytes.get(i + 1)? {
                b'\\' | b'0' | b't' | b'n' | b'r' => i += 2,
                b'x' => {
                    let hex = bytes.get(i + 2..i + 4)?;
                    if !hex.iter().all(u8::is_ascii_hexdigit) {
                        return None;
                    }
                    i += 4;
                }
                _ => return None,
            },
            b' '..=b'~' => i += 1,
            _ => return None,
        }
        n += 1;
    }
    Some(n)
}

#[derive(Clone, Copy)]
enum StringUnion {
    MuxedAccount,
    SignerKey,
    ScAddress,
    AssetCode,
    PublicKey,
    ClaimableBalanceId,
}

impl StringUnion {
    fn from_def(def: &str) -> Option<StringUnion> {
        Some(match def {
            "MuxedAccount" => StringUnion::MuxedAccount,
            "SignerKey" => StringUnion::SignerKey,
            "ScAddress" => StringUnion::ScAddress,
            "AssetCode" => StringUnion::AssetCode,
            "PublicKey" | "AccountId" | "NodeId" => StringUnion::PublicKey,
            "ClaimableBalanceId" => StringUnion::ClaimableBalanceId,
            _ => return None,
        })
    }

    fn type_name(self) -> &'static str {
        match self {
            StringUnion::MuxedAccount => "MuxedAccount",
            StringUnion::SignerKey => "SignerKey",
            StringUnion::ScAddress => "ScAddress",
            StringUnion::AssetCode => "AssetCode",
            StringUnion::PublicKey => "PublicKey",
            StringUnion::ClaimableBalanceId => "ClaimableBalanceId",
        }
    }

    fn arms(self) -> Vec<String> {
        let names: &[&str] = match self {
            StringUnion::MuxedAccount => &MuxedAccount::VARIANTS_STR,
            StringUnion::SignerKey => &SignerKey::VARIANTS_STR,
            StringUnion::ScAddress => &ScAddress::VARIANTS_STR,
            StringUnion::AssetCode => &AssetCode::VARIANTS_STR,
            StringUnion::PublicKey => &PublicKey::VARIANTS_STR,
            StringUnion::ClaimableBalanceId => &ClaimableBalanceId::VARIANTS_STR,
        };
        names.iter().map(|n| snake_case(n)).collect()
    }

    fn walk(self, text: &str, path: &str, out: &mut BTreeSet<String>) -> Result<(), ToolkitError> {
        let parse_err =
            |e: stellar_xdr::Error| shape_err(path, &format!("{} {text:?}: {e}", self.type_name()));
        let (arm, payload_len) = match self {
            StringUnion::MuxedAccount => (
                MuxedAccount::from_str(text).map_err(parse_err)?.name(),
                None,
            ),
            StringUnion::ScAddress => (ScAddress::from_str(text).map_err(parse_err)?.name(), None),
            StringUnion::AssetCode => (AssetCode::from_str(text).map_err(parse_err)?.name(), None),
            StringUnion::PublicKey => (PublicKey::from_str(text).map_err(parse_err)?.name(), None),
            StringUnion::ClaimableBalanceId => (
                ClaimableBalanceId::from_str(text)
                    .map_err(parse_err)?
                    .name(),
                None,
            ),
            StringUnion::SignerKey => {
                if text.starts_with("<INVALID:") && text.ends_with(":EMPTY_PAYLOAD>") {
                    ("Ed25519SignedPayload", Some(0))
                } else {
                    match SignerKey::from_str(text).map_err(parse_err)? {
                        SignerKey::Ed25519SignedPayload(p) => {
                            ("Ed25519SignedPayload", Some(p.payload.len()))
                        }
                        other => (other.name(), None),
                    }
                }
            }
        };
        let arm = snake_case(arm);
        record_choice(Some(self.type_name()), ':', &arm, path, out);
        if let Some(len) = payload_len {
            let payload = format!("{path}:{arm}.payload");
            out.insert(format!(
                "{payload}#{}",
                bucket(len as u64, Some(SIGNED_PAYLOAD_MAX as u64))
            ));
            out.insert(payload);
        }
        Ok(())
    }
}

// matches serde's rename_all = "snake_case", which the crate uses for every union arm
fn snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn schema_err(msg: impl Into<String>) -> ToolkitError {
    ToolkitError::Schema(msg.into())
}

fn shape_err(path: &str, msg: &str) -> ToolkitError {
    ToolkitError::Shape {
        path: path.to_string(),
        message: msg.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::StringM;

    #[test]
    fn escaped_len_matches_the_crate_rendering() {
        let raw: Vec<u8> = vec![b'a', b'\\', 0, b'\t', b'\n', b'\r', 0xff, 0x80, b'~', b' '];
        let s = StringM::<28>::try_from(raw.clone()).unwrap();
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(escaped_byte_len(json.as_str().unwrap()), Some(raw.len()));
        assert_eq!(escaped_byte_len("\\q"), None);
        assert_eq!(escaped_byte_len("\\x4"), None);
    }

    #[test]
    fn snake_case_matches_serde() {
        assert_eq!(snake_case("Ed25519SignedPayload"), "ed25519_signed_payload");
        assert_eq!(snake_case("MuxedEd25519"), "muxed_ed25519");
        assert_eq!(snake_case("CreditAlphanum12"), "credit_alphanum12");
    }

    #[test]
    fn buckets() {
        assert_eq!(bucket(0, Some(20)), "empty");
        assert_eq!(bucket(1, Some(20)), "one");
        assert_eq!(bucket(1, Some(1)), "max");
        assert_eq!(bucket(7, Some(20)), "many");
        assert_eq!(bucket(20, Some(20)), "max");
        assert_eq!(bucket(5, None), "many");
    }

    #[test]
    fn fragment_grammar() {
        for ok in [
            "SignerKey:ed25519_signed_payload",
            ".body:set_options.signer.key:ed25519_signed_payload",
            "TransactionEnvelope:tx.signatures#max",
            ".operations[].source_account#none",
            "ContractEventType=contract",
            "LedgerCloseMeta",
        ] {
            assert_eq!(check_fragment(ok), Ok(()), "{ok}");
        }
        for bad in [
            "",
            "signerKey:x",
            ".a#huge",
            ".a#max.b",
            "A:",
            "A::b",
            ".a b",
            "A]",
        ] {
            assert!(check_fragment(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn fragment_matching_respects_segment_boundaries() {
        let sig: BTreeSet<String> = [
            "T.signed_payload".to_string(),
            "T.x:ed25519_signed_payload.payload#max".to_string(),
            "SignerKey:ed25519_signed_payload".to_string(),
        ]
        .into();
        assert!(fragment_matches(&sig, "SignerKey:ed25519_signed_payload"));
        assert!(fragment_matches(&sig, ".payload#max"));
        assert!(fragment_matches(
            &sig,
            ":ed25519_signed_payload.payload#max"
        ));
        assert!(!fragment_matches(&sig, ".payload"));
        assert!(!fragment_matches(&sig, "payload#max"));
    }
}
