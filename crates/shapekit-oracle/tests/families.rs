//! Known answers per corpus family: the oracle's facts for the committed chains equal what
//! the recipe in `crates/shapekit-gen/src/recipe/` did. Each expectation below is read from
//! the recipe (its constants, roles and the operations it submits), never from an oracle
//! file, and every test runs both network variants.

mod common;

use common::*;
use sha2::{Digest, Sha256};
use shapekit_oracle::*;

/// cap73: SAC `trust(holder)` creates the holder's trustline with limit i64::MAX and emits
/// no event at operation scope; the second call changes nothing.
#[test]
fn cap73_sac_trust_creates_max_trustline_without_event() {
    let fam = "cap73.sac-trust";
    let holder = account(fam, "holder");
    let issuer = account(fam, "issuer");
    for p in [28, 30] {
        for n in NETWORKS {
            let tx = entry_tx(fam, &format!("{fam}.p{p}-trust-meta"), n);
            assert!(tx.successful);
            let ops = op_changes(&tx);
            let lines: Vec<_> = ops
                .iter()
                .filter_map(|c| match c {
                    Change::TrustlineCreated {
                        account,
                        asset,
                        limit,
                        ..
                    } => Some((account.clone(), asset.clone(), limit.0)),
                    _ => None,
                })
                .collect();
            assert_eq!(
                lines,
                vec![(holder.clone(), format!("TRST:{issuer}"), i64::MAX as i128)],
                "p{p} {n}"
            );
            let op_events: Vec<_> = tx
                .changes
                .iter()
                .filter(|c| c.scope == ChangeScope::Operation)
                .filter(|c| {
                    matches!(
                        c.change,
                        Change::TokenEvent(_) | Change::ContractEvent { .. }
                    )
                })
                .collect();
            assert!(op_events.is_empty(), "p{p} {n}: {op_events:?}");

            let again = only_tx(fam, &format!("{fam}.p{p}-trust-again"), n);
            assert!(again.successful);
            assert!(
                !op_changes(&again).iter().any(|c| matches!(
                    c,
                    Change::TrustlineCreated { .. } | Change::TrustlineLimit { .. }
                )),
                "p{p} {n}: second trust touched the trustline"
            );

            // the xlm sac transfer creates the newcomer with 100 XLM
            let t = entry_tx(
                fam,
                &format!("{fam}.p{p}-xlm-transfer-creates-account-meta"),
                n,
            );
            let created: Vec<_> = op_changes(&t)
                .into_iter()
                .filter_map(|c| match c {
                    Change::AccountCreated {
                        account,
                        starting_balance,
                    } => Some((account.clone(), starting_balance.0)),
                    _ => None,
                })
                .collect();
            assert_eq!(
                created,
                vec![(account(fam, "newcomer"), 1_000_000_000)],
                "p{p} {n}"
            );
        }
    }
}

fn cap40_payload(use_: &str, n: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut block = 0u8;
    while out.len() < n {
        let mut h = Sha256::new();
        h.update(b"shapekit/cap40/payload/v1");
        h.update(use_.as_bytes());
        h.update((n as u32).to_be_bytes());
        h.update([block]);
        out.extend_from_slice(&h.finalize());
        block += 1;
    }
    out.truncate(n);
    out
}

fn signed_payload_strkey(recipe: &str, role: &str, payload: Vec<u8>) -> String {
    let key = shapekit_core_driver::keys::derive_key(recipe, role);
    let sp = stellar_strkey::ed25519::SignedPayload::new(key.verifying_key().to_bytes(), &payload)
        .expect("1 to 64 payload bytes");
    format!("{sp}")
}

