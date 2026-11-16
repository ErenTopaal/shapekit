//! Every anchored check the corpus yields has an expected answer the ledger confirms: no
//! derived hash or id is missing from its ledger.

use std::collections::BTreeMap;
use std::path::PathBuf;

use shapekit_conformance::plan::{self, Expect};
use shapekit_conformance::runner::load_manifest;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn every_anchored_check_is_confirmed_by_its_ledger() {
    let (manifest, corpus) = load_manifest(&root().join("corpus/manifest.json")).unwrap();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut unmapped = Vec::new();
    for entry in &manifest.entries {
        for check in plan::checks(entry, &corpus).unwrap() {
            if !shapekit_conformance::anchors::OPS.contains(&check.op.as_str()) {
                continue;
            }
            *counts.entry(check.op.clone()).or_default() += 1;
            if let Expect::Unmappable(why) = &check.expect {
                unmapped.push(format!("{} {}: {why}", entry.id, check.name));
            }
        }
    }
    assert!(unmapped.is_empty(), "{unmapped:#?}");
    for op in shapekit_conformance::anchors::OPS {
        assert!(counts.get(op).copied().unwrap_or(0) > 0, "no {op} check in the corpus");
    }
}
