//! The java testee's generated type table (testees/java/src/main/java/org/shapekit/testee/
//! TypesGen.java) is keyed by the names the testee protocol uses, the stellar-xdr crate's: every
//! key is a crate type, and every crate type is in the table of some sdk version or listed as
//! absent from all of them. a name the generator's port of heck camel-cased differently from
//! the crate would show up here as an unknown key and a missing type.

use std::collections::BTreeSet;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn java_type_table_uses_the_crates_names_and_misses_none() {
    let path = root().join("testees/java/src/main/java/org/shapekit/testee/TypesGen.java");
    let text = std::fs::read_to_string(&path).unwrap();
    let mut keys = BTreeSet::new();
    let mut absent = BTreeSet::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("static final List<String> ABSENT") {
            for part in line.split('"').skip(1).step_by(2) {
                absent.insert(part.to_string());
            }
        }
        // `Map.entry("Name", "Binary")`, several on one line in the per-version tables; the
        // per-version entries `Map.entry("6.0.0", Map.ofEntries(...))` are skipped
        for chunk in line.split("Map.entry(\"").skip(1) {
            let name = chunk.split('"').next().unwrap();
            if !name.starts_with(|c: char| c.is_ascii_digit()) {
                keys.insert(name.to_string());
            }
        }
    }
    assert!(keys.len() > 400, "the table has {} types", keys.len());
    let crate_names: BTreeSet<String> = stellar_xdr::TypeVariant::VARIANTS_STR
        .iter()
        .map(|s| s.to_string())
        .collect();
    let unknown: Vec<&String> = keys
        .union(&absent)
        .filter(|k| !crate_names.contains(*k))
        .collect();
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
        "stellar-xdr types the java table neither has nor lists as absent: {missing:?}"
    );
}
