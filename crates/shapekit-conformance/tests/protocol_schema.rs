//! schema/testee-protocol.schema.json against real traffic: every request the runner builds
//! for the committed corpus, and the rust testee's answers to a request of every op.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use shapekit_conformance::versions::VersionsFile;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn validator() -> jsonschema::Validator {
    let text = std::fs::read_to_string(root().join("schema/testee-protocol.schema.json")).unwrap();
    jsonschema::validator_for(&serde_json::from_str(&text).unwrap()).unwrap()
}

fn assert_valid(v: &jsonschema::Validator, instance: &Value) {
    let errors: Vec<String> = v.iter_errors(instance).map(|e| e.to_string()).collect();
    assert!(
        errors.is_empty(),
        "{instance} breaks the schema: {errors:?}"
    );
}

#[test]
fn rust_testee_answers_follow_the_schema() {
    let file = VersionsFile::load(&root().join("conformance/versions.toml")).unwrap();
    let target = file.target("rust", "30.0.0").unwrap();
    shapekit_conformance::versions::build(&root(), &target).unwrap();
    let v = validator();

    let usd = "AAAAAVVTRAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
    let requests = vec![
        json!({"id": "1", "op": "hello"}),
        json!({"id": "2", "op": "decode_encode", "type": "ScVal", "xdr": "AAAAAwAAAAc=", "depth_limit": 10, "len_limit": 8}),
        json!({"id": "3", "op": "decode_json", "type": "ScVal", "xdr": "AAAAAwAAAAc=", "depth_limit": 10, "len_limit": 8}),
        json!({"id": "4", "op": "encode_json", "type": "ScVal", "json": {"u32": 7}}),
        json!({"id": "5", "op": "strkey_decode", "text": "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF"}),
        json!({"id": "6", "op": "strkey_encode", "kind": "contract", "payload": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}),
        json!({"id": "7", "op": "asset_order", "a": "AAAAAA==", "b": usd}),
        json!({"id": "8", "op": "pool_id", "a": "AAAAAA==", "b": usd, "fee": 30}),
        json!({"id": "9", "op": "map_sort", "entries": [{"key": "AAAAAwAAAAc=", "val": "AAAAAQ=="}]}),
        json!({"id": "10", "op": "map_validate", "xdr": "AAAAEQAAAAEAAAABAAAAAwAAAAcAAAAB"}),
        json!({"id": "11", "op": "id_parse", "kind": "offer", "input": "16"}),
        json!({"id": "12", "op": "id_parse", "kind": "muxed", "input": 16}),
        json!({"id": "13", "op": "to_native", "xdr": "AAAAAwAAAAc="}),
        json!({"id": "14", "op": "decode_encode", "type": "ScVal", "xdr": "AAAAAwAAAAcAAA==", "depth_limit": 10, "len_limit": 10}),
    ];
    let mut child = Command::new(root().join(&target.command[0]))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut statuses = Vec::new();
    for r in &requests {
        assert_valid(&v, r);
        writeln!(stdin, "{r}").unwrap();
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let answer: Value = serde_json::from_str(&line).unwrap();
        assert_valid(&v, &answer);
        assert_eq!(answer["id"], r["id"]);
        statuses.push(answer["status"].as_str().unwrap().to_string());
    }
    // a line that is not a request still gets a schema-valid answer
    writeln!(stdin, "not json").unwrap();
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    assert_valid(&v, &serde_json::from_str(&line).unwrap());
    drop(stdin);
    assert!(child.wait().unwrap().success());
    for s in ["ok", "error", "unsupported"] {
        assert!(statuses.iter().any(|x| x == s), "no {s} answer exercised");
    }
}

#[test]
fn runner_requests_for_the_corpus_follow_the_schema() {
    let manifest = root().join("corpus/manifest.json");
    let (m, corpus) = shapekit_conformance::runner::load_manifest(&manifest).unwrap();
    let v = validator();
    let mut n = 0;
    for e in &m.entries {
        for c in shapekit_conformance::plan::checks(e, &corpus).unwrap() {
            if matches!(c.expect, shapekit_conformance::plan::Expect::Unmappable(_)) {
                panic!("{} {}: the runner cannot map this case", e.id, c.name);
            }
            let mut r = c.request.clone();
            r.insert("id".into(), json!("1"));
            assert_valid(&v, &Value::Object(r));
            n += 1;
        }
    }
    assert!(n > 0);
}
