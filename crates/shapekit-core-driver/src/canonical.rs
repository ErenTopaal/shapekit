//! Canonical order for the ledger entry change lists in `LedgerCloseMeta`.
//!
//! Release builds of stellar-core 30.0.0 emit the changes of one `LedgerTxn`
//! in the iteration order of an unordered map whose hash key is random per
//! process (`LedgerTxn::Impl::getChanges`), so the raw meta of two identical
//! runs differs in the order of those lists and nowhere else. The lists this
//! module reorders are the ones core's `normalizeMeta` (src/util/MetaUtils.cpp,
//! v30.0.0) reorders before comparing goldens:
//!
//! - every upgrade's `changes`
//! - every transaction's `feeProcessing`
//! - the transaction meta: `txChanges` (v1), `txChangesBefore` and
//!   `txChangesAfter` (v2 to v4), and every operation's `changes` (v0 to v4)
//!
//! `postTxApplyFeeProcessing`, events, evicted keys and everything else stay
//! in emission order.
//!
//! The rule is not core's, though. `normalizeMeta` sorts by key, then by
//! change type, then by hash, which is only a comparison aid: it can put a
//! `RESTORED` after the `UPDATED` of the same key, an order core never emits
//! and one ingest code that pairs each change with the one before it cannot
//! read. The canonical form here is a stable sort by the ledger key alone.
//! The changes of one key keep core's raw relative order, which is the
//! logical sequence of that key and does not depend on the process; only the
//! order between keys comes from the hash map, so any key order is one core
//! can emit, and the canonical LCM is a byte string core itself can write.
//!
//! The key comparison is xdrpp's generated `operator<=>`: structs field by
//! field in declaration order, unions by discriminant value and then the arm,
//! vectors and opaques lexicographically with length as the tie break,
//! optionals with absent first. `stellar-xdr`'s derived `Ord` is the same
//! relation as long as every union declares its arms in increasing
//! discriminant order, which the tests check for every union a `LedgerKey`
//! can contain. Matching core's key order is not required for correctness,
//! it only keeps the canonical key order the one core's tools would show.

use stellar_xdr::{
    LedgerCloseMeta, LedgerEntryChange, LedgerEntryChanges, LedgerKey, Limits, TransactionMeta,
    WriteXdr,
};

/// The key a change is about (`getKeyFromChange` in MetaUtils.cpp).
pub fn change_key(c: &LedgerEntryChange) -> LedgerKey {
    match c {
        LedgerEntryChange::State(e)
        | LedgerEntryChange::Created(e)
        | LedgerEntryChange::Updated(e)
        | LedgerEntryChange::Restored(e) => e.to_key(),
        LedgerEntryChange::Removed(k) => k.clone(),
    }
}

/// Sorts one change list by ledger key, keeping each key's changes in the
/// order they were given.
pub fn sort_changes(changes: &mut LedgerEntryChanges) -> Result<(), stellar_xdr::Error> {
    if changes.0.len() < 2 {
        return Ok(());
    }
    let mut keyed: Vec<(LedgerKey, LedgerEntryChange)> = std::mem::take(&mut changes.0)
        .to_vec()
        .into_iter()
        .map(|c| (change_key(&c), c))
        .collect();
    // stable, so one key's changes stay in core's order
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    changes.0 = keyed
        .into_iter()
        .map(|(_, c)| c)
        .collect::<Vec<_>>()
        .try_into()?;
    Ok(())
}

/// Canonical form of every change list in a `TransactionMeta`.
pub fn canonicalize_tx_meta(meta: &mut TransactionMeta) -> Result<(), stellar_xdr::Error> {
    match meta {
        TransactionMeta::V0(ops) => {
            for op in ops.iter_mut() {
                sort_changes(&mut op.changes)?;
            }
        }
        TransactionMeta::V1(m) => {
            sort_changes(&mut m.tx_changes)?;
            for op in m.operations.iter_mut() {
                sort_changes(&mut op.changes)?;
            }
        }
        TransactionMeta::V2(m) => {
            sort_changes(&mut m.tx_changes_before)?;
            sort_changes(&mut m.tx_changes_after)?;
            for op in m.operations.iter_mut() {
                sort_changes(&mut op.changes)?;
            }
        }
        TransactionMeta::V3(m) => {
            sort_changes(&mut m.tx_changes_before)?;
            sort_changes(&mut m.tx_changes_after)?;
            for op in m.operations.iter_mut() {
                sort_changes(&mut op.changes)?;
            }
        }
        TransactionMeta::V4(m) => {
            sort_changes(&mut m.tx_changes_before)?;
            sort_changes(&mut m.tx_changes_after)?;
            for op in m.operations.iter_mut() {
                sort_changes(&mut op.changes)?;
            }
        }
    }
    Ok(())
}

