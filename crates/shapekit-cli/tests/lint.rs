//! `shapekit lint`: the module directory and a --linter path are taken as you give them.

use std::os::unix::fs::PermissionsExt;
use std::process::Command;

fn lint(cwd: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_shapekit"))
        .arg("lint")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap()
}

#[test]
fn a_missing_module_directory_is_named_as_such() {
    let tmp = tempfile::tempdir().unwrap();
    let out = lint(tmp.path(), &["no-such-module", "--linter", "/bin/true"]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("module directory no-such-module does not exist"),
        "{err}"
    );
}

#[test]
fn a_relative_linter_path_is_found_from_where_you_run() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("module")).unwrap();
    std::fs::create_dir(tmp.path().join("bin")).unwrap();
    let linter = tmp.path().join("bin/fake-lint");
    // records the directory it runs in, then reports no findings
    std::fs::write(&linter, "#!/bin/sh\npwd > \"$(dirname \"$0\")/ran-in\"\n").unwrap();
    std::fs::set_permissions(&linter, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = lint(tmp.path(), &["module", "--linter", "bin/fake-lint"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let ran_in = std::fs::read_to_string(tmp.path().join("bin/ran-in")).unwrap();
    assert!(ran_in.trim_end().ends_with("module"), "{ran_in}");
}
