//! `shapekit doctor`: a missing program fails with how to install it, a present one passes.

use std::process::Command;

fn doctor(dir: &std::path::Path, args: &[&str]) -> (Option<i32>, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_shapekit"))
        .arg("doctor")
        .args(args)
        .current_dir(dir)
        // only the system directories, so the toolchains of this machine are not found
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn missing_and_present_programs() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("corpus")).unwrap();
    std::fs::write(tmp.path().join("corpus/manifest.json"), "{}").unwrap();
    std::fs::write(
        tmp.path().join("versions.toml"),
        r#"schema = 1

[sdk.demo]
package = "demo"
current = "1.0.0"
issues_repo = "owner/demo"
memory = { resource = "as", mib = 64 }

[[sdk.demo.versions]]
version = "1.0.0"
build = [
  { argv = ["no-such-tool-for-doctor", "build"] },
  { argv = ["sh", "-c", "true"] },
]
command = ["target/demo/testee"]
"#,
    )
    .unwrap();
    let (code, out) = doctor(
        tmp.path(),
        &["sdk", "--offline", "--versions-file", "versions.toml"],
    );
    assert_eq!(code, Some(1), "{out}");
    assert!(
        out.contains("FAIL  no-such-tool-for-doctor: not found (check sdk: demo)"),
        "{out}"
    );
    assert!(
        out.contains("install no-such-tool-for-doctor and put it on PATH"),
        "{out}"
    );
    assert!(out.contains("ok    sh: found (check sdk: demo)"), "{out}");
    // the testee command is a file the build writes, not a program to install
    assert!(!out.contains("target/demo/testee"), "{out}");
    assert!(
        out.contains("ok    corpus: corpus/manifest.json present"),
        "{out}"
    );
    assert!(out.contains("doctor: 1 missing"), "{out}");

    let (code, out) = doctor(tmp.path(), &["lint", "--linter", "no-such-linter"]);
    assert_eq!(code, Some(1), "{out}");
    assert!(
        out.contains("FAIL  indexerlint: no-such-linter not found (lint)"),
        "{out}"
    );
}

#[test]
fn a_linter_without_an_execute_bit_is_not_found() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("indexerlint"), "#!/bin/sh\n").unwrap();
    let (code, out) = doctor(tmp.path(), &["lint", "--linter", "./indexerlint"]);
    assert_eq!(code, Some(1), "{out}");
    assert!(
        out.contains("FAIL  indexerlint: ./indexerlint not found"),
        "{out}"
    );
    std::fs::set_permissions(
        tmp.path().join("indexerlint"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let (_, out) = doctor(tmp.path(), &["lint", "--linter", "./indexerlint"]);
    assert!(
        out.contains("ok    indexerlint: ./indexerlint found"),
        "{out}"
    );
}

#[test]
fn an_option_for_a_check_not_selected_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    for args in [
        &["lint", "--sdk", "go"][..],
        &["sdk", "--offline", "--adapter", "x"][..],
        &["sdk", "indexer", "--offline", "--linter", "x"][..],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_shapekit"))
            .arg("doctor")
            .args(args)
            .current_dir(tmp.path())
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("which you did not select"),
            "{args:?}"
        );
    }
}
