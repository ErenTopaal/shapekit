//! The smoke chain against the pinned stellar-core image. Needs docker, so
//! the tests are ignored by default; run with `--include-ignored`.

mod node_support;

use std::collections::BTreeSet;
use std::path::Path;

use node_support::{MAINNET, TESTNET, pins_path};
use shapekit_core_driver::canonical::permutation_equal;
use shapekit_core_driver::chain::upgrades;
use shapekit_core_driver::config::{CoreImage, close_time};
use shapekit_core_driver::meta::{FramedLcm, read_all};
use shapekit_core_driver::recorder::{
    CANONICAL_STREAM_FILE, ChainSummary, FIRST_LEDGER, NORMALIZATION, STREAM_FILE,
};
use shapekit_core_driver::settings::NetworkSettings;
use shapekit_core_driver::smoke::{self, SmokeChain};
use shapekit_manifest::signature::shape_signature;
use shapekit_manifest::xdr::roundtrip_check;
use stellar_xdr::{
    ConfigSettingEntry, ConfigSettingId, ContractEventBody, LedgerCloseMeta, LedgerUpgrade, Limits,
    ScVal, TransactionEnvelope, TransactionMeta,
};

const LAST: u32 = 12;

struct Run {
    chain: SmokeChain,
    raw: Vec<FramedLcm>,
    canonical: Vec<FramedLcm>,
    canonical_bytes: Vec<u8>,
}

fn run(passphrase: &str, diagnostic_events: bool, dir: &Path) -> Run {
    let work = dir.join("work");
    std::fs::create_dir(&work).unwrap();
    let target = dir.join("chain");
    let image = CoreImage::from_pins(&pins_path()).unwrap();
    let mut cfg = smoke::node_config(passphrase, image);
    cfg.diagnostic_events = diagnostic_events;
    let chain = smoke::record(cfg, &work, &target).expect("smoke chain records");
    let raw = read_all(&target.join(STREAM_FILE)).unwrap();
    let canonical = read_all(&target.join(CANONICAL_STREAM_FILE)).unwrap();
    let canonical_bytes = std::fs::read(target.join(CANONICAL_STREAM_FILE)).unwrap();
    Run {
        chain,
        raw,
        canonical,
        canonical_bytes,
    }
}

fn envelopes(m: &LedgerCloseMeta) -> Vec<TransactionEnvelope> {
    let LedgerCloseMeta::V2(m) = m else {
        return Vec::new();
    };
    let stellar_xdr::GeneralizedTransactionSet::V1(set) = &m.tx_set;
    let mut out = Vec::new();
    for phase in set.phases.iter() {
        match phase {
            stellar_xdr::TransactionPhase::V0(comps) => {
                for c in comps.iter() {
                    let stellar_xdr::TxSetComponent::TxsetCompTxsMaybeDiscountedFee(c) = c;
                    out.extend(c.txs.iter().cloned());
                }
            }
            stellar_xdr::TransactionPhase::V1(p) => {
                for stage in p.execution_stages.iter() {
                    for cluster in stage.iter() {
                        out.extend(cluster.0.iter().cloned());
                    }
                }
            }
        }
    }
    out
}

fn signatures(env: &TransactionEnvelope) -> Vec<Vec<u8>> {
    match env {
        TransactionEnvelope::Tx(e) => e.signatures.iter().map(|s| s.signature.to_vec()).collect(),
        TransactionEnvelope::TxFeeBump(e) => {
            e.signatures.iter().map(|s| s.signature.to_vec()).collect()
        }
        TransactionEnvelope::TxV0(e) => e.signatures.iter().map(|s| s.signature.to_vec()).collect(),
    }
}

fn tx_hashes(m: &LedgerCloseMeta) -> Vec<[u8; 32]> {
    let LedgerCloseMeta::V2(m) = m else {
        return Vec::new();
    };
    m.tx_processing
        .iter()
        .map(|t| t.result.transaction_hash.0)
        .collect()
}

/// chain.json without the fields that cover core's raw order.
fn canonical_part(s: &ChainSummary) -> ChainSummary {
    let mut s = s.clone();
    s.stream_sha256.clear();
    for l in &mut s.ledgers {
        l.raw_sha256.clear();
    }
    s
}

fn limit(info: &serde_json::Value, path: &[&str]) -> u64 {
    let mut v = info;
    for p in path {
        v = &v[*p];
    }
    v.as_u64()
        .unwrap_or_else(|| panic!("sorobaninfo has no {path:?}: {info}"))
}

