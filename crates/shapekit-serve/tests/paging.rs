//! getTransactions paged with cursors, the way an indexer polls stellar-rpc to the tip: on
//! every committed chain, at several page sizes and while the head moves, every recorded
//! transaction arrives exactly once and in order, the cursor never moves backwards, and a
//! caught-up poller gets an empty page holding its own cursor (stellar-rpc v30.0.1,
//! cmd/stellar-rpc/internal/methods/get_transactions.go).

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use shapekit_oracle::view::LedgerView;
use shapekit_oracle::{Network, decode_lcm};
use shapekit_pack::chain::{ChainFile, open_lake};
use shapekit_serve::replay::Replay;
use shapekit_serve::{DEFAULT_CACHE_BYTES, rpc};

/// (ledger, application order, hash) of one transaction.
type Position = (u32, u32, String);

fn chains() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/chains");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap()
        .flat_map(|c| std::fs::read_dir(c.unwrap().path()).unwrap())
        .map(|d| d.unwrap().path())
        .collect();
    dirs.sort();
    assert!(!dirs.is_empty());
    dirs
}

/// Every transaction of the chain in apply order, read from the lake without the server.
fn recorded(dir: &Path) -> Vec<Position> {
    let c = ChainFile::read(dir).unwrap();
    let net = Network::new(&c.passphrase);
    let mut out = Vec::new();
    for l in open_lake(dir, &c).unwrap().ledgers() {
        let l = l.unwrap();
        let lcm = decode_lcm(&l.xdr).unwrap();
        for tx in &LedgerView::new(&lcm, &net).unwrap().txs {
            out.push((
                l.seq,
                tx.index + 1,
                hex::encode(tx.result.transaction_hash.0),
            ));
        }
    }
    out
}

fn toid(ledger: u32, tx: u32, op: u32) -> i64 {
    (i64::from(ledger) << 32) | (i64::from(tx) << 12) | i64::from(op)
}

fn get_transactions(replay: &Replay, params: Value) -> Value {
    let req = json!({"jsonrpc": "2.0", "id": 1, "method": "getTransactions", "params": params});
    let mut out = Vec::new();
    assert!(rpc::handle(replay, &req, &mut out));
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert!(v.get("error").is_none(), "{params}: {v}");
    v["result"].clone()
}

/// Pages from `cursor` (or from the first ledger) until an empty page hands back the cursor
/// it was asked with, appending what arrives to `got`. Returns that cursor.
fn walk(
    replay: &Replay,
    limit: u64,
    mut cursor: Option<String>,
    got: &mut Vec<Position>,
) -> String {
    for _ in 0..100_000 {
        let params = match &cursor {
            None => json!({"startLedger": replay.first, "pagination": {"limit": limit}}),
            Some(c) => json!({"pagination": {"cursor": c, "limit": limit}}),
        };
        let res = get_transactions(replay, params);
        let txs = res["transactions"].as_array().unwrap();
        let next = res["cursor"].as_str().unwrap().to_string();
        let next_id: i64 = next.parse().unwrap();
        if let Some(c) = &cursor {
            let id: i64 = c.parse().unwrap();
            assert!(next_id >= id, "cursor moved back from {c} to {next}");
        }
        assert!(txs.len() as u64 <= limit);
        if txs.is_empty() {
            // an empty page still moves the cursor past ledgers without transactions; once
            // it stops moving the poller has caught up
            if cursor.as_deref() == Some(next.as_str()) {
                return next;
            }
            cursor = Some(next);
            continue;
        }
        for t in txs {
            got.push((
                t["ledger"].as_u64().unwrap() as u32,
                t["applicationOrder"].as_u64().unwrap() as u32,
                t["txHash"].as_str().unwrap().to_string(),
            ));
        }
        let last = got.last().unwrap();
        let at_last = toid(last.0, last.1, 1);
        if txs.len() as u64 == limit {
            assert_eq!(
                next_id, at_last,
                "a full page's cursor is its last transaction"
            );
        } else {
            // a short page went on to the head, past ledgers it took nothing from
            assert!(next_id >= at_last, "{next_id} is before {at_last}");
            assert_eq!(next_id >> 32, i64::from(replay.head()));
        }
        cursor = Some(next);
    }
    panic!("paging did not reach the tip");
}

