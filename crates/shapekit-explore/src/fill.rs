//! Fills XDR-JSON values from the schema. Every definition is generated from its shape
//! (struct fields, a union arm, an enum value, an array of a few elements), and the types
//! that name something on the chain draw from the [`World`] instead: accounts from the
//! recipe's roles or an account that does not exist, assets from the issuers, offer ids,
//! claimable balances, pools and ledger keys from what the ledgers showed. Numbers draw from
//! a per-field pool of ordinary, zero, boundary and out-of-range values.

use serde_json::{Value, json};
use shapekit_core_driver::config::close_time;
use shapekit_core_driver::keys::derive_key;
use shapekit_schema::names::{case_name, field_name, member_name};
use shapekit_schema::{Definition, Schema, Type, Union};
use stellar_xdr::{Hash, SignerKey, SignerKeyEd25519SignedPayload, Uint256};

use crate::rng::Rng;
use crate::world::{AssetRef, CODES4, CODES12, POOL_FEE, World};

/// Nesting depth past which optional values are left out and arrays kept empty, so recursive
/// types (`ClaimPredicate`) stay finite.
const MAX_DEPTH: u32 = 6;

pub struct Filler<'a> {
    schema: &'a Schema,
    world: &'a World,
    pub rng: &'a mut Rng,
    /// The transaction's source role, which sequence number references prefer.
    source: String,
    /// Roles the transaction already names, reused for later account fields so related
    /// operations (a sponsorship and the operations it sponsors) meet.
    pub named: Vec<String>,
    depth: u32,
}

const AMOUNTS: &[i64] = &[
    0,
    1,
    7,
    100,
    10_000_000,
    1_000_000_000,
    50_000_000_000,
    1_000_000_000_000,
    i64::MAX,
    -1,
];

impl<'a> Filler<'a> {
    pub fn new(schema: &'a Schema, world: &'a World, rng: &'a mut Rng, source: &str) -> Self {
        Self {
            schema,
            world,
            rng,
            source: source.to_owned(),
            named: vec![source.to_owned()],
            depth: 0,
        }
    }

    fn now(&self) -> u64 {
        close_time(self.world.last_closed + 1)
    }

    /// A value of the definition `name`.
    pub fn def(&mut self, name: &str) -> Value {
        self.typed(&Type::Ref { name: name.into() }, None)
    }

    /// One arm of union `union`, chosen by its XDR-JSON name.
    pub fn union_arm(&mut self, union: &str, arm: &str) -> Option<Value> {
        let Some(Definition::Union(u)) = self.schema.get(union) else {
            return None;
        };
        let u = u.clone();
        for a in &u.arms {
            for c in &a.cases {
                if case_name(self.schema, &u, c) == arm {
                    return Some(self.arm_value(&u, arm, a.ty.as_ref()));
                }
            }
        }
        None
    }

    /// The XDR-JSON names of a union's arms.
    pub fn arm_names(schema: &Schema, union: &str) -> Vec<String> {
        let Some(Definition::Union(u)) = schema.get(union) else {
            return Vec::new();
        };
        u.arms
            .iter()
            .flat_map(|a| a.cases.iter().map(|c| case_name(schema, u, c)))
            .collect()
    }

    fn arm_value(&mut self, _u: &Union, name: &str, ty: Option<&Type>) -> Value {
        match ty {
            None => Value::String(name.to_owned()),
            Some(ty) => {
                let ty = ty.clone();
                json!({ name: self.nested(&ty, Some(name)) })
            }
        }
    }

    fn nested(&mut self, ty: &Type, field: Option<&str>) -> Value {
        self.depth += 1;
        let v = self.typed(ty, field);
        self.depth -= 1;
        v
    }

    /// A role, preferring one the transaction already names.
    pub fn role(&mut self) -> String {
        if self.named.len() > 1 && self.rng.chance(1, 2) {
            return self.rng.pick(&self.named).clone();
        }
        let r = self.rng.pick(&self.world.funded).clone();
        if !self.named.contains(&r) {
            self.named.push(r.clone());
        }
        r
    }

