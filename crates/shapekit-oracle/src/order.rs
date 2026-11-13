//! Reading one change list key by key, checking that each key's changes chain.
//!
//! The canonical meta the chains are committed in sorts every change list by ledger key and
//! keeps each key's changes in the order core emitted them (see shapekit-core-driver's
//! `canonical` module), so within one key the list is already the logical sequence. Core
//! emits a list as one or more `LedgerTxn` commits concatenated, each giving a key at most
//! one group: `STATE` then `UPDATED` or `REMOVED`, a lone `CREATED`, or `RESTORED`
//! optionally followed by `UPDATED` or `REMOVED`. A key may have two groups in one list (a
//! fee bump whose fee source is the inner source: `txChangesBefore` holds a no-op signer
//! pass and then the sequence bump for the same account).
//!
//! [`raw_order`] keeps that order and refuses a key whose changes do not chain in it: every
//! `STATE` must equal the value before it, every `CREATED` must follow an absent entry, and
//! every `UPDATED` or `REMOVED` must follow a `STATE` or `RESTORED`. Meta sorted by change
//! type (core's `normalizeMeta`, which can put a `RESTORED` after its `UPDATED`) is refused
//! rather than repaired, since no reader of real meta sees that order.

use std::collections::BTreeMap;

use stellar_xdr::{LedgerEntry, LedgerEntryChange, LedgerKey};

