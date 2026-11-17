//! Every contract round-trips through JSON, follows its own generated schema, and its checks
//! catch what the schema cannot.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use shapekit_contracts::audit::{
    AuditNetwork, CoverageDelta, EnvelopeCounts, LawResult, NewBranch, SdkRef, Violation,
};
use shapekit_contracts::core_coverage::{FileCoverage, Region, RegionClass};
use shapekit_contracts::gates::{Branch, Comparison, Gate, GateSide};
use shapekit_contracts::limits::{Limit, LimitRejection};
use shapekit_contracts::obligations::Obligation;
use shapekit_contracts::recipe_ir::{
    AuthSpec, Footprint, NodeOptions, Outcome, ResourceOverride, Selection, Step, SymRef, TxExpect,
};
use shapekit_contracts::validation::{
    ApplyStatus, FieldVerdict, ProbeAttempt, SubmitStatus, Verdict,
};
use shapekit_contracts::{
    AuditReport, Contract, CoreCoverage, CoreRef, GateList, ImageRef, LimitsTable, ObligationKind,
    ObligationLedger, ObligationStatus, RecipeIr, Stage, ValidationTable, load, render,
    schema_text, write,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn core(image: bool) -> CoreRef {
    CoreRef {
        version: "v30.0.0".into(),
        commit: "0e49751aa9ddf892302cca02cea7f3f5d97df9f1".into(),
        image: image.then(|| ImageRef {
            repository: "stellar/stellar-core".into(),
            tag: "30.0.0-3642.0e49751aa.noble".into(),
            digest: "sha256:eb7a7c81a29b8665c8878d55e1b3979f0d7527426350c4ff258a9c7647ae5c51"
                .into(),
        }),
    }
}

/// Round trip, schema validity, no problems, and load after write.
fn holds<T: Contract + PartialEq + std::fmt::Debug>(value: &T) {
    let text = render(value);
    let back: T = serde_json::from_str(&text).unwrap();
    assert_eq!(&back, value);
    assert_eq!(render(&back), text, "rendering is stable");
    let schema: Value = serde_json::from_str(&schema_text::<T>()).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let instance: Value = serde_json::from_str(&text).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(&instance)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{}: {errors:?}", T::SCHEMA_FILE);
    assert_eq!(value.problems(), Vec::<String>::new(), "{}", T::SCHEMA_FILE);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x/file.json");
    write(&path, value).unwrap();
    assert_eq!(&load::<T>(&path).unwrap(), value);
}

fn has_problem<T: Contract>(value: &T, needle: &str) {
    let p = value.problems();
    assert!(
        p.iter().any(|l| l.contains(needle)),
        "no {needle:?} in {p:#?}"
    );
}

fn ledger() -> ObligationLedger {
    ObligationLedger {
        schema_version: 1,
        obligations: vec![
            Obligation {
                id: "field:tx.memo".into(),
                kind: ObligationKind::Field,
                status: ObligationStatus::Met,
                witnesses: vec!["candidate:probe.memo.p30-id-max".into()],
                evidence: None,
            },
            Obligation {
                id: "gate-side:src/transactions/SetOptionsOpFrame.cpp:308:9:true".into(),
                kind: ObligationKind::GateSide,
                status: ObligationStatus::Open,
                witnesses: vec![],
                evidence: Some("no recipe below protocol 19 yet".into()),
            },
            Obligation {
                id: "seed:js:17.4.0:42".into(),
                kind: ObligationKind::Seed,
                status: ObligationStatus::Unmeetable,
                witnesses: vec![],
                evidence: Some("the mutant is equivalent: it rewrites dead code".into()),
            },
        ],
    }
}

#[test]
fn obligations() {
    let l = ledger();
    holds(&l);
    assert_eq!(l.count(ObligationKind::Field, ObligationStatus::Met), 1);

    let mut bad = l.clone();
    bad.obligations[0].witnesses.clear();
    has_problem(&bad, "is met but has no witness");
    let mut bad = l.clone();
    bad.obligations[2].evidence = None;
    has_problem(&bad, "unmeetable without evidence");
    let mut bad = l.clone();
    bad.obligations[1].witnesses = vec!["chain:x.p30".into()];
    has_problem(&bad, "with a witness it is met");
    let mut bad = l.clone();
    bad.obligations[0].id = "tx.memo".into();
    has_problem(&bad, "must have an id field:<name>");
    let mut bad = l.clone();
    bad.obligations.swap(0, 1);
    has_problem(&bad, "keep them sorted");
    let mut bad = l.clone();
    bad.obligations[0].witnesses = vec!["probe.memo".into()];
    has_problem(&bad, "has no prefix");
}

#[test]
fn every_kind_and_status_is_in_the_schema() {
    let schema = schema_text::<ObligationLedger>();
    for k in ObligationKind::ALL {
        assert!(schema.contains(&format!("\"{}\"", k.as_str())), "{k:?}");
        assert_eq!(serde_json::to_value(k).unwrap(), json!(k.as_str()));
    }
    for s in [
        ObligationStatus::Met,
        ObligationStatus::Unmeetable,
        ObligationStatus::Open,
    ] {
        assert_eq!(serde_json::to_value(s).unwrap(), json!(s.as_str()));
    }
}

fn attempt(variant: &str, sub: SubmitStatus, code: &str, applied: ApplyStatus) -> ProbeAttempt {
    ProbeAttempt {
        variant: variant.into(),
        submission: sub,
        result_code: Some(code.into()),
        result_xdr: Some("AAAAAAAAAGT////7AAAAAA==".into()),
        applied,
        witnesses: vec![format!("candidate:probe.{}", variant.replace(' ', "-"))],
        carrier: None,
    }
}

#[test]
fn validation() {
    let t = ValidationTable {
        schema_version: 1,
        core: core(true),
        fields: vec![
            FieldVerdict {
                path: "TransactionEnvelope.v1.tx.memo".into(),
                verdict: Verdict::NotValidated,
                attempts: vec![
                    attempt(
                        "memo none",
                        SubmitStatus::Pending,
                        "tx_success",
                        ApplyStatus::Succeeded,
                    ),
                    attempt(
                        "memo id max",
                        SubmitStatus::Pending,
                        "tx_success",
                        ApplyStatus::Succeeded,
                    ),
                ],
            },
            FieldVerdict {
                path: "TransactionEnvelope.v1.tx.seq_num".into(),
                verdict: Verdict::Validated,
                attempts: vec![
                    attempt(
                        "next",
                        SubmitStatus::Pending,
                        "tx_success",
                        ApplyStatus::Succeeded,
                    ),
                    attempt(
                        "next plus one",
                        SubmitStatus::Error,
                        "tx_bad_seq",
                        ApplyStatus::NotApplied,
                    ),
                ],
            },
        ],
    };
    holds(&t);

    let mut bad = t.clone();
    bad.fields[0].verdict = Verdict::Validated;
    has_problem(&bad, "every attempt ends the same way");
    let mut bad = t.clone();
    bad.fields[1].verdict = Verdict::NotValidated;
    has_problem(&bad, "2 different outcomes");
    let mut bad = t.clone();
    bad.fields[1].attempts[1].applied = ApplyStatus::Failed;
    has_problem(&bad, "cannot have been applied");
    let mut bad = t.clone();
    bad.fields[0].attempts.pop();
    has_problem(&bad, "at least two attempts");
    let mut bad = t.clone();
    bad.core.commit = "0e49751".into();
    has_problem(&bad, "40 digit hex");
}

#[test]
fn limits() {
    let t = LimitsTable {
        schema_version: 1,
        core: core(true),
        limits: vec![Limit {
            path: "SorobanAuthorizationEntry.root_invocation.sub_invocations".into(),
            measure: Some("invocations in one auth tree level".into()),
            max_accepted: "330".into(),
            first_rejected: "331".into(),
            rejection: LimitRejection {
                stage: Stage::Apply,
                result_code: Some("invoke_host_function.resource_limit_exceeded".into()),
            },
            network_settings: BTreeMap::from([(
                "contract_compute_v0.tx_max_instructions".into(),
                "100000000".into(),
            )]),
            witnesses: vec![
                "entry:auth-trees.p30-wide-330".into(),
                "candidate:auth-trees.p30-wide-331".into(),
            ],
        }],
    };
    holds(&t);
    let mut bad = t.clone();
    bad.limits[0].first_rejected = "340".into();
    has_problem(&bad, "did not finish");
    let mut bad = t.clone();
    bad.limits[0].max_accepted = "0x14a".into();
    has_problem(&bad, "decimal integers");
    let mut bad = t.clone();
    bad.limits[0].witnesses.pop();
    has_problem(&bad, "accepted and for the refused");
}

fn gate(line: u32, comparison: Comparison, version: u32) -> Gate {
    Gate {
        file: "src/transactions/SetOptionsOpFrame.cpp".into(),
        line,
        column: 9,
        function: "SetOptionsOpFrame::doCheckValid".into(),
        comparison,
        version,
        sides: vec![
            GateSide {
                branch: Branch::True,
                status: ObligationStatus::Met,
                witnesses: vec!["chain:cap40.signed-payload-signer.p18".into()],
                evidence: None,
            },
            GateSide {
                branch: Branch::False,
                status: ObligationStatus::Open,
                witnesses: vec![],
                evidence: None,
            },
        ],
    }
}

#[test]
fn gates() {
    let g = GateList {
        schema_version: 1,
        core: core(false),
        gates: vec![
            gate(308, Comparison::IsBefore, 19),
            gate(400, Comparison::Equals, 20),
        ],
    };
    holds(&g);
    let first = &g.gates[0];
    assert_eq!(
        first.side_obligation_id(Branch::True),
        "gate-side:src/transactions/SetOptionsOpFrame.cpp:308:9:true"
    );
    assert_eq!(first.protocols(Branch::True), vec![(0, Some(18))]);
    assert_eq!(first.protocols(Branch::False), vec![(19, None)]);
    assert_eq!(
        g.gates[1].protocols(Branch::False),
        vec![(0, Some(19)), (21, None)]
    );

    let mut bad = g.clone();
    bad.gates[0].sides.reverse();
    has_problem(&bad, "true side, then its false side");
    let mut bad = g.clone();
    bad.gates.reverse();
    has_problem(&bad, "keep them sorted");
    let mut bad = g.clone();
    bad.gates[0].sides[1].status = ObligationStatus::Unmeetable;
    has_problem(&bad, "unmeetable without evidence");
}

#[test]
fn core_coverage() {
    let c = CoreCoverage {
        schema_version: 1,
        core: core(true),
        chains: vec![
            "chain:cap40.signed-payload-signer.p30".into(),
            "chain:smoke".into(),
        ],
        files: vec![FileCoverage {
            path: "src/transactions/SetOptionsOpFrame.cpp".into(),
            regions_reached: 40,
            regions_unreached: 2,
        }],
        unreached: vec![
            Region {
                file: "src/transactions/SetOptionsOpFrame.cpp".into(),
                line_start: 120,
                line_end: 124,
                class: RegionClass::ReachableByCoreTests,
                core_tests: vec!["set options".into()],
            },
            Region {
                file: "src/transactions/SetOptionsOpFrame.cpp".into(),
                line_start: 200,
                line_end: 200,
                class: RegionClass::Unknown,
                core_tests: vec![],
            },
        ],
    };
    holds(&c);
    let mut bad = c.clone();
    bad.unreached[0].core_tests.clear();
    has_problem(&bad, "names no test");
    let mut bad = c.clone();
    bad.unreached[1].file = "src/other.cpp".into();
    has_problem(&bad, "has no files entry");
    let mut bad = c.clone();
    bad.chains = vec!["entry:x".into()];
    has_problem(&bad, "not a chain: reference");
}

#[test]
fn audit() {
    let r = AuditReport {
        schema_version: 1,
        network: AuditNetwork::Pubnet,
        seed: Some(20261116),
        ledgers: vec![1000, 57369500],
        protocols: vec![1, 22],
        transactions: 3,
        envelopes: Some(EnvelopeCounts {
            v0: 1,
            v1: 1,
            fee_bump: 1,
        }),
        sdk: Some(SdkRef {
            sdk: "js".into(),
            version: "17.4.0".into(),
        }),
        laws: vec![
            LawResult {
                law: "roundtrip".into(),
                checked: 2,
                violations: 0,
                examples: vec![],
            },
            LawResult {
                law: "tx-set-result-hash".into(),
                checked: 2,
                violations: 1,
                examples: vec![Violation {
                    ledger: 1000,
                    detail: "hash differs".into(),
                }],
            },
        ],
        coverage: Some(CoverageDelta {
            baseline: "corpus".into(),
            baseline_ranges: 3845,
            ranges: 3846,
            new: vec![NewBranch {
                location: "lib/xdr.js:10:2-12:4".into(),
                ledger: 57369500,
            }],
        }),
    };
    holds(&r);
    assert_eq!(r.total_violations(), 1);
    let mut bad = r.clone();
    bad.coverage.as_mut().unwrap().ranges = 3847;
    has_problem(&bad, "1 new ranges listed, 2 counted");
    let mut bad = r.clone();
    bad.laws[1].examples[0].ledger = 5;
    has_problem(&bad, "was not checked");
    let mut bad = r.clone();
    bad.transactions = 4;
    has_problem(&bad, "envelopes add up to 3");
}

fn recipe() -> RecipeIr {
    let r = RecipeIr {
        schema_version: 1,
        id: "cap01.bump-sequence".into(),
        family: "cap01.bump-sequence".into(),
        protocol: 30,
        node: NodeOptions::default(),
        below_protocol_valid: false,
        steps: vec![
            Step::Fund {
                label: Some("funded".into()),
                roles: vec!["alice".into(), "bob".into()],
                balance: "100000000000".into(),
            },
            Step::Close {
                label: None,
                order: vec![],
            },
            Step::Tx {
                label: Some("bump-forward".into()),
                source: "alice".into(),
                source_mux_id: None,
                operations: vec![json!({
                    "source_account": null,
                    "body": {"bump_sequence": {"bump_to": "@seq:alice+1048576"}}
                })],
                signers: vec![],
                memo: None,
                fee: None,
                cond: None,
                expect: TxExpect::default(),
                fee_bump: None,
            },
            Step::Close {
                label: None,
                order: vec![],
            },
            Step::Capture {
                slug: "bump-forward".into(),
                select: Selection::Ledger {
                    of: "bump-forward".into(),
                },
                notes: "bump sequence moves the source's sequence number 2^20 ahead".into(),
                fields: vec!["op.bump_sequence.bump_to".into()],
                flags: vec![],
                issues: None,
                must_reject: false,
            },
            Step::Tx {
                label: Some("negative".into()),
                source: "alice".into(),
                source_mux_id: None,
                operations: vec![json!({
                    "source_account": null,
                    "body": {"bump_sequence": {"bump_to": "-1"}}
                })],
                signers: vec![],
                memo: None,
                fee: None,
                cond: None,
                expect: TxExpect {
                    outcome: Outcome::Rejected,
                    result: Some(
                        json!({"tx_failed": [{"op_inner": {"bump_sequence": "bad_seq"}}]}),
                    ),
                },
                fee_bump: None,
            },
            Step::Capture {
                slug: "bump-negative".into(),
                select: Selection::Rejected {
                    of: "negative".into(),
                },
                notes: "bump sequence to -1, refused by core at submission".into(),
                fields: vec![],
                flags: vec![],
                issues: None,
                must_reject: false,
            },
            Step::Upload {
                label: "echo".into(),
                source: "bob".into(),
                wasm: "contracts/wasm/echo.wasm".into(),
                sha256: "a".repeat(64),
            },
            Step::Deploy {
                label: "echo-1".into(),
                source: "bob".into(),
                code: "echo".into(),
                salt: "0".repeat(64),
                constructor_args: vec![],
            },
            Step::Upgrade {
                label: Some("fee-up".into()),
                protocol_version: None,
                base_fee: Some(200),
                base_reserve: None,
                max_tx_set_size: None,
                max_soroban_tx_set_size: None,
                flags: None,
                config: vec![],
            },
            Step::Invoke {
                label: Some("call".into()),
                source: "bob".into(),
                contract: "echo-1".into(),
                function: "echo".into(),
                args: vec![json!({"address": "@muxed:alice:7"})],
                auth: Default::default(),
                resources: None,
                simulate: true,
                footprint: None,
                expect: TxExpect::default(),
            },
            Step::Close {
                label: None,
                order: vec![],
            },
            Step::Capture {
                slug: "call".into(),
                select: Selection::Transaction { of: "call".into() },
                notes: "an echo call".into(),
                fields: vec![],
                flags: vec![],
                issues: None,
                must_reject: false,
            },
        ],
    };
    holds(&r);
    r
}

#[test]
fn recipe_ir() {
    let r = recipe();
    holds(&r);

    let with = |f: &dyn Fn(&mut RecipeIr)| {
        let mut r = r.clone();
        f(&mut r);
        r
    };
    has_problem(
        &with(&|r| {
            r.steps.insert(
                4,
                Step::Tx {
                    label: None,
                    source: "carol".into(),
                    source_mux_id: None,
                    operations: vec![json!({})],
                    signers: vec![],
                    memo: None,
                    fee: None,
                    cond: None,
                    expect: TxExpect::default(),
                    fee_bump: None,
                },
            )
        }),
        "role carol is used before a fund step",
    );
    has_problem(
        &with(&|r| {
            r.steps.remove(3);
        }),
        "no ledger has closed since bump-forward",
    );
    has_problem(
        &with(&|r| {
            if let Step::Capture { select, .. } = &mut r.steps[6] {
                *select = Selection::Transaction {
                    of: "negative".into(),
                };
            }
        }),
        "only a rejected capture",
    );
    has_problem(
        &with(&|r| {
            if let Step::Deploy { code, .. } = &mut r.steps[8] {
                *code = "funded".into();
            }
        }),
        "funded is not an upload step",
    );
    has_problem(
        &with(&|r| {
            if let Step::Invoke { args, .. } = &mut r.steps[10] {
                args.push(json!("@contract:nowhere"));
                args.push(json!("@muxed:alice:x"));
            }
        }),
        "no earlier step is labelled nowhere",
    );
    has_problem(
        &with(&|r| {
            if let Step::Invoke { args, .. } = &mut r.steps[10] {
                args.push(json!("@muxed:alice:x"));
            }
        }),
        "does not parse",
    );
    has_problem(
        &with(&|r| {
            if let Step::Capture { slug, .. } = &mut r.steps[12] {
                *slug = "bump-forward".into();
            }
        }),
        "captured twice",
    );
    has_problem(
        &with(&|r| {
            r.steps.push(Step::Fund {
                label: None,
                roles: vec!["root".into()],
                balance: "1".into(),
            })
        }),
        "never funded",
    );
}

#[test]
fn recipe_steps_reject_unknown_fields() {
    let text = render(&recipe()).replacen(
        "\"step\": \"close\"",
        "\"step\": \"close\", \"count\": 2",
        1,
    );
    assert!(serde_json::from_str::<RecipeIr>(&text).is_err());
    let text = render(&recipe()).replacen("\"step\": \"close\"", "\"step\": \"wait\"", 1);
    assert!(serde_json::from_str::<RecipeIr>(&text).is_err());
}

#[test]
fn symbolic_references() {
    for s in [
        "@account:alice",
        "@muxed:alice:18446744073709551615",
        "@contract:echo-1",
        "@muxed-contract:echo-1:67",
        "@wasm:echo",
        "@seq:alice",
        "@seq:alice+1048576",
        "@ledger-start:3",
    ] {
        assert_eq!(SymRef::parse(s).unwrap().unwrap().to_string(), s);
    }
    assert_eq!(
        SymRef::parse("@@handle").unwrap().unwrap(),
        SymRef::Literal("@handle".into())
    );
    assert!(SymRef::parse("GABC").is_none());
    for bad in [
        "@muxed:alice:18446744073709551616",
        "@muxed-contract:echo-1",
        "@muxed-contract:echo-1:x",
        "@seq:alice+-1",
        "@account:Alice",
        "@ledger:3",
        "@account",
    ] {
        assert!(SymRef::parse(bad).unwrap().is_err(), "{bad}");
    }
}

#[test]
fn load_refuses_other_versions_and_bad_values() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("o.json");
    let mut l = ledger();
    l.schema_version = 2;
    std::fs::write(&path, render(&l)).unwrap();
    let err = load::<ObligationLedger>(&path).unwrap_err().to_string();
    assert!(
        err.contains("schema_version 2, this build reads 1"),
        "{err}"
    );
    let mut l = ledger();
    l.obligations[0].witnesses.clear();
    std::fs::write(&path, render(&l)).unwrap();
    let err = load::<ObligationLedger>(&path).unwrap_err().to_string();
    assert!(err.contains("no witness"), "{err}");
    assert!(write(&path, &l).is_err());
}

