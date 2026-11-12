//! Known answers from real testnet transactions: hashes recomputed from the
//! envelope must equal the hash the network reported, and the signatures in
//! the envelope must verify against our payloads. Fixture provenance is in
//! `fixtures/testnet-transactions.provenance.json`.

use ed25519_dalek::{Signature, VerifyingKey};
use serde_json::Value;
use shapekit_core_driver::keys::network_id;
use shapekit_core_driver::tx::{
    authorization_payload, envelope_hash, fee_bump_hash, hint, transaction_hash,
};
use stellar_xdr::{
    DecoratedSignature, FeeBumpTransactionInnerTx, Limits, MuxedAccount, OperationBody, ReadXdr,
    ScAddress, ScVal, SorobanCredentials, Transaction, TransactionEnvelope,
};

const TESTNET: &str = "Test SDF Network ; September 2015";

fn fixture() -> Vec<(String, String, TransactionEnvelope)> {
    let raw = include_str!("fixtures/testnet-transactions.json");
    let v: Value = serde_json::from_str(raw).expect("fixture json");
    assert_eq!(v["network_passphrase"], TESTNET);
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
            (
                t["label"].as_str().expect("label").to_owned(),
                t["tx_hash"].as_str().expect("hash").to_owned(),
                env,
            )
        })
        .collect()
}

fn ed25519_of(m: &MuxedAccount) -> [u8; 32] {
    match m {
        MuxedAccount::Ed25519(k) => k.0,
        MuxedAccount::MuxedEd25519(m) => m.ed25519.0,
    }
}

// every ed25519 key the transaction names as a source, the candidates for its
// signatures
fn source_keys(tx: &Transaction) -> Vec<[u8; 32]> {
    let mut keys = vec![ed25519_of(&tx.source_account)];
    for op in tx.operations.iter() {
        if let Some(s) = &op.source_account {
            keys.push(ed25519_of(s));
        }
    }
    keys
}

// returns how many signatures verified against a key with a matching hint;
// a signature whose hint matches but fails to verify is a test failure
fn verify_signatures(sigs: &[DecoratedSignature], keys: &[[u8; 32]], hash: &[u8; 32]) -> usize {
    let mut ok = 0;
    for s in sigs {
        for k in keys {
            if hint(k) != s.hint {
                continue;
            }
            let vk = VerifyingKey::from_bytes(k).expect("valid key");
            let sig = Signature::from_slice(s.signature.as_slice()).expect("64 byte signature");
            vk.verify_strict(hash, &sig)
                .expect("signature verifies over our hash");
            ok += 1;
        }
    }
    ok
}

#[test]
fn hashes_match_the_network() {
    let nid = network_id(TESTNET);
    let txs = fixture();
    assert_eq!(txs.len(), 4);
    for (label, want, env) in &txs {
        let ours = envelope_hash(env, &nid).expect("hash");
        assert_eq!(&hex::encode(ours), want, "{label}");
        // stellar-xdr has its own implementation; both must agree
        assert_eq!(env.hash(nid).expect("xdr hash"), ours, "{label}");
    }
}

#[test]
fn hash_depends_on_passphrase() {
    let mainnet = network_id("Public Global Stellar Network ; September 2015");
    for (label, want, env) in fixture() {
        let other = envelope_hash(&env, &mainnet).expect("hash");
        assert_ne!(hex::encode(other), want, "{label}");
    }
}

