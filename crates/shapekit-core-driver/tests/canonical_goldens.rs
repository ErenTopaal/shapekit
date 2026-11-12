//! `canonicalize_lcm` against stellar-core's own goldens (v30.0.0, see
//! fixtures/core-golden/provenance.json). Core captured these in the order
//! its LedgerTxn emitted the changes, not sorted, so they show that the
//! canonical form reorders real meta, only reorders it, and is stable.

use std::path::Path;

use shapekit_core_driver::canonical::{canonicalize_lcm, change_multisets, permutation_equal};
use shapekit_core_driver::meta::read_all;
use stellar_xdr::{LedgerCloseMeta, Limits, WriteXdr};

fn golden(name: &str) -> Vec<LedgerCloseMeta> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/core-golden")
        .join(name);
    let frames = read_all(&path).expect("golden decodes stream-framed");
    for f in &frames {
        // the decoded value re-encodes to exactly the stored bytes, so any
        // later byte difference comes from canonicalization alone
        assert_eq!(f.meta.to_xdr(Limits::none()).unwrap(), f.xdr);
    }
    frames.into_iter().map(|f| f.meta).collect()
}

fn check(name: &str) -> usize {
    let mut reordered = 0;
    for raw in golden(name) {
        let mut canon = raw.clone();
        canonicalize_lcm(&mut canon).unwrap();
        let mut twice = canon.clone();
        canonicalize_lcm(&mut twice).unwrap();
        assert_eq!(
            twice.to_xdr(Limits::none()).unwrap(),
            canon.to_xdr(Limits::none()).unwrap(),
            "{name}: not idempotent"
        );
        assert_eq!(
            change_multisets(&raw).unwrap(),
            change_multisets(&canon).unwrap(),
            "{name}: change multisets differ"
        );
        assert!(permutation_equal(&raw, &canon).unwrap());
        if canon != raw {
            reordered += 1;
        }
    }
    reordered
}

#[test]
fn canonical_form_of_core_goldens() {
    // both fixtures were chosen because core left some lists unsorted
    assert!(check("a61f6dd6f81e6f17.xdr") > 0);
    assert!(check("1467a37ff2a1e960.xdr") > 0);
}

#[test]
fn a_changed_entry_is_not_a_permutation() {
    let raw = golden("1467a37ff2a1e960.xdr").remove(1);
    let mut other = raw.clone();
    let LedgerCloseMeta::V2(m) = &mut other else {
        panic!("protocol 30 golden is not LedgerCloseMeta v2");
    };
    let tx = m.tx_processing.iter_mut().next().expect("a transaction");
    let mut fee = tx.fee_processing.0.to_vec();
    fee.pop().expect("a fee change");
    tx.fee_processing.0 = fee.try_into().unwrap();
    assert!(!permutation_equal(&raw, &other).unwrap());
}
