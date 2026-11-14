//! Every synthetic expectation is recomputed here from the reference implementations, and the
//! committed corpus is checked to be exactly what `synth build` writes.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::Value;
use shapekit_gen::synth::{self, Synth};
use shapekit_gen::synth_order::{core_asset_cmp, host_accepts, host_sort, pool_id};
use shapekit_gen::synth_strkey::{SEP23_INVALID, SEP23_VALID, base32, crc16};
use shapekit_gen::synth_textid::{MAX_SAFE_INTEGER, negative_int64_text, parse_text};
use shapekit_gen::synth_xdr::{GHSA_3XHM_BASE64, base64_decode, decode_direct};
use shapekit_manifest::{
    DecodeExpectation, ENVELOPE_DEPTH, EntryKind, Limits, Validity, VectorFile, decode_limits, xdr,
};
use shapekit_registry::{Registry, SourceClass};
use stellar_xdr::ReadXdr;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn registry() -> Registry {
    Registry::load(&repo().join("registry")).expect("the registry loads")
}

// deep envelopes decode recursively, past what a default test thread's stack holds
fn big<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("the check thread panicked")
}

fn synth() -> &'static Synth {
    static S: OnceLock<Synth> = OnceLock::new();
    S.get_or_init(|| synth::generate(&registry()).expect("synth generates"))
}

fn vectors(kind: EntryKind) -> Vec<&'static VectorFile> {
    synth()
        .vectors
        .iter()
        .filter(|v| v.entry.kind == kind)
        .collect()
}

fn decode<T: ReadXdr>(v: &Value) -> T {
    let b64 = v["xdr"].as_str().expect("a value carries xdr");
    T::from_xdr_base64(b64, Limits::none()).expect("a vector value decodes")
}

fn tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    for item in walkdir::WalkDir::new(dir).sort_by_file_name() {
        let item = item.expect("walk");
        if item.file_type().is_file() {
            let rel = item
                .path()
                .strip_prefix(dir)
                .expect("inside")
                .to_string_lossy()
                .into_owned();
            out.insert(rel, std::fs::read(item.path()).expect("read"));
        }
    }
    out
}

#[test]
fn every_entry_is_synthetic_and_counts_are_stable() {
    let s = synth();
    for e in s
        .xdr
        .iter()
        .map(|x| &x.entry)
        .chain(s.vectors.iter().map(|v| &v.entry))
    {
        assert!(e.synthetic, "{}", e.id);
        assert_eq!(e.source.class, SourceClass::Synthetic, "{}", e.id);
        assert!(
            e.id.starts_with(&format!("{}.synth-", e.family)),
            "{}",
            e.id
        );
    }
    let counts = s.counts();
    assert_eq!(counts.get("xdr"), Some(&49));
    assert_eq!(counts.get("ordering"), Some(&17));
    assert_eq!(counts.get("strkey"), Some(&23));
    assert_eq!(counts.get("textual-id"), Some(&7));
    assert_eq!(counts.get("native-projection"), Some(&8));
}

#[test]
fn output_is_deterministic() {
    big(|| {
        let reg = registry();
        let a = synth::generate(&reg).expect("first run");
        let b = synth::generate(&reg).expect("second run");
        let da = tempfile::tempdir().expect("tempdir");
        let db = tempfile::tempdir().expect("tempdir");
        synth::write(da.path(), &a).expect("write a");
        synth::write(db.path(), &b).expect("write b");
        let (ta, tb) = (tree(da.path()), tree(db.path()));
        assert!(!ta.is_empty());
        assert_eq!(ta, tb);
        // a second write over the first changes nothing and removes nothing
        let report = synth::write(da.path(), &a).expect("rewrite");
        assert!(report.removed.is_empty());
        assert_eq!(tree(da.path()), tb);
    });
}

