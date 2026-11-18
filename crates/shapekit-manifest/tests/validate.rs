//! Validator and builder tests on the three-entry sample corpus under tests/fixtures/sample,
//! built by hand from the testnet fixtures.

use std::fs;
use std::path::{Path, PathBuf};

use shapekit_manifest::{Manifest, build, validate};
use shapekit_registry::Registry;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn registry() -> Registry {
    Registry::load(&fixtures().join("registry")).unwrap()
}

fn copy_dir(from: &Path, to: &Path) {
    for item in walkdir::WalkDir::new(from) {
        let item = item.unwrap();
        let target = to.join(item.path().strip_prefix(from).unwrap());
        if item.file_type().is_dir() {
            fs::create_dir_all(&target).unwrap();
        } else {
            fs::copy(item.path(), &target).unwrap();
        }
    }
}

// a scratch copy of the sample corpus and its parsed manifest
fn sample() -> (tempfile::TempDir, Manifest) {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(&fixtures().join("sample"), dir.path());
    let manifest = build::load(&dir.path().join("manifest.json")).unwrap();
    (dir, manifest)
}

fn problems(manifest: &Manifest, root: &Path) -> Vec<String> {
    validate(manifest, &registry(), root)
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn registry_fixture_is_clean() {
    assert_eq!(registry().check(), vec![]);
}

#[test]
fn sample_manifest_is_valid() {
    let (dir, manifest) = sample();
    assert_eq!(manifest.entries.len(), 3);
    assert_eq!(problems(&manifest, dir.path()), Vec::<String>::new());
}

#[test]
fn build_reproduces_the_committed_manifest() {
    let committed = fs::read_to_string(fixtures().join("sample/manifest.json")).unwrap();
    let parsed: Manifest = serde_json::from_str(&committed).unwrap();
    let rebuilt = build::build(
        &fixtures().join("sample"),
        &parsed.corpus_version,
        parsed.generated_with.clone(),
    )
    .unwrap();
    assert_eq!(build::render(&rebuilt), committed);
    assert_eq!(rebuilt.protocol_tiers, vec![30]);
}

#[test]
fn wrong_hash() {
    let (dir, mut manifest) = sample();
    let v = manifest.entries[0].variants.testnet.as_mut().unwrap();
    v.xdr_sha256 = "0".repeat(64);
    let p = problems(&manifest, dir.path());
    assert_eq!(p.len(), 1, "{p:#?}");
    assert!(
        p[0].contains("testnet xdr_file") && p[0].contains("manifest says 0000"),
        "{p:#?}"
    );
}

#[test]
fn changed_file_contents() {
    let (dir, manifest) = sample();
    let file = &manifest.entries[2]
        .variants
        .testnet
        .as_ref()
        .unwrap()
        .xdr_file;
    let mut bytes = fs::read(dir.path().join(file)).unwrap();
    bytes[10] ^= 1;
    fs::write(dir.path().join(file), bytes).unwrap();
    let p = problems(&manifest, dir.path());
    assert_eq!(p.len(), 1, "{p:#?}");
    assert!(p[0].contains("has sha256"), "{p:#?}");
}

#[test]
fn missing_file() {
    let (dir, manifest) = sample();
    let file = manifest.entries[2]
        .variants
        .testnet
        .as_ref()
        .unwrap()
        .expected_json_file
        .clone()
        .unwrap();
    fs::remove_file(dir.path().join(&file)).unwrap();
    let p = problems(&manifest, dir.path());
    assert_eq!(p.len(), 1, "{p:#?}");
    assert!(p[0].contains("cannot be read"), "{p:#?}");
}

#[test]
fn synthetic_mismatch() {
    let (dir, mut manifest) = sample();
    manifest.entries[0].synthetic = true;
    let p = problems(&manifest, dir.path());
    assert_eq!(p.len(), 1, "{p:#?}");
    assert!(
        p[0].contains("synthetic is true but source.class is testnet-history"),
        "{p:#?}"
    );

    // the vector file holds its own copy of the record, so both disagree
    let (dir, mut manifest) = sample();
    let strkey = manifest.entries.iter_mut().find(|e| e.synthetic).unwrap();
    strkey.synthetic = false;
    let p = problems(&manifest, dir.path());
    assert!(
        p.iter()
            .any(|m| m.contains("synthetic is false but source.class is synthetic")),
        "{p:#?}"
    );
}

#[test]
fn must_accept_needs_expected_json() {
    let (dir, mut manifest) = sample();
    let v = manifest.entries[2].variants.testnet.as_mut().unwrap();
    v.expected_json_file = None;
    v.expected_json_sha256 = None;
    let p = problems(&manifest, dir.path());
    assert_eq!(p.len(), 1, "{p:#?}");
    assert!(
        p[0].contains("must-accept xdr entry needs expected_json_file"),
        "{p:#?}"
    );
}

#[test]
fn expected_json_must_match_the_decoded_value() {
    let (dir, mut manifest) = sample();
    let v = manifest.entries[2].variants.testnet.as_mut().unwrap();
    let path = dir.path().join(v.expected_json_file.as_ref().unwrap());
    let text = fs::read_to_string(&path)
        .unwrap()
        .replacen("16900000000", "16900000001", 1);
    fs::write(&path, &text).unwrap();
    v.expected_json_sha256 = Some(shapekit_manifest::sha256_hex(text.as_bytes()));
    let p = problems(&manifest, dir.path());
    assert_eq!(p.len(), 1, "{p:#?}");
    assert!(p[0].contains("differs from expected_json_file"), "{p:#?}");
}

#[test]
fn unknown_registry_references() {
    let (dir, mut manifest) = sample();
    manifest.entries[0].caps.push("CAP-0999".into());
    manifest.entries[0].fields_exercised.push("tx.nope".into());
    let p = problems(&manifest, dir.path());
    assert_eq!(p.len(), 2, "{p:#?}");
    assert!(p.iter().any(|m| m.contains("unknown cap CAP-0999")));
    assert!(p.iter().any(|m| m.contains("unknown field tx.nope")));

    let (dir, mut manifest) = sample();
    manifest.entries[0].family = "cap99.nope".into();
    let p = problems(&manifest, dir.path());
    assert!(
        p.iter().any(|m| m.contains("unknown family cap99.nope")),
        "{p:#?}"
    );
}

#[test]
fn history_entry_needs_provenance() {
    let (dir, mut manifest) = sample();
    manifest.entries[0].source.provenance_url = None;
    let p = problems(&manifest, dir.path());
    assert_eq!(p.len(), 1, "{p:#?}");
    assert!(p[0].contains("needs source.provenance_url"), "{p:#?}");
}

#[test]
fn entries_must_stay_sorted() {
    let (dir, mut manifest) = sample();
    manifest.entries.swap(0, 1);
    let p = problems(&manifest, dir.path());
    assert_eq!(p.len(), 1, "{p:#?}");
    assert!(p[0].contains("not sorted"), "{p:#?}");
}

#[test]
fn paths_cannot_leave_the_corpus() {
    let (dir, mut manifest) = sample();
    manifest.entries[0]
        .variants
        .testnet
        .as_mut()
        .unwrap()
        .xdr_file = "../outside.xdr".into();
    let p = problems(&manifest, dir.path());
    assert!(
        p.iter()
            .any(|m| m.contains("not a relative path inside the corpus")),
        "{p:#?}"
    );
}

// an interrupted entry rewrite or golden import leaves its staging directory behind; it
// holds a copy of an entry, which is not a second definition of it
#[test]
fn build_skips_staging_directories_under_entries() {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(&fixtures().join("sample"), dir.path());
    let committed = fs::read_to_string(dir.path().join("manifest.json")).unwrap();
    let parsed: Manifest = serde_json::from_str(&committed).unwrap();
    let entry = fs::read_dir(dir.path().join("entries"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .flat_map(|family| fs::read_dir(family).unwrap())
        .map(|e| e.unwrap().path().join("entry.json"))
        .find(|p| p.is_file())
        .expect("the sample corpus holds an entry");
    let family = entry.parent().unwrap().parent().unwrap();
    for staging in [
        family.join(".staged-x"),
        dir.path().join("entries/.golden-import-x/0/entry"),
    ] {
        fs::create_dir_all(&staging).unwrap();
        fs::copy(&entry, staging.join("entry.json")).unwrap();
    }
    let rebuilt = build::build(
        dir.path(),
        &parsed.corpus_version,
        parsed.generated_with.clone(),
    )
    .unwrap();
    assert_eq!(build::render(&rebuilt), committed);
}

#[test]
fn build_rejects_duplicate_ids() {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(&fixtures().join("sample"), dir.path());
    let src = dir
        .path()
        .join("vectors/strkey/cap27.strkey-forms.issuer-order-pair.json");
    fs::copy(&src, dir.path().join("vectors/strkey/copy.json")).unwrap();
    let committed = build::load(&dir.path().join("manifest.json")).unwrap();
    let err = build::build(dir.path(), "x", committed.generated_with).unwrap_err();
    assert!(err.to_string().contains("is defined in both"), "{err}");
}

#[test]
fn schema_file_is_up_to_date() {
    let committed = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schema/manifest.schema.json"),
    )
    .unwrap();
    assert_eq!(
        committed,
        shapekit_manifest::manifest_schema(),
        "run: cargo run -p shapekit-cli -- manifest schema"
    );
}

// the schema's source class list is written separately from the registry enum it mirrors
#[test]
fn schema_source_classes_match_the_registry() {
    let schema: serde_json::Value =
        serde_json::from_str(&shapekit_manifest::manifest_schema()).unwrap();
    let text = schema.to_string();
    for class in [
        "core-golden",
        "local-core",
        "testnet-history",
        "mainnet-history",
        "synthetic",
        "core-test-dump",
    ] {
        let parsed: shapekit_registry::SourceClass =
            serde_json::from_value(serde_json::Value::String(class.into())).unwrap();
        assert_eq!(parsed.as_str(), class);
        assert!(text.contains(&format!("\"{class}\"")), "{class}");
    }
}

#[test]
fn normalization_is_only_for_local_core() {
    let (dir, mut manifest) = sample();
    manifest.entries[0].source.normalization = Some(shapekit_manifest::Normalization::KeyStable);
    let p = problems(&manifest, dir.path());
    assert_eq!(p.len(), 1, "{p:#?}");
    assert!(p[0].contains("only recorded for local-core"), "{p:#?}");
}

#[test]
fn refused_local_core_entries_carry_cores_result() {
    use shapekit_manifest::{CoreRejection, RejectionStage, Validity};
    use shapekit_registry::SourceClass;
    use stellar_xdr::{Limits, TransactionResult, TransactionResultExt, WriteXdr};

    let malformed = TransactionResult {
        fee_charged: 100,
        result: stellar_xdr::TransactionResultResult::TxMalformed,
        ext: TransactionResultExt::V0,
    }
    .to_xdr_base64(Limits::none())
    .unwrap();
    let rejection = |stage, code: &str| {
        Some(CoreRejection {
            stage,
            result_code: code.into(),
            result_xdr: malformed.clone(),
        })
    };
    let local = |manifest: &mut Manifest, class| {
        let e = &mut manifest.entries[0];
        e.source.class = class;
        e.source.recipe = Some("cap15.fee-bump".into());
        e.source.chain = Some("cap15.fee-bump.p30".into());
        e.expected.validity = Validity::InvalidByCore;
    };

    for class in [SourceClass::LocalCore, SourceClass::LocalCoreRefused] {
        let (dir, mut manifest) = sample();
        local(&mut manifest, class);
        let p = problems(&manifest, dir.path());
        assert_eq!(p.len(), 1, "{class:?}: {p:#?}");
        assert!(p[0].contains("core_rejection"), "{class:?}: {p:#?}");
    }

    let (dir, mut manifest) = sample();
    local(&mut manifest, SourceClass::LocalCoreRefused);
    manifest.entries[0].source.core_rejection = rejection(RejectionStage::Submit, "tx_bad_auth");
    let p = problems(&manifest, dir.path());
    assert_eq!(p.len(), 1, "{p:#?}");
    assert!(p[0].contains("result_xdr holds tx_malformed"), "{p:#?}");

    let (dir, mut manifest) = sample();
    local(&mut manifest, SourceClass::LocalCoreRefused);
    manifest.entries[0].source.core_rejection = rejection(RejectionStage::Submit, "tx_malformed");
    assert_eq!(problems(&manifest, dir.path()), Vec::<String>::new());

    // bytes refused at submission never reached a ledger, so local-core would present them
    // as core output
    let (dir, mut manifest) = sample();
    local(&mut manifest, SourceClass::LocalCore);
    manifest.entries[0].source.core_rejection = rejection(RejectionStage::Submit, "tx_malformed");
    let p = problems(&manifest, dir.path());
    assert_eq!(p.len(), 1, "{p:#?}");
    assert!(
        p[0].contains("is local-core-refused, not local-core"),
        "{p:#?}"
    );

    // a transaction core applied and failed did reach a ledger
    let (dir, mut manifest) = sample();
    local(&mut manifest, SourceClass::LocalCoreRefused);
    manifest.entries[0].source.core_rejection = rejection(RejectionStage::Apply, "tx_malformed");
    let p = problems(&manifest, dir.path());
    assert_eq!(p.len(), 1, "{p:#?}");
    assert!(
        p[0].contains("needs source.core_rejection.stage submit"),
        "{p:#?}"
    );

    let (dir, mut manifest) = sample();
    local(&mut manifest, SourceClass::LocalCore);
    manifest.entries[0].source.core_rejection = rejection(RejectionStage::Apply, "tx_malformed");
    assert_eq!(problems(&manifest, dir.path()), Vec::<String>::new());

    // a rejection on an entry core accepted contradicts it
    let (dir, mut manifest) = sample();
    manifest.entries[0].source.core_rejection = rejection(RejectionStage::Submit, "tx_malformed");
    let p = problems(&manifest, dir.path());
    assert!(
        p.iter().any(|m| m.contains("not invalid-by-core")),
        "{p:#?}"
    );
}

#[test]
fn recipe_ir_is_a_recipes_path_on_local_core_entries() {
    use shapekit_registry::SourceClass;
    let (dir, mut manifest) = sample();
    manifest.entries[0].source.recipe_ir = Some("corpus/recipes/cap15.fee-bump/p30.json".into());
    let p = problems(&manifest, dir.path());
    assert_eq!(p.len(), 1, "{p:#?}");
    assert!(p[0].contains("only recorded for local-core"), "{p:#?}");

    manifest.entries[0].source.class = SourceClass::LocalCore;
    manifest.entries[0].source.recipe = Some("cap15.fee-bump".into());
    manifest.entries[0].source.chain = Some("cap15.fee-bump.p30".into());
    for bad in [
        "recipes/x.json",
        "corpus/recipes/../x.json",
        "corpus/recipes/x.toml",
    ] {
        manifest.entries[0].source.recipe_ir = Some(bad.into());
        let p = problems(&manifest, dir.path());
        assert!(
            p.iter().any(|l| l.contains("is not a corpus/recipes/")),
            "{bad}: {p:#?}"
        );
    }
}

#[test]
fn obligation_ids_name_a_ledger_kind() {
    let (dir, mut manifest) = sample();
    manifest.entries[0].obligations = vec![
        "field:TransactionEnvelope.v1.tx.memo".into(),
        "format:ScVal.bool".into(),
    ];
    assert_eq!(problems(&manifest, dir.path()), Vec::<String>::new());
    for bad in [
        vec!["memo".to_string()],
        vec!["branch:x".into()],
        vec!["field:".into()],
    ] {
        manifest.entries[0].obligations = bad.clone();
        let p = problems(&manifest, dir.path());
        assert!(
            p.iter().any(|l| l.contains("is not <kind>:<name>")),
            "{bad:?}: {p:#?}"
        );
    }
    manifest.entries[0].obligations = vec!["format:b".into(), "format:a".into()];
    let p = problems(&manifest, dir.path());
    assert!(p.iter().any(|l| l.contains("not sorted")), "{p:#?}");
}

#[test]
fn core_test_dump_entries_are_any_variant_transaction_meta() {
    use shapekit_registry::SourceClass;
    let (dir, mut manifest) = sample();
    let e = &mut manifest.entries[0];
    e.source.class = SourceClass::CoreTestDump;
    e.source.ledger_seq = None;
    e.source.provenance_url = None;
    let p = problems(&manifest, dir.path());
    for want in [
        "needs source.upstream",
        "holds TransactionMeta xdr",
        "has only an any variant",
    ] {
        assert!(p.iter().any(|l| l.contains(want)), "{want}: {p:#?}");
    }
}

#[test]
fn entries_of_a_capless_family_name_no_cap() {
    let (dir, mut manifest) = sample();
    let mut reg = registry();
    let fam = reg
        .families
        .as_mut()
        .unwrap()
        .families
        .iter_mut()
        .find(|f| f.id == manifest.entries[0].family)
        .unwrap();
    fam.caps.clear();
    fam.no_cap_reason = Some("the xdr definitions themselves".into());
    let p: Vec<String> = validate(&manifest, &reg, dir.path())
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(
        p.iter()
            .any(|l| l.contains("names no cap, so its entries name none")),
        "{p:#?}"
    );
    manifest.entries[0].caps.clear();
    let p: Vec<String> = validate(&manifest, &reg, dir.path())
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(!p.iter().any(|l| l.contains("caps")), "{p:#?}");
    let p = problems(&manifest, dir.path());
    assert!(p.iter().any(|l| l.contains("caps is empty")), "{p:#?}");
}
