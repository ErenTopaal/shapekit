//! getTransactions paging and the JSON-RPC envelope against a capture from a live
//! stellar-rpc 30.0.1 (soroban-testnet, `tests/fixtures/stellar-rpc/gettransactions.json`,
//! provenance beside it). The captured walk shows where the real server puts its cursor;
//! the replay is checked to page the same way over a recorded chain, to answer in the same
//! shapes and error messages, and to treat batches, invalid members and notifications the
//! way the captured server did.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use shapekit_serve::replay::Replay;
use shapekit_serve::{
    DEFAULT_CACHE_BYTES, MAX_BATCH_LEN, MAX_RESPONSE_BYTES, ServeConfig, Server, envelope, start,
};

fn chain() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/chains/classic.failed-tx.p30/testnet")
}

fn capture() -> Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/stellar-rpc/gettransactions.json");
    serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()
}

fn serve(head: Option<u32>) -> Server {
    start(ServeConfig {
        chain_dir: chain(),
        listen: "127.0.0.1:0".parse().unwrap(),
        head,
        cache_bytes: DEFAULT_CACHE_BYTES,
        log_requests: false,
    })
    .unwrap()
}

/// Posts a raw body; returns the http status and the parsed body, if any.
fn post(url: &str, body: &[u8]) -> (u16, Option<Value>) {
    let r = reqwest::blocking::Client::new()
        .post(url)
        .header("content-type", "application/json")
        .body(body.to_vec())
        .send()
        .unwrap();
    let status = r.status().as_u16();
    let text = r.text().unwrap();
    let v = (!text.is_empty()).then(|| {
        let mut de = serde_json::Deserializer::from_str(&text);
        de.disable_recursion_limit();
        serde::Deserialize::deserialize(&mut de).unwrap()
    });
    (status, v)
}

fn call(url: &str, params: Value) -> Value {
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": "getTransactions", "params": params});
    post(url, body.to_string().as_bytes()).1.unwrap()
}

fn jtype(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn key_types(v: &Value) -> Value {
    Value::Object(
        v.as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(jtype(v).into())))
            .collect(),
    )
}

/// A key set (key to json type) without the diagnostic events, which only some
/// transactions have.
fn strip(mut k: Value) -> Value {
    let o = k.as_object_mut().unwrap();
    o.remove("diagnosticEventsXdr");
    o.remove("diagnosticEventsJson");
    k
}

/// A transaction's key set, as [`strip`] leaves it.
fn tx_keys(t: &Value) -> Value {
    strip(key_types(t))
}

fn mask(message: &str) -> String {
    let mut out = String::new();
    let mut in_num = false;
    for c in message.chars() {
        if c.is_ascii_digit() {
            if !in_num {
                out.push_str("<n>");
            }
            in_num = true;
        } else {
            in_num = false;
            out.push(c);
        }
    }
    out
}

fn toid(ledger: u64, tx: u64, op: u64) -> u64 {
    (ledger << 32) | (tx << 12) | op
}

/// The paging rule a walk must show: a full page's cursor is its last transaction, and
/// pages follow each other with nothing repeated or skipped (the next transaction is the
/// next one in the same ledger or the first of a later ledger).
fn check_walk(pages: &[(Vec<(u64, u64)>, u64)], limit: usize) {
    let mut prev: Option<(u64, u64)> = None;
    for (txs, cursor) in pages {
        let last = *txs.last().expect("a page with transactions");
        if txs.len() == limit {
            assert_eq!(*cursor, toid(last.0, last.1, 1));
        } else {
            assert!(*cursor >= toid(last.0, last.1, 1));
        }
        for &t in txs {
            if let Some(p) = prev {
                assert!(
                    (t.0 == p.0 && t.1 == p.1 + 1) || (t.0 > p.0 && t.1 == 1),
                    "{t:?} follows {p:?}"
                );
            }
            prev = Some(t);
        }
    }
}

fn captured_pages(fx: &Value) -> Vec<(Vec<(u64, u64)>, u64)> {
    fx["walk"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| {
            let s = &w["summary"];
            let txs = s["txs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| {
                    (
                        t["ledger"].as_u64().unwrap(),
                        t["applicationOrder"].as_u64().unwrap(),
                    )
                })
                .collect();
            (txs, s["cursor"].as_str().unwrap().parse().unwrap())
        })
        .collect()
}

