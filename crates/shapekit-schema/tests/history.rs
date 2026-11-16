//! The schema history in schema/xdr/ and the fragment checker over it.

use std::path::PathBuf;

use shapekit_schema::fragment::Fragment;
use shapekit_schema::history::{History, diff};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn history() -> History {
    History::load(&root().join("schema/xdr")).unwrap()
}

#[test]
fn tags_load_in_release_order() {
    let h = history();
    let labels: Vec<&str> = h.tags.iter().map(|t| t.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "v20", "v20.1", "v21.0", "v21.1", "v22.0", "v23.0", "v24.0", "v25.0", "v26.0", "v27.0",
            "v28.0", "v30.0", "next"
        ]
    );
    assert_eq!(h.get("v30.0").unwrap().protocol, Some(30));
    assert_eq!(h.get("next").unwrap().protocol, None);
}

#[test]
fn every_family_fragment_is_producible_in_the_pinned_schema() {
    // the same fragments `shapekit registry check` validates against the stellar-xdr crate
    let text = std::fs::read_to_string(root().join("registry/families.toml")).unwrap();
    let families: toml::Table = text.parse().unwrap();
    let schema = shapekit_schema::Schema::pinned();
    let mut n = 0;
    for f in families["family"].as_array().unwrap() {
        for frag in f["signature"].as_array().unwrap() {
            let frag = frag.as_str().unwrap();
            let parsed = Fragment::parse(frag).unwrap();
            assert!(
                parsed.producible(&schema),
                "{frag} is not producible in v30.0"
            );
            n += 1;
        }
    }
    assert!(n > 300, "{n} fragments");
}

#[test]
fn impossible_fragments_are_refused() {
    let schema = shapekit_schema::Schema::pinned();
    for frag in [
        "SignerKey:no_such_arm",
        "NoSuchType:v0",
        ".memo:text.nothing",
        "Memo:none.anything",
        ".fee#max",
        ".tx.fee[]",
        "ContractDataDurability=forever",
    ] {
        assert!(
            !Fragment::parse(frag).unwrap().producible(&schema),
            "{frag} was accepted"
        );
    }
    assert!(Fragment::parse("Memo").is_err());
    assert!(Fragment::parse(".x#huge").is_err());
}

#[test]
fn first_seen_dates_protocol_additions() {
    let h = history();
    let seen = h.first_seen();
    assert_eq!(seen["ScVal"], "v20");
    assert_eq!(seen["ScAddress:muxed_contract"], "v30.0");
    assert_eq!(seen["TransactionMeta:v4"], "v23.0");
    assert_eq!(seen["LedgerCloseMeta:v2"], "v23.0");
}

#[test]
fn diff_between_tags() {
    let h = history();
    let d = diff(
        &h.get("v28.0").unwrap().schema,
        &h.get("v30.0").unwrap().schema,
    );
    assert!(d.added.contains(&"ScAddress:muxed_contract".to_string()));
    let same = diff(
        &h.get("v30.0").unwrap().schema,
        &h.get("v30.0").unwrap().schema,
    );
    assert!(same.is_empty());
}
