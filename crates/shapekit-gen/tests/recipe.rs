//! Selection out of real ledger meta and the cap40 recipe's payloads. The
//! end-to-end chain run is in the cli tests, behind SHAPEKIT_DOCKER_TESTS.

use std::path::Path;

use sha2::{Digest, Sha256};
use shapekit_gen::recipe::{Selector, cap40, select};
use shapekit_manifest::{TESTNET_PASSPHRASE, roundtrip_check};
use stellar_xdr::{
    LedgerCloseMeta, LedgerEntry, LedgerEntryChange, Limits, ReadXdr, TransactionEnvelope,
    TransactionMeta, TransactionResultMetaV1,
};

// testnet ledger 5694308, provenance next to the file
fn fixture() -> (LedgerCloseMeta, Vec<u8>) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../shapekit-manifest/tests/fixtures/testnet-5694308.lcm.xdr");
    let bytes = std::fs::read(path).unwrap();
    let lcm = LedgerCloseMeta::from_xdr(&bytes, Limits::none()).unwrap();
    (lcm, bytes)
}

fn network_id() -> [u8; 32] {
    Sha256::digest(TESTNET_PASSPHRASE.as_bytes()).into()
}

fn limits(len: usize) -> Limits {
    Limits { depth: 1000, len }
}

#[test]
fn ledger_selector_returns_the_ledger_as_given() {
    let (lcm, bytes) = fixture();
    let got = select(&Selector::Ledger(5694308), &lcm, &bytes, &network_id()).unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].xdr_type, "LedgerCloseMeta");
    assert_eq!(got[0].bytes, bytes);
    assert_eq!(got[0].lcm_version, Some(2));
    assert!(got[0].normalized);
}

#[test]
fn transaction_selector_finds_every_transaction_of_the_ledger() {
    let (lcm, bytes) = fixture();
    let LedgerCloseMeta::V2(v2) = &lcm else {
        panic!("fixture is a v2 ledger")
    };
    assert!(v2.tx_processing.len() > 1);
    for (i, t) in v2.tx_processing.iter().enumerate() {
        let hash = t.result.transaction_hash.0;
        let sel = Selector::Transaction {
            ledger: 5694308,
            hash,
        };
        let got = select(&sel, &lcm, &bytes, &network_id()).unwrap();
        assert_eq!(got.len(), 2);
        let (env, meta) = (&got[0], &got[1]);
        assert_eq!(env.suffix, "-envelope");
        assert_eq!(meta.suffix, "-meta");
        assert_eq!(meta.xdr_type, "TransactionResultMetaV1");
        assert_eq!(env.tx_index, Some(i as u32));
        roundtrip_check(env.xdr_type, &env.bytes, limits(env.bytes.len())).unwrap();
        let e = TransactionEnvelope::from_xdr(&env.bytes, Limits::none()).unwrap();
        assert_eq!(
            shapekit_core_driver::tx::envelope_hash(&e, &network_id()).unwrap(),
            hash
        );
        let m = TransactionResultMetaV1::from_xdr(&meta.bytes, Limits::none()).unwrap();
        assert_eq!(m, *t);
    }
    let missing = Selector::Transaction {
        ledger: 5694308,
        hash: [7; 32],
    };
    assert!(select(&missing, &lcm, &bytes, &network_id()).is_err());
}

#[test]
fn ledger_entry_selector_takes_the_last_state_in_apply_order() {
    let (lcm, bytes) = fixture();
    let LedgerCloseMeta::V2(v2) = &lcm else {
        panic!("fixture is a v2 ledger")
    };
    // the fee source of the first transaction: its fee is charged first and
    // its account changes again later, so the last update must win
    let first = &v2.tx_processing[0];
    let fee_entry: &LedgerEntry = first
        .fee_processing
        .iter()
        .find_map(|c| match c {
            LedgerEntryChange::Updated(e) => Some(e),
            _ => None,
        })
        .expect("a fee update");
    let key = fee_entry.to_key();
    let mut last = None;
    let mut lists: Vec<&stellar_xdr::LedgerEntryChanges> =
        v2.tx_processing.iter().map(|t| &t.fee_processing).collect();
    for t in v2.tx_processing.iter() {
        if let TransactionMeta::V4(m) = &t.tx_apply_processing {
            lists.push(&m.tx_changes_before);
            lists.extend(m.operations.iter().map(|o| &o.changes));
            lists.push(&m.tx_changes_after);
        }
        lists.push(&t.post_tx_apply_fee_processing);
    }
    for l in lists {
        for c in l.iter() {
            if let LedgerEntryChange::Updated(e) = c
                && e.to_key() == key
            {
                last = Some(e.clone());
            }
        }
    }
    let sel = Selector::LedgerEntry {
        ledger: 5694308,
        key,
    };
    let got = select(&sel, &lcm, &bytes, &network_id()).unwrap();
    assert_eq!(got[0].xdr_type, "LedgerEntry");
    let e = LedgerEntry::from_xdr(&got[0].bytes, Limits::none()).unwrap();
    assert_eq!(Some(e), last);
}

#[test]
fn payloads_have_the_asked_length_and_are_stable() {
    for n in [1, 4, 32, 33, 64] {
        let p = cap40::payload("account", n);
        assert_eq!(p.len(), n);
        assert_eq!(p, cap40::payload("account", n));
    }
    assert_ne!(cap40::payload("account", 64), cap40::payload("extra", 64));
    // a longer payload is not an extension of a shorter one, the length is hashed in
    assert_ne!(
        cap40::payload("account", 4)[..],
        cap40::payload("account", 32)[..4]
    );
}
