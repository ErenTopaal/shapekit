//! A failure no finding takes but a candidate finding does is held, not unmapped.

use std::path::{Path, PathBuf};

use shapekit_findings::collect::collect;
use shapekit_findings::triage::{self, Inputs, Triage};

const ENTRY: &str = "cap03.offer-liabilities.synth-textual-offer-id-noncanonical";
const GO_MATRIX: &str = "candidate/findings/inputs/conformance-go-textual-offer-id.json";
const CANDIDATES: &str = "candidate/findings/triage.toml";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A main triage that imports the go matrix covering the textual offer id entry and has no
/// finding of its own for it.
fn main_triage(candidates: Option<&str>) -> Triage {
    Triage {
        schema: 1,
        candidates: candidates.map(str::to_owned),
        inputs: Inputs {
            manifest: "corpus/manifest.json".into(),
            harness: vec![],
            matrices: vec![GO_MATRIX.into()],
            lint: vec![],
            import: vec![],
            dir: None,
        },
        targets: Default::default(),
        findings: vec![],
    }
}

#[test]
fn go_textual_id_failures_are_held_by_the_candidate_finding() {
    let c = collect(&root(), &main_triage(Some(CANDIDATES))).unwrap();
    assert!(c.unmapped.is_empty(), "unmapped: {:?}", c.unmapped);
    assert_eq!(c.summary.unmapped, 0);
    assert_eq!(c.held.keys().collect::<Vec<_>>(), ["GO-TEXTID"]);
    let held = &c.held["GO-TEXTID"];
    let versions: Vec<&str> = held.iter().map(|o| o.on.as_str()).collect();
    assert_eq!(
        versions,
        [
            "go@v0.7.1",
            "go@v0.7.2",
            "go@v0.7.3",
            "go@v0.8.0",
            "go@v0.9.0"
        ]
    );
    assert!(held.iter().all(|o| o.entry.as_deref() == Some(ENTRY)));
    assert_eq!(c.summary.held, 5);
    assert_eq!(c.summary.failures, 5);
    assert!(c.findings.is_empty());
}

#[test]
fn without_the_candidate_triage_the_same_failures_are_unmapped() {
    let c = collect(&root(), &main_triage(None)).unwrap();
    assert_eq!(c.summary.unmapped, 5);
    assert_eq!(c.summary.held, 0);
    assert!(c.held.is_empty());
}

#[test]
fn a_finding_of_the_triage_takes_a_failure_before_a_candidate_holds_it() {
    let candidates = triage::load(&root().join(CANDIDATES)).unwrap();
    let mut t = main_triage(Some(CANDIDATES));
    let mut def = candidates
        .findings
        .into_iter()
        .find(|f| f.id == "GO-TEXTID")
        .unwrap();
    def.id = "GO-PROMOTED".into();
    t.findings.push(def);
    let c = collect(&root(), &t).unwrap();
    assert!(c.held.is_empty());
    assert_eq!(c.findings[0].occurrences.len(), 5);
}

#[test]
fn a_candidate_id_that_is_also_a_finding_id_is_refused() {
    let candidates = triage::load(&root().join(CANDIDATES)).unwrap();
    let mut t = main_triage(Some(CANDIDATES));
    t.findings.extend(
        candidates
            .findings
            .into_iter()
            .filter(|f| f.id == "GO-TEXTID"),
    );
    let err = collect(&root(), &t).unwrap_err().to_string();
    assert!(err.contains("GO-TEXTID"), "{err}");
}