/// cap40: each SetOptions adds a signed-payload signer, reported as its full `P...` strkey
/// with the payload the recipe built; removing the 1 byte signer reports the same strkey.
#[test]
fn cap40_signed_payload_signers_are_full_p_strkeys() {
    let fam = "cap40.signed-payload-signer";
    let alice = account(fam, "alice");
    for p in [28, 30] {
        for n in NETWORKS {
            for len in [1usize, 4, 32] {
                let want =
                    signed_payload_strkey(fam, "payload-signer", cap40_payload("account", len));
                assert!(want.starts_with('P'));
                let tx = entry_tx(
                    fam,
                    &format!("{fam}.p{p}-set-options-payload-{len}-meta"),
                    n,
                );
                let added: Vec<_> = op_changes(&tx)
                    .into_iter()
                    .filter_map(|c| match c {
                        Change::SignerAdded {
                            account,
                            signer,
                            weight,
                        } => Some((account.clone(), signer.clone(), *weight)),
                        _ => None,
                    })
                    .collect();
                assert_eq!(added, vec![(alice.clone(), want, 1)], "p{p} {n} {len}");
            }
            let p64 = signed_payload_strkey(fam, "payload-signer", cap40_payload("account", 64));
            let tx = entry_tx(
                fam,
                &format!("{fam}.p{p}-sponsored-set-options-payload-64-meta"),
                n,
            );
            let added = |c: &&Change| {
                matches!(c, Change::SignerAdded { account, signer, .. }
                    if *account == alice && *signer == p64)
            };
            assert!(
                op_changes(&tx).iter().any(added),
                "p{p} {n}: 64 byte signer"
            );
            assert!(
                op_changes(&tx).iter().any(|c| matches!(c,
                    Change::Sponsorship { signer: Some(s), sponsor_after: Some(sp), .. }
                        if *s == p64 && *sp == account(fam, "sponsor"))),
                "p{p} {n}: the sponsor sponsors the 64 byte signer"
            );
            let p1 = signed_payload_strkey(fam, "payload-signer", cap40_payload("account", 1));
            let tx = entry_tx(fam, &format!("{fam}.p{p}-remove-signer-payload-1-meta"), n);
            let removed = |c: &&Change| {
                matches!(c, Change::SignerRemoved { account, signer, .. }
                    if *account == alice && *signer == p1)
            };
            assert!(
                op_changes(&tx).iter().any(removed),
                "p{p} {n}: removed 1 byte signer"
            );
        }
    }
}

/// soroban.muxed-source: an extend-TTL and a restore whose source is a muxed account name
/// the base account as fee source and participant, with no mux id (participant mux ids only
/// come from muxed addresses in event topics) and no `M...` participant.
#[test]
fn muxed_soroban_source_yields_the_base_account() {
    let fam = "soroban.muxed-source";
    let alice = account(fam, "alice");
    let cases = [
        (28, "restore-muxed-source"),
        (28, "muxed-and-base-source-one-ledger"),
        (30, "restore-muxed-source"),
        (30, "extend-ttl-muxed-source"),
        (30, "extend-ttl-noop-muxed-source"),
    ];
    for (p, slug) in cases {
        for n in NETWORKS {
            let l = entry_ledger(fam, &format!("{fam}.p{p}-{slug}"), n);
            let tx = l
                .transactions
                .iter()
                .find(|t| t.declared.source.starts_with('M'))
                .unwrap_or_else(|| panic!("p{p} {slug} {n}: the muxed-source transaction"));
            assert!(tx.successful, "p{p} {slug} {n}");
            assert_eq!(tx.fee.source, alice, "p{p} {slug} {n}");
            let a = participant(tx, &alice).unwrap_or_else(|| panic!("p{p} {slug} {n}: alice"));
            assert!(a.mux_ids.is_empty(), "p{p} {slug} {n}: {a:?}");
            assert!(a.roles.iter().any(|r| r == "fee-source"));
            for t in &l.transactions {
                assert!(
                    t.participants.iter().all(|p| !p.address.starts_with('M')),
                    "p{p} {slug} {n}: {:?}",
                    t.participants
                );
            }
            // the envelope does name the muxed source, kept apart in declared
            assert!(tx.declared.source.starts_with('M'), "p{p} {slug} {n}");
        }
    }
    // wallet-backend#753's ledger: the second transaction is bob's payment whose operation
    // source is alice's base account, so both transactions have alice as a participant
    for n in NETWORKS {
        let l = entry_ledger(
            fam,
            &format!("{fam}.p28-muxed-and-base-source-one-ledger"),
            n,
        );
        assert_eq!(l.transactions.len(), 2);
        let pay = l
            .transactions
            .iter()
            .find(|t| t.fee.source == account(fam, "bob"))
            .expect("bob's payment");
        assert!(participant(pay, &alice).is_some());
        assert!(native_op_delta(pay, &alice) < 0);
    }
}

