//! The identifiers and amounts a chain hands the target, each with the encodings a store is
//! known to keep them in. Only what the ledgers themselves carry, or what follows from them
//! and the passphrase alone, counts as known.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shapekit_oracle::decode::LcmStream;
use shapekit_oracle::network::Network;
use stellar_strkey::Strkey;
use stellar_xdr::Asset;

/// How a known value is written in a store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Encoding {
    /// The strkey text, `G...`, `M...`, `P...`.
    Strkey,
    /// Lowercase hex of the strkey's version byte followed by its payload (wallet-backend's
    /// `AddressBytea`).
    VersionedHex,
    /// Lowercase hex of the strkey's payload alone (a 32-byte key for `G` and `C`).
    PayloadHex,
    /// A decimal integer.
    Decimal,
}

impl Encoding {
    pub fn name(self) -> &'static str {
        match self {
            Encoding::Strkey => "strkey",
            Encoding::VersionedHex => "versioned-hex",
            Encoding::PayloadHex => "payload-hex",
            Encoding::Decimal => "decimal",
        }
    }
}

/// One known value in one encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Form {
    /// The value it stands for: a strkey, or a decimal amount.
    pub value: String,
    pub encoding: Encoding,
    /// Whether `value` is an address (otherwise an amount).
    pub address: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Known {
    /// Encoded text to what it encodes. Addresses only win over amounts on a clash, which
    /// cannot happen between hex and decimal of the lengths involved.
    pub forms: BTreeMap<String, Form>,
    pub addresses: BTreeSet<String>,
    pub amounts: BTreeSet<String>,
}

/// JSON members of the ledger whose integer values are amounts a store keeps.
const AMOUNT_KEYS: &[&str] = &[
    "amount",
    "balance",
    "limit",
    "starting_balance",
    "send_amount",
    "send_max",
    "dest_amount",
    "dest_min",
    "fee_charged",
    "buying_liabilities",
    "selling_liabilities",
    "max_amount_a",
    "max_amount_b",
    "min_amount_a",
    "min_amount_b",
    "reserve_a",
    "reserve_b",
    "total_pool_shares",
    "i128",
    "u128",
    "i64",
    "u64",
];

/// The bytes a strkey stands for: version byte then payload, without the checksum.
pub fn strkey_bytes(s: &str) -> Option<Vec<u8>> {
    Strkey::from_string(s).ok()?;
    let mut bits = 0u32;
    let mut acc = 0u32;
    let mut out = Vec::with_capacity(s.len() * 5 / 8);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'2'..=b'7' => c - b'2' + 26,
            _ => return None,
        };
        acc = (acc << 5) | u32::from(v);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    if out.len() < 3 {
        return None;
    }
    out.truncate(out.len() - 2);
    Some(out)
}

/// The base `G` account of a muxed account or a signed-payload signer: a store that drops
/// the mux id or the payload writes this one, which is a reduction, not an invention.
fn base_account(s: &str) -> Option<String> {
    match Strkey::from_string(s).ok()? {
        Strkey::MuxedAccountEd25519(m) => Some(
            Strkey::PublicKeyEd25519(stellar_strkey::ed25519::PublicKey(m.ed25519))
                .to_string()
                .to_string(),
        ),
        Strkey::SignedPayloadEd25519(p) => Some(
            Strkey::PublicKeyEd25519(stellar_strkey::ed25519::PublicKey(*p.ed25519()))
                .to_string()
                .to_string(),
        ),
        Strkey::MuxedContract(m) => Some(
            Strkey::Contract(stellar_strkey::Contract(m.contract_id))
                .to_string()
                .to_string(),
        ),
        _ => None,
    }
}

fn is_decimal(s: &str) -> bool {
    let d = s.strip_prefix('-').unwrap_or(s);
    !d.is_empty() && d.len() <= 40 && d.bytes().all(|b| b.is_ascii_digit())
}

impl Known {
    pub fn add_address(&mut self, s: &str) {
        if !self.addresses.insert(s.to_owned()) {
            return;
        }
        let Some(bytes) = strkey_bytes(s) else {
            return;
        };
        let form = |encoding| Form {
            value: s.to_owned(),
            encoding,
            address: true,
        };
        self.forms.insert(s.to_owned(), form(Encoding::Strkey));
        self.forms
            .insert(hex::encode(&bytes), form(Encoding::VersionedHex));
        self.forms
            .insert(hex::encode(&bytes[1..]), form(Encoding::PayloadHex));
        if let Some(base) = base_account(s) {
            self.add_address(&base);
        }
    }

    fn add_amount(&mut self, s: &str) {
        // zero is in every table; it shows nothing about what was carried
        if s == "0" || s == "-0" || !is_decimal(s) {
            return;
        }
        if self.amounts.insert(s.to_owned()) {
            self.forms.entry(s.to_owned()).or_insert(Form {
                value: s.to_owned(),
                encoding: Encoding::Decimal,
                address: false,
            });
        }
    }

