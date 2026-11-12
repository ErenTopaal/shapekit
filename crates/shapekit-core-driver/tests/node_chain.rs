//! Chain recording against the pinned stellar-core image. Needs docker and
//! `SHAPEKIT_DOCKER_TESTS=1`.

mod node_support;

use std::path::Path;

use node_support::{MAINNET, TESTNET, config, docker_object_exists, enabled};
use shapekit_core_driver::config::close_time;
use shapekit_core_driver::meta::{FramedLcm, read_all};
use shapekit_core_driver::node::CoreNode;
use shapekit_core_driver::recorder::{ChainRecorder, ChainSummary, FIRST_LEDGER, STREAM_FILE};
use stellar_xdr::{LedgerCloseMeta, Limits, WriteXdr};

/// Ledger 2 carries the protocol upgrade, then ten empty ledgers.
const LAST: u32 = FIRST_LEDGER + 10;

fn record(
    passphrase: &str,
    protocol: u32,
    last: u32,
    dir: &Path,
) -> (ChainSummary, Vec<FramedLcm>) {
    let work = dir.join("work");
    std::fs::create_dir(&work).unwrap();
    let node = CoreNode::start(config(passphrase, protocol), &work).expect("core starts");
    let container = node.container().to_owned();
    let network = node.network().to_owned();
    let mut rec = ChainRecorder::new(&node);
    node.upgrade_to(protocol, FIRST_LEDGER, close_time(FIRST_LEDGER))
        .expect("upgrade");
    for seq in FIRST_LEDGER + 1..=last {
        node.close(seq, close_time(seq)).expect("close");
    }
    rec.collect_through(last).expect("meta");
    let target = dir.join("chain");
    let summary = rec.finish(&node, &target).expect("finish");
    node.stop().expect("stop");
    assert!(!docker_object_exists("container", &container));
    assert!(!docker_object_exists("network", &network));
    let frames = read_all(&target.join(STREAM_FILE)).expect("recorded stream");
    (summary, frames)
}

/// First differing json path between two values, for reporting.
fn first_difference(a: &serde_json::Value, b: &serde_json::Value, path: &str) -> Option<String> {
    use serde_json::Value;
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, va) in x {
                match y.get(k) {
                    Some(vb) => {
                        if let Some(d) = first_difference(va, vb, &format!("{path}.{k}")) {
                            return Some(d);
                        }
                    }
                    None => return Some(format!("{path}.{k} missing on the right")),
                }
            }
            y.keys()
                .find(|k| !x.contains_key(*k))
                .map(|k| format!("{path}.{k} missing on the left"))
        }
        (Value::Array(x), Value::Array(y)) => {
            for (i, (va, vb)) in x.iter().zip(y).enumerate() {
                if let Some(d) = first_difference(va, vb, &format!("{path}[{i}]")) {
                    return Some(d);
                }
            }
            (x.len() != y.len()).then(|| format!("{path} lengths {} vs {}", x.len(), y.len()))
        }
        _ => (a != b).then(|| format!("{path}: {a} vs {b}")),
    }
}

fn describe_difference(a: &LedgerCloseMeta, b: &LedgerCloseMeta) -> String {
    let ja = serde_json::to_value(a).unwrap();
    let jb = serde_json::to_value(b).unwrap();
    first_difference(&ja, &jb, "lcm").unwrap_or_else(|| "values equal, encodings differ".into())
}

/// A copy of the meta with each upgrade's ledger entry changes sorted by
/// their xdr. Used only to describe a difference, never to compare chains.
fn with_sorted_upgrade_changes(m: &LedgerCloseMeta) -> LedgerCloseMeta {
    let mut m = m.clone();
    let upgrades = match &mut m {
        LedgerCloseMeta::V0(x) => &mut x.upgrades_processing,
        LedgerCloseMeta::V1(x) => &mut x.upgrades_processing,
        LedgerCloseMeta::V2(x) => &mut x.upgrades_processing,
    };
    let mut list = upgrades.to_vec();
    for u in &mut list {
        let mut changes = u.changes.to_vec();
        changes.sort_by_key(|c| c.to_xdr(Limits::none()).unwrap());
        u.changes = changes.try_into().unwrap();
    }
    *upgrades = list.try_into().unwrap();
    m
}

/// Every ledger whose bytes differ, with the first differing field and
/// whether the difference is only the order of upgrade changes.
fn differences(a: &[FramedLcm], b: &[FramedLcm]) -> Vec<(u32, String, bool)> {
    assert_eq!(a.len(), b.len(), "ledger counts differ");
    let mut out = Vec::new();
    for (x, y) in a.iter().zip(b) {
        assert_eq!(x.ledger_seq, y.ledger_seq);
        if x.xdr != y.xdr {
            let order_only =
                with_sorted_upgrade_changes(&x.meta) == with_sorted_upgrade_changes(&y.meta);
            out.push((
                x.ledger_seq,
                describe_difference(&x.meta, &y.meta),
                order_only,
            ));
        }
    }
    out
}

