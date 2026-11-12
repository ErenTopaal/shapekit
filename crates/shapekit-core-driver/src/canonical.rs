//! Canonical order for the ledger entry change lists in `LedgerCloseMeta`.
//!
//! Release builds of stellar-core 30.0.0 emit the changes of one `LedgerTxn`
//! in the iteration order of an unordered map whose hash key is random per
//! process (`LedgerTxn::Impl::getChanges`), so the raw meta of two identical
//! runs differs in the order of those lists and nowhere else. Core itself
//! compares meta only after `normalizeMeta` (src/util/MetaUtils.cpp, v30.0.0),
//! and this module applies exactly that rule to exactly the lists it sorts:
//!
//! - every upgrade's `changes`
//! - every transaction's `feeProcessing`
//! - the transaction meta: `txChanges` (v1), `txChangesBefore` and
//!   `txChangesAfter` (v2 to v4), and every operation's `changes` (v0 to v4)
//!
//! Core leaves `postTxApplyFeeProcessing`, events, evicted keys and
//! everything else in emission order, and so does this module. Core's golden
//! comparison also zeroes the data of `core_metrics`/`invoke_time_nsecs`
//! diagnostic events (`zeroNonDeterministicDiagnostics`); that is about test
//! timing, changes values rather than order, and is not done here.
//!
//! The order is the one `CmpLedgerEntryChanges` defines: by the ledger key the
//! change is about, then by change type ranked state, created, updated,
//! removed, restored, then by SHA-256 of the change's XDR. The key comparison
//! is xdrpp's generated `operator<=>`: structs field by field in declaration
//! order, unions by discriminant value and then the arm, vectors and opaques
//! lexicographically with length as the tie break, optionals with absent
//! first. `stellar-xdr`'s derived `Ord` is the same relation as long as every
//! union declares its arms in increasing discriminant order, which the tests
//! check for every union a `LedgerKey` can contain.
//!
//! The result only permutes lists, and since core's own order is unspecified
//! per process, a canonical LCM is a byte string core itself can emit.

use sha2::{Digest, Sha256};
use stellar_xdr::{
    LedgerCloseMeta, LedgerEntryChange, LedgerEntryChanges, LedgerKey, Limits, TransactionMeta,
    WriteXdr,
};

/// Position of a change type in core's order (`remap` in MetaUtils.cpp):
/// state, created, updated, removed, restored.
fn type_rank(c: &LedgerEntryChange) -> u8 {
    match c {
        LedgerEntryChange::State(_) => 0,
        LedgerEntryChange::Created(_) => 1,
        LedgerEntryChange::Updated(_) => 2,
        LedgerEntryChange::Removed(_) => 3,
        LedgerEntryChange::Restored(_) => 4,
    }
}

/// The key a change is about (`getKeyFromChange` in MetaUtils.cpp).
fn change_key(c: &LedgerEntryChange) -> LedgerKey {
    match c {
        LedgerEntryChange::State(e)
        | LedgerEntryChange::Created(e)
        | LedgerEntryChange::Updated(e)
        | LedgerEntryChange::Restored(e) => e.to_key(),
        LedgerEntryChange::Removed(k) => k.clone(),
    }
}

/// Sorts one change list into core's normalized order.
pub fn sort_changes(changes: &mut LedgerEntryChanges) -> Result<(), stellar_xdr::Error> {
    if changes.0.len() < 2 {
        return Ok(());
    }
    let mut keyed = Vec::with_capacity(changes.0.len());
    for c in std::mem::take(&mut changes.0).to_vec() {
        let digest: [u8; 32] = Sha256::digest(c.to_xdr(Limits::none())?).into();
        keyed.push(((change_key(&c), type_rank(&c), digest), c));
    }
    // the digest makes the key total up to byte-equal changes, so an
    // unstable sort cannot produce two different byte strings
    keyed.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    changes.0 = keyed
        .into_iter()
        .map(|(_, c)| c)
        .collect::<Vec<_>>()
        .try_into()?;
    Ok(())
}

/// `normalizeMeta(TransactionMeta&)`.
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

/// `normalizeMeta(LedgerCloseMeta&)`.
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

/// Every change of every list [`canonicalize_lcm`] may reorder, each list
/// sorted by the change's XDR bytes. Two LCMs that differ only in the order
/// within those lists give equal results, so comparing this is the multiset
/// check between a raw and a canonical LCM.
pub fn change_multisets(lcm: &LedgerCloseMeta) -> Result<Vec<Vec<Vec<u8>>>, stellar_xdr::Error> {
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

/// Whether `b` is `a` with only the lists [`canonicalize_lcm`] sorts
/// permuted: equal once both are canonical, and holding the same multiset
/// of changes in every list.
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

    #[test]
    fn orders_by_key_then_type_then_hash() {
        let removed = LedgerEntryChange::Removed(LedgerKey::Account(LedgerKeyAccount {
            account_id: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([1; 32]))),
        }));
        let mut c = LedgerEntryChanges(
            vec![
                LedgerEntryChange::Updated(account(2, 5)),
                LedgerEntryChange::Created(account(1, 7)),
                removed.clone(),
                LedgerEntryChange::State(account(2, 9)),
                LedgerEntryChange::State(account(1, 3)),
            ]
            .try_into()
            .unwrap(),
        );
        sort_changes(&mut c).unwrap();
        let got: Vec<_> = c.0.to_vec();
        assert_eq!(
            got,
            vec![
                LedgerEntryChange::State(account(1, 3)),
                LedgerEntryChange::Created(account(1, 7)),
                removed,
                LedgerEntryChange::State(account(2, 9)),
                LedgerEntryChange::Updated(account(2, 5)),
            ]
        );
    }
}
