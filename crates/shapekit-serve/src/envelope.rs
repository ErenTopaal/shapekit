//! The JSON-RPC 2.0 envelope around the methods, handled as stellar-rpc v30.0.1 handles it:
//! its HTTP bridge (`cmd/stellar-rpc/internal/jsonrpc/bridge.go`), the batch limit in
//! front of it (`jsonrpc/batchlimit.go`) and jrpc2 v1.3.3's request parser (`json.go`,
//! `jmessages.parseJSON` and `jmessage.parseJSON`), which the bridge uses unchanged.
//!
//! What that means on the wire, checked against soroban-testnet (stellar-rpc 30.0.1) in
//! `tests/fixtures/stellar-rpc/gettransactions.json`:
//! - a batch of more than [`MAX_BATCH_LEN`] calls gets a batch holding one -32600 error,
//!   and an empty batch a single -32600 error, without running anything;
//! - a body or batch member that is not an object gets -32700 "request is not a JSON
//!   object" with a null id. JSON-RPC 2.0's examples show -32600 for this case, but jrpc2
//!   answers -32700 and clients of stellar-rpc see that, so the replay does too;
//! - invalid members are answered before valid ones, valid ones in request order, and
//!   notifications (no id, or a null id) never get an answer, so a body of nothing but
//!   notifications gets 204.
//!
//! Every answer of one HTTP request shares one byte budget, [`MAX_RESPONSE_BYTES`].

use serde_json::{Value, json};

use crate::replay::Replay;
use crate::rpc::{self, INVALID_REQUEST, PARSE_ERROR, Room, RpcError};

/// Most calls in one batch: stellar-rpc's `MAX_REQUEST_BATCH_SIZE` default
/// (`cmd/stellar-rpc/internal/rpcv1/config/options.go`, v30.0.1).
pub const MAX_BATCH_LEN: usize = 10;

/// Most bytes the answers to one HTTP request may hold before they are written. stellar-rpc
/// bounds the response bytes held across all requests (`max-total-response-bytes`, 2 GiB
/// by default) and fails a call that does not fit; the replay bounds each request instead,
/// so its peak is at most [`crate::BLOCKING_THREADS`] times this. The largest answer the
/// committed corpus gives is 3.8 MB (getTransactions in json format on
/// classic.failed-tx.p30), well inside.
pub const MAX_RESPONSE_BYTES: usize = 64 << 20;

/// Answers up to this size always fit, so cheap calls such as getHealth still answer when
/// the budget is spent: bridge.go's `smallResponseBytes`.
pub const SMALL_RESPONSE_BYTES: usize = 4 << 10;

/// One request object as jrpc2 classifies it before anything runs.
enum Member<'a> {
    /// Answered with this error and id (null unless the request carried a valid one).
    Invalid {
        id: Value,
        error: RpcError,
    },
    Notification,
    Call {
        id: &'a Value,
        req: &'a Value,
    },
}

fn invalid(id: Value, code: i64, message: &str) -> Member<'static> {
    Member::Invalid {
        id,
        error: RpcError {
            code,
            message: message.into(),
            data: None,
        },
    }
}