/// The limits sorobaninfo reports equal the captured testnet entries.
fn assert_testnet_limits(info: &serde_json::Value) {
    let s = NetworkSettings::testnet().unwrap();
    let Some(ConfigSettingEntry::ContractMaxSizeBytes(max_size)) =
        s.get(ConfigSettingId::ContractMaxSizeBytes)
    else {
        panic!("capture has no max contract size");
    };
    assert_eq!(limit(info, &["max_contract_size"]), u64::from(*max_size));
    let Some(ConfigSettingEntry::ContractComputeV0(c)) = s.get(ConfigSettingId::ContractComputeV0)
    else {
        panic!("capture has no compute settings");
    };
    assert_eq!(
        limit(info, &["tx", "max_instructions"]),
        c.tx_max_instructions as u64
    );
    assert_eq!(
        limit(info, &["ledger", "max_instructions"]),
        c.ledger_max_instructions as u64
    );
    assert_eq!(
        limit(info, &["tx", "memory_limit"]),
        u64::from(c.tx_memory_limit)
    );
    let Some(ConfigSettingEntry::ContractBandwidthV0(b)) =
        s.get(ConfigSettingId::ContractBandwidthV0)
    else {
        panic!("capture has no bandwidth settings");
    };
    assert_eq!(
        limit(info, &["tx", "max_size_bytes"]),
        u64::from(b.tx_max_size_bytes)
    );
    let Some(ConfigSettingEntry::ContractExecutionLanes(l)) =
        s.get(ConfigSettingId::ContractExecutionLanes)
    else {
        panic!("capture has no execution lanes");
    };
    assert_eq!(
        limit(info, &["ledger", "max_tx_count"]),
        u64::from(l.ledger_max_tx_count)
    );
    let Some(ConfigSettingEntry::StateArchival(a)) = s.get(ConfigSettingId::StateArchival) else {
        panic!("capture has no state archival settings");
    };
    assert_eq!(
        limit(info, &["state_archival", "min_persistent_ttl"]),
        u64::from(a.min_persistent_ttl)
    );
    assert_eq!(
        limit(info, &["state_archival", "max_entry_ttl"]),
        u64::from(a.max_entry_ttl)
    );
}

fn check_run(name: &str, r: &Run) {
    let s = &r.chain.summary;
    assert_eq!(s.normalization, NORMALIZATION);
    assert_eq!(s.ledger_range.first, FIRST_LEDGER);
    assert_eq!(s.ledger_range.last, LAST, "{name}");
    assert_eq!(r.raw.len(), r.canonical.len());
    for (raw, canon) in r.raw.iter().zip(&r.canonical) {
        assert_eq!(raw.ledger_seq, canon.ledger_seq);
        assert!(
            permutation_equal(&raw.meta, &canon.meta).unwrap(),
            "{name}: ledger {} raw and canonical are not permutations",
            raw.ledger_seq
        );
        roundtrip_check("LedgerCloseMeta", &canon.xdr, Limits::none())
            .unwrap_or_else(|e| panic!("{name}: ledger {} roundtrip: {e}", canon.ledger_seq));
    }
    let settings = &r.chain.settings;
    assert_eq!(settings.upgrade_ledger, 6);
    assert_eq!(
        upgrades(&r.canonical[(settings.upgrade_ledger - FIRST_LEDGER) as usize].meta),
        vec![LedgerUpgrade::Config(settings.upgrade.key.clone())]
    );
    assert_testnet_limits(&settings.soroban_info);
}

fn assert_same_bytes(name: &str, a: &Run, b: &Run) -> Vec<u32> {
    assert_eq!(
        a.canonical_bytes, b.canonical_bytes,
        "{name}: canonical streams differ"
    );
    assert_eq!(
        canonical_part(&a.chain.summary),
        canonical_part(&b.chain.summary),
        "{name}: chain.json differs beyond raw hashes"
    );
    a.raw
        .iter()
        .zip(&b.raw)
        .filter(|(x, y)| x.xdr != y.xdr)
        .map(|(x, _)| x.ledger_seq)
        .collect()
}

