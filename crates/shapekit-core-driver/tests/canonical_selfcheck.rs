//! The recorder's self-check on canonical meta does not rest on the canonicalizer it
//! checks. A canonicalizer that sorted by key and then by change type (core's
//! `normalizeMeta` order) would map a list and its own output to the same bytes, so
//! comparing both sides after canonicalizing them could not see it reorder one key's
//! changes. The per-key sequence check sees it: on cap66.archival.p30 ledger 87 that order
//! moves an `UPDATED` in front of the `RESTORED` of the same contract data key.

use std::path::Path;

use shapekit_core_driver::canonical::change_key;
use shapekit_core_driver::meta::read_all;
use shapekit_core_driver::recorder::{RecorderError, canonical_frame, check_canonical};
use stellar_xdr::{LedgerCloseMeta, LedgerEntryChange, LedgerEntryChanges, TransactionMeta};

const LEDGER: u32 = 87;

fn change_type(c: &LedgerEntryChange) -> u32 {
    match c {
        LedgerEntryChange::Created(_) => 0,
        LedgerEntryChange::Updated(_) => 1,
        LedgerEntryChange::Removed(_) => 2,
        LedgerEntryChange::State(_) => 3,
        LedgerEntryChange::Restored(_) => 4,
    }
}

fn sort_by_key_then_type(l: &LedgerEntryChanges) -> LedgerEntryChanges {
    let mut v = l.0.to_vec();
    v.sort_by(|a, b| {
        change_key(a)
            .cmp(&change_key(b))
            .then(change_type(a).cmp(&change_type(b)))
    });
    LedgerEntryChanges(v.try_into().unwrap())
}

/// The operation change lists of a protocol 30 ledger, sorted as a faulty canonicalizer
/// would sort them.
fn type_sorted(lcm: &LedgerCloseMeta) -> LedgerCloseMeta {
    let mut out = lcm.clone();
    let LedgerCloseMeta::V2(m) = &mut out else {
        panic!("protocol 30 meta is not LedgerCloseMeta v2");
    };
    let mut txs = m.tx_processing.to_vec();
    for tx in &mut txs {
        if let TransactionMeta::V4(t) = &mut tx.tx_apply_processing {
            let mut ops = t.operations.to_vec();
            for op in &mut ops {
                op.changes = sort_by_key_then_type(&op.changes);
            }
            t.operations = ops.try_into().unwrap();
        }
    }
    m.tx_processing = txs.try_into().unwrap();
    out
}

#[test]
fn a_reordering_within_one_key_is_caught() {
    let raw = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/chains/cap66.archival.p30/testnet/ledgers.xdr");
    let frames = read_all(&raw).expect("raw stream decodes");
    let f = frames
        .iter()
        .find(|f| f.ledger_seq == LEDGER)
        .expect("ledger 87 is recorded");

    // the real canonical form passes
    let good = canonical_frame(f).expect("canonical meta passes its self-check");
    check_canonical(LEDGER, &f.meta, &good.meta).unwrap();

    // the faulty order is a fixed point of itself: canonicalizing it with the faulty rule
    // again changes nothing, which is why only an independent check can see it
    let bad = type_sorted(&good.meta);
    assert_ne!(
        bad, good.meta,
        "ledger 87 has a key whose changes the type order moves"
    );
    assert_eq!(type_sorted(&bad), bad);
    match check_canonical(LEDGER, &f.meta, &bad) {
        Err(RecorderError::KeyOrder(LEDGER)) => {}
        other => panic!("expected a key order failure, got {other:?}"),
    }
}
