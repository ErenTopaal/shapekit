//! Golden import, agreement and history capture, run against the committed corpus: the
//! imported core goldens and the captured testnet ledgers are real core output.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::Value;
use shapekit_gen::golden::{self, GOLDEN_PASSPHRASE, TIER};
use shapekit_gen::history::{self, CaptureRequest, Fetched, Network, Slice};
use shapekit_manifest::build::{self, ENTRIES_DIR};
use shapekit_manifest::framing::frame_records;
use shapekit_manifest::{
    CoreImage, Entry, GeneratedWith, TESTNET_PASSPHRASE, XdrDefinitions, sha256_hex, validate,
};
use shapekit_registry::{Registry, SourceClass};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn registry() -> Registry {
    Registry::load(&root().join("registry")).expect("registry loads")
}

fn corpus() -> PathBuf {
    root().join("corpus")
}

fn entries_of(class: SourceClass) -> Vec<Entry> {
    let mut out = Vec::new();
    for item in walkdir::WalkDir::new(corpus().join(ENTRIES_DIR))
        .min_depth(3)
        .max_depth(3)
        .sort_by_file_name()
    {
        let item = item.unwrap();
        if item.file_name() == "entry.json" {
            let e: Entry = serde_json::from_slice(&std::fs::read(item.path()).unwrap()).unwrap();
            if e.source.class == class {
                out.push(e);
            }
        }
    }
    out
}

