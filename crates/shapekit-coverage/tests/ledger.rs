//! The obligation ledger merge: fragments, gap rows and entry witnesses, on a small repository
//! layout built from the manifest crate's sample corpus.

use std::fs;
use std::path::{Path, PathBuf};

use shapekit_contracts::{ObligationKind, ObligationStatus};
use shapekit_coverage::ledger;

fn sample() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../shapekit-manifest/tests/fixtures/sample")
}

fn copy_dir(from: &Path, to: &Path) {
    for item in walkdir::WalkDir::new(from) {
        let item = item.unwrap();
        let target = to.join(item.path().strip_prefix(from).unwrap());
        if item.file_type().is_dir() {
            fs::create_dir_all(&target).unwrap();
        } else {
            fs::copy(item.path(), &target).unwrap();
        }
    }
}

const FRAGMENT: &str = r#"{"schema_version":1,"obligations":[
{"id":"seed:js@17.4.0:a","kind":"seed","status":"open","evidence":"no entry changes"},
{"id":"seed:js@17.4.0:b","kind":"seed","status":"unmeetable","evidence":"equivalent mutant"}
]}"#;

/// A repository with the sample corpus, one fragment, one gap table, and the first corpus
/// entry naming `names` as its obligations.
fn layout(names: &[&str], listed: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    copy_dir(&sample(), &root.join("corpus"));
    fs::create_dir_all(root.join("conformance/seed-check")).unwrap();
    fs::write(
        root.join("conformance/seed-check/js.obligations.json"),
        FRAGMENT,
    )
    .unwrap();
    fs::create_dir_all(root.join("registry/gaps")).unwrap();
    fs::write(
        root.join("registry/gaps/js.toml"),
        "schema = 1\nlanguage = \"js\"\n[[gap]]\nid = \"nan\"\n[[gap]]\nid = \"lone-surrogate\"\nno_vector = \"no testee op writes it\"\n",
    )
    .unwrap();
    let files: Vec<String> = listed.iter().map(|f| format!("\"{f}\"")).collect();
    fs::write(
        root.join("registry/obligation-sources.toml"),
        format!("schema_version = 1\nfiles = [{}]\n", files.join(", ")),
    )
    .unwrap();
    let path = root.join("corpus/manifest.json");
    let mut m = shapekit_manifest::build::load(&path).unwrap();
    m.entries[0].obligations = names.iter().map(|s| s.to_string()).collect();
    fs::write(&path, shapekit_manifest::build::render(&m)).unwrap();
    dir
}

const LISTED: &[&str] = &["conformance/seed-check/js.obligations.json"];

#[test]
fn entries_meet_what_they_name_and_gap_rows_become_obligations() {
    let dir = layout(&["gap:js:nan", "seed:js@17.4.0:a"], LISTED);
    let root = dir.path();
    let (l, problems) = ledger::build(root, &root.join("corpus"), &root.join("registry")).unwrap();
    assert!(problems.is_empty(), "{problems:#?}");
    let by = |id: &str| l.obligations.iter().find(|o| o.id == id).unwrap();
    let entry = format!("entry:{}", {
        let m = shapekit_manifest::build::load(&root.join("corpus/manifest.json")).unwrap();
        m.entries[0].id.clone()
    });
    assert_eq!(by("gap:js:nan").status, ObligationStatus::Met);
    assert_eq!(by("gap:js:nan").witnesses, vec![entry.clone()]);
    // an entry carries its vectors now, so the evidence that none does is gone
    assert_eq!(by("gap:js:nan").evidence, None);
    assert_eq!(by("gap:js:lone-surrogate").status, ObligationStatus::Open);
    assert_eq!(
        by("gap:js:lone-surrogate").evidence.as_deref(),
        Some("no testee op writes it")
    );
    assert_eq!(by("seed:js@17.4.0:a").status, ObligationStatus::Met);
    assert_eq!(by("seed:js@17.4.0:b").status, ObligationStatus::Unmeetable);
    assert_eq!(l.count(ObligationKind::Seed, ObligationStatus::Met), 1);

    let text = ledger::render(&l);
    let back: shapekit_contracts::ObligationLedger = serde_json::from_str(&text).unwrap();
    assert_eq!(back, l);
    assert_eq!(text.lines().count(), l.obligations.len() + 2);
}

#[test]
fn a_witness_on_an_unmeetable_obligation_is_a_problem() {
    let dir = layout(&["seed:js@17.4.0:b"], LISTED);
    let root = dir.path();
    let (_, problems) = ledger::build(root, &root.join("corpus"), &root.join("registry")).unwrap();
    assert!(
        problems
            .iter()
            .any(|p| p.to_string().contains("is unmeetable in its fragment")),
        "{problems:#?}"
    );
}