#[test]
#[ignore = "needs docker and the pinned stellar-core image; run with --include-ignored"]
fn smoke_chain_is_reproducible_and_network_independent_in_shape() {
    let tmp = tempfile::tempdir().unwrap();
    let mut runs = Vec::new();
    for (i, p) in [TESTNET, TESTNET, MAINNET, MAINNET].iter().enumerate() {
        let dir = tmp.path().join(format!("run{i}"));
        std::fs::create_dir(&dir).unwrap();
        let r = run(p, false, &dir);
        check_run(&format!("run{i}"), &r);
        runs.push(r);
    }
    for (name, a, b) in [
        ("testnet", &runs[0], &runs[1]),
        ("mainnet", &runs[2], &runs[3]),
    ] {
        let raw_differs = assert_same_bytes(name, a, b);
        eprintln!(
            "{name}: canonical streams identical ({} bytes, sha256 {}); raw streams differ (change order only) in ledgers {raw_differs:?} of {}",
            a.canonical_bytes.len(),
            a.chain.summary.canonical_stream_sha256,
            a.raw.len()
        );
    }

    let (t, m) = (&runs[0], &runs[2]);
    assert_ne!(t.chain.echo, m.chain.echo, "contract ids are network bound");
    let mut tx_count = 0;
    for (ct, cm) in t.canonical.iter().zip(&m.canonical) {
        assert_eq!(ct.ledger_seq, cm.ledger_seq);
        let seq = ct.ledger_seq;
        let st = shape_signature("LedgerCloseMeta", &ct.xdr, Limits::none()).unwrap();
        let sm = shape_signature("LedgerCloseMeta", &cm.xdr, Limits::none()).unwrap();
        assert_eq!(
            st, sm,
            "ledger {seq}: shape signatures differ across networks"
        );
        let (ht, hm) = (tx_hashes(&ct.meta), tx_hashes(&cm.meta));
        assert_eq!(ht.len(), hm.len());
        for (a, b) in ht.iter().zip(&hm) {
            assert_ne!(a, b, "ledger {seq}: same tx hash on both networks");
        }
        let (et, em) = (envelopes(&ct.meta), envelopes(&cm.meta));
        assert_eq!(et.len(), em.len());
        for (a, b) in et.iter().zip(&em) {
            let (sa, sb) = (signatures(a), signatures(b));
            assert_eq!(sa.len(), sb.len());
            assert!(sa.iter().zip(&sb).all(|(x, y)| x != y), "ledger {seq}");
        }
        tx_count += ht.len();
        let header = |f: &FramedLcm| match &f.meta {
            LedgerCloseMeta::V0(x) => x.ledger_header.header.scp_value.close_time.0,
            LedgerCloseMeta::V1(x) => x.ledger_header.header.scp_value.close_time.0,
            LedgerCloseMeta::V2(x) => x.ledger_header.header.scp_value.close_time.0,
        };
        assert_eq!(header(ct), close_time(seq));
    }
    // three bootstrap transactions, then fund, payment, trustline, upload,
    // deploy and invoke
    assert_eq!(tx_count, 9);
    let all: BTreeSet<String> = t
        .canonical
        .iter()
        .flat_map(|f| shape_signature("LedgerCloseMeta", &f.xdr, Limits::none()).unwrap())
        .collect();
    eprintln!("shape signature arms across the chain: {}", all.len());
}

fn has_timing_event(meta: &TransactionMeta) -> bool {
    let events = match meta {
        TransactionMeta::V4(m) => &m.diagnostic_events,
        _ => return false,
    };
    events.iter().any(|d| {
        let ContractEventBody::V0(body) = &d.event.body;
        matches!(
            body.topics.as_slice(),
            [ScVal::Symbol(a), ScVal::Symbol(b)]
                if a.as_slice() == b"core_metrics" && b.as_slice() == b"invoke_time_nsecs"
        )
    })
}

/// Why chains run with diagnostic events off: core then reports the wall
/// clock time of each host invocation in the meta.
#[test]
#[ignore = "needs docker and the pinned stellar-core image; run with --include-ignored"]
fn diagnostic_events_carry_wall_clock_timing() {
    let tmp = tempfile::tempdir().unwrap();
    let r = run(TESTNET, true, tmp.path());
    let last = r.canonical.last().unwrap();
    let LedgerCloseMeta::V2(m) = &last.meta else {
        panic!("not v2 meta");
    };
    let tx = m.tx_processing.iter().next().expect("the echo invocation");
    assert!(
        has_timing_event(&tx.tx_apply_processing),
        "no core_metrics invoke_time_nsecs event with diagnostic events on"
    );
    // and none of it in the chains we record
    let off = tmp.path().join("off");
    std::fs::create_dir(&off).unwrap();
    let r = run(TESTNET, false, &off);
    for f in &r.canonical {
        if let LedgerCloseMeta::V2(m) = &f.meta {
            for t in m.tx_processing.iter() {
                assert!(!has_timing_event(&t.tx_apply_processing));
            }
        }
    }
}
