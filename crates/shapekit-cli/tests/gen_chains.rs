//! `gen chains`, `manifest build`, `manifest validate` and `gen
//! verify-repro` end to end on the cap40 recipe. Needs docker, the pinned
//! core image; the tests are ignored by default; run with `--include-ignored`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

fn shapekit(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_shapekit"))
        .args(args)
        .output()
        .expect("running shapekit")
}

fn ok(out: &Output) -> String {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0), "{text}");
    text
}

#[test]
#[ignore = "needs docker and the pinned stellar-core image; run with --include-ignored"]
fn cap40_chains_validate_and_reproduce() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = dir.path().join("corpus");
    let c = corpus.to_str().unwrap();
    let registry = repo("registry");
    let pins = repo("pins.toml");
    let (r, p) = (registry.to_str().unwrap(), pins.to_str().unwrap());
    let family = "cap40.signed-payload-signer";

    let text = ok(&shapekit(&[
        "gen",
        "chains",
        "--family",
        family,
        "--jobs",
        "4",
        "--corpus",
        c,
        "--registry",
        r,
        "--pins",
        p,
    ]));
    assert!(text.contains("p30-extra-signer-empty-payload"), "{text}");
    ok(&shapekit(&[
        "manifest", "build", "--corpus", c, "--pins", p,
    ]));
    // the committed corpus carries oracle files and verify-repro regenerates them, so this
    // corpus needs them too
    ok(&shapekit(&["oracle", "run", "--corpus", c]));
    let manifest = corpus.join("manifest.json");
    let text = ok(&shapekit(&[
        "manifest",
        "validate",
        "--file",
        manifest.to_str().unwrap(),
        "--registry",
        r,
    ]));
    assert!(text.contains("manifest ok: 36 entries"), "{text}");

    let m: serde_json::Value = serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    let entries = m["entries"].as_array().unwrap();
    let get = |id: &str| {
        entries
            .iter()
            .find(|e| e["id"] == format!("{family}.{id}"))
            .unwrap_or_else(|| panic!("no entry {id}"))
    };
    for p in ["p28", "p30"] {
        let twin = get(&format!("{p}-extra-signer-empty-payload"));
        assert_eq!(twin["expected"]["validity"], "invalid-by-core");
        assert_eq!(
            twin["source"]["core_rejection"]["result_code"],
            "tx_malformed"
        );
        let twin = get(&format!("{p}-set-options-empty-payload"));
        assert_eq!(twin["source"]["core_rejection"]["result_code"], "tx_failed");
        // the account entry holds no passphrase-bound bytes
        assert_eq!(
            get(&format!("{p}-account-four-payload-signers"))["passphrase_dependent"],
            false
        );
        assert_eq!(
            get(&format!("{p}-authorised-by-payload-32"))["source"]["normalization"],
            "key-stable"
        );
    }

    let text = ok(&shapekit(&[
        "gen",
        "verify-repro",
        "--family",
        family,
        "--jobs",
        "4",
        "--corpus",
        c,
        "--registry",
        r,
        "--pins",
        p,
    ]));
    assert!(
        text.contains("0 differences over 4 chains and 36 entries"),
        "{text}"
    );
}

#[test]
#[ignore = "needs docker and the pinned stellar-core image; run with --include-ignored"]
fn cap01_ir_recipe_reproduces_the_committed_chain() {
    let root = repo("");
    let out = Command::new(env!("CARGO_BIN_EXE_shapekit"))
        .current_dir(&root)
        .args([
            "gen",
            "run-ir",
            "corpus/recipes/cap01.bump-sequence/bump-sequence.json",
            "--network",
            "both",
        ])
        .output()
        .expect("running shapekit");
    let text = ok(&out);
    assert!(
        text.contains("0 differences over 2 chains and 8 entries"),
        "{text}"
    );
}