/// soroban.auth-trees: creations declared in auth entries that never ran give no
/// contract-deployed, whether the call failed, succeeded without consuming the entries, or
/// declared a claimable balance as deployer.
#[test]
fn unmatched_auth_creates_no_contract() {
    let fam = "soroban.auth-trees";
    let cases = [
        ("unmatched-create-failed-tx", false),
        ("unmatched-create-success", true),
        ("unmatched-create-claimable-balance-deployer", true),
    ];
    for (slug, ok) in cases {
        for n in NETWORKS {
            let tx = only_tx(fam, &format!("{fam}.p30-{slug}"), n);
            assert_eq!(tx.successful, ok, "{slug} {n}");
            let deployed: Vec<_> = tx
                .changes
                .iter()
                .filter(|c| matches!(c.change, Change::ContractDeployed { .. }))
                .collect();
            assert!(deployed.is_empty(), "{slug} {n}: {deployed:?}");
            assert!(
                !op_changes(&tx).iter().any(|c| matches!(
                    c,
                    Change::ContractCode {
                        action: Action::Created,
                        ..
                    }
                )),
                "{slug} {n}"
            );
        }
    }
}

/// classic.failed-tx: a failed transaction changes only the fee, the sequence number and
/// (for the pre-authorised one) the pre-auth signer, never at operation scope, and none of
/// the accounts only the envelope names is a participant.
#[test]
fn failed_transactions_have_fee_effects_only() {
    let fam = "classic.failed-tx";
    let owner = account(fam, "owner");
    for n in NETWORKS {
        let tx = only_tx(fam, &format!("{fam}.p30-unknown-destination"), n);
        assert!(!tx.successful);
        assert_eq!(tx.fee.source, owner);
        assert!(effect_kinds(&tx).is_empty(), "{n}: {:?}", effect_kinds(&tx));
        assert_eq!(tx.fee.balance_delta.0, -tx.fee.charged.0);
        for ghost in ["cosigner", "ghost-created", "ghost-destination"] {
            assert!(
                participant(&tx, &account(fam, ghost)).is_none(),
                "{n} {ghost}"
            );
            assert!(
                tx.declared
                    .participants
                    .iter()
                    .any(|p| p.address == account(fam, ghost)),
                "{n}: {ghost} is declared"
            );
        }

        let tx = only_tx(fam, &format!("{fam}.p30-pre-auth-failed-removes-signer"), n);
        assert!(!tx.successful);
        assert!(op_changes(&tx).is_empty(), "{n}: {:?}", op_changes(&tx));
        let removed: Vec<_> = tx
            .changes
            .iter()
            .filter(|c| {
                matches!(&c.change, Change::SignerRemoved { account, signer, .. }
                if *account == owner && signer.starts_with('T'))
            })
            .collect();
        assert_eq!(removed.len(), 1, "{n}: the pre-auth signer goes");
        assert!(participant(&tx, &account(fam, "ghost-preauth")).is_none());
    }
}

fn to_muxed(tx: &TxFacts) -> Vec<(String, Option<ToMuxedId>)> {
    token_events(tx)
        .into_iter()
        .filter(|t| t.event != "fee")
        .map(|t| (t.event.clone(), t.to_muxed_id.clone()))
        .collect()
}

