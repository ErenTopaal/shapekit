//! `shapekit corpus promote` on a real synthetic entry, cap23.claimable-balances.synth-strkey-b-invalid,
//! with the divergence matrices a conformance run over a candidate manifest holding only that
//! entry wrote (tests/fixtures/promote/): stellar-xdr 30.0.0 and @stellar/stellar-sdk 17.4.0
//! pass it, go-stellar-sdk v0.9.0 diverges from the network on two cases (finding GO-1),
//! soroban-sdk 29.0.1 has no reader for its strkey kind, stellar_flutter_sdk 3.12.0 and
//! soneso/stellar-php-sdk 1.18.0 pass it, and the Python stellar-sdk 17.0.1 diverges on case 6
//! (a trailing =), a candidate-lane defect recorded in candidate/finding-candidates.toml, while
//! java-stellar-sdk 6.0.0 and kmp-stellar-sdk 1.18.0 pass it. stellar-dotnet-sdk 17.0.0
//! diverges on cases 13 and 15 (NET-STRKEY) and stellar-cli 30.0.1 on case 7 (CLI-STRKEY-TRIM),
//! both findings of candidate/findings/triage.toml. So with the real sdk set the entry is
//! promoted: go's failure is a main finding, the other three are held by candidate findings or
//! recorded. The chain tests use explorer entries of the pair chain pairs.muxed-payment.a, with
//! matrices the test writes in which every sdk passes them.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const ENTRY: &str = "cap23.claimable-balances.synth-strkey-b-invalid";

/// The sdks tests/fixtures/promote/ holds a matrix for: every sdk of conformance/versions.toml,
/// which `fixtures_cover_every_sdk` checks.
const FIXTURE_SDKS: [&str; 11] = [
    "rust",
    "go",
    "js",
    "soroban",
    "python",
    "java",
    "dart",
    "php",
    "dotnet",
    "kmp",
    "stellar-cli",
];

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(sdk: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/promote")
        .join(format!("{sdk}.json"))
        .display()
        .to_string()
}

fn shapekit(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_shapekit"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("running shapekit")
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
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

/// A repository layout with the entry in the candidate lane and an empty corpus, and the
/// known-failures lines for the entry taken out, as before it was ever promoted. The versions
/// file is the repository's.
fn layout() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let r = repo();
    copy_dir(&r.join("registry"), &d.join("registry"));
    copy_dir(&r.join("findings/issues"), &d.join("findings/issues"));
    fs::copy(
        r.join("findings/triage.toml"),
        d.join("findings/triage.toml"),
    )
    .unwrap();
    fs::copy(r.join("pins.toml"), d.join("pins.toml")).unwrap();
    candidate_findings(d);
    fs::create_dir_all(d.join("conformance/known-failures")).unwrap();
    fs::copy(
        r.join("conformance/versions.toml"),
        d.join("conformance/versions.toml"),
    )
    .unwrap();
    for sdk in [
        "rust",
        "go",
        "js",
        "python",
        "java",
        "dotnet",
        "stellar-cli",
    ] {
        let known = fs::read_to_string(r.join(format!("conformance/known-failures/{sdk}.txt")))
            .unwrap()
            .lines()
            .filter(|l| !l.starts_with(&format!("{ENTRY} ")))
            .map(|l| format!("{l}\n"))
            .collect::<String>();
        fs::write(
            d.join(format!("conformance/known-failures/{sdk}.txt")),
            known,
        )
        .unwrap();
    }
    let vector = format!("vectors/strkey/{ENTRY}.json");
    fs::create_dir_all(d.join("candidate/vectors/strkey")).unwrap();
    fs::create_dir_all(d.join("corpus")).unwrap();
    fs::copy(
        r.join("corpus").join(&vector),
        d.join("candidate").join(&vector),
    )
    .unwrap();
    for lane in ["candidate", "corpus"] {
        let out = shapekit(d, &["manifest", "build", "--corpus", lane]);
        assert!(out.status.success(), "{}", text(&out));
    }
    dir
}

/// The candidate lane's findings and records, as the repository has them.
fn candidate_findings(d: &Path) {
    let r = repo();
    fs::create_dir_all(d.join("candidate/findings")).unwrap();
    for f in [
        "candidate/findings/triage.toml",
        "candidate/finding-candidates.toml",
    ] {
        fs::copy(r.join(f), d.join(f)).unwrap();
    }
    copy_dir(
        &r.join("candidate/findings/issues"),
        &d.join("candidate/findings/issues"),
    );
}

