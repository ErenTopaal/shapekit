//! Command and query server calls against the pinned stellar-core image.
//! Needs docker and `SHAPEKIT_DOCKER_TESTS=1`.

mod node_support;

use node_support::{TESTNET, config, enabled};
use shapekit_core_driver::config::close_time;
use shapekit_core_driver::keys::{account_id, derive_key, muxed_account, network_id, root_key};
use shapekit_core_driver::meta::MetaStream;
use shapekit_core_driver::node::{CoreNode, EntryState, NodeError, SubmitStatus};
use shapekit_core_driver::tx::{TransactionBuilder, sign_transaction};
use stellar_xdr::{
    CreateAccountOp, Hash, LedgerEntryData, LedgerKey, LedgerKeyAccount, LedgerKeyTtl,
    OperationBody, TransactionEnvelope, TransactionResultResult,
};

fn account_key(id: stellar_xdr::AccountId) -> LedgerKey {
    LedgerKey::Account(LedgerKeyAccount { account_id: id })
}

#[test]
fn node_api_round_trip() {
    if !enabled("node_api_round_trip") {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let node = CoreNode::start(config(TESTNET, 30), tmp.path()).expect("core starts");

    let net = std::process::Command::new("docker")
        .args([
            "network",
            "inspect",
            "--format",
            "{{.Internal}}",
            node.network(),
        ])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&net.stdout).trim(), "true");
    // no route off the host from inside the container
    let out = std::process::Command::new("docker")
        .args([
            "exec",
            node.container(),
            "curl",
            "-sS",
            "-m",
            "5",
            "http://1.1.1.1/",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "container reached the internet");

    let info = node.info().unwrap();
    assert_eq!(info.state, "Synced!");
    assert_eq!(info.ledger.num, 1);
    assert_eq!(info.ledger.version, 0);
    assert_eq!(info.network, TESTNET);

    node.upgrade_to(30, 2, close_time(2)).unwrap();

    // a close time behind core's clock is refused with core's message
    match node.close(3, close_time(1)) {
        Err(NodeError::Rejected { message, .. }) => {
            assert!(message.contains("too early"), "{message}")
        }
        other => panic!("expected a rejection, got {other:?}"),
    }

    let root = root_key(TESTNET);
    let fresh = derive_key("node-api", "fresh");
    let entries = node
        .ledger_entries(&[
            account_key(account_id(&root.verifying_key())),
            account_key(account_id(&fresh.verifying_key())),
        ])
        .unwrap();
    assert_eq!(entries.ledger_seq, 2);
    assert_eq!(entries.entries[0].state, EntryState::Live);
    assert_eq!(entries.entries[1].state, EntryState::NotFound);
    let Some(LedgerEntryData::Account(acc)) =
        entries.entries[0].entry.as_ref().map(|e| e.data.clone())
    else {
        panic!("root entry is not an account");
    };

    let tx = TransactionBuilder::new(muxed_account(&root.verifying_key()), acc.seq_num.0 + 1)
        .operation(
            None,
            OperationBody::CreateAccount(CreateAccountOp {
                destination: account_id(&fresh.verifying_key()),
                starting_balance: 100_000_000_000,
            }),
        )
        .build()
        .unwrap();
    let env =
        TransactionEnvelope::Tx(sign_transaction(tx, &network_id(TESTNET), &[&root]).unwrap());
    let first = node.submit(&env).unwrap();
    assert_eq!(first.status, SubmitStatus::Pending, "{first:?}");
    assert_eq!(node.submit(&env).unwrap().status, SubmitStatus::Duplicate);

    // same sequence number again, with a different body: bad seq once queued
    let stale = TransactionBuilder::new(muxed_account(&root.verifying_key()), acc.seq_num.0)
        .operation(
            None,
            OperationBody::CreateAccount(CreateAccountOp {
                destination: account_id(&derive_key("node-api", "other").verifying_key()),
                starting_balance: 100_000_000_000,
            }),
        )
        .build()
        .unwrap();
    let stale =
        TransactionEnvelope::Tx(sign_transaction(stale, &network_id(TESTNET), &[&root]).unwrap());
    let rejected = node.submit(&stale).unwrap();
    assert_eq!(rejected.status, SubmitStatus::Error);
    let result = rejected.error.expect("error result");
    assert!(
        matches!(result.result, TransactionResultResult::TxBadSeq),
        "{result:?}"
    );

    // a blob far larger than any url buffer still reaches core's decoder
    let junk = "A".repeat(300_000);
    match node.submit_base64(&junk) {
        Err(NodeError::Rejected { .. }) => {}
        other => panic!("expected core to reject junk, got {other:?}"),
    }

    let mut stream = MetaStream::new(node.meta_path());
    node.close(3, close_time(3)).unwrap();
    let frames = stream
        .wait_for(3, std::time::Duration::from_secs(30))
        .unwrap();
    let last = frames.last().unwrap();
    assert_eq!(last.ledger_seq, 3);
    let stellar_xdr::LedgerCloseMeta::V2(m) = &last.meta else {
        panic!("not V2 meta");
    };
    assert_eq!(m.tx_processing.len(), 1);

    let after = node
        .ledger_entries(&[account_key(account_id(&fresh.verifying_key()))])
        .unwrap();
    assert_eq!(after.ledger_seq, 3);
    assert_eq!(after.entries[0].state, EntryState::Live);

    assert!(matches!(node.ledger_entries(&[]), Err(NodeError::NoKeys)));
    // core refuses ttl keys outright
    let ttl = LedgerKey::Ttl(LedgerKeyTtl {
        key_hash: Hash([7; 32]),
    });
    match node.ledger_entries(&[ttl]) {
        Err(NodeError::Rejected { message, .. }) => {
            assert_eq!(message, "http 404: TTL keys are not allowed")
        }
        other => panic!("expected a rejection, got {other:?}"),
    }
    node.stop().unwrap();
}