/// Canonical form of every change list in a `LedgerCloseMeta`.
pub fn canonicalize_lcm(lcm: &mut LedgerCloseMeta) -> Result<(), stellar_xdr::Error> {
    match lcm {
        LedgerCloseMeta::V0(m) => {
            for u in m.upgrades_processing.iter_mut() {
                sort_changes(&mut u.changes)?;
            }
            for tx in m.tx_processing.iter_mut() {
                sort_changes(&mut tx.fee_processing)?;
                canonicalize_tx_meta(&mut tx.tx_apply_processing)?;
            }
        }
        LedgerCloseMeta::V1(m) => {
            for u in m.upgrades_processing.iter_mut() {
                sort_changes(&mut u.changes)?;
            }
            for tx in m.tx_processing.iter_mut() {
                sort_changes(&mut tx.fee_processing)?;
                canonicalize_tx_meta(&mut tx.tx_apply_processing)?;
            }
        }
        LedgerCloseMeta::V2(m) => {
            for u in m.upgrades_processing.iter_mut() {
                sort_changes(&mut u.changes)?;
            }
            for tx in m.tx_processing.iter_mut() {
                sort_changes(&mut tx.fee_processing)?;
                canonicalize_tx_meta(&mut tx.tx_apply_processing)?;
            }
        }
    }
    Ok(())
}

/// Every list [`canonicalize_lcm`] may reorder, in the order the LCM holds them.
pub fn change_lists(lcm: &LedgerCloseMeta) -> Vec<&LedgerEntryChanges> {
    let mut lists: Vec<&LedgerEntryChanges> = Vec::new();
    fn tx_lists<'a>(meta: &'a TransactionMeta, out: &mut Vec<&'a LedgerEntryChanges>) {
        match meta {
            TransactionMeta::V0(ops) => out.extend(ops.iter().map(|o| &o.changes)),
            TransactionMeta::V1(m) => {
                out.push(&m.tx_changes);
                out.extend(m.operations.iter().map(|o| &o.changes));
            }
            TransactionMeta::V2(m) => {
                out.push(&m.tx_changes_before);
                out.push(&m.tx_changes_after);
                out.extend(m.operations.iter().map(|o| &o.changes));
            }
            TransactionMeta::V3(m) => {
                out.push(&m.tx_changes_before);
                out.push(&m.tx_changes_after);
                out.extend(m.operations.iter().map(|o| &o.changes));
            }
            TransactionMeta::V4(m) => {
                out.push(&m.tx_changes_before);
                out.push(&m.tx_changes_after);
                out.extend(m.operations.iter().map(|o| &o.changes));
            }
        }
    }
    match lcm {
        LedgerCloseMeta::V0(m) => {
            lists.extend(m.upgrades_processing.iter().map(|u| &u.changes));
            for tx in m.tx_processing.iter() {
                lists.push(&tx.fee_processing);
                tx_lists(&tx.tx_apply_processing, &mut lists);
            }
        }
        LedgerCloseMeta::V1(m) => {
            lists.extend(m.upgrades_processing.iter().map(|u| &u.changes));
            for tx in m.tx_processing.iter() {
                lists.push(&tx.fee_processing);
                tx_lists(&tx.tx_apply_processing, &mut lists);
            }
        }
        LedgerCloseMeta::V2(m) => {
            lists.extend(m.upgrades_processing.iter().map(|u| &u.changes));
            for tx in m.tx_processing.iter() {
                lists.push(&tx.fee_processing);
                tx_lists(&tx.tx_apply_processing, &mut lists);
            }
        }
    }
    lists
}

/// Every change of every list [`canonicalize_lcm`] may reorder, each list
/// sorted by the change's XDR bytes. Two LCMs that differ only in the order
/// within those lists give equal results, so comparing this is the multiset
/// check between a raw and a canonical LCM.
pub fn change_multisets(lcm: &LedgerCloseMeta) -> Result<Vec<Vec<Vec<u8>>>, stellar_xdr::Error> {
    let lists = change_lists(lcm);
    let mut out = Vec::with_capacity(lists.len());
    for l in lists {
        let mut items = Vec::with_capacity(l.0.len());
        for c in l.0.iter() {
            items.push(c.to_xdr(Limits::none())?);
        }
        items.sort_unstable();
        out.push(items);
    }
    Ok(out)
}

