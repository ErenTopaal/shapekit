//! One real test case through the pinned test image.

use std::collections::BTreeMap;
use std::path::Path;

use shapekit_coretest::pins::Image;
use shapekit_coretest::run::{self, Limits, Options};

#[test]
#[ignore = "needs docker and the pinned stellar-core test image; run with --include-ignored"]
fn bump_sequence_dumps_every_protocol() {
    let pins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pins.toml");
    let image = Image::from_pins(&pins, shapekit_coretest::BUILDTESTS_IMAGE_PIN).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let opts = Options {
        image,
        shards: 1,
        limits: Limits::default(),
        out: dir.path().join("out"),
        tests: vec!["bump sequence".into()],
        order_from: None,
        split: Vec::new(),
    };
    let report = run::run(&opts, &|_, _, _| {}).unwrap();
    assert!(report.passed(), "{report:#?}");
    let t = &report.totals;
    // 8 metas per protocol up to 9, 18 for 10 to 18, 28 for 19 to 30
    assert_eq!(t.metas, 578);
    assert_eq!(t.protocols.len(), 31);
    assert_eq!(t.protocols[&0], 8);
    assert_eq!(t.protocols[&10], 18);
    assert_eq!(t.protocols[&30], 28);
    assert_eq!(
        t.meta_versions,
        BTreeMap::from([(2, 270), (3, 84), (4, 224)])
    );
    assert!(
        dir.path()
            .join("out/baseline/000/BumpSequenceTests.json")
            .is_file()
    );
}

#[test]
#[ignore = "needs docker and the pinned stellar-core test image; run with --include-ignored"]
fn a_split_case_records_what_the_whole_case_records() {
    let pins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pins.toml");
    let image = Image::from_pins(&pins, shapekit_coretest::BUILDTESTS_IMAGE_PIN).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let opts = Options {
        image,
        shards: 2,
        limits: Limits::default(),
        out: dir.path().join("out"),
        tests: vec!["bump sequence".into()],
        order_from: None,
        split: vec!["bump sequence".into()],
    };
    let report = run::run(&opts, &|_, _, _| {}).unwrap();
    assert!(report.passed(), "{report:#?}");
    let t = &report.totals;
    assert_eq!(t.metas, 578);
    assert_eq!(t.protocols.len(), 31);
    assert_eq!(
        t.meta_versions,
        BTreeMap::from([(2, 270), (3, 84), (4, 224)])
    );
    assert!(
        dir.path()
            .join("out/baseline/000/v30/BumpSequenceTests.json")
            .is_file()
    );
    assert!(!dir.path().join("out/dump/000-v00.txt").exists());
}
