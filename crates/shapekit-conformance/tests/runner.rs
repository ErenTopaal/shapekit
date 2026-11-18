//! Runner tests: the protocol client against a stand-in testee, and full runs, reproductions
//! and known-failures semantics against the real rust testee over a small corpus written here.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};
use shapekit_conformance::compare::ResultClass;
use shapekit_conformance::known;
use shapekit_conformance::plan::{self, Expect};
use shapekit_conformance::reproduce::{self, Observed};
use shapekit_conformance::runner::{self, RunOptions, Selection};
use shapekit_conformance::testee::{ErrorClass, Failure, Reply, Testee};
use shapekit_conformance::versions::{
    MAX_RESPONSE_LINE, MemoryLimit, MemoryResource, Target, VersionsFile,
};
use shapekit_manifest::sha256_hex;
use stellar_xdr::{Limits, ScMap, ScMapEntry, ScVal, WriteXdr};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn versions_file() -> PathBuf {
    root().join("conformance/versions.toml")
}

/// Builds every rust testee version once per test binary.
fn rust_testees() {
    static BUILT: OnceLock<()> = OnceLock::new();
    BUILT.get_or_init(|| {
        let file = VersionsFile::load(&versions_file()).unwrap();
        for v in ["28.0.0", "28.0.1", "30.0.0"] {
            shapekit_conformance::versions::build(&root(), &file.target("rust", v).unwrap())
                .unwrap();
        }
    });
}

fn fixture(mode: &str, memory: MemoryLimit) -> Target {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fixture-testee.sh");
    Target {
        sdk: "fixture".into(),
        package: "fixture".into(),
        version: "1.0.0".into(),
        timeout_ms: 300,
        memory,
        build: Vec::new(),
        command: vec!["sh".into(), script.to_string_lossy().into_owned()],
        env: [("FIXTURE_MODE".to_string(), mode.to_string())].into(),
        max_protocol: None,
        current: true,
        max_response_line: MAX_RESPONSE_LINE,
    }
}

const SMALL: MemoryLimit = MemoryLimit {
    resource: MemoryResource::As,
    mib: 512,
};

fn order_request() -> serde_json::Map<String, Value> {
    json!({"op": "asset_order", "a": "", "b": ""})
        .as_object()
        .unwrap()
        .clone()
}

#[test]
fn a_hanging_testee_times_out_and_is_restarted() {
    let mut t = Testee::start(&root(), &fixture("slow", SMALL)).unwrap();
    assert_eq!(t.starts, 1);
    let answer = t.request(order_request()).unwrap();
    assert_eq!(answer, Err(Failure::Timeout { ms: 300 }));
    assert_eq!(t.starts, 2, "the testee is restarted after a timeout");
}

#[test]
fn a_dying_testee_is_reported_and_restarted() {
    let mut t = Testee::start(&root(), &fixture("die", SMALL)).unwrap();
    let answer = t.request(order_request()).unwrap();
    match answer {
        Err(Failure::Exited { status, .. }) => assert!(status.contains('3'), "{status}"),
        other => panic!("expected an exit, got {other:?}"),
    }
    assert_eq!(t.starts, 2);
}

#[test]
fn a_response_with_another_id_is_a_protocol_violation() {
    let mut t = Testee::start(&root(), &fixture("badid", SMALL)).unwrap();
    match t.request(order_request()).unwrap() {
        Err(Failure::Protocol { message, .. }) => assert!(message.contains("answers no request")),
        other => panic!("expected a protocol violation, got {other:?}"),
    }
}

#[test]
fn a_response_line_past_the_limit_is_a_protocol_violation() {
    // the huge fixture answers with a line of 255 bytes
    let mut target = fixture("huge", SMALL);
    target.max_response_line = 255;
    let mut t = Testee::start(&root(), &target).unwrap();
    assert!(matches!(
        t.request(order_request()).unwrap(),
        Ok(Reply::Ok(_))
    ));
    target.max_response_line = 254;
    let mut t = Testee::start(&root(), &target).unwrap();
    match t.request(order_request()).unwrap() {
        Err(Failure::Protocol { message, .. }) => {
            assert!(message.contains("longer than 254 bytes"), "{message}")
        }
        other => panic!("expected a protocol violation, got {other:?}"),
    }
    assert_eq!(
        t.starts, 2,
        "the testee is restarted after an overlong line"
    );
}

