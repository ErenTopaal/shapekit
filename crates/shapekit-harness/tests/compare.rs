//! The comparison logic against exports.
//!
//! TEST FIXTURES: until recorded wallet-backend exports exist, the exports here are built in
//! this file from committed corpus oracle files (the pass path) and then edited by hand, one
//! edit per failure verdict. They exist only in this test and are never used by the harness.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use shapekit_harness::compare::{
    ProblemKind, Reason, Stop, Verdict, compare_tx, stopped_verdict, uncomparable_claims,
    verdict_of,
};
use shapekit_harness::protocol::{
    ChangeClaim, Claims, ExportChange, ExportParticipant, ParticipantClaim, RawAddress, TxRecord,
    vocabulary_fields,
};
use shapekit_harness::report::{MemoryOptions, Round, memory_check};
use shapekit_harness::run::bootstrap_checkpoint;
use shapekit_oracle::vocab::{EventCoverage, OracleFile, TxFacts};

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
}

fn oracle_tx(entry: &str) -> TxFacts {
    let family = entry.rsplit_once(".p").map(|(f, _)| f).unwrap();
    let path = corpus()
        .join("entries")
        .join(family)
        .join(entry)
        .join("testnet.oracle.json");
    let file: OracleFile = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(file.ledgers.len(), 1);
    assert_eq!(file.ledgers[0].transactions.len(), 1);
    file.ledgers[0].transactions[0].clone()
}

const SIGNER_ENTRY: &str = "cap40.signed-payload-signer.p28-set-options-payload-1-meta";
const TRUST_ENTRY: &str = "cap73.sac-trust.p28-trust-again";

fn claim(kind: &str, fields: &[&str]) -> ChangeClaim {
    ChangeClaim {
        kind: kind.into(),
        filter: BTreeMap::new(),
        fields: fields.iter().map(|s| s.to_string()).collect(),
    }
}

/// Claims shaped like a wallet-backend adapter's.
fn claims() -> Claims {
    let mut token = claim(
        "token-event",
        &["event", "contract", "from", "to", "amount"],
    );
    token.filter.insert(
        "event".into(),
        vec![
            json!("transfer"),
            json!("mint"),
            json!("burn"),
            json!("fee"),
        ],
    );
    Claims {
        participants: Some(ParticipantClaim {
            address_kinds: vec!["account".into(), "contract".into()],
            mux_ids: false,
        }),
        changes: vec![
            claim("signer-added", &["account", "signer", "weight"]),
            claim("trustline-created", &["account", "asset", "limit"]),
            token,
        ],
    }
}

/// The export a correct target would produce: every claimed fact, built from the oracle.
fn faithful_export(tx: &TxFacts, claims: &Claims) -> TxRecord {
    let participants = tx
        .participants
        .iter()
        .map(|p| ExportParticipant {
            address: Some(p.address.clone()),
            mux_ids: p.mux_ids.clone(),
            raw: None,
        })
        .collect();
    let mut changes = Vec::new();
    for c in &tx.changes {
        let Value::Object(m) = serde_json::to_value(c).unwrap() else {
            unreachable!()
        };
        let kind = m["kind"].as_str().unwrap().to_owned();
        let Some(cl) = claims.changes.iter().find(|cl| {
            cl.kind == kind
                && cl
                    .filter
                    .iter()
                    .all(|(f, vals)| vals.contains(m.get(f).unwrap_or(&Value::Null)))
        }) else {
            continue;
        };
        let fields = cl
            .fields
            .iter()
            .filter_map(|f| m.get(f).map(|v| (f.clone(), v.clone())))
            .collect();
        changes.push(ExportChange {
            kind,
            raw: BTreeMap::new(),
            source: None,
            fields,
        });
    }
    TxRecord {
        ledger: 0,
        hash: tx.hash.clone(),
        participants,
        changes,
        unmapped: Vec::new(),
    }
}

fn verdict(tx: &TxFacts, rec: Option<&TxRecord>, claims: &Claims) -> (Verdict, Option<Reason>) {
    let v = verdict_of(vec![compare_tx(tx, rec, claims, &vocabulary_fields())]);
    (v.verdict, v.reason)
}

