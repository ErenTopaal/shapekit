//! Responses are written as text without a value tree in between. Over every committed
//! chain, every method answers in the exact bytes a `serde_json::Value` of it prints (keys
//! sorted, compact, no key twice), and every json format answer is its base64 twin with each
//! XDR field replaced by the XDR-JSON of the decoded value.

use std::path::Path;

use serde::Deserialize;
use serde_json::{Map, Value, json};
use shapekit_serve::replay::Replay;
use shapekit_serve::{DEFAULT_CACHE_BYTES, rpc};
use stellar_xdr::{LedgerKey, Limits, Type, TypeVariant, WriteXdr};

/// The base64 field, its json format counterpart, the XDR type, and whether json format
/// keeps the base64 field as an empty string (getLedgers does, after go-stellar-sdk).
const XDR_FIELDS: &[(&str, &str, TypeVariant, bool)] = &[
    (
        "headerXdr",
        "headerJson",
        TypeVariant::LedgerHeaderHistoryEntry,
        true,
    ),
    (
        "metadataXdr",
        "metadataJson",
        TypeVariant::LedgerCloseMeta,
        true,
    ),
    (
        "envelopeXdr",
        "envelopeJson",
        TypeVariant::TransactionEnvelope,
        false,
    ),
    (
        "resultXdr",
        "resultJson",
        TypeVariant::TransactionResult,
        false,
    ),
    (
        "resultMetaXdr",
        "resultMetaJson",
        TypeVariant::TransactionMeta,
        false,
    ),
    (
        "diagnosticEventsXdr",
        "diagnosticEventsJson",
        TypeVariant::DiagnosticEvent,
        false,
    ),
    (
        "transactionEventsXdr",
        "transactionEventsJson",
        TypeVariant::TransactionEvent,
        false,
    ),
    (
        "contractEventsXdr",
        "contractEventsJson",
        TypeVariant::ContractEvent,
        false,
    ),
    ("key", "keyJson", TypeVariant::LedgerKey, false),
    ("xdr", "dataJson", TypeVariant::LedgerEntryData, false),
    ("extXdr", "extJson", TypeVariant::LedgerEntryExt, false),
];

fn parse(text: &str) -> Value {
    let mut de = serde_json::Deserializer::from_str(text);
    de.disable_recursion_limit();
    Value::deserialize(&mut de).unwrap()
}

/// One request's response, checked to be in the canonical bytes of its value.
fn call(replay: &Replay, method: &str, params: Value) -> Value {
    let req = json!({"jsonrpc": "2.0", "id": 7, "method": method, "params": params});
    let mut out = Vec::new();
    assert!(rpc::handle(replay, &req, &mut out));
    let text = String::from_utf8(out).unwrap();
    let v = parse(&text);
    // the value prints its keys sorted and compact, as responses were once made
    let canonical = v.to_string();
    assert!(canonical == text, "{req}: not in canonical form");
    v
}

/// The XDR-JSON of a base64 field, through arrays of any depth.
fn to_json(t: TypeVariant, v: &Value) -> Value {
    match v {
        Value::String(s) => {
            serde_json::to_value(Type::from_xdr_base64(t, s, Limits::none()).unwrap()).unwrap()
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| to_json(t, x)).collect()),
        other => panic!("not an XDR field: {other}"),
    }
}

/// The json format answer the base64 answer `b` implies.
fn json_twin(b: &Value) -> Value {
    match b {
        Value::Object(o) => {
            let mut want = Map::new();
            for (k, v) in o {
                match XDR_FIELDS.iter().find(|f| f.0 == k) {
                    Some(&(_, json_key, t, keep)) => {
                        want.insert(json_key.into(), to_json(t, v));
                        if keep {
                            want.insert(k.clone(), json!(""));
                        }
                    }
                    None => {
                        want.insert(k.clone(), json_twin(v));
                    }
                }
            }
            Value::Object(want)
        }
        Value::Array(a) => Value::Array(a.iter().map(json_twin).collect()),
        other => other.clone(),
    }
}