#[test]
fn a_testee_that_stops_reading_cannot_block_the_write() {
    let mut t = Testee::start(&root(), &fixture("noread", SMALL)).unwrap();
    // far larger than a pipe buffer, so the write itself blocks
    let mut request = order_request();
    request.insert("pad".into(), Value::String("x".repeat(4 << 20)));
    let started = std::time::Instant::now();
    assert_eq!(
        t.request(request).unwrap(),
        Err(Failure::Timeout { ms: 300 })
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(3),
        "the timeout covers the write: {:?}",
        started.elapsed()
    );
    assert_eq!(t.starts, 2);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn the_memory_limit_reaches_the_testee() {
    for (resource, field) in [(MemoryResource::As, "as"), (MemoryResource::Data, "data")] {
        let limit = MemoryLimit { resource, mib: 300 };
        let mut t = Testee::start(&root(), &fixture("limits", limit)).unwrap();
        let Ok(Reply::Error { class, message }) = t.request(order_request()).unwrap() else {
            panic!("the limits fixture answers with an error");
        };
        assert_eq!(class, ErrorClass::Internal);
        // ulimit reports KiB
        assert!(
            message.contains(&format!("{field}=307200")),
            "{field}: {message}"
        );
    }
}

// macos has no usable memory rlimit; the runner kills a process over its footprint instead
#[cfg(target_os = "macos")]
#[test]
fn a_testee_over_the_memory_limit_is_killed() {
    let limit = MemoryLimit {
        resource: MemoryResource::As,
        mib: 300,
    };
    let mut target = fixture("hog", limit);
    target.timeout_ms = 10_000;
    let mut t = Testee::start(&root(), &target).unwrap();
    match t.request(order_request()).unwrap() {
        Err(Failure::Exited { status, stderr }) => {
            assert!(status.contains("SIGKILL"), "{status}");
            assert!(
                stderr.contains("killed the testee at a physical footprint of")
                    && stderr.contains("over the 300 MiB limit"),
                "{stderr}"
            );
        }
        other => panic!("the testee is killed, not {other:?}"),
    }
    assert_eq!(t.starts, 2);
}

#[cfg(target_os = "macos")]
#[test]
fn a_process_the_testee_starts_is_held_to_the_memory_limit() {
    let limit = MemoryLimit {
        resource: MemoryResource::As,
        mib: 300,
    };
    let mut target = fixture("hogchild", limit);
    target.timeout_ms = 10_000;
    let mut t = Testee::start(&root(), &target).unwrap();
    // the shell reports 128 + 9 for a child ended by SIGKILL
    let Ok(Reply::Error { message, .. }) = t.request(order_request()).unwrap() else {
        panic!("the testee survives and reports its child's status");
    };
    assert_eq!(message, "child=137");
    assert_eq!(t.starts, 1);
}

#[test]
fn a_testee_of_the_wrong_version_is_refused() {
    let mut target = fixture("ok", SMALL);
    target.version = "2.0.0".into();
    let err = Testee::start(&root(), &target).err().unwrap();
    assert!(format!("{err:#}").contains("not 2.0.0"), "{err:#}");
}

#[test]
fn rust_testee_hello_matches_versions_toml() {
    rust_testees();
    let file = VersionsFile::load(&versions_file()).unwrap();
    for v in ["28.0.0", "28.0.1", "30.0.0"] {
        let t = Testee::start(&root(), &file.target("rust", v).unwrap()).unwrap();
        assert_eq!(t.hello().sdk_version, v);
        assert_eq!(t.hello().components["stellar-strkey"], "1.0.0");
        assert!(!t.supports("to_native"));
    }
}

// a corpus small enough to reason about, written fresh for each test

fn enc(v: &impl WriteXdr) -> Vec<u8> {
    v.to_xdr(Limits::none()).unwrap()
}

fn usd(fill: u8) -> String {
    let mut bytes = vec![0, 0, 0, 1, b'U', b'S', b'D', 0, 0, 0, 0, 0];
    bytes.extend_from_slice(&[fill; 32]);
    B64.encode(bytes)
}

fn entry(id: &str, kind: &str, decode: &str) -> Value {
    json!({
        "id": id,
        "kind": kind,
        "family": "soroban.scval-shapes",
        "caps": [],
        "protocol": {"introduced": 20, "valid_min": 20},
        "source": {"class": "synthetic"},
        "synthetic": true,
        "passphrase_dependent": false,
        "expected": {"validity": "valid", "decode": decode, "roundtrip": decode == "must-accept"},
        "flags": [],
        "fields_exercised": [],
        "linked_issues": [],
    })
}

struct Corpus {
    dir: tempfile::TempDir,
}

impl Corpus {
    fn manifest(&self) -> PathBuf {
        self.dir.path().join("corpus/manifest.json")
    }
    fn known(&self) -> PathBuf {
        self.dir.path().join("known")
    }
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn corpus() -> Corpus {
    let dir = tempfile::tempdir().unwrap();
    let c = dir.path().join("corpus");
    let mut entries = Vec::new();

    let u32_7 = enc(&ScVal::U32(7));
    let accept_json = b"{\"u32\": 7}\n";
    write(&c.join("entries/t/t.accept/any.xdr"), &u32_7);
    write(&c.join("entries/t/t.accept/any.expected.json"), accept_json);
    let mut accept = entry("t.accept", "xdr", "must-accept");
    accept["xdr_type"] = json!("ScVal");
    accept["variants"] = json!({"any": {
        "xdr_file": "entries/t/t.accept/any.xdr", "xdr_sha256": sha256_hex(&u32_7),
        "expected_json_file": "entries/t/t.accept/any.expected.json",
        "expected_json_sha256": sha256_hex(accept_json)}});
    entries.push(accept);

    let mut trailing = u32_7.clone();
    trailing.extend_from_slice(&[0, 0]);
    write(&c.join("entries/t/t.reject/any.xdr"), &trailing);
    let mut reject = entry("t.reject", "xdr", "must-reject");
    reject["xdr_type"] = json!("ScVal");
    reject["variants"] = json!({"any": {
        "xdr_file": "entries/t/t.reject/any.xdr", "xdr_sha256": sha256_hex(&trailing)}});
    entries.push(reject);

    // the expectation is the strkey-text order, so a correct sdk diverges from it: a stand-in
    // for a real divergence that the known-failures tests can list and unlist
    let wrong = entry("t.order-wrong", "ordering", "must-accept");
    let cases = vec![
        json!({"op": "asset_order", "a": {"xdr": usd(0)}, "b": {"xdr": usd(0x3a)},
                            "expected": "greater"}),
        json!({"op": "asset_order", "a": {"xdr": usd(0x3a)}, "b": {"xdr": usd(0)},
                            "expected": "less"}),
    ];
    let text = serde_json::to_string(&json!({"entry": wrong, "cases": cases})).unwrap();
    write(
        &c.join("vectors/ordering/t.order-wrong.json"),
        text.as_bytes(),
    );
    let mut wrong = wrong;
    wrong["vector"] = json!({"path": "vectors/ordering/t.order-wrong.json",
        "sha256": sha256_hex(text.as_bytes())});
    entries.push(wrong);

    let map = ScVal::Map(Some(ScMap(
        vec![ScMapEntry {
            key: ScVal::U32(1),
            val: ScVal::U32(10),
        }]
        .try_into()
        .unwrap(),
    )));
    let native = entry("t.native", "native-projection", "must-accept");
    let cases = vec![
        json!({"op": "to_native", "value": {"xdr": B64.encode(enc(&map))},
                            "expected": {"rule": "no-entry-lost", "entry_count": 1}}),
    ];
    let text = serde_json::to_string(&json!({"entry": native, "cases": cases})).unwrap();
    write(
        &c.join("vectors/native-projection/t.native.json"),
        text.as_bytes(),
    );
    let mut native = native;
    native["vector"] = json!({"path": "vectors/native-projection/t.native.json",
        "sha256": sha256_hex(text.as_bytes())});
    entries.push(native);

    let manifest = json!({
        "schema_version": 1,
        "corpus_version": "test",
        "generated_with": {
            "stellar_xdr": "30.0.0",
            "xdr_definitions": {"tag": "v30.0", "commit": "unchecked"},
            "core_image": {"repository": "unchecked", "tag": "unchecked", "digest": "unchecked"},
            "soroban_env_host": "30.0.0"
        },
        "protocol_tiers": [],
        "entries": entries,
    });
    write(
        &c.join("manifest.json"),
        serde_json::to_string(&manifest).unwrap().as_bytes(),
    );
    Corpus { dir }
}

fn options(c: &Corpus, passes: u32) -> RunOptions {
    RunOptions {
        root: root(),
        manifest: c.manifest(),
        versions: versions_file(),
        known_dir: c.known(),
        sdks: vec!["rust".into()],
        only_versions: Vec::new(),
        all_versions: false,
        selection: Selection::default(),
        build: false,
        out: Some(c.dir.path().join("out")),
        passes,
        command: None,
        coverage: false,
    }
}

fn result_of(m: &runner::Matrix, id: &str) -> ResultClass {
    m.entries.iter().find(|e| e.id == id).unwrap().cells["rust@30.0.0"].result
}

#[test]
fn run_classifies_and_writes_the_matrix() {
    rust_testees();
    let c = corpus();
    let out = runner::run(&options(&c, 2)).unwrap();
    let m = &out.matrix;
    assert_eq!(result_of(m, "t.accept"), ResultClass::Pass);
    assert_eq!(result_of(m, "t.reject"), ResultClass::Pass);
    assert_eq!(
        result_of(m, "t.order-wrong"),
        ResultClass::DivergesFromNetwork
    );
    assert_eq!(result_of(m, "t.native"), ResultClass::Unsupported);
    // decode_encode, decode_json and encode_json all ran on the accepted value; rust 30.0.0
    // rendered the expected json, so its two json checks are the reference, not passes
    let accept = &m.entries.iter().find(|e| e.id == "t.accept").unwrap().cells["rust@30.0.0"];
    assert_eq!(accept.counts[&ResultClass::Pass], 1);
    assert_eq!(accept.counts[&ResultClass::Reference], 2);
    assert_eq!(
        accept.reference_checks,
        vec!["any/decode_json".to_string(), "any/encode_json".to_string()]
    );
    assert_eq!(accept.result, ResultClass::Pass);
    assert!(accept.checks.is_empty());
    assert_eq!(m.summary["rust@30.0.0"][&ResultClass::Reference], 1);
    // the native entry is unsupported as an entry; the op and check totals say why
    let ops = &m.op_summary["rust@30.0.0"];
    assert_eq!(ops["asset_order"][&ResultClass::DivergesFromNetwork], 2);
    assert_eq!(ops["to_native"][&ResultClass::Unsupported], 1);
    assert_eq!(ops["decode_json"][&ResultClass::Reference], 1);
    let checks = &m.check_summary["rust@30.0.0"];
    assert_eq!(checks[&ResultClass::Reference], 2);
    assert_eq!(checks[&ResultClass::Pass], 2);
    assert_eq!(m.testees[0].reference_for.as_deref(), Some("expected_json"));
    assert_eq!(out.peak_rss_kib.len(), 2);
    assert!(!out.ok, "an unlisted divergence fails the run");
    let written = runner::load_matrix(&c.dir.path().join("out/divergence-matrix.json")).unwrap();
    assert_eq!(&written, m);
    let md = std::fs::read_to_string(c.dir.path().join("out/MATRIX.md")).unwrap();
    assert!(md.contains("`t.order-wrong`"));
    assert!(md.contains(
        "t.order-wrong on 30.0.0: `case 0/asset_order` is diverges-from-network and not in the known-failures list"
    ));
    assert!(md.contains("rust@30.0.0 is the reference"), "{md}");
    assert!(md.contains("reference for expected_json"));
    assert!(md.contains("## Summary (checks per op)"));
    assert!(
        md.contains("| rust@30.0.0 | to_native | 0 | 0 | 0 | 0 | 1 | 0 |"),
        "{md}"
    );
}

#[test]
fn only_the_generator_version_is_the_reference() {
    rust_testees();
    let c = corpus();
    let mut opts = options(&c, 1);
    opts.only_versions = vec!["28.0.1".into()];
    let m = runner::run(&opts).unwrap().matrix;
    let accept = &m.entries.iter().find(|e| e.id == "t.accept").unwrap().cells["rust@28.0.1"];
    assert_eq!(accept.counts[&ResultClass::Pass], 3);
    assert!(accept.reference_checks.is_empty());
    assert!(m.testees[0].reference_for.is_none());
}

#[test]
fn known_failures_semantics() {
    rust_testees();
    let c = corpus();
    let list = known::path(&c.known(), "rust");
    write(
        &list,
        b"# test list\nt.order-wrong 30.0.0 diverges-from-network findings/issues/x.md case 0/asset_order, case 1/asset_order\n",
    );
    let out = runner::run(&options(&c, 1)).unwrap();
    assert!(out.ok, "{:?}", out.matrix.known_failure_problems);
    let cell = &out
        .matrix
        .entries
        .iter()
        .find(|e| e.id == "t.order-wrong")
        .unwrap()
        .cells["rust@30.0.0"];
    assert_eq!(cell.known_issue.as_deref(), Some("findings/issues/x.md"));

    // removing the line makes the run fail
    write(&list, b"");
    assert!(!runner::run(&options(&c, 1)).unwrap().ok);

    // a new failing check next to a listed one in the same entry makes it fail
    write(
        &list,
        b"t.order-wrong * diverges-from-network x case 0/asset_order\n",
    );
    let out = runner::run(&options(&c, 1)).unwrap();
    assert_eq!(
        out.matrix.known_failure_problems.len(),
        1,
        "{:?}",
        out.matrix.known_failure_problems
    );
    assert!(out.matrix.known_failure_problems[0].contains("`case 1/asset_order` is"));

    // listing a check that passes makes it fail
    write(
        &list,
        b"t.order-wrong * diverges-from-network x case 0/asset_order, case 1/asset_order\nt.accept * diverges-from-network x any/decode_encode\n",
    );
    let out = runner::run(&options(&c, 1)).unwrap();
    assert!(!out.ok);
    assert!(
        out.matrix.known_failure_problems[0]
            .contains("t.accept on 30.0.0: `any/decode_encode` is listed as a failure"),
        "{:?}",
        out.matrix.known_failure_problems
    );

    // a reference check is not a failure either
    write(
        &list,
        b"t.order-wrong * diverges-from-network x case 0/asset_order, case 1/asset_order\nt.accept * diverges-from-network x any/decode_json\n",
    );
    let out = runner::run(&options(&c, 1)).unwrap();
    assert!(out.matrix.known_failure_problems[0].contains("but is reference"));

    // listing the wrong result makes it fail
    write(
        &list,
        b"t.order-wrong * error x case 0/asset_order, case 1/asset_order\n",
    );
    assert!(!runner::run(&options(&c, 1)).unwrap().ok);

    // a check the entry does not have makes it fail
    write(
        &list,
        b"t.order-wrong * diverges-from-network x case 0/asset_order, case 1/asset_order, case 2/asset_order\n",
    );
    let out = runner::run(&options(&c, 1)).unwrap();
    assert!(
        out.matrix.known_failure_problems[0].contains("not a check of this entry"),
        "{:?}",
        out.matrix.known_failure_problems
    );

    // a line for an entry that does not exist makes it fail
    write(
        &list,
        b"t.order-wrong * diverges-from-network x case 0/asset_order, case 1/asset_order\nt.gone * error x a/b\n",
    );
    assert!(!runner::run(&options(&c, 1)).unwrap().ok);
}

fn cell_checks(pairs: &[(&str, ResultClass)]) -> known::CellChecks {
    pairs.iter().map(|(c, r)| (c.to_string(), *r)).collect()
}

#[test]
fn known_failures_are_judged_per_check() {
    let entries = BTreeSet::from(["a".to_string()]);
    let versions = BTreeSet::from(["1.0".to_string(), "2.0".to_string()]);
    let lines = known::parse(
        "a * diverges-from-network x case 0/id_parse\na 2.0 error y case 1/id_parse\n",
    )
    .unwrap();
    let results = |c0, c1| {
        BTreeMap::from([(
            ("a".to_string(), "2.0".to_string()),
            cell_checks(&[("case 0/id_parse", c0), ("case 1/id_parse", c1)]),
        )])
    };
    let eval = |r| known::evaluate(&lines, &r, &entries, &versions, false);
    assert!(
        eval(results(
            ResultClass::DivergesFromNetwork,
            ResultClass::Error
        ))
        .is_empty()
    );
    // a regression in a listed entry: case 1 now diverges where only an error was known
    let p = eval(results(
        ResultClass::DivergesFromNetwork,
        ResultClass::DivergesFromNetwork,
    ));
    assert!(
        matches!(&p[..], [known::Problem::WrongResult { check, .. }] if check == "case 1/id_parse")
    );
    // a fixed check must leave the list
    let p = eval(results(ResultClass::Pass, ResultClass::Error));
    assert!(
        matches!(&p[..], [known::Problem::NoLongerFails { check, .. }] if check == "case 0/id_parse")
    );
    // on 1.0 only the star line applies, so case 1 failing there is unlisted
    let on_1 = BTreeMap::from([(
        ("a".to_string(), "1.0".to_string()),
        cell_checks(&[
            ("case 0/id_parse", ResultClass::DivergesFromNetwork),
            ("case 1/id_parse", ResultClass::Error),
        ]),
    )]);
    let p = eval(on_1);
    assert!(
        matches!(&p[..], [known::Problem::Unlisted { check, .. }] if check == "case 1/id_parse")
    );
    assert_eq!(
        known::issue_for(&lines, "a", "2.0", ["case 0/id_parse", "case 1/id_parse"]).as_deref(),
        Some("x, y")
    );
    assert_eq!(
        known::issue_for(&lines, "a", "1.0", ["case 1/id_parse"]),
        None
    );
}

// a peers line can only be reproduced when other sdks ran, so a one-sdk run that passes the
// entry must not ask for the line to be removed, while a run with other sdks must
#[test]
fn peers_lines_need_other_sdks() {
    let lines = known::parse("a * diverges-from-peers x any/decode_json\n").unwrap();
    let results = BTreeMap::from([(
        ("a".to_string(), "1.0".to_string()),
        cell_checks(&[("any/decode_json", ResultClass::Pass)]),
    )]);
    let entries = BTreeSet::from(["a".to_string()]);
    let versions = BTreeSet::from(["1.0".to_string()]);
    assert!(known::evaluate(&lines, &results, &entries, &versions, false).is_empty());
    assert_eq!(
        known::evaluate(&lines, &results, &entries, &versions, true).len(),
        1
    );
    let failing = BTreeMap::from([(
        ("a".to_string(), "1.0".to_string()),
        cell_checks(&[("any/decode_json", ResultClass::DivergesFromPeers)]),
    )]);
    assert!(known::evaluate(&lines, &failing, &entries, &versions, true).is_empty());
}

#[test]
fn known_failures_file_format() {
    let lines = known::parse(
        "a 1.0 diverges-from-peers https://x case 0/map_sort,case 1/map_sort # c\n\n# only a comment\n",
    )
    .unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].version.as_deref(), Some("1.0"));
    assert_eq!(
        lines[0].checks,
        BTreeSet::from(["case 0/map_sort".to_string(), "case 1/map_sort".to_string()])
    );
    assert!(
        known::parse("a * pass https://x any/decode_json").is_err(),
        "pass is not a failure"
    );
    assert!(
        known::parse("a * error x").is_err(),
        "the check list is required"
    );
    assert!(
        known::parse("a * error x any/decode_json,").is_err(),
        "an empty check item"
    );
    assert!(
        known::parse("a * error x any/decode_json, any/decode_json").is_err(),
        "a check listed twice"
    );
    assert!(
        known::parse("a * error x any/decode_json\na 1.0 error y any/decode_json").is_err(),
        "a star line and a version line share a check"
    );
    assert!(
        known::parse("a * error x any/decode_json\na 1.0 diverges-from-network y any/encode_json")
            .is_ok(),
        "the same entry and version with other checks"
    );
    assert!(known::parse("a 1.0 error x any/decode_json\na 2.0 error y any/decode_json").is_ok());
}

