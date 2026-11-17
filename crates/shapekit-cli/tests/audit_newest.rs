//! `shapekit audit newest` on a committed lake: the bound a workflow hands every audit job.

use std::path::Path;
use std::process::Command;

#[test]
fn prints_the_newest_ledger_of_the_lake() {
    let lake = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/chains/cap23.claimable-balances.p30/mainnet/ledgers");
    let out = Command::new(env!("CARGO_BIN_EXE_shapekit"))
        .args(["audit", "newest", "--network", "pubnet", "--lake"])
        .arg(&lake)
        .output()
        .expect("running shapekit");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "66\n");
}
