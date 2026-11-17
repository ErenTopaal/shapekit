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
        "corpus/coverage/COVERAGE.md",
        "adapters/walletbackend/reproductions.toml",
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

    // every blocker the coverage names is explained, with the meaning of the committed report
    let coverage_md = std::fs::read_to_string(root.join("corpus/coverage/COVERAGE.md")).unwrap();
    let blockers = coverage["blockers"].as_array().unwrap();
    for b in blockers {
        let row = format!(
            "| `{}` | {} |",
            b["blocker"].as_str().unwrap(),
            b["meaning"].as_str().unwrap()
        );
        assert!(coverage_md.contains(&row), "{row} is not in COVERAGE.md");
    }
    let explained = |name: &str| blockers.iter().any(|b| b["blocker"] == name);
    for name in report["summary"]["arms_uncovered_by_blocker"]
        .as_object()
        .unwrap()
        .keys()
    {
        assert!(explained(name), "blocker {name} has no meaning");
    }

    // every file the site links to exists in the checkout
    let ids: std::collections::BTreeSet<&str> = entries["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            for f in e["files"].as_array().unwrap() {
                let p = f["path"].as_str().unwrap();
                assert!(root.join(p).is_file(), "{p} is linked but missing");
                // the explorer shows sizes and decoded json from these, so both are exact
                let size = std::fs::metadata(root.join(p)).unwrap().len();
                assert_eq!(f["bytes"].as_u64(), Some(size), "{p}");
                if let Some(x) = f["expected"].as_str() {
                    let size = std::fs::metadata(root.join(x)).unwrap().len();
                    assert_eq!(f["expected_bytes"].as_u64(), Some(size), "{x}");
                }
            }
            if let Some(r) = e["recipe_ir"].as_str() {
                assert!(root.join(r).is_file(), "recipe {r} is linked but missing");
            }
            e["id"].as_str().unwrap()
        })
        .collect();

    // the guide rows: every field an entry names is a field table row, and every checklist
    // item names only such rows
    let guide_fields: std::collections::BTreeSet<&str> = entries["guide"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["id"].as_str().unwrap())
        .collect();
    assert!(!guide_fields.is_empty());
    for e in entries["entries"].as_array().unwrap() {
        for f in e["fields"].as_array().unwrap() {
            assert!(guide_fields.contains(f.as_str().unwrap()), "{f}");
        }
    }
    for i in entries["guide"]["checklist"].as_array().unwrap() {
        for f in i["fields"].as_array().unwrap() {
            assert!(guide_fields.contains(f.as_str().unwrap()), "{f}");
        }
    }

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
    // an sdk's cells come from its own matrix and its share of any matrix over every sdk
    let matrices: Vec<Value> = std::fs::read_dir(root.join("findings/inputs"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("conformance-")
        })
        .map(|p| read_json(&p))
        .collect();
    assert!(
        matrices.iter().any(|m| {
            let t = m["testees"].as_array().unwrap();
            t.iter().any(|x| x["sdk"] != t[0]["sdk"])
        }),
        "no matrix spans sdks, so the merge is not exercised"
    );
    for sdk in conformance["sdks"].as_array().unwrap() {
        let prefix = format!("{}@", sdk["sdk"].as_str().unwrap());
        let mut entries = std::collections::BTreeSet::new();
        let mut cells = std::collections::BTreeSet::new();
        for m in &matrices {
            for e in m["entries"].as_array().unwrap() {
                for key in e["cells"].as_object().unwrap().keys() {
                    if key.starts_with(&prefix) {
                        entries.insert(e["id"].as_str().unwrap().to_owned());
                        cells.insert((e["id"].as_str().unwrap().to_owned(), key.clone()));
                    }
                }
            }
        }
        assert_eq!(sdk["entries"].as_array().unwrap().len(), entries.len());
        let cells = cells.len();
        let counted: u64 = sdk["versions"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|v| v["failing_cells_by_result"].as_object().unwrap().values())
            .map(|n| n.as_u64().unwrap())
            .sum();
        assert_eq!(counted as usize, cells, "{}", sdk["sdk"]);
        // failing checks by op count every check of the cells except the ones not answered
        for v in sdk["versions"].as_array().unwrap() {
            let by_op: u64 = v["failing_checks_by_op"]
                .as_object()
                .unwrap()
                .values()
                .map(|n| n.as_u64().unwrap())
                .sum();
            let by_result = v["checks_by_result"].as_object().unwrap();
            let failing: u64 = by_result
                .iter()
                .filter(|(k, _)| {
                    ["error", "diverges-from-network", "diverges-from-peers"].contains(&k.as_str())
                })
                .map(|(_, n)| n.as_u64().unwrap())
                .sum();
            assert_eq!(by_op, failing, "{} {}", sdk["sdk"], v["version"]);
            for k in by_result.keys() {
                assert!(
                    [
                        "error",
                        "diverges-from-network",
                        "diverges-from-peers",
                        "unsupported",
                        "reference"
                    ]
                    .contains(&k.as_str()),
                    "unexpected check result {k}"
                );
            }
        }
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

    // one verdict per replayed entry and network, the same as the report's rows
    let rows = report["entries"].as_array().unwrap();
    let verdicts = run0["verdicts"].as_object().unwrap();
    assert_eq!(run0["distinct_entries"], verdicts.len());
    let pairs: usize = verdicts
        .values()
        .map(|v| v.as_object().unwrap().len())
        .sum();
    assert_eq!(pairs, rows.len());
    for r in rows {
        let v = &verdicts[r["entry"].as_str().unwrap()][r["network"].as_str().unwrap()];
        assert_eq!(v["verdict"], r["verdict"]);
        assert_eq!(v["reason"], r.get("reason").cloned().unwrap_or(Value::Null));
    }
    let chains: u64 = run0["chains_by_network"]
        .as_object()
        .unwrap()
        .values()
        .map(|n| n.as_u64().unwrap())
        .sum();
    assert_eq!(chains as usize, len(&report["chains"]));

    // the wallet-backend reproduction table, one row per sighting, every named entry a corpus
    // entry, and each expectation paired with the committed run's verdict where it ran
    let table: toml::Value = toml::from_str(
        &std::fs::read_to_string(root.join("adapters/walletbackend/reproductions.toml")).unwrap(),
    )
    .unwrap();
    let rows = harness["reproductions"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), table["reproduction"].as_array().unwrap().len());
    for row in rows {
        for case in row["cases"].as_array().unwrap() {
            for x in case["expect"].as_array().unwrap() {
                let entry = x["entry"].as_str().unwrap();
                assert!(
                    ids.contains(entry),
                    "reproduction entry {entry} is not in the manifest"
                );
                for at in x["at_pin"].as_array().unwrap() {
                    assert_eq!(
                        at["verdict"],
                        verdicts[entry][row["network"].as_str().unwrap()]["verdict"]
                    );
                }
            }
        }
    }

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