    fn account(&mut self) -> Value {
        match self.rng.below(20) {
            0 => Value::String(self.rng.pick(&self.world.ghosts).clone()),
            1 => Value::String("@account:root".into()),
            _ => Value::String(format!("@account:{}", self.role())),
        }
    }

    fn muxed(&mut self) -> Value {
        if self.rng.chance(1, 5) {
            let id = *self.rng.pick(&[0u64, 1, u64::MAX]);
            Value::String(format!("@muxed:{}:{id}", self.role()))
        } else {
            self.account()
        }
    }

    /// An asset: one some account trusts, native, or any the issuers issue.
    fn asset_ref(&mut self) -> AssetRef {
        let trusted: Vec<AssetRef> = self.world.trusted.iter().cloned().collect();
        let a = match self.rng.below(5) {
            0 | 1 if !trusted.is_empty() => self.rng.pick(&trusted).clone(),
            0 => AssetRef::native(),
            _ => {
                let assets = self.world.assets();
                self.rng.pick(&assets).clone()
            }
        };
        self.name_issuer(&a);
        a
    }

    /// An asset's issuer counts as named by the transaction, so later account fields and
    /// the transaction source can be the issuer.
    fn name_issuer(&mut self, a: &AssetRef) {
        if let Some(i) = &a.issuer
            && !self.named.contains(i)
            && self.world.funded.contains(i)
        {
            self.named.push(i.clone());
        }
    }

    fn credit_ref(&mut self) -> AssetRef {
        let assets: Vec<AssetRef> = self
            .world
            .assets()
            .into_iter()
            .filter(|a| a.code.is_some())
            .collect();
        if assets.is_empty() {
            return AssetRef::native();
        }
        let a = self.rng.pick(&assets).clone();
        self.name_issuer(&a);
        a
    }

    fn pool_share(&mut self) -> Value {
        let a = self.asset_ref();
        let b = self.credit_ref();
        match self.world.pool(&a, &b) {
            Ok(Some((_, json))) => json,
            // equal assets: core refuses them, which is a value worth trying too
            _ => {
                json!({"liquidity_pool_constant_product": {"asset_a": a.json(), "asset_b": b.json(), "fee": POOL_FEE}})
            }
        }
    }

    fn pool_hash(&mut self) -> [u8; 32] {
        let known: Vec<String> = self.world.pools.keys().cloned().collect();
        let id = if !known.is_empty() && self.rng.chance(4, 5) {
            Some(self.rng.pick(&known).clone())
        } else {
            let a = self.asset_ref();
            let b = self.credit_ref();
            self.world.pool(&a, &b).ok().flatten().map(|(id, _)| id)
        };
        id.and_then(|h| hex::decode(h).ok())
            .and_then(|b| b.try_into().ok())
            .unwrap_or_else(|| self.hash32())
    }

    fn hash32(&mut self) -> [u8; 32] {
        self.rng.bytes(32).try_into().expect("32 bytes")
    }

    fn pool_id(&mut self) -> Value {
        let id = stellar_xdr::PoolId(Hash(self.pool_hash()));
        serde_json::to_value(id).expect("a pool id serializes")
    }

    fn contract_hash(&mut self) -> [u8; 32] {
        let known: Vec<[u8; 32]> = self.world.contracts.iter().copied().collect();
        if !known.is_empty() && self.rng.chance(4, 5) {
            *self.rng.pick(&known)
        } else {
            self.hash32()
        }
    }

    fn contract_id(&mut self) -> Value {
        let id = stellar_xdr::ContractId(Hash(self.contract_hash()));
        serde_json::to_value(id).expect("a contract id serializes")
    }

