//! A run with an input budget is reproducible from its seed with several workers. The other
//! side here is the rust 28.0.0 testee rather than go, so the test needs only cargo; it
//! speaks the same protocol and knows an older xdr, so real disagreement classes come up.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use shapekit_conformance::versions::{self, VersionsFile};
use shapekit_fuzz::types::FuzzType;
use shapekit_fuzz::{Config, report, run_against};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p.strip_prefix(dir).unwrap().to_string_lossy().into_owned();
                out.insert(rel, std::fs::read(&p).unwrap());
            }
        }
    }
    out
}

#[test]
fn two_runs_with_three_workers_write_equal_reports() {
    let root = root();
    let file = VersionsFile::load(&root.join("conformance/versions.toml")).unwrap();
    let other = file.target("rust", "28.0.0").unwrap();
    versions::build(&root, &other).unwrap();
    let cfg = Config {
        root: root.clone(),
        corpus: root.join("corpus"),
        versions: root.join("conformance/versions.toml"),
        go_version: None,
        build: false,
        // generous: a run the clock ends is not expected to reproduce
        seconds: 1200,
        max_inputs: Some(1500),
        seed: 7,
        jobs: 3,
        types: vec![
            FuzzType::ScVal,
            FuzzType::LedgerEntry,
            FuzzType::TransactionEnvelope,
            FuzzType::Asset,
        ],
        max_seed_bytes: 16 * 1024,
        max_arbitrary_bytes: 1024,
    };
    let known = BTreeMap::new();
    let mut outs = Vec::new();
    for _ in 0..2 {
        let outcome = run_against(&cfg, other.clone(), 29).unwrap();
        assert!(!outcome.time_cut, "the clock ended the run");
        assert_eq!(outcome.inputs(), 1500);
        let dir = tempfile::tempdir().unwrap();
        report::write(&cfg, &outcome, &known, dir.path(), None).unwrap();
        let mut f = files(dir.path());
        // the wall-clock figures are kept apart from the compared report on purpose
        assert!(f.remove("timing.json").is_some());
        f.remove("FUZZ.md");
        outs.push(f);
    }
    assert!(outs[0].contains_key("report.json"));
    let report: serde_json::Value = serde_json::from_slice(&outs[0]["report.json"]).unwrap();
    assert_eq!(report["reproducible"], true);
    assert!(report.get("seconds").is_none());
    // the class merge is what used to depend on arrival order, so the run must have some
    let classes = report["classes"].as_array().unwrap();
    assert!(!classes.is_empty(), "no disagreement class came up");
    let names: Vec<&String> = outs[0].keys().collect();
    assert_eq!(names, outs[1].keys().collect::<Vec<_>>());
    for (name, bytes) in &outs[0] {
        assert!(
            *bytes == outs[1][name],
            "{name} differs between two runs with one seed"
        );
    }
}
