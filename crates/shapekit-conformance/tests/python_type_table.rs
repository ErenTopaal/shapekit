//! The python testee's generated type table (testees/python/shapekit_testee/types_gen.py) is
//! keyed by the names the testee protocol uses, the stellar-xdr crate's: every key is a crate
//! type, and every crate type is in the table of some sdk version or listed as absent. that
//! also checks the generator's port of heck's upper camel case, which makes the keys.

use std::collections::BTreeSet;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn quoted(line: &str) -> Vec<String> {
    line.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

#[test]
fn python_type_table_uses_the_crates_names_and_misses_none() {
    let path = root().join("testees/python/shapekit_testee/types_gen.py");
    let text = std::fs::read_to_string(&path).unwrap();
    let mut keys = BTreeSet::new();
    let mut absent = BTreeSet::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("ABSENT = ") {
            absent.extend(quoted(trimmed));
        } else if line.starts_with("    ")
            && trimmed.ends_with("\",")
            && let [key, class] = quoted(trimmed).as_slice()
        {
            assert!(
                !class.is_empty() && class.chars().all(|c| c.is_ascii_alphanumeric()),
                "unreadable table line {line}"
            );
            keys.insert(key.clone());
        }
    }
    assert!(keys.len() > 400, "the table has {} types", keys.len());
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
        "stellar-xdr types the python table neither has nor lists as absent: {missing:?}"
    );
}
