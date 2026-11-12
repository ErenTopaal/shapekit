//! The oracle against real meta: core v30.0.0 goldens and testnet ledgers.
//!
//! Known answers are derived from the C++ test that produced each golden (stellar-core
//! v30.0.0, `src/transactions/test/`), never from the oracle's own output. Every account in
//! a core test is `txtest::getAccount(name)`: the ed25519 key whose seed is the name padded
//! with '.' to 32 bytes (src/test/TxTests.cpp); the root account's seed is the network id.

use std::path::{Path, PathBuf};

use shapekit_oracle::conservation::check_ledger;
use shapekit_oracle::network::CORE_TEST_PASSPHRASE;
use shapekit_oracle::*;
use stellar_xdr::{
    AccountId, AlphaNum4, Asset, AssetCode4, Hash, HashIdPreimage, HashIdPreimageOperationId,
    LedgerCloseMeta, LedgerEntryChange, LedgerEntryData, LedgerKey, LedgerKeyTtl, Limits,
    OperationBody, PublicKey, SequenceNumber, SignerKey, TransactionEnvelope, Uint256, WriteXdr,
};

const TESTNET: &str = "Test SDF Network ; September 2015";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn golden(rel: &str) -> PathBuf {
    fixtures().join("core-goldens").join(rel)
}

fn read_stream(path: &Path) -> Vec<LedgerCloseMeta> {
    LcmStream::open(path)
        .unwrap()
        .map(|r| r.unwrap().meta)
        .collect()
}

fn all_goldens() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in std::fs::read_dir(fixtures().join("core-goldens")).unwrap() {
        for f in std::fs::read_dir(dir.unwrap().path()).unwrap() {
            out.push(f.unwrap().path());
        }
    }
    out.sort();
    out
}

fn testnet_ledgers() -> Vec<LedgerCloseMeta> {
    let manifest_fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../shapekit-manifest/tests/fixtures/testnet-5694308.lcm.xdr");
    [manifest_fixture, fixtures().join("testnet-4437357.lcm.xdr")]
        .iter()
        .map(|p| decode_lcm(&std::fs::read(p).unwrap()).unwrap())
        .collect()
}

fn ed25519_g(seed: &[u8; 32]) -> String {
    let pk = ed25519_dalek::SigningKey::from_bytes(seed)
        .verifying_key()
        .to_bytes();
    AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(pk))).to_string()
}

// txtest::getAccount
fn test_account(name: &str) -> String {
    let mut seed = [b'.'; 32];
    seed[..name.len()].copy_from_slice(name.as_bytes());
    ed25519_g(&seed)
}

fn core_net() -> Network {
    Network::new(CORE_TEST_PASSPHRASE)
}

fn root() -> String {
    core_net().root_account().to_string()
}

fn facts(path: &Path) -> Vec<LedgerFacts> {
    let net = core_net();
    read_stream(path)
        .iter()
        .map(|l| interpret_ledger(l, &net).unwrap())
        .collect()
}

fn txs(path: &Path) -> Vec<TxFacts> {
    facts(path)
        .into_iter()
        .flat_map(|l| l.transactions)
        .collect()
}

fn kinds(changes: &[StateChange]) -> Vec<&'static str> {
    changes
        .iter()
        .map(|c| match &c.change {
            Change::BalanceChange { .. } => "balance-change",
            Change::Sequence { .. } => "sequence",
            Change::Sponsorship { .. } => "sponsorship",
            Change::ClaimableBalanceCreated { .. } => "claimable-balance-created",
            Change::ClaimableBalanceClaimed { .. } => "claimable-balance-claimed",
            Change::AccountCreated { .. } => "account-created",
            Change::TokenEvent(_) => "token-event",
            _ => "other",
        })
        .collect()
}

