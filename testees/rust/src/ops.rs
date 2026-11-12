//! The testee protocol operations on stellar-xdr and stellar-strkey.

use std::collections::HashMap;
use std::io::Cursor;
use std::str::FromStr;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde::Deserialize;
use serde_json::value::RawValue;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use stellar_strkey::ed25519;
use stellar_strkey::{ClaimableBalance, Contract, HashX, LiquidityPool, PreAuthTx, Strkey};
use stellar_xdr::{
    Asset, Limited, Limits, LiquidityPoolConstantProductParameters, LiquidityPoolParameters,
    ManageSellOfferOp, ReadXdr, ScMap, ScMapEntry, ScVal, Type, TypeVariant, Validate, WriteXdr,
};

/// Every op this build implements, as `hello` reports them. `to_native` is absent on purpose:
/// stellar-xdr has no native projection of an `ScVal`; its conversions all go to a target type
/// the caller picks (`u32::try_from(val)`), so there is nothing that could merge map keys.
pub const OPS: &[&str] = &[
    "decode_encode",
    "decode_json",
    "encode_json",
    "strkey_decode",
    "strkey_encode",
    "asset_order",
    "pool_id",
    "map_sort",
    "map_validate",
    "id_parse",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    Rejected,
    Panic,
    BadRequest,
}

impl ErrorClass {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorClass::Rejected => "rejected",
            ErrorClass::Panic => "panic",
            ErrorClass::BadRequest => "bad-request",
        }
    }
}

#[derive(Debug)]
pub enum Outcome {
    Ok(Value),
    Error(ErrorClass, String),
    Unsupported(String),
}

fn bad(message: impl Into<String>) -> Outcome {
    Outcome::Error(ErrorClass::BadRequest, message.into())
}

fn rejected(err: impl std::fmt::Display) -> Outcome {
    Outcome::Error(ErrorClass::Rejected, err.to_string())
}

/// One request line, with every member kept as raw json text so numbers reach the sdk exactly
/// as the runner wrote them.
pub struct Request {
    pub id: String,
    pub op: String,
    fields: HashMap<String, Box<RawValue>>,
}

impl Request {
    /// Parses a request line. On failure returns the id if one could be read, and the reason.
    pub fn parse(line: &str) -> Result<Request, (String, String)> {
        let mut de = serde_json::Deserializer::from_str(line);
        // requests carry sdk json (encode_json) nested as deep as the xdr allows
        de.disable_recursion_limit();
        let mut fields = HashMap::<String, Box<RawValue>>::deserialize(&mut de)
            .and_then(|f| de.end().map(|()| f))
            .map_err(|e| (String::new(), format!("request is not a json object: {e}")))?;
        let id = match fields.remove("id") {
            Some(raw) => serde_json::from_str::<String>(raw.get())
                .map_err(|_| (String::new(), "id is not a string".to_string()))?,
            None => return Err((String::new(), "request has no id".to_string())),
        };
        let op = match fields.remove("op") {
            Some(raw) => serde_json::from_str::<String>(raw.get())
                .map_err(|_| (id.clone(), "op is not a string".to_string()))?,
            None => return Err((id, "request has no op".to_string())),
        };
        Ok(Request { id, op, fields })
    }

    fn raw(&self, name: &str) -> Result<&RawValue, Outcome> {
        self.fields
            .get(name)
            .map(|b| &**b)
            .ok_or_else(|| bad(format!("{} needs member {name}", self.op)))
    }

    fn get<T: serde::de::DeserializeOwned>(&self, name: &str, what: &str) -> Result<T, Outcome> {
        serde_json::from_str(self.raw(name)?.get())
            .map_err(|e| bad(format!("{name} must be {what}: {e}")))
    }

    fn string(&self, name: &str) -> Result<String, Outcome> {
        self.get(name, "a string")
    }

    fn bytes(&self, name: &str) -> Result<Vec<u8>, Outcome> {
        let text = self.string(name)?;
        B64.decode(text.as_bytes())
            .map_err(|e| bad(format!("{name} is not standard base64: {e}")))
    }
}