/// Each list's changes grouped by key, keys in canonical order, each key's
/// changes in the order the list holds them. Equal for two LCMs exactly when
/// every key's changes came in the same order in both.
pub fn per_key_sequences(lcm: &LedgerCloseMeta) -> Vec<Vec<(LedgerKey, Vec<&LedgerEntryChange>)>> {
    change_lists(lcm)
        .into_iter()
        .map(|l| {
            let mut groups: std::collections::BTreeMap<LedgerKey, Vec<&LedgerEntryChange>> =
                std::collections::BTreeMap::new();
            for c in l.0.iter() {
                groups.entry(change_key(c)).or_default().push(c);
            }
            groups.into_iter().collect()
        })
        .collect()
}

/// Whether `a` and `b` differ only in the order between keys within the
/// lists [`canonicalize_lcm`] sorts: the same multiset of changes in every
/// list, and equal once both are canonical, which also needs every key's
/// changes in the same relative order in both.
pub fn permutation_equal(
    a: &LedgerCloseMeta,
    b: &LedgerCloseMeta,
) -> Result<bool, stellar_xdr::Error> {
    if change_multisets(a)? != change_multisets(b)? {
        return Ok(false);
    }
    let (mut ca, mut cb) = (a.clone(), b.clone());
    canonicalize_lcm(&mut ca)?;
    canonicalize_lcm(&mut cb)?;
    Ok(ca == cb)
}

/// What [`compare_raw_order`] found between two raw recordings of one ledger.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RawOrderStats {
    /// Change lists compared.
    pub lists: usize,
    /// Lists holding the same changes in a different order.
    pub lists_reordered: usize,
    /// Keys with two or more changes in one list.
    pub multi_change_keys: usize,
    /// Such keys whose changes came in a different order in the two recordings.
    pub per_key_mismatches: usize,
    /// Lists whose changes differ as a multiset, or ledgers that differ outside the lists:
    /// the recordings are not reorderings of each other there.
    pub content_mismatches: usize,
}

impl RawOrderStats {
    pub fn add(&mut self, o: RawOrderStats) {
        self.lists += o.lists;
        self.lists_reordered += o.lists_reordered;
        self.multi_change_keys += o.multi_change_keys;
        self.per_key_mismatches += o.per_key_mismatches;
        self.content_mismatches += o.content_mismatches;
    }
}