#[test]
fn stale_synthetic_files_are_removed_and_others_kept() {
    big(|| {
        let s = synth();
        let dir = tempfile::tempdir().expect("tempdir");
        synth::write(dir.path(), s).expect("write");
        let mut fewer = s.clone();
        let dropped_xdr = fewer.xdr.pop().expect("an xdr entry");
        let dropped_vec = fewer.vectors.pop().expect("a vector");
        let other = dir
            .path()
            .join("entries/cap27.muxed-accounts/not-synthetic");
        std::fs::create_dir_all(&other).expect("mkdir");
        std::fs::write(other.join("note.txt"), b"kept").expect("write");
        let report = synth::write(dir.path(), &fewer).expect("rewrite");
        let mut removed = report.removed.clone();
        removed.sort();
        let mut want = vec![dropped_xdr.entry.id.clone(), dropped_vec.entry.id.clone()];
        want.sort();
        assert_eq!(removed, want);
        assert!(
            !dir.path()
                .join(synth::entry_dir(&dropped_xdr.entry))
                .exists()
        );
        assert!(
            !dir.path()
                .join(synth::vector_path(&dropped_vec.entry))
                .exists()
        );
        assert!(other.join("note.txt").exists());
    });
}

#[test]
fn committed_corpus_matches_synth_build() {
    big(|| {
        let s = synth();
        let generated = tempfile::tempdir().expect("tempdir");
        synth::write(generated.path(), s).expect("write");
        let corpus = repo().join("corpus");
        for (rel, bytes) in tree(generated.path()) {
            let on_disk = std::fs::read(corpus.join(&rel)).unwrap_or_else(|e| {
                panic!("corpus/{rel} is missing ({e}); run shapekit synth build")
            });
            assert!(
                on_disk == bytes,
                "corpus/{rel} differs from synth build output; run shapekit synth build"
            );
        }
    });
}

#[test]
fn must_reject_vectors_are_rejected_by_stellar_xdr() {
    big(|| {
        let mut n = 0;
        for x in &synth().xdr {
            if x.entry.expected.decode != DecodeExpectation::MustReject {
                continue;
            }
            let ty = x.entry.xdr_type.as_deref().expect("xdr_type");
            assert_eq!(x.limits.len, x.bytes.len(), "{}", x.entry.id);
            let direct = decode_direct(ty, &x.bytes, x.limits.clone()).expect("a direct decoder");
            assert!(direct.is_err(), "{} decodes directly", x.entry.id);
            assert!(
                xdr::decode(ty, &x.bytes, x.limits.clone()).is_err(),
                "{} decodes via the toolkit",
                x.entry.id
            );
            // and with every limit lifted, the rejection is about the bytes, except for the
            // depth vectors, whose bytes are well formed and only too deep
            let unlimited = decode_direct(
                ty,
                &x.bytes,
                Limits {
                    depth: u32::MAX,
                    len: usize::MAX,
                },
            )
            .expect("a direct decoder");
            let depth_only = x.entry.flags.iter().any(|f| f == "over-depth-cap");
            assert_eq!(unlimited.is_ok(), depth_only, "{}", x.entry.id);
            n += 1;
        }
        assert_eq!(n, 35);
    });
}

#[test]
fn must_accept_vectors_round_trip_and_render() {
    big(|| {
        let mut n = 0;
        for x in &synth().xdr {
            if x.entry.expected.decode != DecodeExpectation::MustAccept {
                continue;
            }
            let ty = x.entry.xdr_type.as_deref().expect("xdr_type");
            let limits = decode_limits(ty, x.bytes.len());
            xdr::roundtrip_check(ty, &x.bytes, limits.clone())
                .unwrap_or_else(|e| panic!("{}: {e}", x.entry.id));
            let json = xdr::to_xdr_json(ty, &x.bytes, limits).expect("xdr-json");
            assert_eq!(Some(&json), x.expected_json.as_ref(), "{}", x.entry.id);
            n += 1;
        }
        assert_eq!(n, 12);
    });
}

#[test]
fn auth_depth_twins_sit_where_core_draws_the_line() {
    big(|| {
        let find = |id: &str| {
            synth()
                .xdr
                .iter()
                .find(|x| x.entry.id == id)
                .unwrap_or_else(|| panic!("no entry {id}"))
        };
        // core accepts 493 levels at depth 1000 and refuses 494 with txMALFORMED; the toolkit,
        // which `manifest validate` runs, has to agree with it on both
        for (id, accepted) in [
            (
                "soroban.auth-trees.synth-auth-depth-493-deepest-accepted",
                true,
            ),
            ("soroban.auth-trees.synth-auth-depth-494-over-cap", false),
        ] {
            let x = find(id);
            let limits = Limits {
                depth: ENVELOPE_DEPTH,
                len: x.bytes.len(),
            };
            let direct = decode_direct("TransactionEnvelope", &x.bytes, limits.clone())
                .expect("a direct decoder");
            assert_eq!(direct.is_ok(), accepted, "{id}: direct");
            assert_eq!(
                xdr::decode("TransactionEnvelope", &x.bytes, limits).is_ok(),
                accepted,
                "{id}: toolkit"
            );
        }
    });
}