    fn sc_address(&mut self) -> Value {
        use stellar_xdr::{MuxedEd25519Account, ScAddress};
        let a = match self.rng.below(9) {
            0 | 1 => ScAddress::Contract(stellar_xdr::ContractId(Hash(self.contract_hash()))),
            2 => {
                let role = self.role();
                let key = derive_key(&self.world.recipe_id, &role);
                ScAddress::MuxedAccount(MuxedEd25519Account {
                    id: *self.rng.pick(&[0u64, 1, u64::MAX]),
                    ed25519: Uint256(key.verifying_key().to_bytes()),
                })
            }
            3 => {
                let v = self.balance_id();
                match serde_json::from_value(v) {
                    Ok(id) => ScAddress::ClaimableBalance(id),
                    Err(_) => ScAddress::Contract(stellar_xdr::ContractId(Hash(self.hash32()))),
                }
            }
            4 => ScAddress::LiquidityPool(stellar_xdr::PoolId(Hash(self.pool_hash()))),
            // a muxed contract's strkey is not one a deploy label names, so its contract is
            // a random one: a deployed contract would tie the recipe to one passphrase
            5 => ScAddress::MuxedContract(stellar_xdr::MuxedContract {
                id: *self.rng.pick(&[0u64, 1, u64::MAX]),
                contract_id: stellar_xdr::ContractId(Hash(self.hash32())),
            }),
            _ => {
                let role = self.role();
                let key = derive_key(&self.world.recipe_id, &role);
                ScAddress::Account(stellar_xdr::AccountId(
                    stellar_xdr::PublicKey::PublicKeyTypeEd25519(Uint256(
                        key.verifying_key().to_bytes(),
                    )),
                ))
            }
        };
        self.world
            .recipe_form(serde_json::to_value(a).expect("an address serializes"))
    }

    /// 128 and 256 bit integers, as their parts: zero, one, minus one, the extremes.
    fn wide_int(&mut self, name: &str) -> Value {
        use stellar_xdr::{Int128Parts, Int256Parts, UInt128Parts, UInt256Parts};
        let small: u64 = *self.rng.pick(&[0, 1, 7, 10_000_000]);
        let pick = self.rng.below(4);
        let v = match name {
            "Int128Parts" => serde_json::to_value(match pick {
                0 => Int128Parts { hi: 0, lo: small },
                1 => Int128Parts {
                    hi: -1,
                    lo: u64::MAX,
                },
                2 => Int128Parts {
                    hi: i64::MAX,
                    lo: u64::MAX,
                },
                _ => Int128Parts {
                    hi: i64::MIN,
                    lo: 0,
                },
            }),
            "UInt128Parts" => serde_json::to_value(match pick {
                0 | 1 => UInt128Parts { hi: 0, lo: small },
                _ => UInt128Parts {
                    hi: u64::MAX,
                    lo: u64::MAX,
                },
            }),
            "Int256Parts" => serde_json::to_value(match pick {
                0 => Int256Parts {
                    hi_hi: 0,
                    hi_lo: 0,
                    lo_hi: 0,
                    lo_lo: small,
                },
                1 => Int256Parts {
                    hi_hi: -1,
                    hi_lo: u64::MAX,
                    lo_hi: u64::MAX,
                    lo_lo: u64::MAX,
                },
                2 => Int256Parts {
                    hi_hi: i64::MAX,
                    hi_lo: u64::MAX,
                    lo_hi: u64::MAX,
                    lo_lo: u64::MAX,
                },
                _ => Int256Parts {
                    hi_hi: i64::MIN,
                    hi_lo: 0,
                    lo_hi: 0,
                    lo_lo: 0,
                },
            }),
            _ => serde_json::to_value(match pick {
                0 | 1 => UInt256Parts {
                    hi_hi: 0,
                    hi_lo: 0,
                    lo_hi: 0,
                    lo_lo: small,
                },
                _ => UInt256Parts {
                    hi_hi: u64::MAX,
                    hi_lo: u64::MAX,
                    lo_hi: u64::MAX,
                    lo_lo: u64::MAX,
                },
            }),
        };
        v.expect("an integer serializes")
    }

