use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn shapekit(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_shapekit"))
        .args(args)
        .output()
        .expect("running shapekit")
}

fn crate_fixture(krate: &str, rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(krate)
        .join(rel)
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn registry_check_passes_on_a_clean_registry() {
    let dir = crate_fixture("shapekit-registry", "tests/fixtures/good");
    let out = shapekit(&["registry", "check", "--dir", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    assert!(stdout(&out).starts_with("registry ok: 3 caps"));
}

#[test]
fn registry_check_prints_every_problem_and_fails() {
    let src = crate_fixture("shapekit-registry", "tests/fixtures/good");
    let dir = tempfile::tempdir().unwrap();
    for name in [
        "caps.toml",
        "fields.toml",
        "caps-limits.toml",
        "lint-rules.toml",
        "families.toml",
    ] {
        std::fs::copy(src.join(name), dir.path().join(name)).unwrap();
    }
    let families = dir.path().join("families.toml");
    let text = std::fs::read_to_string(&families).unwrap();
    let text = text
        .replace(
            r#"caps = ["CAP-0040"]"#,
            r#"caps = ["CAP-0040", "CAP-0999"]"#,
        )
        .replace(
            "SignerKey:ed25519_signed_payload",
            "SignerKey:ed25519_signed_payload#huge",
        );
    std::fs::write(&families, text).unwrap();
    let out = shapekit(&["registry", "check", "--dir", dir.path().to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(text.contains("unknown cap CAP-0999"), "{text}");
    assert!(text.contains("#huge is not one of"), "{text}");
    assert!(text.contains("2 problems"), "{text}");
}

#[test]
fn registry_check_refuses_fragments_the_schema_lacks() {
    let src = crate_fixture("shapekit-registry", "tests/fixtures/good");
    let dir = tempfile::tempdir().unwrap();
    for name in [
        "caps.toml",
        "fields.toml",
        "caps-limits.toml",
        "lint-rules.toml",
        "families.toml",
    ] {
        std::fs::copy(src.join(name), dir.path().join(name)).unwrap();
    }
    let families = dir.path().join("families.toml");
    let text = std::fs::read_to_string(&families).unwrap().replace(
        r#"signature = ["SignerKey:ed25519_signed_payload"]"#,
        r#"signature = ["SignerKey:ed25519_signed_payload", "ScError:cryptox", "ScErrorr:contract", "ContractEventType=nope", ".execution_stages#max", ".body:set_options.signer.key:ed25519_signed_payload.payload#max"]"#,
    );
    std::fs::write(&families, text).unwrap();
    let out = shapekit(&["registry", "check", "--dir", dir.path().to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    for want in [
        "ScError:cryptox: not an arm reachable from LedgerCloseMeta",
        "ScErrorr is not a type reachable",
        "nope is not a value of the XDR enum ContractEventType",
        ".execution_stages#max: #max needs",
        "4 problems",
    ] {
        assert!(text.contains(want), "missing {want}: {text}");
    }
}

#[test]
fn registry_check_load_error_exits_2() {
    let out = shapekit(&["registry", "check", "--dir", "/nonexistent"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn manifest_validate_on_the_sample_corpus() {
    let file = crate_fixture("shapekit-manifest", "tests/fixtures/sample/manifest.json");
    let registry = crate_fixture("shapekit-manifest", "tests/fixtures/registry");
    let out = shapekit(&[
        "manifest",
        "validate",
        "--file",
        file.to_str().unwrap(),
        "--registry",
        registry.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    assert_eq!(stdout(&out), "manifest ok: 3 entries\n");
}

#[test]
fn manifest_build_then_validate_catches_a_tampered_file() {
    let src = crate_fixture("shapekit-manifest", "tests/fixtures/sample");
    let dir = tempfile::tempdir().unwrap();
    for item in walkdir::WalkDir::new(&src) {
        let item = item.unwrap();
        let target = dir.path().join(item.path().strip_prefix(&src).unwrap());
        if item.file_type().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
        } else {
            std::fs::copy(item.path(), &target).unwrap();
        }
    }
    let pins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pins.toml");
    let out = shapekit(&[
        "manifest",
        "build",
        "--corpus",
        dir.path().to_str().unwrap(),
        "--pins",
        pins.to_str().unwrap(),
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        std::fs::read(dir.path().join("manifest.json")).unwrap(),
        std::fs::read(src.join("manifest.json")).unwrap()
    );

    let xdr = dir
        .path()
        .join("entries/cap15.fee-bump/cap15.fee-bump.testnet-5694308-soroban/testnet.xdr");
    std::fs::write(&xdr, b"not xdr").unwrap();
    let registry = crate_fixture("shapekit-manifest", "tests/fixtures/registry");
    let file = dir.path().join("manifest.json");
    let out = shapekit(&[
        "manifest",
        "validate",
        "--file",
        file.to_str().unwrap(),
        "--registry",
        registry.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout(&out).contains("has sha256"), "{}", stdout(&out));
}