#[test]
fn every_fragment_must_be_listed_and_entry_ids_need_a_ledger_kind() {
    let dir = layout(&["branch:x"], &[]);
    let root = dir.path();
    let (_, problems) = ledger::build(root, &root.join("corpus"), &root.join("registry")).unwrap();
    let text: Vec<String> = problems.iter().map(ToString::to_string).collect();
    assert!(
        text.iter()
            .any(|p| p
                .contains("conformance/seed-check/js.obligations.json is an obligation fragment")),
        "{text:#?}"
    );
    assert!(
        text.iter()
            .any(|p| p.contains("branch:x is not <kind>:<name>")),
        "{text:#?}"
    );
}

#[test]
fn every_gate_contributes_both_sides() {
    let dir = layout(&[], LISTED);
    let root = dir.path();
    // a witness must name a chain that exists
    fs::create_dir_all(root.join("corpus/chains/cap40.signed-payload-signer.p18")).unwrap();
    fs::create_dir_all(root.join("registry/generated")).unwrap();
    fs::write(
        root.join("registry/generated/gates.json"),
        r#"{"schema_version":1,"core":{"version":"v30.0.0","commit":"0e49751aa9ddf892302cca02cea7f3f5d97df9f1"},"gates":[
{"file":"src/transactions/SetOptionsOpFrame.cpp","line":308,"column":9,"function":"f","comparison":"is-before","version":19,
 "sides":[{"branch":"true","status":"met","witnesses":["chain:cap40.signed-payload-signer.p18"]},{"branch":"false","status":"open"}]}]}"#,
    )
    .unwrap();
    let (l, problems) = ledger::build(root, &root.join("corpus"), &root.join("registry")).unwrap();
    assert!(problems.is_empty(), "{problems:#?}");
    assert_eq!(l.count(ObligationKind::GateSide, ObligationStatus::Met), 1);
    assert_eq!(l.count(ObligationKind::GateSide, ObligationStatus::Open), 1);
    assert!(
        l.obligations
            .iter()
            .any(|o| o.id == "gate-side:src/transactions/SetOptionsOpFrame.cpp:308:9:false")
    );
}

#[test]
fn the_candidate_lane_contradicts_no_uncovered_record() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (report, problems) =
        shapekit_coverage::run(&root.join("corpus"), &root.join("registry")).unwrap();
    use shapekit_coverage::evidence::Standing;
    assert!(
        !report
            .uncovered_evidence
            .iter()
            .any(|r| r.standing == Standing::Contradicted),
        "{problems:?}"
    );
    // core refuses a malformed allow trust at submission, and a probe entry holds that result
    assert!(
        report
            .uncovered_evidence
            .iter()
            .any(|r| r.subject == "arm AllowTrustResult:malformed"
                && r.standing == Standing::ConfirmedByRefusal)
    );
}

/// `run` over the repository's corpus with a copy of its registry whose uncovered.toml gets
/// `extra` appended.
fn problems_with_uncovered(extra: &str) -> Vec<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = tempfile::tempdir().unwrap();
    copy_dir(&root.join("registry"), dir.path());
    let path = dir.path().join("uncovered.toml");
    let mut text = fs::read_to_string(&path).unwrap();
    text.push_str(extra);
    fs::write(&path, text).unwrap();
    let (_, problems) = shapekit_coverage::run(&root.join("corpus"), dir.path()).unwrap();
    problems.iter().map(|p| p.to_string()).collect()
}

#[test]
fn the_history_only_basic_value_is_covered_by_history_alone() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (report, problems) =
        shapekit_coverage::run(&root.join("corpus"), &root.join("registry")).unwrap();
    assert!(problems.is_empty(), "{problems:?}");
    let basic = report
        .arms
        .covered
        .iter()
        .find(|a| a.arm == "StellarValueExt:basic")
        .unwrap();
    assert_eq!(basic.locally_reachable, Some(false));
    assert!(
        basic
            .by_source_class
            .keys()
            .all(|c| c.ends_with("-history"))
    );
}

#[test]
fn a_history_only_arm_a_local_core_writes_is_refused() {
    let p = problems_with_uncovered(
        "\n[[history-only-arm]]\narm = \"StellarValueExt:signed_ms\"\nreason = \"r\"\n",
    );
    assert!(
        p.iter()
            .any(|x| x.contains("history-only-arm StellarValueExt:signed_ms")
                && x.contains("not public network history")),
        "{p:?}"
    );
}

