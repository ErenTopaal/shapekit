//! core's cereal json back to XDR.
//!
//! core writes test metas with `xdrToCerealString` (`src/util/XDRCereal.h` and `.cpp`,
//! stellar-core v30.0.0). The generic part comes from xdrpp: a struct is an object of its
//! fields in declaration order, a union is an object holding the discriminant under its
//! declared name and the arm under the arm's name (nothing for a void arm), an optional is
//! `null` or the value itself, a non-byte array is a json array, a number is a json number.
//! On top of that core overrides these types, and this module undoes each override:
//!
//! | rule | type | written as |
//! |---|---|---|
//! | 1 | `string<N>` | the string |
//! | 2 | `opaque[N]` | lowercase hex |
//! | 3 | `opaque<N>` | lowercase hex |
//! | 4 | non-byte arrays | json array (also generic) |
//! | 5 | enums, and `bool`, which xdrpp declares an enum | member name (`"TRUE"`, `"FALSE"` for a bool), or the decimal value as a string when the value has no name |
//! | 6 | optionals (`T*`) | `null` or the value |
//! | 7 | `PublicKey` | `G...` strkey |
//! | 8 | `SCAddress` | `G...`, `C...`, hex for a claimable balance or pool id, `{id, ed25519}` for a muxed account, `{id, contractId}` for a muxed contract |
//! | 9 | `ConfigUpgradeSetKey` | `{contractID: C..., contentHash}` |
//! | 10 | `MuxedAccount` | `G...`, or `{id, accountID: G...}` |
//! | 11 | `Asset`, `TrustLineAsset`, `ChangeTrustAsset` native | `"NATIVE"` |
//! | 12 | the same three, credit | `{assetCode, issuer}` with the code cut at its first zero byte |
//! | 13 | `TrustLineAsset` pool share | the pool id in hex |
//! | 14 | `ChangeTrustAsset` pool share | `{assetA, assetB, fee}` |
//!
//! Three of these lose information in general, and the inverse needs a rule for each:
//!
//! - rule 12: the arm is not written. A code of up to four bytes is taken as `alphaNum4` and
//!   a longer one as `alphaNum12`, which is what core requires of a valid asset
//!   (`isAssetValid`, `src/transactions/TransactionUtils.cpp`); the code is padded back with
//!   zero bytes. A code with bytes after its first zero cannot be written back and does not
//!   arise for a valid asset.
//! - rule 8: a claimable balance id and a pool id are both 32 bytes of hex. The inverse looks
//!   the value up among the claimable balance ids and pool ids the same meta holds in typed
//!   positions, and fails when it finds it in neither or in both.
//! - rule 11 for `Asset` written as `"INVALID"` (a pool share where only credit assets are
//!   valid) and `"UNKNOWN"`: no inverse; the conversion fails.
//!
//! Every conversion is checked by the caller against core's own short hash of the meta, so a
//! wrong guess in any of these rules shows up as a mismatch, never as silently wrong bytes.

use std::collections::BTreeSet;

use serde_json::{Map, Value};
use shapekit_schema::{Definition, Schema, Type, Union};

#[derive(Debug, thiserror::Error)]
#[error("{path}: {message}")]
pub struct CerealError {
    /// json path of the value that could not be converted
    pub path: String,
    pub message: String,
}

type Result<T> = std::result::Result<T, CerealError>;

/// Ids found in typed positions, used to tell the two hex forms of `SCAddress` apart.
#[derive(Debug, Default, Clone)]
struct Ids {
    claimable_balances: BTreeSet<String>,
    pools: BTreeSet<String>,
}

/// What an ambiguous `SCAddress` hex value resolved to, for the report.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Resolutions {
    pub claimable_balance: u64,
    pub liquidity_pool: u64,
}

pub struct Converter<'s> {
    schema: &'s Schema,
}

struct Pass<'s, 'i> {
    schema: &'s Schema,
    /// `None` in the collecting pass
    known: Option<&'i Ids>,
    found: Ids,
    resolutions: Resolutions,
    out: Vec<u8>,
}

