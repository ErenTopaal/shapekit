//! The lake reader, the watch windows and the random audit over a committed corpus lake.

use std::path::PathBuf;

use shapekit_sentinel::{Lake, Network, Tip, random_audit, ranges_of, watch, window};

/// A chain recorded under the pubnet passphrase, ledgers 2 to 66, one object each.
fn lake() -> Lake {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/chains/cap23.claimable-balances.p30/mainnet/ledgers");
    Lake::dir(dir, Network::Pubnet).unwrap()
}

#[test]
fn reads_the_config_and_the_newest_ledger() {
    let l = lake();
    assert_eq!(l.layout.ledgers_per_batch(), 1);
    assert_eq!(l.layout.batches_per_partition(), 64000);
    assert_eq!(l.newest().unwrap(), 66);
    let err = l.fetch(67).err().unwrap().to_string();
    assert!(err.contains("ledger 67 is not in"), "{err}");
}

#[test]
fn a_lake_of_the_other_network_is_refused() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/chains/cap23.claimable-balances.p30/mainnet/ledgers");
    let err = Lake::dir(dir, Network::Testnet).err().unwrap().to_string();
    assert!(err.contains("not testnet"), "{err}");
}

#[test]
fn two_windows_cover_the_chain_once_with_the_chain_law_across_them() {
    let l = lake();
    let newest = l.newest().unwrap();
    let w1 = window(None, Some(2), newest, 30).unwrap().unwrap();
    assert_eq!((w1.first, w1.last), (2, 31));
    let a1 = watch(&l, None, w1, 3).unwrap();
    assert_eq!(a1.ledgers(), (2..=31).collect::<Vec<_>>());
    let tip = a1.tip().unwrap().clone();
    assert_eq!(tip.ledger, 31);

    let w2 = window(Some(&tip), None, newest, 1000).unwrap().unwrap();
    assert_eq!((w2.first, w2.last), (32, 66));
    let a2 = watch(&l, Some(tip.clone()), w2, 3).unwrap();
    assert_eq!(a2.ledgers(), (32..=66).collect::<Vec<_>>());
    assert!(window(a2.tip(), None, newest, 1000).unwrap().is_none());

    let r1 = a1.report(None, None);
    let r2 = a2.report(None, None);
    assert_eq!(r1.total_violations() + r2.total_violations(), 0);
    let chain = |r: &shapekit_contracts::AuditReport| {
        r.laws
            .iter()
            .find(|l| l.law == "header-chain")
            .unwrap()
            .checked
    };
    // 29 links inside the first window, 1 across, 34 inside the second
    assert_eq!(chain(&r1), 29);
    assert_eq!(chain(&r2), 35);
    let balance = r1
        .laws
        .iter()
        .chain(&r2.laws)
        .filter(|l| l.law == "balance-id");
    assert!(balance.map(|l| l.checked).sum::<u64>() > 0);

    // a cursor whose hash is not the ledger's fails the chain law on the next ledger
    let forged = Tip {
        ledger: 31,
        hash: "00".repeat(32),
    };
    let bad = watch(&l, Some(forged), w2, 3).unwrap().report(None, None);
    let law = bad.laws.iter().find(|l| l.law == "header-chain").unwrap();
    assert_eq!((law.checked, law.violations), (35, 1));
    assert_eq!(law.examples[0].ledger, 32);
}

#[test]
fn random_audit_checks_each_drawn_ledger_once() {
    let l = lake();
    let a = random_audit(&l, 20, 7, 67, 4).unwrap();
    let r = a.report(Some(7), None);
    assert_eq!(r.ledgers.len(), 20);
    assert!(r.ledgers.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(ranges_of(&l, &r.ledgers).len(), 20);
    assert_eq!(r.total_violations(), 0);
    assert_eq!(r.protocols, vec![30]);
    assert!(
        <shapekit_contracts::AuditReport as shapekit_contracts::Contract>::problems(&r).is_empty()
    );
    // the same seed draws the same ledgers
    let again = random_audit(&l, 20, 7, 67, 1)
        .unwrap()
        .report(Some(7), None);
    assert_eq!(again, r);
}