pub fn run(req: &Request) -> Outcome {
    let result = match req.op.as_str() {
        "hello" => Ok(hello()),
        "decode_encode" => decode_encode(req),
        "decode_json" => decode_json(req),
        "encode_json" => encode_json(req),
        "strkey_decode" => strkey_decode(req),
        "strkey_encode" => strkey_encode(req),
        "asset_order" => asset_order(req),
        "pool_id" => pool_id(req),
        "map_sort" => map_sort(req),
        "map_validate" => map_validate(req),
        "id_parse" => id_parse(req),
        "to_native" => Err(Outcome::Unsupported(
            "stellar-xdr has no native projection of an ScVal: its conversions go to a target \
             type the caller names, so the typed ScVal is the value rust code works with"
                .to_string(),
        )),
        other => Err(bad(format!("unknown op {other}"))),
    };
    match result {
        Ok(v) => Outcome::Ok(v),
        Err(o) => o,
    }
}

fn hello() -> Value {
    json!({
        "protocol": crate::PROTOCOL_VERSION,
        "sdk": "stellar-xdr",
        "sdk_version": stellar_xdr::VERSION.pkg,
        "components": {"stellar-strkey": stellar_strkey::VERSION.pkg},
        "ops": OPS,
        "decode_limits": "honoured",
    })
}

fn type_variant(req: &Request) -> Result<TypeVariant, Outcome> {
    let name = req.string("type")?;
    TypeVariant::from_str(&name).map_err(|_| {
        Outcome::Unsupported(format!(
            "stellar-xdr {} has no type {name}",
            stellar_xdr::VERSION.pkg
        ))
    })
}

fn decode(req: &Request) -> Result<(Type, Limits), Outcome> {
    let variant = type_variant(req)?;
    let bytes = req.bytes("xdr")?;
    let limits = Limits {
        depth: req.get("depth_limit", "an unsigned 32-bit integer")?,
        len: req.get("len_limit", "an unsigned integer")?,
    };
    let mut reader = Limited::new(Cursor::new(bytes), limits.clone());
    let value = Type::read_xdr_to_end(variant, &mut reader).map_err(rejected)?;
    Ok((value, limits))
}

fn decode_encode(req: &Request) -> Result<Value, Outcome> {
    let (value, limits) = decode(req)?;
    let again = value.to_xdr(limits).map_err(rejected)?;
    Ok(json!({"xdr": B64.encode(again)}))
}

fn decode_json(req: &Request) -> Result<Value, Outcome> {
    let (value, _) = decode(req)?;
    let rendered = serde_json::to_value(&value).map_err(rejected)?;
    Ok(json!({"json": rendered}))
}

fn encode_json(req: &Request) -> Result<Value, Outcome> {
    let variant = type_variant(req)?;
    let text = req.raw("json")?.get();
    let value = Type::from_json(variant, text.as_bytes()).map_err(rejected)?;
    let bytes = value.to_xdr(Limits::none()).map_err(rejected)?;
    Ok(json!({"xdr": B64.encode(bytes)}))
}

fn strkey_decode(req: &Request) -> Result<Value, Outcome> {
    let text = req.string("text")?;
    // the generic decoder refuses seeds on purpose (it does not zeroize), so seeds go through
    // the crate's dedicated private key type as a caller would
    if text.starts_with('S') {
        let key = ed25519::PrivateKey::from_string(&text).map_err(rejected)?;
        return Ok(strkey_result("seed", &key.0));
    }
    let key = Strkey::from_string(&text).map_err(rejected)?;
    let (kind, payload) = match &key {
        Strkey::PublicKeyEd25519(k) => ("account", k.0.to_vec()),
        Strkey::PreAuthTx(k) => ("pre_auth_tx", k.0.to_vec()),
        Strkey::HashX(k) => ("hash_x", k.0.to_vec()),
        Strkey::MuxedAccountEd25519(k) => ("muxed_account", concat(&k.ed25519, &k.id)),
        Strkey::SignedPayloadEd25519(k) => ("signed_payload", signed_payload_raw(k)),
        Strkey::Contract(k) => ("contract", k.0.to_vec()),
        Strkey::LiquidityPool(k) => ("liquidity_pool", k.0.to_vec()),
        Strkey::ClaimableBalance(ClaimableBalance::V0(h)) => {
            let mut raw = vec![0u8];
            raw.extend_from_slice(h);
            ("claimable_balance", raw)
        }
        Strkey::MuxedContract(k) => ("muxed_contract", concat(&k.contract_id, &k.id)),
    };
    Ok(strkey_result(kind, &payload))
}

