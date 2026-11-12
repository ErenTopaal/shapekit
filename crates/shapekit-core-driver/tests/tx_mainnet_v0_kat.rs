//! Known answers for v0 envelopes from mainnet history at protocol 12, where
//! v0 was the only envelope: the hash recomputed from the envelope must equal
//! the hash the validators wrote into the checkpoint's results, every
//! signature must verify over it, and re-signing the same transaction through
//! the v1 form must give the same payload. Provenance is in
//! `fixtures/mainnet-v0-transactions.provenance.json`.

use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use serde_json::Value;
use shapekit_core_driver::keys::network_id;
use shapekit_core_driver::tx::{
    FIRST_V1_ENVELOPE_PROTOCOL, TxError, envelope_hash, hint, sign_for_protocol,
    sign_transaction_v0, to_v0, transaction_hash, transaction_v0_hash,
};
use stellar_xdr::{
    Limits, MuxedAccount, MuxedAccountMed25519, Preconditions, PreconditionsV2, ReadXdr,
    SequenceNumber, Transaction, TransactionEnvelope, TransactionExt, TransactionV0,
    TransactionV0Envelope, Uint256, WriteXdr,
};

const MAINNET: &str = "Public Global Stellar Network ; September 2015";

fn fixture() -> Vec<(String, String, TransactionV0Envelope)> {
    let raw = include_str!("fixtures/mainnet-v0-transactions.json");
    let v: Value = serde_json::from_str(raw).expect("fixture json");
    assert_eq!(v["network_passphrase"], MAINNET);
    assert_eq!(v["ledger_protocol"], 12);
    v["transactions"]
        .as_array()
        .expect("transactions array")
        .iter()
        .map(|t| {
            let env = TransactionEnvelope::from_xdr_base64(
                t["envelope_xdr"].as_str().expect("envelope"),
                Limits::none(),
            )
            .expect("decode envelope");
            let TransactionEnvelope::TxV0(e) = env else {
                panic!("fixture holds a non-v0 envelope");
            };
            (
                t["label"].as_str().expect("label").to_owned(),
                t["tx_hash"].as_str().expect("hash").to_owned(),
                e,
            )
        })
        .collect()
}

// the transaction source and every operation source, the keys that can sign
fn signer_candidates(tx: &TransactionV0) -> Vec<[u8; 32]> {
    let mut keys = vec![tx.source_account_ed25519.0];
    for op in tx.operations.iter() {
        match &op.source_account {
            Some(MuxedAccount::Ed25519(k)) => keys.push(k.0),
            Some(MuxedAccount::MuxedEd25519(_)) => panic!("muxed operation source before 13"),
            None => {}
        }
    }
    keys
}

#[test]
fn v0_hashes_match_the_network() {
    let nid = network_id(MAINNET);
    let txs = fixture();
    assert_eq!(txs.len(), 2);
    for (label, want, e) in &txs {
        let ours = transaction_v0_hash(&e.tx, &nid).expect("hash");
        assert_eq!(&hex::encode(ours), want, "{label}");
        let env = TransactionEnvelope::TxV0(e.clone());
        assert_eq!(envelope_hash(&env, &nid).expect("hash"), ours, "{label}");
        // stellar-xdr converts to v1 before hashing, core hashes the v0 bytes behind a prefix
        assert_eq!(env.hash(nid).expect("xdr hash"), ours, "{label}");
    }
}

#[test]
fn v0_signatures_verify() {
    for (label, want, e) in fixture() {
        let h: [u8; 32] = hex::decode(&want).unwrap().try_into().unwrap();
        let keys = signer_candidates(&e.tx);
        let mut verified = 0;
        for s in e.signatures.iter() {
            let k = keys
                .iter()
                .find(|k| hint(k) == s.hint)
                .unwrap_or_else(|| panic!("{label}: no candidate key for hint {:?}", s.hint));
            let vk = VerifyingKey::from_bytes(k).expect("valid key");
            let sig = Signature::from_slice(s.signature.as_slice()).expect("64 bytes");
            vk.verify_strict(&h, &sig)
                .unwrap_or_else(|err| panic!("{label}: signature does not verify: {err}"));
            verified += 1;
        }
        assert_eq!(verified, e.signatures.len(), "{label}");
        assert_ne!(
            transaction_v0_hash(&e.tx, &network_id("Test SDF Network ; September 2015")).unwrap(),
            h,
            "{label}: the hash must depend on the network"
        );
    }
}

