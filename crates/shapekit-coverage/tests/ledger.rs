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