/// jrpc2's `jmessage.parseJSON` and the checks after it. Go visits the keys in map order and
/// keeps the first failure, so with several faults its message can vary between calls; here
/// the keys are checked in one fixed order.
fn classify(v: &Value) -> Member<'_> {
    let obj = match v {
        Value::Object(o) => o,
        // json null unmarshals into an empty map, which then lacks the version marker
        Value::Null => return invalid(Value::Null, INVALID_REQUEST, "invalid version marker"),
        _ => return invalid(Value::Null, PARSE_ERROR, "request is not a JSON object"),
    };
    let mut error: Option<RpcError> = None;
    let mut fail = |code: i64, message: &str| {
        error.get_or_insert_with(|| RpcError {
            code,
            message: message.into(),
            data: None,
        });
    };
    let version = match obj.get("jsonrpc") {
        None => "",
        Some(Value::String(s)) => s.as_str(),
        Some(_) => {
            fail(PARSE_ERROR, "invalid version key");
            ""
        }
    };
    let id = match obj.get("id") {
        Some(v @ (Value::String(_) | Value::Number(_))) => Some(v),
        None | Some(Value::Null) => None,
        Some(_) => {
            fail(INVALID_REQUEST, "invalid request ID");
            None
        }
    };
    let method = match obj.get("method") {
        None => "",
        Some(Value::String(s)) => s.as_str(),
        Some(_) => {
            fail(PARSE_ERROR, "invalid method name");
            ""
        }
    };
    if let Some(p) = obj.get("params")
        && !matches!(p, Value::Null | Value::Array(_) | Value::Object(_))
    {
        fail(INVALID_REQUEST, "parameters must be array or object");
    }
    let has_error = match obj.get("error") {
        None | Some(Value::Null) => false,
        Some(Value::Object(e)) => {
            let typed = e.get("code").is_none_or(Value::is_i64)
                && e.get("message").is_none_or(Value::is_string);
            if !typed {
                fail(PARSE_ERROR, "invalid error value");
            }
            true
        }
        Some(_) => {
            fail(PARSE_ERROR, "invalid error value");
            true
        }
    };
    let has_result = obj.contains_key("result");
    let mut extra: Vec<&str> = obj
        .keys()
        .map(String::as_str)
        .filter(|k| {
            !matches!(
                *k,
                "jsonrpc" | "id" | "method" | "params" | "error" | "result"
            )
        })
        .collect();
    if version != "2.0" {
        fail(INVALID_REQUEST, "invalid version marker");
    }
    if !method.is_empty() && (has_error || has_result) {
        fail(INVALID_REQUEST, "mixed request and reply fields");
    }
    let echoed = id.cloned().unwrap_or(Value::Null);
    if let Some(error) = error {
        return Member::Invalid { id: echoed, error };
    }
    if !extra.is_empty() {
        extra.sort_unstable();
        return Member::Invalid {
            id: echoed,
            error: RpcError {
                code: INVALID_REQUEST,
                message: "extra fields in request".into(),
                data: Some(json!(extra)),
            },
        };
    }
    match id {
        None => Member::Notification,
        Some(id) => Member::Call { id, req: v },
    }
}

fn write_error(out: &mut Vec<u8>, id: &Value, e: &RpcError) {
    let v = json!({"jsonrpc": "2.0", "id": id, "error": e.to_json()});
    crate::json::write(out, &v).expect("an error object of plain values serialises");
}

fn error_text(message: &str) -> Vec<u8> {
    let mut out = Vec::new();
    write_error(
        &mut out,
        &Value::Null,
        &RpcError {
            code: INVALID_REQUEST,
            message: message.into(),
            data: None,
        },
    );
    out
}

/// Answers one request object into `out`, unless it is a notification, which gets nothing
/// and makes this return false.
pub(crate) fn answer_member(
    v: &Value,
    out: &mut Vec<u8>,
    budget: usize,
    call: impl FnOnce(&Value, &Value, &mut Vec<u8>, Room),
) -> bool {
    match classify(v) {
        Member::Invalid { id, error } => write_error(out, &id, &error),
        Member::Notification => return false,
        Member::Call { id, req } => {
            let at = out.len();
            call(id, req, out, Room::new(at, at, budget));
        }
    }
    true
}

