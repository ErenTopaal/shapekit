//! JSON-RPC 2.0 methods, shaped after stellar-rpc (`cmd/stellar-rpc/internal/methods`)
//! and the request and response types of go-stellar-sdk v0.8.0 `protocols/rpc` (R10).
//! Error codes and messages follow those sources, so clients that branch on them behave as
//! they would against stellar-rpc. Where v30.0.0 and v30.0.1 differ (getLedgers with a
//! cursor at or past the tip), v30.0.1 is followed: it is the current release and the one
//! the conformance fixtures were captured from.

use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use shapekit_oracle::view::LedgerView;
use stellar_xdr::{
    LedgerCloseMeta, LedgerEntry, LedgerKey, LedgerKeyTtl, Limits, ReadXdr, TransactionEnvelope,
    TransactionMeta, TransactionResultResult, WriteXdr,
};

use crate::json::{self as text, Object};
use crate::replay::{Replay, header_entry};

/// jrpc2's standard codes, the ones stellar-rpc answers with.
pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;

/// stellar-rpc's defaults for getLedgers and getTransactions.
pub const MAX_LIMIT: u64 = 200;
pub const DEFAULT_LIMIT: u64 = 50;

/// `getLedgerEntriesMaxKeys` in stellar-rpc.
pub const MAX_KEYS: usize = 200;

const FORMAT_BASE64: &str = "base64";
const FORMAT_JSON: &str = "json";

/// Limits for decoding request XDR, which a client controls: the shared envelope bound, the
/// one core puts on client XDR, and the request text as the length budget.
fn request_limits(type_name: &str, len: usize) -> Limits {
    shapekit_manifest::decode_limits(type_name, len)
}

#[derive(Debug)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

impl RpcError {
    fn new(code: i64, message: impl Into<String>) -> Self {
        RpcError {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn to_json(&self) -> Value {
        let mut e = json!({"code": self.code, "message": self.message});
        if let Some(d) = &self.data {
            e["data"] = d.clone();
        }
        e
    }
}

fn internal(e: impl std::fmt::Display) -> RpcError {
    RpcError::new(INTERNAL_ERROR, e.to_string())
}

impl From<serde_json::Error> for RpcError {
    fn from(e: serde_json::Error) -> Self {
        internal(e)
    }
}

/// Handles one request object, appending its response object to `out`. Returns false, with
/// nothing written, for a notification (a request without an id): every method is read
/// only, so a notification has no effect to carry out.
///
/// The response is text from the start, ledger by ledger and transaction by transaction,
/// and in the bytes a `serde_json::Value` of it would print: keys sorted, compact.
pub fn handle(replay: &Replay, req: &Value, out: &mut Vec<u8>) -> bool {
    let Some(id) = req.get("id") else {
        return false;
    };
    let mark = out.len();
    // "id" < "jsonrpc" < "result": the result, written last, streams in place
    out.extend_from_slice(b"{\"id\":");
    let wrote = text::write(out, id).map_err(RpcError::from).and_then(|()| {
        out.extend_from_slice(b",\"jsonrpc\":\"2.0\",\"result\":");
        dispatch(replay, req, out)
    });
    match wrote {
        Ok(()) => out.push(b'}'),
        Err(e) => {
            out.truncate(mark);
            let v = json!({"jsonrpc": "2.0", "id": id, "error": e.to_json()});
            text::write(out, &v).expect("an error object of plain values serialises");
        }
    }
    true
}

/// A response for a body that is not JSON at all.
pub fn parse_error(e: &serde_json::Error) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": Value::Null,
        "error": {"code": PARSE_ERROR, "message": "invalid request value", "data": e.to_string()},
    })
}

