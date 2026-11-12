//! What stellar-core 30.0.0 does on fresh standalone networks below protocol
//! 13: the driver must submit v0 envelopes there and find them in the meta by
//! hash, and protocol 1 is probed for ManageData, which core answers
//! MANAGE_DATA_NOT_SUPPORTED_YET for only before protocol 2. Needs docker and
//! `SHAPEKIT_DOCKER_TESTS=1`.

mod node_support;

use node_support::{TESTNET, config, enabled};
use shapekit_core_driver::chain::{ChainDriver, ChainError};
use shapekit_core_driver::keys::{account_id, derive_key, muxed_account, root_key};
use shapekit_core_driver::node::SubmitStatus;
use shapekit_core_driver::tx::{TransactionBuilder, sign_transaction};
use stellar_xdr::{
    CreateAccountOp, DataValue, LedgerCloseMeta, ManageDataOp, ManageDataResult, OperationBody,
    OperationResult, OperationResultTr, PaymentOp, String64, TransactionEnvelope,
    TransactionResultResult,
};

fn old_config(protocol: u32) -> shapekit_core_driver::config::NodeConfig {
    let mut cfg = config(TESTNET, protocol);
    cfg.soroban_high_limits = false;
    cfg.emit_classic_events = false;
    cfg.backfill_stellar_asset_events = false;
    cfg.diagnostic_events = false;
    cfg
}

#[test]
fn v0_envelopes_apply_below_protocol_13() {
    if !enabled("v0_envelopes_apply_below_protocol_13") {
        return;
    }
    for protocol in [11, 12] {
        let dir = tempfile::tempdir().unwrap();
        let mut chain = ChainDriver::start(old_config(protocol), dir.path()).expect("start");
        let root = root_key(TESTNET);
        let alice = derive_key("node-old-protocols", "alice");
        let env = chain
            .classic(
                &root,
                vec![OperationBody::CreateAccount(CreateAccountOp {
                    destination: account_id(&alice.verifying_key()),
                    starting_balance: 100_000_000_000,
                })],
            )
            .unwrap();
        assert!(
            matches!(env, TransactionEnvelope::TxV0(_)),
            "protocol {protocol}: classic must build a v0 envelope"
        );
        let applied = chain.apply_ok(&env).expect("v0 create account applies");
        assert!(matches!(chain.last_meta(), LedgerCloseMeta::V0(_)));

        // a v1 envelope is refused at submission with txNOT_SUPPORTED
        let seq = chain
            .node()
            .account_seq(&account_id(&alice.verifying_key()))
            .unwrap()
            + 1;
        let tx = TransactionBuilder::new(muxed_account(&alice.verifying_key()), seq)
            .operation(
                None,
                OperationBody::Payment(PaymentOp {
                    destination: muxed_account(&root.verifying_key()),
                    asset: stellar_xdr::Asset::Native,
                    amount: 1,
                }),
            )
            .build()
            .unwrap();
        let v1 =
            TransactionEnvelope::Tx(sign_transaction(tx, chain.network_id(), &[&alice]).unwrap());
        match chain.apply(&v1) {
            Err(ChainError::NotQueued { status, result }) => {
                assert_eq!(status, SubmitStatus::Error);
                let r = result.expect("core returns a result");
                assert!(
                    matches!(r.result, TransactionResultResult::TxNotSupported),
                    "protocol {protocol}: {r:?}"
                );
            }
            other => panic!("protocol {protocol}: v1 envelope was not refused: {other:?}"),
        }
        assert!(applied.ledger > 2);
        chain.finish(&dir.path().join("out")).unwrap();
    }
}

/// Upgrading a fresh network to protocol 1 and submitting ManageData there.
/// The answer is printed and asserted as core gives it.
#[test]
fn protocol_1_manage_data() {
    if !enabled("protocol_1_manage_data") {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut chain = ChainDriver::start(old_config(1), dir.path()).expect("start at protocol 1");
    let root = root_key(TESTNET);
    let env = chain
        .classic(
            &root,
            vec![OperationBody::ManageData(ManageDataOp {
                data_name: String64("k".try_into().unwrap()),
                data_value: Some(DataValue(b"v".to_vec().try_into().unwrap())),
            })],
        )
        .unwrap();
    let got = chain.apply(&env);
    eprintln!("protocol 1 manage data: {got:?}");
    match got {
        Err(ChainError::NotQueued { status, result }) => {
            assert_eq!(status, SubmitStatus::Error);
            let r = result.expect("core returns a result");
            let TransactionResultResult::TxFailed(ops) = &r.result else {
                panic!("expected txFAILED, got {r:?}");
            };
            assert!(matches!(
                ops[0],
                OperationResult::OpInner(OperationResultTr::ManageData(
                    ManageDataResult::NotSupportedYet
                ))
            ));
        }
        other => panic!("manage data at protocol 1 was not refused at submission: {other:?}"),
    }
    chain.finish(&dir.path().join("out")).unwrap();
}