    fn balance_id(&mut self) -> Value {
        let known: Vec<String> = self.world.balances.iter().cloned().collect();
        if !known.is_empty() && self.rng.chance(4, 5) {
            let text = self.rng.pick(&known).clone();
            return serde_json::from_str(&text).expect("balance ids are stored as json");
        }
        let id = stellar_xdr::ClaimableBalanceId::ClaimableBalanceIdTypeV0(Hash(
            self.rng.bytes(32).try_into().expect("32 bytes"),
        ));
        serde_json::to_value(id).expect("a claimable balance id serializes")
    }

    fn signer_key(&mut self) -> Value {
        match self.rng.below(6) {
            0 => serde_json::to_value(SignerKey::PreAuthTx(Uint256(
                self.rng.bytes(32).try_into().expect("32 bytes"),
            )))
            .expect("a signer key serializes"),
            1 => serde_json::to_value(SignerKey::HashX(Uint256(
                self.rng.bytes(32).try_into().expect("32 bytes"),
            )))
            .expect("a signer key serializes"),
            2 => {
                let role = self.role();
                let key = derive_key(&self.world.recipe_id, &role);
                let len = *self.rng.pick(&[1usize, 32, 64]);
                let payload = self.rng.bytes(len);
                serde_json::to_value(SignerKey::Ed25519SignedPayload(
                    SignerKeyEd25519SignedPayload {
                        ed25519: Uint256(key.verifying_key().to_bytes()),
                        payload: payload.try_into().expect("at most 64 bytes"),
                    },
                ))
                .expect("a signer key serializes")
            }
            _ => Value::String(format!("@account:{}", self.role())),
        }
    }

    fn ledger_key(&mut self) -> Option<Value> {
        let keys: Vec<Value> = self.world.keys.values().cloned().collect();
        if keys.is_empty() || self.rng.chance(1, 6) {
            return None;
        }
        Some(self.rng.pick(&keys).clone())
    }

    fn int64(&mut self, field: Option<&str>) -> Value {
        let now = i64::try_from(self.now()).unwrap_or(i64::MAX);
        let v: i64 = match field {
            Some("offerID") => {
                let known: Vec<i64> = self.world.offers.iter().map(|(_, id)| *id).collect();
                // zero makes a new offer, the commonest useful value
                if self.rng.chance(3, 5) {
                    0
                } else if !known.is_empty() && self.rng.chance(2, 3) {
                    *self.rng.pick(&known)
                } else {
                    *self.rng.pick(&[1, 99_999, -1])
                }
            }
            Some("absBefore") => *self.rng.pick(&[0, now - 1, now + 864_000, i64::MAX]),
            Some("relBefore") => *self.rng.pick(&[0, 1, 864_000, i64::MAX, -1]),
            Some("startingBalance") => *self.rng.pick(&[
                0,
                1,
                5_000_000,
                10_000_000,
                20_000_000,
                1_000_000_000,
                100_000_000_000,
                i64::MAX,
                -1,
            ]),
            Some("limit") => *self
                .rng
                .pick(&[0, 1, 1000, 10_000_000_000, i64::MAX, i64::MAX, -1]),
            // an amount the ledgers hold, or one stroop either side of it
            _ if !self.world.amounts.is_empty() && self.rng.chance(2, 5) => {
                let v = *self.rng.pick(&self.world.amounts);
                match self.rng.below(4) {
                    0 => v.saturating_add(1),
                    1 => v.saturating_sub(1),
                    2 => v / 2,
                    _ => v,
                }
            }
            _ => *self.rng.pick(AMOUNTS),
        };
        Value::String(v.to_string())
    }

    fn sequence(&mut self) -> Value {
        let role = if self.rng.chance(3, 4) {
            self.source.clone()
        } else {
            self.role()
        };
        Value::String(match self.rng.below(9) {
            0 => format!("@seq:{role}"),
            1 => format!("@seq:{role}+1"),
            2 => format!("@seq:{role}+5"),
            3 => format!("@seq:{role}+1048576"),
            4 => "@ledger-start:3".into(),
            5 => "1".into(),
            6 => "0".into(),
            7 => "-1".into(),
            _ => i64::MAX.to_string(),
        })
    }

