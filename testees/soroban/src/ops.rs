//! The testee protocol operations on soroban-sdk.
//!
//! soroban-sdk's own XDR path is the `ScVal`: `FromXdr` and `ToXdr` (src/xdr.rs) hand the bytes
//! to the host's `deserialize_from_bytes` and `serialize_to_bytes`, and every contract value
//! type converts to and from `ScVal` through the host. That path is what this testee runs. The
//! crate also re-exports the stellar-xdr crate as `soroban_sdk::xdr`; decoding other types with
//! it would test stellar-xdr again, which the rust testee already does, so those requests are
//! unsupported. stellar-xdr is only used here to read request inputs that are not contract
//! values (a `ContractIdPreimage`) and to build the structured address an encoder takes.

use std::collections::HashMap;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde::Deserialize;
use serde_json::value::RawValue;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use soroban_sdk::testutils::Ledger as _;
use soroban_sdk::xdr::{
    AccountId, ContractId, ContractIdPreimage, FromXdr, Hash, Limits, MuxedEd25519Account,
    PublicKey, ReadXdr, ScAddress, ScVal, ToXdr, Uint256, WriteXdr,
};
use soroban_sdk::{
    Address, Bytes, BytesN, Duration, Env, I256, Map, MuxedAddress, String as SdkString, Symbol,
    Timepoint, TryFromVal, U256, Val, Vec as SdkVec,
};

pub const SDK_VERSION: &str = env!("SHAPEKIT_SOROBAN_SDK_VERSION");
pub const HOST_VERSION: &str = env!("SHAPEKIT_SOROBAN_ENV_HOST_VERSION");
pub const XDR_VERSION: &str = env!("SHAPEKIT_STELLAR_XDR_VERSION");

/// Every op this build implements, as `hello` reports them. The rest have no soroban-sdk API:
/// the sdk renders no SEP-51 JSON, has no asset ordering, pool ID, transaction hash, balance ID
/// or textual ID reader, and no strkey codec for kinds other than addresses.
pub const OPS: &[&str] = &[
    "decode_encode",
    "strkey_decode",
    "strkey_encode",
    "map_sort",
    "map_validate",
    "to_native",
    "contract_id",
];

// the depth stellar-xdr reads a request's ContractIdPreimage or map at, as the rust testee does
const DEFAULT_DEPTH: u32 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    Rejected,
    Panic,
    BadRequest,
    Internal,
}

impl ErrorClass {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorClass::Rejected => "rejected",
            ErrorClass::Panic => "panic",
            ErrorClass::BadRequest => "bad-request",
            ErrorClass::Internal => "internal",
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

fn rejected(err: impl std::fmt::Debug) -> Outcome {
    Outcome::Error(ErrorClass::Rejected, format!("{err:?}"))
}

/// The text of a panic payload.
pub fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic with a non-string payload".to_string())
}

/// A host function that fails returns a `HostError`, and soroban-sdk built with `testutils`
/// escalates it into a panic whose message is the error's debug form, `HostError: Error(..)`
/// (soroban-env-host src/host.rs `escalate_error_to_panic`, src/host/error.rs). That is the
/// sdk's documented error path, the native stand-in for a wasm trap, so it is a rejection.
/// Any other panic is the sdk crashing where it had a `Result` to return.
pub fn classify_panic(message: &str) -> ErrorClass {
    if message.starts_with("HostError: ") {
        ErrorClass::Rejected
    } else {
        ErrorClass::Panic
    }
}

/// Runs `f` and turns a host error escalated into a panic back into an `Err`; any other panic
/// keeps unwinding to the request's catcher, which reports it as `panic`.
fn host_result<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(v) => Ok(v),
        Err(payload) => {
            let message = panic_text(&*payload);
            if classify_panic(&message) == ErrorClass::Rejected {
                Err(message.lines().next().unwrap_or("").to_string())
            } else {
                std::panic::resume_unwind(payload)
            }
        }
    }
}

/// One request line, with every member kept as raw json text.
pub struct Request {
    pub id: String,
    pub op: String,
    fields: HashMap<String, Box<RawValue>>,
}

