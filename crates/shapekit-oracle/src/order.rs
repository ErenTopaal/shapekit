//! Recovering an apply order for one change list in canonical form.
//!
//! The canonical meta the chains are committed in sorts every change list by key, then by
//! change type (state, created, updated, removed, restored), then by hash (core's
//! `normalizeMeta`). Core emits a list as one or more `LedgerTxn` commits concatenated, each
//! giving a key at most one group: `STATE` then `UPDATED` or `REMOVED`, a lone `CREATED`,
//! or `RESTORED` optionally followed by `UPDATED` or `REMOVED`. When a key has two groups in
//! one list (a fee bump whose fee source is the inner source: `txChangesBefore` holds a
//! no-op signer pass and the sequence bump for the same account), or `RESTORED` sorts
//! after its `UPDATED`, reading the sorted list front to back pairs the wrong values.
//!
//! [`raw_order`] puts each such key's changes back into an order that chains: every
//! `STATE` equals the value before it, every `CREATED` follows an absent entry. All orders
//! that chain are searched; they must agree on the value before and after the list, or the
//! list is reported as ambiguous. A caller that knows the value before the list (the ledger
//! state) passes it to settle an ambiguity, never to replace a contradiction.

use std::collections::BTreeMap;

use stellar_xdr::{LedgerEntry, LedgerEntryChange, LedgerKey};

/// Larger groups would make the search slow; core never writes one key this often in one
/// list (each group is one `LedgerTxn` commit of a transaction step).
const MAX_GROUP: usize = 8;

fn key_of(c: &LedgerEntryChange) -> LedgerKey {
    match c {
        LedgerEntryChange::Created(e)
        | LedgerEntryChange::Updated(e)
        | LedgerEntryChange::State(e)
        | LedgerEntryChange::Restored(e) => e.to_key(),
        LedgerEntryChange::Removed(k) => k.clone(),
    }
}

// a group that reads correctly front to back
fn plain(group: &[&LedgerEntryChange]) -> bool {
    use LedgerEntryChange as C;
    matches!(
        group,
        [_] | [C::State(_) | C::Restored(_), C::Updated(_) | C::Removed(_)]
    )
}

#[derive(Clone, Copy, PartialEq)]
enum Cur<'a> {
    Any,
    Val(Option<&'a LedgerEntry>),
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Free,
    AfterState,
    AfterRestore,
}

struct Search<'a> {
    group: Vec<&'a LedgerEntryChange>,
    used: Vec<bool>,
    path: Vec<usize>,
    // (value before, value after, order)
    found: Vec<(Option<&'a LedgerEntry>, Option<&'a LedgerEntry>, Vec<usize>)>,
}

impl<'a> Search<'a> {
    fn run(&mut self, cur: Cur<'a>, phase: Phase, before: Option<Option<&'a LedgerEntry>>) {
        if self.path.len() == self.group.len() {
            if phase != Phase::AfterState
                && let (Some(b), Cur::Val(a)) = (before, cur)
            {
                self.found.push((b, a, self.path.clone()));
            }
            return;
        }
        for i in 0..self.group.len() {
            if self.used[i] {
                continue;
            }
            // equal changes are interchangeable, so only the first unused one is tried
            if (0..i).any(|j| !self.used[j] && self.group[j] == self.group[i]) {
                continue;
            }
            let c = self.group[i];
            let next = match (phase, c) {
                (Phase::Free | Phase::AfterRestore, LedgerEntryChange::State(e))
                    if matches!(cur, Cur::Any) || cur == Cur::Val(Some(e)) =>
                {
                    Some((Cur::Val(Some(e)), Phase::AfterState, Some(e)))
                }
                (Phase::Free | Phase::AfterRestore, LedgerEntryChange::Created(e))
                    if matches!(cur, Cur::Any | Cur::Val(None)) =>
                {
                    Some((Cur::Val(Some(e)), Phase::Free, None))
                }
                (Phase::Free | Phase::AfterRestore, LedgerEntryChange::Restored(e)) => {
                    Some((Cur::Val(Some(e)), Phase::AfterRestore, Some(e)))
                }
                (Phase::AfterState | Phase::AfterRestore, LedgerEntryChange::Updated(e)) => {
                    Some((Cur::Val(Some(e)), Phase::Free, None))
                }
                (Phase::AfterState | Phase::AfterRestore, LedgerEntryChange::Removed(_)) => {
                    Some((Cur::Val(None), Phase::Free, None))
                }
                _ => None,
            };
            let Some((cur2, phase2, start)) = next else {
                continue;
            };
            // the value before the list is fixed by the first change
            let before2 = before.or(match c {
                LedgerEntryChange::Created(_) => Some(None),
                _ => start.map(Some),
            });
            self.used[i] = true;
            self.path.push(i);
            self.run(cur2, phase2, before2);
            self.path.pop();
            self.used[i] = false;
        }
    }
}