#[test]
fn faithful_export_passes() {
    for entry in [SIGNER_ENTRY, TRUST_ENTRY] {
        let tx = oracle_tx(entry);
        let rec = faithful_export(&tx, &claims());
        let v = verdict_of(vec![compare_tx(
            &tx,
            Some(&rec),
            &claims(),
            &vocabulary_fields(),
        )]);
        assert_eq!(v.verdict, Verdict::Pass, "{entry}: {v:?}");
        assert!(v.notes.is_empty(), "{entry}: {:?}", v.notes);
    }
}

#[test]
fn missing_signer_is_dropped() {
    let tx = oracle_tx(SIGNER_ENTRY);
    let mut rec = faithful_export(&tx, &claims());
    rec.changes.retain(|c| c.kind != "signer-added");
    let v = verdict_of(vec![compare_tx(
        &tx,
        Some(&rec),
        &claims(),
        &vocabulary_fields(),
    )]);
    assert_eq!(v.verdict, Verdict::Dropped);
    assert_eq!(v.problems.len(), 1);
    assert_eq!(v.problems[0].fact, "signer-added");
    assert!(
        v.problems[0].expected.as_ref().unwrap()["signer"]
            .as_str()
            .unwrap()
            .starts_with('P')
    );
}

#[test]
fn nothing_stored_for_the_transaction_is_dropped() {
    let tx = oracle_tx(TRUST_ENTRY);
    assert_eq!(verdict(&tx, None, &claims()), (Verdict::Dropped, None));
}

#[test]
fn other_weight_is_a_wrong_value() {
    let tx = oracle_tx(SIGNER_ENTRY);
    let mut rec = faithful_export(&tx, &claims());
    let c = rec
        .changes
        .iter_mut()
        .find(|c| c.kind == "signer-added")
        .unwrap();
    c.fields.insert("weight".into(), json!(2));
    let v = verdict_of(vec![compare_tx(
        &tx,
        Some(&rec),
        &claims(),
        &vocabulary_fields(),
    )]);
    assert_eq!(
        (v.verdict, v.reason),
        (Verdict::Mishandled, Some(Reason::WrongValue))
    );
    assert_eq!(v.problems[0].kind, ProblemKind::WrongValue);
    assert!(
        v.problems[0].detail.contains("weight"),
        "{}",
        v.problems[0].detail
    );
}

#[test]
fn truncated_signer_bytes_are_undecodable() {
    let tx = oracle_tx(SIGNER_ENTRY);
    let mut rec = faithful_export(&tx, &claims());
    let c = rec
        .changes
        .iter_mut()
        .find(|c| c.kind == "signer-added")
        .unwrap();
    c.fields.insert("signer".into(), Value::Null);
    // a P key cut to the 32 key bytes behind its version byte, as wallet-backend#716 stored it
    c.raw.insert(
        "signer".into(),
        RawAddress {
            column: "state_changes.signer_account_id".into(),
            hex: format!("78{}", "c6".repeat(32)),
            strkey: None,
            error: Some("signed payload strkey needs at least 40 payload bytes".into()),
        },
    );
    assert_eq!(
        verdict(&tx, Some(&rec), &claims()),
        (Verdict::Mishandled, Some(Reason::UndecodableAddress))
    );
}

#[test]
fn undecodable_participant_beats_a_drop() {
    let tx = oracle_tx(SIGNER_ENTRY);
    let mut rec = faithful_export(&tx, &claims());
    let p = rec
        .participants
        .iter_mut()
        .find(|p| p.address.as_deref().is_some_and(|a| a.starts_with('G')))
        .unwrap();
    p.address = None;
    p.raw = Some(RawAddress {
        column: "transactions_accounts.account_id".into(),
        hex: format!("60{}", "00".repeat(32)),
        strkey: None,
        error: Some("muxed account strkey needs 40 payload bytes".into()),
    });
    let v = verdict_of(vec![compare_tx(
        &tx,
        Some(&rec),
        &claims(),
        &vocabulary_fields(),
    )]);
    assert_eq!(v.reason, Some(Reason::UndecodableAddress));
    assert!(v.problems.iter().any(|p| p.kind == ProblemKind::Dropped));
}