#[test]
fn an_issue_url_keeps_its_fragment() {
    let lines = known::parse(
        "a * error https://github.com/o/r/pull/1#issuecomment-2 any/decode_json # why\n  # indented comment\n",
    )
    .unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(
        lines[0].issue,
        "https://github.com/o/r/pull/1#issuecomment-2"
    );
    assert_eq!(
        lines[0].checks,
        BTreeSet::from(["any/decode_json".to_string()])
    );
    assert_eq!(known::strip_comment("x#y #z"), "x#y ");
    assert_eq!(known::strip_comment("#all"), "");
}

#[test]
fn reproduce_shows_the_ghsa_flaw_on_28_0_0_only() {
    rust_testees();
    let c = corpus();
    let file = c.dir.path().join("reproductions.toml");
    write(
        &file,
        br#"schema = 1
[[reproduction]]
id = "ghsa"
sighting = "test"
link = "https://example.invalid"
sdk = "rust"
entries = ["t.reject"]
fail = ["28.0.0"]
pass = ["28.0.1", "30.0.0"]
must_diverge = [{ probe = "advisory", checks = ["probe/encode_json"] }]
[[reproduction.probe]]
name = "advisory"
op = "encode_json"
request = { type = "ScSpecFunctionV0", json = { doc = "", name = "f", inputs = [], outputs = ["u32", "u32"] } }
expect = "rejected"
source = "test"
"#,
    );
    let cells = reproduce::run(&reproduce::ReproduceOptions {
        root: root(),
        manifest: c.manifest(),
        versions: versions_file(),
        reproductions: file,
        sdks: vec!["rust".into()],
        build: false,
    })
    .unwrap();
    let got: Vec<(&str, Observed)> = cells
        .iter()
        .map(|c| (c.version.as_str(), c.observed))
        .collect();
    assert_eq!(
        got,
        vec![
            ("28.0.0", Observed::Fail),
            ("28.0.1", Observed::Pass),
            ("30.0.0", Observed::Pass)
        ]
    );
    assert!(cells.iter().all(reproduce::Cell::holds));
}