/// Writes the result of one request into `out`; on an error `out` holds a partial result
/// that the caller discards.
fn dispatch(replay: &Replay, req: &Value, out: &mut Vec<u8>) -> Result<(), RpcError> {
    if req.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(RpcError::new(INVALID_REQUEST, "invalid version marker"));
    }
    let method = req
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::new(INVALID_REQUEST, "empty method name"))?;
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    let small = match method {
        "getHealth" => get_health(replay),
        "getNetwork" => get_network(replay),
        "getVersionInfo" => get_version_info(replay),
        "getLatestLedger" => get_latest_ledger(replay),
        "getLedgers" => return get_ledgers(replay, parse(params)?, out),
        "getLedgerEntries" => return get_ledger_entries(replay, parse(params)?, out),
        "getTransaction" => return get_transaction(replay, parse(params)?, out),
        "getTransactions" => return get_transactions(replay, parse(params)?, out),
        "sendTransaction" => Err(RpcError::new(
            INVALID_REQUEST,
            "shapekit replay server is read-only: it serves recorded ledgers and accepts no transactions",
        )),
        // simulateTransaction, getEvents, getFeeStats and anything else: not implemented,
        // and saying so is the only honest answer
        other => Err(RpcError {
            code: METHOD_NOT_FOUND,
            message: "method not found".into(),
            data: Some(Value::String(other.to_string())),
        }),
    }?;
    Ok(text::write(out, &small)?)
}

/// Decodes params the way jrpc2 does for stellar-rpc's handlers: an object or nothing,
/// never positional.
fn parse<T: for<'de> Deserialize<'de> + Default>(params: Value) -> Result<T, RpcError> {
    match params {
        Value::Null => Ok(T::default()),
        Value::Object(_) => serde_json::from_value(params).map_err(|e| RpcError {
            code: INVALID_PARAMS,
            message: "invalid parameters".into(),
            data: Some(Value::String(e.to_string())),
        }),
        other => Err(RpcError {
            code: INVALID_PARAMS,
            message: "invalid parameters".into(),
            data: Some(Value::String(format!(
                "positional or scalar params are not accepted: {other}"
            ))),
        }),
    }
}

fn check_format(f: &Option<String>) -> Result<bool, String> {
    match f.as_deref() {
        None | Some("") | Some(FORMAT_BASE64) => Ok(false),
        Some(FORMAT_JSON) => Ok(true),
        Some(other) => Err(format!(
            "got '{other}': expected base64, json for optional 'xdrFormat'"
        )),
    }
}

fn b64(t: &impl WriteXdr) -> Result<String, RpcError> {
    t.to_xdr_base64(Limits::none()).map_err(internal)
}

fn b64_bytes(b: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(b)
}

fn latest(replay: &Replay) -> Result<(u32, u64), RpcError> {
    let head = replay.head();
    Ok((head, replay.close_time(head).map_err(internal)?))
}

fn get_health(replay: &Replay) -> Result<Value, RpcError> {
    let (head, head_time) = latest(replay)?;
    Ok(json!({
        "status": "healthy",
        "latestLedger": head,
        "latestLedgerCloseTime": head_time.to_string(),
        "oldestLedger": replay.first,
        "oldestLedgerCloseTime": replay.first_close_time.to_string(),
        // everything recorded is retained
        "ledgerRetentionWindow": replay.last - replay.first + 1,
    }))
}

fn protocol_version(replay: &Replay) -> Result<u32, RpcError> {
    let (_, lcm) = replay.ledger(replay.head()).map_err(internal)?;
    Ok(header_entry(&lcm).header.ledger_version)
}

fn get_network(replay: &Replay) -> Result<Value, RpcError> {
    // no friendbot: the field is omitted, as stellar-rpc does when none is configured
    Ok(json!({
        "passphrase": replay.chain.passphrase,
        "protocolVersion": protocol_version(replay)?,
    }))
}

fn get_version_info(replay: &Replay) -> Result<Value, RpcError> {
    let img = &replay.chain.core_image;
    Ok(json!({
        "version": format!("shapekit-replay {}", env!("CARGO_PKG_VERSION")),
        "commitHash": "",
        "buildTimestamp": "",
        "captiveCoreVersion": format!(
            "none: replaying ledgers recorded by {}:{} ({})",
            img.repository, img.tag, img.digest
        ),
        "protocolVersion": protocol_version(replay)?,
    }))
}

fn get_latest_ledger(replay: &Replay) -> Result<Value, RpcError> {
    let head = replay.head();
    let (xdr, lcm) = replay.ledger(head).map_err(internal)?;
    let h = header_entry(&lcm);
    Ok(json!({
        "id": hex::encode(h.hash.0),
        "protocolVersion": h.header.ledger_version,
        "sequence": head,
        "closeTime": h.header.scp_value.close_time.0.to_string(),
        "headerXdr": b64(&h.header)?,
        "metadataXdr": b64_bytes(&xdr),
    }))
}