impl Request {
    /// Parses a request line. On failure returns the id if one could be read, and the reason.
    pub fn parse(line: &str) -> Result<Request, (String, String)> {
        let mut fields = serde_json::from_str::<HashMap<String, Box<RawValue>>>(line)
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

    fn has(&self, name: &str) -> bool {
        self.fields.contains_key(name)
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
        "strkey_decode" => strkey_decode(req),
        "strkey_encode" => strkey_encode(req),
        "map_sort" => map_sort(req),
        "map_validate" => map_validate(req),
        "to_native" => to_native(req),
        "contract_id" => contract_id(req),
        "decode_json" | "encode_json" => Err(Outcome::Unsupported(
            "soroban-sdk has no SEP-51 XDR-JSON reader or writer".into(),
        )),
        "asset_order" | "pool_id" => Err(Outcome::Unsupported(
            "soroban-sdk has no classic Asset type of its own and no asset ordering or pool ID \
             function"
                .into(),
        )),
        "id_parse" => Err(Outcome::Unsupported(
            "soroban-sdk reads no textual muxed or offer ID".into(),
        )),
        "tx_hash" | "balance_id" => Err(Outcome::Unsupported(
            "soroban-sdk builds no transaction and has no transaction hash or balance ID \
             function"
                .into(),
        )),
        "coverage_snapshot" | "coverage_new" => Err(Outcome::Unsupported(
            "the soroban testee has no coverage build".into(),
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
        "sdk": "soroban-sdk",
        "sdk_version": SDK_VERSION,
        "components": {"soroban-env-host": HOST_VERSION, "stellar-xdr": XDR_VERSION},
        "ops": OPS,
        // FromXdr takes no limits: the host reads every ScVal under its own
        // DEFAULT_XDR_RW_LIMITS and converts it under its depth limit
        "decode_limits": "ignored",
    })
}

/// A fresh test environment per request, so no value or budget use carries over. The budget
/// is lifted: a network's resource limits are configuration, and a conversion that only fails
/// for lack of budget says nothing about the value.
fn env() -> Env {
    let env = Env::default();
    env.cost_estimate().budget().reset_unlimited();
    env
}

/// Reads ScVal bytes the way a contract does, through `FromXdr`.
fn read_val(env: &Env, bytes: &[u8]) -> Result<Val, Outcome> {
    let input = Bytes::from_slice(env, bytes);
    match host_result(|| Val::from_xdr(env, &input)) {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Err(rejected(e)),
        Err(message) => Err(Outcome::Error(ErrorClass::Rejected, message)),
    }
}

fn write_val(env: &Env, val: Val) -> Result<Vec<u8>, Outcome> {
    match host_result(|| bytes_vec(&val.to_xdr(env))) {
        Ok(b) => Ok(b),
        Err(message) => Err(Outcome::Error(ErrorClass::Rejected, message)),
    }
}

fn decode_encode(req: &Request) -> Result<Value, Outcome> {
    let name = req.string("type")?;
    if name != "ScVal" {
        return Err(Outcome::Unsupported(format!(
            "soroban-sdk reads XDR only as contract values (FromXdr and ToXdr, src/xdr.rs, \
             always the ScVal form); {name} is read only by the re-exported stellar-xdr crate"
        )));
    }
    let bytes = req.bytes("xdr")?;
    let env = env();
    let val = read_val(&env, &bytes)?;
    Ok(json!({"xdr": B64.encode(write_val(&env, val)?)}))
}

#[derive(Deserialize)]
struct MapEntryIn {
    key: String,
    val: String,
}

/// `Map::set` on an empty map for each entry in input order: the sdk's map is sorted by the
/// host's value order, and `set` is how a contract builds one from entries it does not know at
/// compile time (`Map::from_array` takes a fixed-size array and calls `set` the same way).
fn map_sort(req: &Request) -> Result<Value, Outcome> {
    let entries: Vec<MapEntryIn> = req.get("entries", "an array of {key, val}")?;
    let env = env();
    let mut map: Map<Val, Val> = Map::new(&env);
    for (i, e) in entries.iter().enumerate() {
        let key = B64
            .decode(&e.key)
            .map_err(|err| bad(format!("entries[{i}].key is not base64: {err}")))?;
        let val = B64
            .decode(&e.val)
            .map_err(|err| bad(format!("entries[{i}].val is not base64: {err}")))?;
        let k = read_val(&env, &key)?;
        let v = read_val(&env, &val)?;
        if let Err(message) = host_result(|| map.set(k, v)) {
            return Err(Outcome::Error(ErrorClass::Rejected, message));
        }
    }
    Ok(json!({"xdr": B64.encode(write_val(&env, map.to_val())?)}))
}

/// Whether the host accepts the map as a contract value. The bytes are read with stellar-xdr
/// first, so that bytes no decoder takes are `rejected` and only a map the host refuses to
/// convert is `valid: false`.
fn map_validate(req: &Request) -> Result<Value, Outcome> {
    let bytes = req.bytes("xdr")?;
    let limits = Limits {
        depth: DEFAULT_DEPTH,
        len: bytes.len(),
    };
    let scval = <ScVal as ReadXdr>::from_xdr(&bytes, limits).map_err(rejected)?;
    if !matches!(scval, ScVal::Map(Some(_))) {
        return Err(bad("map_validate needs an ScVal map"));
    }
    let env = env();
    let valid = matches!(host_result(|| Val::try_from_val(&env, &scval)), Ok(Ok(_)));
    Ok(json!({"valid": valid}))
}

fn to_native(req: &Request) -> Result<Value, Outcome> {
    let bytes = req.bytes("xdr")?;
    let env = env();
    let val = read_val(&env, &bytes)?;
    Ok(json!({"native": native(&env, val)?}))
}

fn object(class: &str, text: String) -> Value {
    json!({"type": "object", "class": class, "text": text})
}

fn integer(text: String) -> Value {
    json!({"type": "integer", "value": text})
}

/// Describes a host value as the sdk type it converts into. soroban-sdk has no untyped native
/// form: a contract names the type, and `TryFromVal` checks the value's tag. Each type is tried
/// in turn; `Address` comes before `MuxedAddress`, which accepts plain addresses too.
fn native(env: &Env, val: Val) -> Result<Value, Outcome> {
    macro_rules! as_type {
        ($t:ty) => {
            <$t>::try_from_val(env, &val).ok()
        };
    }
    if val.is_void() {
        return Ok(json!({"type": "null"}));
    }
    if let Some(b) = as_type!(bool) {
        return Ok(json!({"type": "bool", "value": b}));
    }
    if let Some(n) = as_type!(u32) {
        return Ok(integer(n.to_string()));
    }
    if let Some(n) = as_type!(i32) {
        return Ok(integer(n.to_string()));
    }
    if let Some(n) = as_type!(u64) {
        return Ok(integer(n.to_string()));
    }
    if let Some(n) = as_type!(i64) {
        return Ok(integer(n.to_string()));
    }
    if let Some(n) = as_type!(u128) {
        return Ok(integer(n.to_string()));
    }
    if let Some(n) = as_type!(i128) {
        return Ok(integer(n.to_string()));
    }
    if let Some(n) = as_type!(U256) {
        return Ok(integer(decimal(&be32(&n.to_be_bytes())?, false)));
    }
    if let Some(n) = as_type!(I256) {
        return Ok(integer(decimal(&be32(&n.to_be_bytes())?, true)));
    }
    if let Some(t) = as_type!(Timepoint) {
        return Ok(object("soroban_sdk::Timepoint", t.to_unix().to_string()));
    }
    if let Some(d) = as_type!(Duration) {
        return Ok(object("soroban_sdk::Duration", d.to_seconds().to_string()));
    }
    if let Some(b) = as_type!(Bytes) {
        return Ok(json!({"type": "bytes", "value": B64.encode(bytes_vec(&b))}));
    }
    if let Some(s) = as_type!(SdkString) {
        let mut buf = vec![0u8; s.len() as usize];
        s.copy_into_slice(&mut buf);
        return Ok(match std::string::String::from_utf8(buf) {
            Ok(text) => json!({"type": "string", "value": text}),
            // a soroban String is a byte string the host never checks for utf-8; with no text
            // form the bytes themselves are its lossless description
            Err(e) => object("soroban_sdk::String", B64.encode(e.into_bytes())),
        });
    }
    if let Some(s) = as_type!(Symbol) {
        return Ok(object("soroban_sdk::Symbol", s.to_string()));
    }
    if let Some(a) = as_type!(Address) {
        return Ok(object("soroban_sdk::Address", text_of(&a.to_string())));
    }
    if let Some(m) = as_type!(MuxedAddress) {
        return Ok(object("soroban_sdk::MuxedAddress", text_of(&m.to_strkey())));
    }
    if let Some(v) = as_type!(SdkVec<Val>) {
        let mut items = Vec::with_capacity(v.len() as usize);
        for item in v.iter() {
            items.push(native(env, item)?);
        }
        return Ok(json!({"type": "array", "items": items}));
    }
    if let Some(m) = as_type!(Map<Val, Val>) {
        let mut entries = Vec::with_capacity(m.len() as usize);
        for (k, v) in m.iter() {
            entries.push(json!({"key": native(env, k)?, "value": native(env, v)?}));
        }
        return Ok(json!({"type": "map", "entries": entries}));
    }
    if let Some(e) = as_type!(soroban_sdk::Error) {
        return Ok(object("soroban_sdk::Error", format!("{e:?}")));
    }
    Err(Outcome::Error(
        ErrorClass::Rejected,
        "the host value converts into no soroban-sdk type".into(),
    ))
}

fn bytes_vec(b: &Bytes) -> Vec<u8> {
    let mut out = vec![0u8; b.len() as usize];
    b.copy_into_slice(&mut out);
    out
}

fn text_of(s: &SdkString) -> String {
    let mut buf = vec![0u8; s.len() as usize];
    s.copy_into_slice(&mut buf);
    std::string::String::from_utf8_lossy(&buf).into_owned()
}

fn be32(b: &Bytes) -> Result<[u8; 32], Outcome> {
    bytes_vec(b).try_into().map_err(|_| {
        Outcome::Error(
            ErrorClass::Internal,
            "a 256-bit integer did not give 32 bytes".into(),
        )
    })
}

/// The decimal text of a 256-bit big-endian integer, two's complement when `signed`.
fn decimal(be: &[u8; 32], signed: bool) -> String {
    let negative = signed && be[0] & 0x80 != 0;
    let mut mag = *be;
    if negative {
        // two's complement magnitude: invert and add one
        let mut carry = 1u16;
        for b in mag.iter_mut().rev() {
            let v = u16::from(!*b) + carry;
            *b = v as u8;
            carry = v >> 8;
        }
    }
    let mut digits = Vec::new();
    while mag.iter().any(|&b| b != 0) {
        let mut rem = 0u32;
        for b in mag.iter_mut() {
            let cur = (rem << 8) | u32::from(*b);
            *b = (cur / 10) as u8;
            rem = cur % 10;
        }
        digits.push(b'0' + rem as u8);
    }
    if digits.is_empty() {
        digits.push(b'0');
    }
    if negative {
        digits.push(b'-');
    }
    digits.reverse();
    std::string::String::from_utf8(digits).expect("ascii digits")
}

fn strkey_input(req: &Request) -> Result<Vec<u8>, Outcome> {
    match (req.has("text"), req.has("text_base64")) {
        (true, false) => Ok(req.string("text")?.into_bytes()),
        (false, true) => req.bytes("text_base64"),
        _ => Err(bad(
            "strkey_decode needs exactly one of text and text_base64",
        )),
    }
}

fn no_strkey_codec(first: Option<&u8>) -> Outcome {
    let what = match first {
        Some(c) if c.is_ascii_graphic() => format!("a strkey starting with {}", char::from(*c)),
        Some(_) => "a strkey starting with that byte".to_string(),
        None => "an empty strkey".to_string(),
    };
    Outcome::Unsupported(format!(
        "soroban-sdk reads and writes strkeys only as addresses: Address::from_string_bytes \
         and to_string for G and C (src/address.rs), MuxedAddress::from_string_bytes and \
         to_strkey for M (src/muxed_address.rs); it has no codec for {what}"
    ))
}

/// The sdk reads strkey bytes with `Address::from_string_bytes`, which takes `Bytes` and so
/// reaches the host's strkey reader with input that is not utf-8 too. M strings go through
/// `MuxedAddress::from_string_bytes`, which `Address` refuses by design.
fn strkey_decode(req: &Request) -> Result<Value, Outcome> {
    let input = strkey_input(req)?;
    let env = env();
    let bytes = Bytes::from_slice(&env, &input);
    let scaddress = match input.first() {
        Some(b'G' | b'C') => host_result(|| ScAddress::from(&Address::from_string_bytes(&bytes)))
            .map_err(|m| Outcome::Error(ErrorClass::Rejected, m))?,
        Some(b'M') => {
            match host_result(|| ScVal::from(&MuxedAddress::from_string_bytes(&bytes)))
                .map_err(|m| Outcome::Error(ErrorClass::Rejected, m))?
            {
                ScVal::Address(a) => a,
                other => {
                    return Err(Outcome::Error(
                        ErrorClass::Internal,
                        format!("a MuxedAddress converted to the ScVal {other:?}"),
                    ));
                }
            }
        }
        first => return Err(no_strkey_codec(first)),
    };
    let (kind, payload) = match scaddress {
        ScAddress::Account(AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(k)))) => {
            ("account", k.to_vec())
        }
        ScAddress::Contract(ContractId(Hash(h))) => ("contract", h.to_vec()),
        ScAddress::MuxedAccount(MuxedEd25519Account { id, ed25519 }) => {
            let mut p = ed25519.0.to_vec();
            p.extend_from_slice(&id.to_be_bytes());
            ("muxed_account", p)
        }
        other => {
            return Err(Outcome::Error(
                ErrorClass::Internal,
                format!("the sdk read an address that is no strkey kind it writes: {other:?}"),
            ));
        }
    };
    Ok(json!({"kind": kind, "payload": B64.encode(payload)}))
}

