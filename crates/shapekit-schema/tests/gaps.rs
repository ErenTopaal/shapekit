//! The representation gap tables in registry/gaps/.

use std::path::PathBuf;

use shapekit_schema::gaps::Table;

fn table(lang: &str) -> Table {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../registry/gaps")
        .join(format!("{lang}.toml"));
    Table::load(&p).unwrap()
}

#[test]
fn every_table_loads() {
    for lang in ["js", "go", "rust"] {
        let t = table(lang);
        assert_eq!(t.language, lang);
        assert!(!t.gaps.is_empty());
    }
}

#[test]
fn javascript_covers_the_required_points_with_vectors() {
    let t = table("js");
    for id in [
        "unsafe-integer",
        "fractional-number",
        "nan",
        "negative-zero",
        "invalid-utf8",
        "lone-surrogate",
    ] {
        let g = t
            .gaps
            .iter()
            .find(|g| g.id == id)
            .unwrap_or_else(|| panic!("js table has no {id} row"));
        assert!(!g.vectors.is_empty(), "{id} has no vector");
    }
}

#[test]
fn a_row_without_vector_or_reason_is_refused() {
    let t: Table = toml::from_str(
        r#"schema = 1
language = "js"
sdk = "x"
[[gap]]
id = "bare"
xdr = ["int"]
native = "number"
loss = "l"
sources = ["https://example.org"]
sdk_note = "n"
"#,
    )
    .unwrap();
    assert_eq!(
        t.problems(),
        ["gap bare has neither a vector nor no_vector"]
    );
}
