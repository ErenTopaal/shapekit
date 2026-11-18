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
        stderr(&out).contains("no LedgerCloseMeta entry"),
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
