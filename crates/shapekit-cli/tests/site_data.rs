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
        "candidate/findings/findings.json",
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
    // the summary counts CAPs; the PRE-CAP row tags shapes older than every CAP and is no CAP
    let real_caps = coverage["caps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["id"] != "PRE-CAP")
        .count();
    assert_eq!(
        real_caps,
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

    // the arms that a ledger can hold: every arm minus the arms of the two blockers that no
    // ledger can hold. both blockers have a meaning, and the count is the report's own
    let summary = &report["summary"];
    for name in ["refused-before-ledger", "never-emitted"] {
        assert!(explained(name), "blocker {name} has no meaning");
    }
    let by_blocker = |name: &str| {
        summary["arms_uncovered_by_blocker"][name]
            .as_u64()
            .unwrap_or(0)
    };
    let all_arms = summary["arms_reachable"].as_u64().unwrap();
    let covered_arms = summary["arms_covered"].as_u64().unwrap();
    let ledger_arms = all_arms - by_blocker("refused-before-ledger") - by_blocker("never-emitted");
    assert!(covered_arms <= ledger_arms && ledger_arms <= all_arms);
    assert_eq!(
        summary["arms_uncovered"].as_u64().unwrap(),
        all_arms - covered_arms
    );

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
    // the candidate lane: its own record's summary and rows, beside the main record's
    let cand_record = read_json(&root.join("candidate/findings/findings.json"));
    let cand = &findings["candidates"];
    assert_eq!(cand["record"], "candidate/findings/findings.json");
    assert_eq!(cand["summary"], cand_record["summary"]);
    assert_eq!(len(&cand["findings"]), len(&cand_record["findings"]));
    for f in cand["findings"].as_array().unwrap() {
        if let Some(d) = f["draft"].as_str() {
            assert!(root.join(d).is_file(), "draft {d} is missing");
        }
    }

    // the landing page counts a finding as reported before only when shapekit did not file its
    // link: a finding of either lane whose link is an issue a filed finding holds is filed too
    let rows: Vec<&Value> = findings["findings"]
        .as_array()
        .unwrap()
        .iter()
        .chain(cand["findings"].as_array().unwrap())
        .collect();
    let own: std::collections::BTreeSet<&str> = rows
        .iter()
        .filter(|f| f["status"] == "filed")
        .filter_map(|f| f["link"].as_str())
        .collect();
    for f in &rows {
        if let Some(l) = f["link"].as_str() {
            assert!(
                f["status"] == "filed" || !own.contains(l),
                "{} is {} of {l}, an issue shapekit filed",
                f["id"],
                f["status"]
            );
        }
    }

    // the findings total every page shows: one count over both records, and every breakdown
    // the pages draw adds up to it
    let reported = &findings["reported"];
    let statuses: Vec<&str> = reported["statuses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();
    assert_eq!(statuses, ["filed", "duplicate-of", "private-report"]);
    let all: Vec<&Value> = findings["findings"]
        .as_array()
        .unwrap()
        .iter()
        .chain(cand["findings"].as_array().unwrap())
        .collect();
    let counted: Vec<&str> = all
        .iter()
        .filter(|f| statuses.contains(&f["status"].as_str().unwrap()))
        .map(|f| f["id"].as_str().unwrap())
        .collect();
    let total = reported["findings"].as_u64().unwrap() as usize;
    assert!(total > 0);
    assert_eq!(total, counted.len());
    let reported_ids: Vec<&str> = reported["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i.as_str().unwrap())
        .collect();
    assert_eq!(reported_ids, counted);
    let sum_of = |v: &Value| -> usize {
        v.as_object()
            .unwrap()
            .values()
            .map(|n| n.as_u64().unwrap() as usize)
            .sum()
    };
    assert_eq!(sum_of(&reported["by_status"]), total);
    assert_eq!(sum_of(&reported["by_severity"]), total);
    assert_eq!(sum_of(&reported["by_record"]), total);
    let by_component = reported["by_component"].as_object().unwrap();
    assert_eq!(by_component.values().map(sum_of).sum::<usize>(), total);
    assert_eq!(reported["components"], by_component.len());
    assert_eq!(sum_of(&reported["not_reported"]) + total, all.len());
    // each record's share is the reported part of that record's own summary
    for (path, summary) in [
        ("findings/findings.json", &record["summary"]),
        ("candidate/findings/findings.json", &cand_record["summary"]),
    ] {
        let own: u64 = statuses
            .iter()
            .map(|s| summary["by_status"][*s].as_u64().unwrap_or(0))
            .sum();
        assert_eq!(reported["by_record"][path], own, "{path}");
    }

    // every page shows the findings total from that one count: the landing page and the
    // dashboard call reportedFindings in site/src/lib/metrics.ts, which checks its rows against
    // reported.findings, and no page counts the findings of one record as the total
    let site = root.join("site/src");
    let read = |rel: &str| std::fs::read_to_string(site.join(rel)).unwrap();
    assert!(read("lib/metrics.ts").contains("f.reported.findings"));
    for page in [
        "landing/stats.ts",
        "pages/overview.tsx",
        "pages/findings.tsx",
    ] {
        assert!(
            read(page).contains("reportedFindings("),
            "{page} does not count the findings with reportedFindings"
        );
    }

    // the landing page and the dashboard show the same arm coverage: both call ledgerArms in
    // site/src/lib/metrics.ts, which takes out the arms of the two blockers that no ledger can
    // hold, and the dashboard's arms metric is its total
    let metrics = read("lib/metrics.ts");
    for name in ["refused-before-ledger", "never-emitted"] {
        assert!(metrics.contains(&format!(r#"by["{name}"]"#)));
    }
    assert!(metrics.contains("total: arms.total"));
    for page in ["landing/stats.ts", "pages/corpus.tsx"] {
        assert!(
            read(page).contains("ledgerArms("),
            "{page} does not count the arms with ledgerArms"
        );
    }
    assert!(read("routes/home.tsx").contains("s.arms.percent"));
    let mut stack = vec![site.clone()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap();
            let rel = p
                .strip_prefix(&site)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            for bad in [
                "findings.summary.findings",
                "findings.findings.length",
                "fs.findings",
            ] {
                assert!(
                    !text.contains(bad),
                    "{rel} shows {bad}, the findings of one record, not reported.findings"
                );
            }
        }
    }

    // conformance: every sdk of the versions file at every version, its failing cells exactly
    // the checks its known-failures list names for that version, which a conformance run
    // without known-failures problems fails and nothing else
    let conformance = read_json(&out.join("conformance.json"));
    let versions =
        shapekit_conformance::versions::VersionsFile::load(&root.join("conformance/versions.toml"))
            .unwrap();
    assert_eq!(len(&conformance["sdks"]), versions.sdk.len());
    let known_dir = root.join("conformance/known-failures");
    for (name, def) in &versions.sdk {
        let sdk = conformance["sdks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["sdk"] == name.as_str())
            .unwrap_or_else(|| panic!("{name} is not in conformance.json"));
        assert_eq!(sdk["current"], def.current.as_str(), "{name}");
        let lines = shapekit_conformance::known::load(&known_dir, name).unwrap();
        let file = format!("conformance/known-failures/{name}.txt");
        assert_eq!(
            sdk["known_failures_file"].as_str(),
            root.join(&file).is_file().then_some(file.as_str()),
            "{name}"
        );
        let json_versions = sdk["versions"].as_array().unwrap();
        assert_eq!(json_versions.len(), def.versions.len(), "{name}");
        let current: Vec<&Value> = json_versions
            .iter()
            .filter(|v| v["current"] == true)
            .collect();
        assert_eq!(current.len(), 1, "{name}");
        assert_eq!(current[0]["version"], def.current.as_str(), "{name}");
        for (v, def_v) in json_versions.iter().zip(&def.versions) {
            let version = def_v.version.as_str();
            assert_eq!(v["version"], version, "{name}");
            let key = format!("{name}@{version}");
            // what the list names for this version: entry, check and result
            let mut listed = std::collections::BTreeSet::new();
            for l in &lines {
                if l.version.as_deref().is_none_or(|lv| lv == version) {
                    for c in &l.checks {
                        listed.insert((l.entry.clone(), c.clone(), l.result.to_string()));
                    }
                }
            }
            let listed_entries: std::collections::BTreeSet<&str> =
                listed.iter().map(|(e, _, _)| e.as_str()).collect();
            let diverging: u64 = v["failing_cells_by_result"]
                .as_object()
                .unwrap()
                .values()
                .map(|n| n.as_u64().unwrap())
                .sum();
            assert_eq!(
                diverging as usize,
                listed_entries.len(),
                "{key}: diverging entries"
            );
            assert_eq!(v["failing_cells_with_known_issue"], diverging, "{key}");
            let mut shown = std::collections::BTreeSet::new();
            for e in sdk["entries"].as_array().unwrap() {
                let id = e["id"].as_str().unwrap();
                assert!(ids.contains(id), "entry {id} is not in the manifest");
                if let Some(cell) = e["cells"].get(&key) {
                    for k in cell["checks"].as_array().unwrap() {
                        let result = match k["failure"].as_str() {
                            Some(f) => format!("{}:{f}", k["result"].as_str().unwrap()),
                            None => k["result"].as_str().unwrap().to_owned(),
                        };
                        shown.insert((
                            id.to_owned(),
                            k["check"].as_str().unwrap().to_owned(),
                            result,
                        ));
                    }
                }
            }
            assert_eq!(shown, listed, "{key}: failing checks");
            let sum = |field: &str| -> u64 {
                v[field]
                    .as_object()
                    .unwrap()
                    .values()
                    .map(|n| n.as_u64().unwrap())
                    .sum()
            };
            assert_eq!(sum("failing_checks_by_op") as usize, listed.len(), "{key}");
            assert_eq!(
                sum("failing_checks_by_result") as usize,
                listed.len(),
                "{key}"
            );
        }
    }
    // the committed matrices of both lanes give the listed checks their detail
    let triage: toml::Value =
        toml::from_str(&std::fs::read_to_string(root.join("findings/triage.toml")).unwrap())
            .unwrap();
    let cand: toml::Value = toml::from_str(
        &std::fs::read_to_string(root.join(triage["candidates"].as_str().unwrap())).unwrap(),
    )
    .unwrap();
    let mut detailed = 0usize;
    for t in [&triage, &cand] {
        for rel in t["inputs"]["matrices"].as_array().unwrap() {
            let m = read_json(&root.join(rel.as_str().unwrap()));
            for e in m["entries"].as_array().unwrap() {
                for (key, cell) in e["cells"].as_object().unwrap() {
                    let name = key.split('@').next().unwrap();
                    let sdk = conformance["sdks"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|s| s["sdk"] == name)
                        .unwrap();
                    let shown = sdk["entries"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|x| x["id"] == e["id"])
                        .map(|x| &x["cells"][key]);
                    for k in cell["checks"].as_array().unwrap() {
                        if ["pass", "unsupported"].contains(&k["result"].as_str().unwrap()) {
                            continue;
                        }
                        let s = shown
                            .and_then(|c| c["checks"].as_array())
                            .and_then(|cs| cs.iter().find(|x| x["check"] == k["check"]))
                            .unwrap_or_else(|| {
                                panic!("{rel} {} {key} {} is not shown", e["id"], k["check"])
                            });
                        assert_eq!(s["result"], k["result"]);
                        assert_eq!(s["detail"], k["detail"]);
                        detailed += 1;
                    }
                }
            }
        }
    }
    assert!(detailed > 0);
    let repro = &conformance["reproductions"];
    assert!(len(repro) > 0);
    // every row whose versions the versions file lists is observed; only probes are not
    for r in repro.as_array().unwrap() {
        for side in ["fail", "pass"] {
            for (v, o) in r[side].as_object().unwrap() {
                let probe = len(&r["probes"]) > 0;
                assert_eq!(o == "not-recorded", probe, "{} {v}", r["id"]);
            }
        }
    }

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