/// Release builds of stellar-core order the ledger entry changes of one
/// LedgerTxn by iterating an unordered map keyed through a siphash whose key
/// comes from libsodium randombytes at process start
/// (`LedgerTxn::Impl::getChanges`, `std::hash<LedgerKey>` in
/// LedgerHashUtils.h, `shortHash::initialize`, v30.0.0). Nothing in a
/// release build can seed it, so the order of the changes in an upgrade
/// ledger differs from run to run. This asserts exactly that: every other
/// byte is identical, and a differing ledger differs only in that order.
/// It does not make the recorded chains equal; they are stored as core
/// wrote them.
fn assert_identical_but_change_order(name: &str, a: &[FramedLcm], b: &[FramedLcm]) {
    let diffs = differences(a, b);
    for (seq, field, order_only) in &diffs {
        eprintln!(
            "{name}: ledger {seq} bytes differ between runs, first at {field}, order only: {order_only}"
        );
    }
    for (seq, field, order_only) in &diffs {
        assert!(
            *order_only && *seq == FIRST_LEDGER,
            "{name}: ledger {seq} differs beyond upgrade change order, first at {field}"
        );
    }
}

fn header(f: &FramedLcm) -> &stellar_xdr::LedgerHeader {
    match &f.meta {
        LedgerCloseMeta::V0(m) => &m.ledger_header.header,
        LedgerCloseMeta::V1(m) => &m.ledger_header.header,
        LedgerCloseMeta::V2(m) => &m.ledger_header.header,
    }
}

#[test]
fn node_chain_is_byte_reproducible_per_passphrase() {
    if !enabled("node_chain_is_byte_reproducible_per_passphrase") {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let mut runs = Vec::new();
    for (i, p) in [TESTNET, TESTNET, MAINNET, MAINNET].iter().enumerate() {
        let dir = tmp.path().join(format!("run{i}"));
        std::fs::create_dir(&dir).unwrap();
        runs.push(record(p, 30, LAST, &dir));
    }

    for (name, a, b) in [
        ("testnet", &runs[0], &runs[1]),
        ("mainnet", &runs[2], &runs[3]),
    ] {
        assert_identical_but_change_order(name, &a.1, &b.1);
        // chain.json may only differ in the hashes that cover ledger 2
        let strip = |c: &ChainSummary| {
            let mut c = c.clone();
            c.ledgers.retain(|l| l.ledger_seq != FIRST_LEDGER);
            c.stream_sha256.clear();
            c
        };
        assert_eq!(strip(&a.0), strip(&b.0), "{name}: chain.json differs");
    }

    let (tn, mn) = (&runs[0], &runs[2]);
    assert_eq!(tn.0.ledger_range, mn.0.ledger_range);
    assert_eq!(tn.0.ledger_range.first, FIRST_LEDGER);
    assert_eq!(tn.0.ledger_range.last, LAST);
    assert_eq!(tn.1.len(), (LAST - FIRST_LEDGER + 1) as usize);
    for (t, m) in tn.1.iter().zip(&mn.1) {
        assert_eq!(t.ledger_seq, m.ledger_seq);
        assert_ne!(
            t.xdr, m.xdr,
            "ledger {} equal across networks",
            t.ledger_seq
        );
        let (ht, hm) = (header(t), header(m));
        assert_eq!(ht.ledger_version, 30);
        assert_eq!(ht.scp_value.close_time.0, close_time(t.ledger_seq));
        assert_eq!(ht.scp_value.close_time, hm.scp_value.close_time);
        assert_ne!(ht.previous_ledger_hash, hm.previous_ledger_hash);
    }
    // the upgrade ledger is closed under genesis protocol 0 rules, so its
    // meta is V0; it carries the upgrade and the rest are empty V2 ledgers
    for f in &tn.1 {
        match &f.meta {
            LedgerCloseMeta::V0(m) if f.ledger_seq == FIRST_LEDGER => {
                assert!(m.tx_processing.is_empty());
                assert_eq!(m.upgrades_processing.len(), 1);
            }
            LedgerCloseMeta::V2(m) if f.ledger_seq != FIRST_LEDGER => {
                assert!(m.tx_processing.is_empty());
                assert!(m.upgrades_processing.is_empty());
            }
            _ => panic!("ledger {} has unexpected meta version", f.ledger_seq),
        }
    }
    assert!(tn.0.checkpoints.is_empty());
}

#[test]
fn node_chain_publishes_first_checkpoint() {
    if !enabled("node_chain_publishes_first_checkpoint") {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let (summary, frames) = record(TESTNET, 30, 63, tmp.path());
    assert_eq!(summary.checkpoints, vec![63]);
    assert_eq!(frames.len(), 62);
    for sub in ["ledger", "transactions", "results", "scp"] {
        let f = tmp.path().join(format!(
            "chain/archive/{sub}/00/00/00/{sub}-0000003f.xdr.gz"
        ));
        assert!(f.is_file(), "{} missing", f.display());
    }
}

#[test]
fn node_runs_older_protocols() {
    if !enabled("node_runs_older_protocols") {
        return;
    }
    for protocol in [22, 24, 26, 28] {
        let tmp = tempfile::tempdir().unwrap();
        let (summary, frames) = record(TESTNET, protocol, FIRST_LEDGER + 3, tmp.path());
        assert_eq!(summary.protocol, protocol);
        for f in &frames {
            assert_eq!(
                header(f).ledger_version,
                protocol,
                "ledger {}",
                f.ledger_seq
            );
        }
    }
}