fn err<T>(path: &str, message: impl Into<String>) -> Result<T> {
    Err(CerealError {
        path: path.to_owned(),
        message: message.into(),
    })
}

fn pad(out: &mut Vec<u8>, len: usize) {
    out.resize(out.len() + (4 - len % 4) % 4, 0);
}

fn obj<'v>(v: &'v Value, path: &str) -> Result<&'v Map<String, Value>> {
    v.as_object().ok_or_else(|| CerealError {
        path: path.to_owned(),
        message: format!("expected an object, found {}", kind_of(v)),
    })
}

fn str_of<'v>(v: &'v Value, path: &str) -> Result<&'v str> {
    v.as_str().ok_or_else(|| CerealError {
        path: path.to_owned(),
        message: format!("expected a string, found {}", kind_of(v)),
    })
}

fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a bool",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

fn hex_bytes(v: &Value, path: &str) -> Result<Vec<u8>> {
    let s = str_of(v, path)?;
    if s.bytes().any(|b| b.is_ascii_uppercase()) {
        return err(path, "hex with uppercase digits; core writes lowercase");
    }
    hex::decode(s).map_err(|e| CerealError {
        path: path.to_owned(),
        message: format!("bad hex: {e}"),
    })
}

fn account_key(s: &str, path: &str) -> Result<[u8; 32]> {
    stellar_strkey::ed25519::PublicKey::from_string(s)
        .map(|k| k.0)
        .map_err(|e| CerealError {
            path: path.to_owned(),
            message: format!("not an account strkey {s:?}: {e:?}"),
        })
}

fn contract_key(s: &str, path: &str) -> Result<[u8; 32]> {
    stellar_strkey::Contract::from_string(s)
        .map(|k| k.0)
        .map_err(|e| CerealError {
            path: path.to_owned(),
            message: format!("not a contract strkey {s:?}: {e:?}"),
        })
}

/// Checks that an object holds exactly `keys`.
fn exact_keys(m: &Map<String, Value>, keys: &[&str], path: &str) -> Result<()> {
    let have: BTreeSet<&str> = m.keys().map(String::as_str).collect();
    let want: BTreeSet<&str> = keys.iter().copied().collect();
    if have != want {
        return err(path, format!("object has keys {have:?}, expected {want:?}"));
    }
    Ok(())
}

fn field<'v>(m: &'v Map<String, Value>, key: &str, path: &str) -> Result<&'v Value> {
    m.get(key).ok_or_else(|| CerealError {
        path: path.to_owned(),
        message: format!("missing {key:?}"),
    })
}

fn sub(path: &str, key: &str) -> String {
    format!("{path}.{key}")
}

/// `{"type": <name>, <arm>: <value>}`, the generic form of a union value.
fn generic_union(disc: &str, arm: Option<(&str, Value)>) -> Value {
    let mut m = Map::new();
    m.insert("type".into(), Value::String(disc.into()));
    if let Some((name, v)) = arm {
        m.insert(name.into(), v);
    }
    Value::Object(m)
}

impl<'s> Converter<'s> {
    pub fn new(schema: &'s Schema) -> Self {
        Converter { schema }
    }

    /// Converts the value of a cereal document's root (`{"<root>": value}`) to XDR.
    pub fn document(&self, doc: &Value, root: &str) -> Result<(Vec<u8>, Resolutions)> {
        let m = obj(doc, "$")?;
        exact_keys(m, &[root], "$")?;
        self.value(&m[root], root)
    }

    /// Converts one value of type `name`.
    pub fn value(&self, v: &Value, name: &str) -> Result<(Vec<u8>, Resolutions)> {
        let ty = Type::Ref {
            name: name.to_owned(),
        };
        let mut collect = Pass {
            schema: self.schema,
            known: None,
            found: Ids::default(),
            resolutions: Resolutions::default(),
            out: Vec::new(),
        };
        collect.encode(&ty, v, name)?;
        let ids = collect.found;
        let mut pass = Pass {
            schema: self.schema,
            known: Some(&ids),
            found: Ids::default(),
            resolutions: Resolutions::default(),
            out: Vec::new(),
        };
        pass.encode(&ty, v, name)?;
        Ok((pass.out, pass.resolutions))
    }
}