#[test]
fn a_participant_the_meta_does_not_show_is_a_wrong_value() {
    let tx = oracle_tx(TRUST_ENTRY);
    let mut rec = faithful_export(&tx, &claims());
    rec.participants.push(ExportParticipant {
        address: Some("GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN7".into()),
        mux_ids: Vec::new(),
        raw: None,
    });
    let v = verdict_of(vec![compare_tx(
        &tx,
        Some(&rec),
        &claims(),
        &vocabulary_fields(),
    )]);
    assert_eq!(v.reason, Some(Reason::WrongValue));
    assert_eq!(v.problems[0].kind, ProblemKind::Extra);
}

#[test]
fn a_claimed_balance_change_left_out_is_dropped() {
    let tx = oracle_tx(TRUST_ENTRY);
    let trust_claims = Claims {
        participants: None,
        changes: vec![claim(
            "balance-change",
            &["holder", "asset", "held_in", "delta"],
        )],
    };
    let rec = faithful_export(&tx, &trust_claims);
    assert_eq!(verdict(&tx, Some(&rec), &trust_claims).0, Verdict::Pass);
    let mut dropped = rec.clone();
    dropped.changes.clear();
    assert_eq!(
        verdict(&tx, Some(&dropped), &trust_claims).0,
        Verdict::Dropped
    );
}

#[test]
fn equal_changes_count_as_a_multiset() {
    let tx = oracle_tx(SIGNER_ENTRY);
    let mut doubled = tx.clone();
    let signer = doubled
        .changes
        .iter()
        .find(|c| serde_json::to_value(c).unwrap()["kind"] == "signer-added")
        .unwrap()
        .clone();
    doubled.changes.push(signer);
    let rec = faithful_export(&tx, &claims());
    assert_eq!(verdict(&doubled, Some(&rec), &claims()).0, Verdict::Dropped);
}

#[test]
fn nothing_claimed_is_out_of_scope() {
    let tx = oracle_tx(SIGNER_ENTRY);
    let none = Claims {
        participants: None,
        changes: vec![claim("config-setting", &["setting"])],
    };
    let rec = faithful_export(&tx, &none);
    assert_eq!(verdict(&tx, Some(&rec), &none).0, Verdict::OutOfScope);
}

#[test]
fn unclaimed_exports_are_noted_not_judged() {
    let tx = oracle_tx(SIGNER_ENTRY);
    let mut rec = faithful_export(&tx, &claims());
    let mut fields = serde_json::Map::new();
    fields.insert(
        "account".into(),
        json!("GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN7"),
    );
    rec.changes.push(ExportChange {
        kind: "home-domain".into(),
        raw: BTreeMap::new(),
        source: None,
        fields,
    });
    let v = verdict_of(vec![compare_tx(
        &tx,
        Some(&rec),
        &claims(),
        &vocabulary_fields(),
    )]);
    assert_eq!(v.verdict, Verdict::Pass);
    assert_eq!(v.notes.len(), 1, "{:?}", v.notes);
}

#[test]
fn a_stop_marks_its_ledger_and_leaves_later_ones_unevaluated() {
    let stop = Stop {
        ledger: 66,
        reason: Reason::Halted,
        message: "COPY failed".into(),
    };
    let at = stopped_verdict(66, &stop, Some("e1"));
    assert_eq!(
        (at.verdict, at.reason),
        (Verdict::Mishandled, Some(Reason::Halted))
    );
    let later = stopped_verdict(70, &stop, Some("e1"));
    assert_eq!(later.verdict, Verdict::NotEvaluated);
    assert!(later.detail.contains("e1"), "{}", later.detail);
}

#[test]
fn bootstrap_is_the_newest_checkpoint_before_the_first_entry() {
    assert_eq!(bootstrap_checkpoint(&[63, 127], 64), Some(63));
    assert_eq!(bootstrap_checkpoint(&[63, 127], 200), Some(127));
    assert_eq!(bootstrap_checkpoint(&[63], 63), None);
    assert_eq!(bootstrap_checkpoint(&[], 10), None);
}

#[test]
fn memory_growth_across_rounds_fails_the_check() {
    let round = |round, peak| Round {
        round,
        peak_rss_bytes: peak,
        rss_after_bytes: peak,
        seconds: 1.0,
        same_verdicts_as_first: true,
        chain_errors: Vec::new(),
    };
    let mem = MemoryOptions {
        rounds: 2,
        slack_fraction: 0.1,
        slack_min_bytes: 16 << 20,
        budget_bytes: Some(512 << 20),
    };
    let flat = memory_check(&[round(1, 200 << 20), round(2, 210 << 20)], &mem);
    assert!(flat.flat && flat.within_budget, "{flat:?}");
    let grew = memory_check(&[round(1, 200 << 20), round(2, 260 << 20)], &mem);
    assert!(!grew.flat, "{grew:?}");
    let over = memory_check(&[round(1, 600 << 20)], &mem);
    assert!(!over.within_budget, "{over:?}");
}