fn order_group<'a>(
    key: &LedgerKey,
    group: Vec<&'a LedgerEntryChange>,
    known: Option<Option<&LedgerEntry>>,
) -> Result<Vec<&'a LedgerEntryChange>, String> {
    if plain(&group) {
        return Ok(group);
    }
    if group.len() > MAX_GROUP {
        return Err(format!(
            "{} changes to one key in one list, more than {MAX_GROUP}: {key:?}",
            group.len()
        ));
    }
    let n = group.len();
    let mut s = Search {
        group,
        used: vec![false; n],
        path: Vec::new(),
        found: Vec::new(),
    };
    s.run(Cur::Any, Phase::Free, None);
    let agree = |f: &[(Option<&LedgerEntry>, Option<&LedgerEntry>, Vec<usize>)]| {
        f.iter().all(|x| x.0 == f[0].0 && x.1 == f[0].1)
    };
    if s.found.is_empty() {
        return Err(format!("the changes to {key:?} do not chain in any order"));
    }
    if !agree(&s.found) {
        let Some(k) = known else {
            return Err(format!(
                "the changes to {key:?} chain in orders with different results"
            ));
        };
        s.found.retain(|x| x.0 == k);
        if s.found.is_empty() || !agree(&s.found) {
            return Err(format!(
                "the changes to {key:?} chain in orders with different results, \
                 and the known value before the list does not settle it"
            ));
        }
    }
    let order = s.found.swap_remove(0).2;
    Ok(order.into_iter().map(|i| s.group[i]).collect())
}

/// `changes` in an order core could have emitted them in, one key's changes together, keys
/// in order of first appearance. `known` gives, where the caller knows it, the value of a
/// key before the list (`Some(None)` for an absent entry).
pub fn raw_order<'a, 'k>(
    changes: &'a [LedgerEntryChange],
    known: impl Fn(&LedgerKey) -> Option<Option<&'k LedgerEntry>>,
) -> Result<Vec<&'a LedgerEntryChange>, String> {
    let mut keys: Vec<LedgerKey> = Vec::new();
    let mut groups: BTreeMap<LedgerKey, Vec<&'a LedgerEntryChange>> = BTreeMap::new();
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
        out.extend(order_group(&k, g, known(&k))?);
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

    /// The fee bump over its own source: a no-op pass then the sequence bump, sorted so
    /// both STATEs come first and the no-op UPDATED last.
    #[test]
    fn two_groups_of_one_key_chain_back() {
        let sorted = vec![
            LedgerEntryChange::State(acct(8)),
            LedgerEntryChange::State(acct(8)),
            LedgerEntryChange::Updated(acct(9)),
            LedgerEntryChange::Updated(acct(8)),
        ];
        let got = raw_order(&sorted, |_| None).unwrap();
        assert_eq!(seqs(&got), vec![('s', 8), ('u', 8), ('s', 8), ('u', 9)]);
    }

    #[test]
    fn restored_then_updated_is_put_back_in_front() {
        let sorted = vec![
            LedgerEntryChange::Updated(acct(5)),
            LedgerEntryChange::Restored(acct(4)),
        ];
        let got = raw_order(&sorted, |_| None).unwrap();
        assert_eq!(seqs(&got), vec![('r', 4), ('u', 5)]);
    }

    #[test]
    fn a_cycle_needs_the_known_value() {
        // 1 -> 2 -> 1 and 2 -> 1 -> 2 chain the same multiset
        let sorted = vec![
            LedgerEntryChange::State(acct(1)),
            LedgerEntryChange::State(acct(2)),
            LedgerEntryChange::Updated(acct(1)),
            LedgerEntryChange::Updated(acct(2)),
        ];
        assert!(raw_order(&sorted, |_| None).is_err());
        let one = acct(1);
        let got = raw_order(&sorted, |_| Some(Some(&one))).unwrap();
        assert_eq!(seqs(&got), vec![('s', 1), ('u', 2), ('s', 2), ('u', 1)]);
    }

    #[test]
    fn changes_that_never_chain_are_refused() {
        let sorted = vec![
            LedgerEntryChange::State(acct(1)),
            LedgerEntryChange::State(acct(3)),
            LedgerEntryChange::Updated(acct(2)),
            LedgerEntryChange::Updated(acct(4)),
        ];
        assert!(raw_order(&sorted, |_| None).is_err());
    }
}
