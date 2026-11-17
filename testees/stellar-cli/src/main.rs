//! The stellar-cli testee for the shapekit conformance runner: answers testee protocol
//! requests (docs/testee-protocol.md) by running the `stellar` binary of a stellar-cli release,
//! `stellar xdr decode|encode`, `stellar strkey decode|encode` and `stellar tx hash`, once per
//! request. The binary's path is the first argument.

mod coverage;

use std::io::{BufRead, Write};
use std::process::{Command, ExitCode, Stdio};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Map, Value, json};

const OPS: &[&str] = &[
    "decode_encode",
    "decode_json",
    "encode_json",
    "strkey_decode",
    "strkey_encode",
    "tx_hash",
];

enum Outcome {
    Ok(Value),
    Error(&'static str, String),
    Unsupported(String),
}

fn bad(m: impl Into<String>) -> Outcome {
    Outcome::Error("bad-request", m.into())
}

fn main() -> ExitCode {
    let Some(cli) = std::env::args().nth(1) else {
        eprintln!("usage: shapekit-testee-stellar-cli <path to the stellar binary>");
        return ExitCode::FAILURE;
    };
    // deep XDR-JSON is parsed and written recursively, so requests run on a thread whose stack
    // is far larger than the default main thread's
    let worker = std::thread::Builder::new()
        .stack_size(512 << 20)
        .spawn(move || serve(&cli))
        .expect("starting the request thread");
    let code = worker.join().unwrap_or(ExitCode::FAILURE);
    if let Some(d) = coverage::dir() {
        let _ = std::fs::remove_dir_all(d);
    }
    code
}

fn parse(text: &str) -> serde_json::Result<Value> {
    let mut de = serde_json::Deserializer::from_str(text);
    de.disable_recursion_limit();
    let v = serde::Deserialize::deserialize(&mut de)?;
    de.end()?;
    Ok(v)
}

fn serve(cli: &str) -> ExitCode {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            return ExitCode::FAILURE;
        };
        if line.is_empty() {
            continue;
        }
        let resp = handle(cli, &line);
        if writeln!(out, "{resp}").and_then(|()| out.flush()).is_err() {
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

fn handle(cli: &str, line: &str) -> Value {
    let req: Map<String, Value> = match parse(line) {
        Ok(Value::Object(o)) => o,
        _ => return error("", "bad-request", "request is not a json object"),
    };
    let Some(id) = req.get("id").and_then(Value::as_str).map(str::to_string) else {
        return error("", "bad-request", "request has no string id");
    };
    let op = req.get("op").and_then(Value::as_str).unwrap_or("");
    let outcome = match op {
        "hello" => hello(cli),
        "decode_encode" => decode_encode(cli, &req),
        "decode_json" => decode_json(cli, &req),
        "encode_json" => encode_json(cli, &req),
        "strkey_decode" => strkey_decode(cli, &req),
        "strkey_encode" => strkey_encode(cli, &req),
        "tx_hash" => tx_hash(cli, &req),
        "coverage_snapshot" => match coverage::dir() {
            Some(d) => match coverage::snapshot(d, cli) {
                Ok(v) => Outcome::Ok(v),
                Err(e) => Outcome::Error("internal", e),
            },
            None => Outcome::Unsupported(
                "coverage is collected only for a coverage build (SHAPEKIT_COVERAGE=1 and a stellar \
                 binary built with -C instrument-coverage)"
                    .into(),
            ),
        },
        "coverage_new" => Outcome::Unsupported(
            "the counters are in the profile files the stellar binary writes when each run \
             exits, readable only by merging them with llvm-profdata, too slow to ask after \
             every input"
                .into(),
        ),
        "" => bad("request has no op"),
        other if is_known_op(other) => Outcome::Unsupported(format!(
            "stellar-cli has no command for {other}: its xdr commands are decode, encode, \
             guess, compare and generate, its strkey commands decode and encode"
        )),
        other => bad(format!("unknown op {other}")),
    };
    match outcome {
        Outcome::Ok(v) => json!({"id": id, "status": "ok", "result": v}),
        Outcome::Error(class, m) => error(&id, class, &m),
        Outcome::Unsupported(r) => json!({"id": id, "status": "unsupported", "reason": r}),
    }
}

fn is_known_op(op: &str) -> bool {
    matches!(
        op,
        "asset_order"
            | "pool_id"
            | "map_sort"
            | "map_validate"
            | "id_parse"
            | "to_native"
            | "address_from_scval"
            | "balance_id"
            | "contract_id"
    )
}

fn error(id: &str, class: &str, message: &str) -> Value {
    json!({"id": id, "status": "error", "error": {"class": class, "message": message}})
}

/// Runs the binary with `args`, `input` on stdin. A non-zero exit with the cli's own error line
/// is a refusal; a rust panic message or a death by signal is a crash.
fn run(cli: &str, args: &[&str], input: &[u8]) -> Result<String, Outcome> {
    let mut child = Command::new(cli)
        .args(args)
        // no config or cache of this machine's stellar-cli setup is read or written
        .env("STELLAR_CONFIG_HOME", std::env::temp_dir())
        .env("XDG_CONFIG_HOME", std::env::temp_dir())
        .env("STELLAR_NO_CACHE", "true")
        // a coverage build's binary adds its counters to this process's profile files
        .env(
            "LLVM_PROFILE_FILE",
            coverage::dir().map_or_else(|| "/dev/null".into(), coverage::profile_file),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Outcome::Error("internal", format!("starting {cli}: {e}")))?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let input = input.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let output = child
        .wait_with_output()
        .map_err(|e| Outcome::Error("internal", format!("running {cli}: {e}")))?;
    let _ = writer.join();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        return String::from_utf8(output.stdout)
            .map_err(|e| Outcome::Error("internal", format!("the cli wrote non-utf-8: {e}")));
    }
    if output.status.code().is_none() || stderr.contains("panicked at") {
        return Err(Outcome::Error(
            "panic",
            format!("{}: {stderr}", output.status),
        ));
    }
    Err(Outcome::Error("rejected", stderr))
}

fn hello(cli: &str) -> Outcome {
    // the first line of `stellar version` is `stellar <version> (<commit>)`, the second the
    // stellar-xdr crate it links
    let text = match run(cli, &["version"], b"") {
        Ok(t) => t,
        Err(o) => return o,
    };
    let mut lines = text.lines();
    let version = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap_or("")
        .to_string();
    let xdr = lines
        .next()
        .and_then(|l| l.strip_prefix("stellar-xdr "))
        .and_then(|l| l.split_whitespace().next())
        .unwrap_or("")
        .to_string();
    let mut ops = OPS.to_vec();
    if coverage::dir().is_some() {
        ops.push("coverage_snapshot");
    }
    Outcome::Ok(json!({
        "protocol": 2,
        "sdk": "stellar-cli",
        "sdk_version": version,
        "components": {"stellar-xdr": xdr},
        "ops": ops,
        // the xdr commands take no limits
        "decode_limits": "ignored",
    }))
}

fn string<'a>(req: &'a Map<String, Value>, name: &str) -> Result<&'a str, Outcome> {
    req.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| bad(format!("{name} must be a string")))
}

