//! Turns manifest entries into testee requests and the answer the network gives for each.
//! The mapping per entry kind is the table in docs/testee-protocol.md.

use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Map, Value, json};
use shapekit_manifest::{
    DecodeExpectation, Entry, EntryKind, VectorFile, decode_depth, read_verified,
};
use stellar_xdr::{Limits, ReadXdr, ScVal};

use crate::testee::parse_deep;

/// What stellar-xdr writes in XDR-JSON for a value it cannot render as a strkey.
pub const INVALID_MARKER: &str = "<INVALID:";

/// What a check's answer is compared with.
#[derive(Debug, Clone, PartialEq)]
pub enum Expect {
    /// `result.xdr` equals these bytes (base64).
    Bytes(String),
    /// The value decodes; no byte comparison because the entry does not require a round trip.
    Accepted,
    /// `result.json` equals this value.
    Json(Value),
    /// Any `result.json` passes; the value is the reference rendering, kept for reports. Used
    /// when the reference rendering is not canonical SEP-51 (see `INVALID_MARKER`).
    AnyJson(Value),
    /// The sdk must refuse the input.
    Rejected,
    Order(i64),
    PoolId(String),
    /// `result.<member>` equals this lowercase hex: a hash or id the network wrote into a
    /// ledger (see `anchors`).
    Hex {
        member: String,
        value: String,
    },
    Valid(bool),
    Id(String),
    /// Either this id or a refusal passes; any other id fails. Used where the network has one
    /// value but an sdk may refuse an input it cannot read exactly (JSON numbers past 2^53-1).
    IdOrRejected(String),
    Strkey {
        kind: String,
        payload: String,
    },
    Text(String),
    /// No map entry may disappear in the native projection: the native map holds this many
    /// entries.
    NoEntryLost {
        entries: usize,
    },
    /// The network leaves the answer to the implementation; the sdks are compared with each
    /// other.
    Peers,
    /// The runner could not turn the corpus case into a request. Always an `error`, never an
    /// sdk finding.
    Unmappable(String),
}

#[derive(Debug, Clone)]
pub struct Check {
    /// `<variant>/<op>` for xdr entries, `case <n>/<op>` for vectors.
    pub name: String,
    pub op: String,
    /// The request members other than `id`.
    pub request: Map<String, Value>,
    pub expect: Expect,
    /// Where the expected answer comes from, for issue bodies.
    pub source: String,
}

/// How sources name the manifest's directory: its own name, `corpus` or `candidate`, the
/// way the repository lays both lanes out.
pub fn root_label(corpus: &Path) -> String {
    corpus
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "corpus".to_string())
}

/// All checks for one entry. `corpus` is the directory holding manifest.json.
pub fn checks(entry: &Entry, corpus: &Path) -> Result<Vec<Check>> {
    match entry.kind {
        EntryKind::Xdr => xdr_checks(entry, corpus),
        _ => vector_checks(entry, corpus),
    }
}