#[test]
fn committed_schemas_are_current() {
    let stale = shapekit_contracts::sync_schemas(&root().join("schema"), false).unwrap();
    assert!(
        stale.is_empty(),
        "stale: {stale:?}; run `shapekit contracts schema --write`"
    );
    assert_eq!(shapekit_contracts::all().len(), 7);
}

#[test]
fn a_fee_bump_names_funded_roles() {
    use shapekit_contracts::recipe_ir::FeeBump;
    let mut r = recipe();
    let Step::Tx { fee_bump, .. } = &mut r.steps[2] else {
        panic!("step 2 is the bump-forward tx");
    };
    *fee_bump = Some(FeeBump {
        source: "bob".into(),
        fee: Some(400),
        signers: vec![],
    });
    holds(&r);
    let with = |fb: FeeBump| {
        let mut r = recipe();
        if let Step::Tx { fee_bump, .. } = &mut r.steps[2] {
            *fee_bump = Some(fb);
        }
        r
    };
    has_problem(
        &with(FeeBump {
            source: "carol".into(),
            fee: None,
            signers: vec![],
        }),
        "role carol is used before a fund step",
    );
    has_problem(
        &with(FeeBump {
            source: "bob".into(),
            fee: Some(0),
            signers: vec!["dave".into()],
        }),
        "the fee bump fee must be positive",
    );
}