#[derive(Debug, Default, Deserialize)]
struct Pagination {
    #[serde(default)]
    cursor: String,
    #[serde(default)]
    limit: u64,
}

#[derive(Debug, Default, Deserialize)]
struct GetLedgersRequest {
    #[serde(rename = "startLedger", default)]
    start_ledger: u32,
    #[serde(default)]
    pagination: Option<Pagination>,
    #[serde(rename = "xdrFormat", default)]
    format: Option<String>,
}

/// `ValidatePagination` from go-stellar-sdk v0.8.0 `protocols/rpc/get_ledgers.go`.
fn validate_pagination(
    start: u32,
    pagination: &Option<Pagination>,
    first: u32,
    last: u32,
) -> Result<(), String> {
    let bad_page = || {
        format!(
            "start ledger ({start}) must be between the oldest ledger: {first} and the latest ledger: {last} for this rpc instance"
        )
    };
    let within = |s: u32| s >= first && s <= last;
    match pagination {
        Some(p) => {
            if !p.cursor.is_empty() {
                if start != 0 {
                    return Err(format!(
                        "startLedger ({start}) and cursor ({}) cannot both be set",
                        p.cursor
                    ));
                }
            } else if !within(start) {
                return Err(bad_page());
            }
            if p.limit > MAX_LIMIT {
                return Err(format!("limit must not exceed {MAX_LIMIT}"));
            }
            Ok(())
        }
        None if !within(start) => Err(bad_page()),
        None => Ok(()),
    }
}

/// `errors.Join` of the pagination and format checks, as stellar-rpc builds it.
fn join(errs: [Result<(), String>; 2]) -> Result<(), String> {
    let msgs: Vec<String> = errs.into_iter().filter_map(Result::err).collect();
    if msgs.is_empty() {
        Ok(())
    } else {
        Err(msgs.join("\n"))
    }
}

fn get_ledgers(replay: &Replay, req: GetLedgersRequest, out: &mut Vec<u8>) -> Result<(), RpcError> {
    let (first, head) = (replay.first, replay.head());
    let as_json = check_format(&req.format);
    join([
        validate_pagination(req.start_ledger, &req.pagination, first, head),
        as_json.clone().map(|_| ()),
    ])
    .map_err(|m| RpcError::new(INVALID_REQUEST, m))?;
    let as_json = as_json.expect("joined above");

    let (start, limit) = match &req.pagination {
        None => (req.start_ledger, DEFAULT_LIMIT),
        Some(p) => {
            let mut start = req.start_ledger;
            if !p.cursor.is_empty() {
                let c: u32 = p
                    .cursor
                    .parse()
                    .map_err(|_| RpcError::new(INVALID_PARAMS, go_parse_uint_error(&p.cursor)))?;
                // only below the oldest ledger is an error; a cursor at or past the head is
                // a caught-up poller and gets an empty page with its cursor echoed (v30.0.1).
                // a cursor of u32::MAX wraps to 0 and lands here too
                start = c.wrapping_add(1);
                if start < first {
                    return Err(RpcError::new(
                        INVALID_PARAMS,
                        format!(
                            "cursor ('{}') must be at or above the oldest ledger: {first} for this rpc instance",
                            p.cursor
                        ),
                    ));
                }
            }
            (start, if p.limit == 0 { DEFAULT_LIMIT } else { p.limit })
        }
    };
    let end = (u64::from(start) + limit - 1).min(u64::from(head)) as u32;
    // the members before "ledgers" depend on how the page ends, so the page is written
    // first and they are put in front of it after
    let at = out.len();
    out.extend_from_slice(b"\"ledgers\":[");
    for seq in start..=end {
        if seq != start {
            out.push(b',');
        }
        let (xdr, lcm) = replay.ledger(seq).map_err(|e| {
            RpcError::new(
                INTERNAL_ERROR,
                format!("error fetching ledgers from db: {e}"),
            )
        })?;
        ledger_info(&xdr, &lcm, as_json, out)?;
    }
    out.push(b']');
    let cursor = if start <= end {
        end.to_string()
    } else {
        match &req.pagination {
            Some(p) if !p.cursor.is_empty() => p.cursor.clone(),
            _ => (start - 1).to_string(),
        }
    };
    let head_time = replay.close_time(head).map_err(internal)?;
    let mut front = b"{".to_vec();
    members(
        &mut front,
        &[
            ("cursor", &cursor),
            ("latestLedger", &head),
            ("latestLedgerCloseTime", &head_time),
        ],
    )?;
    front.push(b',');
    insert(out, at, &front);
    out.push(b',');
    members(
        out,
        &[
            ("oldestLedger", &first),
            ("oldestLedgerCloseTime", &replay.first_close_time),
        ],
    )?;
    out.push(b'}');
    Ok(())
}