#[test]
fn bool_two_is_read_as_false_by_stellar_xdr() {
    let x = synth()
        .xdr
        .iter()
        .find(|x| x.entry.id == "soroban.scval-shapes.synth-bool-value-2")
        .expect("the bool entry");
    assert_eq!(
        x.entry.expected.decode,
        DecodeExpectation::ImplementationDefined
    );
    let v = stellar_xdr::ScVal::from_xdr(&x.bytes, Limits::none()).expect("stellar-xdr accepts it");
    assert_eq!(v, stellar_xdr::ScVal::Bool(false));
}

#[test]
fn ghsa_bytes_are_the_advisory_struct() {
    let raw = base64_decode(GHSA_3XHM_BASE64).expect("base64");
    assert_eq!(raw.len(), 28);
    let x = synth()
        .xdr
        .iter()
        .find(|x| x.entry.id == "soroban.invoke-basics.synth-ghsa-3xhm-spec-outputs-2")
        .expect("the advisory entry");
    assert_eq!(x.bytes, raw);
    assert!(stellar_xdr::ScSpecFunctionV0::from_xdr(&raw, Limits::none()).is_err());
}

#[test]
fn asset_order_matches_core_rule_and_pool_ids() {
    let mut pairs = 0;
    for v in vectors(EntryKind::Ordering) {
        for case in &v.cases {
            match case["op"].as_str() {
                Some("asset_order") => {
                    let a: stellar_xdr::Asset = decode(&case["a"]);
                    let b: stellar_xdr::Asset = decode(&case["b"]);
                    let core = core_asset_cmp(&a, &b);
                    assert_eq!(core, a.cmp(&b), "{}", v.entry.id);
                    let want = match core {
                        Ordering::Less => "less",
                        Ordering::Equal => "equal",
                        Ordering::Greater => "greater",
                    };
                    assert_eq!(case["expected"], want, "{}", v.entry.id);
                    pairs += 1;
                }
                Some("pool_id") => {
                    let a: stellar_xdr::Asset = decode(&case["a"]);
                    let b: stellar_xdr::Asset = decode(&case["b"]);
                    if core_asset_cmp(&a, &b) == Ordering::Less {
                        let id = hex::encode(pool_id(&a, &b).expect("pool id"));
                        assert_eq!(case["expected"]["pool_id"], id.as_str(), "{}", v.entry.id);
                        let l = stellar_strkey::Strkey::from_string(
                            case["expected"]["strkey"].as_str().expect("strkey"),
                        )
                        .expect("pool strkey decodes");
                        assert_eq!(
                            l.to_string().to_string(),
                            case["expected"]["strkey"].as_str().unwrap()
                        );
                    } else {
                        assert!(case["expected"]["reject"].is_string(), "{}", v.entry.id);
                    }
                }
                Some("map_sort") | Some("map_validate") => {}
                other => panic!("unknown ordering op {other:?}"),
            }
        }
    }
    assert_eq!(pairs, 20);
}

#[test]
fn r14_and_testnet_facts_hold() {
    let zero = stellar_strkey::ed25519::PublicKey([0; 32]).to_string();
    let colon = stellar_strkey::ed25519::PublicKey([0x3a; 32]).to_string();
    assert_eq!(zero.as_str(), shapekit_gen::synth_order::R14_ZERO_ISSUER);
    assert_eq!(colon.as_str(), shapekit_gen::synth_order::R14_COLON_ISSUER);
    // by text the 0x3a issuer is lower, by bytes the zero issuer is
    assert!(colon.as_str() < zero.as_str());
    let v = vectors(EntryKind::Ordering)
        .into_iter()
        .find(|v| v.entry.id == "cap38.liquidity-pools.synth-assets-py1246-testnet-pool")
        .expect("the py-stellar-base pair");
    let ids: Vec<&str> = v
        .cases
        .iter()
        .filter_map(|c| c["expected"]["pool_id"].as_str())
        .collect();
    assert_eq!(ids, vec![shapekit_gen::synth_order::PY_1246_POOL_ID]);
}