// the v1 transaction core treats as equivalent: plain ed25519 source, time
// bounds as PRECOND_TIME, no ext
fn as_v1(tx: &TransactionV0) -> Transaction {
    Transaction {
        source_account: MuxedAccount::Ed25519(tx.source_account_ed25519.clone()),
        fee: tx.fee,
        seq_num: tx.seq_num.clone(),
        cond: match &tx.time_bounds {
            Some(tb) => Preconditions::Time(tb.clone()),
            None => Preconditions::None,
        },
        memo: tx.memo.clone(),
        operations: tx.operations.clone(),
        ext: TransactionExt::V0,
    }
}

#[test]
fn v0_and_v1_forms_share_a_hash_and_round_trip() {
    let nid = network_id(MAINNET);
    for (label, want, e) in fixture() {
        let v1 = as_v1(&e.tx);
        assert_eq!(
            hex::encode(transaction_hash(&v1, &nid).unwrap()),
            want,
            "{label}"
        );
        let back = to_v0(v1).expect("representable as v0");
        assert_eq!(back, e.tx, "{label}");
        assert_eq!(
            back.to_xdr(Limits::none()).unwrap(),
            e.tx.to_xdr(Limits::none()).unwrap(),
            "{label}"
        );
    }
}

#[test]
fn signing_picks_the_envelope_the_protocol_accepts() {
    let nid = network_id(MAINNET);
    let key = SigningKey::from_bytes(&[7u8; 32]);
    let (_, _, e) = fixture().into_iter().next().unwrap();
    let mut tx = as_v1(&e.tx);
    tx.source_account = MuxedAccount::Ed25519(Uint256(key.verifying_key().to_bytes()));
    let old = sign_for_protocol(tx.clone(), FIRST_V1_ENVELOPE_PROTOCOL - 1, &nid, &[&key]).unwrap();
    let new = sign_for_protocol(tx.clone(), FIRST_V1_ENVELOPE_PROTOCOL, &nid, &[&key]).unwrap();
    let TransactionEnvelope::TxV0(old_e) = &old else {
        panic!("protocol 12 must give a v0 envelope");
    };
    assert!(matches!(new, TransactionEnvelope::Tx(_)));
    // one transaction, one hash, one signature, whichever envelope carries it
    assert_eq!(
        envelope_hash(&old, &nid).unwrap(),
        envelope_hash(&new, &nid).unwrap()
    );
    let direct = sign_transaction_v0(to_v0(tx).unwrap(), &nid, &[&key]).unwrap();
    assert_eq!(&direct, old_e);
    let TransactionEnvelope::Tx(new_e) = &new else {
        unreachable!()
    };
    assert_eq!(new_e.signatures, old_e.signatures);
}

#[test]
fn v1_only_shapes_have_no_v0_form() {
    let (_, _, e) = fixture().into_iter().next().unwrap();
    let base = as_v1(&e.tx);
    let mut muxed = base.clone();
    muxed.source_account = MuxedAccount::MuxedEd25519(MuxedAccountMed25519 {
        id: 1,
        ed25519: e.tx.source_account_ed25519.clone(),
    });
    assert!(matches!(to_v0(muxed), Err(TxError::NotV0(_))));
    let mut v2 = base;
    v2.cond = Preconditions::V2(PreconditionsV2 {
        time_bounds: None,
        ledger_bounds: None,
        min_seq_num: Some(SequenceNumber(1)),
        min_seq_age: 0.into(),
        min_seq_ledger_gap: 0,
        extra_signers: Default::default(),
    });
    assert!(matches!(to_v0(v2), Err(TxError::NotV0(_))));
}