/// `"key":value` pairs, comma separated, in the order given, which the caller keeps sorted.
fn members(out: &mut Vec<u8>, pairs: &[(&str, &dyn erased::Value)]) -> Result<(), RpcError> {
    for (i, (key, v)) in pairs.iter().enumerate() {
        debug_assert!(i == 0 || pairs[i - 1].0 < *key, "members out of order");
        if i > 0 {
            out.push(b',');
        }
        text::write(out, *key)?;
        out.push(b':');
        v.write(out)?;
    }
    Ok(())
}

/// Puts `bytes` into `out` at `at`, moving what follows within the buffer.
fn insert(out: &mut Vec<u8>, at: usize, bytes: &[u8]) {
    out.splice(at..at, bytes.iter().copied());
}

/// A serialisable value behind a reference of one type, for lists of mixed members.
mod erased {
    pub trait Value {
        fn write(&self, out: &mut Vec<u8>) -> Result<(), serde_json::Error>;
    }

    impl<T: serde::Serialize> Value for T {
        fn write(&self, out: &mut Vec<u8>) -> Result<(), serde_json::Error> {
            crate::json::write(out, self)
        }
    }
}

/// Go's `strconv.ParseUint(s, 10, 32)` error text.
fn go_parse_uint_error(s: &str) -> String {
    let digits = !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let kind = if digits {
        "value out of range"
    } else {
        "invalid syntax"
    };
    format!("strconv.ParseUint: parsing {}: {kind}", go_quote(s))
}

fn go_parse_int_error(s: &str) -> String {
    let body = s.strip_prefix(['+', '-']).unwrap_or(s);
    let digits = !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit());
    let kind = if digits {
        "value out of range"
    } else {
        "invalid syntax"
    };
    format!("strconv.ParseInt: parsing {}: {kind}", go_quote(s))
}

fn go_quote(s: &str) -> String {
    serde_json::to_string(s).expect("a string serialises")
}

fn ledger_info(
    xdr: &[u8],
    lcm: &LedgerCloseMeta,
    as_json: bool,
    out: &mut Vec<u8>,
) -> Result<(), RpcError> {
    let h = header_entry(lcm);
    let mut info = Object::new(out);
    info.member("hash", &hex::encode(h.hash.0))?;
    if as_json {
        // go-stellar-sdk leaves the xdr fields empty but present in json mode
        info.member("headerJson", h)?;
        info.member("headerXdr", "")?;
    } else {
        info.member("headerXdr", &b64(h)?)?;
    }
    info.member(
        "ledgerCloseTime",
        &h.header.scp_value.close_time.0.to_string(),
    )?;
    if as_json {
        info.member("metadataJson", lcm)?;
        info.member("metadataXdr", "")?;
    } else {
        info.member("metadataXdr", &b64_bytes(xdr))?;
    }
    info.member("sequence", &h.header.ledger_seq)?;
    info.end();
    Ok(())
}

#[derive(Debug, Default, Deserialize)]
struct GetLedgerEntriesRequest {
    #[serde(default)]
    keys: Vec<String>,
    #[serde(rename = "xdrFormat", default)]
    format: Option<String>,
}