/// Puts findings into the layout's findings/triage.toml that take the entry's failures on python,
/// dotnet and stellar-cli, with their drafts under findings/issues/, as moving those findings out
/// of the candidate lane would. dotnet brings its candidate draft; python's defect is a candidate
/// in candidate/finding-candidates.toml with no draft yet, and the stellar-cli finding is
/// recorded as intended with no draft, so the test writes those two.
fn triage_the_other_failures(d: &Path) {
    let r = repo();
    let draft = format!("dotnet-{ENTRY}.md");
    fs::copy(
        r.join("candidate/findings/issues").join(&draft),
        d.join("findings/issues").join(&draft),
    )
    .unwrap_or_else(|e| panic!("dotnet: {e}"));
    fs::write(
        d.join(format!("findings/issues/python-{ENTRY}.md")),
        "a strkey with a trailing = decodes (stellar_sdk/strkey.py _decode_check)\n",
    )
    .unwrap();
    fs::write(
        d.join(format!("findings/issues/stellar-cli-{ENTRY}.md")),
        "stellar strkey decode trims the text before decoding it (stellar-strkey src/cli/decode.rs)\n",
    )
    .unwrap();
    let mut triage = fs::read_to_string(d.join("findings/triage.toml")).unwrap();
    for (id, sdk, draft) in [
        ("NET-STRKEY", "dotnet", format!("dotnet-{ENTRY}.md")),
        (
            "CLI-STRKEY-TRIM",
            "stellar-cli",
            format!("stellar-cli-{ENTRY}.md"),
        ),
        ("PY-STRKEY-PAD", "python", format!("python-{ENTRY}.md")),
    ] {
        triage.push_str(&format!(
            "\n[[finding]]\nid = \"{id}\"\ntitle = \"{id}\"\ncomponent = \"{sdk}\"\nseverity = \"divergence\"\nstatus = \"draft\"\ndraft = \"findings/issues/{draft}\"\ncause = \"moved from the candidate lane for the test\"\n\n[[finding.conformance]]\nsdk = \"{sdk}\"\nentries = [\"{ENTRY}\"]\n"
        ));
    }
    fs::write(d.join("findings/triage.toml"), triage).unwrap();
}

fn promote(d: &Path, extra: &[&str], sdks: &[&str]) -> Output {
    let mut args = vec!["corpus", "promote", ENTRY];
    let paths: Vec<String> = sdks.iter().map(|s| fixture(s)).collect();
    for p in &paths {
        args.push("--matrix");
        args.push(p);
    }
    args.extend_from_slice(extra);
    shapekit(d, &args)
}

fn entries(d: &Path, lane: &str) -> usize {
    let m: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(d.join(lane).join("manifest.json")).unwrap())
            .unwrap();
    m["entries"].as_array().unwrap().len()
}

#[test]
fn a_gating_sdk_failure_no_finding_takes_keeps_the_entry_a_candidate() {
    let dir = layout();
    let d = dir.path();
    let triage = fs::read_to_string(d.join("findings/triage.toml")).unwrap();
    let start = triage.find("[[finding]]\nid = \"GO-1\"").unwrap();
    let end = start + 5 + triage[start + 5..].find("[[finding]]").unwrap();
    fs::write(
        d.join("findings/triage.toml"),
        format!("{}{}", &triage[..start], &triage[end..]),
    )
    .unwrap();

    let out = promote(d, &[], &FIXTURE_SDKS);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        text(&out).contains(&format!(
            "{ENTRY}: refused: go@v0.9.0 answers diverges-from-network; go is one of rust, go and js"
        )),
        "{}",
        text(&out)
    );
    assert_eq!((entries(d, "candidate"), entries(d, "corpus")), (1, 0));
    assert!(
        !fs::read_to_string(d.join("conformance/known-failures/go.txt"))
            .unwrap()
            .contains(ENTRY)
    );
}