#[test]
fn draft_issues_for_a_divergence() {
    rust_testees();
    let c = corpus();
    let out = runner::run(&options(&c, 1)).unwrap();
    let (manifest, corpus_dir) = runner::load_manifest(&c.manifest()).unwrap();
    let file = VersionsFile::load(&versions_file()).unwrap();
    let drafts =
        shapekit_conformance::issues::drafts(&out.matrix, &manifest, &corpus_dir, &file).unwrap();
    assert_eq!(drafts.len(), 1);
    let d = &drafts[0];
    assert_eq!(d.file, "rust-t.order-wrong.md");
    assert_eq!(d.repo, "stellar/rs-stellar-xdr");
    assert!(
        d.body
            .contains("shapekit conformance run --sdk rust --version 30.0.0 --entry t.order-wrong")
    );
    assert!(d.body.contains("\"op\": \"asset_order\""));
    // an upstream maintainer has no checkout, so sources are links at the release tag
    assert!(
        d.body
            .contains("https://github.com/ErenTopaal/shapekit/blob/v")
    );
    assert!(!d.body.contains("findings/issues/"));
    let (repo, title, _, vector) =
        shapekit_conformance::issues::parse(&shapekit_conformance::issues::render(d)).unwrap();
    assert_eq!(
        (repo.as_str(), vector.as_str()),
        ("stellar/rs-stellar-xdr", "t.order-wrong")
    );
    assert_eq!(title, d.title);
}

