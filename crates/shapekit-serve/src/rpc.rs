//! JSON-RPC 2.0 methods, shaped after stellar-rpc (`cmd/stellar-rpc/internal/methods`)
//! and the request and response types of go-stellar-sdk v0.8.0 `protocols/rpc` (R10).
//! Error codes and messages follow those sources, so clients that branch on them behave as
//! they would against stellar-rpc. Where v30.0.0 and v30.0.1 differ (getLedgers with a
//! cursor at or past the tip), v30.0.1 is followed: it is the current release and the one
//! the conformance fixtures were captured from.

use serde::Deserialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use shapekit_manifest::xdr::json_value;
use shapekit_oracle::view::LedgerView;
use stellar_xdr::{
    LedgerCloseMeta, LedgerEntry, LedgerKey, LedgerKeyTtl, Limits, ReadXdr, TransactionEnvelope,
    TransactionMeta, TransactionResultResult, Type, WriteXdr,
};

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

/// Limits for decoding request XDR, which a client controls.
fn request_limits(len: usize) -> Limits {
    Limits { depth: 500, len }
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

/// Handles one request object and returns its response object, or `None` for a
/// notification (a request without an id).
pub fn handle(replay: &Replay, req: &Value) -> Option<Value> {
    let id = req.get("id").cloned();
    let result = dispatch(replay, req);
    let id = id?;
    Some(match result {
        Ok(v) => json!({"jsonrpc": "2.0", "id": id, "result": v}),
        Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": e.to_json()}),
    })
}

/// A response for a body that is not JSON at all.
pub fn parse_error(e: &serde_json::Error) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": Value::Null,
        "error": {"code": PARSE_ERROR, "message": "invalid request value", "data": e.to_string()},
    })
}

fn dispatch(replay: &Replay, req: &Value) -> Result<Value, RpcError> {
    if req.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(RpcError::new(INVALID_REQUEST, "invalid version marker"));
    }
    let method = req
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::new(INVALID_REQUEST, "empty method name"))?;
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    match method {
        "getHealth" => get_health(replay),
        "getNetwork" => get_network(replay),
        "getVersionInfo" => get_version_info(replay),
        "getLatestLedger" => get_latest_ledger(replay),
        "getLedgers" => get_ledgers(replay, parse(params)?),
        "getLedgerEntries" => get_ledger_entries(replay, parse(params)?),
        "getTransaction" => get_transaction(replay, parse(params)?),
        "getTransactions" => get_transactions(replay, parse(params)?),
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
    }
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

