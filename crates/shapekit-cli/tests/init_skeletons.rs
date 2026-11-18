//! `shapekit init`: every skeleton is written, built with its own toolchain and driven with a
//! request of every command, and every line it answers must follow the protocol schema. The
//! adapter skeletons need python3, node and go; the testee skeletons install their sdk from the
//! package registries, so those tests are ignored by default.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn validator(schema: &str) -> jsonschema::Validator {
    let text = std::fs::read_to_string(root().join("schema").join(schema)).unwrap();
    jsonschema::validator_for(&serde_json::from_str(&text).unwrap()).unwrap()
}

fn assert_valid(v: &jsonschema::Validator, instance: &Value) {
    let errors: Vec<String> = v.iter_errors(instance).map(|e| e.to_string()).collect();
    assert!(
        errors.is_empty(),
        "{instance} breaks the schema: {errors:?}"
    );
}

fn shapekit(dir: &Path, args: &[&str]) {
    let out = Command::new(env!("CARGO_BIN_EXE_shapekit"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "shapekit {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn run_in(dir: &Path, argv: &[&str]) {
    let st = Command::new(argv[0])
        .args(&argv[1..])
        .current_dir(dir)
        .env("GOWORK", "off")
        .status()
        .unwrap_or_else(|e| panic!("{argv:?}: {e}"));
    assert!(st.success(), "{argv:?} failed");
}

/// A running skeleton: one request line in, response lines out.
struct Peer {
    child: std::process::Child,
    stdin: Option<std::process::ChildStdin>,
    stdout: BufReader<std::process::ChildStdout>,
}

impl Peer {
    fn start(dir: &Path, argv: &[&str]) -> Peer {
        let mut child = Command::new(argv[0])
            .args(&argv[1..])
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("{argv:?}: {e}"));
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Peer {
            child,
            stdin,
            stdout,
        }
    }

    fn send(&mut self, line: &str) {
        writeln!(self.stdin.as_mut().unwrap(), "{line}").unwrap();
    }

    fn read(&mut self) -> Value {
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("{line:?}: {e}"))
    }

    fn finish(mut self) {
        drop(self.stdin.take());
        assert!(self.child.wait().unwrap().success());
    }
}

fn adapter(lang: &str, build: &[&[&str]], command: &[&str]) {
    let tmp = tempfile::tempdir().unwrap();
    shapekit(
        tmp.path(),
        &[
            "init",
            "adapter",
            "--lang",
            lang,
            "--target",
            "my-indexer",
            "--target-version",
            "0123456789abcdef",
            "--out",
            "a",
        ],
    );
    let dir = tmp.path().join("a");
    assert!(dir.join("README.md").is_file());
    for step in build {
        run_in(&dir, step);
    }
    let v = validator("adapter-protocol.schema.json");
    let start = json!({
        "id": "2", "cmd": "start", "chain": "cap40.signed-payload-signer.p30", "network": "testnet",
        "network_passphrase": "Test SDF Network ; September 2015", "protocol": 30,
        "delivery": {"mode": "rpc", "rpc_url": "http://127.0.0.1:1/", "archive_url": "http://127.0.0.1:1/archive",
            "lake": {"url": "http://127.0.0.1:1/lake", "path": "/nonexistent", "ledgers_per_batch": 1, "batches_per_partition": 64000}},
        "ledgers": {"bootstrap": 2, "last": 10, "checkpoint_frequency": 64},
        "limits": {"memory_bytes": 1u64 << 32}, "workdir": "/nonexistent"
    });
    let mut s3 = start.clone();
    s3["id"] = json!("3");
    s3["delivery"]["mode"] = json!("sep54-s3");
    // the request, and the status and error class of the final answer it must get
    let cases = [
        (
            json!({"id": "0", "cmd": "describe", "protocol": 2}),
            "ok",
            None,
        ),
        (
            json!({"id": "1", "cmd": "describe", "protocol": 1}),
            "error",
            Some("unsupported"),
        ),
        (start, "error", Some("target")),
        (s3, "error", Some("unsupported")),
        (json!({"id": "4", "cmd": "advance", "head": 3}), "ok", None),
        (json!({"id": "5", "cmd": "status"}), "error", Some("target")),
        (
            json!({"id": "6", "cmd": "export", "from": 3, "to": 10}),
            "error",
            Some("target"),
        ),
        (
            json!({"id": "7", "cmd": "dump"}),
            "error",
            Some("bad-request"),
        ),
        (json!({"id": "8", "cmd": "stop"}), "error", Some("target")),
    ];
    let mut peer = Peer::start(&dir, command);
    for (req, status, class) in &cases {
        assert_valid(&v, req);
        peer.send(&req.to_string());
        let answer = peer.read();
        assert_valid(&v, &answer);
        assert_eq!(answer["id"], req["id"], "{lang}: {answer}");
        assert_eq!(answer["status"], *status, "{lang} {req}: {answer}");
        if let Some(c) = class {
            assert_eq!(answer["error"]["class"], *c, "{lang} {req}: {answer}");
        }
        if req["id"] == "0" {
            let r = &answer["result"];
            assert_eq!(r["target"], "my-indexer");
            assert_eq!(r["target_version"], "0123456789abcdef");
            assert_eq!(r["protocol"], 2);
            assert_eq!(r["vocab_version"], shapekit_oracle::ORACLE_VOCAB_VERSION);
        }
        if req["id"] == "4" {
            assert_eq!(answer["result"]["head"], 3);
        }
    }
    for line in ["not json", "[1]", "{\"cmd\": \"status\"}"] {
        peer.send(line);
        let answer = peer.read();
        assert_valid(&v, &answer);
        assert_eq!(answer["id"], "", "{lang}: {answer}");
        assert_eq!(answer["error"]["class"], "bad-request");
    }
    peer.finish();
}

