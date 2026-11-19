//! The java and kmp builds check the jars maven copied into lib/ against the committed
//! lib.sha256. These tests run the committed build steps after `mvn` against a lib/ written
//! here, so a jar that is not in lib.sha256 fails the build like a changed or missing one.

use std::path::{Path, PathBuf};

use shapekit_conformance::versions::{self, Target, VersionsFile};
use shapekit_manifest::sha256_hex;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every java and kmp target with its build cut to the steps after maven, the ones that check
/// lib/.
fn check_targets() -> Vec<Target> {
    let file = VersionsFile::load(&root().join("conformance/versions.toml")).unwrap();
    let mut out = Vec::new();
    for sdk in ["java", "kmp"] {
        let s = file.sdk(sdk).unwrap();
        for v in &s.versions {
            let mut t = file.target(sdk, &v.version).unwrap();
            let after = t.build.iter().position(|s| s.argv[0] == "mvn").unwrap() + 1;
            t.build.drain(..after);
            assert!(!t.build.is_empty(), "{} has no step after mvn", t.key());
            out.push(t);
        }
    }
    assert!(out.len() >= 3, "expected the java and kmp versions, got {}", out.len());
    out
}

/// A scratch root holding the repository files the steps run (scripts named by a relative
/// argv[0]), a lib/ with `jars`, the jacoco agent next to it, and a lib.sha256 listing `listed`.
fn scratch(t: &Target, jars: &[(&str, &[u8])], listed: &[(&str, &[u8])]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for step in &t.build {
        let prog = Path::new(&step.argv[0]);
        if prog.components().count() > 1 {
            let to = dir.path().join(prog);
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::copy(root().join(prog), &to).unwrap();
        }
    }
    let build = dir.path().join(format!("target/testees/{}/{}", t.sdk, t.version));
    std::fs::create_dir_all(build.join("lib")).unwrap();
    std::fs::write(build.join("jacocoagent.jar"), b"agent").unwrap();
    for (name, body) in jars {
        std::fs::write(build.join("lib").join(name), body).unwrap();
    }
    let mut sums = String::new();
    for (name, body) in listed {
        sums.push_str(&format!("{}  {name}\n", sha256_hex(body)));
    }
    sums.push_str(&format!("{}  ../jacocoagent.jar\n", sha256_hex(b"agent")));
    let pin = dir
        .path()
        .join(format!("testees/{}/versions/{}", t.sdk, t.version));
    std::fs::create_dir_all(&pin).unwrap();
    std::fs::write(pin.join("lib.sha256"), sums).unwrap();
    dir
}

const A: (&str, &[u8]) = ("a-1.0.jar", b"a");
const B: (&str, &[u8]) = ("b-2.0.jar", b"b");

#[test]
fn lib_matching_the_pin_builds() {
    for t in check_targets() {
        let dir = scratch(&t, &[A, B], &[A, B]);
        versions::build(dir.path(), &t).unwrap_or_else(|e| panic!("{}: {e:#}", t.key()));
    }
}

#[test]
fn a_jar_lib_sha256_does_not_list_fails_the_build() {
    for t in check_targets() {
        let dir = scratch(&t, &[A, B, ("extra-9.9.jar", b"x")], &[A, B]);
        let err = versions::build(dir.path(), &t)
            .err()
            .unwrap_or_else(|| panic!("{}: an extra jar in lib/ passed", t.key()));
        assert!(format!("{err:#}").contains("building"), "{err:#}");
    }
}

#[test]
fn a_missing_or_changed_jar_fails_the_build() {
    for t in check_targets() {
        let dir = scratch(&t, &[A], &[A, B]);
        assert!(versions::build(dir.path(), &t).is_err(), "{}: missing jar", t.key());
        let dir = scratch(&t, &[A, ("b-2.0.jar", b"changed")], &[A, B]);
        assert!(versions::build(dir.path(), &t).is_err(), "{}: changed jar", t.key());
    }
}