#[test]
fn other_sdk_failures_held_or_recorded_are_promoted() {
    let dir = layout();
    let d = dir.path();
    let out = promote(d, &[], &FIXTURE_SDKS);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    for (key, how) in [
        ("go@v0.9.0", "is finding GO-1"),
        (
            "python@17.0.1",
            "is attributed in the candidate records (a candidate)",
        ),
        ("dotnet@17.0.0", "is held by candidate finding NET-STRKEY"),
        (
            "stellar-cli@30.0.1",
            "is held by candidate finding CLI-STRKEY-TRIM",
        ),
    ] {
        assert!(
            text(&out).contains(&format!("{ENTRY}: {key}: diverges-from-network {how}")),
            "{key}: {}",
            text(&out)
        );
    }
    for key in ["java@6.0.0", "kmp@1.18.0", "dart@3.12.0", "php@1.18.0"] {
        assert!(!text(&out).contains(key), "{key}: {}", text(&out));
    }
    assert_eq!((entries(d, "candidate"), entries(d, "corpus")), (0, 1));
    let known = |sdk: &str| {
        fs::read_to_string(d.join(format!("conformance/known-failures/{sdk}.txt"))).unwrap()
    };
    assert!(
        known("python").contains(&format!(
            "{ENTRY} 17.0.1 diverges-from-network candidate/finding-candidates.toml case 6/strkey_decode"
        )),
        "{}",
        known("python")
    );
    // NET-STRKEY is filed, so its line names the upstream issue, not the draft
    assert!(
        known("dotnet").contains(&format!(
            "{ENTRY} 17.0.0 diverges-from-network https://github.com/Beans-BV/dotnet-stellar-sdk/issues/278"
        )),
        "{}",
        known("dotnet")
    );
    for lane in ["corpus", "candidate"] {
        let file = format!("{lane}/manifest.json");
        let out = shapekit(d, &["manifest", "validate", "--file", &file]);
        assert!(out.status.success(), "{lane}: {}", text(&out));
    }
}

#[test]
fn an_unattributed_failure_of_another_sdk_keeps_the_entry_a_candidate() {
    let dir = layout();
    let d = dir.path();
    // the python record of the entry's trailing = is the only thing that takes that failure
    let records = fs::read_to_string(d.join("candidate/finding-candidates.toml")).unwrap();
    let records = records.replace(
        "entries = [\"cap23.claimable-balances.synth-strkey-b-invalid\", \"cap27.muxed-accounts.synth-strkey-m-invalid\"",
        "entries = [\"cap27.muxed-accounts.synth-strkey-m-invalid\"",
    );
    fs::write(d.join("candidate/finding-candidates.toml"), records).unwrap();
    let out = promote(d, &[], &FIXTURE_SDKS);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        text(&out).contains(&format!(
            "{ENTRY}: refused: python@17.0.1 answers diverges-from-network; no finding in findings/triage.toml or the candidate triage takes it and no record in candidate/finding-candidates.toml names it"
        )),
        "{}",
        text(&out)
    );
    assert!(
        text(&out).contains(&format!("{ENTRY} stays in the candidate lane")),
        "{}",
        text(&out)
    );
    assert_eq!(entries(d, "corpus"), 0);
}

#[test]
fn fixtures_cover_every_sdk() {
    let versions: toml::Table =
        toml::from_str(&fs::read_to_string(repo().join("conformance/versions.toml")).unwrap())
            .unwrap();
    let mut sdks: Vec<&str> = versions["sdk"]
        .as_table()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    sdks.sort_unstable();
    let mut fixtures = FIXTURE_SDKS.to_vec();
    fixtures.sort_unstable();
    assert_eq!(
        sdks, fixtures,
        "every sdk of versions.toml needs a fixture matrix"
    );
    for sdk in FIXTURE_SDKS {
        assert!(Path::new(&fixture(sdk)).is_file(), "{sdk}");
    }
}

#[test]
fn every_sdk_needs_a_result() {
    let dir = layout();
    let out = promote(dir.path(), &[], &["rust", "js"]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        text(&out).contains("no matrix has a result for"),
        "{}",
        text(&out)
    );
    assert_eq!(entries(dir.path(), "corpus"), 0);
}