#[test]
fn paging_matches_captured_stellar_rpc() {
    let fx = capture();
    assert!(
        fx["server_version"]["version"]
            .as_str()
            .unwrap()
            .starts_with("30.0.1-")
    );
    let pages = captured_pages(&fx);
    assert_eq!(pages.len(), 12);
    // the rule holds on the real server: this is what the replay is held to
    let limit = fx["page_limit"].as_u64().unwrap();
    check_walk(&pages, limit as usize);

    let srv = serve(None);
    let url = srv.url();
    let want_top = &fx["walk"][0]["summary"]["top_keys"];
    let want_tx: Vec<Value> = fx["walk"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|w| w["summary"]["tx_key_sets"].as_array().unwrap().clone())
        .map(strip)
        .collect();
    let mut ours = Vec::new();
    let mut params = json!({"startLedger": 2, "pagination": {"limit": limit}});
    loop {
        let r = call(&url, params.clone());
        let res = &r["result"];
        assert_eq!(&key_types(res), want_top, "{r}");
        let txs = res["transactions"].as_array().unwrap();
        let cursor: u64 = res["cursor"].as_str().unwrap().parse().unwrap();
        if txs.is_empty() {
            // past ledgers without transactions the cursor still moves; at the tip it is
            // the request's own
            if res["cursor"] == params["pagination"]["cursor"] {
                break;
            }
            params = json!({"pagination": {"cursor": cursor.to_string(), "limit": limit}});
            continue;
        }
        for t in txs {
            assert!(want_tx.contains(&tx_keys(t)), "{}", key_types(t));
        }
        ours.push((
            txs.iter()
                .map(|t| {
                    (
                        t["ledger"].as_u64().unwrap(),
                        t["applicationOrder"].as_u64().unwrap(),
                    )
                })
                .collect::<Vec<_>>(),
            cursor,
        ));
        params = json!({"pagination": {"cursor": cursor.to_string(), "limit": limit}});
    }
    assert!(ours.len() > 20, "{} pages", ours.len());
    check_walk(&ours, limit as usize);

    // json format: the same key set as the captured json page
    let r = call(
        &url,
        json!({"startLedger": 2, "pagination": {"limit": 1}, "xdrFormat": "json"}),
    );
    let got = tx_keys(&r["result"]["transactions"][0]);
    let want = strip(fx["getTransactions"][0]["summary"]["tx_key_sets"][0].clone());
    assert_eq!(fx["getTransactions"][0]["name"], "json-limit-1");
    assert_eq!(got, want);
    srv.stop().unwrap();
}

#[test]
fn errors_and_caught_up_cursors_match_captured_stellar_rpc() {
    let fx = capture();
    let srv = serve(None);
    let url = srv.url();
    let replay = Replay::open(&chain(), None, DEFAULT_CACHE_BYTES).unwrap();
    let (first, head) = (u64::from(replay.first), u64::from(replay.head()));
    let mut compared = 0;
    for c in fx["getTransactions"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let s = &c["summary"];
        let params = match name {
            "json-limit-1" => continue,
            "cursor-past-tip" => {
                json!({"pagination": {"cursor": toid(head + 1000, 0, 0).to_string()}})
            }
            "cursor-past-tip-mid-ledger" => {
                json!({"pagination": {"cursor": toid(head + 1000, 7, 1).to_string()}})
            }
            "cursor-and-start" => json!({"startLedger": first, "pagination": {"cursor": "1"}}),
            "limit-201" => json!({"startLedger": first, "pagination": {"limit": 201}}),
            "start-after-latest" => json!({"startLedger": head + 100}),
            "format-unknown" => json!({"startLedger": first, "xdrFormat": "yaml"}),
            _ => c["params"].clone(),
        };
        let got = call(&url, params.clone());
        if s["kind"] == "error" {
            assert_eq!(got["error"]["code"], s["code"], "{name}: {got}");
            assert_eq!(
                mask(got["error"]["message"].as_str().unwrap()),
                s["message_template"].as_str().unwrap(),
                "{name}: {got}"
            );
        } else {
            // captured: an empty page whose cursor is the request's own
            assert_eq!(s["txs"], json!([]), "{name}");
            assert_eq!(s["cursor"], s["request_cursor"], "{name}");
            assert_eq!(got["result"]["transactions"], json!([]), "{name}: {got}");
            assert_eq!(
                got["result"]["cursor"], params["pagination"]["cursor"],
                "{name}: {got}"
            );
            assert_eq!(key_types(&got["result"]), s["top_keys"], "{name}");
        }
        compared += 1;
    }
    assert_eq!(compared, 8);
    srv.stop().unwrap();
}

