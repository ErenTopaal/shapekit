use serde_json::{Value, json};

use crate::handle_line;
use crate::ops::{ErrorClass, classify_panic};

fn ask(request: Value) -> Value {
    handle_line(&request.to_string())
}

#[test]
fn hello_reports_the_linked_versions() {
    let r = ask(json!({"id": "1", "op": "hello"}));
    assert_eq!(r["status"], "ok");
    assert_eq!(r["result"]["sdk"], "soroban-sdk");
    assert_eq!(r["result"]["sdk_version"], crate::ops::SDK_VERSION);
    assert_eq!(r["result"]["decode_limits"], "ignored");
}

#[test]
fn scval_round_trips_through_the_host() {
    // ScVal u32 7
    let r = ask(
        json!({"id": "2", "op": "decode_encode", "type": "ScVal", "xdr": "AAAAAwAAAAc=",
        "depth_limit": 1000, "len_limit": 8}),
    );
    assert_eq!(r["result"]["xdr"], "AAAAAwAAAAc=", "{r}");
}

#[test]
fn other_types_are_unsupported() {
    let r = ask(
        json!({"id": "3", "op": "decode_encode", "type": "TransactionEnvelope",
        "xdr": "AAAA", "depth_limit": 1000, "len_limit": 3}),
    );
    assert_eq!(r["status"], "unsupported");
}

#[test]
fn truncated_scval_is_rejected() {
    let r = ask(
        json!({"id": "4", "op": "decode_encode", "type": "ScVal", "xdr": "AAAAAwAA",
        "depth_limit": 1000, "len_limit": 6}),
    );
    assert_eq!(r["error"]["class"], "rejected", "{r}");
}

#[test]
fn map_sort_orders_by_the_host() {
    // keys u32 2 then u32 1, values void
    let r = ask(json!({"id": "5", "op": "map_sort", "entries": [
        {"key": "AAAAAwAAAAI=", "val": "AAAAAQ=="},
        {"key": "AAAAAwAAAAE=", "val": "AAAAAQ=="}]}));
    assert_eq!(
        r["result"]["xdr"], "AAAAEQAAAAEAAAACAAAAAwAAAAEAAAABAAAAAwAAAAIAAAAB",
        "{r}"
    );
}

#[test]
fn account_strkey_round_trips() {
    let g = "GD7777777777777777777777777777777777777777777777777773DB";
    let r = ask(json!({"id": "6", "op": "strkey_decode", "text": g}));
    assert_eq!(r["result"]["kind"], "account", "{r}");
    let back = ask(json!({"id": "7", "op": "strkey_encode", "kind": "account",
        "payload": r["result"]["payload"]}));
    assert_eq!(back["result"]["text"], g, "{back}");
}

#[test]
fn host_errors_are_rejections_and_other_panics_are_not() {
    assert_eq!(
        classify_panic("HostError: Error(Value, InvalidInput)"),
        ErrorClass::Rejected
    );
    assert_eq!(classify_panic("incorrect scval type"), ErrorClass::Panic);
}
