//! The report over the committed run outputs of findings/inputs/: its groups are the
//! findings record's, and a baseline classes changed rows as new and fixed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use shapekit_report::{GroupKind, Options, RunKind, build, html, resolve_input, summary};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn options(input: PathBuf) -> Options {
    Options {
        root: root(),
        input,
        triage: PathBuf::from("findings/triage.toml"),
        manifest: None,
        baseline: None,
        known_failures: None,
        site: shapekit_report::DEFAULT_SITE.into(),
    }
}

const HARNESS: &str = "findings/inputs/harness-wallet-backend-3e22515a.json";
const GO: &str = "findings/inputs/conformance-go.json";

/// Per finding id, the occurrences of one source the committed record holds.
fn recorded(source: &str) -> BTreeMap<String, usize> {
    let text = std::fs::read_to_string(root().join("findings/findings.json")).unwrap();
    let v: Value = serde_json::from_str(&text).unwrap();
    let mut out = BTreeMap::new();
    for f in v["findings"].as_array().unwrap() {
        let n = f["occurrences"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|o| o["source"] == source)
            .count();
        if n > 0 {
            out.insert(f["id"].as_str().unwrap().to_owned(), n);
        }
    }
    out
}

#[test]
fn harness_groups_are_the_findings_record() {
    let r = build(&options(root().join(HARNESS))).unwrap();
    assert_eq!(r.kind, RunKind::Harness);
    let got: BTreeMap<String, usize> = r
        .groups
        .iter()
        .map(|g| (g.id.clone(), g.occurrences.len()))
        .collect();
    assert_eq!(got, recorded("harness"));
    assert!(r.groups.iter().all(|g| g.kind == GroupKind::Finding));
    assert_eq!(r.failures, got.values().sum::<usize>());
    for g in &r.groups {
        let m = g.minimal_entry.as_deref().unwrap();
        assert!(g.entries.iter().any(|e| e == m), "{}: {m}", g.id);
        let cmd = g.reproduce.as_deref().unwrap();
        assert!(cmd.starts_with("shapekit check indexer --chain "), "{cmd}");
        assert!(g.page.as_deref().unwrap().ends_with(&format!("#{}", g.id)));
    }
    // every group whose entries name a field links the field table, and the checklist items
    // that name one of those fields
    let w4 = r.groups.iter().find(|g| g.id == "WB-W4").unwrap();
    assert!(w4.fields.iter().any(|f| f.id == "tx.memo.text"));
    assert!(
        w4.guide
            .iter()
            .any(|x| x.title == "Memo text is stored as bytes.")
    );
}

#[test]
fn conformance_against_its_known_failures() {
    let mut o = options(root().join(GO));
    o.known_failures = Some(root().join("conformance/known-failures"));
    let r = build(&o).unwrap();
    let got: BTreeMap<String, usize> = r
        .groups
        .iter()
        .map(|g| (g.id.clone(), g.occurrences.len()))
        .collect();
    let want: BTreeMap<String, usize> = recorded("conformance")
        .into_iter()
        .filter(|(id, _)| id.starts_with("GO-"))
        .collect();
    assert_eq!(got, want);
    let c = r.comparison.as_ref().unwrap();
    // the committed matrix and lists agree, which `findings check` also requires
    assert!(c.new.is_empty() && c.changed.is_empty(), "{c:?}");
    assert!(c.unchanged > 0);
    for g in &r.groups {
        assert!(
            g.reproduce
                .as_deref()
                .unwrap()
                .starts_with("shapekit check sdk --sdk go --version ")
        );
    }
}

#[test]
fn harness_baseline_new_fixed_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let text = std::fs::read_to_string(root().join(HARNESS)).unwrap();
    let mut v: Value = serde_json::from_str(&text).unwrap();
    let rows = v["entries"].as_array_mut().unwrap();
    let failing = rows
        .iter()
        .position(|r| r["verdict"] == "dropped" || r["verdict"] == "mishandled")
        .unwrap();
    let passing = rows.iter().position(|r| r["verdict"] == "pass").unwrap();
    let failing_id = (
        rows[failing]["entry"].clone(),
        rows[failing]["network"].clone(),
    );
    let passing_id = (
        rows[passing]["entry"].clone(),
        rows[passing]["network"].clone(),
    );
    // the baseline had the failing row passing and the passing row dropped
    rows[failing]["verdict"] = "pass".into();
    rows[failing].as_object_mut().unwrap().remove("reason");
    rows[failing]["problems"] = Value::Array(Vec::new());
    rows[passing]["verdict"] = "dropped".into();
    let base = tmp.path().join("harness-report.json");
    std::fs::write(&base, serde_json::to_string(&v).unwrap()).unwrap();

    let mut o = options(root().join(HARNESS));
    o.baseline = Some(tmp.path().to_path_buf());
    let r = build(&o).unwrap();
    let c = r.comparison.unwrap();
    assert_eq!(c.new.len(), 1);
    assert_eq!(c.fixed.len(), 1);
    assert!(c.changed.is_empty());
    assert_eq!(c.not_rerun, 0);
    assert_eq!(
        (
            Value::from(c.new[0].entry.clone()),
            Value::from(c.new[0].at.clone())
        ),
        failing_id
    );
    assert!(c.new[0].group.is_some());
    assert_eq!(
        (
            Value::from(c.fixed[0].entry.clone()),
            Value::from(c.fixed[0].at.clone())
        ),
        passing_id
    );
    assert_eq!(c.fixed[0].now.as_deref(), Some("pass"));

    let md = summary::render(&build(&o).unwrap(), Some("report.html"));
    assert!(md.starts_with(shapekit_report::SUMMARY_MARKER));
    assert!(md.contains("**1 new**"));
    assert!(md.contains("**1 fixed**"));
}

#[test]
fn a_run_compared_with_itself_has_nothing_new() {
    let mut o = options(root().join(HARNESS));
    o.baseline = Some(root().join(HARNESS));
    let c = build(&o).unwrap().comparison.unwrap();
    assert!(c.new.is_empty() && c.fixed.is_empty() && c.changed.is_empty());
    assert!(c.unchanged > 0);
}

#[test]
fn kinds_and_directories() {
    assert_eq!(
        resolve_input(&root().join(GO)).unwrap().0,
        RunKind::Conformance
    );
    let tmp = tempfile::tempdir().unwrap();
    std::fs::copy(root().join(HARNESS), tmp.path().join("harness-report.json")).unwrap();
    assert_eq!(resolve_input(tmp.path()).unwrap().0, RunKind::Harness);
    std::fs::copy(root().join(GO), tmp.path().join("divergence-matrix.json")).unwrap();
    assert!(resolve_input(tmp.path()).is_err());
    let mut o = options(root().join(HARNESS));
    o.baseline = Some(root().join(GO));
    assert!(
        build(&o).is_err(),
        "a conformance baseline for a harness run"
    );
}

#[test]
fn html_is_self_contained_and_escaped() {
    let r = build(&options(root().join(HARNESS))).unwrap();
    let page = html::render(&r);
    assert!(page.contains("Content-Security-Policy"));
    assert!(!page.contains("<script"));
    assert!(!page.contains("src=\""));
    assert!(!page.contains("<link"));
    assert_eq!(
        html::esc("<a href=\"x\">&'"),
        "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
    );
    for g in &r.groups {
        assert!(page.contains(&format!("id=\"{}\"", g.id)));
    }
}