#[test]
fn envelope_signatures_verify() {
    let nid = network_id(TESTNET);
    for (label, _, env) in fixture() {
        match &env {
            TransactionEnvelope::Tx(e) => {
                let h = transaction_hash(&e.tx, &nid).expect("hash");
                let n = verify_signatures(e.signatures.as_slice(), &source_keys(&e.tx), &h);
                assert!(n >= 1, "{label}: no source signature verified");
            }
            TransactionEnvelope::TxFeeBump(e) => {
                let h = fee_bump_hash(&e.tx, &nid).expect("hash");
                let n =
                    verify_signatures(e.signatures.as_slice(), &[ed25519_of(&e.tx.fee_source)], &h);
                assert!(n >= 1, "{label}: fee source signature did not verify");
                let FeeBumpTransactionInnerTx::Tx(inner) = &e.tx.inner_tx;
                let ih = transaction_hash(&inner.tx, &nid).expect("inner hash");
                let n =
                    verify_signatures(inner.signatures.as_slice(), &source_keys(&inner.tx), &ih);
                assert!(n >= 1, "{label}: inner signature did not verify");
            }
            TransactionEnvelope::TxV0(_) => panic!("{label}: v0 envelope in fixture"),
        }
    }
}

#[test]
fn classic_fixture_has_no_soroban_operation() {
    let (_, _, env) = fixture()
        .into_iter()
        .find(|(l, _, _)| l == "classic")
        .expect("classic fixture");
    let TransactionEnvelope::Tx(e) = env else {
        panic!("classic is not a v1 envelope");
    };
    assert!(
        e.tx.operations
            .iter()
            .all(|op| !matches!(op.body, OperationBody::InvokeHostFunction(_)))
    );
}

#[test]
fn soroban_address_auth_signatures_verify() {
    check_auth("soroban_auth_address", false);
}

#[test]
fn soroban_address_v2_auth_signatures_verify() {
    check_auth("soroban_auth_address_v2", true);
}

// verifies every account signature in the transaction's auth entries over
// the payload we compute, and that the expected credential arm was present
fn check_auth(label: &str, v2: bool) {
    let nid = network_id(TESTNET);
    let (_, _, env) = fixture()
        .into_iter()
        .find(|(l, _, _)| l == label)
        .expect("fixture");
    let TransactionEnvelope::Tx(e) = env else {
        panic!("{label} is not a v1 envelope");
    };
    let mut verified = 0;
    for op in e.tx.operations.iter() {
        let OperationBody::InvokeHostFunction(ih) = &op.body else {
            continue;
        };
        for entry in ih.auth.iter() {
            let creds = match (&entry.credentials, v2) {
                (SorobanCredentials::Address(c), false)
                | (SorobanCredentials::AddressV2(c), true) => c,
                _ => continue,
            };
            let ScAddress::Account(account) = &creds.address else {
                continue;
            };
            let stellar_xdr::PublicKey::PublicKeyTypeEd25519(account_key) = &account.0;
            let payload = authorization_payload(entry, &nid).expect("payload");
            let mainnet = network_id("Public Global Stellar Network ; September 2015");
            assert_ne!(
                payload,
                authorization_payload(entry, &mainnet).expect("payload")
            );
            let ScVal::Vec(Some(items)) = &creds.signature else {
                panic!("account signature is not a vec: {:?}", creds.signature);
            };
            for item in items.iter() {
                let ScVal::Map(Some(m)) = item else {
                    panic!("signature item is not a map");
                };
                assert_eq!(m.len(), 2);
                let field = |name: &str| -> Vec<u8> {
                    let e = m
                        .iter()
                        .find(|e| e.key == ScVal::Symbol(name.try_into().expect("symbol")))
                        .unwrap_or_else(|| panic!("missing {name}"));
                    match &e.val {
                        ScVal::Bytes(b) => b.to_vec(),
                        other => panic!("{name} is not bytes: {other:?}"),
                    }
                };
                let pk: [u8; 32] = field("public_key").try_into().expect("32 byte key");
                let sig = Signature::from_slice(&field("signature")).expect("64 byte signature");
                VerifyingKey::from_bytes(&pk)
                    .expect("valid key")
                    .verify_strict(&payload, &sig)
                    .expect("auth signature verifies over our payload");
                if pk == account_key.0 {
                    verified += 1;
                }
            }
        }
    }
    assert!(verified >= 1, "{label}: no account auth signature found");
}