fn decode_to_json(cli: &str, req: &Map<String, Value>) -> Result<String, Outcome> {
    let ty = string(req, "type")?;
    let xdr = string(req, "xdr")?;
    run(
        cli,
        &[
            "xdr",
            "decode",
            "--type",
            ty,
            "--input",
            "single-base64",
            "--output",
            "json",
        ],
        xdr.as_bytes(),
    )
    .map_err(|o| unknown_type(o, ty))
}

/// The cli refuses a type it does not know; that is a missing type, not a refusal of the value.
fn unknown_type(o: Outcome, ty: &str) -> Outcome {
    match o {
        Outcome::Error("rejected", m) if m.contains(&format!("unknown type {ty},")) => {
            Outcome::Unsupported(format!("stellar-cli has no xdr type {ty}"))
        }
        o => o,
    }
}

fn encode_from_json(cli: &str, ty: &str, json_text: &str) -> Result<String, Outcome> {
    run(
        cli,
        &[
            "xdr",
            "encode",
            "--type",
            ty,
            "--input",
            "json",
            "--output",
            "single-base64",
        ],
        json_text.as_bytes(),
    )
    .map(|s| s.trim().to_string())
    .map_err(|o| unknown_type(o, ty))
}

/// The cli writes no bytes back from bytes; its round trip is decode to json, then encode.
fn decode_encode(cli: &str, req: &Map<String, Value>) -> Outcome {
    let json_text = match decode_to_json(cli, req) {
        Ok(t) => t,
        Err(o) => return o,
    };
    let ty = req.get("type").and_then(Value::as_str).unwrap_or("");
    match encode_from_json(cli, ty, json_text.trim()) {
        Ok(x) => Outcome::Ok(json!({"xdr": x})),
        Err(o) => o,
    }
}

