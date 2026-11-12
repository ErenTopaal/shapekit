use std::fs;
use std::path::{Path, PathBuf};

use shapekit_registry::Registry;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/good")
}

// copies the clean fixture registry and applies one textual edit to one file
fn registry_with(file: &str, from: &str, to: &str) -> (tempfile::TempDir, Registry) {
    let dir = tempfile::tempdir().unwrap();
    for entry in fs::read_dir(fixture_dir()).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), dir.path().join(entry.file_name())).unwrap();
    }
    if !file.is_empty() {
        let path = dir.path().join(file);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{from:?} not in {file}");
        fs::write(&path, text.replacen(from, to, 1)).unwrap();
    }
    let reg = Registry::load(dir.path()).unwrap();
    (dir, reg)
}

fn messages(reg: &Registry) -> Vec<String> {
    reg.check().iter().map(ToString::to_string).collect()
}

fn assert_one_problem(reg: &Registry, needle: &str) {
    let msgs = messages(reg);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(msgs[0].contains(needle), "{msgs:#?}");
}

#[test]
fn clean_fixture_passes() {
    let reg = Registry::load(&fixture_dir()).unwrap();
    assert_eq!(messages(&reg), Vec::<String>::new());
}

#[test]
fn family_naming_unknown_cap() {
    let (_d, reg) = registry_with(
        "families.toml",
        r#"caps = ["CAP-0040"]"#,
        r#"caps = ["CAP-0040", "CAP-0999"]"#,
    );
    assert_one_problem(&reg, "unknown cap CAP-0999");
}

#[test]
fn family_naming_unknown_field() {
    let (_d, reg) = registry_with(
        "families.toml",
        r#"fields = ["tx.extra-signers"]"#,
        r#"fields = ["tx.nope"]"#,
    );
    assert_one_problem(&reg, "unknown field tx.nope");
}

#[test]
fn field_without_citation() {
    let (_d, reg) = registry_with(
        "fields.toml",
        r#"{ repo = "stellar/stellar-core", ref = "v30.0.0", path = "src/transactions/TransactionFrame.cpp", symbol = "TransactionFrame::commonValidPreSeqNum", lines = "100-140" },"#,
        "",
    );
    assert_one_problem(&reg, "has no source citation");
}

#[test]
fn citation_with_empty_symbol() {
    let (_d, reg) = registry_with(
        "fields.toml",
        r#"symbol = "TransactionFrame::commonValidPreSeqNum""#,
        r#"symbol = " ""#,
    );
    assert_one_problem(&reg, "citation 0 has empty symbol");
}

#[test]
fn indexer_visible_cap_without_family() {
    let (_d, reg) = registry_with(
        "caps.toml",
        "indexer_visible = false",
        "indexer_visible = true",
    );
    assert_one_problem(
        &reg,
        "CAP-0059: indexer-visible cap is not referenced by any family",
    );
}

#[test]
fn hidden_cap_without_reason() {
    let (_d, reg) = registry_with(
        "caps.toml",
        r#"reason = "adds cost types only, no shape an indexer reads""#,
        "",
    );
    assert_one_problem(&reg, "indexer_visible is false but no reason is given");
}

#[test]
fn duplicate_ids() {
    let (_d, reg) = registry_with("caps.toml", "id = \"CAP-0089\"", "id = \"CAP-0040\"");
    let msgs = messages(&reg);
    assert!(
        msgs.iter()
            .any(|m| m.contains("CAP-0040: cap id is used more than once")),
        "{msgs:#?}"
    );
}

#[test]
fn lint_rule_ids_are_exact() {
    let (_d, reg) = registry_with("lint-rules.toml", "id = \"IDX003\"", "id = \"IDX004\"");
    let msgs = messages(&reg);
    assert_eq!(msgs.len(), 2, "{msgs:#?}");
    assert!(msgs.iter().any(|m| m.contains("rule IDX003 is missing")));
    assert!(msgs.iter().any(|m| m.contains("rule IDX004 is not one of")));
}

#[test]
fn insane_protocol_ranges() {
    let (_d, reg) = registry_with(
        "fields.toml",
        "protocol_min = 19",
        "protocol_min = 19\nprotocol_max = 18",
    );
    assert_one_problem(&reg, "protocol range 19..Some(18) is not sane");
    let (_d, reg) = registry_with(
        "families.toml",
        "protocol_valid = { min = 19 }",
        "protocol_valid = { min = 19, max = 3 }",
    );
    assert_one_problem(&reg, "protocol_valid 19..Some(3) is not sane");
    let (_d, reg) = registry_with(
        "families.toml",
        "protocol_introduced = 19",
        "protocol_introduced = 18",
    );
    assert_one_problem(
        &reg,
        "protocol_introduced 18 is before CAP-0040's protocol 19",
    );
}

#[test]
fn missing_families_file_is_a_problem_not_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    for name in [
        "caps.toml",
        "fields.toml",
        "caps-limits.toml",
        "lint-rules.toml",
    ] {
        fs::copy(fixture_dir().join(name), dir.path().join(name)).unwrap();
    }
    let reg = Registry::load(dir.path()).unwrap();
    assert!(reg.families.is_none());
    assert_eq!(
        messages(&reg),
        vec!["families.toml: families.toml missing".to_string()]
    );
}

#[test]
fn unknown_keys_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    for entry in fs::read_dir(fixture_dir()).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), dir.path().join(entry.file_name())).unwrap();
    }
    let path = dir.path().join("caps.toml");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(
        &path,
        text.replacen("status = \"Final\"", "status = \"Final\"\nstauts = 1", 1),
    )
    .unwrap();
    let err = Registry::load(dir.path()).unwrap_err().to_string();
    assert!(err.contains("stauts"), "{err}");
}

#[test]
fn limit_needs_a_derivation() {
    let (_d, reg) = registry_with(
        "caps-limits.toml",
        r#"derived_from = [
  { repo = "stellar/rs-soroban-env", ref = "v30.0.0", path = "soroban-env-host/src/host.rs", symbol = "DEFAULT_XDR_RW_LIMITS", value = 500 },
]"#,
        "derived_from = []",
    );
    assert_one_problem(&reg, "xdr-depth: derived_from names no upstream constant");
}

#[test]
fn network_setting_limit_must_cite_a_network() {
    let (_d, reg) = registry_with(
        "caps-limits.toml",
        r#"  { network = "testnet", rpc = "https://soroban-testnet.stellar.org", setting = "CONFIG_SETTING_CONTRACT_BANDWIDTH_V0.txMaxSizeBytes", value = "132096", read_at = "2026-11-12", ledger = 5694341 },
"#,
        "",
    );
    assert_one_problem(
        &reg,
        "network_setting is true but no network setting is cited",
    );
}

#[test]
fn derivation_with_an_unknown_key_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    for entry in fs::read_dir(fixture_dir()).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), dir.path().join(entry.file_name())).unwrap();
    }
    let path = dir.path().join("caps-limits.toml");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, text.replacen("measured_with", "measured_by", 1)).unwrap();
    assert!(Registry::load(dir.path()).is_err());
}