#[test]
fn main_findings_take_a_failure_before_the_candidate_lane() {
    let dir = layout();
    let d = dir.path();
    triage_the_other_failures(d);
    let dry = promote(d, &["--dry-run"], &FIXTURE_SDKS);
    assert_eq!(dry.status.code(), Some(0), "{}", text(&dry));
    assert!(
        text(&dry).contains("is finding NET-STRKEY"),
        "{}",
        text(&dry)
    );
    assert_eq!(entries(d, "corpus"), 0);

    let out = promote(d, &[], &FIXTURE_SDKS);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(text(&out).contains("is finding GO-1"), "{}", text(&out));
    assert_eq!((entries(d, "candidate"), entries(d, "corpus")), (0, 1));
    assert!(
        d.join(format!("corpus/vectors/strkey/{ENTRY}.json"))
            .is_file()
    );
    assert!(
        !d.join(format!("candidate/vectors/strkey/{ENTRY}.json"))
            .exists()
    );
    let known = fs::read_to_string(d.join("conformance/known-failures/go.txt")).unwrap();
    assert!(
        known.contains(&format!(
            "{ENTRY} v0.9.0 diverges-from-network https://github.com/stellar/go-stellar-sdk/issues/6046 case 13/strkey_decode, case 15/strkey_decode"
        )),
        "{known}"
    );
    let known = fs::read_to_string(d.join("conformance/known-failures/dotnet.txt")).unwrap();
    assert!(
        known.contains(&format!(
            "{ENTRY} 17.0.0 diverges-from-network findings/issues/dotnet-{ENTRY}.md"
        )),
        "{known}"
    );
    for lane in ["corpus", "candidate"] {
        let file = format!("{lane}/manifest.json");
        let out = shapekit(d, &["manifest", "validate", "--file", &file]);
        assert!(out.status.success(), "{lane}: {}", text(&out));
    }
    let again = promote(d, &[], &FIXTURE_SDKS);
    assert_eq!(again.status.code(), Some(2), "{}", text(&again));
}

const PAIR_B: &str = "explore.pairs.muxed-payment-b-l64";

/// Two entries of side a after its first checkpoint (63), and one at ledger 2, before it.
const PAIR_ENTRIES: [&str; 3] = [
    "explore.pairs.muxed-payment-a-l64",
    "explore.pairs.muxed-payment-a-l65",
    "explore.pairs.muxed-payment-a-l2",
];

/// The layout with both sides of the muxed-payment pair in the candidate lane: their chains,
/// recipes, the pair's comparison and README, the lane's chain list naming the two chains,
/// and every entry of side a; and a gate list with a witness of each side's recipe.
fn pair_layout() -> tempfile::TempDir {
    let dir = layout();
    let d = dir.path();
    let r = repo();
    // the pair was promoted into the repository's corpus, all but its ledger 2 entries, so it
    // is put back into the candidate lane as it was before
    let lane_of = |rel: &str| {
        ["candidate", "corpus"]
            .into_iter()
            .map(|lane| r.join(lane).join(rel))
            .find(|p| p.exists())
            .unwrap_or_else(|| panic!("{rel} is in neither lane"))
    };
    for side in ["a", "b"] {
        let chain = format!("chains/pairs.muxed-payment.{side}");
        copy_dir(&lane_of(&chain), &d.join("candidate").join(&chain));
    }
    copy_dir(
        &lane_of("recipes/explore.pairs/muxed-payment"),
        &d.join("candidate/recipes/explore.pairs/muxed-payment"),
    );
    fs::copy(
        lane_of("recipes/explore.pairs/README.md"),
        d.join("candidate/recipes/explore.pairs/README.md"),
    )
    .unwrap();
    fs::write(
        d.join("candidate/recipes/chains.toml"),
        "schema = 1\n\n[[chain]]\nid = \"pairs.muxed-payment.a\"\nfamily = \"explore.pairs\"\nrecipe = \"explore.pairs/muxed-payment/a.json\"\n\n[[chain]]\nid = \"pairs.muxed-payment.b\"\nfamily = \"explore.pairs\"\nrecipe = \"explore.pairs/muxed-payment/b.json\"\n",
    )
    .unwrap();
    for id in PAIR_ENTRIES.iter().chain(&[PAIR_B]) {
        let rel = format!("entries/explore.pairs/{id}");
        let to = d.join("candidate").join(&rel);
        copy_dir(&lane_of(&rel), &to);
        let file = to.join("entry.json");
        let text = fs::read_to_string(&file).unwrap().replace(
            "corpus/recipes/explore.pairs/",
            "candidate/recipes/explore.pairs/",
        );
        let mut entry: serde_json::Value = serde_json::from_str(&text).unwrap();
        entry["source"].as_object_mut().unwrap().remove("recipe_ir");
        fs::write(&file, serde_json::to_string_pretty(&entry).unwrap()).unwrap();
    }
    fs::create_dir_all(d.join("registry/generated")).unwrap();
    fs::write(d.join("registry/generated/gates.json"), GATES_BEFORE).unwrap();
    let out = shapekit(d, &["manifest", "build", "--corpus", "candidate"]);
    assert!(out.status.success(), "{}", text(&out));
    dir
}

