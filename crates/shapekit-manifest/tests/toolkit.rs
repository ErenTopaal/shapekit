//! Toolkit tests on real testnet XDR. The fixtures and how they were taken are described in
//! tests/fixtures/testnet-5694308.provenance.json.

use std::path::{Path, PathBuf};

use shapekit_manifest::framing::{FramedReader, decode_framed, frame_records};
use shapekit_manifest::signature::check_fragment;
use shapekit_manifest::{
    Limits, ToolkitError, fragment_matches, reachable_arms, roundtrip_check, sha256_hex,
    shape_signature, to_xdr_json,
};
use stellar_xdr::{
    FeeBumpTransactionInnerTx, GeneralizedTransactionSet, LedgerCloseMeta, ReadXdr, ScVal,
    TransactionEnvelope, TransactionMeta, TransactionPhase, Type, WriteXdr,
};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(fixture_path(name)).unwrap()
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

const LCM: &str = "testnet-5694308.lcm.xdr";
const ENVELOPE: &str = "testnet-5694308.envelope.xdr";
const SCVAL: &str = "testnet-5694308.scval.xdr";

fn limits(bytes: &[u8]) -> Limits {
    Limits {
        depth: shapekit_manifest::META_DEPTH,
        len: bytes.len(),
    }
}

fn fixtures() -> [(&'static str, Vec<u8>); 3] {
    [
        ("LedgerCloseMeta", fixture(LCM)),
        ("TransactionEnvelope", fixture(ENVELOPE)),
        ("ScVal", fixture(SCVAL)),
    ]
}

#[test]
fn provenance_matches_the_fixture_files() {
    let prov: serde_json::Value =
        serde_json::from_slice(&fixture("testnet-5694308.provenance.json")).unwrap();
    assert_eq!(prov["sha256"], sha256_hex(&fixture(LCM)));
    assert_eq!(prov["ledger_sequence"], 5694308);
    for name in [ENVELOPE, SCVAL] {
        assert_eq!(
            prov["derived"][name]["sha256"],
            sha256_hex(&fixture(name)),
            "{name}"
        );
    }
    let lcm = LedgerCloseMeta::from_xdr(fixture(LCM), limits(&fixture(LCM))).unwrap();
    let LedgerCloseMeta::V2(v2) = &lcm else {
        panic!("expected a v2 ledger close meta");
    };
    assert_eq!(v2.ledger_header.header.ledger_seq, 5694308);
    assert_eq!(
        hex_of(&v2.ledger_header.hash.0),
        prov["ledger_hash"].as_str().unwrap()
    );
}

fn hex_of(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// the envelope and scval fixtures are re-derived from the ledger, so they cannot drift from it
#[test]
fn derived_fixtures_come_from_the_ledger() {
    let lcm = LedgerCloseMeta::from_xdr(fixture(LCM), Limits::none()).unwrap();
    let LedgerCloseMeta::V2(v2) = &lcm else {
        panic!("expected a v2 ledger close meta");
    };
    let GeneralizedTransactionSet::V1(set) = &v2.tx_set;
    let envelope = set
        .phases
        .iter()
        .find_map(|p| match p {
            TransactionPhase::V1(par) => par
                .execution_stages
                .iter()
                .flat_map(|s| s.iter())
                .flat_map(|c| c.iter())
                .find(|e| matches!(e, TransactionEnvelope::TxFeeBump(_)))
                .cloned(),
            TransactionPhase::V0(_) => None,
        })
        .unwrap();
    assert_eq!(envelope.to_xdr(Limits::none()).unwrap(), fixture(ENVELOPE));
    let scval = v2
        .tx_processing
        .iter()
        .find_map(|r| match &r.tx_apply_processing {
            TransactionMeta::V4(m) => m
                .soroban_meta
                .as_ref()
                .and_then(|s| s.return_value.clone())
                .filter(|v| matches!(v, ScVal::Vec(_))),
            _ => None,
        })
        .unwrap();
    assert_eq!(scval.to_xdr(Limits::none()).unwrap(), fixture(SCVAL));

    // the ledger holds both classic and soroban transactions
    let TransactionEnvelope::TxFeeBump(fb) = &envelope else {
        unreachable!()
    };
    let FeeBumpTransactionInnerTx::Tx(inner) = &fb.tx.inner_tx;
    assert_eq!(inner.tx.operations[0].body.name(), "InvokeHostFunction");
    let classic = set
        .phases
        .iter()
        .any(|p| matches!(p, TransactionPhase::V0(c) if !c.is_empty()));
    assert!(classic);
}

#[test]
fn roundtrip_passes_on_real_xdr() {
    for (ty, bytes) in fixtures() {
        roundtrip_check(ty, &bytes, limits(&bytes)).unwrap_or_else(|e| panic!("{ty}: {e}"));
    }
}

#[test]
fn roundtrip_fails_on_corrupted_copies() {
    for (ty, bytes) in fixtures() {
        // a trailing byte is never consumed
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(
            matches!(
                roundtrip_check(ty, &extra, limits(&extra)),
                Err(ToolkitError::Decode(_))
            ),
            "{ty} with a trailing byte"
        );
        // truncation cuts a value short
        let short = &bytes[..bytes.len() - 1];
        assert!(
            roundtrip_check(ty, short, limits(short)).is_err(),
            "{ty} truncated"
        );
    }
    // an out-of-range union discriminant: ScVal's type is the first word
    let mut bad = fixture(SCVAL);
    bad[3] = 0xff;
    assert!(matches!(
        roundtrip_check("ScVal", &bad, limits(&bad)),
        Err(ToolkitError::Decode(_))
    ));
    // a limit below the value's depth refuses it
    let bytes = fixture(SCVAL);
    let shallow = Limits {
        depth: 2,
        len: bytes.len(),
    };
    assert!(roundtrip_check("ScVal", &bytes, shallow).is_err());
    assert!(matches!(
        roundtrip_check("NoSuchType", &bytes, limits(&bytes)),
        Err(ToolkitError::UnknownType(_))
    ));
}

#[test]
fn xdr_json_is_stable() {
    for (ty, bytes) in fixtures() {
        let a = to_xdr_json(ty, &bytes, limits(&bytes)).unwrap();
        let b = to_xdr_json(ty, &bytes, limits(&bytes)).unwrap();
        assert_eq!(a, b);
        assert!(a.ends_with("}\n") && !a.ends_with("\n\n"), "{ty}");
        // the json converts back to the same bytes through the crate's own serde
        let variant = shapekit_manifest::xdr::type_variant(ty).unwrap();
        let back = Type::from_json(variant, a.as_bytes()).unwrap();
        assert_eq!(back.to_xdr(Limits::none()).unwrap(), bytes, "{ty}");
    }
    let scval = to_xdr_json("ScVal", &fixture(SCVAL), limits(&fixture(SCVAL))).unwrap();
    let expected = std::fs::read_to_string(fixture_path("testnet-5694308.scval.json")).unwrap();
    assert_eq!(scval, expected);
}

#[test]
fn xdr_json_keys_are_sorted() {
    let bytes = fixture(ENVELOPE);
    let text = to_xdr_json("TransactionEnvelope", &bytes, limits(&bytes)).unwrap();
    // struct order would put source_account before fee in the transaction
    let fee = text.find("\"fee\"").unwrap();
    let ext = text.find("\"ext\"").unwrap();
    assert!(ext < fee);
}

#[test]
fn signature_of_the_ledger() {
    let bytes = fixture(LCM);
    let sig = shape_signature("LedgerCloseMeta", &bytes, limits(&bytes)).unwrap();
    for path in [
        "LedgerCloseMeta",
        "LedgerCloseMeta:v2",
        "LedgerCloseMeta:v2.tx_set:v1.phases[]:v0[]:txset_comp_txs_maybe_discounted_fee.txs[]:tx.tx.operations[].body:payment.destination:ed25519",
        "LedgerCloseMeta:v2.tx_set:v1.phases[]:v1.execution_stages[][][]:tx.tx.operations[].body:invoke_host_function.auth[].credentials:source_account",
        "LedgerCloseMeta:v2.tx_set:v1.phases[]:v1.execution_stages[][][]:tx_fee_bump.signatures[].signature#max",
        "LedgerCloseMeta:v2.tx_processing[].tx_apply_processing:v4.events[].stage=after_all_txs",
        "LedgerCloseMeta:v2.ledger_header.header.scp_value.ext:signed_ms.lc_value_signature.signature#max",
        "LedgerCloseMeta:v2.tx_processing[].fee_processing[]:state.data:account.inflation_dest#none",
        "OperationBody:payment",
        "OperationBody:invoke_host_function",
        "TransactionEnvelope:tx_fee_bump",
        "MuxedAccount:ed25519",
        "ScAddress:contract",
        "ContractEventType=contract",
    ] {
        assert!(sig.contains(path), "missing {path}");
    }
    // leaf values never appear: no strkey, no number
    assert!(sig.iter().all(|p| !p.contains("GBBD47")));
    assert!(sig.iter().all(|p| !p.contains("5694308")));
    // every rooted path's parent is present
    for p in sig.iter().filter(|p| p.starts_with("LedgerCloseMeta:")) {
        let parent = match p.strip_suffix("[]") {
            Some(parent) => parent,
            None => &p[..p.rfind(['.', ':', '=', '#']).unwrap()],
        };
        assert!(sig.contains(parent), "{p} without {parent}");
    }
    assert!(fragment_matches(&sig, ".body:payment.destination:ed25519"));
    assert!(!fragment_matches(&sig, "SignerKey:ed25519_signed_payload"));
}

#[test]
fn signature_of_the_envelope_and_scval() {
    let bytes = fixture(ENVELOPE);
    let sig = shape_signature("TransactionEnvelope", &bytes, limits(&bytes)).unwrap();
    for path in [
        "TransactionEnvelope:tx_fee_bump.tx.fee_source:ed25519",
        "TransactionEnvelope:tx_fee_bump.signatures#one",
        "TransactionEnvelope:tx_fee_bump.tx.inner_tx:tx.tx.ext:v1.resources.footprint.read_only[]:contract_data.durability=persistent",
        "TransactionEnvelope:tx_fee_bump.tx.inner_tx:tx.tx.memo:none",
        "SorobanCredentials:address",
        "HostFunction:invoke_contract",
    ] {
        assert!(sig.contains(path), "missing {path}");
    }

    let bytes = fixture(SCVAL);
    let sig = shape_signature("ScVal", &bytes, limits(&bytes)).unwrap();
    let expected: Vec<&str> = vec![
        "ScVal",
        "ScVal:i128",
        "ScVal:vec",
        "ScVal:vec#many",
        "ScVal:vec[]",
        "ScVal:vec[]:vec",
        "ScVal:vec[]:vec#many",
        "ScVal:vec[]:vec[]",
        "ScVal:vec[]:vec[]:i128",
    ];
    assert_eq!(sig.iter().map(String::as_str).collect::<Vec<_>>(), expected);
}

#[test]
fn signature_records_signed_payload_and_bounds() {
    use stellar_xdr::{
        Asset, BytesM, DecoratedSignature, Memo, MuxedAccount, MuxedAccountMed25519, Operation,
        OperationBody, PaymentOp, Preconditions, SequenceNumber, SetOptionsOp, Signature,
        SignatureHint, Signer, SignerKey, SignerKeyEd25519SignedPayload, StringM, Transaction,
        TransactionExt, TransactionV1Envelope, Uint256,
    };
    let payload = |n: usize| SignerKeyEd25519SignedPayload {
        ed25519: Uint256([7; 32]),
        payload: BytesM::try_from(vec![1u8; n]).unwrap(),
    };
    let set_options = |n: usize| Operation {
        source_account: Some(MuxedAccount::MuxedEd25519(MuxedAccountMed25519 {
            id: 9,
            ed25519: Uint256([3; 32]),
        })),
        body: OperationBody::SetOptions(SetOptionsOp {
            inflation_dest: None,
            clear_flags: None,
            set_flags: None,
            master_weight: None,
            low_threshold: None,
            med_threshold: None,
            high_threshold: None,
            home_domain: None,
            signer: Some(Signer {
                key: SignerKey::Ed25519SignedPayload(payload(n)),
                weight: 1,
            }),
        }),
    };
    let ops = vec![
        set_options(64),
        set_options(0),
        Operation {
            source_account: None,
            body: OperationBody::Payment(PaymentOp {
                destination: MuxedAccount::Ed25519(Uint256([5; 32])),
                asset: Asset::Native,
                amount: 1,
            }),
        },
    ];
    let env = TransactionEnvelope::Tx(TransactionV1Envelope {
        tx: Transaction {
            source_account: MuxedAccount::Ed25519(Uint256([1; 32])),
            fee: 100,
            seq_num: SequenceNumber(1),
            cond: Preconditions::None,
            memo: Memo::Text(StringM::try_from(vec![0xffu8; 28]).unwrap()),
            operations: ops.try_into().unwrap(),
            ext: TransactionExt::V0,
        },
        signatures: vec![
            DecoratedSignature {
                hint: SignatureHint([0; 4]),
                signature: Signature(BytesM::try_from(vec![0u8; 64]).unwrap()),
            };
            20
        ]
        .try_into()
        .unwrap(),
    });
    let bytes = env.to_xdr(Limits::none()).unwrap();
    let sig = shape_signature("TransactionEnvelope", &bytes, limits(&bytes)).unwrap();
    for path in [
        "TransactionEnvelope:tx.signatures#max",
        "TransactionEnvelope:tx.tx.memo:text#max",
        "TransactionEnvelope:tx.tx.operations#many",
        "TransactionEnvelope:tx.tx.cond:none",
        "TransactionEnvelope:tx.tx.operations[].source_account:muxed_ed25519",
        "TransactionEnvelope:tx.tx.operations[].source_account#none",
        "TransactionEnvelope:tx.tx.operations[].body:set_options.signer.key:ed25519_signed_payload.payload#max",
        "TransactionEnvelope:tx.tx.operations[].body:set_options.signer.key:ed25519_signed_payload.payload#empty",
        "TransactionEnvelope:tx.tx.operations[].body:set_options.home_domain#none",
        "SignerKey:ed25519_signed_payload",
        "MuxedAccount:muxed_ed25519",
        "Asset:native",
    ] {
        assert!(sig.contains(path), "missing {path}");
    }
    assert!(fragment_matches(
        &sig,
        ".signer.key:ed25519_signed_payload.payload#empty"
    ));
}

// the coverage denominator; a change here means the xdr crate changed what is reachable
#[test]
fn reachable_arms_of_ledger_close_meta_are_stable() {
    let arms = reachable_arms("LedgerCloseMeta").unwrap();
    assert_eq!(arms.len(), 499);
    for arm in [
        "LedgerCloseMeta:v0",
        "LedgerCloseMeta:v2",
        "TransactionMeta:v4",
        "OperationBody:invoke_host_function",
        "SignerKey:ed25519_signed_payload",
        "MuxedAccount:muxed_ed25519",
        "ScAddress:muxed_contract",
        "StellarValueExt:signed_ms",
        "ScVal:void",
    ] {
        assert!(arms.contains(arm), "missing {arm}");
    }
    // enums are values, not arms
    assert!(!arms.iter().any(|a| a.starts_with("ContractEventType")));
    // every arm the real ledger takes is in the denominator
    let bytes = fixture(LCM);
    let sig = shape_signature("LedgerCloseMeta", &bytes, limits(&bytes)).unwrap();
    let anchored: Vec<&String> = sig
        .iter()
        .filter(|p| !p.starts_with("LedgerCloseMeta") && p.contains(':'))
        .collect();
    assert!(!anchored.is_empty());
    for a in anchored {
        assert!(arms.contains(a), "{a} taken but not reachable");
    }
}

#[test]
fn signature_fragments_from_real_paths_are_valid() {
    let bytes = fixture(LCM);
    let sig = shape_signature("LedgerCloseMeta", &bytes, limits(&bytes)).unwrap();
    for p in &sig {
        check_fragment(p).unwrap_or_else(|e| panic!("{e}"));
    }
}

#[test]
fn framed_stream_round_trips() {
    let lcm = fixture(LCM);
    let env = fixture(ENVELOPE);
    let stream = frame_records([lcm.as_slice(), lcm.as_slice()]).unwrap();
    assert_eq!(stream.len(), 2 * (4 + lcm.len()));
    let records: Vec<Vec<u8>> = FramedReader::new(stream.as_slice(), 1 << 20)
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(records, vec![lcm.clone(), lcm.clone()]);
    assert_eq!(
        frame_records(records.iter().map(Vec::as_slice)).unwrap(),
        stream
    );

    let values = decode_framed(
        "LedgerCloseMeta",
        &stream,
        shapekit_manifest::decode_limits("LedgerCloseMeta", 1 << 20),
    )
    .unwrap();
    assert_eq!(values.len(), 2);

    // agrees with the crate's own framed reader
    let mut limited = stellar_xdr::Limited::new(stream.as_slice(), Limits::none());
    let theirs: Vec<Type> =
        Type::read_xdr_framed_iter(stellar_xdr::TypeVariant::LedgerCloseMeta, &mut limited)
            .collect::<Result<_, _>>()
            .unwrap();
    assert_eq!(theirs, values);

    // a record split across two fragments is joined
    let mut split = Vec::new();
    split.extend_from_slice(&(10u32).to_be_bytes());
    split.extend_from_slice(&env[..10]);
    split.extend_from_slice(&((1u32 << 31) | (env.len() as u32 - 10)).to_be_bytes());
    split.extend_from_slice(&env[10..]);
    let joined: Vec<Vec<u8>> = FramedReader::new(split.as_slice(), 1 << 20)
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(joined, vec![env.clone()]);
}

#[test]
fn framed_stream_errors() {
    let env = fixture(ENVELOPE);
    let stream = frame_records([env.as_slice()]).unwrap();
    let truncated = &stream[..stream.len() - 1];
    let r: Vec<_> = FramedReader::new(truncated, 1 << 20).collect();
    assert_eq!(r.len(), 1);
    assert!(
        r[0].as_ref()
            .unwrap_err()
            .to_string()
            .contains("declares 1212 bytes")
    );

    let header_cut = &stream[..2];
    let r: Vec<_> = FramedReader::new(header_cut, 1 << 20).collect();
    assert!(r[0].is_err());

    let r: Vec<_> = FramedReader::new(stream.as_slice(), 100).collect();
    assert!(
        r[0].as_ref()
            .unwrap_err()
            .to_string()
            .contains("longer than the 100 byte limit")
    );

    assert_eq!(FramedReader::new(&[][..], 100).count(), 0);
}

#[test]
fn decoding_a_framed_value_with_trailing_bytes_fails() {
    let env = fixture(ENVELOPE);
    let mut padded = env.clone();
    padded.extend_from_slice(&[0, 0, 0, 0]);
    let stream = frame_records([padded.as_slice()]).unwrap();
    assert!(decode_framed("TransactionEnvelope", &stream, Limits::none()).is_err());
    let ok = frame_records([env.as_slice()]).unwrap();
    let decoded = decode_framed("TransactionEnvelope", &ok, Limits::none()).unwrap();
    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].to_xdr(Limits::none()).unwrap(), env);
}
