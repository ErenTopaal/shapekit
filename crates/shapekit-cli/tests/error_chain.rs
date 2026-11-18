//! an error that reaches the top prints each of its causes once.

use std::process::Command;

#[test]
fn a_missing_file_prints_its_cause_once() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("missing.json");
    let out = Command::new(env!("CARGO_BIN_EXE_shapekit"))
        .args(["manifest", "validate", "--file"])
        .arg(&missing)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    let cause = std::io::Error::from_raw_os_error(2).to_string();
    assert!(err.starts_with("error: "), "{err}");
    assert_eq!(err.matches(&cause).count(), 1, "{err}");
}