#[test]
fn undecodable_submissions_carry_no_result() {
    let undecodable = ProbeAttempt {
        variant: "ext discriminant 1".into(),
        submission: SubmitStatus::Undecodable,
        result_code: None,
        result_xdr: None,
        applied: ApplyStatus::NotApplied,
        witnesses: vec!["candidate:probe.ext-1".into()],
        carrier: None,
    };
    let t = ValidationTable {
        schema_version: 1,
        core: core(true),
        fields: vec![FieldVerdict {
            path: "TransactionEnvelope.tx_v0.tx.ext".into(),
            verdict: Verdict::Validated,
            attempts: vec![
                attempt(
                    "ext v0",
                    SubmitStatus::Pending,
                    "tx_success",
                    ApplyStatus::Succeeded,
                ),
                undecodable.clone(),
            ],
        }],
    };
    holds(&t);
    assert_eq!(
        serde_json::to_value(SubmitStatus::Undecodable).unwrap(),
        json!("undecodable")
    );
    let mut bad = t.clone();
    bad.fields[0].attempts[1] = ProbeAttempt {
        result_code: Some("tx_malformed".into()),
        result_xdr: Some("AAAAAAAAAGT////7AAAAAA==".into()),
        ..undecodable.clone()
    };
    has_problem(&bad, "returned no result");
    let mut bad = t.clone();
    bad.fields[0].attempts[1].applied = ApplyStatus::Failed;
    has_problem(&bad, "cannot have been applied");
}