fn entry(id: &str) -> Entry {
    let family = id.rsplit_once('.').map(|(f, _)| f).unwrap();
    let path = corpus()
        .join(ENTRIES_DIR)
        .join(family)
        .join(id)
        .join("entry.json");
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn variant_bytes(e: &Entry) -> Vec<u8> {
    let (_, v) = e.variants.iter().next().unwrap();
    std::fs::read(corpus().join(&v.xdr_file)).unwrap()
}

fn generated_with() -> GeneratedWith {
    GeneratedWith {
        stellar_xdr: "30.0.0".into(),
        xdr_definitions: XdrDefinitions {
            tag: "v30.0".into(),
            commit: "4f524bbac80c781c06e4fb5fc93a2d0d331d6705".into(),
        },
        core_image: CoreImage {
            repository: "stellar/stellar-core".into(),
            tag: "30.0.0-3642.0e49751aa.noble".into(),
            digest: "sha256:eb7a7c81a29b8665c8878d55e1b3979f0d7527426350c4ff258a9c7647ae5c51"
                .into(),
        },
        soroban_env_host: "30.0.0".into(),
    }
}

#[test]
fn imported_goldens_match_the_committed_index() {
    let index: Value = serde_json::from_slice(
        &std::fs::read(corpus().join("goldens/core-v30.0.0/index.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(index["network_passphrase"], GOLDEN_PASSPHRASE);
    assert_eq!(index["upstream"]["tier"], TIER);
    let files = index["files"].as_array().unwrap();
    assert_eq!(
        files.len(),
        index["totals"]["files"].as_u64().unwrap() as usize
    );
    let goldens = entries_of(SourceClass::CoreGolden);
    assert!(!goldens.is_empty());
    for e in &goldens {
        let up = e.source.upstream.as_ref().unwrap();
        let file = files
            .iter()
            .find(|f| f["path"] == up.path.as_str())
            .unwrap_or_else(|| panic!("{} names {} which is not indexed", e.id, up.path));
        assert_eq!(file["sha256"], up.sha256.as_str(), "{}", e.id);
        assert!(
            file["imported_entries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|x| x == e.id.as_str()),
            "{} is not listed as imported from {}",
            e.id,
            up.path
        );
        let bytes = variant_bytes(e);
        let lcm = golden::decode_lcm(&bytes).unwrap();
        assert_eq!(Some(golden::ledger_seq(&lcm)), e.source.ledger_seq);
        assert!(bytes.len() <= golden::MAX_IMPORT_BYTES);
    }
}

// core writes a STATE change before every UPDATED and REMOVED change of an operation, and
// every successful operation result has an operation meta; both hold in every golden
#[test]
fn change_kinds_of_real_goldens_follow_core_meta_rules() {
    let mut ops_seen = BTreeSet::new();
    for e in entries_of(SourceClass::CoreGolden) {
        let lcm = golden::decode_lcm(&variant_bytes(&e)).unwrap();
        let kinds = golden::op_change_kinds(&lcm).unwrap();
        for (op, set) in &kinds {
            ops_seen.insert(op.clone());
            for k in set {
                if let Some(t) = k
                    .strip_prefix("updated:")
                    .or_else(|| k.strip_prefix("removed:"))
                {
                    assert!(
                        set.contains(&format!("state:{t}")),
                        "{}: {op} has {k} without state:{t}",
                        e.id
                    );
                }
            }
            if op == "create_account" {
                assert!(set.contains("created:account"), "{}", e.id);
            }
        }
    }
    for op in ["create_account", "payment", "invoke_host_function"] {
        assert!(ops_seen.contains(op), "no golden has a successful {op}");
    }
}

// a two-file core tree built from committed golden bytes goes through scan, import, index
// verification and manifest validation, the same path `shapekit import goldens` takes
#[test]
fn scan_import_and_verify_a_golden_tree() {
    let registry = registry();
    let a = entry("cap35.clawback.golden-3f24be68a9962089-11");
    let b = entry("cap18.maintain-liabilities.golden-3f24be68a9962089-4");
    let (a_bytes, b_bytes) = (variant_bytes(&a), variant_bytes(&b));

    let tmp = tempfile::tempdir().unwrap();
    let core = tmp.path().join("core");
    let dir = core.join(TIER).join("ClawbackTests");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("index.json"),
        r#"{
   "!cfg protocol version" : 30,
   "!rng seed" : 12345,
   "!test all versions" : false,
   "!versions to test" : [ 30 ],
   "00000000000000aa" : "clawback|both ledgers",
   "00000000000000bb" : "clawback|one ledger"
}
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("00000000000000aa.xdr"),
        frame_records([a_bytes.as_slice(), b_bytes.as_slice()]).unwrap(),
    )
    .unwrap();
    std::fs::write(
        dir.join("00000000000000bb.xdr"),
        frame_records([b_bytes.as_slice()]).unwrap(),
    )
    .unwrap();

    let mut set =
        golden::scan(&core, "v30.0.0", "0e49751aa9ddf892302cca02cea7f3f5d97df9f1").unwrap();
    assert_eq!(set.files.len(), 2);
    assert_eq!(set.lcms.len(), 3);
    assert_eq!(set.files[0].test, "clawback");
    assert_eq!(set.files[0].section, "both ledgers");
    assert_eq!(set.files[0].stamps.rng_seed, 12345);

    let (selections, skipped) = golden::select(&set, &registry).unwrap();
    assert!(skipped.is_empty());
    for sel in &selections {
        let chosen = set.lcms[sel.lcm].bytes.len();
        let family = registry.family(&sel.family).unwrap();
        for f in &sel.fragments {
            assert!(family.signature.contains(f));
            let smallest = set
                .lcms
                .iter()
                .filter(|l| shapekit_manifest::fragment_matches(&l.signature, f))
                .map(|l| l.bytes.len())
                .min()
                .unwrap();
            assert_eq!(chosen, smallest, "{} {f}", sel.family);
        }
    }
    assert!(selections.iter().any(|s| s.family == "cap35.clawback"));

    let out = tmp.path().join("corpus");
    let report = golden::import(&mut set, &registry, &out).unwrap();
    assert_eq!(
        report.written.values().map(Vec::len).sum::<usize>(),
        selections.len()
    );
    let written: Value =
        serde_json::from_slice(&std::fs::read(&report.index_path).unwrap()).unwrap();
    assert!(golden::verify_index(&set, &written).is_empty());
    let mut tampered = written.clone();
    tampered["files"][1]["sha256"] = Value::from("0".repeat(64));
    assert_eq!(golden::verify_index(&set, &tampered).len(), 1);

    let manifest = build::build(&out, "test", generated_with()).unwrap();
    let problems = validate(&manifest, &registry, &out);
    assert!(problems.is_empty(), "{problems:?}");

    // a second import replaces the first one's entries instead of piling up
    let again = golden::import(&mut set, &registry, &out).unwrap();
    assert_eq!(again.removed.len(), selections.len());
}

#[test]
fn scan_rejects_an_index_that_disagrees_with_the_files() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join(TIER).join("X");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("index.json"),
        r#"{"!cfg protocol version": 30, "!rng seed": 12345, "!test all versions": false,
            "!versions to test": [30], "00000000000000aa": "t|s"}"#,
    )
    .unwrap();
    let err = golden::scan(tmp.path(), "v30.0.0", "x").err().unwrap();
    assert!(format!("{err:#}").contains("without a file"), "{err:#}");
}

