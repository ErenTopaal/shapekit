//! The candidate gap vectors are exactly what `shapekit vectors gaps` writes from the tables.

use std::process::Command;

#[test]
fn candidate_gap_vectors_match_the_tables() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = Command::new(env!("CARGO_BIN_EXE_shapekit"))
        .args(["vectors", "gaps", "--check"])
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
