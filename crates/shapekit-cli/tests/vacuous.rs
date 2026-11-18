//! A check that has nothing to check is an error, not a pass.

use std::process::{Command, Output};

fn shapekit(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_shapekit"))
        .args(args)
        .output()
        .unwrap()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn oracle_check_of_an_empty_stream() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("empty.xdr");
    std::fs::write(&file, b"").unwrap();
    let out = shapekit(&[
        "oracle",
        "check",
        "--file",
        file.to_str().unwrap(),
        "--passphrase",
        "Test SDF Network ; September 2015",
    ]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("holds no ledgers"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn laws_check_of_a_corpus_without_ledgers() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("manifest.json"), r#"{"entries":[]}"#).unwrap();
    let out = shapekit(&["laws", "check", "--corpus", tmp.path().to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("no LedgerCloseMeta entry with a variant"),
        "{}",
        stderr(&out)
    );
    // an entry whose variants are empty checks no law either
    let empty = tmp.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    std::fs::write(
        empty.join("manifest.json"),
        r#"{"entries":[{"id":"x","xdr_type":"LedgerCloseMeta","variants":{}}]}"#,
    )
    .unwrap();
    let out = shapekit(&["laws", "check", "--corpus", empty.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("no LedgerCloseMeta entry with a variant"),
        "{}",
        stderr(&out)
    );
    // options that only apply to --lcm are refused rather than ignored
    for extra in [&["--batch"][..], &["--passphrase", "x"][..]] {
        let mut args = vec!["laws", "check", "--corpus", tmp.path().to_str().unwrap()];
        args.extend_from_slice(extra);
        let out = shapekit(&args);
        assert_eq!(out.status.code(), Some(2), "{extra:?}");
        assert!(
            stderr(&out).contains("cannot be used with"),
            "{}",
            stderr(&out)
        );
    }
}

#[test]
fn verify_raw_order_does_not_count_a_copied_recording() {
    let tmp = tempfile::tempdir().unwrap();
    for corpus in ["a", "b"] {
        let dir = tmp.path().join(corpus).join("chains/c/testnet");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ledgers.xdr"), b"same bytes").unwrap();
    }
    // a chain only the first corpus holds
    let only = tmp.path().join("a/chains/d/testnet");
    std::fs::create_dir_all(&only).unwrap();
    std::fs::write(only.join("ledgers.xdr"), b"x").unwrap();
    let out = shapekit(&[
        "gen",
        "verify-raw-order",
        "--corpus",
        tmp.path().join("a").to_str().unwrap(),
        "--other",
        tmp.path().join("b").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("no chain is recorded independently"),
        "{}",
        stderr(&out)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("not in "), "{stdout}");
    assert!(
        stdout.contains("1 recordings are byte-identical"),
        "{stdout}"
    );
}

fn repo(rel: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

#[test]
fn goldens_agree_without_pairs() {
    // the manifest crate's sample corpus has no golden entry, so no family pairs
    let sample = repo("crates/shapekit-manifest/tests/fixtures/sample/manifest.json");
    let out = shapekit(&[
        "goldens",
        "agree",
        "--file",
        sample.to_str().unwrap(),
        "--registry",
        repo("registry").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(stderr(&out).contains("no pairs"), "{}", stderr(&out));
}

#[test]
fn contracts_validate_of_a_file_it_cannot_read() {
    let tmp = tempfile::tempdir().unwrap();
    let garbled = tmp.path().join("garbled.json");
    std::fs::write(&garbled, "{ not json").unwrap();
    let wrong = tmp.path().join("wrong.json");
    std::fs::write(&wrong, r#"{"schema_version": 1, "no_such_field": 1}"#).unwrap();
    let validate = |file: &std::path::Path| {
        shapekit(&[
            "contracts",
            "validate",
            "--contract",
            "obligations",
            "--file",
            file.to_str().unwrap(),
        ])
    };
    // missing or not JSON: an error; JSON that is not the contract: a failed check
    assert_eq!(
        validate(&tmp.path().join("missing.json")).status.code(),
        Some(2)
    );
    assert_eq!(validate(&garbled).status.code(), Some(2));
    assert_eq!(validate(&wrong).status.code(), Some(1));
}