fn decode_json(cli: &str, req: &Map<String, Value>) -> Outcome {
    match decode_to_json(cli, req) {
        Ok(t) => match parse(&t) {
            Ok(v) => Outcome::Ok(json!({"json": v})),
            Err(e) => Outcome::Error(
                "internal",
                format!("the cli wrote json that does not parse: {e}"),
            ),
        },
        Err(o) => o,
    }
}

fn encode_json(cli: &str, req: &Map<String, Value>) -> Outcome {
    let ty = match string(req, "type") {
        Ok(t) => t,
        Err(o) => return o,
    };
    let Some(json) = req.get("json") else {
        return bad("encode_json needs member json");
    };
    match encode_from_json(cli, ty, &json.to_string()) {
        Ok(x) => Outcome::Ok(json!({"xdr": x})),
        Err(o) => o,
    }
}

fn hex_bytes(v: Option<&Value>) -> Option<Vec<u8>> {
    hex::decode(v?.as_str()?).ok()
}

fn strkey_decode(cli: &str, req: &Map<String, Value>) -> Outcome {
    let text: Vec<u8> = match (req.get("text"), req.get("text_base64")) {
        (Some(Value::String(t)), None) => t.clone().into_bytes(),
        (None, Some(Value::String(b))) => match B64.decode(b) {
            Ok(b) => b,
            Err(e) => return bad(format!("text_base64 is not base64: {e}")),
        },
        _ => return bad("strkey_decode needs exactly one of text and text_base64"),
    };
    // the strkey goes on stdin, so its bytes reach the cli as they are
    let out = match run(cli, &["strkey", "decode"], &text) {
        Ok(o) => o,
        Err(o) => return o,
    };
    let v: Value = match parse(out.trim()) {
        Ok(v) => v,
        Err(e) => {
            return Outcome::Error(
                "internal",
                format!("the cli wrote json that does not parse: {e}"),
            );
        }
    };
    let Some((k, x)) = v.as_object().and_then(|o| o.iter().next()) else {
        return Outcome::Error("internal", format!("unexpected strkey json {v}"));
    };
    let payload = |b: Option<Vec<u8>>| {
        b.ok_or_else(|| Outcome::Error("internal", format!("unexpected strkey json {v}")))
    };
    let result = match k.as_str() {
        "public_key_ed25519" => payload(hex_bytes(Some(x))).map(|p| ("account", p)),
        "private_key_ed25519" => payload(hex_bytes(Some(x))).map(|p| ("seed", p)),
        "pre_auth_tx" => payload(hex_bytes(Some(x))).map(|p| ("pre_auth_tx", p)),
        "hash_x" => payload(hex_bytes(Some(x))).map(|p| ("hash_x", p)),
        "contract" => payload(hex_bytes(Some(x))).map(|p| ("contract", p)),
        "liquidity_pool" => payload(hex_bytes(Some(x))).map(|p| ("liquidity_pool", p)),
        "claimable_balance" => payload(hex_bytes(x.get("v0"))).map(|p| {
            let mut out = vec![0u8];
            out.extend(p);
            ("claimable_balance", out)
        }),
        "muxed_account_ed25519" => payload(hex_bytes(x.get("ed25519"))).and_then(|mut p| {
            let id = x
                .get("id")
                .and_then(Value::as_u64)
                .ok_or_else(|| Outcome::Error("internal", "no id".into()))?;
            p.extend(id.to_be_bytes());
            Ok(("muxed_account", p))
        }),
        "muxed_contract" => payload(hex_bytes(x.get("contract_id"))).and_then(|mut p| {
            let id = x
                .get("id")
                .and_then(Value::as_u64)
                .ok_or_else(|| Outcome::Error("internal", "no id".into()))?;
            p.extend(id.to_be_bytes());
            Ok(("muxed_contract", p))
        }),
        "signed_payload_ed25519" => payload(hex_bytes(x.get("ed25519"))).and_then(|mut p| {
            let body = hex_bytes(x.get("payload"))
                .ok_or_else(|| Outcome::Error("internal", "no payload".into()))?;
            p.extend((body.len() as u32).to_be_bytes());
            p.extend(&body);
            p.extend(vec![0u8; (4 - body.len() % 4) % 4]);
            Ok(("signed_payload", p))
        }),
        other => Err(Outcome::Error(
            "internal",
            format!("unexpected strkey kind {other}"),
        )),
    };
    match result {
        Ok((kind, p)) => Outcome::Ok(json!({"kind": kind, "payload": B64.encode(p)})),
        Err(o) => o,
    }
}