#[test]
fn map_order_matches_soroban_env_host() {
    let mut sets = 0;
    for v in vectors(EntryKind::Ordering) {
        for case in &v.cases {
            match case["op"].as_str() {
                Some("map_sort") => {
                    let entries: Vec<stellar_xdr::ScMapEntry> = case["entries"]
                        .as_array()
                        .expect("entries")
                        .iter()
                        .map(|e| stellar_xdr::ScMapEntry {
                            key: decode(&e["key"]),
                            val: decode(&e["val"]),
                        })
                        .collect();
                    let sorted = host_sort(&entries).expect("host sort");
                    let order: Vec<u64> = case["expected"]["order"]
                        .as_array()
                        .expect("order")
                        .iter()
                        .map(|i| i.as_u64().expect("index"))
                        .collect();
                    let by_order: Vec<stellar_xdr::ScMapEntry> =
                        order.iter().map(|&i| entries[i as usize].clone()).collect();
                    assert_eq!(by_order, sorted, "{}", v.entry.id);
                    let map: stellar_xdr::ScVal = decode(&case["expected"]["map"]);
                    let stellar_xdr::ScVal::Map(Some(m)) = &map else {
                        panic!("a map")
                    };
                    assert_eq!(m.0.to_vec(), sorted, "{}", v.entry.id);
                    sets += 1;
                }
                Some("map_validate") => {
                    let map: stellar_xdr::ScVal = decode(&case["map"]);
                    let want = case["expected"] == "accept";
                    assert_eq!(host_accepts(&map), want, "{}", v.entry.id);
                }
                _ => {}
            }
        }
    }
    assert_eq!(sets, 7);
}

fn strkey_verdict(text: &[u8]) -> Result<String, stellar_strkey::DecodeError> {
    if text.first() == Some(&b'S') {
        let k = stellar_strkey::ed25519::PrivateKey::from_slice(text)?;
        return Ok(hex::encode(k.0));
    }
    Ok(stellar_strkey::Strkey::from_slice(text)?
        .to_string()
        .to_string())
}

#[test]
fn strkey_expectations_match_stellar_strkey_and_sep23() {
    let mut texts = Vec::new();
    for v in vectors(EntryKind::Strkey) {
        let must_accept = v.entry.expected.decode == DecodeExpectation::MustAccept;
        for case in &v.cases {
            // input that is not utf-8 travels as text_base64, and only invalid strings need it
            if let Some(b) = case["text_base64"].as_str() {
                assert!(!must_accept, "{}: a valid strkey as bytes", v.entry.id);
                assert!(
                    case["text"].is_null(),
                    "{}: text and text_base64",
                    v.entry.id
                );
                let raw = B64.decode(b).expect("text_base64 is base64");
                assert!(
                    std::str::from_utf8(&raw).is_err(),
                    "{}: utf-8 as bytes",
                    v.entry.id
                );
                assert!(
                    strkey_verdict(&raw).is_err(),
                    "{}: {raw:?} accepted",
                    v.entry.id
                );
                assert!(case["expected"]["reject"].is_string());
                continue;
            }
            let text = case["text"].as_str().expect("text");
            texts.push(text.to_string());
            let verdict = strkey_verdict(text.as_bytes());
            if must_accept {
                let got =
                    verdict.unwrap_or_else(|e| panic!("{}: {text} rejected: {e}", v.entry.id));
                if !text.starts_with('S') {
                    assert_eq!(got, text, "canonical text");
                }
                assert!(case["expected"]["kind"].is_string());
            } else {
                assert!(verdict.is_err(), "{}: {text:?} accepted", v.entry.id);
                assert!(case["expected"]["reject"].is_string());
            }
        }
    }
    for s in SEP23_VALID {
        assert!(strkey_verdict(s.as_bytes()).is_ok(), "{s}");
        assert!(
            texts.iter().any(|t| t == s),
            "SEP-23 valid case {s} has no vector"
        );
    }
    for s in SEP23_INVALID {
        assert!(strkey_verdict(s.as_bytes()).is_err(), "{s}");
        assert!(
            texts.iter().any(|t| t == s),
            "SEP-23 invalid case {s} has no vector"
        );
    }
}

#[test]
fn every_prefix_has_valid_and_invalid_vectors() {
    let ids: Vec<&str> = vectors(EntryKind::Strkey)
        .iter()
        .map(|v| v.entry.id.as_str())
        .collect();
    for letter in ["g", "m", "s", "t", "x", "p", "c", "l", "b", "w"] {
        for side in ["valid", "invalid"] {
            let suffix = format!(".synth-strkey-{letter}-{side}");
            assert!(ids.iter().any(|id| id.ends_with(&suffix)), "no {suffix}");
        }
    }
}

