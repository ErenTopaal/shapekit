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

/// [`recorded`] plus the failures the record lists as held by candidate findings, which a
/// report groups under their candidate finding like any other.
fn recorded_with_held(source: &str) -> BTreeMap<String, usize> {
    let mut out = recorded(source);
    let text = std::fs::read_to_string(root().join("findings/findings.json")).unwrap();
    let v: Value = serde_json::from_str(&text).unwrap();
    for (id, occs) in v["held"].as_object().unwrap() {
        let n = occs
            .as_array()
            .unwrap()
            .iter()
            .filter(|o| o["source"] == source)
            .count();
        if n > 0 {
            *out.entry(id.clone()).or_default() += n;
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
    let want: BTreeMap<String, usize> = recorded_with_held("conformance")
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
    assert!(md.starts_with("<!-- shapekit-report harness -->\n"));
    assert!(md.contains("**1 new**"));
    assert!(md.contains("**1 fixed**"));
}

// a pull request's run held to the expectations its default branch committed at another
// build: the gate (expect, regressions mode) and the comment (the baseline comparison) name
// the same entries
#[test]
fn regressions_gate_and_comment_agree() {
    use shapekit_harness::expect::{self, Mode};
    let tmp = tempfile::tempdir().unwrap();
    let text = std::fs::read_to_string(root().join(HARNESS)).unwrap();
    let run: shapekit_harness::report::Report = serde_json::from_str(&text).unwrap();
    let mut want = expect::from_report(&run).unwrap();
    want.target_version = "0000000000000000000000000000000000000000".into();
    let path = tmp.path().join("expected-verdicts.toml");
    let write = |w: &expect::Expectations| {
        std::fs::write(&path, expect::render(w, "test").unwrap()).unwrap();
    };

    // another build, the same verdicts: met, where exact fails on the build alone
    write(&want);
    let w = expect::load(&path).unwrap();
    assert!(expect::check(&run, &w, Mode::Regressions).met());
    assert_eq!(expect::check(&run, &w, Mode::Exact).failures.len(), 1);

    let failing: Vec<usize> = want
        .entries
        .iter()
        .enumerate()
        .filter(|(_, x)| x.network == "testnet" && expect::is_failure(&x.verdict))
        .map(|(i, _)| i)
        .collect();
    let passing = want
        .entries
        .iter()
        .position(|x| x.network == "testnet" && x.verdict == "pass")
        .unwrap();
    let (new_i, changed_i) = (failing[0], failing[1]);
    let new_id = want.entries[new_i].entry.clone();
    let changed_id = want.entries[changed_i].entry.clone();
    let fixed_id = want.entries[passing].entry.clone();
    assert!(new_id != changed_id && new_id != fixed_id && changed_id != fixed_id);
    // the default branch passed one entry the run now fails, failed another with another
    // verdict, and failed one the run now passes
    want.entries[new_i].verdict = "pass".into();
    want.entries[new_i].reason = None;
    let other = if want.entries[changed_i].verdict == "dropped" {
        ("mishandled", Some("halted".to_owned()))
    } else {
        ("dropped", None)
    };
    want.entries[changed_i].verdict = other.0.into();
    want.entries[changed_i].reason = other.1;
    want.entries[passing].verdict = "mishandled".into();
    want.entries[passing].reason = Some("wrong-value".into());
    write(&want);
    let w = expect::load(&path).unwrap();
    let o = expect::check(&run, &w, Mode::Regressions);
    assert_eq!(o.failures.len(), 2, "{o:?}");
    assert!(
        o.failures
            .iter()
            .any(|f| f.starts_with(&format!("{new_id} ")) && f.contains("new failure")),
        "{o:?}"
    );
    assert!(
        o.failures
            .iter()
            .any(|f| f.starts_with(&format!("{changed_id} ")) && f.contains("changed failure")),
        "{o:?}"
    );
    assert!(
        o.notes
            .iter()
            .any(|n| n.starts_with(&format!("{fixed_id} ")) && n.contains("fixed")),
        "{o:?}"
    );

    // only the fix: met, and still reported
    let mut only_fix = expect::from_report(&run).unwrap();
    only_fix.target_version = want.target_version.clone();
    only_fix.entries[passing] = want.entries[passing].clone();
    write(&only_fix);
    let o = expect::check(&run, &expect::load(&path).unwrap(), Mode::Regressions);
    assert!(o.met(), "{o:?}");
    assert!(o.notes.iter().any(|n| n.contains("fixed")), "{o:?}");

    write(&want);
    let mut opts = options(root().join(HARNESS));
    opts.baseline = Some(path.clone());
    let r = build(&opts).unwrap();
    let c = r.comparison.as_ref().unwrap();
    let ids = |l: &[shapekit_report::Change]| -> Vec<String> {
        l.iter().map(|x| x.entry.clone()).collect()
    };
    assert_eq!(ids(&c.new), vec![new_id.clone()]);
    assert_eq!(ids(&c.changed), vec![changed_id.clone()]);
    assert_eq!(ids(&c.fixed), vec![fixed_id.clone()]);
    let md = summary::render(&r, Some("report.html"));
    for id in [&new_id, &changed_id, &fixed_id] {
        assert!(md.contains(&format!("`{id}`")), "{id} not in\n{md}");
    }
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
        // the smallest entry links its page in the site's corpus explorer
        let m = g.minimal_entry.as_deref().unwrap();
        assert!(page.contains(&format!("https://shapekit.org/dashboard/corpus/{m}\"")));
    }
}
