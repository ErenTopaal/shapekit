// temporary research probe: does core 30.0.0 apply classic payments on a fresh standalone
// network upgraded from 0 straight to an old protocol. not part of the tree.

use shapekit_core_driver::chain::ChainDriver;
use shapekit_core_driver::config::{CoreImage, NodeConfig};
use shapekit_core_driver::keys::{account_id, derive_key, muxed_account, root_key};
use shapekit_core_driver::tx::{sign_hash, transaction_hash};
use stellar_xdr::{
    Asset, CreateAccountOp, LedgerCloseMeta, MuxedAccount, OperationBody, PaymentOp,
    TransactionEnvelope, TransactionV0, TransactionV0Envelope, TransactionV0Ext, Uint256,
};

const TESTNET: &str = "Test SDF Network ; September 2015";

fn results(lcm: &LedgerCloseMeta) -> Vec<String> {
    match lcm {
        LedgerCloseMeta::V0(m) => m
            .tx_processing
            .iter()
            .map(|t| format!("{:?}", t.result.result.result))
            .collect(),
        LedgerCloseMeta::V1(m) => m
            .tx_processing
            .iter()
            .map(|t| format!("{:?}", t.result.result.result))
            .collect(),
        LedgerCloseMeta::V2(m) => m
            .tx_processing
            .iter()
            .map(|t| format!("{:?}", t.result.result.result))
            .collect(),
    }
}

fn header_version(lcm: &LedgerCloseMeta) -> (u32, u32, &'static str) {
    let (h, v) = match lcm {
        LedgerCloseMeta::V0(m) => (&m.ledger_header.header, "v0"),
        LedgerCloseMeta::V1(m) => (&m.ledger_header.header, "v1"),
        LedgerCloseMeta::V2(m) => (&m.ledger_header.header, "v2"),
    };
    (h.ledger_seq, h.ledger_version, v)
}

fn v0(env: &TransactionEnvelope, key: &ed25519_dalek::SigningKey, nid: &[u8; 32]) -> TransactionEnvelope {
    let TransactionEnvelope::Tx(e) = env else { panic!() };
    let tx = &e.tx;
    let hash = transaction_hash(tx, nid).unwrap();
    let MuxedAccount::Ed25519(src) = &tx.source_account else { panic!() };
    TransactionEnvelope::TxV0(TransactionV0Envelope {
        tx: TransactionV0 {
            source_account_ed25519: Uint256(src.0),
            fee: tx.fee,
            seq_num: tx.seq_num.clone(),
            time_bounds: None,
            memo: tx.memo.clone(),
            operations: tx.operations.clone(),
            ext: TransactionV0Ext::V0,
        },
        signatures: vec![sign_hash(key, &hash)].try_into().unwrap(),
    })
}

fn submit_close(chain: &mut ChainDriver, env: &TransactionEnvelope, label: &str) {
    match chain.node().submit(env) {
        Ok(s) => {
            println!("  {label}: submit {:?} {:?}", s.status, s.error.map(|e| e.result));
        }
        Err(e) => {
            println!("  {label}: submit error {e}");
        }
    }
    let seq = chain.close_empty().expect("close");
    let (lseq, ver, mv) = header_version(chain.last_meta());
    println!(
        "  {label}: closed {seq} (header seq {lseq} protocol {ver}, LedgerCloseMeta {mv}) results {:?}",
        results(chain.last_meta())
    );
}

fn main() {
    let protocol: u32 = std::env::args().nth(1).unwrap().parse().unwrap();
    let pins = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pins.toml");
    let cfg = NodeConfig {
        passphrase: TESTNET.to_owned(),
        image: CoreImage::from_pins(&pins).unwrap(),
        protocol,
        soroban_high_limits: false,
        emit_classic_events: false,
        backfill_stellar_asset_events: false,
        diagnostic_events: false,
        min_persistent_ttl: None,
    };
    let tmp = tempfile::tempdir().unwrap();
    let mut chain = ChainDriver::start(cfg, tmp.path()).expect("chain starts");
    let nid = *chain.network_id();
    let (seq, ver, mv) = header_version(chain.last_meta());
    println!("protocol {protocol}: started, ledger {seq} protocol {ver} meta {mv}");
    let root = root_key(TESTNET);
    let alice = derive_key("oldproto", "alice");
    let bob = derive_key("oldproto", "bob");

    // v1 envelope (ENVELOPE_TYPE_TX), which cap-0015 added at protocol 13
    let create = chain
        .classic(
            &root,
            vec![OperationBody::CreateAccount(CreateAccountOp {
                destination: account_id(&alice.verifying_key()),
                starting_balance: 1_000_000_000,
            })],
        )
        .unwrap();
    submit_close(&mut chain, &create, "v1 create_account");

    // the same operation in a v0 envelope
    let create_v0 = chain
        .classic(
            &root,
            vec![OperationBody::CreateAccount(CreateAccountOp {
                destination: account_id(&bob.verifying_key()),
                starting_balance: 1_000_000_000,
            })],
        )
        .unwrap();
    submit_close(&mut chain, &v0(&create_v0, &root, &nid), "v0 create_account");

    let pay = |to: &ed25519_dalek::SigningKey| {
        OperationBody::Payment(PaymentOp {
            destination: muxed_account(&to.verifying_key()),
            asset: Asset::Native,
            amount: 12_345,
        })
    };
    let p1 = chain.classic(&root, vec![pay(&alice)]).unwrap();
    submit_close(&mut chain, &p1, "v1 payment root->alice");
    let p2 = chain.classic(&root, vec![pay(&bob)]).unwrap();
    submit_close(&mut chain, &v0(&p2, &root, &nid), "v0 payment root->bob");
    let out = tmp.path().join("chain-out");
    match chain.finish(&out) {
        Ok(_) => println!("protocol {protocol}: chain finished and written"),
        Err(e) => println!("protocol {protocol}: finish error {e}"),
    }
}
