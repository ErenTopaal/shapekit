//! Signing, hints, builders and key derivation.

use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};
use shapekit_core_driver::keys::{
    DERIVE_KEY_DOMAIN, account_address, account_id, account_strkey, derive_key, derive_seed,
    muxed_account, muxed_account_with_id, muxed_strkey, network_id, root_key, secret_strkey,
    signed_payload_signer, signed_payload_strkey,
};
use shapekit_core_driver::tx::{
    BASE_FEE, TransactionBuilder, TxError, account_signature, assemble_soroban,
    authorization_payload, envelope_hash, fee_bump, hint, push_signature, sign_authorization_entry,
    sign_fee_bump, sign_hash_x, sign_signed_payload, sign_transaction, signed_payload_hint,
    soroban_transaction_data, transaction_hash,
};
use stellar_xdr::{
    Asset, ContractId, Hash, HostFunction, InvokeContractArgs, InvokeHostFunctionOp,
    LedgerFootprint, Limits, Memo, OperationBody, PaymentOp, ReadXdr, ScAddress, ScSymbol, ScVal,
    SorobanAddressCredentials, SorobanAuthorizationEntry, SorobanAuthorizedFunction,
    SorobanAuthorizedInvocation, SorobanCredentials, SorobanResources, SorobanTransactionDataExt,
    TransactionEnvelope, TransactionExt, WriteXdr,
};

const TESTNET: &str = "Test SDF Network ; September 2015";
const MAINNET: &str = "Public Global Stellar Network ; September 2015";

fn verify(pk: &[u8; 32], msg: &[u8], sig: &[u8]) {
    VerifyingKey::from_bytes(pk)
        .expect("key")
        .verify_strict(msg, &Signature::from_slice(sig).expect("sig len"))
        .expect("signature verifies");
}

#[test]
fn root_keys_are_the_known_network_roots() {
    // the genesis root accounts of the two public networks
    assert_eq!(
        account_strkey(&root_key(TESTNET).verifying_key()),
        "GBRPYHIL2CI3FNQ4BXLFMNDLFJUNPU2HY3ZMFSHONUCEOASW7QC7OX2H"
    );
    assert_eq!(
        account_strkey(&root_key(MAINNET).verifying_key()),
        "GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN7"
    );
    assert_eq!(root_key(TESTNET).to_bytes(), network_id(TESTNET));
}

#[test]
fn derive_key_layout() {
    let mut want = Vec::new();
    want.extend_from_slice(b"shapekit/derive-key/v1");
    want.extend_from_slice(&5u32.to_be_bytes());
    want.extend_from_slice(b"cap40");
    want.extend_from_slice(&6u32.to_be_bytes());
    want.extend_from_slice(b"issuer");
    assert_eq!(DERIVE_KEY_DOMAIN.len(), 22);
    let seed: [u8; 32] = Sha256::digest(&want).into();
    assert_eq!(derive_seed("cap40", "issuer"), seed);
    assert_eq!(derive_key("cap40", "issuer").to_bytes(), seed);
    // the length prefix keeps split points apart
    assert_ne!(derive_seed("ab", "c"), derive_seed("a", "bc"));
    // stable across calls
    assert_eq!(
        derive_key("r", "x").to_bytes(),
        derive_key("r", "x").to_bytes()
    );
}

#[test]
fn strkeys_round_trip() {
    let k = derive_key("strkey", "a");
    let vk = k.verifying_key();
    let g = account_strkey(&vk);
    assert!(g.starts_with('G'));
    assert_eq!(
        stellar_strkey::ed25519::PublicKey::from_string(&g)
            .expect("g")
            .0,
        vk.to_bytes()
    );
    let m = muxed_strkey(&vk, u64::MAX);
    assert!(m.starts_with('M'));
    let s = secret_strkey(&k);
    assert!(s.starts_with('S'));
    assert_eq!(
        stellar_strkey::ed25519::PrivateKey::from_string(&s)
            .expect("s")
            .0,
        k.to_bytes()
    );
    let p = signed_payload_strkey(&vk, &[1, 2, 3]).expect("p");
    assert!(p.starts_with('P'));
    assert!(signed_payload_strkey(&vk, &[]).is_err());
    assert!(signed_payload_strkey(&vk, &[0; 65]).is_err());
    assert!(signed_payload_signer(&vk, &[0; 65]).is_err());
    assert!(signed_payload_signer(&vk, &[0; 64]).is_ok());
}