fn xdr_json(t: Type) -> Result<Value, RpcError> {
    json_value(&t).map_err(internal)
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

fn get_ledgers(replay: &Replay, req: GetLedgersRequest) -> Result<Value, RpcError> {
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
    let mut ledgers = Vec::new();
    for seq in start..=end {
        let (xdr, lcm) = replay.ledger(seq).map_err(|e| {
            RpcError::new(
                INTERNAL_ERROR,
                format!("error fetching ledgers from db: {e}"),
            )
        })?;
        ledgers.push(ledger_info(&xdr, lcm, as_json)?);
    }
    let cursor = match ledgers.last() {
        Some(_) => end.to_string(),
        None => match &req.pagination {
            Some(p) if !p.cursor.is_empty() => p.cursor.clone(),
            _ => (start - 1).to_string(),
        },
    };
    let head_time = replay.close_time(head).map_err(internal)?;
    Ok(json!({
        "ledgers": ledgers,
        "latestLedger": head,
        "latestLedgerCloseTime": head_time,
        "oldestLedger": first,
        "oldestLedgerCloseTime": replay.first_close_time,
        "cursor": cursor,
    }))
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

fn ledger_info(xdr: &[u8], lcm: LedgerCloseMeta, as_json: bool) -> Result<Value, RpcError> {
    let h = header_entry(&lcm).clone();
    let mut info = json!({
        "hash": hex::encode(h.hash.0),
        "sequence": h.header.ledger_seq,
        "ledgerCloseTime": h.header.scp_value.close_time.0.to_string(),
    });
    if as_json {
        // go-stellar-sdk leaves the xdr fields empty but present in json mode
        info["headerXdr"] = json!("");
        info["headerJson"] = xdr_json(Type::LedgerHeaderHistoryEntry(Box::new(h)))?;
        info["metadataXdr"] = json!("");
        info["metadataJson"] = xdr_json(Type::LedgerCloseMeta(Box::new(lcm)))?;
    } else {
        info["headerXdr"] = json!(b64(&h)?);
        info["metadataXdr"] = json!(b64_bytes(xdr));
    }
    Ok(info)
}

#[derive(Debug, Default, Deserialize)]
struct GetLedgerEntriesRequest {
    #[serde(default)]
    keys: Vec<String>,
    #[serde(rename = "xdrFormat", default)]
    format: Option<String>,
}

fn get_ledger_entries(replay: &Replay, req: GetLedgerEntriesRequest) -> Result<Value, RpcError> {
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
        let key = LedgerKey::from_xdr_base64(k, request_limits(k.len())).map_err(|_| {
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
    let mut out = Vec::with_capacity(entries.len());
    for (key, entry, ttl) in entries {
        let mut r = Map::new();
        if as_json {
            r.insert("keyJson".into(), xdr_json(Type::LedgerKey(Box::new(key)))?);
            r.insert(
                "dataJson".into(),
                xdr_json(Type::LedgerEntryData(Box::new(entry.data)))?,
            );
            r.insert(
                "extJson".into(),
                xdr_json(Type::LedgerEntryExt(Box::new(entry.ext)))?,
            );
        } else {
            r.insert("key".into(), json!(b64(&key)?));
            r.insert("xdr".into(), json!(b64(&entry.data)?));
            r.insert("extXdr".into(), json!(b64(&entry.ext)?));
        }
        r.insert(
            "lastModifiedLedgerSeq".into(),
            json!(entry.last_modified_ledger_seq),
        );
        if let Some(t) = ttl {
            r.insert("liveUntilLedgerSeq".into(), json!(t));
        }
        out.push(Value::Object(r));
    }
    Ok(json!({"entries": out, "latestLedger": head}))
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

fn get_transaction(replay: &Replay, req: GetTransactionRequest) -> Result<Value, RpcError> {
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
    let mut out = json!({
        "latestLedger": head,
        "latestLedgerCloseTime": head_time.to_string(),
        "oldestLedger": replay.first,
        "oldestLedgerCloseTime": replay.first_close_time.to_string(),
    });
    let loc = replay.tx(&hash).filter(|l| l.ledger <= head);
    let Some(loc) = loc else {
        let o = out.as_object_mut().expect("an object");
        o.insert("status".into(), json!("NOT_FOUND"));
        o.insert("txHash".into(), json!(req.hash));
        o.insert("applicationOrder".into(), json!(0));
        o.insert("feeBump".into(), json!(false));
        o.insert("events".into(), json!({}));
        o.insert("ledger".into(), json!(0));
        o.insert("createdAt".into(), json!("0"));
        return Ok(out);
    };
    let (_, lcm) = replay.ledger(loc.ledger).map_err(internal)?;
    let close_time = header_entry(&lcm).header.scp_value.close_time.0;
    let view = LedgerView::new(&lcm, &replay.network).map_err(internal)?;
    let tx = &view.txs[loc.index as usize];
    let details = tx_details(tx, loc.ledger, as_json)?;
    let o = out.as_object_mut().expect("an object");
    o.extend(details);
    o.insert("createdAt".into(), json!(close_time.to_string()));
    Ok(out)
}

/// `TransactionDetails` for one transaction, as stellar-rpc's `ParseTransaction` and
/// `GetTransactionEvents` extract it. The hash is the one the ledger records.
fn tx_details(
    tx: &shapekit_oracle::view::TxView<'_>,
    ledger: u32,
    as_json: bool,
) -> Result<Map<String, Value>, RpcError> {
    let result = &tx.result.result;
    let success = matches!(
        result.result,
        TransactionResultResult::TxSuccess(_) | TransactionResultResult::TxFeeBumpInnerSuccess(_)
    );
    let fee_bump = matches!(tx.envelope, TransactionEnvelope::TxFeeBump(_));
    let (tx_events, op_events, diagnostic) = events(tx.meta, tx.envelope);
    let mut o = Map::new();
    o.insert(
        "status".into(),
        json!(if success { "SUCCESS" } else { "FAILED" }),
    );
    o.insert(
        "txHash".into(),
        json!(hex::encode(tx.result.transaction_hash.0)),
    );
    o.insert("applicationOrder".into(), json!(tx.index + 1));
    o.insert("feeBump".into(), json!(fee_bump));
    let mut ev = Map::new();
    if as_json {
        o.insert(
            "envelopeJson".into(),
            xdr_json(Type::TransactionEnvelope(Box::new(tx.envelope.clone())))?,
        );
        o.insert(
            "resultJson".into(),
            xdr_json(Type::TransactionResult(Box::new(result.clone())))?,
        );
        o.insert(
            "resultMetaJson".into(),
            xdr_json(Type::TransactionMeta(Box::new(tx.meta.clone())))?,
        );
        if !diagnostic.is_empty() {
            let d = diagnostic
                .iter()
                .map(|e| xdr_json(Type::DiagnosticEvent(Box::new((*e).clone()))))
                .collect::<Result<Vec<_>, _>>()?;
            o.insert("diagnosticEventsJson".into(), json!(d));
        }
        if !tx_events.is_empty() {
            let t = tx_events
                .iter()
                .map(|e| xdr_json(Type::TransactionEvent(Box::new((*e).clone()))))
                .collect::<Result<Vec<_>, _>>()?;
            ev.insert("transactionEventsJson".into(), json!(t));
        }
        if !op_events.is_empty() {
            let c = op_events
                .iter()
                .map(|op| {
                    op.iter()
                        .map(|e| xdr_json(Type::ContractEvent(Box::new((*e).clone()))))
                        .collect::<Result<Vec<_>, _>>()
                })
                .collect::<Result<Vec<_>, _>>()?;
            ev.insert("contractEventsJson".into(), json!(c));
        }
    } else {
        o.insert("envelopeXdr".into(), json!(b64(tx.envelope)?));
        o.insert("resultXdr".into(), json!(b64(result)?));
        o.insert("resultMetaXdr".into(), json!(b64(tx.meta)?));
        if !diagnostic.is_empty() {
            let d = diagnostic
                .iter()
                .map(|e| b64(*e))
                .collect::<Result<Vec<_>, _>>()?;
            o.insert("diagnosticEventsXdr".into(), json!(d));
        }
        if !tx_events.is_empty() {
            let t = tx_events
                .iter()
                .map(|e| b64(*e))
                .collect::<Result<Vec<_>, _>>()?;
            ev.insert("transactionEventsXdr".into(), json!(t));
        }
        if !op_events.is_empty() {
            let c = op_events
                .iter()
                .map(|op| op.iter().map(|e| b64(*e)).collect::<Result<Vec<_>, _>>())
                .collect::<Result<Vec<_>, _>>()?;
            ev.insert("contractEventsXdr".into(), json!(c));
        }
    }
    o.insert("events".into(), Value::Object(ev));
    o.insert("ledger".into(), json!(ledger));
    Ok(o)
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

fn get_transactions(replay: &Replay, req: GetTransactionsRequest) -> Result<Value, RpcError> {
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
    let mut txs = Vec::new();
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
            let mut d = tx_details(&view.txs[(i - 1) as usize], s, as_json)?;
            d.insert("createdAt".into(), json!(close_time));
            txs.push(Value::Object(d));
            if txs.len() as u64 >= limit {
                done = true;
                break;
            }
        }
        if done {
            break;
        }
        seq += 1;
    }
    let (head, head_time) = latest(replay)?;
    Ok(json!({
        "transactions": txs,
        "latestLedger": head,
        "latestLedgerCloseTimestamp": head_time,
        "oldestLedger": first,
        "oldestLedgerCloseTimestamp": replay.first_close_time,
        "cursor": cursor.to_string(),
    }))
}
