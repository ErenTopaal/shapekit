//! The ledger that showed core's `normalizeMeta` order is not one core emits:
//! cap66.archival.p30 ledger 87, an invocation that auto-restores an archived
//! contract data entry and then writes it. Core's raw stream holds operation 0's
//! changes as [restored ttl, restored contract data, updated contract data];
//! sorting by change type moves the update in front of its restore, and ingest
//! code that pairs an `UPDATED` with the change before it fails on that. The
//! canonical form has to keep the restore in front.

use std::path::Path;

use shapekit_core_driver::canonical::{canonicalize_lcm, change_key};
use shapekit_core_driver::meta::read_all;
use stellar_xdr::{LedgerCloseMeta, LedgerEntryChange, TransactionMeta};

const LEDGER: u32 = 87;

fn op0_changes(lcm: &LedgerCloseMeta) -> Vec<Vec<LedgerEntryChange>> {
    let LedgerCloseMeta::V2(m) = lcm else {
        panic!("protocol 30 meta is not LedgerCloseMeta v2");
    };
    m.tx_processing
        .iter()
        .filter_map(|tx| match &tx.tx_apply_processing {
            TransactionMeta::V4(t) => t.operations.first().map(|o| o.changes.0.to_vec()),
            _ => None,
        })
        .collect()
}

// positions of a restore and a later-needed update of the same contract data key
fn restore_and_update(list: &[LedgerEntryChange]) -> Option<(usize, usize)> {
    list.iter().enumerate().find_map(|(i, c)| {
        let LedgerEntryChange::Restored(e) = c else {
            return None;
        };
        if !matches!(e.data, stellar_xdr::LedgerEntryData::ContractData(_)) {
            return None;
        }
        let key = change_key(c);
        list.iter()
            .position(|u| matches!(u, LedgerEntryChange::Updated(_)) && change_key(u) == key)
            .map(|j| (i, j))
    })
}

#[test]
fn restore_precedes_its_update_in_cap66_ledger_87() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/chains/cap66.archival.p30");
    for network in ["testnet", "mainnet"] {
        let frames = read_all(&root.join(network).join("ledgers.xdr")).expect("raw stream decodes");
        let frame = frames
            .iter()
            .find(|f| f.ledger_seq == LEDGER)
            .unwrap_or_else(|| panic!("{network}: no ledger {LEDGER}"));
        let mut canon = frame.meta.clone();
        canonicalize_lcm(&mut canon).unwrap();

        let raw_lists = op0_changes(&frame.meta);
        let canon_lists = op0_changes(&canon);
        let mut seen = 0;
        for (raw, canon) in raw_lists.iter().zip(&canon_lists) {
            let Some((r, u)) = restore_and_update(raw) else {
                continue;
            };
            seen += 1;
            assert!(r < u, "{network}: core's own stream has the update first");
            let (r, u) = restore_and_update(canon).expect("the canonical list keeps both");
            assert!(
                r < u,
                "{network}: the canonical list puts the update before its restore"
            );
        }
        assert!(
            seen > 0,
            "{network}: ledger {LEDGER} has no auto-restore followed by a write"
        );
    }
}