#[test]
fn signed_payload_hints() {
    let pk: [u8; 32] = core::array::from_fn(|i| i as u8 + 1);
    // last four key bytes are 29 30 31 32
    let tail = [29u8, 30, 31, 32];
    let cases: [(usize, [u8; 4]); 4] = [
        // one byte payload [0xaa] pads to aa 00 00 00
        (1, [0xaa, 0, 0, 0]),
        (4, [0xaa, 0xab, 0xac, 0xad]),
        (32, [0xc6, 0xc7, 0xc8, 0xc9]),
        (64, [0xe6, 0xe7, 0xe8, 0xe9]),
    ];
    for (len, payload_tail) in cases {
        let payload: Vec<u8> = (0..len).map(|i| 0xaa + i as u8).collect();
        let want: [u8; 4] = core::array::from_fn(|i| tail[i] ^ payload_tail[i]);
        assert_eq!(
            signed_payload_hint(&pk, &payload).0,
            want,
            "payload of {len} bytes"
        );
    }
    // plain hint is the key's last four bytes
    assert_eq!(hint(&pk).0, tail);
}

#[test]
fn signed_payload_signature_is_not_network_bound() {
    let key = derive_key("cap40", "signer");
    let payload: Vec<u8> = (0u8..32).collect();
    let a = sign_signed_payload(&key, &payload).expect("sign");
    let b = sign_signed_payload(&key, &payload).expect("sign");
    assert_eq!(a, b);
    verify(
        &key.verifying_key().to_bytes(),
        &payload,
        a.signature.as_slice(),
    );
    assert_eq!(
        a.hint,
        signed_payload_hint(&key.verifying_key().to_bytes(), &payload)
    );

    // the same transaction signed under both passphrases differs, the
    // signed-payload signature attached to it does not
    let src = derive_key("cap40", "source");
    let tx = TransactionBuilder::new(muxed_account(&src.verifying_key()), 1)
        .operation(
            None,
            OperationBody::Payment(PaymentOp {
                destination: muxed_account(&key.verifying_key()),
                asset: Asset::Native,
                amount: 1,
            }),
        )
        .build()
        .expect("build");
    let mut sigs = Vec::new();
    for passphrase in [TESTNET, MAINNET] {
        let mut env = TransactionEnvelope::Tx(
            sign_transaction(tx.clone(), &network_id(passphrase), &[&src]).expect("sign"),
        );
        push_signature(&mut env, sign_signed_payload(&key, &payload).expect("sp")).expect("push");
        let TransactionEnvelope::Tx(e) = env else {
            unreachable!()
        };
        sigs.push(e.signatures.to_vec());
    }
    assert_ne!(
        sigs[0][0], sigs[1][0],
        "tx signature must depend on the passphrase"
    );
    assert_eq!(sigs[0][1], sigs[1][1], "signed-payload signature must not");
    assert!(matches!(
        sign_signed_payload(&key, &[0; 65]),
        Err(TxError::PayloadTooLong(65))
    ));
}

#[test]
fn hash_x_signature() {
    let preimage = b"shapekit hash-x preimage";
    let sig = sign_hash_x(preimage).expect("hash x");
    assert_eq!(sig.signature.as_slice(), preimage);
    let d: [u8; 32] = Sha256::digest(preimage).into();
    assert_eq!(sig.hint.0, d[28..32]);
}