#[test]
fn every_chain_pages_to_the_tip_without_duplicates_or_gaps() {
    let mut chains_with_txs = 0;
    for dir in chains() {
        let want = recorded(&dir);
        let replay = Replay::open(&dir, None, DEFAULT_CACHE_BYTES).unwrap();
        let (first, last) = (replay.first, replay.last);
        if !want.is_empty() {
            chains_with_txs += 1;
        }
        for limit in [1, 2, 3, 200] {
            let mut got = Vec::new();
            replay.set_head(last).unwrap();
            let end = walk(&replay, limit, None, &mut got);
            assert_eq!(got, want, "{} limit {limit}", dir.display());
            // polling again from the tip delivers nothing, and keeps the cursor
            let again = get_transactions(
                &replay,
                json!({"pagination": {"cursor": end, "limit": limit}}),
            );
            assert_eq!(again["transactions"], json!([]));
            assert_eq!(again["cursor"].as_str().unwrap(), end);

            // the same, with the head released in steps while the poller catches up each time
            let mut got = Vec::new();
            let mut cursor = None;
            let step = ((last - first) / 3).max(1);
            let mut head = first;
            loop {
                replay.set_head(head).unwrap();
                cursor = Some(walk(&replay, limit, cursor, &mut got));
                if head == last {
                    break;
                }
                head = (head + step).min(last);
            }
            assert_eq!(got, want, "{} limit {limit}, head moving", dir.display());
        }
    }
    assert!(
        chains_with_txs > 10,
        "only {chains_with_txs} chains hold transactions"
    );
}

/// A cursor at or past the head is a caught-up poller: an empty page with its cursor back,
/// not "0", which the next call would refuse.
#[test]
fn cursor_past_the_head_is_echoed() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/chains/soroban.auth-trees.p30/testnet");
    let replay = Replay::open(&dir, Some(64), DEFAULT_CACHE_BYTES).unwrap();
    // the audit's reproduction: one transaction in ledger 64, then the page after it
    let res = get_transactions(
        &replay,
        json!({"startLedger": 64, "pagination": {"limit": 200}}),
    );
    let txs = res["transactions"].as_array().unwrap();
    assert_eq!(txs.len(), 1);
    let c = res["cursor"].as_str().unwrap().to_string();
    assert_eq!(c, toid(64, 1, 1).to_string());
    for _ in 0..2 {
        let next = get_transactions(&replay, json!({"pagination": {"cursor": c}}));
        assert_eq!(next["transactions"], json!([]));
        assert_eq!(next["cursor"].as_str().unwrap(), c);
    }
    for cursor in [toid(65, 0, 0), toid(1000, 7, 1), toid(64, 0xF_FFFF, 0xFFF)] {
        let next = get_transactions(
            &replay,
            json!({"pagination": {"cursor": cursor.to_string()}}),
        );
        assert_eq!(next["transactions"], json!([]), "{cursor}");
        assert_eq!(next["cursor"].as_str().unwrap(), cursor.to_string());
        assert_eq!(next["latestLedger"], 64);
    }
    // once the head moves past it, the same cursor picks up the next ledgers' transactions
    replay.set_head(replay.last).unwrap();
    let mut got = Vec::new();
    walk(&replay, 5, Some(c), &mut got);
    let want: Vec<Position> = recorded(&dir).into_iter().filter(|p| p.0 > 64).collect();
    assert_eq!(got, want);
}

/// A cursor below the oldest ledger, or with a negative ledger, is refused as stellar-rpc
/// refuses it.
#[test]
fn cursors_outside_the_chain_are_refused() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/chains/soroban.auth-trees.p30/testnet");
    let replay = Replay::open(&dir, None, DEFAULT_CACHE_BYTES).unwrap();
    let err = |cursor: i64| {
        let req = json!({"jsonrpc": "2.0", "id": 1, "method": "getTransactions",
            "params": {"pagination": {"cursor": cursor.to_string()}}});
        let mut out = Vec::new();
        assert!(rpc::handle(&replay, &req, &mut out));
        let v: Value = serde_json::from_slice(&out).unwrap();
        (
            v["error"]["code"].as_i64().unwrap(),
            v["error"]["message"].as_str().unwrap().to_string(),
        )
    };
    assert_eq!(
        err(toid(1, 0, 0)),
        (
            -32602,
            "database does not contain metadata for ledger: 1".into()
        )
    );
    assert_eq!(
        err(-(1 << 32)),
        (-32602, "cursor ledger sequence cannot be negative".into())
    );
}