/// What the captured server answered to a body, with getNetwork results reduced to their
/// keys and the friendbot url dropped (a replay has none, so stellar-rpc would omit it).
fn reduce(v: &Value) -> Value {
    let one = |r: &Value| -> Value {
        match r.get("result") {
            Some(res) => {
                let mut keys: Vec<&str> = res
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .filter(|k| *k != "friendbotUrl")
                    .collect();
                keys.sort_unstable();
                json!({"id": r["id"], "result_keys": keys})
            }
            None => match r.get("result_keys") {
                // already reduced by the capture tool
                Some(keys) => {
                    let keys: Vec<&Value> = keys
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|k| *k != "friendbotUrl")
                        .collect();
                    json!({"id": r["id"], "result_keys": keys})
                }
                None => r.clone(),
            },
        }
    };
    match v {
        Value::Array(a) => Value::Array(a.iter().map(one).collect()),
        other => one(other),
    }
}

#[test]
fn envelope_matches_captured_stellar_rpc() {
    let fx = capture();
    let srv = serve(None);
    let url = srv.url();
    let envelopes = fx["envelopes"].as_array().unwrap();
    for e in envelopes {
        let name = e["name"].as_str().unwrap();
        let (status, got) = post(&url, e["body"].as_str().unwrap().as_bytes());
        assert_eq!(
            u64::from(status),
            e["http_status"].as_u64().unwrap(),
            "{name}"
        );
        let want = if e["response"].is_null() {
            Value::Null
        } else {
            reduce(&e["response"])
        };
        let got = got.as_ref().map_or(Value::Null, reduce);
        assert_eq!(got, want, "{name}");
    }
    assert_eq!(envelopes.len(), 15);
    srv.stop().unwrap();
}

#[test]
fn batch_limit_is_stellar_rpcs() {
    assert_eq!(MAX_BATCH_LEN, 10);
    let replay = Replay::open(&chain(), None, DEFAULT_CACHE_BYTES).unwrap();
    let batch = |n: usize| {
        let calls: Vec<Value> = (0..n)
            .map(|i| json!({"jsonrpc": "2.0", "id": i, "method": "getHealth"}))
            .collect();
        let body = serde_json::to_vec(&calls).unwrap();
        let out = envelope::answer_body(&replay, &body, MAX_RESPONSE_BYTES, false).unwrap();
        serde_json::from_slice::<Value>(&out).unwrap()
    };
    assert_eq!(batch(10).as_array().unwrap().len(), 10);
    let over = batch(11);
    assert_eq!(
        over,
        json!([{"jsonrpc": "2.0", "id": null,
            "error": {"code": -32600, "message": "batch exceeds the limit of 10 calls"}}])
    );
}

/// The answers to one request share a byte budget: a call that would take them past it
/// gets stellar-rpc's "response is larger" error, a small call still answers, and the
/// answer stays near the budget instead of growing with the request.
#[test]
fn response_budget_bounds_a_request() {
    let replay = Replay::open(&chain(), None, DEFAULT_CACHE_BYTES).unwrap();
    let big = json!({"jsonrpc": "2.0", "id": 1, "method": "getLedgers",
        "params": {"startLedger": 2, "pagination": {"limit": 200}, "xdrFormat": "json"}});
    let health = json!({"jsonrpc": "2.0", "id": 2, "method": "getHealth"});
    let body = serde_json::to_vec(&json!([big, health, big])).unwrap();

    let unbounded = envelope::answer_body(&replay, &body, MAX_RESPONSE_BYTES, false).unwrap();
    let all: Value = parse(&unbounded);
    assert!(all[0]["result"].is_object() && all[2]["result"].is_object());
    let one_big = unbounded.len() / 2;

    let budget = one_big + one_big / 2;
    let out = envelope::answer_body(&replay, &body, budget, false).unwrap();
    assert!(
        out.len() < budget + 1024 * 1024,
        "{} bytes over a budget of {budget}",
        out.len()
    );
    let v = parse(&out);
    assert!(v[0]["result"].is_object(), "the first call fits");
    assert!(v[1]["result"].is_object(), "a small call always fits");
    assert_eq!(
        v[2]["error"],
        json!({"code": -32098,
            "message": "the response is larger than the node may hold; request fewer items"})
    );
    assert_eq!(v[2]["id"], 1);

    // alone, a call larger than the budget fails the same way
    let alone = serde_json::to_vec(&big).unwrap();
    let out = envelope::answer_body(&replay, &alone, 64 * 1024, false).unwrap();
    assert_eq!(parse(&out)["error"]["code"], -32098);
}

fn parse(bytes: &[u8]) -> Value {
    let mut de = serde_json::Deserializer::from_slice(bytes);
    de.disable_recursion_limit();
    serde::Deserialize::deserialize(&mut de).unwrap()
}