/// Answers one HTTP request body. `None` means no answer at all (HTTP 204): the body held
/// only notifications. `budget` bounds the bytes of all its answers together.
pub fn answer_body(replay: &Replay, body: &[u8], budget: usize, log: bool) -> Option<Vec<u8>> {
    let req: Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => return Some(rpc::parse_error(&e).to_string().into_bytes()),
    };
    let call = |id: &Value, req: &Value, out: &mut Vec<u8>, room: Room| {
        if log {
            eprintln!(
                "rpc {}",
                req.get("method").and_then(Value::as_str).unwrap_or("?")
            );
        }
        rpc::answer(replay, id, req, out, room);
    };
    let items = match &req {
        Value::Array(items) => items,
        single => {
            let mut out = Vec::new();
            return answer_member(single, &mut out, budget, call).then_some(out);
        }
    };
    if items.len() > MAX_BATCH_LEN {
        // a batch gets a batch reply, here holding one error (batchlimit.go)
        let mut out = b"[".to_vec();
        out.extend(error_text(&format!(
            "batch exceeds the limit of {MAX_BATCH_LEN} calls"
        )));
        out.push(b']');
        return Some(out);
    }
    if items.is_empty() {
        return Some(error_text("empty batch"));
    }
    let members: Vec<Member<'_>> = items.iter().map(classify).collect();
    let mut out = b"[".to_vec();
    let mut any = false;
    let mut sep = |out: &mut Vec<u8>| {
        if any {
            out.push(b',');
        }
        any = true;
    };
    // invalid requests are answered first, as the bridge sorts them (bridge.go `rank`)
    for m in &members {
        if let Member::Invalid { id, error } = m {
            sep(&mut out);
            write_error(&mut out, id, error);
        }
    }
    for m in &members {
        if let Member::Call { id, req } = m {
            sep(&mut out);
            let at = out.len();
            call(id, req, &mut out, Room::new(1, at, budget));
        }
    }
    if !any {
        return None;
    }
    out.push(b']');
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_call(v: &Value) -> bool {
        matches!(classify(v), Member::Call { .. })
    }

    fn error_of(v: Value) -> (Value, i64, String) {
        match classify(&v) {
            Member::Invalid { id, error } => (id, error.code, error.message),
            Member::Notification => panic!("{v} is a notification"),
            Member::Call { .. } => panic!("{v} is a call"),
        }
    }

    #[test]
    fn members_are_classified_as_jrpc2_does() {
        assert_eq!(
            error_of(json!(5)),
            (Value::Null, -32700, "request is not a JSON object".into())
        );
        assert_eq!(
            error_of(json!("getHealth")),
            (Value::Null, -32700, "request is not a JSON object".into())
        );
        assert_eq!(
            error_of(json!([1])),
            (Value::Null, -32700, "request is not a JSON object".into())
        );
        assert_eq!(
            error_of(Value::Null),
            (Value::Null, -32600, "invalid version marker".into())
        );
        assert_eq!(
            error_of(json!({"jsonrpc": "1.0", "id": 4, "method": "m"})),
            (json!(4), -32600, "invalid version marker".into())
        );
        assert_eq!(
            error_of(json!({"jsonrpc": "2.0", "id": {"a": 1}, "method": "m"})),
            (Value::Null, -32600, "invalid request ID".into())
        );
        assert_eq!(
            error_of(json!({"jsonrpc": "2.0", "id": 6, "method": "m", "params": 1})),
            (
                json!(6),
                -32600,
                "parameters must be array or object".into()
            )
        );
        assert_eq!(
            error_of(json!({"jsonrpc": "2.0", "id": 5, "method": "m", "x": 1})),
            (json!(5), -32600, "extra fields in request".into())
        );
        assert_eq!(
            error_of(json!({"jsonrpc": "2.0", "id": 1, "method": "m", "result": 1})),
            (json!(1), -32600, "mixed request and reply fields".into())
        );
        // an invalid request is answered even without an id
        assert_eq!(
            error_of(json!({"jsonrpc": "1.0", "method": "m"})),
            (Value::Null, -32600, "invalid version marker".into())
        );
        assert!(matches!(
            classify(&json!({"jsonrpc": "2.0", "id": null, "method": "m"})),
            Member::Notification
        ));
        assert!(matches!(
            classify(&json!({"jsonrpc": "2.0", "method": "m"})),
            Member::Notification
        ));
        assert!(is_call(
            &json!({"jsonrpc": "2.0", "id": "a", "method": "m"})
        ));
        // no method is valid at this stage; the call is answered "empty method name" in order
        assert!(is_call(&json!({"jsonrpc": "2.0", "id": 3})));
        // a reply object sent as a request is valid too, and also has no method
        assert!(is_call(&json!({"jsonrpc": "2.0", "id": 1, "result": 2})));
    }

    #[test]
    fn extra_fields_are_named() {
        let Member::Invalid { error, .. } =
            classify(&json!({"jsonrpc": "2.0", "id": 5, "method": "m", "z": 1, "a": 2}))
        else {
            panic!("not invalid");
        };
        assert_eq!(error.data, Some(json!(["a", "z"])));
    }
}