impl<'s> Pass<'s, '_> {
    fn encode(&mut self, ty: &Type, v: &Value, path: &str) -> Result<()> {
        match ty {
            Type::Int => {
                let n = v
                    .as_i64()
                    .and_then(|n| i32::try_from(n).ok())
                    .ok_or_else(|| CerealError {
                        path: path.into(),
                        message: format!("expected an int, found {v}"),
                    })?;
                self.out.extend_from_slice(&n.to_be_bytes());
            }
            Type::UnsignedInt => {
                let n = v
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| CerealError {
                        path: path.into(),
                        message: format!("expected an unsigned int, found {v}"),
                    })?;
                self.out.extend_from_slice(&n.to_be_bytes());
            }
            Type::Hyper => {
                let n = v.as_i64().ok_or_else(|| CerealError {
                    path: path.into(),
                    message: format!("expected a hyper, found {v}"),
                })?;
                self.out.extend_from_slice(&n.to_be_bytes());
            }
            Type::UnsignedHyper => {
                let n = v.as_u64().ok_or_else(|| CerealError {
                    path: path.into(),
                    message: format!("expected an unsigned hyper, found {v}"),
                })?;
                self.out.extend_from_slice(&n.to_be_bytes());
            }
            Type::Bool => {
                // xdrpp declares bool an enum named FALSE and TRUE (`xdr_traits<bool>`,
                // xdrpp/types.h), so rule 5 writes it as one of the two names
                let b = match v.as_str() {
                    Some("FALSE") => 0u32,
                    Some("TRUE") => 1,
                    _ => return err(path, format!("expected \"TRUE\" or \"FALSE\", found {v}")),
                };
                self.out.extend_from_slice(&b.to_be_bytes());
            }
            Type::Float | Type::Double => return err(path, "floating point types are not in use"),
            Type::OpaqueFixed { size } => {
                let b = hex_bytes(v, path)?;
                if b.len() as u64 != *size {
                    return err(path, format!("{} bytes, expected {size}", b.len()));
                }
                self.out.extend_from_slice(&b);
                pad(&mut self.out, b.len());
            }
            Type::OpaqueVar { max_size } => {
                let b = hex_bytes(v, path)?;
                self.var_bytes(&b, *max_size, path)?;
            }
            Type::String { max_size } => {
                let s = str_of(v, path)?;
                self.var_bytes(s.as_bytes(), *max_size, path)?;
            }
            Type::Array { element, count } => {
                let a = v.as_array().ok_or_else(|| CerealError {
                    path: path.into(),
                    message: format!("expected an array, found {}", kind_of(v)),
                })?;
                if a.len() as u64 != *count {
                    return err(path, format!("{} elements, expected {count}", a.len()));
                }
                for (i, e) in a.iter().enumerate() {
                    self.encode(element, e, &format!("{path}[{i}]"))?;
                }
            }
            Type::VarArray { element, max_count } => {
                let a = v.as_array().ok_or_else(|| CerealError {
                    path: path.into(),
                    message: format!("expected an array, found {}", kind_of(v)),
                })?;
                if let Some(max) = max_count
                    && a.len() as u64 > *max
                {
                    return err(path, format!("{} elements, at most {max} allowed", a.len()));
                }
                let n = u32::try_from(a.len()).map_err(|_| CerealError {
                    path: path.into(),
                    message: "array too long".into(),
                })?;
                self.out.extend_from_slice(&n.to_be_bytes());
                for (i, e) in a.iter().enumerate() {
                    self.encode(element, e, &format!("{path}[{i}]"))?;
                }
            }
            Type::Optional { element } => {
                if v.is_null() {
                    self.out.extend_from_slice(&0u32.to_be_bytes());
                } else {
                    self.out.extend_from_slice(&1u32.to_be_bytes());
                    self.encode(element, v, path)?;
                }
            }
            Type::Ref { name } => self.encode_ref(name, v, path)?,
        }
        Ok(())
    }

    fn var_bytes(&mut self, b: &[u8], max: Option<u64>, path: &str) -> Result<()> {
        if let Some(max) = max
            && b.len() as u64 > max
        {
            return err(path, format!("{} bytes, at most {max} allowed", b.len()));
        }
        let n = u32::try_from(b.len()).map_err(|_| CerealError {
            path: path.into(),
            message: "too long".into(),
        })?;
        self.out.extend_from_slice(&n.to_be_bytes());
        self.out.extend_from_slice(b);
        pad(&mut self.out, b.len());
        Ok(())
    }

    fn definition(&self, name: &str, path: &str) -> Result<&'s Definition> {
        self.schema.get(name).ok_or_else(|| CerealError {
            path: path.into(),
            message: format!("no definition named {name}"),
        })
    }

    fn encode_ref(&mut self, name: &str, v: &Value, path: &str) -> Result<()> {
        if name == "PoolID" {
            self.found.pools.insert(str_of(v, path)?.to_owned());
        }
        let def = self.definition(name, path)?;
        match def {
            Definition::Typedef(t) => self.encode(&t.ty, v, path),
            Definition::Const(_) => err(path, format!("{name} is a constant, not a type")),
            Definition::Enum(e) => {
                let s = str_of(v, path)?;
                let value = match e.members.iter().find(|m| m.name == s) {
                    Some(m) => m.value,
                    // rule 5: a value without a name is written as its decimal digits
                    None => s.parse::<i32>().map(i64::from).map_err(|_| CerealError {
                        path: path.into(),
                        message: format!("{s:?} is not a member of {name}"),
                    })?,
                };
                let n = i32::try_from(value).map_err(|_| CerealError {
                    path: path.into(),
                    message: format!("{name} value {value} does not fit an int"),
                })?;
                self.out.extend_from_slice(&n.to_be_bytes());
                Ok(())
            }
            Definition::Struct(s) => {
                if name == "ConfigUpgradeSetKey" {
                    return self.config_upgrade_set_key(v, path);
                }
                let m = obj(v, path)?;
                let keys: Vec<&str> = s.fields.iter().map(|f| f.name.as_str()).collect();
                exact_keys(m, &keys, path)?;
                for f in &s.fields {
                    self.encode(&f.ty, &m[&f.name], &sub(path, &f.name))?;
                }
                Ok(())
            }
            Definition::Union(u) => match name {
                "PublicKey" => self.public_key(v, path),
                "MuxedAccount" => self.muxed_account(u, v, path),
                "SCAddress" => self.sc_address(u, v, path),
                "Asset" | "TrustLineAsset" | "ChangeTrustAsset" => self.asset(name, u, v, path),
                _ => self.generic_union(name, u, v, path),
            },
        }
    }

    fn generic_union(&mut self, name: &str, u: &Union, v: &Value, path: &str) -> Result<()> {
        let m = obj(v, path)?;
        let disc_name = u.discriminant.name.as_str();
        let disc_path = sub(path, disc_name);
        let dv = field(m, disc_name, path)?;
        let start = self.out.len();
        self.encode(&u.discriminant.ty, dv, &disc_path)?;
        let value = i64::from(i32::from_be_bytes(
            self.out[start..start + 4]
                .try_into()
                .expect("four bytes written"),
        ));
        let resolved = self.schema.resolve(&u.discriminant.ty);
        let value = if matches!(resolved, Type::UnsignedInt) {
            i64::from(value as u32)
        } else {
            value
        };
        let arm = u.arm(value).ok_or_else(|| CerealError {
            path: disc_path.clone(),
            message: format!("{value} selects no arm of {name}"),
        })?;
        match (&arm.name, &arm.ty) {
            (Some(arm_name), Some(ty)) => {
                exact_keys(m, &[disc_name, arm_name], path)?;
                let av = &m[arm_name];
                if name == "ClaimableBalanceID" {
                    self.found
                        .claimable_balances
                        .insert(str_of(av, path)?.to_owned());
                }
                self.encode(ty, av, &sub(path, arm_name))
            }
            _ => exact_keys(m, &[disc_name], path),
        }
    }

    /// rule 7
    fn public_key(&mut self, v: &Value, path: &str) -> Result<()> {
        let key = account_key(str_of(v, path)?, path)?;
        self.out.extend_from_slice(&0u32.to_be_bytes());
        self.out.extend_from_slice(&key);
        Ok(())
    }

    /// rule 10
    fn muxed_account(&mut self, u: &Union, v: &Value, path: &str) -> Result<()> {
        let generic = match v {
            Value::String(s) => generic_union(
                "KEY_TYPE_ED25519",
                Some(("ed25519", hex_value(&account_key(s, path)?))),
            ),
            Value::Object(m) => {
                exact_keys(m, &["id", "accountID"], path)?;
                let key = account_key(str_of(&m["accountID"], &sub(path, "accountID"))?, path)?;
                let mut med = Map::new();
                med.insert("id".into(), m["id"].clone());
                med.insert("ed25519".into(), hex_value(&key));
                generic_union(
                    "KEY_TYPE_MUXED_ED25519",
                    Some(("med25519", Value::Object(med))),
                )
            }
            _ => {
                return err(
                    path,
                    format!("unexpected muxed account form {}", kind_of(v)),
                );
            }
        };
        self.generic_union("MuxedAccount", u, &generic, path)
    }

    /// rule 8
    fn sc_address(&mut self, u: &Union, v: &Value, path: &str) -> Result<()> {
        let generic = match v {
            Value::String(s) if s.starts_with('G') => {
                generic_union("SC_ADDRESS_TYPE_ACCOUNT", Some(("accountId", v.clone())))
            }
            Value::String(s) if s.starts_with('C') => generic_union(
                "SC_ADDRESS_TYPE_CONTRACT",
                Some(("contractId", hex_value(&contract_key(s, path)?))),
            ),
            Value::String(s) if s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()) => {
                let as_balance = || {
                    let mut id = Map::new();
                    id.insert("type".into(), "CLAIMABLE_BALANCE_ID_TYPE_V0".into());
                    id.insert("v0".into(), v.clone());
                    generic_union(
                        "SC_ADDRESS_TYPE_CLAIMABLE_BALANCE",
                        Some(("claimableBalanceId", Value::Object(id))),
                    )
                };
                let as_pool = || {
                    generic_union(
                        "SC_ADDRESS_TYPE_LIQUIDITY_POOL",
                        Some(("liquidityPoolId", v.clone())),
                    )
                };
                match self.known {
                    // collecting pass: its bytes are discarded, and encoding either reading
                    // would record this very value as a typed id
                    None => return Ok(()),
                    Some(ids) => {
                        let balance = ids.claimable_balances.contains(s.as_str());
                        let pool = ids.pools.contains(s.as_str());
                        match (balance, pool) {
                            (true, false) => {
                                self.resolutions.claimable_balance += 1;
                                as_balance()
                            }
                            (false, true) => {
                                self.resolutions.liquidity_pool += 1;
                                as_pool()
                            }
                            (true, true) => {
                                return err(
                                    path,
                                    format!(
                                        "{s} is both a claimable balance id and a pool id in this meta"
                                    ),
                                );
                            }
                            (false, false) => {
                                return err(
                                    path,
                                    format!(
                                        "{s} is neither a claimable balance id nor a pool id this meta holds"
                                    ),
                                );
                            }
                        }
                    }
                }
            }
            Value::Object(m) if m.contains_key("ed25519") => {
                exact_keys(m, &["id", "ed25519"], path)?;
                generic_union(
                    "SC_ADDRESS_TYPE_MUXED_ACCOUNT",
                    Some(("muxedAccount", v.clone())),
                )
            }
            Value::Object(m) if m.contains_key("contractId") => {
                exact_keys(m, &["id", "contractId"], path)?;
                let key = contract_key(str_of(&m["contractId"], &sub(path, "contractId"))?, path)?;
                let mut mc = Map::new();
                mc.insert("id".into(), m["id"].clone());
                mc.insert("contractId".into(), hex_value(&key));
                generic_union(
                    "SC_ADDRESS_TYPE_MUXED_CONTRACT",
                    Some(("muxedContract", Value::Object(mc))),
                )
            }
            _ => return err(path, format!("unexpected address form {v}")),
        };
        self.generic_union("SCAddress", u, &generic, path)
    }

    /// rule 9
    fn config_upgrade_set_key(&mut self, v: &Value, path: &str) -> Result<()> {
        let m = obj(v, path)?;
        exact_keys(m, &["contractID", "contentHash"], path)?;
        let key = contract_key(str_of(&m["contractID"], &sub(path, "contractID"))?, path)?;
        self.out.extend_from_slice(&key);
        let hash = Type::Ref {
            name: "Hash".into(),
        };
        self.encode(&hash, &m["contentHash"], &sub(path, "contentHash"))
    }

    /// rules 11 to 14
    fn asset(&mut self, name: &str, u: &Union, v: &Value, path: &str) -> Result<()> {
        let generic = match v {
            Value::String(s) if s == "NATIVE" => generic_union("ASSET_TYPE_NATIVE", None),
            Value::String(s)
                if name == "TrustLineAsset"
                    && s.len() == 64
                    && s.bytes().all(|b| b.is_ascii_hexdigit()) =>
            {
                generic_union(
                    "ASSET_TYPE_POOL_SHARE",
                    Some(("liquidityPoolID", v.clone())),
                )
            }
            Value::Object(m) if m.contains_key("assetCode") => {
                exact_keys(m, &["assetCode", "issuer"], path)?;
                let code = str_of(&m["assetCode"], &sub(path, "assetCode"))?.as_bytes();
                if code.contains(&0) {
                    return err(path, "asset code holds a zero byte");
                }
                let (disc, arm, width) = match code.len() {
                    1..=4 => ("ASSET_TYPE_CREDIT_ALPHANUM4", "alphaNum4", 4),
                    5..=12 => ("ASSET_TYPE_CREDIT_ALPHANUM12", "alphaNum12", 12),
                    n => return err(path, format!("asset code of {n} bytes")),
                };
                let mut padded = code.to_vec();
                padded.resize(width, 0);
                let mut alpha = Map::new();
                alpha.insert("assetCode".into(), hex_value(&padded));
                alpha.insert("issuer".into(), m["issuer"].clone());
                generic_union(disc, Some((arm, Value::Object(alpha))))
            }
            Value::Object(m) if name == "ChangeTrustAsset" && m.contains_key("assetA") => {
                exact_keys(m, &["assetA", "assetB", "fee"], path)?;
                let mut cp = Map::new();
                cp.insert("type".into(), "LIQUIDITY_POOL_CONSTANT_PRODUCT".into());
                cp.insert("constantProduct".into(), v.clone());
                generic_union(
                    "ASSET_TYPE_POOL_SHARE",
                    Some(("liquidityPool", Value::Object(cp))),
                )
            }
            _ => return err(path, format!("{name} written as {v}, which has no inverse")),
        };
        self.generic_union(name, u, &generic, path)
    }
}