#[test]
fn python_adapter_skeleton() {
    adapter("python", &[], &["python3", "adapter.py"]);
}

#[test]
fn ts_adapter_skeleton() {
    adapter("ts", &[], &["node", "adapter.ts"]);
}

#[test]
fn go_adapter_skeleton() {
    adapter(
        "go",
        &[
            &["go", "vet", "."],
            &["go", "build", "-o", "my-indexer-adapter", "."],
        ],
        &["./my-indexer-adapter"],
    );
}

// the readme's build line runs from the root of a shapekit checkout, inside a go workspace that
// does not list the skeleton's module
#[test]
fn go_adapter_readme_builds_inside_a_go_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("go.work"), "go 1.27.4\n").unwrap();
    shapekit(
        tmp.path(),
        &[
            "init",
            "adapter",
            "--lang",
            "go",
            "--target",
            "my-indexer",
            "--target-version",
            "0123456789abcdef",
            "--out",
            "a",
        ],
    );
    let readme = std::fs::read_to_string(tmp.path().join("a/README.md")).unwrap();
    let build = readme
        .lines()
        .find(|l| l.starts_with("(cd a && "))
        .expect("the readme has a build line");
    let st = Command::new("sh")
        .args(["-c", build])
        .current_dir(tmp.path())
        .env_remove("GOWORK")
        .status()
        .unwrap();
    assert!(st.success(), "{build} failed");
    assert!(tmp.path().join("a/my-indexer-adapter").is_file());
}

#[test]
fn init_refuses_a_directory_that_is_not_empty() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("a")).unwrap();
    std::fs::write(tmp.path().join("a/keep"), "").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_shapekit"))
        .args(["init", "testee", "--lang", "python", "--out", "a"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("is not empty"));
    assert_eq!(std::fs::read_dir(tmp.path().join("a")).unwrap().count(), 1);
}

/// Writes a testee skeleton, runs the build steps of its versions.toml as the runner does, and
/// drives the testee with hello, an op it does not implement, an unknown op and lines that are
/// not requests.
fn testee(lang: &str) {
    let tmp = tempfile::tempdir().unwrap();
    shapekit(
        tmp.path(),
        &["init", "testee", "--lang", lang, "--out", "t"],
    );
    let file =
        shapekit_conformance::versions::VersionsFile::load(&tmp.path().join("t/versions.toml"))
            .unwrap();
    let sdk = file.sdk(lang).unwrap().clone();
    let target = file.target(lang, &sdk.current).unwrap();
    shapekit_conformance::versions::build(tmp.path(), &target).unwrap();
    let v = validator("testee-protocol.schema.json");
    let argv: Vec<&str> = target.command.iter().map(String::as_str).collect();
    let mut peer = Peer::start(tmp.path(), &argv);
    let hello = json!({"id": "1", "op": "hello"});
    assert_valid(&v, &hello);
    peer.send(&hello.to_string());
    let answer = peer.read();
    assert_valid(&v, &answer);
    assert_eq!(answer["status"], "ok", "{answer}");
    assert_eq!(answer["result"]["sdk"], sdk.package.as_str());
    assert_eq!(answer["result"]["sdk_version"], sdk.current.as_str());
    assert_eq!(answer["result"]["ops"], json!([]));
    let req = json!({"id": "2", "op": "decode_encode", "type": "ScVal", "xdr": "AAAAAwAAAAc=", "depth_limit": 10, "len_limit": 8});
    assert_valid(&v, &req);
    peer.send(&req.to_string());
    let answer = peer.read();
    assert_valid(&v, &answer);
    assert_eq!(answer["status"], "unsupported", "{answer}");
    peer.send(r#"{"id": "3", "op": "no_such_op"}"#);
    let answer = peer.read();
    assert_valid(&v, &answer);
    assert_eq!(answer["error"]["class"], "bad-request");
    peer.send("not json");
    let answer = peer.read();
    assert_valid(&v, &answer);
    assert_eq!(answer["id"], "");
    peer.finish();
}

#[test]
#[ignore = "installs the sdk from pypi; run with --include-ignored"]
fn python_testee_skeleton() {
    testee("python");
}

#[test]
#[ignore = "installs the sdk from npm; run with --include-ignored"]
fn ts_testee_skeleton() {
    testee("ts");
}

#[test]
#[ignore = "fetches the sdk from the go module proxy; run with --include-ignored"]
fn go_testee_skeleton() {
    testee("go");
}