fn xdr_checks(entry: &Entry, corpus: &Path) -> Result<Vec<Check>> {
    let Some(xdr_type) = &entry.xdr_type else {
        bail!("{}: an xdr entry without xdr_type", entry.id);
    };
    let mut out = Vec::new();
    for (variant, v) in entry.variants.iter() {
        // an edited manifest must not point outside the corpus or at stale bytes
        let bytes = read_verified(corpus, &v.xdr_file, Some(&v.xdr_sha256))
            .map_err(|e| anyhow!("{}: {e}", entry.id))?;
        let b64 = B64.encode(&bytes);
        let decode_request = |op: &str| {
            obj(json!({
                "op": op,
                "type": xdr_type,
                "xdr": b64,
                "depth_limit": decode_depth(xdr_type),
                "len_limit": bytes.len(),
            }))
        };
        let file_source = format!("{}/{}", root_label(corpus), v.xdr_file);
        match entry.expected.decode {
            DecodeExpectation::MustAccept => {
                let source = format!(
                    "{file_source}: decode = must-accept, roundtrip = {}",
                    entry.expected.roundtrip
                );
                out.push(Check {
                    name: format!("{variant}/decode_encode"),
                    op: "decode_encode".into(),
                    request: decode_request("decode_encode"),
                    expect: if entry.expected.roundtrip {
                        Expect::Bytes(b64.clone())
                    } else {
                        Expect::Accepted
                    },
                    source: source.clone(),
                });
                if let Some(json_file) = &v.expected_json_file {
                    let sha = v
                        .expected_json_sha256
                        .as_deref()
                        .with_context(|| format!("{}: {json_file} has no sha256", entry.id))?;
                    let text = read_verified_text(corpus, json_file, sha)
                        .map_err(|e| anyhow!("{}: {e}", entry.id))?;
                    let expected = parse_deep(&text)
                        .with_context(|| format!("{}: parsing {json_file}", entry.id))?;
                    let json_source = format!(
                        "{}/{json_file}: SEP-51 rendering by stellar-xdr {}",
                        root_label(corpus),
                        shapekit_manifest::STELLAR_XDR_VERSION.pkg
                    );
                    // stellar-xdr writes "<INVALID:...>" where a value has no strkey form (an
                    // empty signed payload); sep-51 defines no rendering for it, so any rendering
                    // passes and there is no canonical json to encode back from
                    let canonical = !text.contains(INVALID_MARKER);
                    out.push(Check {
                        name: format!("{variant}/decode_json"),
                        op: "decode_json".into(),
                        request: decode_request("decode_json"),
                        expect: if canonical {
                            Expect::Json(expected.clone())
                        } else {
                            Expect::AnyJson(expected.clone())
                        },
                        source: if canonical {
                            json_source.clone()
                        } else {
                            format!(
                                "{json_source}; holds {INVALID_MARKER}, which has no canonical SEP-51 form, so any rendering is accepted"
                            )
                        },
                    });
                    if entry.expected.roundtrip && canonical {
                        out.push(Check {
                            name: format!("{variant}/encode_json"),
                            op: "encode_json".into(),
                            request: obj(json!({
                                "op": "encode_json",
                                "type": xdr_type,
                                "json": expected,
                            })),
                            expect: Expect::Bytes(b64.clone()),
                            source: format!("{json_source}; must encode back to {file_source}"),
                        });
                    }
                }
                if crate::anchors::anchored(entry) {
                    out.extend(anchored_checks(
                        entry,
                        corpus,
                        variant,
                        v,
                        &bytes,
                        &file_source,
                    ));
                }
            }
            DecodeExpectation::MustReject => out.push(Check {
                name: format!("{variant}/decode_encode"),
                op: "decode_encode".into(),
                request: decode_request("decode_encode"),
                expect: Expect::Rejected,
                source: format!(
                    "{file_source}: decode = must-reject (validity {:?})",
                    entry.expected.validity
                ),
            }),
            DecodeExpectation::ImplementationDefined => {
                for op in ["decode_encode", "decode_json"] {
                    out.push(Check {
                        name: format!("{variant}/{op}"),
                        op: op.into(),
                        request: decode_request(op),
                        expect: Expect::Peers,
                        source: format!("{file_source}: decode = implementation-defined"),
                    });
                }
            }
        }
    }
    Ok(out)
}

/// The checks whose answers the ledger anchors (see `anchors`), for a variant of an entry
/// that is the network's own output.
fn anchored_checks(
    entry: &Entry,
    corpus: &Path,
    variant: &str,
    v: &shapekit_manifest::Variant,
    bytes: &[u8],
    file_source: &str,
) -> Vec<Check> {
    let unmappable = |op: &str, why: String| Check {
        name: format!("{variant}/{op}"),
        op: op.into(),
        request: Map::new(),
        expect: Expect::Unmappable(why),
        source: file_source.to_string(),
    };
    let xdr_type = entry.xdr_type.as_deref().unwrap_or_default();
    let anchors_here = xdr_type == "LedgerCloseMeta"
        || (xdr_type == "TransactionEnvelope"
            && entry.source.chain.is_some()
            && entry.source.ledger_seq.is_some());
    if !anchors_here {
        return Vec::new();
    }
    let Some(passphrase) = v.network_passphrase.as_deref() else {
        return vec![unmappable(
            "tx_hash",
            "the variant names no network passphrase".into(),
        )];
    };
    if xdr_type == "LedgerCloseMeta" {
        return match crate::anchors::decode_ledger(bytes) {
            Ok(lcm) => crate::anchors::ledger_checks(variant, passphrase, &lcm, file_source),
            Err(e) => vec![unmappable(
                "tx_hash",
                format!("the ledger does not decode: {e}"),
            )],
        };
    }
    let (Some(chain), Some(seq)) = (&entry.source.chain, entry.source.ledger_seq) else {
        unreachable!("checked above");
    };
    match crate::anchors::decode_envelope(bytes) {
        Ok(env) => crate::anchors::envelope_checks(
            variant,
            passphrase,
            &env,
            crate::anchors::chain_ledger(corpus, chain, variant, seq),
            &format!("{file_source}, applied in ledger {seq} of chain {chain}"),
        ),
        Err(e) => vec![unmappable(
            "tx_hash",
            format!("the envelope does not decode: {e}"),
        )],
    }
}