    fn uint32(&mut self, field: Option<&str>, signed: bool) -> Value {
        let v: i64 = match field {
            Some("setFlags" | "clearFlags") => {
                let mut f = 0;
                for bit in [1, 2, 4, 8] {
                    if self.rng.chance(1, 3) {
                        f |= bit;
                    }
                }
                if self.rng.chance(1, 20) {
                    f |= 16;
                }
                f
            }
            Some("masterWeight" | "lowThreshold" | "medThreshold" | "highThreshold" | "weight") => {
                *self.rng.pick(&[0, 1, 1, 1, 2, 10, 255, 256])
            }
            Some("authorize") => *self.rng.pick(&[0, 1, 2, 3]),
            Some("fee") => *self.rng.pick(&[30, 30, 30, 0, 100]),
            Some("n" | "d") => *self
                .rng
                .pick(&[1, 1, 1, 2, 3, 7, 0, -1, i64::from(i32::MAX)]),
            Some("minSeqLedgerGap") => *self.rng.pick(&[0, 1, 5]),
            Some("minLedger" | "maxLedger") => {
                let now = i64::from(self.world.last_closed) + 1;
                *self.rng.pick(&[0, now, now + 1, now + 10, now - 1])
            }
            _ => *self.rng.pick(&[0, 1, 2, 3, 7, 100]),
        };
        let v = if signed {
            v.clamp(i64::from(i32::MIN), i64::from(i32::MAX))
        } else {
            v.clamp(0, i64::from(u32::MAX))
        };
        json!(v)
    }

    fn uint64(&mut self, field: Option<&str>) -> Value {
        let now = self.now();
        let v: u64 = match field {
            Some("minTime") => *self.rng.pick(&[0, 0, now - 172_800, now + 864_000]),
            Some("maxTime") => *self
                .rng
                .pick(&[0, 0, now + 864_000, u64::MAX, now - 172_800]),
            Some("minSeqAge") => *self.rng.pick(&[0, 1, 8_640_000]),
            _ => *self.rng.pick(&[0, 1, 1000, u64::MAX]),
        };
        Value::String(v.to_string())
    }

    fn string(&mut self, field: Option<&str>, max: u64) -> Value {
        let long: String = "a".repeat(max.min(64) as usize);
        let pool: Vec<String> = match field {
            Some("dataName") => vec!["k".into(), "key1".into(), "key2".into(), long],
            Some("homeDomain") => vec![String::new(), "example.com".into(), long],
            _ => vec![String::new(), "x".into(), "shapekit".into(), long],
        };
        let mut s = self.rng.pick(&pool).clone();
        s.truncate(max as usize);
        Value::String(s)
    }

    fn opaque(&mut self, len: usize) -> Value {
        Value::String(hex::encode(self.rng.bytes(len)))
    }

