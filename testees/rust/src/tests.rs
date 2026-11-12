use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};
use stellar_xdr::{Limits, ReadXdr, ScVal};

use crate::handle_line;

fn call(request: Value) -> Value {
    handle_line(&request.to_string())
}

fn ok(request: Value) -> Value {
    let response = call(request.clone());
    assert_eq!(response["status"], "ok", "{request} gave {response}");
    response["result"].clone()
}

fn error_class(request: Value) -> String {
    let response = call(request.clone());
    assert_eq!(response["status"], "error", "{request} gave {response}");
    response["error"]["class"].as_str().unwrap().to_string()
}

#[test]
fn hello_reports_the_linked_versions() {
    let r = ok(json!({"id": "0", "op": "hello"}));
    assert_eq!(r["protocol"], 1);
    assert_eq!(r["sdk"], "stellar-xdr");
    assert_eq!(r["sdk_version"], stellar_xdr::VERSION.pkg);
    assert_eq!(r["components"]["stellar-strkey"], "1.0.0");
    assert!(!r["ops"].as_array().unwrap().contains(&json!("to_native")));
}

#[test]
fn malformed_requests_are_bad_requests() {
    let r = handle_line("not json");
    assert_eq!(r["id"], "");
    assert_eq!(r["error"]["class"], "bad-request");
    assert_eq!(error_class(json!({"id": "1", "op": "nope"})), "bad-request");
    assert_eq!(
        error_class(json!({"id": "1", "op": "decode_encode", "type": "ScVal"})),
        "bad-request"
    );
}

// scval u32 7
const U32_7: &str = "AAAAAwAAAAc=";

#[test]
fn decode_encode_round_trips_and_rejects_trailing_bytes() {
    let req = |xdr: &str| {
        json!({"id": "2", "op": "decode_encode", "type": "ScVal", "xdr": xdr,
               "depth_limit": 1000, "len_limit": 64})
    };
    assert_eq!(ok(req(U32_7))["xdr"], U32_7);
    // two extra zero bytes after the value
    assert_eq!(error_class(req("AAAAAwAAAAcAAA==")), "rejected");
}

#[test]
fn decode_honours_the_depth_limit() {
    // scvVec of one scvVec of one u32: three levels of nesting
    let nested = "AAAAEAAAAAEAAAABAAAAEAAAAAEAAAABAAAAAwAAAAc=";
    let req = |depth: u32| {
        json!({"id": "3", "op": "decode_encode", "type": "ScVal", "xdr": nested,
               "depth_limit": depth, "len_limit": 1024})
    };
    assert_eq!(ok(req(100))["xdr"], nested);
    assert_eq!(error_class(req(2)), "rejected");
}

#[test]
fn depth_limit_counts_like_a_direct_decode() {
    let nested = B64
        .decode("AAAAEAAAAAEAAAABAAAAEAAAAAEAAAABAAAAAwAAAAc=")
        .unwrap();
    let direct = |depth| {
        ScVal::from_xdr(
            &nested,
            Limits {
                depth,
                len: nested.len(),
            },
        )
        .is_ok()
    };
    let needed = (1..32).find(|d| direct(*d)).expect("fits in 32 levels");
    let req = |depth: u32| {
        json!({"id": "4", "op": "decode_encode", "type": "ScVal", "xdr": B64.encode(&nested),
               "depth_limit": depth, "len_limit": 1024})
    };
    assert_eq!(ok(req(needed))["xdr"], B64.encode(&nested));
    assert_eq!(error_class(req(needed - 1)), "rejected");
}

#[test]
fn unknown_types_are_unsupported() {
    let r = call(
        json!({"id": "4", "op": "decode_json", "type": "NoSuchType", "xdr": "",
                        "depth_limit": 10, "len_limit": 10}),
    );
    assert_eq!(r["status"], "unsupported");
}

#[test]
fn decode_json_and_encode_json_agree() {
    let json_value = ok(
        json!({"id": "5", "op": "decode_json", "type": "ScVal", "xdr": U32_7,
                               "depth_limit": 10, "len_limit": 64}),
    )["json"]
        .clone();
    assert_eq!(json_value, json!({"u32": 7}));
    let back = ok(json!({"id": "6", "op": "encode_json", "type": "ScVal", "json": json_value}));
    assert_eq!(back["xdr"], U32_7);
}

