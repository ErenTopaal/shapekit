//! The candidate format vectors are exactly what `shapekit vectors gen` writes from the schema.

use std::process::Command;

#[test]
fn candidate_format_vectors_match_the_schema() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = Command::new(env!("CARGO_BIN_EXE_shapekit"))
        .args(["vectors", "gen", "--check"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
