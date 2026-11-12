//! `getLedgers` and the other read methods against responses captured once from a live
//! stellar-rpc (`tests/fixtures/stellar-rpc/getledgers.json`, provenance beside it).
//!
//! The captured server and the replay server hold different ranges, so requests are
//! templates relative to the oldest and latest ledger, and responses are compared by
//! structure: error codes and messages with numbers masked, key sets and value types,
//! xdr-json outer structure, cursor rules and how many ledgers a request returns.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use shapekit_serve::{DEFAULT_CACHE_BYTES, ServeConfig, Server, start};

fn chain() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../shapekit-pack/tests/fixtures/checkpoints.p30/testnet")
}

fn capture() -> Value {
    let p =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stellar-rpc/getledgers.json");
    serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()
}

fn serve(dir: PathBuf, head: Option<u32>) -> Server {
    start(ServeConfig {
        chain_dir: dir,
        listen: "127.0.0.1:0".parse().unwrap(),
        head,
        cache_bytes: DEFAULT_CACHE_BYTES,
        log_requests: false,
    })
    .unwrap()
}

fn call(url: &str, method: &str, params: Option<Value>) -> Value {
    let mut body = json!({"jsonrpc": "2.0", "id": 1, "method": method});
    if let Some(p) = params {
        body["params"] = p;
    }
    let text = reqwest::blocking::Client::new()
        .post(url)
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .unwrap()
        .text()
        .unwrap();
    serde_json::from_str(&text).unwrap()
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
    let o = v.as_object().unwrap();
    Value::Object(
        o.iter()
            .map(|(k, v)| (k.clone(), Value::String(jtype(v).into())))
            .collect(),
    )
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

/// Placeholders resolved for the replay server. `$latest` is replayed as one below the head:
/// the captured server's tip moved on by at least one ledger between the getHealth call
/// that fixed `$latest` and each request (every captured `latestLedger` is above it), so
/// "at latest" requests saw a ledger after their start. Any positive offset from `$oldest`
/// stands for "inside the range with room after it", which here is the second ledger.
fn resolve(v: &Value, first: u32, head: u32) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, v)| (k.clone(), resolve(v, first, head)))
                .collect(),
        ),
        Value::String(s) if s.starts_with('$') || s.starts_with('#') => {
            let body = &s[1..];
            let (base, off): (&str, i64) = if let Some((b, o)) = body.split_once('+') {
                (b, o.parse().unwrap())
            } else if let Some((b, o)) = body.split_once('-') {
                (b, -o.parse::<i64>().unwrap())
            } else {
                (body, 0)
            };
            let n = match base {
                "oldest" if off > 0 => i64::from(first) + 1,
                "oldest" => i64::from(first) + off,
                "latest" => i64::from(head) - 1 + off,
                other => panic!("unknown placeholder {other}"),
            };
            if s.starts_with('#') {
                Value::String(n.to_string())
            } else {
                json!(n)
            }
        }
        other => other.clone(),
    }
}

/// First ledger a request asks for and how many it may return.
fn start_and_limit(params: &Value) -> (u64, u64) {
    let p = params.get("pagination");
    let limit = p
        .and_then(|p| p.get("limit"))
        .and_then(Value::as_u64)
        .filter(|l| *l > 0)
        .unwrap_or(50);
    let start = match p.and_then(|p| p.get("cursor")).and_then(Value::as_str) {
        Some(c) => c.parse::<u64>().unwrap() + 1,
        None => params["startLedger"].as_u64().unwrap(),
    };
    (start, limit)
}

fn check_result(name: &str, params: &Value, want: &Value, got: &Value) {
    let res = got
        .get("result")
        .unwrap_or_else(|| panic!("{name}: expected a result, got {got}"));
    assert_eq!(key_types(res), want["top_keys"], "{name}: top-level keys");
    let ledgers = res["ledgers"].as_array().unwrap();
    for l in ledgers {
        assert_eq!(key_types(l), want["ledger_keys"], "{name}: ledger keys");
    }
    if let Some(shapes) = want["json_shapes"].as_object().filter(|s| !s.is_empty()) {
        let l = &ledgers[0];
        let keys = |v: &Value| {
            let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
            k.sort();
            json!(k)
        };
        assert_eq!(keys(&l["headerJson"]), shapes["headerJson"], "{name}");
        let (arm, body) = l["metadataJson"]
            .as_object()
            .unwrap()
            .iter()
            .next()
            .unwrap();
        assert_eq!(
            json!({arm.clone(): keys(body)}),
            shapes["metadataJson"],
            "{name}"
        );
    }

    // the captured server followed the count rule against its own latest ledger, so the
    // rule is derived from the capture rather than assumed
    let (cap_start, limit) = start_and_limit(&want["params"]);
    let cap_latest = want["summary"]["latestLedger"].as_u64().unwrap();
    let cap_count = want["summary"]["ledger_count"].as_u64().unwrap();
    assert_eq!(
        cap_count,
        limit.min(cap_latest - cap_start + 1),
        "{name}: the capture does not follow the count rule"
    );
    assert_eq!(
        want["summary"]["first_sequence"],
        json!(cap_start),
        "{name}"
    );

    let (start, limit) = start_and_limit(params);
    let latest = res["latestLedger"].as_u64().unwrap();
    assert_eq!(
        ledgers.len() as u64,
        limit.min(latest - start + 1),
        "{name}: ledger count"
    );
    let seqs: Vec<u64> = ledgers
        .iter()
        .map(|l| l["sequence"].as_u64().unwrap())
        .collect();
    assert_eq!(seqs, (start..start + seqs.len() as u64).collect::<Vec<_>>());
    assert_eq!(
        res["cursor"],
        json!(seqs.last().unwrap().to_string()),
        "{name}: the cursor is the last sequence"
    );
    assert_eq!(want["summary"]["contiguous"], json!(true));
    assert_eq!(
        want["summary"]["cursor"],
        json!(
            want["summary"]["last_sequence"]
                .as_u64()
                .unwrap()
                .to_string()
        )
    );
}