fn exceptions() -> Vec<golden::AgreeException> {
    golden::load_exceptions(&corpus().join(golden::EXCEPTIONS_FILE)).unwrap()
}

/// Runs `f` with room for a whole ledger at `xdr.meta.depth`: the corpus holds a ledger
/// whose envelope core accepted at depth 1000, and an unoptimised decode of it needs more
/// than a test thread's default stack.
fn big(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("the check thread panicked");
}

#[test]
fn committed_corpus_goldens_agree_with_history() {
    big(committed_corpus_goldens_agree);
}

fn committed_corpus_goldens_agree() {
    let registry = registry();
    let manifest = build::build(&corpus(), "test", generated_with()).unwrap();
    let report = golden::agree(&manifest, &registry, &corpus(), &exceptions()).unwrap();
    let failed: Vec<_> = report
        .families
        .iter()
        .flat_map(|f| &f.combinations)
        .filter(|c| c.failed())
        .collect();
    assert_eq!(
        report.failures(),
        0,
        "{failed:#?} {:#?}",
        report.unknown_exceptions
    );
    let pool = report
        .families
        .iter()
        .find(|f| f.family == "cap38.liquidity-pools")
        .expect("the py-stellar-base#1246 ledger is compared with the pool goldens");
    assert!(
        pool.others
            .iter()
            .any(|(id, class)| id == "cap38.liquidity-pools.testnet-5521432"
                && *class == SourceClass::TestnetHistory)
    );
    assert!(!pool.shared_fragments.is_empty());
}

// a result meta entry is compared on its own; the envelope entry next to it is optional
#[test]
fn meta_entries_need_no_envelope_entry() {
    let registry = registry();
    let manifest = build::build(&corpus(), "test", generated_with()).unwrap();
    let mut m = manifest.clone();
    m.entries.retain(|e| {
        e.family == "soroban.scval-shapes" && e.xdr_type.as_deref() != Some("TransactionEnvelope")
    });
    let report = golden::agree(&m, &registry, &corpus(), &[]).unwrap();
    let family = &report.families[0];
    assert!(
        family
            .others
            .iter()
            .any(|(id, _)| id == "soroban.scval-shapes.mainnet-63092996-71f6c6ff-meta")
    );
}

// goldens relabelled as local-core entries stand in for recipe output: a copy covers every
// combination of its golden, while a ledger that restores less than the golden leaves the
// golden's combination unmatched until an exception names it
#[test]
fn agreement_requires_every_golden_combination() {
    let registry = registry();
    let manifest = build::build(&corpus(), "test", generated_with()).unwrap();
    let goldens: Vec<Entry> = manifest
        .entries
        .iter()
        .filter(|e| {
            e.family == "cap23.claimable-balances" && e.source.class == SourceClass::CoreGolden
        })
        .cloned()
        .collect();
    assert!(goldens.len() >= 2);
    let mut checked = 0;
    for g in &goldens {
        let mut copy = g.clone();
        copy.id = format!("{}-copy", g.id);
        copy.source.class = SourceClass::LocalCore;
        let mut m = manifest.clone();
        m.entries = vec![g.clone(), copy];
        let report = golden::agree(&m, &registry, &corpus(), &[]).unwrap();
        assert_eq!(report.failures(), 0);
        assert!(report.families[0].local_only.is_empty());
        checked += report.families[0].combinations.len();
    }
    assert!(checked > 0);

    // only the golden restores contract code
    let restore_code = entry("soroban.invoke-basics.golden-7b59aa36c1776fe1-32");
    let mut restore_data = entry("soroban.invoke-basics.golden-04667ce620bffee7-26");
    restore_data.source.class = SourceClass::LocalCore;
    let mut m = manifest.clone();
    m.entries = vec![restore_data.clone(), restore_code.clone()];
    m.entries.sort_by(|a, b| a.id.cmp(&b.id));
    let report = golden::agree(&m, &registry, &corpus(), &[]).unwrap();
    let family = &report.families[0];
    let restore = family
        .combinations
        .iter()
        .find(|c| c.op == "restore_footprint")
        .unwrap();
    assert!(restore.failed());
    assert!(
        restore
            .kinds
            .contains(&"restored:contract_code".to_string())
    );
    assert!(
        family
            .local_only
            .iter()
            .any(|(op, kinds, _)| op == "restore_footprint"
                && !kinds.contains(&"restored:contract_code".to_string()))
    );
    let before = report.failures();

    let exception = golden::AgreeException {
        golden: restore_code.id.clone(),
        op: "restore_footprint".into(),
        kinds: restore.kinds.iter().rev().cloned().collect(),
        reason: "the local ledger had no archived code to restore".into(),
    };
    let report = golden::agree(&m, &registry, &corpus(), std::slice::from_ref(&exception)).unwrap();
    assert_eq!(report.failures(), before - 1);
    assert!(report.stale_exceptions.is_empty());

    let mut typo = exception;
    typo.op = "restore_footprnt".into();
    let report = golden::agree(&m, &registry, &corpus(), &[typo]).unwrap();
    assert_eq!(report.unknown_exceptions.len(), 1);
    assert_eq!(report.failures(), before + 1);
}

