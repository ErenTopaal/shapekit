//! `shapekit site data` over the committed corpus, findings and tables: the numbers it writes
//! must be the numbers of the files it read, and the record of what it read must hold.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn site_data(out: &Path, commit: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_shapekit"))
        .args(["site", "data", "--root"])
        .arg(repo_root())
        .arg("--out")
        .arg(out)
        .args(["--commit", commit])
        .output()
        .expect("running shapekit")
}

fn read_json(path: &Path) -> Value {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn len(v: &Value) -> usize {
    v.as_array().expect("an array").len()
}

#[test]
fn site_data_matches_the_committed_files() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("data");
    let run = site_data(&out, COMMIT);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let root = repo_root();

    // meta: the commit as passed, every output's digest, every source's digest
    let meta = read_json(&out.join("meta.json"));
    assert_eq!(meta["commit"], COMMIT);
    assert_eq!(meta["repo"], "ErenTopaal/shapekit");
    let outputs = meta["outputs"].as_array().unwrap();
    assert_eq!(outputs.len(), 5);
    for o in outputs {
        let file = o["file"].as_str().unwrap();
        let bytes = std::fs::read(out.join(file)).unwrap();
        assert_eq!(o["sha256"], shapekit_manifest::sha256_hex(&bytes), "{file}");
    }
    let sources = meta["sources"].as_array().unwrap();
    let paths: Vec<&str> = sources
        .iter()
        .map(|s| s["path"].as_str().unwrap())
        .collect();
    for want in [
        "corpus/manifest.json",
        "corpus/coverage/coverage.json",
        "registry/caps.toml",
        "registry/families.toml",
        "pins.toml",
        "findings/triage.toml",
        "findings/findings.json",
        "conformance/versions.toml",
        "conformance/reproductions.toml",
    ] {
        assert!(paths.contains(&want), "{want} is not recorded as a source");
    }
    for s in sources {
        let rel = s["path"].as_str().unwrap();
        let bytes = std::fs::read(root.join(rel)).unwrap();
        assert_eq!(s["sha256"], shapekit_manifest::sha256_hex(&bytes), "{rel}");
        assert_eq!(s["bytes"], bytes.len(), "{rel}");
    }

    // coverage and entries carry the counts of the report and the manifest
    let manifest = read_json(&root.join("corpus/manifest.json"));
    let report = read_json(&root.join("corpus/coverage/coverage.json"));
    let coverage = read_json(&out.join("coverage.json"));
    let entries = read_json(&out.join("entries.json"));
    assert_eq!(coverage["summary"], report["summary"]);
    assert_eq!(
        len(&coverage["caps"]),
        report["summary"]["caps"].as_u64().unwrap() as usize
    );
    assert_eq!(
        len(&coverage["families"]),
        report["summary"]["families"].as_u64().unwrap() as usize
    );
    assert_eq!(len(&entries["entries"]), len(&manifest["entries"]));
    assert_eq!(meta["corpus"]["entries"], len(&manifest["entries"]));
    let cap_entries: u64 = coverage["caps"]
        .as_array()
        .unwrap()
        .iter()
        .zip(report["caps"].as_array().unwrap())
        .map(|(a, b)| {
            assert_eq!(a["id"], b["id"]);
            assert_eq!(a["entries"], b["entries"]);
            a["entries"].as_u64().unwrap()
        })
        .sum();
    assert!(cap_entries > 0);

    // every file the site links to exists in the checkout
    let ids: std::collections::BTreeSet<&str> = entries["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            for f in e["files"].as_array().unwrap() {
                let p = f["path"].as_str().unwrap();
                assert!(root.join(p).is_file(), "{p} is linked but missing");
            }
            e["id"].as_str().unwrap()
        })
        .collect();

    // findings: the record's summary and one row per finding
    let record = read_json(&root.join("findings/findings.json"));
    let findings = read_json(&out.join("findings.json"));
    assert_eq!(findings["summary"], record["summary"]);
    assert_eq!(len(&findings["findings"]), len(&record["findings"]));
    for f in findings["findings"].as_array().unwrap() {
        if let Some(d) = f["draft"].as_str() {
            assert!(root.join(d).is_file(), "draft {d} is missing");
        }
    }

    // conformance: every matrix entry is a corpus entry, and cell counts add up
    let conformance = read_json(&out.join("conformance.json"));
    assert_eq!(len(&conformance["sdks"]), 3);
    for sdk in conformance["sdks"].as_array().unwrap() {
        let matrix = read_json(&root.join(sdk["matrix"].as_str().unwrap()));
        assert_eq!(
            sdk["entries"].as_array().unwrap().len(),
            len(&matrix["entries"])
        );
        let cells: usize = matrix["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["cells"].as_object().unwrap().len())
            .sum();
        let counted: u64 = sdk["versions"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|v| v["failing_cells_by_result"].as_object().unwrap().values())
            .map(|n| n.as_u64().unwrap())
            .sum();
        assert_eq!(counted as usize, cells, "{}", sdk["sdk"]);
        for e in sdk["entries"].as_array().unwrap() {
            let id = e["id"].as_str().unwrap();
            assert!(ids.contains(id), "matrix entry {id} is not in the manifest");
        }
    }
    let repro = read_json(&out.join("conformance.json"))["reproductions"].clone();
    assert!(len(&repro) > 0);

    // harness: the report's totals, and per family and network counts that sum to them
    let harness = read_json(&out.join("harness.json"));
    let run0 = &harness["runs"][0];
    let report = read_json(&root.join(run0["report"].as_str().unwrap()));
    assert_eq!(run0["totals"], report["totals"]);
    let summed: u64 = run0["families"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|f| f["verdicts"].as_object().unwrap().values())
        .map(|n| n.as_u64().unwrap())
        .sum();
    assert_eq!(summed, report["totals"]["entries"].as_u64().unwrap());

    // the same inputs give the same bytes
    let again = dir.path().join("again");
    assert!(site_data(&again, COMMIT).status.success());
    for o in outputs {
        let file = o["file"].as_str().unwrap();
        assert_eq!(
            std::fs::read(out.join(file)).unwrap(),
            std::fs::read(again.join(file)).unwrap(),
            "{file} differs between two runs"
        );
    }
}

#[test]
fn site_data_refuses_a_commit_that_is_not_a_hex_id() {
    let dir = tempfile::tempdir().unwrap();
    for bad in ["HEAD", "main", "0123", "g123456", "0123456789ABCDEF"] {
        let run = site_data(dir.path(), bad);
        assert_eq!(run.status.code(), Some(2), "{bad} was accepted");
    }
    assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
}