fn get_ledger_entries(
    replay: &Replay,
    req: GetLedgerEntriesRequest,
    out: &mut Vec<u8>,
) -> Result<(), RpcError> {
    let as_json = check_format(&req.format).map_err(|m| RpcError::new(INVALID_PARAMS, m))?;
    if req.keys.len() > MAX_KEYS {
        return Err(RpcError::new(
            INVALID_PARAMS,
            format!(
                "key count ({}) exceeds maximum supported ({MAX_KEYS})",
                req.keys.len()
            ),
        ));
    }
    let mut keys = Vec::with_capacity(req.keys.len());
    for (i, k) in req.keys.iter().enumerate() {
        let key =
            LedgerKey::from_xdr_base64(k, request_limits("LedgerKey", k.len())).map_err(|_| {
                RpcError::new(
                    INVALID_PARAMS,
                    format!("cannot unmarshal key value {k} at index {i}"),
                )
            })?;
        if matches!(key, LedgerKey::Ttl(_)) {
            return Err(RpcError::new(
                INVALID_PARAMS,
                "ledger ttl entries cannot be queried directly",
            ));
        }
        keys.push(key);
    }
    let found = replay
        .with_state(|state, head| {
            let entries: Vec<(LedgerKey, LedgerEntry, Option<u32>)> = keys
                .iter()
                .filter_map(|k| {
                    let e = state.get(k)?.clone();
                    let ttl = live_until(state, k);
                    Some((k.clone(), e, ttl))
                })
                .collect();
            (entries, head)
        })
        .map_err(internal)?;
    let (entries, head) = found;
    out.extend_from_slice(b"{\"entries\":[");
    for (i, (key, entry, ttl)) in entries.iter().enumerate() {
        if i > 0 {
            out.push(b',');
        }
        let mut r = Object::new(out);
        if as_json {
            r.member("keyJson", key)?;
            r.member("dataJson", &entry.data)?;
            r.member("extJson", &entry.ext)?;
        } else {
            r.member("key", &b64(key)?)?;
            r.member("xdr", &b64(&entry.data)?)?;
            r.member("extXdr", &b64(&entry.ext)?)?;
        }
        r.member("lastModifiedLedgerSeq", &entry.last_modified_ledger_seq)?;
        if let Some(t) = ttl {
            r.member("liveUntilLedgerSeq", t)?;
        }
        r.end();
    }
    out.extend_from_slice(b"],");
    members(out, &[("latestLedger", &head)])?;
    out.push(b'}');
    Ok(())
}

/// TTL of a contract data or code entry, from its TTL entry keyed by the key's hash.
fn live_until(state: &shapekit_oracle::LedgerState, key: &LedgerKey) -> Option<u32> {
    if !matches!(key, LedgerKey::ContractData(_) | LedgerKey::ContractCode(_)) {
        return None;
    }
    let bytes = key.to_xdr(Limits::none()).ok()?;
    let ttl_key = LedgerKey::Ttl(LedgerKeyTtl {
        key_hash: stellar_xdr::Hash(Sha256::digest(bytes).into()),
    });
    match &state.get(&ttl_key)?.data {
        stellar_xdr::LedgerEntryData::Ttl(t) => Some(t.live_until_ledger_seq),
        _ => None,
    }
}

#[derive(Debug, Default, Deserialize)]
struct GetTransactionRequest {
    #[serde(default)]
    hash: String,
    #[serde(rename = "xdrFormat", default)]
    format: Option<String>,
}

fn get_transaction(
    replay: &Replay,
    req: GetTransactionRequest,
    out: &mut Vec<u8>,
) -> Result<(), RpcError> {
    let as_json = check_format(&req.format).map_err(|m| RpcError::new(INVALID_PARAMS, m))?;
    if req.hash.len() / 2 != 32 {
        return Err(RpcError::new(
            INVALID_PARAMS,
            format!("unexpected hash length ({})", req.hash.len()),
        ));
    }
    let mut hash = [0u8; 32];
    hex::decode_to_slice(&req.hash, &mut hash)
        .map_err(|e| RpcError::new(INVALID_PARAMS, format!("incorrect hash: {e}")))?;
    let (head, head_time) = latest(replay)?;
    let loc = replay.tx(&hash).filter(|l| l.ledger <= head);
    let found = match loc {
        Some(loc) => {
            let (_, lcm) = replay.ledger(loc.ledger).map_err(internal)?;
            Some((loc, lcm))
        }
        None => None,
    };
    let mut o = Object::new(out);
    o.member("latestLedger", &head)?;
    o.member("latestLedgerCloseTime", &head_time.to_string())?;
    o.member("oldestLedger", &replay.first)?;
    o.member(
        "oldestLedgerCloseTime",
        &replay.first_close_time.to_string(),
    )?;
    let Some((loc, lcm)) = found else {
        o.member("status", "NOT_FOUND")?;
        o.member("txHash", &req.hash)?;
        o.member("applicationOrder", &0)?;
        o.member("feeBump", &false)?;
        o.member("events", &json!({}))?;
        o.member("ledger", &0)?;
        o.member("createdAt", "0")?;
        o.end();
        return Ok(());
    };
    let close_time = header_entry(&lcm).header.scp_value.close_time.0;
    let view = LedgerView::new(&lcm, &replay.network).map_err(internal)?;
    let tx = &view.txs[loc.index as usize];
    tx_details(tx, loc.ledger, as_json, &mut o)?;
    o.member("createdAt", &close_time.to_string())?;
    o.end();
    Ok(())
}