/// A gate list in the order `twin gate-witnesses` writes it, whose met side names both pair
/// recipes while they are candidates.
const GATES_BEFORE: &str = r#"{
  "schema_version": 1,
  "core": {
    "version": "v30.0.0",
    "commit": "0e49751aa9ddf892302cca02cea7f3f5d97df9f1"
  },
  "gates": [
    {
      "file": "src/transactions/PaymentOpFrame.cpp",
      "line": 10,
      "column": 5,
      "function": "PaymentOpFrame::doApply",
      "comparison": "starts-from",
      "version": 13,
      "sides": [
        {
          "branch": "true",
          "status": "met",
          "witnesses": [
            "recipe:candidate/recipes/explore.pairs/muxed-payment/a.json@p30",
            "recipe:candidate/recipes/explore.pairs/muxed-payment/b.json@p30"
          ],
          "evidence": "2 recipe rerun(s) ran the call"
        },
        {
          "branch": "false",
          "status": "open",
          "witnesses": [],
          "evidence": "no chain evaluates this condition"
        }
      ]
    }
  ]
}
"#;

/// The same list after recipe a moved to the corpus: only that witness changes, and the side's
/// witnesses stay sorted.
const GATES_AFTER: &str = r#"{
  "schema_version": 1,
  "core": {
    "version": "v30.0.0",
    "commit": "0e49751aa9ddf892302cca02cea7f3f5d97df9f1"
  },
  "gates": [
    {
      "file": "src/transactions/PaymentOpFrame.cpp",
      "line": 10,
      "column": 5,
      "function": "PaymentOpFrame::doApply",
      "comparison": "starts-from",
      "version": 13,
      "sides": [
        {
          "branch": "true",
          "status": "met",
          "witnesses": [
            "recipe:candidate/recipes/explore.pairs/muxed-payment/b.json@p30",
            "recipe:corpus/recipes/explore.pairs/muxed-payment/a.json@p30"
          ],
          "evidence": "2 recipe rerun(s) ran the call"
        },
        {
          "branch": "false",
          "status": "open",
          "witnesses": [],
          "evidence": "no chain evaluates this condition"
        }
      ]
    }
  ]
}
"#;

/// A matrix in which the current version of every sdk passes each of `ids`.
fn passing_matrix(d: &Path, ids: &[&str]) -> String {
    let versions: toml::Table =
        toml::from_str(&fs::read_to_string(repo().join("conformance/versions.toml")).unwrap())
            .unwrap();
    let mut cells = serde_json::Map::new();
    for (sdk, def) in versions["sdk"].as_table().unwrap() {
        let current = def["current"].as_str().unwrap();
        cells.insert(
            format!("{sdk}@{current}"),
            serde_json::json!({"result": "pass"}),
        );
    }
    let entries: Vec<_> = ids
        .iter()
        .map(|id| serde_json::json!({"id": id, "cells": cells}))
        .collect();
    let path = d.join("passing-matrix.json");
    fs::write(
        &path,
        serde_json::to_string(&serde_json::json!({
            "corpus_version": "test",
            "testees": [],
            "entries": entries,
            "known_failure_problems": [],
        }))
        .unwrap(),
    )
    .unwrap();
    path.display().to_string()
}

#[test]
fn an_entry_whose_chain_is_not_listed_stays_a_candidate() {
    let dir = pair_layout();
    let d = dir.path();
    fs::write(d.join("candidate/recipes/chains.toml"), "schema = 1\n").unwrap();
    let matrix = passing_matrix(
        d,
        &[PAIR_ENTRIES[0], PAIR_ENTRIES[1], PAIR_ENTRIES[2], PAIR_B],
    );
    let out = shapekit(
        d,
        &["corpus", "promote", PAIR_ENTRIES[0], "--matrix", &matrix],
    );
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        text(&out).contains(
            "its chain pairs.muxed-payment.a is not under corpus/chains and candidate/recipes/chains.toml does not list it"
        ),
        "{}",
        text(&out)
    );
    assert_eq!(entries(d, "corpus"), 0);
}

