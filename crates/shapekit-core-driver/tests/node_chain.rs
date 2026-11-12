//! Chain recording against the pinned stellar-core image. Needs docker and
//! `SHAPEKIT_DOCKER_TESTS=1`.

mod node_support;

use std::path::{Path, PathBuf};

use node_support::{MAINNET, TESTNET, config, docker_object_exists, enabled};
use shapekit_core_driver::canonical::permutation_equal;
use shapekit_core_driver::config::close_time;
use shapekit_core_driver::meta::{FramedLcm, read_all};
use shapekit_core_driver::node::CoreNode;
use shapekit_core_driver::recorder::{
    CANONICAL_STREAM_FILE, ChainRecorder, ChainSummary, FIRST_LEDGER, STREAM_FILE,
};
use stellar_xdr::LedgerCloseMeta;

/// Ledger 2 carries the protocol upgrade, then ten empty ledgers.
const LAST: u32 = FIRST_LEDGER + 10;

fn record(
    passphrase: &str,
    protocol: u32,
    last: u32,
    dir: &Path,
) -> (ChainSummary, Vec<FramedLcm>, PathBuf) {
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
    (summary, frames, target)
}

/// Release builds of stellar-core order the ledger entry changes of one
/// LedgerTxn by iterating an unordered map keyed through a siphash whose key
/// comes from libsodium randombytes at process start
/// (`LedgerTxn::Impl::getChanges`, `std::hash<LedgerKey>` in
/// LedgerHashUtils.h, `shortHash::initialize`, v30.0.0), so raw meta may
/// differ between runs in that order. The canonical stream must not differ
/// at all, and every raw difference must be a permutation of the same
/// changes. Returns the ledgers whose raw bytes differ.
fn assert_reproducible(name: &str, a: &Path, b: &Path) -> Vec<u32> {
    let ca = std::fs::read(a.join(CANONICAL_STREAM_FILE)).unwrap();
    let cb = std::fs::read(b.join(CANONICAL_STREAM_FILE)).unwrap();
    assert_eq!(ca, cb, "{name}: canonical streams differ");
    let ra = read_all(&a.join(STREAM_FILE)).unwrap();
    let rb = read_all(&b.join(STREAM_FILE)).unwrap();
    assert_eq!(ra.len(), rb.len());
    let mut differ = Vec::new();
    for (x, y) in ra.iter().zip(&rb) {
        assert_eq!(x.ledger_seq, y.ledger_seq);
        if x.xdr != y.xdr {
            assert!(
                permutation_equal(&x.meta, &y.meta).unwrap(),
                "{name}: ledger {} differs beyond change order",
                x.ledger_seq
            );
            differ.push(x.ledger_seq);
        }
    }
    eprintln!("{name}: canonical identical, raw differs only in change order at {differ:?}");
    differ
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
        let differ = assert_reproducible(name, &a.2, &b.2);
        // only the upgrade ledger has a change list of more than one entry
        assert!(differ.iter().all(|s| *s == FIRST_LEDGER), "{differ:?}");
        // chain.json may only differ in the hashes of core's raw order
        let strip = |c: &ChainSummary| {
            let mut c = c.clone();
            c.stream_sha256.clear();
            for l in &mut c.ledgers {
                l.raw_sha256.clear();
            }
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
    let (summary, frames, _) = record(TESTNET, 30, 63, tmp.path());
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
        let (summary, frames, _) = record(TESTNET, protocol, FIRST_LEDGER + 3, tmp.path());
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