fn strkey_result(kind: &str, payload: &[u8]) -> Value {
    json!({"kind": kind, "payload": B64.encode(payload)})
}

fn concat(key: &[u8; 32], id: &u64) -> Vec<u8> {
    let mut raw = key.to_vec();
    raw.extend_from_slice(&id.to_be_bytes());
    raw
}

// sep-23 layout: key, length, payload, zero padding to four bytes
fn signed_payload_raw(k: &ed25519::SignedPayload) -> Vec<u8> {
    let inner = k.payload();
    let mut raw = k.ed25519().to_vec();
    let len = u32::try_from(inner.len()).expect("the crate caps signed payloads at 64 bytes");
    raw.extend_from_slice(&len.to_be_bytes());
    raw.extend_from_slice(inner);
    raw.resize(raw.len() + (4 - inner.len() % 4) % 4, 0);
    raw
}

fn strkey_encode(req: &Request) -> Result<Value, Outcome> {
    let kind = req.string("kind")?;
    let payload = req.bytes("payload")?;
    // the types that take raw bytes parse them through the crate; the rest are plain structs
    // over fixed-size arrays, so a payload of another length cannot be handed to the crate at
    // all and counts as the crate refusing it
    let text = match kind.as_str() {
        "account" => display(ed25519::PublicKey::from_payload(&payload).map_err(rejected)?),
        "seed" => display(
            ed25519::PrivateKey::from_payload(&payload)
                .map_err(rejected)?
                .as_unredacted(),
        ),
        "muxed_account" => {
            display(ed25519::MuxedAccount::from_payload(&payload).map_err(rejected)?)
        }
        "signed_payload" => {
            display(ed25519::SignedPayload::from_payload(&payload).map_err(rejected)?)
        }
        "pre_auth_tx" => display(PreAuthTx(array32(&payload, "PreAuthTx")?)),
        "hash_x" => display(HashX(array32(&payload, "HashX")?)),
        "contract" => display(Contract(array32(&payload, "Contract")?)),
        "liquidity_pool" => display(LiquidityPool(array32(&payload, "LiquidityPool")?)),
        "claimable_balance" => match payload.split_first() {
            Some((0, hash)) => {
                display(ClaimableBalance::V0(array32(hash, "ClaimableBalance::V0")?))
            }
            _ => {
                return Err(rejected(
                    "stellar-strkey's ClaimableBalance has only V0, whose type byte is 0",
                ));
            }
        },
        "muxed_contract" => {
            if payload.len() != 40 {
                return Err(rejected(format!(
                    "stellar-strkey's MuxedContract is a 32-byte contract id and a u64, got {} bytes",
                    payload.len()
                )));
            }
            let (contract, id) = payload.split_at(32);
            display(stellar_strkey::MuxedContract {
                contract_id: array32(contract, "MuxedContract")?,
                id: u64::from_be_bytes(id.try_into().expect("split at 32 of 40 bytes")),
            })
        }
        other => return Err(bad(format!("unknown strkey kind {other}"))),
    };
    Ok(json!({"text": text}))
}

// every strkey type implements Display; its inherent to_string returns a fixed-capacity string
fn display(value: impl std::fmt::Display) -> String {
    value.to_string()
}

fn array32(bytes: &[u8], what: &str) -> Result<[u8; 32], Outcome> {
    bytes.try_into().map_err(|_| {
        rejected(format!(
            "stellar-strkey's {what} holds exactly 32 bytes, got {}",
            bytes.len()
        ))
    })
}

