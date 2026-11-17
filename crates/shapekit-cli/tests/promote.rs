//! `shapekit corpus promote` on a real synthetic entry, cap23.claimable-balances.synth-strkey-b-invalid,
//! with the divergence matrices a conformance run over a candidate manifest holding only that
//! entry wrote (tests/fixtures/promote/): stellar-xdr 30.0.0 and @stellar/stellar-sdk 17.4.0
//! pass it, go-stellar-sdk v0.9.0 diverges from the network on two cases (finding GO-1),
//! soroban-sdk 29.0.1 has no reader for its strkey kind, stellar_flutter_sdk 3.12.0 and
//! soneso/stellar-php-sdk 1.18.0 pass it, and the Python stellar-sdk 17.0.1 diverges on case 6
//! (a trailing =), a candidate-lane defect no finding in findings/triage.toml takes, while
//! java-stellar-sdk 6.0.0 passes it. So with python in the versions file the entry stays a
//! candidate; the tests of the promotion itself use a versions file without python and java.
//! The other sdks of the versions file have no fixture and are left out of the copy.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const ENTRY: &str = "cap23.claimable-balances.synth-strkey-b-invalid";

/// The sdks tests/fixtures/promote/ holds a matrix for.
const FIXTURE_SDKS: [&str; 6] = ["rust", "go", "js", "soroban", "dart", "php"];

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
/// file holds the sdks of FIXTURE_SDKS, and python and java only when `with_python`.
fn layout() -> tempfile::TempDir {
    layout_with(false)
}

fn layout_with(with_python: bool) -> tempfile::TempDir {
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
    // promotion asks every sdk of the versions file, so the copy keeps the sdks with a fixture
    // matrix, and python and java only when asked
    let mut versions: toml::Table =
        toml::from_str(&fs::read_to_string(r.join("conformance/versions.toml")).unwrap()).unwrap();
    versions["sdk"].as_table_mut().unwrap().retain(|k, _| {
        FIXTURE_SDKS.contains(&k) || (with_python && (k == "python" || k == "java"))
    });
    fs::write(
        d.join("conformance/versions.toml"),
        toml::to_string(&versions).unwrap(),
    )
    .unwrap();
    for sdk in ["rust", "go", "js"] {
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

    let out = promote(d, &[], &["rust", "go", "js", "soroban", "dart", "php"]);
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
fn a_python_divergence_no_finding_takes_keeps_the_entry_a_candidate() {
    let dir = layout_with(true);
    let d = dir.path();
    let out = promote(
        d,
        &[],
        &[
            "rust", "go", "js", "soroban", "dart", "php", "python", "java",
        ],
    );
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        text(&out).contains(
            "refused: python@17.0.1 answers diverges-from-network and no finding in findings/triage.toml takes it"
        ),
        "{}",
        text(&out)
    );
    assert_eq!(entries(d, "corpus"), 0);
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
    let dry = promote(
        d,
        &["--dry-run"],
        &["rust", "go", "js", "soroban", "dart", "php"],
    );
    assert_eq!(dry.status.code(), Some(0), "{}", text(&dry));
    assert_eq!(entries(d, "corpus"), 0);

    let out = promote(d, &[], &["rust", "go", "js", "soroban", "dart", "php"]);
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
    let again = promote(d, &[], &["rust", "go", "js", "soroban", "dart", "php"]);
    assert_eq!(again.status.code(), Some(2), "{}", text(&again));
}