#[test]
fn transaction_slices_are_cut_byte_for_byte() {
    let e = entry("cap38.liquidity-pools.testnet-5521432");
    let bytes = variant_bytes(&e);
    let lcm = golden::decode_lcm(&bytes).unwrap();
    let tx = "0126748091930959a3aefac9296e1773b947f964a9b73f37ce856c4df32d346f";
    let index = history::tx_hashes(&lcm)
        .iter()
        .position(|h| h == tx)
        .unwrap();
    assert_eq!(Some(index as u32), e.source.tx_index);
    let env = history::find_envelope(&lcm, TESTNET_PASSPHRASE, tx).unwrap();
    let (meta_type, meta) = history::result_meta(&lcm, index).unwrap();
    assert_eq!(meta_type, "TransactionResultMetaV1");
    for part in [&env, &meta] {
        assert!(bytes.windows(part.len()).any(|w| w == part.as_slice()));
    }
    // under another passphrase the hashes differ, so the envelope is not found
    assert!(history::find_envelope(&lcm, GOLDEN_PASSPHRASE, tx).is_err());
}

#[test]
fn captures_write_valid_entries_and_refuse_the_wrong_family() {
    let registry = registry();
    let source = entry("cap38.liquidity-pools.testnet-5521432");
    let fetched = Fetched {
        bytes: variant_bytes(&source),
        path: "committed test fixture",
        url: source.source.provenance_url.clone().unwrap(),
    };
    let tmp = tempfile::tempdir().unwrap();
    let tx = "0126748091930959a3aefac9296e1773b947f964a9b73f37ce856c4df32d346f";
    let req = |family: &'static str, slice: Slice| CaptureRequest {
        network: Network::Testnet,
        ledger: 5_521_432,
        tx: Some(tx),
        slice,
        family,
        slug: "fixture",
        linked_issues: vec![],
        note: None,
    };
    let report = history::write_capture(
        tmp.path(),
        &registry,
        &req("cap38.liquidity-pools", Slice::Tx),
        &fetched,
        "2026-11-12",
    )
    .unwrap();
    let ids: Vec<&str> = report.entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "cap38.liquidity-pools.fixture-envelope",
            "cap38.liquidity-pools.fixture-meta"
        ]
    );
    assert_eq!(report.tx_index, Some(2));
    assert_eq!(report.protocol, 29);
    let manifest = build::build(tmp.path(), "test", generated_with()).unwrap();
    let problems = validate(&manifest, &registry, tmp.path());
    assert!(problems.is_empty(), "{problems:?}");
    for e in &manifest.entries {
        assert_eq!(e.source.class, SourceClass::TestnetHistory);
        assert_eq!(e.source.tx_hash.as_deref(), Some(tx));
        let v = e.variants.testnet.as_ref().unwrap();
        assert_eq!(
            v.xdr_sha256,
            sha256_hex(&std::fs::read(tmp.path().join(&v.xdr_file)).unwrap())
        );
    }

    let err = history::write_capture(
        tmp.path(),
        &registry,
        &req("cap84.muxed-contract", Slice::Ledger),
        &fetched,
        "2026-11-12",
    )
    .err()
    .unwrap();
    assert!(format!("{err:#}").contains("matches none"), "{err:#}");
}