fn strkey_encode(req: &Request) -> Result<Value, Outcome> {
    let kind = req.string("kind")?;
    let payload = req.bytes("payload")?;
    let fixed = |n: usize| -> Result<Vec<u8>, Outcome> {
        if payload.len() == n {
            Ok(payload.clone())
        } else {
            // the encoders take fixed-size structured addresses, which cannot hold this payload
            Err(Outcome::Error(
                ErrorClass::Rejected,
                format!(
                    "a {kind} address holds {n} bytes, the payload has {}",
                    payload.len()
                ),
            ))
        }
    };
    let env = env();
    let text = match kind.as_str() {
        "account" => {
            let k: [u8; 32] = fixed(32)?.try_into().expect("32 bytes");
            let a = ScAddress::Account(AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(k))));
            host_result(|| Address::try_from_val(&env, &a).map(|a| text_of(&a.to_string())))
        }
        "contract" => {
            let h: [u8; 32] = fixed(32)?.try_into().expect("32 bytes");
            let a = ScAddress::Contract(ContractId(Hash(h)));
            host_result(|| Address::try_from_val(&env, &a).map(|a| text_of(&a.to_string())))
        }
        "muxed_account" => {
            let p = fixed(40)?;
            let a = ScAddress::MuxedAccount(MuxedEd25519Account {
                ed25519: Uint256(p[..32].try_into().expect("32 bytes")),
                id: u64::from_be_bytes(p[32..].try_into().expect("8 bytes")),
            });
            host_result(|| MuxedAddress::try_from_val(&env, &a).map(|m| text_of(&m.to_strkey())))
        }
        other => {
            return Err(Outcome::Unsupported(format!(
                "soroban-sdk writes strkeys only for addresses (account, contract, \
                 muxed_account); it has no encoder for {other}"
            )));
        }
    };
    match text {
        Ok(Ok(t)) => Ok(json!({"text": t})),
        Ok(Err(e)) => Err(rejected(e)),
        Err(m) => Err(Outcome::Error(ErrorClass::Rejected, m)),
    }
}