#[test]
fn a_history_only_arm_nothing_covers_is_refused() {
    let p = problems_with_uncovered(
        "\n[[history-only-arm]]\narm = \"StellarValueExt:empty_tx_set\"\nreason = \"r\"\n",
    );
    assert!(
        p.iter().any(|x| x.contains("also an [[arm]] record")),
        "{p:?}"
    );
    assert!(
        p.iter().any(|x| x.contains("must be covered by history")),
        "{p:?}"
    );
}

#[test]
fn a_chain_id_under_both_corpus_and_candidate_is_ambiguous() {
    let dir = layout(&[], LISTED);
    let root = dir.path();
    let id = "cap01.bump-sequence.p30".to_string();
    fs::create_dir_all(root.join("corpus/chains").join(&id)).unwrap();
    let (_, before) = ledger::build(root, &root.join("corpus"), &root.join("registry")).unwrap();
    assert!(before.is_empty(), "{before:#?}");
    fs::create_dir_all(root.join("candidate/chains").join(&id)).unwrap();
    fs::create_dir_all(root.join("candidate/chains/explore.only-here")).unwrap();
    let (_, after) = ledger::build(root, &root.join("corpus"), &root.join("registry")).unwrap();
    let text: Vec<String> = after.iter().map(|p| p.to_string()).collect();
    assert_eq!(text.len(), 1, "{text:#?}");
    assert!(
        text[0].contains(&id) && text[0].contains("ambiguous"),
        "{text:#?}"
    );
}

#[test]
fn a_witness_naming_a_moved_recipe_or_chain_is_a_problem() {
    let dir = layout(
        &[],
        &[
            "conformance/seed-check/js.obligations.json",
            "conformance/w.obligations.json",
        ],
    );
    let root = dir.path();
    fs::create_dir_all(root.join("candidate/recipes/explore.witnesses/g2")).unwrap();
    fs::write(
        root.join("candidate/recipes/explore.witnesses/g2/r0000.json"),
        "{}",
    )
    .unwrap();
    fs::create_dir_all(root.join("corpus/chains/explore.g2.r0000")).unwrap();
    let fragment = |recipe: &str, chain: &str| {
        format!(
            r#"{{"schema_version":1,"obligations":[{{"id":"seed:js@17.4.0:w","kind":"seed","status":"met","witnesses":["chain:{chain}","recipe:{recipe}@p23"]}}]}}"#
        )
    };
    let write = |recipe: &str, chain: &str| {
        fs::write(
            root.join("conformance/w.obligations.json"),
            fragment(recipe, chain),
        )
        .unwrap()
    };
    write(
        "candidate/recipes/explore.witnesses/g2/r0000.json",
        "explore.g2.r0000",
    );
    let (_, ok) = ledger::build(root, &root.join("corpus"), &root.join("registry")).unwrap();
    assert!(ok.is_empty(), "{ok:#?}");
    // the paths before the move of explorer recipes from candidate/recipes/explore/
    write(
        "candidate/recipes/explore/g2/r0000.json",
        "explore.g2.r9999",
    );
    let (_, after) = ledger::build(root, &root.join("corpus"), &root.join("registry")).unwrap();
    let text: Vec<String> = after.iter().map(|p| p.to_string()).collect();
    assert_eq!(text.len(), 2, "{text:#?}");
    assert!(
        text.iter().any(|t| t.contains(
            "recipe:candidate/recipes/explore/g2/r0000.json@p23 names nothing that exists"
        )),
        "{text:#?}"
    );
    assert!(
        text.iter()
            .any(|t| t.contains("chain:explore.g2.r9999 names nothing that exists")),
        "{text:#?}"
    );
}

#[test]
fn a_path_witness_outside_the_repository_is_a_problem() {
    let dir = layout(
        &[],
        &[
            "conformance/seed-check/js.obligations.json",
            "conformance/w.obligations.json",
        ],
    );
    let root = dir.path();
    // each names a file or directory that exists, but outside the tree it is joined to
    fs::write(dir.path().join("outside.json"), "{}").unwrap();
    fs::create_dir_all(root.join("corpus/chains/inner")).unwrap();
    fs::write(
        root.join("conformance/w.obligations.json"),
        r#"{"schema_version":1,"obligations":[{"id":"seed:js@17.4.0:w","kind":"seed","status":"met","witnesses":["chain:../chains/inner","dataset:corpus/../../outside.json","recipe:/etc/hostname@p23"]}]}"#,
    )
    .unwrap();
    let (_, problems) = ledger::build(root, &root.join("corpus"), &root.join("registry")).unwrap();
    let text: Vec<String> = problems.iter().map(|p| p.to_string()).collect();
    assert_eq!(
        text.iter()
            .filter(|t| t.contains("is not a plain path inside the repository"))
            .count(),
        3,
        "{text:#?}"
    );
}