fn xdr_value<T: ReadXdr>(req: &Request, name: &str) -> Result<T, Outcome> {
    T::from_xdr(req.bytes(name)?, Limits::none()).map_err(rejected)
}

fn asset_order(req: &Request) -> Result<Value, Outcome> {
    let a: Asset = xdr_value(req, "a")?;
    let b: Asset = xdr_value(req, "b")?;
    Ok(json!({"order": a.cmp(&b) as i8}))
}

// stellar-xdr has the cap-38 types but no pool id function, so the id is the sha-256 of the
// parameters as the crate encodes them, and the pair must be strictly ordered by the crate's
// own Asset ordering, as core's isPoolShareAssetValid requires
fn pool_id(req: &Request) -> Result<Value, Outcome> {
    let asset_a: Asset = xdr_value(req, "a")?;
    let asset_b: Asset = xdr_value(req, "b")?;
    let fee: i32 = req.get("fee", "a signed 32-bit integer")?;
    if asset_a >= asset_b {
        return Err(rejected(
            "asset a does not sort strictly before asset b by stellar-xdr's Asset ordering",
        ));
    }
    let params = LiquidityPoolParameters::LiquidityPoolConstantProduct(
        LiquidityPoolConstantProductParameters {
            asset_a,
            asset_b,
            fee,
        },
    );
    let bytes = params.to_xdr(Limits::none()).map_err(rejected)?;
    Ok(json!({"pool_id": hex::encode(Sha256::digest(&bytes))}))
}

#[derive(Deserialize)]
struct MapEntryIn {
    key: String,
    val: String,
}

fn map_sort(req: &Request) -> Result<Value, Outcome> {
    let entries: Vec<MapEntryIn> = req.get("entries", "an array of {key, val}")?;
    let mut parsed = Vec::with_capacity(entries.len());
    for (i, e) in entries.iter().enumerate() {
        let key = B64
            .decode(&e.key)
            .map_err(|err| bad(format!("entries[{i}].key is not base64: {err}")))?;
        let val = B64
            .decode(&e.val)
            .map_err(|err| bad(format!("entries[{i}].val is not base64: {err}")))?;
        parsed.push(ScMapEntry {
            key: ScVal::from_xdr(key, Limits::none()).map_err(rejected)?,
            val: ScVal::from_xdr(val, Limits::none()).map_err(rejected)?,
        });
    }
    let map = ScMap::sorted_from(parsed).map_err(rejected)?;
    let bytes = ScVal::Map(Some(map))
        .to_xdr(Limits::none())
        .map_err(rejected)?;
    Ok(json!({"xdr": B64.encode(bytes)}))
}

fn map_validate(req: &Request) -> Result<Value, Outcome> {
    let value: ScVal = xdr_value(req, "xdr")?;
    match value {
        ScVal::Map(Some(map)) => Ok(json!({"valid": map.validate().is_ok()})),
        _ => Err(bad("map_validate needs an ScVal of arm SCV_MAP with a map")),
    }
}

// the only textual or numeric id input stellar-xdr has is its xdr-json reader, where offer ids
// are numbers or strings (NumberOrString). muxed account ids have no such input: in xdr-json a
// muxed account is an M strkey, and the rust types take a u64
fn id_parse(req: &Request) -> Result<Value, Outcome> {
    let kind = req.string("kind")?;
    let input = req.raw("input")?.get();
    match kind.as_str() {
        "offer" => {
            let doc = format!(
                r#"{{"selling":"native","buying":"native","amount":"0","price":{{"n":1,"d":1}},"offer_id":{input}}}"#
            );
            let op: ManageSellOfferOp = serde_json::from_str(&doc).map_err(rejected)?;
            Ok(json!({"id": op.offer_id.to_string()}))
        }
        "muxed" => Err(Outcome::Unsupported(
            "stellar-xdr takes muxed ids only as a u64 or inside an M strkey; it has no textual \
             or numeric id input to test"
                .to_string(),
        )),
        other => Err(bad(format!("unknown id kind {other}"))),
    }
}