#[test]
fn ghsa_3xhm_p452_9wjh_input() {
    let r = call(
        json!({"id": "7", "op": "encode_json", "type": "ScSpecFunctionV0",
                        "json": {"doc": "", "name": "f", "inputs": [], "outputs": ["u32", "u32"]}}),
    );
    // 28.0.0 accepted the second output and wrote bytes no decoder reads; 28.0.1 refuses it
    if stellar_xdr::VERSION.pkg == "28.0.0" {
        assert_eq!(r["status"], "ok", "{r}");
        assert_eq!(
            r["result"]["xdr"],
            "AAAAAAAAAAFmAAAAAAAAAAAAAAIAAAAEAAAABA=="
        );
    } else {
        assert_eq!(r["error"]["class"], "rejected", "{r}");
    }
}

const ZERO_ACCOUNT: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

#[test]
fn strkey_decode_and_encode() {
    let r = ok(json!({"id": "8", "op": "strkey_decode", "text": ZERO_ACCOUNT}));
    assert_eq!(r["kind"], "account");
    assert_eq!(r["payload"], "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=");
    let t = ok(json!({"id": "9", "op": "strkey_encode", "kind": "account",
                      "payload": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}));
    assert_eq!(t["text"], ZERO_ACCOUNT);
    assert_eq!(
        error_class(json!({"id": "10", "op": "strkey_decode", "text": "GAAA"})),
        "rejected"
    );
    // a pre-auth tx payload one byte short cannot reach the crate's fixed-size type
    assert_eq!(
        error_class(
            json!({"id": "11", "op": "strkey_encode", "kind": "pre_auth_tx",
                           "payload": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="})
        ),
        "rejected"
    );
}

#[test]
fn every_strkey_kind_round_trips() {
    let key = [7u8; 32];
    let mut muxed = key.to_vec();
    muxed.extend_from_slice(&5u64.to_be_bytes());
    let mut signed = key.to_vec();
    signed.extend_from_slice(&[0, 0, 0, 5, 1, 2, 3, 4, 5, 0, 0, 0]);
    let mut balance = vec![0u8];
    balance.extend_from_slice(&key);
    let cases: Vec<(&str, Vec<u8>, char)> = vec![
        ("account", key.to_vec(), 'G'),
        ("seed", key.to_vec(), 'S'),
        ("muxed_account", muxed.clone(), 'M'),
        ("signed_payload", signed, 'P'),
        ("pre_auth_tx", key.to_vec(), 'T'),
        ("hash_x", key.to_vec(), 'X'),
        ("contract", key.to_vec(), 'C'),
        ("liquidity_pool", key.to_vec(), 'L'),
        ("claimable_balance", balance, 'B'),
        ("muxed_contract", muxed, 'W'),
    ];
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD;
    for (kind, payload, prefix) in cases {
        let p = b64.encode(&payload);
        let text =
            ok(json!({"id": "12", "op": "strkey_encode", "kind": kind, "payload": p}))["text"]
                .as_str()
                .unwrap()
                .to_string();
        assert!(text.starts_with(prefix), "{kind}: {text}");
        let back = ok(json!({"id": "13", "op": "strkey_decode", "text": text}));
        assert_eq!(back["kind"], kind);
        assert_eq!(back["payload"], p, "{kind}");
    }
}

// Asset xdr for USD with a 32-byte issuer of `fill`
fn usd(fill: u8) -> String {
    use base64::Engine as _;
    let mut bytes = vec![0, 0, 0, 1, b'U', b'S', b'D', 0, 0, 0, 0, 0];
    bytes.extend_from_slice(&[fill; 32]);
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[test]
fn asset_order_uses_raw_issuer_bytes() {
    // the js-stellar-sdk#1766 pair: zero bytes sort first although its strkey sorts last
    let r = ok(json!({"id": "14", "op": "asset_order", "a": usd(0), "b": usd(0x3a)}));
    assert_eq!(r["order"], -1);
    let r = ok(json!({"id": "15", "op": "asset_order", "a": usd(0x3a), "b": usd(0)}));
    assert_eq!(r["order"], 1);
    let r = ok(json!({"id": "16", "op": "asset_order", "a": usd(1), "b": usd(1)}));
    assert_eq!(r["order"], 0);
}

#[test]
fn pool_id_matches_core_and_needs_order() {
    // the hash input written out by hand from the cap-38 definition: params arm 0, native,
    // USD with issuer bytes 9, fee 30
    use sha2::{Digest, Sha256};
    let mut params = vec![0, 0, 0, 0];
    params.extend_from_slice(&[0, 0, 0, 0]);
    params.extend_from_slice(&[0, 0, 0, 1, b'U', b'S', b'D', 0, 0, 0, 0, 0]);
    params.extend_from_slice(&[9; 32]);
    params.extend_from_slice(&30i32.to_be_bytes());
    let expected = hex::encode(Sha256::digest(&params));
    let native = "AAAAAA==";
    let r = ok(json!({"id": "17", "op": "pool_id", "a": native, "b": usd(9), "fee": 30}));
    assert_eq!(r["pool_id"], expected);
    assert_eq!(
        error_class(json!({"id": "18", "op": "pool_id", "a": usd(9), "b": native, "fee": 30})),
        "rejected"
    );
}

#[test]
fn map_sort_and_validate() {
    use base64::Engine as _;
    use stellar_xdr::{Limits, ScVal, ScVec, WriteXdr};
    let b64 = base64::engine::general_purpose::STANDARD;
    let enc = |v: ScVal| b64.encode(v.to_xdr(Limits::none()).unwrap());
    let sym = |s: &str| ScVal::Symbol(s.try_into().unwrap());
    let entries: Vec<Value> = ["b", "aa", "a"]
        .iter()
        .map(|s| json!({"key": enc(sym(s)), "val": enc(ScVal::Void)}))
        .collect();
    let sorted = ok(json!({"id": "19", "op": "map_sort", "entries": entries}))["xdr"].clone();
    let valid = ok(json!({"id": "20", "op": "map_validate", "xdr": sorted}));
    assert_eq!(valid["valid"], true);
    let bytes = b64.decode(sorted.as_str().unwrap()).unwrap();
    let ScVal::Map(Some(map)) =
        <ScVal as stellar_xdr::ReadXdr>::from_xdr(bytes, Limits::none()).unwrap()
    else {
        panic!("not a map")
    };
    let keys: Vec<ScVal> = map.iter().map(|e| e.key.clone()).collect();
    assert_eq!(keys, vec![sym("a"), sym("aa"), sym("b")]);
    let unsorted = stellar_xdr::ScMap(
        vec![
            stellar_xdr::ScMapEntry {
                key: sym("b"),
                val: ScVal::Void,
            },
            stellar_xdr::ScMapEntry {
                key: sym("a"),
                val: ScVal::Void,
            },
        ]
        .try_into()
        .unwrap(),
    );
    let r = ok(json!({"id": "21", "op": "map_validate", "xdr": enc(ScVal::Map(Some(unsorted)))}));
    assert_eq!(r["valid"], false);
    let dup = vec![json!({"key": enc(sym("a")), "val": enc(ScVal::Void)}); 2];
    assert_eq!(
        error_class(json!({"id": "22", "op": "map_sort", "entries": dup})),
        "rejected"
    );
    let vec_val = enc(ScVal::Vec(Some(ScVec(vec![].try_into().unwrap()))));
    assert_eq!(
        error_class(json!({"id": "23", "op": "map_validate", "xdr": vec_val})),
        "bad-request"
    );
}

#[test]
fn id_parse_reads_offer_ids_through_xdr_json() {
    let parse = |input: &str| {
        handle_line(&format!(
            r#"{{"id":"24","op":"id_parse","kind":"offer","input":{input}}}"#
        ))
    };
    assert_eq!(parse("16")["result"]["id"], "16");
    assert_eq!(parse("\"16\"")["result"]["id"], "16");
    // a number above 2^53 must arrive exactly, not through a float
    assert_eq!(
        parse("9007199254740993")["result"]["id"],
        "9007199254740993"
    );
    assert_eq!(parse("-1")["result"]["id"], "-1");
    assert_eq!(parse("\"0x10\"")["error"]["class"], "rejected");
    assert_eq!(parse("\"\"")["error"]["class"], "rejected");
    assert_eq!(parse("9223372036854775808")["error"]["class"], "rejected");
    let muxed = call(json!({"id": "25", "op": "id_parse", "kind": "muxed", "input": "16"}));
    assert_eq!(muxed["status"], "unsupported");
}

#[test]
fn to_native_is_unsupported() {
    let r = call(json!({"id": "26", "op": "to_native", "xdr": U32_7}));
    assert_eq!(r["status"], "unsupported");
}