#[test]
fn transaction_build_sign_and_round_trip() {
    let nid = network_id(TESTNET);
    let src = derive_key("build", "source");
    let dst = derive_key("build", "dest");
    let tx = TransactionBuilder::new(muxed_account_with_id(&src.verifying_key(), 7), 42)
        .memo(Memo::Text("hi".try_into().expect("memo")))
        .operation(
            None,
            OperationBody::Payment(PaymentOp {
                destination: muxed_account(&dst.verifying_key()),
                asset: Asset::Native,
                amount: 10_000_000,
            }),
        )
        .build()
        .expect("build");
    assert_eq!(tx.fee, BASE_FEE);
    let env =
        TransactionEnvelope::Tx(sign_transaction(tx.clone(), &nid, &[&src, &dst]).expect("sign"));
    let bytes = env.to_xdr(Limits::none()).expect("encode");
    let back = TransactionEnvelope::from_xdr(&bytes, Limits::none()).expect("decode");
    assert_eq!(back, env);

    let h = transaction_hash(&tx, &nid).expect("hash");
    assert_eq!(env.hash(nid).expect("xdr hash"), h);
    assert_eq!(envelope_hash(&env, &nid).expect("hash"), h);
    let TransactionEnvelope::Tx(e) = &env else {
        unreachable!()
    };
    assert_eq!(e.signatures.len(), 2);
    for (s, k) in e.signatures.iter().zip([&src, &dst]) {
        let pk = k.verifying_key().to_bytes();
        assert_eq!(s.hint.0, pk[28..32]);
        verify(&pk, &h, s.signature.as_slice());
    }
}

#[test]
fn fee_bump_build_and_sign() {
    let nid = network_id(MAINNET);
    let src = derive_key("feebump", "inner");
    let payer = derive_key("feebump", "payer");
    let tx = TransactionBuilder::new(muxed_account(&src.verifying_key()), 9)
        .operation(
            None,
            OperationBody::Payment(PaymentOp {
                destination: muxed_account(&payer.verifying_key()),
                asset: Asset::Native,
                amount: 5,
            }),
        )
        .build()
        .expect("build");
    let inner = sign_transaction(tx, &nid, &[&src]).expect("inner");
    let fb = fee_bump(inner, muxed_account_with_id(&payer.verifying_key(), 3), 400);
    let env = TransactionEnvelope::TxFeeBump(sign_fee_bump(fb, &nid, &[&payer]).expect("sign"));
    let h = envelope_hash(&env, &nid).expect("hash");
    assert_eq!(env.hash(nid).expect("xdr hash"), h);
    let TransactionEnvelope::TxFeeBump(e) = &env else {
        unreachable!()
    };
    verify(
        &payer.verifying_key().to_bytes(),
        &h,
        e.signatures[0].signature.as_slice(),
    );
}

fn invoke_entry(signer: &ScAddress, v2: bool) -> SorobanAuthorizationEntry {
    let creds = SorobanAddressCredentials {
        address: signer.clone(),
        nonce: 1234,
        signature_expiration_ledger: 0,
        signature: ScVal::Void,
    };
    SorobanAuthorizationEntry {
        credentials: if v2 {
            SorobanCredentials::AddressV2(creds)
        } else {
            SorobanCredentials::Address(creds)
        },
        root_invocation: SorobanAuthorizedInvocation {
            function: SorobanAuthorizedFunction::ContractFn(InvokeContractArgs {
                contract_address: ScAddress::Contract(ContractId(Hash([7; 32]))),
                function_name: ScSymbol("node".try_into().expect("sym")),
                args: vec![ScVal::U32(1)].try_into().expect("args"),
            }),
            sub_invocations: Default::default(),
        },
    }
}