fn read_verified_text(corpus: &Path, rel: &str, sha256: &str) -> Result<String, String> {
    let bytes = read_verified(corpus, rel, Some(sha256))?;
    String::from_utf8(bytes).map_err(|e| format!("{rel} is not utf-8: {e}"))
}

fn vector_checks(entry: &Entry, corpus: &Path) -> Result<Vec<Check>> {
    let Some(file) = &entry.vector else {
        bail!(
            "{}: a {:?} entry without a vector file",
            entry.id,
            entry.kind
        );
    };
    let text = read_verified_text(corpus, &file.path, &file.sha256)
        .map_err(|e| anyhow!("{}: {e}", entry.id))?;
    let vf: VectorFile = serde_json::from_str(&text)
        .with_context(|| format!("{}: parsing {}", entry.id, file.path))?;
    let mut out = Vec::new();
    for (i, case) in vf.cases.iter().enumerate() {
        let source = format!("{}/{} case {i}", root_label(corpus), file.path);
        out.extend(case_checks(i, case, &source));
    }
    Ok(out)
}

/// The checks of one vector case.
pub fn case_checks(index: usize, case: &Value, source: &str) -> Vec<Check> {
    let op = case.get("op").and_then(Value::as_str).unwrap_or("");
    let name = |op: &str| format!("case {index}/{op}");
    let mapped = match op {
        "asset_order" => asset_order(case),
        "pool_id" => pool_id(case),
        "map_sort" => map_sort(case),
        "map_validate" => map_validate(case),
        "strkey_decode" => return strkey(index, case, source),
        "id_parse" => id_parse(case),
        "to_native" => to_native(case),
        other => Err(format!("unknown case op {other:?}")),
    };
    let op_name = if op.is_empty() { "unknown" } else { op };
    match mapped {
        Ok((request, expect)) => vec![Check {
            name: name(op_name),
            op: op_name.into(),
            request,
            expect,
            source: source.to_string(),
        }],
        Err(why) => vec![unmappable(name(op_name), op_name, why, source)],
    }
}

fn unmappable(name: String, op: &str, why: String, source: &str) -> Check {
    Check {
        name,
        op: op.into(),
        request: Map::new(),
        expect: Expect::Unmappable(why),
        source: source.to_string(),
    }
}

type Mapped = Result<(Map<String, Value>, Expect), String>;

fn obj(v: Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m,
        _ => unreachable!("built from an object literal"),
    }
}

/// The base64 xdr of a case value, written as `{"xdr": ...}` or as a bare string.
fn xdr_of(v: Option<&Value>, what: &str) -> Result<String, String> {
    match v {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(Value::Object(o)) => o
            .get("xdr")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("{what} has no xdr")),
        _ => Err(format!("case has no {what}")),
    }
}

fn is_reject(expected: &Value) -> bool {
    expected == "reject" || expected.get("reject").is_some()
}

fn expected(case: &Value) -> Result<&Value, String> {
    case.get("expected")
        .ok_or_else(|| "case has no expected".to_string())
}

fn asset_order(case: &Value) -> Mapped {
    let order = match expected(case)?.as_str() {
        Some("less") => -1,
        Some("equal") => 0,
        Some("greater") => 1,
        other => return Err(format!("asset_order expected {other:?}")),
    };
    Ok((
        obj(json!({
            "op": "asset_order",
            "a": xdr_of(case.get("a"), "a")?,
            "b": xdr_of(case.get("b"), "b")?,
        })),
        Expect::Order(order),
    ))
}

fn pool_id(case: &Value) -> Mapped {
    let fee = case
        .get("fee")
        .and_then(Value::as_i64)
        .ok_or("pool_id case has no integer fee")?;
    let e = expected(case)?;
    let expect = if is_reject(e) {
        Expect::Rejected
    } else {
        Expect::PoolId(
            e.get("pool_id")
                .and_then(Value::as_str)
                .ok_or("pool_id expected has no pool_id")?
                .to_string(),
        )
    };
    Ok((
        obj(json!({
            "op": "pool_id",
            "a": xdr_of(case.get("a"), "a")?,
            "b": xdr_of(case.get("b"), "b")?,
            "fee": fee,
        })),
        expect,
    ))
}

