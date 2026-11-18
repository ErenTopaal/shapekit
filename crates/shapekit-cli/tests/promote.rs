//! `shapekit corpus promote` on a real synthetic entry, cap23.claimable-balances.synth-strkey-b-invalid,
//! with the divergence matrices a conformance run over a candidate manifest holding only that
//! entry wrote (tests/fixtures/promote/): stellar-xdr 30.0.0 and @stellar/stellar-sdk 17.4.0
//! pass it, go-stellar-sdk v0.9.0 diverges from the network on two cases (finding GO-1),
//! soroban-sdk 29.0.1 has no reader for its strkey kind, stellar_flutter_sdk 3.12.0 and
//! soneso/stellar-php-sdk 1.18.0 pass it, and the Python stellar-sdk 17.0.1 diverges on case 6
//! (a trailing =), a candidate-lane defect no finding in findings/triage.toml takes, while
//! java-stellar-sdk 6.0.0 and kmp-stellar-sdk 1.18.0 pass it. stellar-dotnet-sdk 17.0.0
//! diverges on cases 13 and 15 (NET-STRKEY) and stellar-cli 30.0.1 on case 7 (CLI-STRKEY-TRIM),
//! both findings of the candidate lane. So with the real sdk set the entry stays a candidate;
//! the test that promotes it first moves the findings of those three sdks into
//! findings/triage.toml, as a promotion would need.

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

/// Puts findings into the layout's findings/triage.toml that take the entry's failures on python,
/// dotnet and stellar-cli, with their drafts under findings/issues/, as moving those findings out
/// of the candidate lane would. dotnet and stellar-cli bring their candidate drafts; python's
/// defect is a candidate in candidate/finding-candidates.toml with no draft yet, so the test
/// writes one.
fn triage_the_other_failures(d: &Path) {
    let r = repo();
    for (sdk, draft) in [
        ("dotnet", format!("dotnet-{ENTRY}.md")),
        ("stellar-cli", format!("stellar-cli-{ENTRY}.md")),
    ] {
        fs::copy(
            r.join("candidate/findings/issues").join(&draft),
            d.join("findings/issues").join(&draft),
        )
        .unwrap_or_else(|e| panic!("{sdk}: {e}"));
    }
    fs::write(
        d.join(format!("findings/issues/python-{ENTRY}.md")),
        "a strkey with a trailing = decodes (stellar_sdk/strkey.py _decode_check)\n",
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
fn a_failure_no_finding_takes_keeps_the_entry_a_candidate() {
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
        text(&out).contains(
            "refused: go@v0.9.0 answers diverges-from-network and no finding in findings/triage.toml takes it"
        ),
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
fn candidate_lane_divergences_keep_the_entry_a_candidate() {
    let dir = layout();
    let d = dir.path();
    let out = promote(d, &[], &FIXTURE_SDKS);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    for key in ["python@17.0.1", "dotnet@17.0.0", "stellar-cli@30.0.1"] {
        assert!(
            text(&out).contains(&format!(
                "refused: {key} answers diverges-from-network and no finding in findings/triage.toml takes it"
            )),
            "{key}: {}",
            text(&out)
        );
    }
    for key in ["java@6.0.0", "kmp@1.18.0", "dart@3.12.0", "php@1.18.0"] {
        assert!(!text(&out).contains(key), "{key}: {}", text(&out));
    }
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
fn a_triaged_failure_with_a_draft_is_promoted_with_its_known_failure() {
    let dir = layout();
    let d = dir.path();
    triage_the_other_failures(d);
    let dry = promote(d, &["--dry-run"], &FIXTURE_SDKS);
    assert_eq!(dry.status.code(), Some(0), "{}", text(&dry));
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
    for lane in ["corpus", "candidate"] {
        let file = format!("{lane}/manifest.json");
        let out = shapekit(d, &["manifest", "validate", "--file", &file]);
        assert!(out.status.success(), "{lane}: {}", text(&out));
    }
    let again = promote(d, &[], &FIXTURE_SDKS);
    assert_eq!(again.status.code(), Some(2), "{}", text(&again));
}

#[test]
fn an_entry_whose_chain_is_not_in_the_corpus_stays_a_candidate() {
    let dir = layout();
    let d = dir.path();
    let r = repo();
    let id = "cap01.bump-sequence.p30-bump-behind";
    copy_dir(
        &r.join("corpus/entries/cap01.bump-sequence").join(id),
        &d.join("candidate/entries/cap01.bump-sequence").join(id),
    );
    let out = shapekit(d, &["manifest", "build", "--corpus", "candidate"]);
    assert!(out.status.success(), "{}", text(&out));
    let paths: Vec<String> = FIXTURE_SDKS.iter().map(|s| fixture(s)).collect();
    let mut args = vec!["corpus", "promote", id, "--dry-run"];
    for p in &paths {
        args.push("--matrix");
        args.push(p);
    }
    let out = shapekit(d, &args);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        text(&out).contains("its chain cap01.bump-sequence.p30 is not under corpus/chains"),
        "{}",
        text(&out)
    );
}

#[test]
fn a_dry_run_decides_several_entries_with_one_load() {
    let dir = layout();
    let d = dir.path();
    let matrices: Vec<String> = FIXTURE_SDKS.iter().map(|s| fixture(s)).collect();
    let mut args = vec!["corpus", "promote", ENTRY, ENTRY];
    for p in &matrices {
        args.push("--matrix");
        args.push(p);
    }
    let moving = shapekit(d, &args);
    assert_eq!(moving.status.code(), Some(2), "{}", text(&moving));
    assert!(
        text(&moving).contains("several ids are for --dry-run"),
        "{}",
        text(&moving)
    );

    args.push("--dry-run");
    let out = shapekit(d, &args);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert_eq!(
        text(&out)
            .matches(&format!("{ENTRY} stays in the candidate lane"))
            .count(),
        2,
        "{}",
        text(&out)
    );

    triage_the_other_failures(d);
    let out = shapekit(d, &args);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert_eq!(
        text(&out)
            .matches(&format!("{ENTRY} can be promoted"))
            .count(),
        2,
        "{}",
        text(&out)
    );
    assert_eq!(entries(d, "corpus"), 0);

    let mut unknown = args.clone();
    unknown[3] = "no.such.entry";
    let out = shapekit(d, &unknown);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(
        text(&out).contains("has no entry no.such.entry"),
        "{}",
        text(&out)
    );
}