#[test]
fn file_issues_refuses_without_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let err = shapekit_conformance::issues::file(&shapekit_conformance::issues::FileOptions {
        issues: dir.path().to_path_buf(),
        only: Vec::new(),
        confirm: false,
    })
    .unwrap_err();
    assert!(err.to_string().contains("--confirm"));
}

#[test]
fn corpus_cases_map_to_protocol_requests() {
    let strkey = json!({"op": "strkey_decode", "text": "GAAA",
        "expected": {"kind": "signed_payload_ed25519", "ed25519": "00".repeat(32), "payload": "0102030405"}});
    let checks = plan::case_checks(0, &strkey, "test");
    assert_eq!(
        checks.len(),
        2,
        "a valid strkey is decoded and encoded back"
    );
    let Expect::Strkey { kind, payload } = &checks[0].expect else {
        panic!("{:?}", checks[0].expect)
    };
    assert_eq!(kind, "signed_payload");
    let mut raw = vec![0u8; 32];
    raw.extend_from_slice(&[0, 0, 0, 5, 1, 2, 3, 4, 5, 0, 0, 0]);
    assert_eq!(payload, &B64.encode(raw));
    assert_eq!(checks[1].expect, Expect::Text("GAAA".into()));

    let id = json!({"op": "id_parse", "kind": "offer", "input": {"number": 9007199254740993u64},
                    "expected": {"reject": "unsafe-integer"}});
    let checks = plan::case_checks(1, &id, "test");
    assert_eq!(checks[0].request["input"], json!(9007199254740993u64));
    assert_eq!(checks[0].expect, Expect::Rejected);

    let either = json!({"op": "id_parse", "kind": "offer", "input": {"number": 9007199254740993u64},
                        "expected": {"either": [{"id": "9007199254740993"}, {"reject": "unsafe-integer"}]}});
    let c = &plan::case_checks(3, &either, "test")[0];
    assert_eq!(c.expect, Expect::IdOrRejected("9007199254740993".into()));
    use shapekit_conformance::compare::{Judged, judge};
    let verdict = |reply: Reply| match judge(&c.expect, &Ok(reply)) {
        Judged::Done(v) => v.result,
        Judged::Peer(_) => panic!("decided per case"),
    };
    assert_eq!(
        verdict(Reply::Ok(json!({"id": "9007199254740993"}))),
        ResultClass::Pass
    );
    assert_eq!(
        verdict(Reply::Error {
            class: ErrorClass::Rejected,
            message: "unsafe".into()
        }),
        ResultClass::Pass
    );
    assert_eq!(
        verdict(Reply::Ok(json!({"id": "9007199254740992"}))),
        ResultClass::DivergesFromNetwork
    );

    let unknown = json!({"op": "teleport", "expected": 1});
    assert!(matches!(
        plan::case_checks(2, &unknown, "test")[0].expect,
        Expect::Unmappable(_)
    ));
}