/// `TransactionDetails` for one transaction, as stellar-rpc's `ParseTransaction` and
/// `GetTransactionEvents` extract it, added to `o`. The hash is the one the ledger records.
fn tx_details(
    tx: &shapekit_oracle::view::TxView<'_>,
    ledger: u32,
    as_json: bool,
    o: &mut Object<'_>,
) -> Result<(), RpcError> {
    let result = &tx.result.result;
    let success = matches!(
        result.result,
        TransactionResultResult::TxSuccess(_) | TransactionResultResult::TxFeeBumpInnerSuccess(_)
    );
    let fee_bump = matches!(tx.envelope, TransactionEnvelope::TxFeeBump(_));
    let (tx_events, op_events, diagnostic) = events(tx.meta, tx.envelope);
    o.member("status", if success { "SUCCESS" } else { "FAILED" })?;
    o.member("txHash", &hex::encode(tx.result.transaction_hash.0))?;
    o.member("applicationOrder", &(tx.index + 1))?;
    o.member("feeBump", &fee_bump)?;
    if as_json {
        o.member("envelopeJson", tx.envelope)?;
        o.member("resultJson", result)?;
        o.member("resultMetaJson", tx.meta)?;
        if !diagnostic.is_empty() {
            o.member("diagnosticEventsJson", &diagnostic)?;
        }
    } else {
        o.member("envelopeXdr", &b64(tx.envelope)?)?;
        o.member("resultXdr", &b64(result)?)?;
        o.member("resultMetaXdr", &b64(tx.meta)?)?;
        if !diagnostic.is_empty() {
            o.member("diagnosticEventsXdr", &all_b64(&diagnostic)?)?;
        }
    }
    o.member_with("events", |out| -> Result<(), RpcError> {
        let mut ev = Object::new(out);
        if as_json {
            if !tx_events.is_empty() {
                ev.member("transactionEventsJson", &tx_events)?;
            }
            if !op_events.is_empty() {
                ev.member("contractEventsJson", &op_events)?;
            }
        } else {
            if !tx_events.is_empty() {
                ev.member("transactionEventsXdr", &all_b64(&tx_events)?)?;
            }
            if !op_events.is_empty() {
                let c = op_events
                    .iter()
                    .map(|op| all_b64(op))
                    .collect::<Result<Vec<_>, _>>()?;
                ev.member("contractEventsXdr", &c)?;
            }
        }
        ev.end();
        Ok(())
    })?;
    o.member("ledger", &ledger)?;
    Ok(())
}

fn all_b64<T: WriteXdr>(items: &[&T]) -> Result<Vec<String>, RpcError> {
    items.iter().map(|e| b64(*e)).collect()
}

type Events<'a> = (
    Vec<&'a stellar_xdr::TransactionEvent>,
    Vec<Vec<&'a stellar_xdr::ContractEvent>>,
    Vec<&'a stellar_xdr::DiagnosticEvent>,
);

/// go-stellar-sdk `LedgerTransaction.GetTransactionEvents` and `GetDiagnosticEvents`.
fn events<'a>(meta: &'a TransactionMeta, env: &TransactionEnvelope) -> Events<'a> {
    match meta {
        TransactionMeta::V3(m) => {
            let Some(s) = &m.soroban_meta else {
                return (vec![], vec![], vec![]);
            };
            let diagnostic = s.diagnostic_events.iter().collect();
            // only a soroban transaction reports its contract events, as one operation
            if is_soroban(env) {
                (vec![], vec![s.events.iter().collect()], diagnostic)
            } else {
                (vec![], vec![], diagnostic)
            }
        }
        TransactionMeta::V4(m) => (
            m.events.iter().collect(),
            m.operations
                .iter()
                .map(|op| op.events.iter().collect())
                .collect(),
            m.diagnostic_events.iter().collect(),
        ),
        _ => (vec![], vec![], vec![]),
    }
}

