//! What the explorer knows about the chain it drives: the roles of the recipe, the assets
//! they issue, and the offers, claimable balances, liquidity pools and ledger entries the
//! ledgers so far have shown. Values are kept as XDR-JSON in recipe form, with every
//! account written as a symbolic reference, so whatever the filler takes from here is
//! independent of the network passphrase.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use serde_json::Value;
use sha2::{Digest, Sha256};
use shapekit_core_driver::canonical::{change_key, change_lists};
use shapekit_core_driver::keys::{account_strkey, derive_key, root_key};
use stellar_xdr::{
    AccountId, AlphaNum4, AlphaNum12, Asset, AssetCode4, AssetCode12, LedgerCloseMeta,
    LedgerEntryChange, LedgerEntryData, LedgerKey, Limits, LiquidityPoolConstantProductParameters,
    LiquidityPoolEntryBody, LiquidityPoolParameters, PublicKey, Uint256, WriteXdr,
};

/// Asset codes the filler draws from: two four-character codes and one long code.
pub const CODES4: &[&str] = &["USD", "EUR"];
pub const CODES12: &[&str] = &["LONGASSET12"];

/// Amounts kept for the filler to reuse.
const MAX_AMOUNTS: usize = 64;

/// Core's only liquidity pool fee (`LIQUIDITY_POOL_FEE_V18`, 30 basis points).
pub const POOL_FEE: i32 = 30;

/// A credit or native asset in recipe form, and the issuer role when it has one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AssetRef {
    pub code: Option<String>,
    pub issuer: Option<String>,
}

impl AssetRef {
    pub fn native() -> Self {
        Self {
            code: None,
            issuer: None,
        }
    }