/// sep41.muxed-addresses: SAC transfers to a muxed account carry `to_muxed_id` typed u64
/// with the recipe's mux id, the balance lands on the base account, and the mux id never
/// becomes a participant mux id (CAP-67 topics hold the base account).
#[test]
fn sep41_to_muxed_id_is_typed_u64() {
    let fam = "sep41.muxed-addresses";
    let holder = account(fam, "holder");
    let cases = [
        ("xlm-transfer-to-muxed", "transfer", 1_234_567_890_123u64),
        ("credit-transfer-to-muxed", "transfer", u64::MAX),
        ("credit-mint-to-muxed", "mint", 0),
    ];
    for p in [28, 30] {
        for n in NETWORKS {
            for (slug, event, id) in cases {
                let tx = only_tx(fam, &format!("{fam}.p{p}-{slug}"), n);
                assert!(tx.successful);
                assert_eq!(
                    to_muxed(&tx),
                    vec![(event.to_string(), Some(ToMuxedId::U64(id.into())))],
                    "p{p} {n} {slug}"
                );
                let ev = token_events(&tx)
                    .into_iter()
                    .find(|t| t.event == event)
                    .unwrap();
                assert_eq!(ev.to.as_deref(), Some(holder.as_str()));
                assert_eq!(ev.amount, Some(Int(3_000_000)));
                let h = participant(&tx, &holder).expect("holder");
                assert!(h.mux_ids.is_empty(), "p{p} {n} {slug}: {h:?}");
            }
        }
    }
}

/// cap84.muxed-contract: SAC transfers to a muxed contract name the plain contract and
/// carry `to_muxed_id` typed u64 (2^64-1, then 0 for the issuer's mint); the refused mint
/// emits no token event besides the fee.
#[test]
fn cap84_to_muxed_id_is_typed_u64() {
    let fam = "cap84.muxed-contract";
    let issuer = account(fam, "issuer");
    for n in NETWORKS {
        let tx = only_tx(fam, &format!("{fam}.p30-xlm-transfer-to-muxed-contract"), n);
        assert_eq!(
            to_muxed(&tx),
            vec![("transfer".into(), Some(ToMuxedId::U64(u64::MAX.into())))]
        );
        let ev = token_events(&tx)
            .into_iter()
            .find(|t| t.event == "transfer")
            .unwrap();
        assert!(
            ev.to.as_deref().is_some_and(|t| t.starts_with('C')),
            "{ev:?}"
        );
        assert_eq!(ev.asset.as_deref(), Some("native"));
        assert_eq!(ev.amount, Some(Int(250_000_000)));
        assert!(tx.participants.iter().all(|p| p.mux_ids.is_empty()));

        let tx = only_tx(
            fam,
            &format!("{fam}.p30-issuer-transfer-to-muxed-contract"),
            n,
        );
        assert_eq!(
            to_muxed(&tx),
            vec![("mint".into(), Some(ToMuxedId::U64(0u64.into())))]
        );
        let ev = token_events(&tx)
            .into_iter()
            .find(|t| t.event == "mint")
            .unwrap();
        assert_eq!(ev.asset, Some(format!("MUXC:{issuer}")));
        assert_eq!(ev.amount, Some(Int(10_000_000_000)));

        let tx = only_tx(fam, &format!("{fam}.p30-mint-to-muxed-contract-refused"), n);
        assert!(!tx.successful);
        assert!(to_muxed(&tx).is_empty());
        assert!(effect_kinds(&tx).is_empty(), "{n}: {:?}", effect_kinds(&tx));
    }
}

/// Every committed chain applies from core's genesis state with every change checked and
/// conserves lumens ledger by ledger (the work `common::chain` does), for both networks.
#[test]
fn every_chain_applies_from_genesis_and_conserves() {
    let mut n_ledgers = 0;
    for id in chain_ids() {
        for n in NETWORKS {
            n_ledgers += chain(&id, n).ledgers.len();
        }
    }
    assert!(n_ledgers > 0);
}