/// Compares two raw recordings of the same ledger. The canonical form rests on core keeping
/// each key's changes in one order across processes and varying only the order between keys;
/// this measures exactly that on real output.
pub fn compare_raw_order(
    a: &LedgerCloseMeta,
    b: &LedgerCloseMeta,
) -> Result<RawOrderStats, stellar_xdr::Error> {
    let mut stats = RawOrderStats::default();
    let (la, lb) = (change_lists(a), change_lists(b));
    if la.len() != lb.len() {
        stats.lists = la.len().max(lb.len());
        stats.content_mismatches = stats.lists;
        return Ok(stats);
    }
    let (ma, mb) = (change_multisets(a)?, change_multisets(b)?);
    let (ka, kb) = (per_key_sequences(a), per_key_sequences(b));
    for i in 0..la.len() {
        stats.lists += 1;
        if ma[i] != mb[i] {
            stats.content_mismatches += 1;
            continue;
        }
        if la[i] != lb[i] {
            stats.lists_reordered += 1;
        }
        // equal multisets give the same keys with the same counts in the same key order
        for ((_, seq_a), (_, seq_b)) in ka[i].iter().zip(kb[i].iter()) {
            if seq_a.len() < 2 {
                continue;
            }
            stats.multi_change_keys += 1;
            if seq_a != seq_b {
                stats.per_key_mismatches += 1;
            }
        }
    }
    if stats.content_mismatches == 0 && stats.per_key_mismatches == 0 {
        let (mut ca, mut cb) = (a.clone(), b.clone());
        canonicalize_lcm(&mut ca)?;
        canonicalize_lcm(&mut cb)?;
        if ca != cb {
            stats.content_mismatches += 1;
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::{
        AccountEntry, AccountEntryExt, AccountId, LedgerEntry, LedgerEntryData, LedgerEntryExt,
        LedgerKeyAccount, PublicKey, SequenceNumber, Thresholds, Uint256,
    };

    // every union a LedgerKey can hold, directly or through ScVal: derived
    // Ord compares arms in declaration order, xdrpp by discriminant value,
    // so the two agree exactly when declaration order is increasing
    #[test]
    fn union_arms_are_declared_in_discriminant_order() {
        fn increasing(name: &str, values: &[i32]) {
            assert!(
                values.windows(2).all(|w| w[0] < w[1]),
                "{name} arms are not in increasing discriminant order: {values:?}"
            );
        }
        macro_rules! check {
            ($($t:ident),* $(,)?) => {
                $(increasing(
                    stringify!($t),
                    &stellar_xdr::$t::VARIANTS.iter().map(|v| *v as i32).collect::<Vec<_>>(),
                );)*
            };
        }
        check!(
            LedgerKey,
            PublicKey,
            TrustLineAsset,
            ClaimableBalanceId,
            ScAddress,
            ScVal,
            ScError,
            ContractExecutable,
            LedgerEntryChange,
        );
        // fieldless enums inside keys compare by value in both, listed for
        // the record that their values are what core uses
        check!(
            LedgerEntryType,
            AssetType,
            ContractDataDurability,
            ConfigSettingId,
            ScValType,
            ScErrorType,
            ScErrorCode,
            ScAddressType,
            ContractExecutableType,
            ClaimableBalanceIdType,
        );
    }

    fn account(n: u8, balance: i64) -> LedgerEntry {
        LedgerEntry {
            last_modified_ledger_seq: 1,
            data: LedgerEntryData::Account(AccountEntry {
                account_id: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([n; 32]))),
                balance,
                seq_num: SequenceNumber(0),
                num_sub_entries: 0,
                inflation_dest: None,
                flags: 0,
                home_domain: Default::default(),
                thresholds: Thresholds([1, 0, 0, 0]),
                signers: Default::default(),
                ext: AccountEntryExt::V0,
            }),
            ext: LedgerEntryExt::V0,
        }
    }

    fn removed(n: u8) -> LedgerEntryChange {
        LedgerEntryChange::Removed(LedgerKey::Account(LedgerKeyAccount {
            account_id: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([n; 32]))),
        }))
    }

    #[test]
    fn orders_by_key_and_keeps_each_keys_order() {
        // the restore stays in front of the update and the removal after its
        // update, where a sort by change type would move both
        let mut c = LedgerEntryChanges(
            vec![
                LedgerEntryChange::Restored(account(2, 4)),
                LedgerEntryChange::State(account(1, 3)),
                LedgerEntryChange::Updated(account(2, 5)),
                LedgerEntryChange::Updated(account(1, 7)),
                removed(1),
            ]
            .try_into()
            .unwrap(),
        );
        sort_changes(&mut c).unwrap();
        assert_eq!(
            c.0.to_vec(),
            vec![
                LedgerEntryChange::State(account(1, 3)),
                LedgerEntryChange::Updated(account(1, 7)),
                removed(1),
                LedgerEntryChange::Restored(account(2, 4)),
                LedgerEntryChange::Updated(account(2, 5)),
            ]
        );
    }

    // splitmix64, so the lists are the same on every run
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    fn random_change(rng: &mut Rng) -> LedgerEntryChange {
        let n = rng.below(4) as u8 + 1;
        let v = rng.below(3) as i64;
        match rng.below(5) {
            0 => LedgerEntryChange::State(account(n, v)),
            1 => LedgerEntryChange::Created(account(n, v)),
            2 => LedgerEntryChange::Updated(account(n, v)),
            3 => removed(n),
            _ => LedgerEntryChange::Restored(account(n, v)),
        }
    }

    fn per_key(list: &[LedgerEntryChange]) -> Vec<(LedgerKey, Vec<LedgerEntryChange>)> {
        let mut out: Vec<(LedgerKey, Vec<LedgerEntryChange>)> = Vec::new();
        for c in list {
            let k = change_key(c);
            match out.iter_mut().find(|(x, _)| *x == k) {
                Some((_, v)) => v.push(c.clone()),
                None => out.push((k, vec![c.clone()])),
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    #[test]
    fn canonical_is_a_permutation_that_keeps_every_keys_order() {
        let mut rng = Rng(0x5eed);
        for _ in 0..2000 {
            let len = rng.below(12) as usize;
            let raw: Vec<_> = (0..len).map(|_| random_change(&mut rng)).collect();
            let mut c = LedgerEntryChanges(raw.clone().try_into().unwrap());
            sort_changes(&mut c).unwrap();
            let got = c.0.to_vec();

            // the same multiset of changes
            let bytes = |l: &[LedgerEntryChange]| {
                let mut v: Vec<_> = l
                    .iter()
                    .map(|c| c.to_xdr(Limits::none()).unwrap())
                    .collect();
                v.sort();
                v
            };
            assert_eq!(bytes(&raw), bytes(&got));
            // keys in order, and each key's changes exactly as raw had them
            let keys: Vec<_> = got.iter().map(change_key).collect();
            assert!(keys.windows(2).all(|w| w[0] <= w[1]), "keys out of order");
            assert_eq!(per_key(&raw), per_key(&got));
            // and sorting again changes nothing
            let mut again = c.clone();
            sort_changes(&mut again).unwrap();
            assert_eq!(again, c);
        }
    }
}