#[test]
fn a_broken_gate_list_stops_the_promotion_before_anything_moves() {
    let dir = pair_layout();
    let d = dir.path();
    let gates = d.join("registry/generated/gates.json");
    fs::write(
        &gates,
        GATES_BEFORE.replace("\"schema_version\": 1", "\"schema_version\": 9"),
    )
    .unwrap();
    let matrix = passing_matrix(d, &[PAIR_ENTRIES[0]]);
    let out = shapekit(
        d,
        &["corpus", "promote", PAIR_ENTRIES[0], "--matrix", &matrix],
    );
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(
        text(&out).contains("gates.json: schema_version 9"),
        "{}",
        text(&out)
    );
    assert_eq!(entries(d, "corpus"), 0);
    assert!(!d.join("corpus/chains/pairs.muxed-payment.a").exists());
    assert!(d.join("candidate/chains/pairs.muxed-payment.a").exists());
}

#[test]
fn a_listed_chain_moves_with_its_first_promoted_entry() {
    let dir = pair_layout();
    let d = dir.path();
    let matrix = passing_matrix(
        d,
        &[PAIR_ENTRIES[0], PAIR_ENTRIES[1], PAIR_ENTRIES[2], PAIR_B],
    );
    let out = shapekit(
        d,
        &[
            "corpus",
            "promote",
            PAIR_ENTRIES[0],
            PAIR_ENTRIES[1],
            "--matrix",
            &matrix,
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(
        text(&out).contains("moved chain pairs.muxed-payment.a and recipe explore.pairs/muxed-payment/a.json into corpus"),
        "{}",
        text(&out)
    );
    // the layout's strkey entry and side b's entry stay in the lane beside the third pair entry
    assert_eq!((entries(d, "candidate"), entries(d, "corpus")), (3, 2));
    for (path, there) in [
        (
            "corpus/chains/pairs.muxed-payment.a/testnet/chain.json",
            true,
        ),
        (
            "corpus/chains/pairs.muxed-payment.a/mainnet/chain.json",
            true,
        ),
        ("candidate/chains/pairs.muxed-payment.a", false),
        (
            "candidate/chains/pairs.muxed-payment.b/testnet/chain.json",
            true,
        ),
        ("corpus/recipes/explore.pairs/muxed-payment/a.json", true),
        (
            "candidate/recipes/explore.pairs/muxed-payment/a.json",
            false,
        ),
        ("candidate/recipes/explore.pairs/muxed-payment/b.json", true),
        // the pair's comparison and the family's README are copied, and kept in the lane
        // while side b is still there
        ("corpus/recipes/explore.pairs/muxed-payment/pair.json", true),
        (
            "candidate/recipes/explore.pairs/muxed-payment/pair.json",
            true,
        ),
        ("corpus/recipes/explore.pairs/README.md", true),
        ("candidate/recipes/explore.pairs/README.md", true),
    ] {
        assert_eq!(d.join(path).exists(), there, "{path}");
    }
    let corpus_index = fs::read_to_string(d.join("corpus/recipes/chains.toml")).unwrap();
    assert!(
        corpus_index.contains("id = \"pairs.muxed-payment.a\"\nfamily = \"explore.pairs\"\nrecipe = \"explore.pairs/muxed-payment/a.json\""),
        "{corpus_index}"
    );
    let candidate_index = fs::read_to_string(d.join("candidate/recipes/chains.toml")).unwrap();
    assert!(
        !candidate_index.contains("muxed-payment.a"),
        "{candidate_index}"
    );
    assert!(
        candidate_index.contains("muxed-payment.b"),
        "{candidate_index}"
    );
    // the promoted entries and the one left in the lane name the recipe where it is now
    for e in [
        format!(
            "corpus/entries/explore.pairs/{}/entry.json",
            PAIR_ENTRIES[0]
        ),
        format!(
            "candidate/entries/explore.pairs/{}/entry.json",
            PAIR_ENTRIES[2]
        ),
    ] {
        let entry: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(d.join(&e)).unwrap()).unwrap();
        assert_eq!(
            entry["source"]["recipe"], "corpus/recipes/explore.pairs/muxed-payment/a.json",
            "{e}"
        );
        assert_eq!(
            entry["source"]["recipe_ir"], "corpus/recipes/explore.pairs/muxed-payment/a.json",
            "{e}"
        );
        let notes = entry["notes"].as_str().unwrap();
        assert!(
            notes.contains("corpus/recipes/explore.pairs/README.md")
                && notes
                    .contains("recorded from corpus/recipes/explore.pairs/muxed-payment/a.json"),
            "{e}: {notes}"
        );
    }
    // byte for byte: the generator's field order survives and only the moved witness changes
    let gates = fs::read_to_string(d.join("registry/generated/gates.json")).unwrap();
    assert_eq!(gates, GATES_AFTER);
    for lane in ["corpus", "candidate"] {
        let file = format!("{lane}/manifest.json");
        let out = shapekit(d, &["manifest", "validate", "--file", &file]);
        assert!(out.status.success(), "{lane}: {}", text(&out));
    }

    // the entry at ledger 2 comes before the chain's first checkpoint, so an indexer that
    // bootstraps from a checkpoint could not replay the chain with it
    let out = shapekit(
        d,
        &["corpus", "promote", PAIR_ENTRIES[2], "--matrix", &matrix],
    );
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        text(&out).contains("its ledger 2 is not after its chain's first history checkpoint 63"),
        "{}",
        text(&out)
    );
    assert_eq!((entries(d, "candidate"), entries(d, "corpus")), (3, 2));
}