    fn walk(&mut self, v: &Value, key: Option<&str>, net: &Network) {
        match v {
            Value::String(s) => {
                if s.len() >= 56
                    && s.as_bytes()[0].is_ascii_uppercase()
                    && Strkey::from_string(s).is_ok()
                {
                    self.add_address(s);
                } else if key.is_some_and(|k| AMOUNT_KEYS.contains(&k)) {
                    self.add_amount(s);
                } else if s == "native" && key.is_some_and(|k| k.contains("asset")) {
                    self.add_asset(&Asset::Native, net);
                }
            }
            Value::Number(n) => {
                if key.is_some_and(|k| AMOUNT_KEYS.contains(&k)) {
                    self.add_amount(&n.to_string());
                }
            }
            Value::Array(a) => a.iter().for_each(|x| self.walk(x, key, net)),
            Value::Object(o) => {
                if o.len() == 1
                    && (o.contains_key("credit_alphanum4") || o.contains_key("credit_alphanum12"))
                    && let Ok(asset) = serde_json::from_value::<Asset>(v.clone())
                {
                    self.add_asset(&asset, net);
                }
                for (k, x) in o {
                    self.walk(x, Some(k), net);
                }
            }
            Value::Null | Value::Bool(_) => {}
        }
    }

    /// The asset's Stellar Asset Contract: its id follows from the asset and the passphrase.
    fn add_asset(&mut self, asset: &Asset, net: &Network) {
        let id = net.asset_contract_id(asset);
        self.add_address(&Strkey::Contract(stellar_strkey::Contract(id)).to_string());
    }

    /// Everything the ledgers of `chain_dir` up to `through` carry, plus what the passphrase
    /// alone gives: the genesis root account and the native asset's contract.
    pub fn from_chain(chain_dir: &Path, passphrase: &str, through: u32) -> Result<Known> {
        let net = Network::new(passphrase);
        let mut k = Known::default();
        let root = net.root_account();
        k.add_address(&shapekit_oracle::addr::account(&root));
        k.add_asset(&Asset::Native, &net);
        let path = chain_dir.join("ledgers.xdr");
        let mut ledgers = 0;
        for l in LcmStream::open(&path).with_context(|| format!("opening {}", path.display()))? {
            let l = l.with_context(|| format!("reading {}", path.display()))?;
            if shapekit_pack::lake::seq_of(&l.meta) > through {
                break;
            }
            let v = serde_json::to_value(&l.meta).context("rendering a ledger as json")?;
            k.walk(&v, None, &net);
            ledgers += 1;
        }
        if ledgers == 0 {
            bail!("{} has no ledger up to {through}", path.display());
        }
        Ok(k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strkey_bytes_drop_the_checksum() {
        let g = "GB4WH2VTILAXT2GUDH4KI2ZA2GGYSLJJUYBH6LP42LXI4WNBRIHHDDVT";
        let b = strkey_bytes(g).unwrap();
        assert_eq!(b.len(), 33);
        assert_eq!(b[0], 6 << 3);
        let p = "PDDLGS6XTXODPNKK7OP4FHZSNGVJG3OE3GKGMYN47OP3PWM3SSTPCAAAAAAXMAAAAASL2";
        let b = strkey_bytes(p).unwrap();
        assert_eq!(b[0], 15 << 3);
        assert!(b.len() > 37);
        assert!(strkey_bytes("GB4WH2VTILAXT2GUDH4KI2ZA2GGYSLJJUYBH6LP42LXI4WNBRIHHDDVA").is_none());
    }

    #[test]
    fn a_muxed_account_brings_its_base() {
        let mut k = Known::default();
        k.add_address("MBUTAF6RSBU7HAFPBOEUDFS7CHACUA33KU2E2YQIQ7VA3WK6H55RSAAAAAAAAAAAAEV2Q");
        assert_eq!(k.addresses.len(), 2);
        assert!(k.addresses.iter().any(|a| a.starts_with('G')));
    }
}

#[cfg(test)]
mod chain_tests {
    use super::*;

    #[test]
    fn a_recorded_chain_carries_its_signed_payload_signer_and_the_native_contract() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/chains/cap40.signed-payload-signer.p28/testnet");
        let chain = shapekit_pack::chain::ChainFile::read(&dir).unwrap();
        let k = Known::from_chain(&dir, &chain.passphrase, chain.ledger_range.last).unwrap();
        assert!(k.addresses.iter().any(|a| a.starts_with('P')));
        let native = Network::new(&chain.passphrase).asset_contract_id(&Asset::Native);
        assert!(
            k.addresses.contains(
                &Strkey::Contract(stellar_strkey::Contract(native))
                    .to_string()
                    .to_string()
            )
        );
    }
}
