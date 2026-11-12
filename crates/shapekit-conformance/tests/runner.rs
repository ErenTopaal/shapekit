//! Runner tests: the protocol client against a stand-in testee, and full runs, reproductions
//! and known-failures semantics against the real rust testee over a small corpus written here.

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
use shapekit_conformance::versions::{MemoryLimit, MemoryResource, Target, VersionsFile};
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
    write(&c.join("entries/t/t.accept/any.xdr"), &u32_7);
    write(
        &c.join("entries/t/t.accept/any.expected.json"),
        b"{\"u32\": 7}\n",
    );
    let mut accept = entry("t.accept", "xdr", "must-accept");
    accept["xdr_type"] = json!("ScVal");
    accept["variants"] = json!({"any": {
        "xdr_file": "entries/t/t.accept/any.xdr", "xdr_sha256": "unchecked",
        "expected_json_file": "entries/t/t.accept/any.expected.json"}});
    entries.push(accept);

    let mut trailing = u32_7.clone();
    trailing.extend_from_slice(&[0, 0]);
    write(&c.join("entries/t/t.reject/any.xdr"), &trailing);
    let mut reject = entry("t.reject", "xdr", "must-reject");
    reject["xdr_type"] = json!("ScVal");
    reject["variants"] = json!({"any": {
        "xdr_file": "entries/t/t.reject/any.xdr", "xdr_sha256": "unchecked"}});
    entries.push(reject);

    // the expectation is the strkey-text order, so a correct sdk diverges from it: a stand-in
    // for a real divergence that the known-failures tests can list and unlist
    let wrong = entry("t.order-wrong", "ordering", "must-accept");
    let cases = vec![
        json!({"op": "asset_order", "a": {"xdr": usd(0)}, "b": {"xdr": usd(0x3a)},
                            "expected": "greater"}),
    ];
    write(
        &c.join("vectors/ordering/t.order-wrong.json"),
        serde_json::to_string(&json!({"entry": wrong, "cases": cases}))
            .unwrap()
            .as_bytes(),
    );
    let mut wrong = wrong;
    wrong["vector"] = json!({"path": "vectors/ordering/t.order-wrong.json", "sha256": "unchecked"});
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
    write(
        &c.join("vectors/native-projection/t.native.json"),
        serde_json::to_string(&json!({"entry": native, "cases": cases}))
            .unwrap()
            .as_bytes(),
    );
    let mut native = native;
    native["vector"] =
        json!({"path": "vectors/native-projection/t.native.json", "sha256": "unchecked"});
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
    // decode_encode, decode_json and encode_json all ran on the accepted value
    let accept = &m.entries.iter().find(|e| e.id == "t.accept").unwrap().cells["rust@30.0.0"];
    assert_eq!(accept.counts[&ResultClass::Pass], 3);
    assert_eq!(out.peak_rss_kib.len(), 2);
    assert!(!out.ok, "an unlisted divergence fails the run");
    let written = runner::load_matrix(&c.dir.path().join("out/divergence-matrix.json")).unwrap();
    assert_eq!(&written, m);
    let md = std::fs::read_to_string(c.dir.path().join("out/MATRIX.md")).unwrap();
    assert!(md.contains("`t.order-wrong`"));
    assert!(md.contains(
        "t.order-wrong on 30.0.0: diverges-from-network and not in the known-failures list"
    ));
}

#[test]
fn known_failures_semantics() {
    rust_testees();
    let c = corpus();
    let list = known::path(&c.known(), "rust");
    write(
        &list,
        b"# test list\nt.order-wrong 30.0.0 diverges-from-network findings/issues/x.md\n",
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

    // listing an entry that passes makes it fail
    write(
        &list,
        b"t.order-wrong * diverges-from-network x\nt.accept * diverges-from-network x\n",
    );
    let out = runner::run(&options(&c, 1)).unwrap();
    assert!(!out.ok);
    assert!(
        out.matrix.known_failure_problems[0].contains("t.accept on 30.0.0: listed as a failure"),
        "{:?}",
        out.matrix.known_failure_problems
    );

    // listing the wrong result makes it fail
    write(&list, b"t.order-wrong * error x\n");
    assert!(!runner::run(&options(&c, 1)).unwrap().ok);

    // a line for an entry that does not exist makes it fail
    write(
        &list,
        b"t.order-wrong * diverges-from-network x\nt.gone * error x\n",
    );
    assert!(!runner::run(&options(&c, 1)).unwrap().ok);
}

#[test]
fn known_failures_file_format() {
    let lines =
        known::parse("a 1.0 diverges-from-peers https://x # c\n\n# only a comment\n").unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].version.as_deref(), Some("1.0"));
    assert!(
        known::parse("a * pass https://x").is_err(),
        "pass is not a failure"
    );
    assert!(known::parse("a * error").is_err(), "the issue is required");
    assert!(
        known::parse("a * error x\na 1.0 error y").is_err(),
        "a star line and a version line overlap"
    );
    assert!(known::parse("a 1.0 error x\na 2.0 error y").is_ok());
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
    let (repo, title, _, vector) =
        shapekit_conformance::issues::parse(&shapekit_conformance::issues::render(d)).unwrap();
    assert_eq!(
        (repo.as_str(), vector.as_str()),
        ("stellar/rs-stellar-xdr", "t.order-wrong")
    );
    assert_eq!(title, d.title);
}

#[test]
fn file_issues_refuses_without_owner_approval() {
    let dir = tempfile::tempdir().unwrap();
    let err = shapekit_conformance::issues::file(&shapekit_conformance::issues::FileOptions {
        issues: dir.path().to_path_buf(),
        only: Vec::new(),
        owner_approval: false,
    })
    .unwrap_err();
    assert!(err.to_string().contains("--i-have-owner-approval"));
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
fn peers_are_compared_with_each_other() {
    use shapekit_conformance::compare::judge_peers;
    let same = judge_peers(&[Some(json!(1)), Some(json!(1)), None]);
    assert_eq!(same[0].as_ref().unwrap().result, ResultClass::Pass);
    assert!(same[2].is_none());
    let split = judge_peers(&[Some(json!(1)), Some(json!(1)), Some(json!(2))]);
    assert_eq!(
        split[2].as_ref().unwrap().result,
        ResultClass::DivergesFromPeers
    );
    let tie = judge_peers(&[Some(json!(1)), Some(json!(2))]);
    assert!(
        tie.iter()
            .all(|v| v.as_ref().unwrap().result == ResultClass::DivergesFromPeers)
    );
    let alone = judge_peers(&[Some(json!(1))]);
    assert_eq!(alone[0].as_ref().unwrap().result, ResultClass::Unsupported);
}
