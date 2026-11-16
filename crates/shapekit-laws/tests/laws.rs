//! Each law holds on recorded ledgers and fails on a ledger changed to break it.

use std::path::PathBuf;

use shapekit_laws::{Checker, Input, Law, Report};
use stellar_xdr::{LedgerCloseMeta, Limits, ReadXdr, WriteXdr};

fn corpus() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
}

/// Every LedgerCloseMeta variant of the given family in the corpus, with its passphrase.
fn family(name: &str) -> Vec<(String, Vec<u8>, String)> {
    let m: serde_json::Value =
        serde_json::from_slice(&std::fs::read(corpus().join("manifest.json")).unwrap()).unwrap();
    let mut out = Vec::new();
    for e in m["entries"].as_array().unwrap() {
        if e["xdr_type"] != "LedgerCloseMeta" || e["family"] != name {
            continue;
        }
        for (variant, v) in e["variants"].as_object().unwrap() {
            let bytes = std::fs::read(corpus().join(v["xdr_file"].as_str().unwrap())).unwrap();
            out.push((
                format!("{} {variant}", e["id"].as_str().unwrap()),
                bytes,
                v["network_passphrase"].as_str().unwrap().to_string(),
            ));
        }
    }
    assert!(
        !out.is_empty(),
        "no LedgerCloseMeta entries in family {name}"
    );
    out
}

fn check(bytes: &[u8], passphrase: &str) -> Report {
    let mut c = Checker::new(passphrase);
    c.bytes(bytes, Input::Ledger).unwrap();
    c.report
}

fn checked(r: &Report, law: Law) -> u64 {
    r.laws[&law].checked
}

fn violated(r: &Report, law: Law) -> u64 {
    r.laws[&law].violated
}

#[test]
fn families_hold_and_exercise_their_laws() {
    // each family is chosen for the identifier law it exercises
    for (fam, law) in [
        ("cap23.claimable-balances", Law::BalanceId),
        ("cap38.liquidity-pools", Law::PoolId),
        ("soroban.invoke-basics", Law::ContractId),
        ("cap15.fee-bump", Law::TxHash),
    ] {
        let mut total = Report::new();
        for (id, bytes, pass) in family(fam) {
            let r = check(&bytes, &pass);
            assert!(r.holds(), "{id}: {:?}", r.violations);
            total.merge(r);
        }
        assert!(checked(&total, law) > 0, "{fam} never applied {law:?}");
        assert!(checked(&total, Law::ResultSetHash) > 0);
    }
}

fn first(fam: &str) -> (Vec<u8>, String) {
    let (_, bytes, pass) = family(fam)
        .into_iter()
        .find(|(_, b, p)| {
            let lcm = LedgerCloseMeta::from_xdr(b, Limits::none()).unwrap();
            let r = check(b, p);
            r.transactions > 0 && matches!(lcm, LedgerCloseMeta::V2(_))
        })
        .expect("a v2 ledger with transactions");
    (bytes, pass)
}

fn edit(bytes: &[u8], f: impl FnOnce(&mut stellar_xdr::LedgerCloseMetaV2)) -> Vec<u8> {
    let mut lcm = LedgerCloseMeta::from_xdr(bytes, Limits::none()).unwrap();
    let LedgerCloseMeta::V2(v) = &mut lcm else {
        panic!("a v2 ledger")
    };
    f(v);
    lcm.to_xdr(Limits::none()).unwrap()
}

#[test]
fn a_changed_header_breaks_the_header_hash() {
    let (bytes, pass) = first("cap01.bump-sequence");
    let bad = edit(&bytes, |v| v.ledger_header.header.fee_pool += 1);
    let r = check(&bad, &pass);
    assert_eq!(violated(&r, Law::HeaderHash), 1);
    assert_eq!(r.violations.len(), 1, "{:?}", r.violations);
}

#[test]
fn a_changed_result_breaks_the_result_set_hash() {
    let (bytes, pass) = first("cap01.bump-sequence");
    let bad = edit(&bytes, |v| {
        let mut txs = v.tx_processing.to_vec();
        txs[0].result.result.fee_charged += 1;
        v.tx_processing = txs.try_into().unwrap();
    });
    let r = check(&bad, &pass);
    assert_eq!(violated(&r, Law::ResultSetHash), 1, "{:?}", r.violations);
}