fn balance(c: &StateChange) -> Option<(&str, &str, &str, i128)> {
    match &c.change {
        Change::BalanceChange {
            holder,
            asset,
            held_in,
            delta,
            ..
        } => Some((holder.as_str(), asset.as_str(), held_in.as_str(), delta.0)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------
// state consistency and conservation over every fixture ledger

#[test]
fn goldens_apply_from_test_genesis_with_every_change_checked() {
    let net = core_net();
    let mut total_checked = 0;
    for path in all_goldens() {
        let mut state = LedgerState::core_test_genesis(&net);
        for lcm in read_stream(&path) {
            let prev = state.header().cloned();
            let report = state
                .apply_ledger(&lcm, &net)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            total_checked += report.checked;
            // only config settings, written at test genesis without meta, may go unchecked
            if report.unchecked > 0 {
                let view = shapekit_oracle::view::LedgerView::new(&lcm, &net).unwrap();
                assert!(
                    !view.upgrades.is_empty() || view.txs.iter().any(|t| touches_config(t)),
                    "{}: ledger {} has unchecked changes outside config settings",
                    path.display(),
                    report.ledger_seq
                );
            }
            check_ledger(&lcm, &net, prev.as_ref())
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        }
    }
    assert!(
        total_checked > 1_000,
        "only {total_checked} changes checked"
    );
}

fn touches_config(t: &shapekit_oracle::view::TxView<'_>) -> bool {
    t.all_lists().iter().any(|l| {
        l.changes.iter().any(|c| match c {
            LedgerEntryChange::Removed(k) => matches!(k, LedgerKey::ConfigSetting(_)),
            LedgerEntryChange::Created(e)
            | LedgerEntryChange::Updated(e)
            | LedgerEntryChange::State(e)
            | LedgerEntryChange::Restored(e) => {
                matches!(e.data, LedgerEntryData::ConfigSetting(_))
            }
        })
    })
}

#[test]
fn genesis_root_is_the_account_every_golden_funds_from() {
    // the first transaction of the bump sequence golden is root paying itself: its fee
    // processing STATE must equal the genesis entry exactly, or a Complete-mode apply fails
    let net = core_net();
    let mut state = LedgerState::genesis(&net);
    for lcm in read_stream(&golden("BumpSequenceTests/27d2533d64835282.xdr")) {
        state.apply_ledger(&lcm, &net).unwrap();
    }
    assert_eq!(state.head_seq(), Some(3));
}

#[test]
fn strict_genesis_rejects_meta_that_needs_unseen_config() {
    // InvokeHostFunctionTests run with soroban config settings created at test genesis
    // without meta, so a plain genesis state must refuse them rather than guess
    let net = core_net();
    let mut state = LedgerState::genesis(&net);
    let err = read_stream(&golden("InvokeHostFunctionTests/4c437318f87437c5.xdr"))
        .iter()
        .find_map(|l| state.apply_ledger(l, &net).err())
        .expect("a config setting STATE without a created entry");
    assert!(err.to_string().contains("config setting"), "{err}");
}

#[test]
fn state_rejects_tampered_and_out_of_order_meta() {
    let net = core_net();
    let ledgers = read_stream(&golden("TxEnvelopeTests/8c2395b250372f24.xdr"));
    let mut state = LedgerState::core_test_genesis(&net);
    state.apply_ledger(&ledgers[0], &net).unwrap();
    let again = state.apply_ledger(&ledgers[0], &net).unwrap_err();
    assert!(matches!(again, StateError::Sequence { .. }), "{again}");

    // a STATE that disagrees with what the state holds must be caught
    let mut tampered = ledgers[1].clone();
    let LedgerCloseMeta::V2(m) = &mut tampered else {
        panic!("golden is meta v2")
    };
    let mut records = m.tx_processing.to_vec();
    let mut fee: Vec<LedgerEntryChange> = records[0].fee_processing.to_vec();
    for c in &mut fee {
        if let LedgerEntryChange::State(e) = c
            && let LedgerEntryData::Account(a) = &mut e.data
        {
            a.balance += 1;
        }
    }
    records[0].fee_processing = fee.try_into().unwrap();
    m.tx_processing = records.try_into().unwrap();
    let err = state.apply_ledger(&tampered, &net).unwrap_err();
    assert!(err.to_string().contains("STATE differs"), "{err}");
}

#[test]
fn conservation_catches_a_wrong_fee() {
    let net = core_net();
    let ledgers = read_stream(&golden("TxEnvelopeTests/8c2395b250372f24.xdr"));
    let mut tampered = ledgers[1].clone();
    let LedgerCloseMeta::V2(m) = &mut tampered else {
        panic!("golden is meta v2")
    };
    let mut records = m.tx_processing.to_vec();
    records[0].result.result.fee_charged += 1;
    m.tx_processing = records.try_into().unwrap();
    // the transaction hash is still found, the books no longer balance
    let err = check_ledger(&tampered, &net, None).unwrap_err();
    assert!(
        matches!(err, conservation::ConservationError::Tx { .. }),
        "{err}"
    );
}

#[test]
fn testnet_ledgers_conserve_lumens_and_fees_match_balances() {
    let net = Network::new(TESTNET);
    for lcm in testnet_ledgers() {
        let report = check_ledger(&lcm, &net, None).unwrap();
        assert!(report.transactions_checked > 0);
        LedgerState::partial().apply_ledger(&lcm, &net).unwrap();
        for t in interpret(&lcm, &net).unwrap() {
            // fee processing takes the full fee, post-apply refunds the rest, and feeCharged
            // is the net (TransactionFrame::processFeeSeqNum, refundSorobanFee)
            assert_eq!(t.fee.balance_delta.0, -t.fee.charged.0, "{}", t.hash);
        }
    }
}

#[test]
fn parallel_read_only_ttl_bumps_merge_with_max() {
    // "parallel txs|multi RO extensions with a single RW extension in a single stage and
    // cluster": five transactions of ledger 41 bump the same TTL (to 94, 360, 164, 546 and
    // 100). Each meta shows the TTL as of the stage start; CAP-63 merges the bumps with
    // max, so the ledger leaves 546.
    let net = core_net();
    let mut state = LedgerState::core_test_genesis(&net);
    for lcm in read_stream(&golden("InvokeHostFunctionTests/13c1727d3159fbb9.xdr")) {
        let seq = shapekit_oracle::view::ledger_seq(&lcm);
        state.apply_ledger(&lcm, &net).unwrap();
        if seq == 41 {
            break;
        }
    }
    let key_hash: [u8; 32] =
        hex::decode("0bd1731ccea53994c57ce69659155de4db7b69fa2ad60d211ddd746807987af7")
            .unwrap()
            .try_into()
            .unwrap();
    let key = LedgerKey::Ttl(LedgerKeyTtl {
        key_hash: Hash(key_hash),
    });
    let LedgerEntryData::Ttl(ttl) = &state.get(&key).unwrap().data else {
        panic!("not a ttl")
    };
    assert_eq!(ttl.live_until_ledger_seq, 546);
}

// ---------------------------------------------------------------------------------------
// known answers from core's C++ tests

#[test]
fn kat_bump_sequence_root_self_payments() {
    // BumpSequenceTests.cpp "minSeqAge under sub-second ledgers", section "whole-second
    // ledger one second later": root pays itself 1 stroop (ledger 2), then a payment with
    // minSeqAge 1 from root to root (ledger 3, transactionWithV2Precondition). Base fee
    // is 100 (Config TESTING_UPGRADE_DESIRED_FEE defaults to the genesis base fee), root
    // starts at sequence 0 with all 10^18 stroops, and a self payment moves nothing.
    let root = root();
    let t = txs(&golden("BumpSequenceTests/27d2533d64835282.xdr"));
    assert_eq!(t.len(), 2);
    let mut balance_before: i128 = 1_000_000_000_000_000_000;
    for (i, tx) in t.iter().enumerate() {
        assert!(tx.successful);
        assert_eq!(tx.fee.source, root);
        assert_eq!(tx.fee.charged.0, 100);
        assert_eq!(kinds(&tx.changes), ["balance-change", "sequence"]);
        assert_eq!(
            tx.changes[0].change,
            Change::BalanceChange {
                holder: root.clone(),
                asset: "native".into(),
                held_in: "account".into(),
                before: Int(balance_before),
                after: Int(balance_before - 100),
                delta: Int(-100),
            }
        );
        assert_eq!(tx.changes[0].scope, ChangeScope::FeeProcessing);
        assert_eq!(
            tx.changes[1].change,
            Change::Sequence {
                account: root.clone(),
                before: Int(i as i128),
                after: Int(i as i128 + 1),
            }
        );
        balance_before -= 100;
        let p: Vec<&str> = tx.participants.iter().map(|p| p.address.as_str()).collect();
        assert_eq!(p, [root.as_str()]);
        assert_eq!(tx.participants[0].roles, ["entry-changed", "fee-source"]);
        assert_eq!(
            tx.declared.participants[0].roles,
            ["destination", "tx-source"]
        );
    }
}

fn balance_id_of(source: &str, seq: i64, op: u32) -> String {
    let source: AccountId = source.parse().unwrap();
    let pre = HashIdPreimage::OpId(HashIdPreimageOperationId {
        source_account: source,
        seq_num: SequenceNumber(seq),
        op_num: op,
    });
    let h: [u8; 32] = sha2_digest(&pre.to_xdr(Limits::none()).unwrap());
    stellar_xdr::ClaimableBalanceId::ClaimableBalanceIdTypeV0(Hash(h)).to_string()
}

fn sha2_digest(b: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    sha2::Sha256::digest(b).into()
}

#[test]
fn kat_claimable_balance_create_and_claim() {
    // ClaimableBalanceTests.cpp "claimable balance absBefore under sub-second ledgers",
    // section "claim within the same second succeeds": root creates a 100 stroop native
    // balance claimable by root (sequence 1, op 0, so the id is TestAccount::getBalanceID's
    // sha256 of OpId(root, 1, 0)), then claims it. The creator sponsors the entry's reserve
    // (CAP-33), so the entry carries root as sponsor while it lives.
    let root = root();
    let id = balance_id_of(&root, 1, 0);
    let t = txs(&golden("ClaimableBalanceTests/3635a80135f66ca8.xdr"));
    assert_eq!(t.len(), 2);

    let create = &t[0];
    let op: Vec<&StateChange> = create
        .changes
        .iter()
        .filter(|c| c.scope == ChangeScope::Operation)
        .collect();
    assert!(op.iter().any(|c| c.change
        == Change::ClaimableBalanceCreated {
            balance_id: id.clone(),
            asset: "native".into(),
            amount: Int(100),
            claimants: vec![root.clone()],
        }));
    assert!(op.iter().any(|c| c.change
        == Change::Sponsorship {
            target: format!("claimable balance {id}"),
            signer: None,
            sponsor_before: None,
            sponsor_after: Some(root.clone()),
        }));
    let mut moves: Vec<_> = op.iter().filter_map(|c| balance(c)).collect();
    moves.sort();
    assert_eq!(
        moves,
        [
            (id.as_str(), "native", "claimable-balance", 100),
            (root.as_str(), "native", "account", -100)
        ]
    );
    assert!(
        create
            .participants
            .iter()
            .any(|p| p.address == id && p.kind == AddressKind::ClaimableBalance)
    );

    let claim = &t[1];
    let op: Vec<&StateChange> = claim
        .changes
        .iter()
        .filter(|c| c.scope == ChangeScope::Operation)
        .collect();
    assert!(op.iter().any(|c| c.change
        == Change::ClaimableBalanceClaimed {
            balance_id: id.clone(),
            asset: "native".into(),
            amount: Int(100),
        }));
    let mut moves: Vec<_> = op.iter().filter_map(|c| balance(c)).collect();
    moves.sort();
    assert_eq!(
        moves,
        [
            (id.as_str(), "native", "claimable-balance", -100),
            (root.as_str(), "native", "account", 100)
        ]
    );
}

#[test]
fn kat_failed_claim_has_fee_effects_only() {
    // same test, section "claim in the next second fails": the claim lands after absBefore,
    // fails with CLAIM_CLAIMABLE_BALANCE_CANNOT_CLAIM, and only the fee and the sequence
    // bump remain
    let root = root();
    let t = txs(&golden("ClaimableBalanceTests/67b49513ed2f1609.xdr"));
    let failed = &t[1];
    assert!(!failed.successful);
    assert_eq!(failed.result, "tx_failed");
    assert_eq!(kinds(&failed.changes), ["balance-change", "sequence"]);
    assert_eq!(
        balance(&failed.changes[0]),
        Some((root.as_str(), "native", "account", -100))
    );
    assert!(
        failed
            .declared
            .participants
            .iter()
            .any(|p| p.roles.contains(&"claimable-balance".to_string()))
    );
    assert!(failed.participants.iter().all(|p| p.address == root));
}

#[test]
fn kat_create_accounts_and_payment() {
    // TxEnvelopeTests.cpp "txset - correct apply order": root creates a1 and b1 with
    // getLastReserve() * 10 = 10 * 100000000 stroops each, then a1 pays b1 1000 stroops.
    // The later tx set is only sorted, never applied, so the stream holds three txs.
    let (root, a1, b1) = (root(), test_account("a1"), test_account("b1"));
    let t = txs(&golden("TxEnvelopeTests/8c2395b250372f24.xdr"));
    assert_eq!(t.len(), 3);
    for (tx, who) in [(&t[0], &a1), (&t[1], &b1)] {
        assert!(tx.changes.iter().any(|c| c.change
            == Change::AccountCreated {
                account: who.clone(),
                starting_balance: Int(1_000_000_000),
            }));
        let mut op: Vec<_> = tx
            .changes
            .iter()
            .filter(|c| c.scope == ChangeScope::Operation)
            .filter_map(balance)
            .collect();
        op.sort();
        let mut want = vec![
            (who.as_str(), "native", "account", 1_000_000_000),
            (root.as_str(), "native", "account", -1_000_000_000),
        ];
        want.sort();
        assert_eq!(op, want);
    }
    let pay = &t[2];
    assert_eq!(pay.fee.source, a1);
    let mut op: Vec<_> = pay
        .changes
        .iter()
        .filter(|c| c.scope == ChangeScope::Operation)
        .filter_map(balance)
        .collect();
    op.sort();
    let mut want = vec![
        (a1.as_str(), "native", "account", -1000),
        (b1.as_str(), "native", "account", 1000),
    ];
    want.sort();
    assert_eq!(op, want);
}

#[test]
fn kat_sponsorship_success_declares_more_than_it_touches() {
    // BeginSponsoringFutureReservesTests.cpp "sponsor future reserves", section "success":
    // root begins sponsoring a1 and a1 ends it in the same tx; nothing is created, so no
    // entry of a1 changes. a1 is in the envelope (op source, sponsored id) but did not take
    // part in any ledger change: declared and actual participants differ.
    let (root, a1) = (root(), test_account("a1"));
    let t = txs(&golden(
        "BeginSponsoringFutureReservesTests/1c7fb8566ea79e53.xdr",
    ));
    let tx = t.last().unwrap();
    assert!(tx.successful);
    let actual: Vec<&str> = tx.participants.iter().map(|p| p.address.as_str()).collect();
    assert_eq!(actual, [root.as_str()]);
    let declared: Vec<&str> = tx
        .declared
        .participants
        .iter()
        .map(|p| p.address.as_str())
        .collect();
    let mut want = vec![root.as_str(), a1.as_str()];
    want.sort();
    assert_eq!(declared, want);
    assert_eq!(kinds(&tx.changes), ["balance-change", "sequence"]);
}

#[test]
fn kat_sponsored_trustline_and_signer() {
    // same file, section "add sponsored entry before adding first sponsored signer": root
    // sponsors a1's CUR1 trustline (limit 1000, issuer root) and then a1's signer S1 with
    // weight 1. root has no AUTH_REQUIRED flag, so the trustline starts authorized (flag 1).
    let (root, a1, s1) = (root(), test_account("a1"), test_account("S1"));
    let cur1 = format!("CUR1:{root}");
    let all: Vec<Change> = txs(&golden(
        "BeginSponsoringFutureReservesTests/1467a37ff2a1e960.xdr",
    ))
    .into_iter()
    .flat_map(|t| t.changes)
    .map(|c| c.change)
    .collect();
    let want = [
        Change::TrustlineCreated {
            account: a1.clone(),
            asset: cur1.clone(),
            limit: Int(1000),
            flags: 1,
        },
        Change::Sponsorship {
            target: format!("trustline {a1} {cur1}"),
            signer: None,
            sponsor_before: None,
            sponsor_after: Some(root.clone()),
        },
        Change::SignerAdded {
            account: a1.clone(),
            signer: s1.clone(),
            weight: 1,
        },
        Change::Sponsorship {
            target: format!("account {a1}"),
            signer: Some(s1.clone()),
            sponsor_before: None,
            sponsor_after: Some(root.clone()),
        },
    ];
    for w in want {
        assert_eq!(all.iter().filter(|c| **c == w).count(), 1, "{w:?}");
    }
}

#[test]
fn kat_sac_trust_creates_max_trustline_without_token_event() {
    // InvokeHostFunctionTests.cpp "Trustline stellar asset contract", protocol 30: under
    // CAP-73 `client.trust(trustAcc)` (SorobanTxTestUtils.cpp AssetContractTestClient::trust,
    // invoked with trustAcc as source) creates trustAcc's IDR trustline with balance 0 and
    // limit INT64_MAX, and the SAC emits no event for it. The issuer cleared
    // AUTH_CLAWBACK_ENABLED earlier and never set AUTH_REQUIRED, so the new trustline is
    // authorized (flag 1) without clawback. The second trust call is a no-op.
    let trust_acc = test_account("trustAcc");
    let idr = format!("IDR:{}", test_account("issuer"));
    let t = txs(&golden("InvokeHostFunctionTests/4c437318f87437c5.xdr"));
    let hits: Vec<(usize, &TxFacts)> = t
        .iter()
        .enumerate()
        .filter(|(_, tx)| {
            tx.changes.iter().any(|c| {
                matches!(&c.change, Change::TrustlineCreated { account, .. } if *account == trust_acc)
            })
        })
        .collect();
    assert_eq!(hits.len(), 1);
    let (i, tx) = hits[0];
    assert!(tx.successful);
    assert_eq!(tx.fee.source, trust_acc);
    let created: Vec<&StateChange> = tx
        .changes
        .iter()
        .filter(|c| matches!(c.change, Change::TrustlineCreated { .. }))
        .collect();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].scope, ChangeScope::Operation);
    assert_eq!(
        created[0].change,
        Change::TrustlineCreated {
            account: trust_acc.clone(),
            asset: idr.clone(),
            limit: Int(i64::MAX.into()),
            flags: 1,
        }
    );
    let events: Vec<&TokenEvent> = tx
        .changes
        .iter()
        .filter_map(|c| match &c.change {
            Change::TokenEvent(e) => Some(e),
            _ => None,
        })
        .collect();
    assert!(!events.is_empty(), "the fee events are there");
    assert!(
        events
            .iter()
            .all(|e| e.event == "fee" && e.stage != "operation")
    );

    // the idempotent second call is the next trustAcc transaction and changes no trustline
    let again = t[i + 1..]
        .iter()
        .find(|tx| tx.fee.source == trust_acc)
        .expect("second trust call");
    assert!(again.successful);
    assert!(!again.changes.iter().any(|c| matches!(
        c.change,
        Change::TrustlineCreated { .. }
            | Change::TrustlineLimit { .. }
            | Change::TrustlineFlags { .. }
    )));
}

#[test]
fn kat_muxed_contract_mint_carries_mux_id() {
    // InvokeHostFunctionTests.cpp "Stellar asset contract transfer to muxed contract
    // (CAP-0084)", section "custom asset|issuer transfer emits mint": root (the USDC
    // issuer) transfers 500000000 to the muxed contract address of the deployed
    // sac_transfer contract with mux id 987654321987654321. The SAC credits the base
    // contract and reports the mint with to_muxed_id (makeMintOrBurnEvent in the test).
    let net = core_net();
    let root_s = root();
    let usdc = Asset::CreditAlphanum4(AlphaNum4 {
        asset_code: AssetCode4(*b"USDC"),
        issuer: root_s.parse().unwrap(),
    });
    let sac = shapekit_oracle::addr::contract_hash(&net.asset_contract_id(&usdc));
    let usdc_id = format!("USDC:{root_s}");
    let all = txs(&golden("InvokeHostFunctionTests/426337b02a4e2a17.xdr"));
    // SorobanTest's own setup deploys a wasm contract before the test body runs, so the
    // destination is the first wasm deploy after the test creates a1
    let a1 = test_account("a1");
    let mut after_a1 = false;
    let mut dest = None;
    for c in all.iter().flat_map(|t| &t.changes) {
        match &c.change {
            Change::AccountCreated { account, .. } if *account == a1 => after_a1 = true,
            Change::ContractDeployed {
                contract,
                executable,
            } if after_a1 && dest.is_none() && executable.starts_with("wasm:") => {
                dest = Some(contract.clone())
            }
            _ => {}
        }
    }
    let dest = &dest.expect("the sac_transfer contract is deployed after a1");
    let tx = all.last().unwrap();
    assert!(tx.successful);
    let mints: Vec<&TokenEvent> = tx
        .changes
        .iter()
        .filter_map(|c| match &c.change {
            Change::TokenEvent(e) if e.event == "mint" => Some(e),
            _ => None,
        })
        .collect();
    assert_eq!(mints.len(), 1);
    let m = mints[0];
    assert_eq!(m.contract, sac);
    assert_eq!(m.asset.as_deref(), Some(usdc_id.as_str()));
    assert_eq!(m.to.as_deref(), Some(dest.as_str()));
    assert_eq!(m.amount, Some(Int(500_000_000)));
    assert_eq!(m.to_muxed_id.as_deref(), Some("id:987654321987654321"));
    let p = tx.participants.iter().find(|p| &p.address == dest).unwrap();
    assert_eq!(p.kind, AddressKind::Contract);
    assert_eq!(p.mux_ids, [Int(987_654_321_987_654_321)]);
    assert!(tx.changes.iter().any(|c| balance(c)
        == Some((dest.as_str(), usdc_id.as_str(), "contract", 500_000_000))));
}

#[test]
fn signed_payload_signer_on_testnet_is_reported_as_its_p_strkey() {
    // testnet ledger 4437357 (named by wallet-backend#729): a SetOptions adds an
    // ed25519 signed-payload signer. The expected strkey is computed here from the
    // envelope's signer with stellar-strkey directly, independent of the oracle.
    let net = Network::new(TESTNET);
    let lcm = &testnet_ledgers()[1];
    let view = shapekit_oracle::view::LedgerView::new(lcm, &net).unwrap();
    let mut expected = Vec::new();
    for tx in &view.txs {
        let ops = match tx.envelope {
            TransactionEnvelope::Tx(e) => e.tx.operations.to_vec(),
            _ => continue,
        };
        for op in ops {
            if let OperationBody::SetOptions(so) = op.body
                && let Some(s) = so.signer
                && let SignerKey::Ed25519SignedPayload(p) = s.key
            {
                let sp =
                    stellar_strkey::ed25519::SignedPayload::new(p.ed25519.0, p.payload.as_slice())
                        .unwrap();
                expected.push((
                    hex::encode(tx.result.transaction_hash.0),
                    sp.to_string().as_str().to_string(),
                    s.weight,
                ));
            }
        }
    }
    assert!(!expected.is_empty());
    let facts = interpret(lcm, &net).unwrap();
    for (hash, p, weight) in expected {
        assert!(p.starts_with('P'));
        let tx = facts.iter().find(|t| t.hash == hash).unwrap();
        assert!(tx.successful);
        assert!(tx.changes.iter().any(|c| matches!(&c.change,
            Change::SignerAdded { signer, weight: w, .. } if *signer == p && *w == weight)));
        // and the strkey decodes back to the same key and payload
        let back: SignerKey = p.parse().unwrap();
        assert!(matches!(back, SignerKey::Ed25519SignedPayload(_)));
    }
}

#[test]
fn failed_transactions_have_fee_effects_only() {
    // across every fixture ledger: a failed transaction keeps its fee charge (and Soroban
    // refund), its sequence bump and its fee events, and nothing from its operations
    let mut seen = 0;
    let mut check = |tx: &TxFacts| {
        if tx.successful {
            return;
        }
        seen += 1;
        for c in &tx.changes {
            match (&c.scope, &c.change) {
                (ChangeScope::Operation, _) => panic!("{}: operation change {c:?}", tx.hash),
                (_, Change::BalanceChange { holder, asset, .. }) => {
                    assert_eq!(holder, &tx.fee.source, "{}", tx.hash);
                    assert_eq!(asset, "native");
                    assert!(matches!(
                        c.scope,
                        ChangeScope::FeeProcessing
                            | ChangeScope::TxAfter
                            | ChangeScope::PostApplyFee
                    ));
                }
                (ChangeScope::TxBefore, Change::Sequence { .. }) => {}
                (_, Change::TokenEvent(e)) => assert_eq!(e.event, "fee", "{}", tx.hash),
                other => panic!("{}: unexpected {other:?}", tx.hash),
            }
        }
    };
    for path in all_goldens() {
        for tx in txs(&path) {
            check(&tx);
        }
    }
    let net = Network::new(TESTNET);
    for lcm in testnet_ledgers() {
        for tx in interpret(&lcm, &net).unwrap() {
            check(&tx);
        }
    }
    assert!(seen >= 5, "only {seen} failed transactions in the fixtures");
}

#[test]
fn output_is_stable_json_with_integer_strings() {
    let net = Network::new(TESTNET);
    let lcm = &testnet_ledgers()[0];
    let a = interpret_ledger(lcm, &net).unwrap();
    let b = interpret_ledger(lcm, &net).unwrap();
    assert_eq!(a, b);
    let file = OracleFile {
        oracle_vocab_version: ORACLE_VOCAB_VERSION,
        network_passphrase: TESTNET.into(),
        ledgers: vec![a],
    };
    let text = serde_json::to_string(&file).unwrap();
    let back: OracleFile = serde_json::from_str(&text).unwrap();
    assert_eq!(back, file);
    // no float anywhere: every number in the output is a small count, index or flag
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    fn walk(v: &serde_json::Value) {
        match v {
            serde_json::Value::Number(n) => assert!(n.as_u64().is_some_and(|n| n < 1 << 32), "{n}"),
            serde_json::Value::Array(a) => a.iter().for_each(walk),
            serde_json::Value::Object(o) => o.values().for_each(walk),
            _ => {}
        }
    }
    walk(&v["ledgers"][0]["transactions"]);
}

#[test]
fn kat_offer_cross_reports_the_trade() {
    // OfferTests.cpp "offer crossing with minimal receive cap", protocol 30, section "maker
    // offer is fully taken": maker sells 1001 CUR1 at price 3/2; taker offers 3000 CUR2 at
    // 2/3. From protocol 29 the cross moves exactly wheatAmount = 1001 CUR1 and
    // buyingLiability = floor(1001 * 3 / 2) = 1501 CUR2, the maker offer is deleted and the
    // taker's offer rests with 3000 - 1501 = 1499.
    let issuer = test_account("issuer");
    let (maker, taker) = (test_account("maker"), test_account("taker"));
    let (wheat, sheep) = (format!("CUR1:{issuer}"), format!("CUR2:{issuer}"));
    let t = txs(&golden("OfferTests/847b038d8a2ce1c2.xdr"));
    let trades: Vec<(&TxFacts, &Change)> = t
        .iter()
        .flat_map(|tx| tx.changes.iter().map(move |c| (tx, &c.change)))
        .filter(|(_, c)| matches!(c, Change::Trade { .. }))
        .collect();
    assert_eq!(trades.len(), 1);
    let (tx, trade) = trades[0];
    let Change::Trade {
        counterparty,
        sold_asset,
        sold_amount,
        bought_asset,
        bought_amount,
        ..
    } = trade
    else {
        unreachable!()
    };
    assert_eq!(counterparty, &maker);
    assert_eq!((sold_asset, sold_amount.0), (&wheat, 1001));
    assert_eq!((bought_asset, bought_amount.0), (&sheep, 1501));
    assert_eq!(tx.fee.source, taker);
    let offers: Vec<(Action, &str, Option<i128>)> = tx
        .changes
        .iter()
        .filter_map(|c| match &c.change {
            Change::Offer {
                action,
                seller,
                amount_after,
                ..
            } => Some((*action, seller.as_str(), amount_after.map(|a| a.0))),
            _ => None,
        })
        .collect();
    assert!(offers.contains(&(Action::Removed, maker.as_str(), None)));
    assert!(offers.contains(&(Action::Created, taker.as_str(), Some(1499))));
    let mut moves: Vec<_> = tx
        .changes
        .iter()
        .filter(|c| c.scope == ChangeScope::Operation)
        .filter_map(balance)
        .collect();
    moves.sort();
    let mut want = vec![
        (maker.as_str(), wheat.as_str(), "trustline", -1001),
        (maker.as_str(), sheep.as_str(), "trustline", 1501),
        (taker.as_str(), wheat.as_str(), "trustline", 1001),
        (taker.as_str(), sheep.as_str(), "trustline", -1501),
    ];
    want.sort();
    assert_eq!(moves, want);
}