fn key_of(c: &LedgerEntryChange) -> LedgerKey {
    match c {
        LedgerEntryChange::Created(e)
        | LedgerEntryChange::Updated(e)
        | LedgerEntryChange::State(e)
        | LedgerEntryChange::Restored(e) => e.to_key(),
        LedgerEntryChange::Removed(k) => k.clone(),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Free,
    AfterState,
    AfterRestore,
}

// `None` while nothing about the value is known yet, then the value itself
fn check_group(key: &LedgerKey, group: &[&LedgerEntryChange]) -> Result<(), String> {
    let mut cur: Option<Option<&LedgerEntry>> = None;
    let mut phase = Phase::Free;
    for (i, c) in group.iter().enumerate() {
        let next = match (phase, c) {
            (Phase::Free | Phase::AfterRestore, LedgerEntryChange::State(e))
                if cur.is_none() || cur == Some(Some(e)) =>
            {
                Some((Some(e), Phase::AfterState))
            }
            (Phase::Free | Phase::AfterRestore, LedgerEntryChange::Created(e))
                if matches!(cur, None | Some(None)) =>
            {
                Some((Some(e), Phase::Free))
            }
            (Phase::Free | Phase::AfterRestore, LedgerEntryChange::Restored(e)) => {
                Some((Some(e), Phase::AfterRestore))
            }
            (Phase::AfterState | Phase::AfterRestore, LedgerEntryChange::Updated(e)) => {
                Some((Some(e), Phase::Free))
            }
            (Phase::AfterState | Phase::AfterRestore, LedgerEntryChange::Removed(_)) => {
                Some((None, Phase::Free))
            }
            _ => None,
        };
        let Some((value, phase2)) = next else {
            return Err(format!(
                "change {i} of {} to {key:?} does not follow the ones before it",
                group.len()
            ));
        };
        cur = Some(value);
        phase = phase2;
    }
    if phase == Phase::AfterState {
        return Err(format!("the changes to {key:?} end in a STATE"));
    }
    Ok(())
}

/// `changes` one key's changes together, keys in order of first appearance, each key's
/// changes in the order given. Fails when a key's changes do not chain in that order.
pub fn raw_order(changes: &[LedgerEntryChange]) -> Result<Vec<&LedgerEntryChange>, String> {
    let mut keys: Vec<LedgerKey> = Vec::new();
    let mut groups: BTreeMap<LedgerKey, Vec<&LedgerEntryChange>> = BTreeMap::new();
    for c in changes {
        let k = key_of(c);
        let g = groups.entry(k.clone()).or_default();
        if g.is_empty() {
            keys.push(k);
        }
        g.push(c);
    }
    let mut out = Vec::with_capacity(changes.len());
    for k in keys {
        let g = groups.remove(&k).expect("every key has a group");
        check_group(&k, &g)?;
        out.extend(g);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use stellar_xdr::{
        AccountEntry, AccountEntryExt, AccountId, LedgerEntryData, LedgerEntryExt, PublicKey,
        SequenceNumber, String32, Thresholds, Uint256,
    };

    use super::*;

    fn acct(seq: i64) -> LedgerEntry {
        LedgerEntry {
            last_modified_ledger_seq: 1,
            data: LedgerEntryData::Account(AccountEntry {
                account_id: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([7; 32]))),
                balance: 10,
                seq_num: SequenceNumber(seq),
                num_sub_entries: 0,
                inflation_dest: None,
                flags: 0,
                home_domain: String32::default(),
                thresholds: Thresholds([1, 0, 0, 0]),
                signers: Default::default(),
                ext: AccountEntryExt::V0,
            }),
            ext: LedgerEntryExt::V0,
        }
    }

    fn seqs(v: &[&LedgerEntryChange]) -> Vec<(char, i64)> {
        v.iter()
            .map(|c| match c {
                LedgerEntryChange::State(e) => ('s', seq(e)),
                LedgerEntryChange::Updated(e) => ('u', seq(e)),
                LedgerEntryChange::Created(e) => ('c', seq(e)),
                LedgerEntryChange::Restored(e) => ('r', seq(e)),
                LedgerEntryChange::Removed(_) => ('x', 0),
            })
            .collect()
    }

    fn seq(e: &LedgerEntry) -> i64 {
        match &e.data {
            LedgerEntryData::Account(a) => a.seq_num.0,
            _ => unreachable!(),
        }
    }

    /// The fee bump over its own source: a no-op pass then the sequence bump.
    #[test]
    fn two_groups_of_one_key_are_read_as_given() {
        let raw = vec![
            LedgerEntryChange::State(acct(8)),
            LedgerEntryChange::Updated(acct(8)),
            LedgerEntryChange::State(acct(8)),
            LedgerEntryChange::Updated(acct(9)),
        ];
        let got = raw_order(&raw).unwrap();
        assert_eq!(seqs(&got), vec![('s', 8), ('u', 8), ('s', 8), ('u', 9)]);
    }

    #[test]
    fn a_cycle_is_read_as_given() {
        let raw = vec![
            LedgerEntryChange::State(acct(2)),
            LedgerEntryChange::Updated(acct(1)),
            LedgerEntryChange::State(acct(1)),
            LedgerEntryChange::Updated(acct(2)),
        ];
        let got = raw_order(&raw).unwrap();
        assert_eq!(seqs(&got), vec![('s', 2), ('u', 1), ('s', 1), ('u', 2)]);
    }

    #[test]
    fn restore_then_update_chains() {
        let raw = vec![
            LedgerEntryChange::Restored(acct(4)),
            LedgerEntryChange::Updated(acct(5)),
        ];
        let got = raw_order(&raw).unwrap();
        assert_eq!(seqs(&got), vec![('r', 4), ('u', 5)]);
    }

    /// The order core's `normalizeMeta` gives the pair above; no reader of real meta sees
    /// it, so it is refused instead of being put back.
    #[test]
    fn update_before_its_restore_is_refused() {
        let sorted = vec![
            LedgerEntryChange::Updated(acct(5)),
            LedgerEntryChange::Restored(acct(4)),
        ];
        assert!(raw_order(&sorted).is_err());
    }

    #[test]
    fn changes_that_do_not_chain_are_refused() {
        let raw = vec![
            LedgerEntryChange::State(acct(1)),
            LedgerEntryChange::Updated(acct(2)),
            LedgerEntryChange::State(acct(3)),
            LedgerEntryChange::Updated(acct(4)),
        ];
        assert!(raw_order(&raw).is_err());
    }
}