#[test]
fn limits_past_an_xdr_bound_are_refused_by_the_decoder() {
    let mut t = LimitsTable {
        schema_version: 1,
        core: core(true),
        limits: vec![Limit {
            path: "TransactionEnvelope.v1.tx.operations".into(),
            measure: Some("operations in one transaction".into()),
            max_accepted: "100".into(),
            first_rejected: "101".into(),
            rejection: LimitRejection {
                stage: Stage::Decode,
                result_code: None,
            },
            network_settings: BTreeMap::new(),
            witnesses: vec![
                "candidate:probe.ops-100".into(),
                "candidate:probe.ops-101".into(),
            ],
        }],
    };
    holds(&t);
    assert_eq!(
        serde_json::to_value(Stage::Decode).unwrap(),
        json!("decode")
    );
    t.limits[0].rejection.result_code = Some("tx_malformed".into());
    has_problem(&t, "a decode rejection has no result code");
    t.limits[0].rejection = LimitRejection {
        stage: Stage::Submit,
        result_code: None,
    };
    has_problem(&t, "has no rejection result code");
}

#[test]
fn outcomes_compare_within_a_carrier() {
    let carried = |variant: &str, carrier: &str, code: &str| ProbeAttempt {
        carrier: Some(carrier.into()),
        ..attempt(variant, SubmitStatus::Pending, code, ApplyStatus::Succeeded)
    };
    // the mux id is never refused; the two carriers' bases end differently
    let mut t = ValidationTable {
        schema_version: 1,
        core: core(true),
        fields: vec![FieldVerdict {
            path: "MuxedAccountMed25519.id".into(),
            verdict: Verdict::NotValidated,
            attempts: vec![
                carried("payment 0", "payment", "tx_success"),
                carried("payment max", "payment", "tx_success"),
                carried("fee bump 0", "fee-bump", "tx_fee_bump_inner_success"),
                carried("fee bump max", "fee-bump", "tx_fee_bump_inner_success"),
            ],
        }],
    };
    holds(&t);
    t.fields[0].verdict = Verdict::Validated;
    has_problem(&t, "every attempt ends the same way");
    t.fields[0].verdict = Verdict::NotValidated;
    t.fields[0].attempts.pop();
    has_problem(
        &t,
        "in carrier fee-bump: a verdict needs at least two attempts",
    );
    t.fields[0].attempts.push(ProbeAttempt {
        carrier: Some("fee-bump".into()),
        ..attempt(
            "fee bump bad",
            SubmitStatus::Error,
            "tx_bad_auth",
            ApplyStatus::NotApplied,
        )
    });
    has_problem(
        &t,
        "in carrier fee-bump is not-validated, but its attempts end in 2",
    );
}