    /// XDR-JSON in recipe form (the issuer as `@account:<role>`).
    pub fn json(&self) -> Value {
        match (&self.code, &self.issuer) {
            (Some(code), Some(issuer)) => {
                let arm = if code.len() <= 4 {
                    "credit_alphanum4"
                } else {
                    "credit_alphanum12"
                };
                serde_json::json!({ arm: { "asset_code": code, "issuer": format!("@account:{issuer}") } })
            }
            _ => Value::String("native".into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct World {
    pub recipe_id: String,
    /// Roles a fund step created, in funding order.
    pub funded: Vec<String>,
    /// Roles whose assets the filler uses.
    pub issuers: Vec<String>,
    /// Accounts that exist on no chain of this recipe, as literal strkeys.
    pub ghosts: Vec<String>,
    /// Account strkey to role, the network's root included.
    pub roles_by_strkey: BTreeMap<String, String>,
    /// (seller reference, offer id) of every live offer.
    pub offers: BTreeSet<(String, i64)>,
    /// Live claimable balance ids, XDR-JSON.
    pub balances: BTreeSet<String>,
    /// Live liquidity pools by hex id, with their two assets as XDR-JSON.
    pub pools: BTreeMap<String, (Value, Value)>,
    /// Amounts the ledgers hold (balances, limits, offer and claimable balance amounts, pool
    /// reserves), the latest last.
    pub amounts: Vec<i64>,
    /// Credit assets some account holds a trustline to.
    pub trusted: BTreeSet<AssetRef>,
    /// Contract strkey to the deploy label that created it.
    pub contract_labels: BTreeMap<String, String>,
    /// Live Soroban ledger entries' keys (contract data and code), XDR-JSON in recipe form;
    /// only keys whose contracts a deploy label names.
    pub soroban_keys: BTreeMap<String, Value>,
    /// Contracts the chain holds an instance of.
    pub contracts: BTreeSet<[u8; 32]>,
    /// Live classic ledger entries' keys, XDR-JSON in recipe form.
    pub keys: BTreeMap<String, Value>,
    pub last_closed: u32,
    pub protocol: u32,
    /// The ledger's base fee, from the last header.
    pub base_fee: u32,
}

impl World {
    pub fn new(recipe_id: &str, passphrase: &str, protocol: u32) -> Self {
        let mut roles_by_strkey = BTreeMap::new();
        roles_by_strkey.insert(
            account_strkey(&root_key(passphrase).verifying_key()),
            "root".to_owned(),
        );
        let ghosts = (0..2)
            .map(|i| account_strkey(&derive_key(recipe_id, &format!("ghost-{i}")).verifying_key()))
            .collect();
        Self {
            recipe_id: recipe_id.to_owned(),
            funded: Vec::new(),
            issuers: Vec::new(),
            ghosts,
            roles_by_strkey,
            offers: BTreeSet::new(),
            balances: BTreeSet::new(),
            pools: BTreeMap::new(),
            amounts: Vec::new(),
            trusted: BTreeSet::new(),
            contract_labels: BTreeMap::new(),
            soroban_keys: BTreeMap::new(),
            contracts: BTreeSet::new(),
            keys: BTreeMap::new(),
            last_closed: 0,
            protocol,
            base_fee: 100,
        }
    }

    pub fn add_role(&mut self, role: &str) {
        let strkey = account_strkey(&derive_key(&self.recipe_id, role).verifying_key());
        self.roles_by_strkey.insert(strkey, role.to_owned());
        self.funded.push(role.to_owned());
    }

    /// Every credit asset the issuers issue under the code pools, and native.
    pub fn assets(&self) -> Vec<AssetRef> {
        let mut out = vec![AssetRef::native()];
        for issuer in &self.issuers {
            for code in CODES4.iter().chain(CODES12) {
                out.push(AssetRef {
                    code: Some((*code).to_owned()),
                    issuer: Some(issuer.clone()),
                });
            }
        }
        out
    }

    /// The stellar-xdr asset an [`AssetRef`] names on this recipe's chains.
    pub fn xdr_asset(&self, a: &AssetRef) -> Result<Asset> {
        Ok(match (&a.code, &a.issuer) {
            (Some(code), Some(issuer)) => {
                let key = if issuer == "root" {
                    anyhow::bail!("root issues no asset")
                } else {
                    derive_key(&self.recipe_id, issuer)
                };
                let issuer = AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(
                    key.verifying_key().to_bytes(),
                )));
                if code.len() <= 4 {
                    let mut b = [0u8; 4];
                    b[..code.len()].copy_from_slice(code.as_bytes());
                    Asset::CreditAlphanum4(AlphaNum4 {
                        asset_code: AssetCode4(b),
                        issuer,
                    })
                } else {
                    let mut b = [0u8; 12];
                    b[..code.len()].copy_from_slice(code.as_bytes());
                    Asset::CreditAlphanum12(AlphaNum12 {
                        asset_code: AssetCode12(b),
                        issuer,
                    })
                }
            }
            _ => Asset::Native,
        })
    }

    /// The pool of two assets in core's order, with its id: sha256 of the parameters' XDR
    /// (CAP-38). `None` when the two are equal.
    pub fn pool(&self, a: &AssetRef, b: &AssetRef) -> Result<Option<(String, Value)>> {
        let (xa, xb) = (self.xdr_asset(a)?, self.xdr_asset(b)?);
        if xa == xb {
            return Ok(None);
        }
        let ((xa, ja), (xb, jb)) = if xa < xb {
            ((xa, a.json()), (xb, b.json()))
        } else {
            ((xb, b.json()), (xa, a.json()))
        };
        let params = LiquidityPoolParameters::LiquidityPoolConstantProduct(
            LiquidityPoolConstantProductParameters {
                asset_a: xa,
                asset_b: xb,
                fee: POOL_FEE,
            },
        );
        let id = hex::encode(Sha256::digest(params.to_xdr(Limits::none())?));
        let json = serde_json::json!({
            "liquidity_pool_constant_product": { "asset_a": ja, "asset_b": jb, "fee": POOL_FEE }
        });
        Ok(Some((id, json)))
    }

    /// Writes `value` in recipe form: account strkeys of known roles become references, and
    /// strings that would read as a reference are escaped.
    pub fn recipe_form(&self, value: Value) -> Value {
        match value {
            Value::String(s) => match self.roles_by_strkey.get(&s) {
                Some(role) => Value::String(format!("@account:{role}")),
                None if self.contract_labels.contains_key(&s) => {
                    Value::String(format!("@contract:{}", self.contract_labels[&s]))
                }
                None if s.starts_with('@') => Value::String(format!("@{s}")),
                None => Value::String(s),
            },
            Value::Array(items) => {
                Value::Array(items.into_iter().map(|v| self.recipe_form(v)).collect())
            }
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(k, v)| (k, self.recipe_form(v)))
                    .collect(),
            ),
            other => other,
        }
    }