#[test]
fn implementation_defined_answers_are_recorded_not_judged() {
    use shapekit_conformance::compare::judge_peers;
    let all = [true, true, true];
    let split = judge_peers(&[Some(json!(1)), Some(json!(1)), Some(json!(2))], &all);
    for v in &split {
        assert_eq!(v.as_ref().unwrap().result, ResultClass::Pass);
    }
    assert_eq!(split[2].as_ref().unwrap().actual, json!(2));
    assert!(
        split[2]
            .as_ref()
            .unwrap()
            .detail
            .as_deref()
            .unwrap()
            .contains("2 different answers, 1 of them this one")
    );
    let none = judge_peers(&[Some(json!(1)), None], &[true, true]);
    assert!(none[1].is_none());
    let alone = judge_peers(&[Some(json!(1))], &[true]);
    assert_eq!(alone[0].as_ref().unwrap().result, ResultClass::Pass);
}

// an sdk run at many versions speaks once in the spread of answers
#[test]
fn one_vote_per_sdk() {
    use shapekit_conformance::compare::judge_peers;
    let file = VersionsFile::load(&versions_file()).unwrap();
    let mut targets = Vec::new();
    for (sdk, v) in [
        ("go", "v0.7.1"),
        ("go", "v0.8.0"),
        ("go", "v0.9.0"),
        ("rust", "30.0.0"),
        ("js", "17.4.0"),
    ] {
        targets.push(file.target(sdk, v).unwrap());
    }
    let refs: Vec<&Target> = targets.iter().collect();
    let voters = runner::voters(&refs);
    let current_go = file.sdk("go").unwrap().current.clone();
    let expected: Vec<bool> = targets
        .iter()
        .map(|t| t.sdk != "go" || t.version == current_go)
        .collect();
    assert_eq!(voters, expected);
    let outcomes = [
        Some(json!("rejected")),
        Some(json!("rejected")),
        Some(json!("rejected")),
        Some(json!({"xdr": "AAAA"})),
        Some(json!({"xdr": "AAAA"})),
    ];
    // every answer is recorded as a pass; the spread counts go once, not three times
    let v = judge_peers(&outcomes, &voters);
    assert!(
        v.iter()
            .all(|x| x.as_ref().unwrap().result == ResultClass::Pass)
    );
    let detail = v[0].as_ref().unwrap().detail.clone().unwrap();
    assert!(
        detail.contains("the 3 sdks gave 2 different answers, 1 of them this one"),
        "{detail}"
    );
    let go_only = &refs[..3];
    let v = judge_peers(&outcomes[..3], &runner::voters(go_only));
    assert!(
        v.iter()
            .all(|x| x.as_ref().unwrap().result == ResultClass::Pass)
    );
    // without the current version the last one in the run votes
    let old = [&targets[0], &targets[1]];
    assert_eq!(runner::voters(&old), vec![false, true]);
}

#[test]
fn crashes_are_not_downgraded_by_max_protocol() {
    use shapekit_conformance::compare::{Judged, judge};
    let c = corpus();
    let (manifest, _) = runner::load_manifest(&c.manifest()).unwrap();
    let entry = manifest
        .entries
        .iter()
        .find(|e| e.id == "t.accept")
        .unwrap();
    let mut target = fixture("ok", SMALL);
    target.max_protocol = Some(runner::entry_protocol(entry) - 1);
    let verdict = |answer| match judge(&Expect::Bytes("AAAA".into()), &answer) {
        Judged::Done(v) => runner::beyond_protocol(entry, &target, v),
        Judged::Peer(_) => unreachable!(),
    };
    let panic = verdict(Ok(Reply::Error {
        class: ErrorClass::Panic,
        message: "boom".into(),
    }));
    assert_eq!(panic.result, ResultClass::DivergesFromNetwork);
    let died = verdict(Err(Failure::Exited {
        status: "signal 9".into(),
        stderr: String::new(),
    }));
    assert_eq!(died.result, ResultClass::Error);
    assert!(died.crashed);
    // an ordinary divergence on a newer protocol is still not held against the version
    let rejected = verdict(Ok(Reply::Error {
        class: ErrorClass::Rejected,
        message: "unknown arm".into(),
    }));
    assert_eq!(rejected.result, ResultClass::Unsupported);
}

fn reproductions(c: &Corpus, text: &str) -> PathBuf {
    let file = c.dir.path().join("reproductions.toml");
    write(&file, text.as_bytes());
    file
}

fn reproduce_rust(c: &Corpus, file: PathBuf) -> anyhow::Result<Vec<reproduce::Cell>> {
    reproduce::run(&reproduce::ReproduceOptions {
        root: root(),
        manifest: c.manifest(),
        versions: versions_file(),
        reproductions: file,
        sdks: vec!["rust".into()],
        build: false,
    })
}

const ROW: &str = r#"schema = 1
[[reproduction]]
id = "row"
sighting = "test"
link = "https://example.invalid"
sdk = "rust"
"#;