#[test]
fn soroban_auth_modes_and_footprint_steps() {
    let mut r = recipe();
    // the invoke at step 10 gets hand-written entries and resource overrides
    if let Step::Invoke {
        auth, resources, ..
    } = &mut r.steps[10]
    {
        *auth = AuthSpec::Entries {
            entries: vec![json!({"credentials": {"address": {"address": "@account:alice"}}})],
            sign: vec!["alice".into()],
        };
        *resources = Some(ResourceOverride {
            instructions: Some(1),
            disk_read_bytes: None,
            write_bytes: None,
            resource_fee: Some(0),
        });
    }
    let close_at = r.steps.len() - 2;
    r.steps.insert(
        close_at,
        Step::HostFunction {
            label: Some("create".into()),
            source: "bob".into(),
            function: json!({"create_contract": {"contract_id_preimage": {"address": {"address": "@account:bob", "salt": "00"}}, "executable": {"wasm": "@wasm:echo"}}}),
            auth: AuthSpec::Record {
                allow_non_root: true,
                address_v2: true,
                signers: BTreeMap::from([("alice".into(), vec![]), ("bob".into(), vec!["alice".into()])]),
                delegates: BTreeMap::new(),
            },
            resources: None,
            simulate: true,
            footprint: None,
            expect: TxExpect::default(),
        },
    );
    r.steps.insert(
        close_at,
        Step::ExtendTtl {
            label: Some("extend".into()),
            source: "bob".into(),
            keys: vec![json!({"contract_code": {"hash": "@wasm:echo"}})],
            extend_to: 1000,
            resources: None,
            expect: TxExpect::default(),
        },
    );
    r.steps.insert(
        close_at,
        Step::Restore {
            label: Some("restore".into()),
            source: "bob".into(),
            keys: vec![json!({"contract_code": {"hash": "@wasm:echo"}})],
            resources: None,
            expect: TxExpect::default(),
        },
    );
    if let Step::Upgrade {
        max_soroban_tx_set_size,
        ..
    } = &mut r.steps[9]
    {
        *max_soroban_tx_set_size = Some(5);
    }
    holds(&r);

    let text = render(&r);
    assert!(text.contains("\"mode\": \"record\""), "{text}");
    let unknown = text.replacen("\"mode\": \"record\"", "\"mode\": \"delegated\"", 1);
    assert!(serde_json::from_str::<RecipeIr>(&unknown).is_err());
    // an invoke without auth still reads, as source authorization
    let plain = render(&recipe());
    assert!(!plain.contains("\"mode\""));

    let mut bad = r.clone();
    if let Step::HostFunction { auth, .. } = &mut bad.steps[close_at + 2] {
        *auth = AuthSpec::Record {
            allow_non_root: false,
            address_v2: false,
            signers: BTreeMap::from([("carol".into(), vec![])]),
            delegates: BTreeMap::new(),
        };
    }
    has_problem(&bad, "role carol is used before a fund step");
    let mut bad = r.clone();
    if let Step::ExtendTtl {
        keys, extend_to, ..
    } = &mut bad.steps[close_at + 1]
    {
        keys.clear();
        *extend_to = 0;
    }
    has_problem(&bad, "the footprint needs at least one key");
    has_problem(&bad, "extend_to must be above 0");
    let mut bad = r.clone();
    if let Step::Invoke { auth, .. } = &mut bad.steps[10] {
        *auth = AuthSpec::Entries {
            entries: vec![],
            sign: vec!["dave".into()],
        };
    }
    has_problem(&bad, "auth mode entries lists no entry");
    has_problem(&bad, "role dave is used before a fund step");
}