fn map_sort(case: &Value) -> Mapped {
    let entries = case
        .get("entries")
        .and_then(Value::as_array)
        .ok_or("map_sort case has no entries")?;
    let mut list = Vec::with_capacity(entries.len());
    for (i, e) in entries.iter().enumerate() {
        list.push(json!({
            "key": xdr_of(e.get("key"), &format!("entries[{i}].key"))?,
            "val": xdr_of(e.get("val"), &format!("entries[{i}].val"))?,
        }));
    }
    let e = expected(case)?;
    let expect = if is_reject(e) {
        Expect::Rejected
    } else {
        Expect::Bytes(xdr_of(e.get("map"), "expected.map")?)
    };
    Ok((obj(json!({"op": "map_sort", "entries": list})), expect))
}

fn map_validate(case: &Value) -> Mapped {
    let valid = match expected(case)?.as_str() {
        Some("accept") => true,
        Some("reject") => false,
        other => return Err(format!("map_validate expected {other:?}")),
    };
    Ok((
        obj(json!({"op": "map_validate", "xdr": xdr_of(case.get("map"), "map")?})),
        Expect::Valid(valid),
    ))
}

fn id_parse(case: &Value) -> Mapped {
    let kind = case
        .get("kind")
        .and_then(Value::as_str)
        .ok_or("id_parse case has no kind")?;
    let input = case.get("input").ok_or("id_parse case has no input")?;
    let input = match (input.get("text"), input.get("number")) {
        (Some(t @ Value::String(_)), None) => t.clone(),
        (None, Some(n @ Value::Number(_))) => n.clone(),
        _ => {
            return Err(format!(
                "id_parse input {input} is not {{text}} or {{number}}"
            ));
        }
    };
    let e = expected(case)?;
    let expect = if let Some(either) = e.get("either") {
        id_or_rejected(either)?
    } else if is_reject(e) {
        Expect::Rejected
    } else {
        Expect::Id(
            e.get("id")
                .and_then(Value::as_str)
                .ok_or("id_parse expected has no id")?
                .to_string(),
        )
    };
    Ok((
        obj(json!({"op": "id_parse", "kind": kind, "input": input})),
        expect,
    ))
}

// `{"either": [{id, ...}, {"reject": ...}]}`: exactly one id alternative and one refusal
fn id_or_rejected(either: &Value) -> Result<Expect, String> {
    let alts = either
        .as_array()
        .ok_or("id_parse expected.either is not an array")?;
    let ids: Vec<&str> = alts
        .iter()
        .filter(|a| !is_reject(a))
        .map(|a| {
            a.get("id")
                .and_then(Value::as_str)
                .ok_or("an either alternative has no id")
        })
        .collect::<Result<_, _>>()?;
    let rejects = alts.iter().filter(|a| is_reject(a)).count();
    match (ids.as_slice(), rejects) {
        ([id], 1) => Ok(Expect::IdOrRejected((*id).to_string())),
        _ => Err(format!(
            "id_parse expected.either must hold one id and one reject, has {} ids and {rejects} rejects",
            ids.len()
        )),
    }
}

fn to_native(case: &Value) -> Mapped {
    let xdr = xdr_of(case.get("value"), "value")?;
    let e = expected(case)?;
    if e.get("rule").and_then(Value::as_str) != Some("no-entry-lost") {
        return Err(format!("to_native expected rule {:?}", e.get("rule")));
    }
    let bytes = B64
        .decode(&xdr)
        .map_err(|err| format!("value is not base64: {err}"))?;
    let entries = match ScVal::from_xdr(bytes, Limits::none()) {
        Ok(ScVal::Map(Some(m))) => m.len(),
        Ok(_) => return Err("to_native value is not a map".into()),
        Err(err) => return Err(format!("to_native value does not decode: {err}")),
    };
    if let Some(count) = e.get("entry_count").and_then(Value::as_u64)
        && usize::try_from(count).ok() != Some(entries)
    {
        return Err(format!(
            "expected.entry_count {count} disagrees with the map's {entries} entries"
        ));
    }
    Ok((
        obj(json!({"op": "to_native", "xdr": xdr})),
        Expect::NoEntryLost { entries },
    ))
}