/// The same request in both formats, and the format-free variants answering as base64.
fn both(replay: &Replay, method: &str, mut params: Value) -> Value {
    params["xdrFormat"] = json!("base64");
    let b = call(replay, method, params.clone());
    for same in [json!(null), json!("")] {
        params["xdrFormat"] = same;
        assert_eq!(call(replay, method, params.clone()), b, "{method} {params}");
    }
    params["xdrFormat"] = json!("json");
    let j = call(replay, method, params.clone());
    assert!(
        j == json_twin(&b),
        "{method} {params}: json is not its base64 twin"
    );
    b
}

fn check_chain(dir: &Path) {
    let replay = Replay::open(dir, None, DEFAULT_CACHE_BYTES).unwrap();
    let (first, head) = (replay.first, replay.head());
    for m in [
        "getHealth",
        "getNetwork",
        "getVersionInfo",
        "getLatestLedger",
        "getEvents",
    ] {
        call(&replay, m, Value::Null);
    }

    let mut start = first;
    let mut ledgers = 0;
    while start <= head {
        let r = both(
            &replay,
            "getLedgers",
            json!({"startLedger": start, "pagination": {"limit": 200}}),
        );
        let n = r["result"]["ledgers"].as_array().unwrap().len() as u32;
        ledgers += n;
        start += n;
    }
    assert_eq!(ledgers, head - first + 1);
    both(
        &replay,
        "getLedgers",
        json!({"pagination": {"cursor": head.to_string()}}),
    );
    both(&replay, "getLedgers", json!({"startLedger": head + 1}));

    let mut hashes = Vec::new();
    let mut cursor = String::new();
    loop {
        let p = if cursor.is_empty() {
            json!({"startLedger": first, "pagination": {"limit": 200}})
        } else {
            json!({"pagination": {"cursor": cursor, "limit": 200}})
        };
        let r = both(&replay, "getTransactions", p);
        let txs = r["result"]["transactions"].as_array().unwrap();
        hashes.extend(
            txs.iter()
                .map(|t| t["txHash"].as_str().unwrap().to_string()),
        );
        let next = r["result"]["cursor"].as_str().unwrap().to_string();
        if txs.is_empty() || next == cursor {
            break;
        }
        cursor = next;
    }
    both(
        &replay,
        "getTransactions",
        json!({"pagination": {"cursor": "x"}}),
    );

    hashes.push("00".repeat(32));
    for h in &hashes {
        both(&replay, "getTransaction", json!({"hash": h}));
    }

    let keys: Vec<String> = replay
        .with_state(|s, _| {
            s.iter()
                .filter(|(k, _)| !matches!(k, LedgerKey::Ttl(_)))
                .map(|(k, _)| k.to_xdr_base64(Limits::none()).unwrap())
                .collect()
        })
        .unwrap();
    for chunk in keys.chunks(200) {
        let r = both(&replay, "getLedgerEntries", json!({"keys": chunk}));
        assert_eq!(
            r["result"]["entries"].as_array().unwrap().len(),
            chunk.len()
        );
    }
}

#[test]
fn every_chain_answers_in_canonical_bytes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/chains");
    let mut dirs: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .flat_map(|c| std::fs::read_dir(c.unwrap().path()).unwrap())
        .map(|d| d.unwrap().path())
        .collect();
    dirs.sort();
    assert!(!dirs.is_empty());
    // deep XDR-JSON recurses in parsing, comparing and dropping
    std::thread::Builder::new()
        .stack_size(1 << 30)
        .spawn(move || {
            for d in &dirs {
                check_chain(d);
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn errors_and_notifications() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/chains/classic.failed-tx.p30/testnet");
    let replay = Replay::open(&dir, None, DEFAULT_CACHE_BYTES).unwrap();
    let mut out = Vec::new();
    assert!(!rpc::handle(
        &replay,
        &json!({"jsonrpc": "2.0", "method": "getHealth"}),
        &mut out
    ));
    assert!(out.is_empty());
    // an error answer carries the request id, its keys sorted like every other object
    let req = json!({"jsonrpc": "2.0", "id": {"z": 1, "a": 2}, "method": "getLedgers",
        "params": {"pagination": {"cursor": "0"}}});
    assert!(rpc::handle(&replay, &req, &mut out));
    let text = String::from_utf8(out).unwrap();
    assert_eq!(parse(&text).to_string(), text);
    assert!(text.starts_with(r#"{"error":{"code":-32602,"#), "{text}");
    assert!(
        text.ends_with(r#""id":{"a":2,"z":1},"jsonrpc":"2.0"}"#),
        "{text}"
    );
}