#[test]
fn a_close_can_fix_the_order_of_what_it_applies() {
    let set_order = |r: &mut RecipeIr, order: &[&str]| {
        let Step::Close { order: o, .. } = &mut r.steps[11] else {
            panic!("step 11 is the last close");
        };
        *o = order.iter().map(|s| s.to_string()).collect();
    };
    let second = |memo: Option<serde_json::Value>| Step::Tx {
        label: Some("second".into()),
        source: "bob".into(),
        source_mux_id: None,
        operations: vec![
            json!({"source_account": null, "body": {"bump_sequence": {"bump_to": "1"}}}),
        ],
        signers: vec![],
        memo,
        fee: None,
        cond: None,
        expect: TxExpect::default(),
        fee_bump: None,
    };
    let mut r = recipe();
    r.steps.insert(11, second(None));
    // the close moved to 12
    let Step::Close { order, .. } = &mut r.steps[12] else {
        panic!("step 12 is the last close");
    };
    *order = vec!["second".into(), "call".into()];
    holds(&r);
    assert!(render(&r).contains("\"order\""));
    assert!(!render(&recipe()).contains("\"order\""));

    let mut bad = recipe();
    set_order(&mut bad, &["nope"]);
    has_problem(&bad, "no earlier step is labelled nope");
    let mut bad = recipe();
    set_order(&mut bad, &["bump-forward"]);
    has_problem(&bad, "was queued before the previous close");
    let mut bad = recipe();
    set_order(&mut bad, &["call", "call"]);
    has_problem(&bad, "order names call twice");
    let mut bad = recipe();
    set_order(&mut bad, &["negative"]);
    has_problem(&bad, "is not a transaction step this close applies");
    let mut bad = recipe();
    bad.steps.insert(11, second(Some(json!({"text": "hi"}))));
    let Step::Close { order, .. } = &mut bad.steps[12] else {
        panic!("step 12 is the last close");
    };
    *order = vec!["second".into()];
    has_problem(&bad, "cannot carry a memo");
}