const V3_CLASSIC_ENTRY: &str = "cap67.unified-events.p22-payment-transfer";
const V4_CLASSIC_ENTRY: &str = "cap67.unified-events.p22-backfill-payment-transfer";

/// A transfer and a fee derived from the operation, the way wallet-backend stores them for a
/// payment whether or not the meta has events.
fn derived_events(tx: &TxFacts) -> Vec<ExportChange> {
    let fields = |v: Value| match v {
        Value::Object(m) => m,
        _ => unreachable!(),
    };
    vec![
        ExportChange {
            kind: "token-event".into(),
            raw: BTreeMap::new(),
            source: None,
            fields: fields(json!({"event": "transfer", "contract": "CXX", "from": tx.fee.source, "to": "GXX", "amount": "1", "scope": "operation"})),
        },
        ExportChange {
            kind: "token-event".into(),
            raw: BTreeMap::new(),
            source: None,
            fields: fields(json!({"event": "fee", "contract": "CXX", "from": tx.fee.source, "amount": "100", "scope": "fee-processing"})),
        },
    ]
}

#[test]
fn events_the_meta_cannot_carry_are_out_of_scope_not_extra() {
    let tx = oracle_tx(V3_CLASSIC_ENTRY);
    assert_eq!(tx.events, Some(EventCoverage::None));
    let mut rec = faithful_export(&tx, &claims());
    rec.changes.extend(derived_events(&tx));
    let v = verdict_of(vec![compare_tx(
        &tx,
        Some(&rec),
        &claims(),
        &vocabulary_fields(),
    )]);
    assert_eq!(v.verdict, Verdict::OutOfScope, "{v:?}");
    assert!(v.problems.is_empty(), "{:?}", v.problems);
    assert!(v.detail.contains("cannot be checked"), "{}", v.detail);
}

#[test]
fn a_real_problem_still_wins_over_uncomparable_events() {
    let tx = oracle_tx(V3_CLASSIC_ENTRY);
    let mut rec = faithful_export(&tx, &claims());
    rec.participants.clear();
    rec.changes.extend(derived_events(&tx));
    let v = verdict_of(vec![compare_tx(
        &tx,
        Some(&rec),
        &claims(),
        &vocabulary_fields(),
    )]);
    assert_eq!(v.verdict, Verdict::Dropped, "{v:?}");
    assert!(v.problems.iter().all(|p| p.fact == "participant"));
}

#[test]
fn events_in_v4_meta_are_compared() {
    let tx = oracle_tx(V4_CLASSIC_ENTRY);
    assert_eq!(tx.events, Some(EventCoverage::Unified));
    let mut rec = faithful_export(&tx, &claims());
    assert_eq!(verdict(&tx, Some(&rec), &claims()), (Verdict::Pass, None));
    rec.changes.extend(derived_events(&tx));
    assert_eq!(
        verdict(&tx, Some(&rec), &claims()),
        (Verdict::Mishandled, Some(Reason::WrongValue))
    );
}

#[test]
fn soroban_v3_coverage_drops_only_claims_beyond_operation_events() {
    let mut op = claim("token-event", &["event", "contract", "from", "to", "amount"]);
    op.filter.insert("stage".into(), vec![json!("operation")]);
    let mut fee = claim("token-event", &["event", "contract", "from"]);
    fee.filter.insert("stage".into(), vec![json!("before-all-txs")]);
    let any = claim("contract-event", &["contract", "stage"]);
    let c = Claims {
        participants: None,
        changes: vec![op, fee, any],
    };
    assert_eq!(
        uncomparable_claims(EventCoverage::SorobanOperation, &c),
        vec![1, 2]
    );
    assert_eq!(uncomparable_claims(EventCoverage::None, &c), vec![0, 1, 2]);
    assert!(uncomparable_claims(EventCoverage::Unified, &c).is_empty());
}