#[test]
fn a_version_in_both_fail_and_pass_is_refused() {
    let c = corpus();
    let file = reproductions(
        &c,
        &format!(
            "{ROW}entries = [\"t.accept\"]\nfail = [\"28.0.1\"]\npass = [\"28.0.1\", \"30.0.0\"]\nmust_diverge = [{{ entry = \"t.accept\", checks = [\"any/decode_encode\"] }}]\n"
        ),
    );
    let err = reproduce::ReproductionsFile::load(&file).unwrap_err();
    assert!(err.to_string().contains("both fail and pass"), "{err}");
    let file = reproductions(
        &c,
        &format!("{ROW}entries = [\"t.accept\"]\nfail = [\"28.0.1\"]\npass = [\"30.0.0\"]\n"),
    );
    assert!(
        reproduce::ReproductionsFile::load(&file).is_err(),
        "must_diverge is required"
    );
}

// the row's entries diverge on every version, but not in the check the sighting names, so the
// row must not count as reproduced
#[test]
fn a_reproduction_needs_its_named_checks_to_diverge() {
    rust_testees();
    let c = corpus();
    let text = |named: &str| {
        format!(
            "{ROW}entries = [\"t.accept\", \"t.order-wrong\"]\nfail = [\"30.0.0\"]\npass = [\"28.0.1\"]\nmust_diverge = [{{ entry = \"{named}\", checks = [\"case 0/asset_order\"] }}]\n"
        )
    };
    let file = reproductions(&c, &text("t.order-wrong"));
    let cells = reproduce_rust(&c, file).unwrap();
    let got: Vec<(&str, Observed)> = cells
        .iter()
        .map(|c| (c.version.as_str(), c.observed))
        .collect();
    // fails where it must; on the pass version the named check still diverges
    assert_eq!(
        got,
        vec![("28.0.1", Observed::Fail), ("30.0.0", Observed::Fail)]
    );
    assert!(cells[1].holds() && !cells[0].holds());

    let named_pass = format!(
        "{ROW}entries = [\"t.accept\", \"t.order-wrong\"]\nfail = [\"30.0.0\"]\npass = [\"28.0.1\"]\nmust_diverge = [{{ entry = \"t.accept\", checks = [\"any/decode_encode\"] }}]\n"
    );
    let cells = reproduce_rust(&c, reproductions(&c, &named_pass)).unwrap();
    assert!(cells.iter().all(|c| c.observed == Observed::Mixed));
    assert!(cells.iter().all(|c| !c.holds()));

    let err = reproduce_rust(&c, reproductions(&c, &text("t.reject"))).unwrap_err();
    assert!(err.to_string().contains("not a counted check"), "{err}");
}

fn existing(title: &str, entry: &str) -> String {
    shapekit_conformance::issues::render(&shapekit_conformance::issues::Draft {
        file: String::new(),
        sdk: "rust".into(),
        entry: entry.into(),
        repo: "stellar/rs-stellar-xdr".into(),
        title: title.into(),
        body: format!("hand kept\n\nshapekit vector: `{entry}`\n"),
    })
}

#[test]
fn draft_issues_only_prunes_cells_the_matrix_covers() {
    use shapekit_conformance::issues;
    rust_testees();
    let c = corpus();
    let out = c.dir.path().join("out");
    runner::run(&options(&c, 1)).unwrap();
    let drafts = c.dir.path().join("drafts");
    let wide = existing(
        "stellar-xdr 28.0.0, 30.0.0: x (t.order-wrong)",
        "t.order-wrong",
    );
    let stale = existing("stellar-xdr 30.0.0: x (t.accept)", "t.accept");
    let other_version = existing("stellar-xdr 28.0.1: x (t.reject)", "t.reject");
    let outside = existing("stellar-xdr 30.0.0: x (t.elsewhere)", "t.elsewhere");
    write(&drafts.join("rust-t.order-wrong.md"), wide.as_bytes());
    write(&drafts.join("rust-t.accept.md"), stale.as_bytes());
    write(&drafts.join("rust-t.reject.md"), other_version.as_bytes());
    write(&drafts.join("rust-t.elsewhere.md"), outside.as_bytes());
    write(&drafts.join("go-t.accept.md"), stale.as_bytes());
    let opts = issues::DraftOptions {
        matrix: out.join("divergence-matrix.json"),
        manifest: c.manifest(),
        versions: versions_file(),
        out: drafts.clone(),
    };
    let report = issues::draft(&opts).unwrap();
    assert_eq!(report.removed, vec!["rust-t.accept.md".to_string()]);
    assert!(report.written.is_empty(), "{:?}", report.written);
    let kept: Vec<&str> = report.kept.iter().map(|(f, _)| f.as_str()).collect();
    assert_eq!(kept, vec!["rust-t.order-wrong.md", "rust-t.reject.md"]);
    assert!(report.kept[0].1.contains("28.0.0"));
    let read = |f: &str| std::fs::read_to_string(drafts.join(f)).unwrap();
    assert_eq!(
        read("rust-t.order-wrong.md"),
        wide,
        "a wider draft is not shrunk"
    );
    assert_eq!(read("rust-t.reject.md"), other_version);
    assert_eq!(read("rust-t.elsewhere.md"), outside);
    assert_eq!(read("go-t.accept.md"), stale);

    // a draft naming only covered versions is replaced
    let narrow = existing("stellar-xdr 30.0.0: x (t.order-wrong)", "t.order-wrong");
    write(&drafts.join("rust-t.order-wrong.md"), narrow.as_bytes());
    let report = issues::draft(&opts).unwrap();
    assert_eq!(report.written, vec!["rust-t.order-wrong.md".to_string()]);
    assert!(read("rust-t.order-wrong.md").contains("--entry t.order-wrong"));
}

#[test]
fn title_versions_reads_drafted_titles() {
    use shapekit_conformance::issues::title_versions;
    assert_eq!(
        title_versions(
            "github.com/stellar/go-stellar-sdk v0.7.1, v0.8.0: the sdk: odd (e)",
            "github.com/stellar/go-stellar-sdk"
        ),
        Some(BTreeSet::from(["v0.7.1".to_string(), "v0.8.0".to_string()]))
    );
    assert_eq!(title_versions("stellar-xdr: x", "stellar-xdr"), None);
    assert_eq!(title_versions("other 1.0: x", "stellar-xdr"), None);
}

