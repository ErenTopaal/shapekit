//! Transaction, ledger entry and head control behaviour on a corpus chain with Soroban and
//! classic transactions. Answers are checked against the chain itself, read independently
//! through the lake and the oracle.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use shapekit_oracle::view::LedgerView;
use shapekit_oracle::{LedgerState, Network, decode_lcm};
use shapekit_pack::chain::{ChainFile, open_lake};
use shapekit_serve::{DEFAULT_CACHE_BYTES, ServeConfig, Server, start};
use stellar_xdr::{
    LedgerEntryData, LedgerKey, Limits, ReadXdr, TransactionEnvelope, TransactionMeta, WriteXdr,
};

fn chain() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/chains/soroban.invoke-basics.p30/testnet")
}

fn serve(head: Option<u32>, cache_bytes: usize) -> Server {
    start(ServeConfig {
        chain_dir: chain(),
        listen: "127.0.0.1:0".parse().unwrap(),
        head,
        cache_bytes,
        log_requests: false,
    })
    .unwrap()
}

fn post(url: &str, body: &Value) -> Value {
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

fn call(url: &str, method: &str, params: Value) -> Value {
    post(
        url,
        &json!({"jsonrpc": "2.0", "id": 7, "method": method, "params": params}),
    )
}

fn limits() -> Limits {
    Limits {
        depth: 1000,
        len: 64 * 1024 * 1024,
    }
}

/// Every transaction of the chain, in order: (ledger, hash, envelope, meta).
fn recorded_txs() -> Vec<(u32, String, TransactionEnvelope, TransactionMeta)> {
    let dir = chain();
    let c = ChainFile::read(&dir).unwrap();
    let net = Network::new(&c.passphrase);
    let mut out = Vec::new();
    for l in open_lake(&dir, &c).unwrap().ledgers() {
        let l = l.unwrap();
        let lcm = decode_lcm(&l.xdr).unwrap();
        let view = LedgerView::new(&lcm, &net).unwrap();
        for tx in &view.txs {
            out.push((
                l.seq,
                hex::encode(tx.result.transaction_hash.0),
                tx.envelope.clone(),
                tx.meta.clone(),
            ));
        }
    }
    out
}

#[test]
fn transactions_come_from_recorded_meta() {
    let srv = serve(None, DEFAULT_CACHE_BYTES);
    let url = srv.url();
    let txs = recorded_txs();
    assert!(txs.len() > 5);

    // every transaction, paged three at a time through the cursor
    let mut got = Vec::new();
    let mut params = json!({"startLedger": 2, "pagination": {"limit": 3}});
    loop {
        let r = call(&url, "getTransactions", params.clone());
        let res = &r["result"];
        let page = res["transactions"].as_array().unwrap();
        got.extend(page.iter().cloned());
        if page.len() < 3 {
            break;
        }
        params = json!({"pagination": {"cursor": res["cursor"], "limit": 3}});
    }
    assert_eq!(got.len(), txs.len());
    for (g, (seq, hash, env, meta)) in got.iter().zip(&txs) {
        assert_eq!(g["txHash"], json!(hash));
        assert_eq!(g["ledger"], json!(seq));
        assert!(g["createdAt"].is_number());
        let e = TransactionEnvelope::from_xdr_base64(g["envelopeXdr"].as_str().unwrap(), limits())
            .unwrap();
        assert_eq!(&e, env);
        let m = TransactionMeta::from_xdr_base64(g["resultMetaXdr"].as_str().unwrap(), limits())
            .unwrap();
        assert_eq!(&m, meta);
        // meta v4 reports one contract event list per operation, and like go's omitempty
        // drops the field when there are no operations (a transaction failed before any ran)
        if let TransactionMeta::V4(v4) = meta {
            let ops = v4.operations.len();
            assert_eq!(
                g["events"]["contractEventsXdr"].as_array().map(Vec::len),
                (ops > 0).then_some(ops),
                "{hash}"
            );
        }

        let one = call(&url, "getTransaction", json!({"hash": hash}));
        let r = &one["result"];
        assert!(r["status"] == "SUCCESS" || r["status"] == "FAILED");
        assert_eq!(r["envelopeXdr"], g["envelopeXdr"]);
        assert_eq!(r["resultXdr"], g["resultXdr"]);
        assert_eq!(r["applicationOrder"], g["applicationOrder"]);
        assert!(r["createdAt"].is_string());

        let as_json = call(
            &url,
            "getTransaction",
            json!({"hash": hash, "xdrFormat": "json"}),
        );
        assert!(as_json["result"]["envelopeJson"].is_object());
        assert!(as_json["result"].get("envelopeXdr").is_none());
    }

    let missing = call(&url, "getTransaction", json!({"hash": "00".repeat(32)}));
    let r = &missing["result"];
    assert_eq!(r["status"], "NOT_FOUND");
    assert_eq!(r["events"], json!({}));
    assert_eq!(r["createdAt"], "0");

    let bad = call(&url, "getTransaction", json!({"hash": "abc"}));
    assert_eq!(bad["error"]["code"], -32602);
    assert_eq!(bad["error"]["message"], "unexpected hash length (3)");

    // a transaction past the head is not visible yet
    let (last_seq, last_hash, _, _) = txs.last().unwrap().clone();
    let c = post(
        &format!("{url}/control/head"),
        &json!({"ledger": last_seq - 1}),
    );
    assert_eq!(c["head"], last_seq - 1);
    let hidden = call(&url, "getTransaction", json!({"hash": last_hash}));
    assert_eq!(hidden["result"]["status"], "NOT_FOUND");
    srv.stop().unwrap();
}

#[test]
fn ledger_entries_follow_the_head() {
    let srv = serve(None, DEFAULT_CACHE_BYTES);
    let url = srv.url();
    let dir = chain();
    let c = ChainFile::read(&dir).unwrap();
    let net = Network::new(&c.passphrase);

    // the expected state, applied independently up to ledger 10 and to the end
    let mut state = LedgerState::genesis(&net);
    let mut at10 = None;
    for l in open_lake(&dir, &c).unwrap().ledgers() {
        let l = l.unwrap();
        state
            .apply_ledger(&decode_lcm(&l.xdr).unwrap(), &net)
            .unwrap();
        if l.seq == 10 {
            at10 = Some(state.clone());
        }
    }
    let at10 = at10.unwrap();
    let keys: Vec<LedgerKey> = state.iter().map(|(k, _)| k.clone()).collect();
    let queryable: Vec<&LedgerKey> = keys
        .iter()
        .filter(|k| !matches!(k, LedgerKey::Ttl(_)))
        .collect();
    let b64: Vec<String> = queryable
        .iter()
        .map(|k| k.to_xdr_base64(Limits::none()).unwrap())
        .collect();
    assert!(b64.len() <= 200, "{} keys", b64.len());

    let check = |want: &LedgerState, head: u32| {
        let r = call(&url, "getLedgerEntries", json!({"keys": b64}));
        let res = &r["result"];
        assert_eq!(res["latestLedger"], head);
        let entries = res["entries"].as_array().unwrap();
        let expected: Vec<&LedgerKey> = queryable
            .iter()
            .copied()
            .filter(|k| want.get(k).is_some())
            .collect();
        assert_eq!(entries.len(), expected.len());
        for (e, k) in entries.iter().zip(expected) {
            assert_eq!(e["key"], json!(k.to_xdr_base64(Limits::none()).unwrap()));
            let w = want.get(k).unwrap();
            let data =
                LedgerEntryData::from_xdr_base64(e["xdr"].as_str().unwrap(), limits()).unwrap();
            assert_eq!(data, w.data);
            assert_eq!(e["lastModifiedLedgerSeq"], w.last_modified_ledger_seq);
            let soroban = matches!(k, LedgerKey::ContractData(_) | LedgerKey::ContractCode(_));
            assert_eq!(e.get("liveUntilLedgerSeq").is_some(), soroban, "{e}");
        }
    };
    check(&state, c.ledger_range.last);
    // moving the head back rebuilds the state from genesis
    post(&format!("{url}/control/head"), &json!({"ledger": 10}));
    check(&at10, 10);
    post(
        &format!("{url}/control/head"),
        &json!({"ledger": c.ledger_range.last}),
    );
    check(&state, c.ledger_range.last);

    let ttl = keys
        .iter()
        .find(|k| matches!(k, LedgerKey::Ttl(_)))
        .unwrap();
    let r = call(
        &url,
        "getLedgerEntries",
        json!({"keys": [ttl.to_xdr_base64(Limits::none()).unwrap()]}),
    );
    assert_eq!(
        r["error"]["message"],
        "ledger ttl entries cannot be queried directly"
    );
    let r = call(&url, "getLedgerEntries", json!({"keys": ["!!"]}));
    assert_eq!(r["error"]["code"], -32602);
    srv.stop().unwrap();
}

#[test]
fn head_control_and_static_routes() {
    // a cache smaller than one batch still serves everything, reading from disk each time
    let srv = serve(Some(5), 1);
    let url = srv.url();
    let c = ChainFile::read(&chain()).unwrap();
    let client = reqwest::blocking::Client::new();
    let status = |path: &str| {
        client
            .get(format!("{url}{path}"))
            .send()
            .unwrap()
            .status()
            .as_u16()
    };

    let latest = call(&url, "getLatestLedger", json!(null));
    assert_eq!(latest["result"]["sequence"], 5);
    let beyond = call(&url, "getLedgers", json!({"startLedger": 6}));
    assert_eq!(beyond["error"]["code"], -32600);

    let key5 = &c.lake.batches.iter().find(|b| b.first == 5).unwrap().key;
    let key6 = &c.lake.batches.iter().find(|b| b.first == 6).unwrap().key;
    assert_eq!(status(&format!("/ledgers/{key5}")), 200);
    assert_eq!(status(&format!("/ledgers/{key6}")), 404);
    assert_eq!(status("/ledgers/.config.json"), 200);
    assert_eq!(status("/ledgers/../chain.json"), 404);
    assert_eq!(status("/archive/.well-known/stellar-history.json"), 200);

    let lake_bytes = client
        .get(format!("{url}/ledgers/{key5}"))
        .send()
        .unwrap()
        .bytes()
        .unwrap();
    assert_eq!(
        lake_bytes.as_ref(),
        std::fs::read(chain().join("ledgers").join(key5)).unwrap()
    );

    let r = post(
        &format!("{url}/control/head"),
        &json!({"ledger": c.ledger_range.last + 1}),
    );
    assert!(r["error"].as_str().unwrap().contains("outside"));
    post(&format!("{url}/control/head"), &json!({"ledger": 6}));
    assert_eq!(status(&format!("/ledgers/{key6}")), 200);

    let send = call(&url, "sendTransaction", json!({"transaction": "AAAA"}));
    assert_eq!(send["error"]["code"], -32600);
    assert!(
        send["error"]["message"]
            .as_str()
            .unwrap()
            .contains("read-only")
    );
    let sim = call(&url, "simulateTransaction", json!({"transaction": "AAAA"}));
    assert_eq!(sim["error"]["code"], -32601);
    assert_eq!(sim["error"]["data"], "simulateTransaction");

    let positional = call(&url, "getLedgers", json!([2]));
    assert_eq!(positional["error"]["code"], -32602);
    let batch = post(
        &url,
        &json!([
            {"jsonrpc": "2.0", "id": 1, "method": "getHealth"},
            {"jsonrpc": "2.0", "id": 2, "method": "getNetwork"},
        ]),
    );
    assert_eq!(batch.as_array().unwrap().len(), 2);
    srv.stop().unwrap();
}

#[test]
fn json_format_matches_the_xdr() {
    use sha2::Digest;
    let c = ChainFile::read(&chain()).unwrap();
    let srv = serve(None, DEFAULT_CACHE_BYTES);
    let url = srv.url();
    let b = call(
        &url,
        "getLedgers",
        json!({"startLedger": 2, "pagination": {"limit": 200}}),
    );
    let j = call(
        &url,
        "getLedgers",
        json!({"startLedger": 2, "pagination": {"limit": 200}, "xdrFormat": "json"}),
    );
    let bl = b["result"]["ledgers"].as_array().unwrap();
    let jl = j["result"]["ledgers"].as_array().unwrap();
    assert_eq!(bl.len(), jl.len());
    for (x, y) in bl.iter().zip(jl) {
        let lcm = stellar_xdr::LedgerCloseMeta::from_xdr_base64(
            x["metadataXdr"].as_str().unwrap(),
            limits(),
        )
        .unwrap();
        let from_json: stellar_xdr::LedgerCloseMeta =
            serde_json::from_value(y["metadataJson"].clone()).unwrap();
        assert_eq!(lcm, from_json);
        assert_eq!(y["metadataXdr"], "");
        assert_eq!(x["hash"], y["hash"]);
        // the served bytes are the lake's canonical bytes, not a re-encode of something else
        let bytes = lcm.to_xdr(Limits::none()).unwrap();
        let seq = x["sequence"].as_u64().unwrap() as u32;
        let rec = c.ledgers.iter().find(|r| r.ledger_seq == seq).unwrap();
        assert_eq!(hex::encode(sha2::Sha256::digest(&bytes)), rec.sha256);
    }
    assert_eq!(bl.len(), c.ledgers.len());
    srv.stop().unwrap();
}