#[test]
fn signed_payload_sizes_are_covered() {
    let payload_lens = |side: &str| -> Vec<usize> {
        let v = vectors(EntryKind::Strkey)
            .into_iter()
            .find(|v| v.entry.id.ends_with(&format!("strkey-p-{side}")))
            .expect("p vectors");
        v.cases
            .iter()
            .filter_map(|c| c["expected"]["payload"].as_str().map(|p| p.len() / 2))
            .collect()
    };
    let valid = payload_lens("valid");
    for n in [1, 64] {
        assert!(valid.contains(&n), "no valid {n}-byte signed payload");
    }
    let invalid = vectors(EntryKind::Strkey)
        .into_iter()
        .find(|v| v.entry.id.ends_with("strkey-p-invalid"))
        .expect("p invalid");
    let notes: Vec<&str> = invalid
        .cases
        .iter()
        .filter_map(|c| c["note"].as_str())
        .collect();
    assert!(notes.iter().any(|n| n.starts_with("an empty payload")));
    assert!(notes.iter().any(|n| n.starts_with("a 65-byte payload")));
}

#[test]
fn local_strkey_encoder_matches_stellar_strkey() {
    let mut bin = vec![6 << 3];
    bin.extend_from_slice(&[0x3a; 32]);
    bin.extend_from_slice(&crc16(&bin));
    assert_eq!(base32(&bin, 0), shapekit_gen::synth_order::R14_COLON_ISSUER);
}

#[test]
fn textual_ids_follow_the_canonical_rule() {
    let mut accepted = 0;
    let mut rejected = 0;
    let mut lenient = 0;
    let mut negative = 0;
    for v in vectors(EntryKind::TextualId) {
        let muxed = v.entry.id.contains("muxed");
        let max = if muxed { u64::MAX } else { i64::MAX as u64 };
        if v.entry.id.ends_with("-offer-id-negative") {
            // valid int64 values core refuses at validation: the exact id or a refusal
            assert_eq!(
                v.entry.expected.decode,
                DecodeExpectation::ImplementationDefined
            );
            assert_eq!(v.entry.expected.validity, Validity::InvalidByCore);
            for case in &v.cases {
                let id: i64 = match case["input"]["text"].as_str() {
                    Some(t) => {
                        assert_eq!(negative_int64_text(t), t.parse().ok(), "{t:?}");
                        t.parse().expect("an int64")
                    }
                    None => case["input"]["number"].as_i64().expect("an int64"),
                };
                assert!(id < 0, "{id}");
                let either = case["expected"]["either"].as_array().expect("either");
                assert_eq!(either[0]["id"], id.to_string());
                let op: stellar_xdr::ManageSellOfferOp = decode(&either[0]["manage_sell_offer"]);
                assert_eq!(op.offer_id, id);
                assert_eq!(either[1]["reject"], "negative");
                negative += 1;
            }
            continue;
        }
        if v.entry.expected.decode == DecodeExpectation::ImplementationDefined {
            // forms that name one in-range id: the exact id or a refusal
            assert!(
                v.entry.id.ends_with("-implementation-defined"),
                "{}",
                v.entry.id
            );
            for case in &v.cases {
                let (id, reason) = if let Some(t) = case["input"]["text"].as_str() {
                    assert!(t.len() > 1 && t.starts_with('0'), "{t:?}");
                    (t.parse::<u64>().expect("digits"), "leading-zero")
                } else {
                    let n = case["input"]["number"].as_number().expect("number");
                    match n.as_u64() {
                        Some(u) => {
                            assert!(u > MAX_SAFE_INTEGER, "{u}");
                            (u, "unsafe-integer")
                        }
                        None => {
                            let f = n.as_f64().expect("float");
                            assert_eq!(f.fract(), 0.0);
                            (f as u64, "integral-fraction")
                        }
                    }
                };
                assert!(id <= max, "{}: {id}", v.entry.id);
                let either = case["expected"]["either"].as_array().expect("either");
                assert_eq!(either[0]["id"], id.to_string());
                assert_eq!(either[1]["reject"], reason);
                lenient += 1;
            }
            continue;
        }
        let must_accept = v.entry.expected.decode == DecodeExpectation::MustAccept;
        for case in &v.cases {
            let id = if let Some(t) = case["input"]["text"].as_str() {
                parse_text(t, max).ok()
            } else {
                let n = case["input"]["number"].as_number().expect("number");
                n.as_u64().filter(|&u| u <= MAX_SAFE_INTEGER && u <= max)
            };
            if must_accept {
                let id = id.expect("an accepted case parses");
                assert_eq!(case["expected"]["id"], id.to_string());
                if muxed {
                    let m = stellar_strkey::ed25519::MuxedAccount::from_string(
                        case["expected"]["muxed_address"].as_str().expect("address"),
                    )
                    .expect("the M address decodes");
                    assert_eq!(m.id, id);
                    let x: stellar_xdr::MuxedAccount = decode(&case["expected"]["muxed_account"]);
                    let stellar_xdr::MuxedAccount::MuxedEd25519(med) = x else {
                        panic!("muxed")
                    };
                    assert_eq!(med.id, id);
                } else {
                    let op: stellar_xdr::ManageSellOfferOp =
                        decode(&case["expected"]["manage_sell_offer"]);
                    assert_eq!(op.offer_id as u64, id);
                }
                accepted += 1;
            } else {
                assert!(id.is_none(), "{}: {case}", v.entry.id);
                rejected += 1;
            }
        }
    }
    // the js-stellar-sdk#1785 inputs that encode a different id are all refused
    for t in ["0x10", " 16 ", ""] {
        assert!(parse_text(t, u64::MAX).is_err(), "{t:?}");
    }
    // non-canonical text stays refused; "-0" is not a canonical int64 spelling
    for t in ["-16", "+16", "-0", " 16", "0x10", ""] {
        assert!(parse_text(t, i64::MAX as u64).is_err(), "{t:?}");
    }
    for t in ["-0", "-016", "+16", "-", "-9223372036854775809"] {
        assert_eq!(negative_int64_text(t), None, "{t:?}");
    }
    assert_eq!((accepted, rejected, lenient, negative), (22, 42, 13, 5));
}