#[test]
fn file_issues_matches_the_exact_marker_and_package() {
    use shapekit_conformance::issues::matching_issue;
    let issue = |url: &str, body: &str| json!({"url": url, "body": body});
    let found = vec![
        // search ignores punctuation, so a longer id comes back too
        issue(
            "https://x/1",
            "`@stellar/stellar-sdk` 17.4.0 ...\n\nshapekit vector: `a.b-c`\n",
        ),
        // the right vector, another sdk
        issue(
            "https://x/2",
            "`stellar-xdr` 30.0.0 ...\n\nshapekit vector: `a.b`\n",
        ),
        issue(
            "https://x/3",
            "`@stellar/stellar-sdk` 17.4.0 ...\n\nshapekit vector: `a.b`\n",
        ),
    ];
    assert_eq!(
        matching_issue(&found, "a.b", "@stellar/stellar-sdk").as_deref(),
        Some("https://x/3")
    );
    assert_eq!(
        matching_issue(&found[..2], "a.b", "@stellar/stellar-sdk"),
        None
    );
    assert_eq!(
        matching_issue(&found, "a.b-c", "@stellar/stellar-sdk").as_deref(),
        Some("https://x/1")
    );
}

// a versions file whose sdks are the fixture in json modes, as independent decoders
fn fixture_versions(c: &Corpus, modes: &[(&str, &str)]) -> PathBuf {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fixture-testee.sh");
    let mut text = String::from("schema = 1\n");
    for (sdk, mode) in modes {
        text.push_str(&format!(
            "[sdk.{sdk}]\npackage = \"fixture\"\ncurrent = \"1.0.0\"\nissues_repo = \"o/r\"\ntimeout_ms = 5000\nmemory = {{ resource = \"as\", mib = 512 }}\n[[sdk.{sdk}.versions]]\nversion = \"1.0.0\"\nbuild = []\ncommand = [\"sh\", \"{}\"]\nenv = {{ FIXTURE_MODE = \"{mode}\" }}\n",
            script.display()
        ));
    }
    let name: Vec<String> = modes.iter().map(|(s, m)| format!("{s}-{m}")).collect();
    let path = c
        .dir
        .path()
        .join(format!("versions-{}.toml", name.join("-")));
    write(&path, text.as_bytes());
    path
}

fn crosscheck(c: &Corpus, versions: PathBuf) -> shapekit_conformance::crosscheck::Report {
    use shapekit_conformance::crosscheck::{CrosscheckOptions, run};
    run(&CrosscheckOptions {
        root: root(),
        manifest: c.manifest(),
        versions,
        known_dir: c.known(),
        sdks: vec!["a".into(), "b".into()],
        selection: Selection::default(),
        build: false,
        out: Some(c.dir.path().join("crosscheck")),
    })
    .unwrap()
}

#[test]
fn crosscheck_classifies_disagreements() {
    use shapekit_conformance::crosscheck::{Answer, Class};
    let c = corpus();
    // t.accept's expected json is {"u32": 7}
    let r = crosscheck(&c, fixture_versions(&c, &[("a", "json7"), ("b", "json7")]));
    assert_eq!(r.checks, 1);
    assert!(r.disagreements.is_empty() && r.ok());
    assert_eq!(r.confirmed_by[&2], 1);

    let r = crosscheck(&c, fixture_versions(&c, &[("a", "json8"), ("b", "json8")]));
    assert_eq!(r.disagreements[0].class, Class::PossibleStellarXdrRendering);
    assert!(!r.ok());
    assert_eq!(
        r.disagreements[0].answers["a@1.0.0"],
        Answer::Differs {
            at: "/u32".into(),
            expected: json!(7),
            actual: json!(8)
        }
    );

    let r = crosscheck(&c, fixture_versions(&c, &[("a", "json8"), ("b", "json9")]));
    assert_eq!(r.disagreements[0].class, Class::AllDiffer);

    let r = crosscheck(&c, fixture_versions(&c, &[("a", "json7"), ("b", "json8")]));
    assert_eq!(r.disagreements[0].class, Class::UnknownSdkDivergence);
    assert!(!r.ok());
    write(
        &known::path(&c.known(), "b"),
        b"t.accept * diverges-from-network findings/issues/b.md any/decode_json\n",
    );
    let r = crosscheck(&c, fixture_versions(&c, &[("a", "json7"), ("b", "json8")]));
    assert_eq!(r.disagreements[0].class, Class::KnownSdkDivergence);
    assert_eq!(r.disagreements[0].known["b@1.0.0"], "findings/issues/b.md");
    assert!(r.ok());
    let md = std::fs::read_to_string(c.dir.path().join("crosscheck/CROSSCHECK.md")).unwrap();
    assert!(md.contains("already in its known-failures list"), "{md}");
}

#[test]
fn crosscheck_refuses_the_reference_itself() {
    use shapekit_conformance::crosscheck::{CrosscheckOptions, run};
    let c = corpus();
    let err = run(&CrosscheckOptions {
        root: root(),
        manifest: c.manifest(),
        versions: versions_file(),
        known_dir: c.known(),
        sdks: vec!["rust".into()],
        selection: Selection::default(),
        build: false,
        out: None,
    })
    .err()
    .unwrap();
    assert!(
        err.to_string().contains("rendered expected_json itself"),
        "{err}"
    );
}

#[test]
fn first_difference_points_at_the_change() {
    use shapekit_conformance::crosscheck::first_difference;
    let a = json!({"x": [1, {"y": "a"}], "z": 1});
    assert_eq!(first_difference(&a, &a), None);
    let b = json!({"x": [1, {"y": "b"}], "z": 1});
    assert_eq!(
        first_difference(&a, &b),
        Some(("/x/1/y".into(), json!("a"), json!("b")))
    );
    let extra = json!({"x": [1, {"y": "a"}], "z": 1, "w": 2});
    assert_eq!(
        first_difference(&a, &extra),
        Some(("/w".into(), json!("(absent)"), json!(2)))
    );
}