#[test]
fn a_changed_result_hash_breaks_the_tx_hash_law() {
    let (bytes, pass) = first("cap01.bump-sequence");
    let bad = edit(&bytes, |v| {
        let mut txs = v.tx_processing.to_vec();
        txs[0].result.transaction_hash.0[0] ^= 1;
        v.tx_processing = txs.try_into().unwrap();
    });
    let r = check(&bad, &pass);
    assert_eq!(violated(&r, Law::TxHash), 1, "{:?}", r.violations);
}

#[test]
fn another_passphrase_breaks_the_tx_hash_law() {
    let (bytes, _) = first("cap01.bump-sequence");
    let r = check(&bytes, "not the network");
    assert_eq!(violated(&r, Law::TxHash), 1);
}

#[test]
fn a_changed_tx_set_breaks_the_tx_set_hash() {
    let (bytes, pass) = first("cap01.bump-sequence");
    let bad = edit(&bytes, |v| {
        let stellar_xdr::GeneralizedTransactionSet::V1(s) = &mut v.tx_set;
        s.previous_ledger_hash.0[0] ^= 1;
    });
    let r = check(&bad, &pass);
    assert_eq!(violated(&r, Law::TxSetHash), 1);
    assert_eq!(violated(&r, Law::TxSetPrevious), 1);
}

#[test]
fn trailing_bytes_and_truncation_are_rejected() {
    let (bytes, pass) = first("cap01.bump-sequence");
    let r = check(&bytes, &pass);
    assert_eq!(checked(&r, Law::Roundtrip), 1);
    assert_eq!(checked(&r, Law::NoncanonicalRejected), 1);
    assert!(r.holds());
    let mut longer = bytes.clone();
    longer.extend_from_slice(&[0; 4]);
    assert!(Checker::new(&pass).bytes(&longer, Input::Ledger).is_err());
}

#[test]
fn the_chain_law_links_consecutive_ledgers() {
    let (bytes, pass) = first("cap01.bump-sequence");
    let lcm = LedgerCloseMeta::from_xdr(&bytes, Limits::none()).unwrap();
    let LedgerCloseMeta::V2(v) = &lcm else {
        panic!("a v2 ledger")
    };
    // a successor whose previousLedgerHash is this ledger's hash, and one whose is not
    let mut next = v.clone();
    next.ledger_header.header.ledger_seq += 1;
    next.ledger_header.header.previous_ledger_hash = v.ledger_header.hash.clone();
    let mut c = Checker::new(&pass);
    c.ledger(&lcm);
    c.ledger(&LedgerCloseMeta::V2(next.clone()));
    assert_eq!(c.report.laws[&Law::HeaderChain].checked, 1);
    assert_eq!(c.report.laws[&Law::HeaderChain].violated, 0);

    next.ledger_header.header.previous_ledger_hash.0[0] ^= 1;
    let mut c = Checker::new(&pass);
    c.ledger(&lcm);
    c.ledger(&LedgerCloseMeta::V2(next));
    assert_eq!(c.report.laws[&Law::HeaderChain].violated, 1);
}

#[test]
fn derived_ids_match_known_values() {
    // the public network id is a published constant
    assert_eq!(
        hex::encode(shapekit_laws::derive::network_id(
            "Public Global Stellar Network ; September 2015"
        )),
        "7ac33997544e3175d266bd022439b22cdb16508c01163f26e5cb2a3e1045a979"
    );
}

#[test]
fn every_recorded_corpus_ledger_holds() {
    let m: serde_json::Value =
        serde_json::from_slice(&std::fs::read(corpus().join("manifest.json")).unwrap()).unwrap();
    let mut variants = 0;
    let mut synthetic_violations = Vec::new();
    for e in m["entries"].as_array().unwrap() {
        if e["xdr_type"] != "LedgerCloseMeta" {
            continue;
        }
        for v in e["variants"].as_object().unwrap().values() {
            let bytes = std::fs::read(corpus().join(v["xdr_file"].as_str().unwrap())).unwrap();
            let mut c = match v["network_passphrase"].as_str() {
                Some(p) => Checker::new(p),
                None => Checker::without_network(),
            };
            c.bytes(&bytes, Input::Ledger).unwrap();
            variants += 1;
            if e["synthetic"] == true {
                synthetic_violations.extend(c.report.violations.iter().map(|v| v.law));
            } else {
                assert!(c.report.holds(), "{}: {:?}", e["id"], c.report.violations);
            }
        }
    }
    assert_eq!(variants, 714);
    // the one synthetic ledger is hand-made with zeroed header hashes; its tx set and result
    // set are not the ones those hashes would commit to
    assert_eq!(
        synthetic_violations,
        vec![Law::HeaderHash, Law::TxSetPrevious, Law::ResultSetHash]
    );
}