fn hex_value(b: &[u8]) -> Value {
    Value::String(hex::encode(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::{Limits, ReadXdr, TransactionMeta, WriteXdr};

    fn convert(json: &str, root: &str) -> Vec<u8> {
        let schema = Schema::pinned();
        let doc: Value = serde_json::from_str(json).unwrap();
        Converter::new(&schema).document(&doc, root).unwrap().0
    }

    #[test]
    fn converts_a_v2_meta_with_an_account_entry() {
        let json = r#"{"TransactionMeta": {"v": 2, "v2": {
            "txChangesBefore": [],
            "operations": [{"changes": [{"type": "LEDGER_ENTRY_CREATED", "created": {
                "lastModifiedLedgerSeq": 3,
                "data": {"type": "ACCOUNT", "account": {
                    "accountID": "GBW5HOHGBZV32TE6SYWOMRSY27JDD754PEBMIQNA5OBWLCUBTXO56ZFV",
                    "balance": 200001000, "seqNum": 12884901888, "numSubEntries": 0,
                    "inflationDest": null, "flags": 0, "homeDomain": "",
                    "thresholds": "01000000", "signers": [], "ext": {"v": 0}}},
                "ext": {"v": 0}}}]}],
            "txChangesAfter": []}}}"#;
        let bytes = convert(json, "TransactionMeta");
        let meta = TransactionMeta::from_xdr(&bytes, Limits::none()).unwrap();
        assert_eq!(meta.to_xdr(Limits::none()).unwrap(), bytes);
        let TransactionMeta::V2(v2) = meta else {
            panic!("not v2")
        };
        assert_eq!(v2.operations.len(), 1);
    }

    #[test]
    fn credit_assets_are_split_by_code_length() {
        let issuer = "GBW5HOHGBZV32TE6SYWOMRSY27JDD754PEBMIQNA5OBWLCUBTXO56ZFV";
        let four = convert(
            &format!(r#"{{"Asset": {{"assetCode": "USD", "issuer": "{issuer}"}}}}"#),
            "Asset",
        );
        assert_eq!(&four[..4], &1u32.to_be_bytes());
        assert_eq!(&four[4..8], b"USD\0");
        let twelve = convert(
            &format!(r#"{{"Asset": {{"assetCode": "ABCDE", "issuer": "{issuer}"}}}}"#),
            "Asset",
        );
        assert_eq!(&twelve[..4], &2u32.to_be_bytes());
        assert_eq!(&twelve[4..16], b"ABCDE\0\0\0\0\0\0\0");
    }

    #[test]
    fn invalid_asset_has_no_inverse() {
        let schema = Schema::pinned();
        let doc: Value = serde_json::from_str(r#"{"Asset": "INVALID"}"#).unwrap();
        let e = Converter::new(&schema).document(&doc, "Asset").unwrap_err();
        assert!(e.message.contains("no inverse"), "{e}");
    }

    #[test]
    fn bools_are_enum_names() {
        let t = convert(r#"{"SCVal": {"type": "SCV_BOOL", "b": "TRUE"}}"#, "SCVal");
        assert_eq!(t, [0, 0, 0, 0, 0, 0, 0, 1]);
        let schema = Schema::pinned();
        let doc: Value =
            serde_json::from_str(r#"{"SCVal": {"type": "SCV_BOOL", "b": true}}"#).unwrap();
        assert!(Converter::new(&schema).document(&doc, "SCVal").is_err());
    }

    #[test]
    fn unknown_enum_values_are_written_as_digits() {
        let bytes = convert(r#"{"LedgerEntryType": "77"}"#, "LedgerEntryType");
        assert_eq!(bytes, 77u32.to_be_bytes());
    }

    #[test]
    fn extra_keys_are_refused() {
        let schema = Schema::pinned();
        let doc: Value = serde_json::from_str(r#"{"ExtensionPoint": {"v": 0, "x": 1}}"#).unwrap();
        assert!(
            Converter::new(&schema)
                .document(&doc, "ExtensionPoint")
                .is_err()
        );
    }

    #[test]
    fn sc_address_hex_needs_an_id_in_a_typed_position() {
        let id = "aa".repeat(32);
        let schema = Schema::pinned();
        let alone: Value = serde_json::from_str(&format!(r#"{{"SCAddress": "{id}"}}"#)).unwrap();
        let e = Converter::new(&schema)
            .document(&alone, "SCAddress")
            .unwrap_err();
        assert!(e.message.contains("neither"), "{e}");
    }
}
