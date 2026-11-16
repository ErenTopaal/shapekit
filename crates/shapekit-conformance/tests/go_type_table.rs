//! The go testee's generated type table (testees/go/tables_gen*.go) is keyed by the names the
//! testee protocol uses, the stellar-xdr crate's: every key is a crate type, and every crate
//! type is in the table of some sdk version or listed as absent from all of them.

use std::collections::BTreeSet;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn go_type_table_uses_the_crates_names_and_misses_none() {
    let mut keys = BTreeSet::new();
    let mut absent = BTreeSet::new();
    let dir = root().join("testees/go");
    for e in std::fs::read_dir(&dir).unwrap() {
        let path = e.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !(name.starts_with("tables_gen") && name.ends_with(".go")) {
            continue;
        }
        for line in std::fs::read_to_string(&path).unwrap().lines() {
            let line = line.trim();
            if line.starts_with("var absentTypes") {
                for part in line.split('"').skip(1).step_by(2) {
                    absent.insert(part.to_string());
                }
            } else if line.contains("func() any {") && line.contains("new(xdr.") {
                // `"Name": func() any {...}` in the table, `xdrTypes["Name"] = ...` in init()
                let k = line
                    .split('"')
                    .nth(1)
                    .unwrap_or_else(|| panic!("unreadable table line {line}"))
                    .to_string();
                keys.insert(k);
            }
        }
    }
    let crate_names: BTreeSet<String> = stellar_xdr::TypeVariant::VARIANTS_STR
        .iter()
        .map(|s| s.to_string())
        .collect();
    let unknown: Vec<&String> = keys.difference(&crate_names).collect();
    assert!(
        unknown.is_empty(),
        "keys stellar-xdr does not name: {unknown:?}"
    );
    let missing: Vec<&String> = crate_names
        .iter()
        .filter(|n| !keys.contains(*n) && !absent.contains(*n))
        .collect();
    assert!(
        missing.is_empty(),
        "stellar-xdr types the go table neither has nor lists as absent: {missing:?}"
    );
}