#[test]
fn native_projection_maps_are_host_valid_with_colliding_keys() {
    for v in vectors(EntryKind::NativeProjection) {
        assert_eq!(v.cases.len(), 1);
        let case = &v.cases[0];
        let map: stellar_xdr::ScVal = decode(&case["value"]);
        assert!(host_accepts(&map), "{}", v.entry.id);
        let stellar_xdr::ScVal::Map(Some(m)) = &map else {
            panic!("a map")
        };
        assert_eq!(case["expected"]["entry_count"], m.len());
        assert_eq!(case["expected"]["rule"], "no-entry-lost");
        let mut keys: Vec<&stellar_xdr::ScVal> = m.iter().map(|e| &e.key).collect();
        keys.dedup();
        assert_eq!(keys.len(), m.len(), "{} has duplicate keys", v.entry.id);
    }
    let literal = vectors(EntryKind::NativeProjection)
        .into_iter()
        .find(|v| v.entry.id.ends_with("map-js1789-u32-symbol"))
        .expect("the #1789 literal");
    let map: stellar_xdr::ScVal = decode(&literal.cases[0]["value"]);
    let want = stellar_xdr::ScVal::Map(Some(stellar_xdr::ScMap(
        vec![
            stellar_xdr::ScMapEntry {
                key: stellar_xdr::ScVal::U32(1),
                val: stellar_xdr::ScVal::U32(10),
            },
            stellar_xdr::ScMapEntry {
                key: stellar_xdr::ScVal::Symbol("1".try_into().unwrap()),
                val: stellar_xdr::ScVal::U32(20),
            },
        ]
        .try_into()
        .unwrap(),
    )));
    assert_eq!(map, want);
}

#[test]
fn rfp_f7_shapes_are_present() {
    let s = synth();
    let has = |flag: &str| {
        s.xdr
            .iter()
            .map(|x| &x.entry)
            .chain(s.vectors.iter().map(|v| &v.entry))
            .any(|e| e.flags.iter().any(|f| f == flag))
    };
    for flag in [
        "issuer-order-differs",
        "muxed-account",
        "non-canonical-id",
        "max-length-vec",
        "nested-auth",
        "map-key-collision-after-native",
    ] {
        assert!(has(flag), "no synthetic entry flagged {flag}");
    }
}