    fn note_amounts(&mut self, data: &LedgerEntryData) {
        let found: Vec<i64> = match data {
            LedgerEntryData::Account(a) => vec![a.balance],
            LedgerEntryData::Trustline(t) => vec![t.balance, t.limit],
            LedgerEntryData::Offer(o) => vec![o.amount],
            LedgerEntryData::ClaimableBalance(b) => vec![b.amount],
            LedgerEntryData::LiquidityPool(p) => {
                let LiquidityPoolEntryBody::LiquidityPoolConstantProduct(cp) = &p.body;
                vec![cp.reserve_a, cp.reserve_b, cp.total_pool_shares]
            }
            _ => Vec::new(),
        };
        for v in found {
            self.amounts.retain(|x| *x != v);
            self.amounts.push(v);
        }
        if self.amounts.len() > MAX_AMOUNTS {
            let extra = self.amounts.len() - MAX_AMOUNTS;
            self.amounts.drain(..extra);
        }
    }

    /// Takes in what a closed ledger changed.
    pub fn observe(&mut self, lcm: &LedgerCloseMeta, seq: u32) -> Result<()> {
        self.last_closed = seq;
        self.base_fee = match lcm {
            LedgerCloseMeta::V0(m) => m.ledger_header.header.base_fee,
            LedgerCloseMeta::V1(m) => m.ledger_header.header.base_fee,
            LedgerCloseMeta::V2(m) => m.ledger_header.header.base_fee,
        };
        for list in change_lists(lcm) {
            for c in list.iter() {
                let key = change_key(c);
                if matches!(key, LedgerKey::Ttl(_) | LedgerKey::ConfigSetting(_)) {
                    continue;
                }
                if matches!(key, LedgerKey::ContractData(_) | LedgerKey::ContractCode(_)) {
                    let json = self.recipe_form(serde_json::to_value(&key)?);
                    let text = serde_json::to_string(&json)?;
                    match c {
                        LedgerEntryChange::Removed(_) => {
                            self.soroban_keys.remove(&text);
                        }
                        // a contract no deploy label names would make the key passphrase
                        // dependent
                        _ if !names_raw_contract(&json) => {
                            self.soroban_keys.insert(text, json);
                        }
                        _ => {}
                    }
                    continue;
                }
                let key_json = self.recipe_form(serde_json::to_value(&key)?);
                let key_text = serde_json::to_string(&key_json)?;
                match c {
                    LedgerEntryChange::Removed(_) => {
                        self.keys.remove(&key_text);
                        match &key {
                            LedgerKey::Account(a) => {
                                let id = self.recipe_form(serde_json::to_value(&a.account_id)?);
                                if let Some(role) =
                                    id.as_str().and_then(|s| s.strip_prefix("@account:"))
                                {
                                    self.funded.retain(|r| r != role);
                                    self.issuers.retain(|r| r != role);
                                }
                            }
                            LedgerKey::Offer(o) => {
                                let seller = self.recipe_form(serde_json::to_value(&o.seller_id)?);
                                if let Value::String(s) = seller {
                                    self.offers.remove(&(s, o.offer_id));
                                }
                            }
                            LedgerKey::ClaimableBalance(b) => {
                                self.balances.remove(&serde_json::to_string(
                                    &serde_json::to_value(&b.balance_id)?,
                                )?);
                            }
                            LedgerKey::LiquidityPool(p) => {
                                self.pools.remove(&hex::encode(p.liquidity_pool_id.0.0));
                            }
                            _ => {}
                        }
                    }
                    LedgerEntryChange::State(e)
                    | LedgerEntryChange::Created(e)
                    | LedgerEntryChange::Updated(e)
                    | LedgerEntryChange::Restored(e) => {
                        self.keys.insert(key_text, key_json);
                        self.note_amounts(&e.data);
                        match &e.data {
                            LedgerEntryData::Trustline(t) => {
                                let asset = self.recipe_form(serde_json::to_value(&t.asset)?);
                                if let Some(a) = asset_ref_of(&asset) {
                                    self.trusted.insert(a);
                                }
                            }
                            LedgerEntryData::Offer(o) => {
                                let seller = self.recipe_form(serde_json::to_value(&o.seller_id)?);
                                if let Value::String(s) = seller {
                                    self.offers.insert((s, o.offer_id));
                                }
                            }
                            LedgerEntryData::ClaimableBalance(b) => {
                                self.balances.insert(serde_json::to_string(
                                    &serde_json::to_value(&b.balance_id)?,
                                )?);
                            }
                            LedgerEntryData::LiquidityPool(p) => {
                                let LiquidityPoolEntryBody::LiquidityPoolConstantProduct(cp) =
                                    &p.body;
                                let a = self.recipe_form(serde_json::to_value(&cp.params.asset_a)?);
                                let b = self.recipe_form(serde_json::to_value(&cp.params.asset_b)?);
                                self.pools
                                    .insert(hex::encode(p.liquidity_pool_id.0.0), (a, b));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// Whether a value in recipe form still holds a contract strkey.
fn names_raw_contract(v: &Value) -> bool {
    match v {
        Value::String(s) => s.len() == 56 && s.starts_with('C'),
        Value::Array(a) => a.iter().any(names_raw_contract),
        Value::Object(m) => m.values().any(names_raw_contract),
        _ => false,
    }
}

/// The [`AssetRef`] of a credit asset in recipe form, when its issuer is a role.
fn asset_ref_of(v: &Value) -> Option<AssetRef> {
    let (_, inner) = v.as_object()?.iter().next()?;
    let code = inner.get("asset_code")?.as_str()?;
    let issuer = inner.get("issuer")?.as_str()?.strip_prefix("@account:")?;
    Some(AssetRef {
        code: Some(code.to_owned()),
        issuer: Some(issuer.to_owned()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_ids_follow_cap38_and_assets_are_ordered() {
        let mut w = World::new("explore.test", "Test SDF Network ; September 2015", 30);
        w.add_role("a");
        w.issuers.push("a".into());
        let usd = AssetRef {
            code: Some("USD".into()),
            issuer: Some("a".into()),
        };
        let (id1, p1) = w.pool(&usd, &AssetRef::native()).unwrap().unwrap();
        let (id2, p2) = w.pool(&AssetRef::native(), &usd).unwrap().unwrap();
        assert_eq!(id1, id2);
        assert_eq!(p1, p2);
        assert_eq!(
            p1["liquidity_pool_constant_product"]["asset_a"],
            Value::String("native".into())
        );
        assert!(w.pool(&usd, &usd).unwrap().is_none());
        // the json form decodes to the parameters the id was hashed from
        let resolved = serde_json::to_string(&p1).unwrap().replace(
            "@account:a",
            &account_strkey(&derive_key("explore.test", "a").verifying_key()),
        );
        let params: stellar_xdr::ChangeTrustAsset =
            serde_json::from_str(&format!("{{\"pool_share\":{resolved}}}")).unwrap();
        let stellar_xdr::ChangeTrustAsset::PoolShare(params) = params else {
            panic!("not a pool share")
        };
        assert_eq!(
            hex::encode(Sha256::digest(params.to_xdr(Limits::none()).unwrap())),
            id1
        );
    }

    #[test]
    fn recipe_form_names_roles_and_escapes_at_signs() {
        let mut w = World::new("explore.test", "Test SDF Network ; September 2015", 30);
        w.add_role("b");
        let b = account_strkey(&derive_key("explore.test", "b").verifying_key());
        let v = w.recipe_form(serde_json::json!({"x": [b, "@odd", "plain"]}));
        assert_eq!(
            v,
            serde_json::json!({"x": ["@account:b", "@@odd", "plain"]})
        );
    }
}