#[test]
fn auth_entry_signing() {
    let nid = network_id(TESTNET);
    let a = derive_key("auth", "a");
    let b = derive_key("auth", "b");
    let addr = account_address(&a.verifying_key());
    for v2 in [false, true] {
        let signed =
            sign_authorization_entry(invoke_entry(&addr, v2), &nid, 500, &[&a, &b]).expect("sign");
        let payload = authorization_payload(&signed, &nid).expect("payload");
        let (SorobanCredentials::Address(c) | SorobanCredentials::AddressV2(c)) =
            &signed.credentials
        else {
            panic!("credentials arm changed");
        };
        assert_eq!(c.signature_expiration_ledger, 500);
        // the signature value equals the canonical builder output and every
        // inner signature verifies, keys ascending
        assert_eq!(
            c.signature,
            account_signature(&payload, &[&b, &a]).expect("sig")
        );
        let ScVal::Vec(Some(items)) = &c.signature else {
            panic!("not a vec")
        };
        let mut last = [0u8; 32];
        for item in items.iter() {
            let ScVal::Map(Some(m)) = item else {
                panic!("not a map")
            };
            assert_eq!(
                m[0].key,
                ScVal::Symbol("public_key".try_into().expect("sym"))
            );
            assert_eq!(
                m[1].key,
                ScVal::Symbol("signature".try_into().expect("sym"))
            );
            let (ScVal::Bytes(pk), ScVal::Bytes(sig)) = (&m[0].val, &m[1].val) else {
                panic!("not bytes")
            };
            let pk: [u8; 32] = pk.as_slice().try_into().expect("32");
            assert!(pk > last);
            last = pk;
            verify(&pk, &payload, sig.as_slice());
        }
    }
    // v1 and v2 payloads differ because v2 binds the address
    let p1 = authorization_payload(&invoke_entry(&addr, false), &nid).expect("p1");
    let p2 = authorization_payload(&invoke_entry(&addr, true), &nid).expect("p2");
    assert_ne!(p1, p2);

    let contract = ScAddress::Contract(ContractId(Hash([1; 32])));
    assert!(matches!(
        sign_authorization_entry(invoke_entry(&contract, false), &nid, 1, &[&a]),
        Err(TxError::NotAnAccountAddress)
    ));
    let source_entry = SorobanAuthorizationEntry {
        credentials: SorobanCredentials::SourceAccount,
        ..invoke_entry(&addr, false)
    };
    assert!(matches!(
        sign_authorization_entry(source_entry, &nid, 1, &[&a]),
        Err(TxError::SourceAccountCredentials)
    ));
}

#[test]
fn soroban_assembly() {
    let src = derive_key("assemble", "source");
    let addr = account_address(&src.verifying_key());
    let op = InvokeHostFunctionOp {
        host_function: HostFunction::InvokeContract(InvokeContractArgs {
            contract_address: ScAddress::Contract(ContractId(Hash([7; 32]))),
            function_name: ScSymbol("node".try_into().expect("sym")),
            args: Default::default(),
        }),
        auth: Default::default(),
    };
    let tx = TransactionBuilder::new(muxed_account(&src.verifying_key()), 2)
        .operation(None, OperationBody::InvokeHostFunction(op))
        .build()
        .expect("build");
    let resources = SorobanResources {
        footprint: LedgerFootprint {
            read_only: Default::default(),
            read_write: Default::default(),
        },
        instructions: 1000,
        disk_read_bytes: 0,
        write_bytes: 0,
    };
    let data = soroban_transaction_data(resources.clone(), 12_345, &[]).expect("data");
    assert_eq!(data.ext, SorobanTransactionDataExt::V0);
    let with_archived = soroban_transaction_data(resources, 1, &[0, 2]).expect("data");
    assert!(matches!(
        with_archived.ext,
        SorobanTransactionDataExt::V1(_)
    ));

    let auth = vec![invoke_entry(&addr, false)];
    let assembled =
        assemble_soroban(tx.clone(), data.clone(), Some(auth.clone())).expect("assemble");
    assert_eq!(assembled.fee, BASE_FEE + 12_345);
    assert_eq!(assembled.ext, TransactionExt::V1(data.clone()));
    let OperationBody::InvokeHostFunction(op) = &assembled.operations[0].body else {
        panic!("op changed")
    };
    assert_eq!(op.auth.to_vec(), auth);

    let mut huge = data;
    huge.resource_fee = i64::from(u32::MAX);
    assert!(matches!(
        assemble_soroban(tx, huge, None),
        Err(TxError::FeeOverflow { .. })
    ));
    assert_eq!(addr, ScAddress::Account(account_id(&src.verifying_key())));
}