#[test]
fn an_untagged_core_names_its_commit_as_its_version() {
    let mut g = GateList {
        schema_version: 1,
        core: CoreRef {
            version: "362dbc5184c5fdc0d6e898f3046639dc5e2a4f32".into(),
            commit: "362dbc5184c5fdc0d6e898f3046639dc5e2a4f32".into(),
            image: None,
        },
        gates: vec![],
    };
    holds(&g);
    g.core.version = "30.0.1".into();
    has_problem(&g, "neither a release tag like v30.0.0 nor the commit");
}

#[test]
fn footprint_steps_take_resource_overrides() {
    use shapekit_contracts::recipe_ir::ResourceOverride;
    let low = ResourceOverride {
        instructions: Some(1),
        disk_read_bytes: Some(0),
        write_bytes: Some(0),
        resource_fee: Some(1),
    };
    for step in [
        Step::ExtendTtl {
            label: Some("extend".into()),
            source: "bob".into(),
            keys: vec![json!({"contract_code": {"hash": "@wasm:echo"}})],
            extend_to: 1000,
            resources: Some(low),
            expect: TxExpect {
                outcome: Outcome::Failed,
                result: None,
            },
        },
        Step::Restore {
            label: Some("restore".into()),
            source: "bob".into(),
            keys: vec![json!({"contract_code": {"hash": "@wasm:echo"}})],
            resources: Some(low),
            expect: TxExpect {
                outcome: Outcome::Failed,
                result: None,
            },
        },
    ] {
        let mut r = recipe();
        let at = r.steps.len() - 2;
        r.steps.insert(at, step);
        holds(&r);
        assert!(render(&r).contains("\"resource_fee\": 1"));
        let mut bad = r.clone();
        match &mut bad.steps[at] {
            Step::ExtendTtl { resources, .. } | Step::Restore { resources, .. } => {
                resources.as_mut().unwrap().resource_fee = Some(-1);
            }
            _ => unreachable!(),
        }
        has_problem(&bad, "the resource fee cannot be negative");
    }
}

// the committed recipes are formatted by hand and leave defaults out, so the check is that
// each reads, writes and reads back to the same recipe, and that writing it adds no
// emit_ledger_close_meta_ext_v1 key: a new option off by default changes no file
#[test]
fn committed_recipes_round_trip_unchanged() {
    let dir = root().join("corpus/recipes");
    let mut n = 0;
    for item in walkdir::WalkDir::new(&dir) {
        let item = item.unwrap();
        if item.path().extension().is_some_and(|x| x == "json") {
            let text = std::fs::read_to_string(item.path()).unwrap();
            let r: RecipeIr = serde_json::from_str(&text).unwrap();
            let written = render(&r);
            let back: RecipeIr = serde_json::from_str(&written).unwrap();
            assert_eq!(back, r, "{}", item.path().display());
            assert_eq!(render(&back), written, "{}", item.path().display());
            assert!(!written.contains("emit_ledger_close_meta_ext_v1"));
            assert!(!written.contains("emit_soroban_transaction_meta_ext_v1"));
            assert!(!written.contains("\"simulate\""));
            assert!(!written.contains("\"delegates\""));
            n += 1;
        }
    }
    assert!(n > 0, "no recipe under corpus/recipes");
}

#[test]
fn ledger_close_meta_ext_v1_is_written_only_when_set() {
    let mut r = recipe();
    assert!(!render(&r).contains("emit_ledger_close_meta_ext_v1"));
    r.node.emit_ledger_close_meta_ext_v1 = true;
    holds(&r);
    assert!(render(&r).contains("\"emit_ledger_close_meta_ext_v1\": true"));
}

#[test]
fn a_contract_reference_names_a_deploy_or_a_creating_host_function() {
    let host = |label: &str, function: serde_json::Value| Step::HostFunction {
        label: Some(label.into()),
        source: "bob".into(),
        function,
        auth: Default::default(),
        resources: None,
        simulate: true,
        footprint: None,
        expect: TxExpect::default(),
    };
    let calling = |contract: &str| Step::Invoke {
        label: None,
        source: "bob".into(),
        contract: format!("@contract:{contract}"),
        function: "balance".into(),
        args: vec![],
        auth: Default::default(),
        resources: None,
        simulate: true,
        footprint: None,
        expect: TxExpect::default(),
    };
    let with = |steps: Vec<Step>| {
        let mut r = recipe();
        let at = r.steps.len() - 2;
        for (i, s) in steps.into_iter().enumerate() {
            r.steps.insert(at + i, s);
        }
        r
    };
    let sac = json!({"create_contract": {"contract_id_preimage": {"asset": "native"}, "executable": "stellar_asset"}});
    holds(&with(vec![host("sac", sac.clone()), calling("sac")]));
    let v2 = json!({"create_contract_v2": {"contract_id_preimage": {"address": {"address": "@account:bob", "salt": "00"}}, "executable": {"wasm": "@wasm:echo"}, "constructor_args": []}});
    holds(&with(vec![host("made", v2), calling("made")]));
    // the deploy label still works
    holds(&with(vec![calling("echo-1")]));
    has_problem(
        &with(vec![calling("nowhere")]),
        "no earlier step is labelled nowhere",
    );
    let upload = json!({"upload_contract_wasm": "00"});
    has_problem(
        &with(vec![host("uploaded", upload), calling("uploaded")]),
        "uploaded is not a deploy step or a host-function step that creates a contract",
    );
}