fn check_error(name: &str, want: &Value, got: &Value) {
    let e = got
        .get("error")
        .unwrap_or_else(|| panic!("{name}: expected an error, got {got}"));
    assert_eq!(e["code"], want["code"], "{name}: code ({e})");
    assert_eq!(
        mask(e["message"].as_str().unwrap()),
        want["message_template"].as_str().unwrap(),
        "{name}: message"
    );
}

#[test]
fn get_ledgers_matches_captured_stellar_rpc() {
    let fx = capture();
    let srv = serve(chain(), None);
    let url = srv.url();
    let health = call(&url, "getHealth", None);
    let first = health["result"]["oldestLedger"].as_u64().unwrap() as u32;
    let head = health["result"]["latestLedger"].as_u64().unwrap() as u32;
    let mut page_cursor = None;
    let mut compared = 0;
    for case in fx["getLedgers"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let summary = &case["summary"];
        let params = if name == "cursor-page-next" {
            json!({"pagination": {"cursor": page_cursor.clone().unwrap(), "limit": 3}})
        } else {
            resolve(&case["template"], first, head)
        };
        let got = call(&url, "getLedgers", Some(params.clone()));
        match name {
            // the captured testnet server backfills from a data lake, which widens its
            // range to ledger 2 and, misconfigured, fails these with -32603 from the
            // datastore; a server without one answers per ValidatePagination and
            // parseCursor in stellar-rpc v30.0.1, which is what is checked here
            "start-before-oldest" | "cursor-before-oldest" => {
                assert_eq!(summary["code"], json!(-32603));
                assert!(summary["message"].as_str().unwrap().contains("datastore"));
                let (code, template) = if name == "start-before-oldest" {
                    (
                        -32600,
                        "start ledger (<n>) must be between the oldest ledger: <n> and the latest ledger: <n> for this rpc instance",
                    )
                } else {
                    (
                        -32602,
                        "cursor ('<n>') must be at or above the oldest ledger: <n> for this rpc instance",
                    )
                };
                check_error(
                    name,
                    &json!({"code": code, "message_template": template}),
                    &got,
                );
            }
            _ if summary["kind"] == "error" => check_error(name, summary, &got),
            _ => {
                let mut want = case.clone();
                want["top_keys"] = summary["top_keys"].clone();
                want["ledger_keys"] = summary["ledger_keys"].clone();
                want["json_shapes"] = summary["json_shapes"].clone();
                check_result(name, &params, &want, &got);
            }
        }
        if name == "cursor-page" {
            page_cursor = got["result"]["cursor"].as_str().map(str::to_string);
        }
        compared += 1;
    }
    assert_eq!(compared, 21);
    srv.stop().unwrap();
}

#[test]
fn other_methods_match_captured_shapes() {
    let fx = capture();
    let srv = serve(chain(), None);
    let url = srv.url();
    for m in fx["other_methods"].as_array().unwrap() {
        let name = m["name"].as_str().unwrap();
        let got = call(&url, m["method"].as_str().unwrap(), None);
        if name == "unknown-method" {
            assert_eq!(got["error"], m["response"]["error"], "{got}");
            continue;
        }
        let mut want = m["top_keys"].as_object().unwrap().clone();
        if name == "getNetwork" {
            // stellar-rpc omits friendbotUrl when none is configured, and a replay has none
            want.remove("friendbotUrl");
        }
        assert_eq!(
            key_types(&got["result"]),
            Value::Object(want),
            "{name}: {got}"
        );
    }
    let v = call(&url, "getVersionInfo", None);
    assert!(
        v["result"]["version"]
            .as_str()
            .unwrap()
            .starts_with("shapekit-replay ")
    );
    srv.stop().unwrap();
}

/// A poller whose cursor reached the head gets an empty page with its cursor back, not an
/// error (stellar-rpc v30.0.1 `getLedgers`); a startLedger past the head stays an error.
#[test]
fn caught_up_cursor_gets_an_empty_page() {
    let srv = serve(chain(), Some(100));
    let url = srv.url();
    for cursor in ["100", "150", "4294967294"] {
        let r = call(
            &url,
            "getLedgers",
            Some(json!({"pagination": {"cursor": cursor}})),
        );
        let res = &r["result"];
        assert_eq!(res["ledgers"], json!([]), "{r}");
        assert_eq!(res["cursor"], cursor);
        assert_eq!(res["latestLedger"], 100);
    }
    let wrapped = call(
        &url,
        "getLedgers",
        Some(json!({"pagination": {"cursor": "4294967295"}})),
    );
    assert_eq!(wrapped["error"]["code"], -32602);
    let past = call(&url, "getLedgers", Some(json!({"startLedger": 101})));
    assert_eq!(past["error"]["code"], -32600);
    srv.stop().unwrap();
}