    /// A value of `ty`, the type of `field` (an XDR field or arm name) when known.
    fn typed(&mut self, ty: &Type, field: Option<&str>) -> Value {
        match ty {
            Type::Int => self.uint32(field, true),
            Type::UnsignedInt => self.uint32(field, false),
            Type::Hyper => self.int64(field),
            Type::UnsignedHyper => self.uint64(field),
            Type::Float | Type::Double => json!(0.0),
            Type::Bool => Value::Bool(self.rng.chance(1, 2)),
            Type::OpaqueFixed { size } => self.opaque(*size as usize),
            Type::OpaqueVar { max_size } => {
                let max = max_size.unwrap_or(64) as usize;
                let len = *self.rng.pick(&[0, 1, max.min(32), max.min(64)]);
                self.opaque(len)
            }
            Type::String { max_size } => self.string(field, max_size.unwrap_or(32)),
            Type::Optional { element } => {
                // most optional fields stay out: a struct of many (set_options) filled at
                // once mostly locks the account out instead of trying one change
                if self.depth >= MAX_DEPTH || self.rng.chance(7, 10) {
                    Value::Null
                } else {
                    self.nested(element, field)
                }
            }
            Type::Array { element, count } => {
                Value::Array((0..*count).map(|_| self.nested(element, field)).collect())
            }
            Type::VarArray { element, max_count } => {
                let max = max_count.unwrap_or(3).min(3) as usize;
                // one element half the time, since most lists need at least one to be valid;
                // a list bounded at two (the and/or predicates) is full half the time instead
                let n = if self.depth >= MAX_DEPTH {
                    0
                } else if max_count == &Some(2) && self.rng.chance(1, 2) {
                    2
                } else if max >= 1 && self.rng.chance(1, 2) {
                    1
                } else {
                    self.rng.below(max + 1)
                };
                Value::Array((0..n).map(|_| self.nested(element, field)).collect())
            }
            Type::Ref { name } => self.named_def(name, field),
        }
    }

    fn named_def(&mut self, name: &str, field: Option<&str>) -> Value {
        match name {
            "AccountID" | "NodeID" | "PublicKey" => return self.account(),
            "MuxedAccount" => return self.muxed(),
            "Asset" => return self.asset_ref().json(),
            "ChangeTrustAsset" => {
                return match self.rng.below(20) {
                    0..=3 => json!({ "pool_share": self.pool_share() }),
                    4 => AssetRef::native().json(),
                    _ => self.credit_ref().json(),
                };
            }
            "TrustLineAsset" => {
                return if self.rng.chance(1, 5) {
                    json!({ "pool_share": self.pool_id() })
                } else {
                    self.asset_ref().json()
                };
            }
            "AssetCode" => {
                let code = self.credit_ref().code.unwrap_or_else(|| "USD".into());
                return serde_json::to_value(code_xdr(&code)).expect("an asset code serializes");
            }
            "AssetCode4" => return Value::String(self.rng.pick(CODES4).to_string()),
            "AssetCode12" => return Value::String(self.rng.pick(CODES12).to_string()),
            "PoolID" => return self.pool_id(),
            "ContractID" => return self.contract_id(),
            "SCAddress" => return self.sc_address(),
            "Int128Parts" | "UInt128Parts" | "Int256Parts" | "UInt256Parts" => {
                return self.wide_int(name);
            }
            "ClaimableBalanceID" => return self.balance_id(),
            "SignerKey" => return self.signer_key(),
            "SequenceNumber" => return self.sequence(),
            "LedgerKey" => {
                if let Some(k) = self.ledger_key() {
                    return k;
                }
            }
            _ => {}
        }
        let Some(def) = self.schema.get(name).cloned() else {
            // the schema is checked to have no dangling references
            unreachable!("{name} is not defined")
        };
        match def {
            Definition::Typedef(t) => self.typed(&t.ty, field),
            Definition::Struct(s) => {
                let mut map = serde_json::Map::new();
                for f in &s.fields {
                    let v = self.nested(&f.ty, Some(&f.name));
                    map.insert(field_name(&f.name), v);
                }
                Value::Object(map)
            }
            Definition::Union(u) => {
                let cases: Vec<(String, Option<Type>)> = u
                    .arms
                    .iter()
                    .flat_map(|a| {
                        a.cases
                            .iter()
                            .map(|c| (case_name(self.schema, &u, c), a.ty.clone()))
                            .collect::<Vec<_>>()
                    })
                    .collect();
                // past the depth bound, a void arm ends a recursive union when it has one
                let pick = if self.depth >= MAX_DEPTH {
                    cases
                        .iter()
                        .position(|(_, t)| t.is_none())
                        .unwrap_or_else(|| self.rng.below(cases.len()))
                } else {
                    self.rng.below(cases.len())
                };
                let (arm, ty) = cases[pick].clone();
                self.arm_value(&u, &arm, ty.as_ref())
            }
            Definition::Enum(e) => {
                let m = self.rng.pick(&e.members).clone();
                Value::String(member_name(&m.name, &e.member_prefix))
            }
            Definition::Const(_) => unreachable!("a constant is not a type"),
        }
    }
}