fn is_soroban(env: &TransactionEnvelope) -> bool {
    use stellar_xdr::{FeeBumpTransactionInnerTx, TransactionExt};
    let ext = match env {
        TransactionEnvelope::TxV0(_) => return false,
        TransactionEnvelope::Tx(e) => &e.tx.ext,
        TransactionEnvelope::TxFeeBump(e) => match &e.tx.inner_tx {
            FeeBumpTransactionInnerTx::Tx(i) => &i.tx.ext,
        },
    };
    matches!(ext, TransactionExt::V1(_))
}

#[derive(Debug, Default, Deserialize)]
struct GetTransactionsRequest {
    #[serde(rename = "startLedger", default)]
    start_ledger: u32,
    #[serde(default)]
    pagination: Option<Pagination>,
    #[serde(rename = "xdrFormat", default)]
    format: Option<String>,
}

/// Total order id of go-stellar-sdk `toid`: ledger, transaction order, operation order.
fn toid(ledger: i64, tx: i64, op: i64) -> i64 {
    (ledger << 32) | (tx << 12) | op
}

fn get_transactions(
    replay: &Replay,
    req: GetTransactionsRequest,
    out: &mut Vec<u8>,
) -> Result<(), RpcError> {
    let (first, head) = (replay.first, replay.head());
    let as_json = check_format(&req.format);
    join([
        validate_pagination(req.start_ledger, &req.pagination, first, head),
        as_json.clone().map(|_| ()),
    ])
    .map_err(|m| RpcError::new(INVALID_REQUEST, m))?;
    let as_json = as_json.expect("joined above");
    if req.start_ledger > i32::MAX as u32 {
        return Err(RpcError::new(
            INVALID_PARAMS,
            "startLedger exceeds supported range",
        ));
    }
    let mut start_ledger = i64::from(req.start_ledger);
    let mut start_tx = 1i64;
    let mut limit = DEFAULT_LIMIT;
    if let Some(p) = &req.pagination {
        if !p.cursor.is_empty() {
            let c: i64 = p
                .cursor
                .parse()
                .map_err(|_| RpcError::new(INVALID_PARAMS, go_parse_int_error(&p.cursor)))?;
            start_ledger = c >> 32;
            start_tx = ((c >> 12) & 0xFFFFF) + 1;
        }
        if p.limit > 0 {
            limit = p.limit;
        }
    }
    // "cursor" comes first and is known only once the page is written, so the page is
    // written first and the members before it are put in front after
    let at = out.len();
    out.extend_from_slice(b"\"transactions\":[");
    let mut count = 0u64;
    let mut cursor = toid(0, 0, 0);
    let mut seq = start_ledger;
    while seq <= i64::from(head) {
        if seq < 0 {
            return Err(RpcError::new(
                INVALID_PARAMS,
                "cursor ledger sequence cannot be negative",
            ));
        }
        let s = seq as u32;
        if s < first {
            return Err(RpcError::new(
                INVALID_PARAMS,
                format!("database does not contain metadata for ledger: {s}"),
            ));
        }
        let (_, lcm) = replay.ledger(s).map_err(internal)?;
        let close_time = header_entry(&lcm).header.scp_value.close_time.0;
        let view = LedgerView::new(&lcm, &replay.network).map_err(internal)?;
        let from = if seq == start_ledger { start_tx } else { 1 };
        cursor = toid(seq, 0, 1);
        let mut done = false;
        for i in from..=(view.txs.len() as i64) {
            cursor = toid(seq, i, 1);
            if count > 0 {
                out.push(b',');
            }
            let mut d = Object::new(out);
            tx_details(&view.txs[(i - 1) as usize], s, as_json, &mut d)?;
            d.member("createdAt", &close_time)?;
            d.end();
            count += 1;
            if count >= limit {
                done = true;
                break;
            }
        }
        if done {
            break;
        }
        seq += 1;
    }
    out.extend_from_slice(b"]}");
    let (head, head_time) = latest(replay)?;
    let mut front = b"{".to_vec();
    members(
        &mut front,
        &[
            ("cursor", &cursor.to_string()),
            ("latestLedger", &head),
            ("latestLedgerCloseTimestamp", &head_time),
            ("oldestLedger", &first),
            ("oldestLedgerCloseTimestamp", &replay.first_close_time),
        ],
    )?;
    front.push(b',');
    insert(out, at, &front);
    Ok(())
}