#[test]
fn soroban_transaction_meta_ext_v1_is_written_only_when_set() {
    let mut r = recipe();
    assert!(!render(&r).contains("emit_soroban_transaction_meta_ext_v1"));
    r.node.emit_soroban_transaction_meta_ext_v1 = true;
    holds(&r);
    assert!(render(&r).contains("\"emit_soroban_transaction_meta_ext_v1\": true"));
}

/// The recipe with `step` inserted before its last two steps.
fn recipe_with(step: Step) -> RecipeIr {
    let mut r = recipe();
    let at = r.steps.len() - 2;
    r.steps.insert(at, step);
    r
}

fn invoke(
    simulate: bool,
    footprint: Option<Footprint>,
    resources: Option<ResourceOverride>,
    auth: AuthSpec,
) -> Step {
    Step::Invoke {
        label: None,
        source: "bob".into(),
        contract: "echo-1".into(),
        function: "echo".into(),
        args: vec![json!({"u32": 7})],
        auth,
        resources,
        simulate,
        footprint,
        expect: TxExpect::default(),
    }
}

fn full_resources() -> ResourceOverride {
    ResourceOverride {
        instructions: Some(1),
        disk_read_bytes: Some(0),
        write_bytes: Some(0),
        resource_fee: Some(100),
    }
}

fn footprint() -> Footprint {
    Footprint {
        read_only: vec![
            json!({"contract_data": {"contract": "@contract:echo-1", "key": "ledger_key_contract_instance", "durability": "persistent"}}),
        ],
        read_write: vec![],
    }
}

#[test]
fn a_step_that_does_not_simulate_says_its_footprint_and_every_resource() {
    let ok = recipe_with(invoke(
        false,
        Some(footprint()),
        Some(full_resources()),
        AuthSpec::Source,
    ));
    holds(&ok);
    assert!(render(&ok).contains("\"simulate\": false"));
    // simulate: true is the default and is not written
    let simulated = recipe_with(invoke(true, None, None, AuthSpec::Source));
    holds(&simulated);
    assert!(!render(&simulated).contains("\"simulate\""));

    has_problem(
        &recipe_with(invoke(
            false,
            None,
            Some(full_resources()),
            AuthSpec::Source,
        )),
        "simulate: false needs an explicit footprint",
    );
    let partial = ResourceOverride {
        resource_fee: None,
        ..full_resources()
    };
    has_problem(
        &recipe_with(invoke(
            false,
            Some(footprint()),
            Some(partial),
            AuthSpec::Source,
        )),
        "needs instructions, disk_read_bytes, write_bytes and resource_fee",
    );
    has_problem(
        &recipe_with(invoke(false, Some(footprint()), None, AuthSpec::Source)),
        "needs instructions, disk_read_bytes, write_bytes and resource_fee",
    );
    has_problem(
        &recipe_with(invoke(true, Some(footprint()), None, AuthSpec::Source)),
        "a footprint is only written with simulate: false",
    );
    let record = AuthSpec::Record {
        allow_non_root: false,
        address_v2: false,
        signers: BTreeMap::new(),
        delegates: BTreeMap::new(),
    };
    has_problem(
        &recipe_with(invoke(
            false,
            Some(footprint()),
            Some(full_resources()),
            record,
        )),
        "auth mode record writes its entries by simulation",
    );
    // the same rules hold on a host-function step
    let host = Step::HostFunction {
        label: None,
        source: "bob".into(),
        function: json!({"upload_contract_wasm": "00"}),
        auth: AuthSpec::Source,
        resources: None,
        simulate: false,
        footprint: None,
        expect: TxExpect::default(),
    };
    has_problem(
        &recipe_with(host),
        "simulate: false needs an explicit footprint",
    );
}

#[test]
fn record_mode_delegates_name_existing_roles() {
    let with = |delegates: BTreeMap<String, Vec<String>>| {
        recipe_with(invoke(
            true,
            None,
            None,
            AuthSpec::Record {
                allow_non_root: false,
                address_v2: false,
                signers: BTreeMap::from([("bob".into(), vec![])]),
                delegates,
            },
        ))
    };
    let ok = with(BTreeMap::from([("bob".into(), vec!["alice".into()])]));
    holds(&ok);
    assert!(render(&ok).contains("\"delegates\""));
    assert!(!render(&with(BTreeMap::new())).contains("\"delegates\""));
    has_problem(
        &with(BTreeMap::from([("bob".into(), vec!["carol".into()])])),
        "role carol is used before a fund step",
    );
    has_problem(
        &with(BTreeMap::from([("carol".into(), vec!["alice".into()])])),
        "role carol is used before a fund step",
    );
}

#[test]
fn a_muxed_contract_reference_names_a_contract_an_earlier_step_made() {
    let calling = |to: &str| {
        recipe_with(Step::Invoke {
            label: None,
            source: "bob".into(),
            contract: "echo-1".into(),
            function: "echo".into(),
            args: vec![json!({"address": to})],
            auth: AuthSpec::Source,
            resources: None,
            simulate: true,
            footprint: None,
            expect: TxExpect::default(),
        })
    };
    holds(&calling("@muxed-contract:echo-1:67"));
    has_problem(
        &calling("@muxed-contract:nowhere:67"),
        "no earlier step is labelled nowhere",
    );
}
