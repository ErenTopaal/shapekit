use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn shapekit(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_shapekit"))
        .args(args)
        .output()
        .expect("running shapekit")
}

fn repo_pins() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pins.toml")
}

#[test]
fn missing_file_is_an_error() {
    let out = shapekit(&["pins", "verify", "--file", "/nonexistent/pins.toml"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("reading /nonexistent/pins.toml"));
}

#[test]
fn invalid_file_fails_before_any_network_call() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pins.toml");
    std::fs::write(&path, "schema = 2\n").unwrap();
    let out = shapekit(&["pins", "verify", "--file", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unsupported schema 2"));
}

#[test]
fn registered_commands() {
    let out = shapekit(&["--help"]);
    let help = String::from_utf8_lossy(&out.stdout);
    let commands: Vec<&str> = help
        .split("Commands:")
        .nth(1)
        .unwrap()
        .split("Options:")
        .next()
        .unwrap()
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert_eq!(
        commands,
        [
            "pins",
            "registry",
            "manifest",
            "conformance",
            "oracle",
            "import",
            "goldens",
            "capture",
            "gen",
            "synth",
            "guide",
            "pack",
            "serve",
            "harness",
            "findings",
            "fuzz",
            "native-fuzz",
            "coverage",
            "site",
            "twin",
            "laws",
            "vectors",
            "audit",
            "schema",
            "contracts",
            "corpus",
            "probe",
            "coretest",
            "help",
        ]
    );
}

#[test]
#[ignore = "needs network access to github, docker hub and the package registries; run with --include-ignored"]
fn online_repo_pins_pass() {
    let out = shapekit(&["pins", "verify", "--file", repo_pins().to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "pins verify failed:\n{stdout}");
    assert!(stdout.contains(" 0 failed"));
}

#[test]
#[ignore = "needs network access to github, docker hub and the package registries; run with --include-ignored"]
fn online_altered_sha_fails() {
    let text = std::fs::read_to_string(repo_pins()).unwrap();
    let real = "0e49751aa9ddf892302cca02cea7f3f5d97df9f1";
    assert!(text.contains(real));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pins.toml");
    std::fs::write(
        &path,
        text.replacen(real, "0e49751aa9ddf892302cca02cea7f3f5d97df9f0", 1),
    )
    .unwrap();
    let out = shapekit(&["pins", "verify", "--file", path.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected a pin failure:\n{stdout}"
    );
    assert!(stdout.contains("FAIL  git       stellar-core  refs/tags/v30.0.0 resolves to 0e49751aa9ddf892302cca02cea7f3f5d97df9f1"));
    assert!(stdout.contains(" 1 failed"));
}