fn code_xdr(code: &str) -> stellar_xdr::AssetCode {
    if code.len() <= 4 {
        let mut b = [0u8; 4];
        b[..code.len()].copy_from_slice(code.as_bytes());
        stellar_xdr::AssetCode::CreditAlphanum4(stellar_xdr::AssetCode4(b))
    } else {
        let mut b = [0u8; 12];
        b[..code.len()].copy_from_slice(code.as_bytes());
        stellar_xdr::AssetCode::CreditAlphanum12(stellar_xdr::AssetCode12(b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shapekit_core_driver::keys::account_strkey;
    use std::collections::BTreeMap;

    fn world() -> World {
        let mut w = World::new("explore.test", "Test SDF Network ; September 2015", 30);
        for r in ["a", "b", "c"] {
            w.add_role(r);
        }
        w.issuers.push("a".into());
        w.last_closed = 70;
        w
    }

    /// Replaces references the way the interpreter does, with fixed values.
    fn resolve(v: &Value, w: &World) -> Value {
        match v {
            Value::String(s) if s.starts_with("@@") => Value::String(s[1..].to_owned()),
            Value::String(s) if s.starts_with('@') => {
                let (kind, rest) = s[1..].split_once(':').unwrap();
                Value::String(match kind {
                    "account" => {
                        if rest == "root" {
                            account_strkey(
                                &shapekit_core_driver::keys::root_key("x").verifying_key(),
                            )
                        } else {
                            account_strkey(&derive_key(&w.recipe_id, rest).verifying_key())
                        }
                    }
                    "muxed" => {
                        let (role, id) = rest.split_once(':').unwrap();
                        shapekit_core_driver::keys::muxed_strkey(
                            &derive_key(&w.recipe_id, role).verifying_key(),
                            id.parse().unwrap(),
                        )
                    }
                    "seq" => "5".into(),
                    "ledger-start" => "300647710720".into(),
                    other => panic!("unexpected reference {other}"),
                })
            }
            Value::Array(a) => Value::Array(a.iter().map(|x| resolve(x, w)).collect()),
            Value::Object(m) => {
                Value::Object(m.iter().map(|(k, x)| (k.clone(), resolve(x, w))).collect())
            }
            other => other.clone(),
        }
    }

    #[test]
    fn every_operation_arm_fills_to_valid_xdr_json() {
        let schema = Schema::pinned();
        let w = world();
        let mut rng = Rng::new(11);
        let mut failures: BTreeMap<String, String> = BTreeMap::new();
        for arm in Filler::arm_names(&schema, "OperationBody") {
            for _ in 0..200 {
                let mut f = Filler::new(&schema, &w, &mut rng, "a");
                let body = f.union_arm("OperationBody", &arm).unwrap();
                let op = json!({"source_account": null, "body": body});
                let resolved = resolve(&op, &w);
                if let Err(e) = serde_json::from_value::<stellar_xdr::Operation>(resolved.clone()) {
                    failures.insert(arm.clone(), format!("{e}: {resolved}"));
                }
            }
        }
        for name in [
            "Memo",
            "Preconditions",
            "SCVal",
            "LedgerUpgrade",
            "SorobanAuthorizationEntry",
        ] {
            for _ in 0..200 {
                let mut f = Filler::new(&schema, &w, &mut rng, "a");
                let v = resolve(&f.def(name), &w);
                let variant: stellar_xdr::TypeVariant =
                    shapekit_schema::names::rust_type_name(name)
                        .parse()
                        .unwrap();
                let r = stellar_xdr::Type::from_json(variant, v.to_string().as_bytes());
                if let Err(e) = r {
                    failures.insert(name.to_owned(), format!("{e}: {v}"));
                }
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }
}