/// The cli encodes from its json form of a strkey, so the payload is written into that form
/// field by field, without repair; a payload that does not fit the form is refused.
fn strkey_encode(cli: &str, req: &Map<String, Value>) -> Outcome {
    let kind = match string(req, "kind") {
        Ok(k) => k,
        Err(o) => return o,
    };
    let p = match string(req, "payload").map(|s| B64.decode(s)) {
        Ok(Ok(p)) => p,
        Ok(Err(e)) => return bad(format!("payload is not base64: {e}")),
        Err(o) => return o,
    };
    let refuse = |why: &str| Outcome::Error("rejected", format!("a {kind} payload {why}"));
    let doc = match kind {
        "account" => json!({"public_key_ed25519": hex::encode(&p)}),
        "seed" => json!({"private_key_ed25519": hex::encode(&p)}),
        "pre_auth_tx" => json!({"pre_auth_tx": hex::encode(&p)}),
        "hash_x" => json!({"hash_x": hex::encode(&p)}),
        "contract" => json!({"contract": hex::encode(&p)}),
        "liquidity_pool" => json!({"liquidity_pool": hex::encode(&p)}),
        "claimable_balance" => match p.split_first() {
            Some((0, rest)) => json!({"claimable_balance": {"v0": hex::encode(rest)}}),
            _ => return refuse("does not start with the v0 type byte"),
        },
        "muxed_account" | "muxed_contract" => {
            if p.len() < 8 {
                return refuse("is shorter than an id");
            }
            let (key, id) = p.split_at(p.len() - 8);
            let id = u64::from_be_bytes(id.try_into().expect("8 bytes"));
            if kind == "muxed_account" {
                json!({"muxed_account_ed25519": {"ed25519": hex::encode(key), "id": id}})
            } else {
                json!({"muxed_contract": {"contract_id": hex::encode(key), "id": id}})
            }
        }
        "signed_payload" => {
            if p.len() < 36 {
                return refuse("has no key and length");
            }
            let n = u32::from_be_bytes(p[32..36].try_into().expect("4 bytes")) as usize;
            if 36 + n > p.len() {
                return refuse("length runs past the payload");
            }
            json!({"signed_payload_ed25519": {"ed25519": hex::encode(&p[..32]), "payload": hex::encode(&p[36..36 + n])}})
        }
        other => return bad(format!("unknown strkey kind {other}")),
    };
    match run(cli, &["strkey", "encode"], doc.to_string().as_bytes()) {
        Ok(t) => Outcome::Ok(json!({"text": t.trim()})),
        Err(o) => o,
    }
}

fn tx_hash(cli: &str, req: &Map<String, Value>) -> Outcome {
    if req.get("inner").and_then(Value::as_bool) == Some(true) {
        return Outcome::Unsupported("stellar tx hash hashes the outer transaction only".into());
    }
    let (Ok(xdr), Ok(pass)) = (string(req, "xdr"), string(req, "network_passphrase")) else {
        return bad("tx_hash needs xdr and network_passphrase");
    };
    match run(
        cli,
        &["tx", "hash", "--network-passphrase", pass],
        xdr.as_bytes(),
    ) {
        Ok(h) => Outcome::Ok(json!({"hash": h.trim()})),
        Err(o) => o,
    }
}