/// A strkey_decode case, and for a valid string also the encode check back to the text.
fn strkey(index: usize, case: &Value, source: &str) -> Vec<Check> {
    let decode_name = format!("case {index}/strkey_decode");
    // text that is not utf-8 has no json string form, so such a case carries its bytes
    let (text, request) = match (
        case.get("text").and_then(Value::as_str),
        case.get("text_base64").and_then(Value::as_str),
    ) {
        (Some(t), None) => (Some(t), obj(json!({"op": "strkey_decode", "text": t}))),
        (None, Some(b)) => (None, obj(json!({"op": "strkey_decode", "text_base64": b}))),
        _ => {
            return vec![unmappable(
                decode_name,
                "strkey_decode",
                "case needs exactly one of text and text_base64".into(),
                source,
            )];
        }
    };
    let e = match expected(case) {
        Ok(e) => e,
        Err(why) => return vec![unmappable(decode_name, "strkey_decode", why, source)],
    };
    if is_reject(e) {
        return vec![Check {
            name: decode_name,
            op: "strkey_decode".into(),
            request,
            expect: Expect::Rejected,
            source: source.to_string(),
        }];
    }
    let Some(text) = text else {
        return vec![unmappable(
            decode_name,
            "strkey_decode",
            "a valid strkey is base32 text, so it cannot need text_base64".into(),
            source,
        )];
    };
    match raw_strkey(e) {
        Ok((kind, payload)) => {
            let payload = B64.encode(payload);
            vec![
                Check {
                    name: decode_name,
                    op: "strkey_decode".into(),
                    request,
                    expect: Expect::Strkey {
                        kind: kind.to_string(),
                        payload: payload.clone(),
                    },
                    source: source.to_string(),
                },
                Check {
                    name: format!("case {index}/strkey_encode"),
                    op: "strkey_encode".into(),
                    request: obj(json!({"op": "strkey_encode", "kind": kind, "payload": payload})),
                    expect: Expect::Text(text.to_string()),
                    source: format!("{source}: a valid strkey must encode back to its text"),
                },
            ]
        }
        Err(why) => vec![unmappable(decode_name, "strkey_decode", why, source)],
    }
}

fn hex_field(e: &Value, name: &str) -> Result<Vec<u8>, String> {
    let text = e
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("expected has no {name}"))?;
    hex::decode(text).map_err(|err| format!("expected.{name} is not hex: {err}"))
}

fn id_field(e: &Value) -> Result<u64, String> {
    e.get("id")
        .and_then(Value::as_str)
        .ok_or("expected has no decimal id")?
        .parse()
        .map_err(|err| format!("expected.id: {err}"))
}

/// The protocol's kind name and SEP-23 raw payload for a decoded strkey as the corpus writes it.
pub fn raw_strkey(e: &Value) -> Result<(&'static str, Vec<u8>), String> {
    let kind = e
        .get("kind")
        .and_then(Value::as_str)
        .ok_or("expected has no kind")?;
    Ok(match kind {
        "public_key_ed25519" => ("account", hex_field(e, "ed25519")?),
        "private_key_ed25519" => ("seed", hex_field(e, "seed")?),
        "muxed_account_ed25519" => {
            let mut raw = hex_field(e, "ed25519")?;
            raw.extend_from_slice(&id_field(e)?.to_be_bytes());
            ("muxed_account", raw)
        }
        "pre_auth_tx" => ("pre_auth_tx", hex_field(e, "hash")?),
        "hash_x" => ("hash_x", hex_field(e, "hash")?),
        "signed_payload_ed25519" => {
            let mut raw = hex_field(e, "ed25519")?;
            let inner = hex_field(e, "payload")?;
            let len = u32::try_from(inner.len()).map_err(|_| "payload too long")?;
            raw.extend_from_slice(&len.to_be_bytes());
            raw.extend_from_slice(&inner);
            raw.resize(raw.len() + (4 - inner.len() % 4) % 4, 0);
            ("signed_payload", raw)
        }
        "contract" => ("contract", hex_field(e, "contract")?),
        "liquidity_pool" => ("liquidity_pool", hex_field(e, "pool")?),
        "claimable_balance" => {
            let version = e
                .get("version")
                .and_then(Value::as_u64)
                .and_then(|v| u8::try_from(v).ok())
                .ok_or("expected has no one-byte version")?;
            let mut raw = vec![version];
            raw.extend_from_slice(&hex_field(e, "hash")?);
            ("claimable_balance", raw)
        }
        "muxed_contract" => {
            let mut raw = hex_field(e, "contract")?;
            raw.extend_from_slice(&id_field(e)?.to_be_bytes());
            ("muxed_contract", raw)
        }
        other => return Err(format!("unknown strkey kind {other:?}")),
    })
}