#[test]
fn a_dry_run_decides_several_entries_and_an_id_given_twice_is_an_error() {
    let dir = layout();
    let d = dir.path();
    let matrices: Vec<String> = FIXTURE_SDKS.iter().map(|s| fixture(s)).collect();
    let mut args = vec!["corpus", "promote", ENTRY, ENTRY];
    for p in &matrices {
        args.push("--matrix");
        args.push(p);
    }
    args.push("--dry-run");
    let out = shapekit(d, &args);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(text(&out).contains("is given twice"), "{}", text(&out));

    let mut unknown = args.clone();
    unknown[3] = "no.such.entry";
    let out = shapekit(d, &unknown);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(
        text(&out).contains("has no entry no.such.entry"),
        "{}",
        text(&out)
    );

    args.remove(3);
    let out = shapekit(d, &args);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(
        text(&out).contains(&format!(
            "{ENTRY} can be promoted\n1 of 1 can be promoted, 0 refused"
        )),
        "{}",
        text(&out)
    );
    assert_eq!(entries(d, "corpus"), 0);
}

#[test]
fn both_sides_of_a_pair_move_in_one_call() {
    let dir = pair_layout();
    let d = dir.path();
    let matrix = passing_matrix(
        d,
        &[PAIR_ENTRIES[0], PAIR_ENTRIES[1], PAIR_ENTRIES[2], PAIR_B],
    );
    let out = shapekit(
        d,
        &[
            "corpus",
            "promote",
            PAIR_ENTRIES[0],
            PAIR_ENTRIES[1],
            PAIR_ENTRIES[2],
            PAIR_B,
            "--matrix",
            &matrix,
        ],
    );
    // side a's entry at ledger 2 is refused, the other three move with both chains
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert_eq!((entries(d, "candidate"), entries(d, "corpus")), (2, 3));
    for side in ["a", "b"] {
        assert!(
            d.join(format!("corpus/chains/pairs.muxed-payment.{side}"))
                .is_dir()
        );
        assert!(
            !d.join(format!("candidate/chains/pairs.muxed-payment.{side}"))
                .exists()
        );
    }
    assert!(
        d.join("corpus/recipes/explore.pairs/muxed-payment/pair.json")
            .is_file()
    );
    assert!(d.join("corpus/recipes/explore.pairs/README.md").is_file());
    assert!(!d.join("candidate/recipes/explore.pairs").exists());
    let candidate_index = fs::read_to_string(d.join("candidate/recipes/chains.toml")).unwrap();
    assert!(!candidate_index.contains("[[chain]]"), "{candidate_index}");
}