/// The address the sdk's deployer would give the contract the preimage describes, on the
/// request's network: `Deployer::with_address(..).deployed_address()` and
/// `Deployer::with_stellar_asset(..).deployed_address()` (src/deploy.rs).
fn contract_id(req: &Request) -> Result<Value, Outcome> {
    let bytes = req.bytes("preimage")?;
    let limits = Limits {
        depth: DEFAULT_DEPTH,
        len: bytes.len(),
    };
    let preimage = ContractIdPreimage::from_xdr(&bytes, limits).map_err(rejected)?;
    let passphrase = req.string("network_passphrase")?;
    let env = env();
    env.ledger()
        .set_network_id(Sha256::digest(passphrase.as_bytes()).into());
    let deployed = match preimage {
        ContractIdPreimage::Address(p) => {
            let address = host_result(|| Address::try_from_val(&env, &p.address))
                .map_err(|m| Outcome::Error(ErrorClass::Rejected, m))?
                .map_err(rejected)?;
            let salt = BytesN::from_array(&env, &p.salt.0);
            host_result(|| {
                env.deployer()
                    .with_address(address, salt)
                    .deployed_address()
            })
        }
        ContractIdPreimage::Asset(asset) => {
            let serialized = asset.to_xdr(Limits::none()).map_err(rejected)?;
            let serialized = Bytes::from_slice(&env, &serialized);
            host_result(|| {
                env.deployer()
                    .with_stellar_asset(serialized)
                    .deployed_address()
            })
        }
    }
    .map_err(|m| Outcome::Error(ErrorClass::Rejected, m))?;
    match ScAddress::from(&deployed) {
        ScAddress::Contract(ContractId(Hash(h))) => Ok(json!({"contract_id": hex::encode(h)})),
        other => Err(Outcome::Error(
            ErrorClass::Internal,
            format!("the deployer gave a non-contract address {other:?}"),
        )),
    }
}
