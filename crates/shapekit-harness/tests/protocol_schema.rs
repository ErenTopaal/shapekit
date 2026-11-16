//! schema/adapter-protocol.schema.json and the generated part of docs/adapter-interface.md
//! are current, and the messages the harness builds follow the schema.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use shapekit_harness::protocol::{
    Command, Delivery, DeliveryMode, Lake, LedgerWindow, Limits, Request, StartParams,
    sync_generated,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn validator() -> jsonschema::Validator {
    let text = std::fs::read_to_string(root().join("schema/adapter-protocol.schema.json")).unwrap();
    jsonschema::validator_for(&serde_json::from_str(&text).unwrap()).unwrap()
}

fn assert_valid(v: &jsonschema::Validator, instance: &Value) {
    let errors: Vec<String> = v.iter_errors(instance).map(|e| e.to_string()).collect();
    assert!(
        errors.is_empty(),
        "{instance} breaks the schema: {errors:?}"
    );
}

#[test]
fn generated_files_are_current() {
    let stale = sync_generated(&root(), false).unwrap();
    assert!(
        stale.is_empty(),
        "stale: {stale:?}; run `shapekit harness schema --write`"
    );
}

#[test]
fn harness_requests_follow_the_schema() {
    let v = validator();
    let start = StartParams {
        chain: "cap40.signed-payload-signer.p28".into(),
        network: "testnet".into(),
        network_passphrase: "Test SDF Network ; September 2015".into(),
        protocol: 28,
        delivery: Delivery {
            mode: DeliveryMode::Rpc,
            rpc_url: "http://192.168.64.1:41000/rpc".into(),
            archive_url: "http://192.168.64.1:41000/archive".into(),
            lake: Lake {
                url: "http://192.168.64.1:41000/ledgers".into(),
                path: "/corpus/chains/x/testnet/ledgers".into(),
                ledgers_per_batch: 1,
                batches_per_partition: 64000,
            },
        },
        ledgers: LedgerWindow {
            bootstrap: 63,
            last: 80,
            checkpoint_frequency: 64,
        },
        limits: Limits {
            memory_bytes: 2 << 30,
        },
        docker_network: Some("shapekit-harness-1-0".into()),
        workdir: "/tmp/shapekit-harness-x".into(),
    };
    for (i, cmd) in [
        Command::Describe { protocol: 2 },
        Command::Describe { protocol: 1 },
        Command::Start(Box::new(start)),
        Command::Advance { head: 64 },
        Command::Status,
        Command::Export { from: 64, to: 80 },
        Command::Dump { tables: Vec::new() },
        Command::Dump {
            tables: vec!["state_changes".into()],
        },
        Command::Stop,
    ]
    .into_iter()
    .enumerate()
    {
        let req = serde_json::to_value(Request {
            id: i.to_string(),
            command: cmd,
        })
        .unwrap();
        assert_valid(&v, &req);
    }
}

#[test]
fn documented_responses_follow_the_schema_and_parse() {
    let v = validator();
    let lines = [
        json!({"id": "0", "status": "ok", "result": {
            "protocol": 1, "adapter": "a", "adapter_version": "1", "target": "wallet-backend",
            "target_version": "e967ce8b7362b79c277e700e789181d76fbb5967",
            "delivery_modes": ["rpc", "sep54-s3"], "default_delivery": "rpc",
            "bootstrap": "checkpoint", "vocab_version": 2, "protocols": {"min": 20, "max": 30},
            "claims": {"participants": {"address_kinds": ["account"], "mux_ids": true},
                       "changes": [{"kind": "token-event", "where": {"event": ["transfer"]},
                                    "fields": ["event", "from", "to", "amount"]}]}}}),
        json!({"id": "3", "status": "ok", "result": {"last_ingested_ledger": 65, "health": "running", "last_error": null, "peak_memory_bytes": 1000}}),
        json!({"id": "4", "status": "record", "record": {"type": "transaction", "ledger": 65,
            "hash": "6f9d6a82579a63c2b02baabbc76d51d4f382133a8a83feb2fd44d95ddd871884",
            "participants": [{"address": null, "raw": {"column": "transactions_accounts.account_id", "hex": "60aa", "error": "too short"}}],
            "changes": [{"kind": "signer-added", "account": "GCCLUSWMBODKXUESRSM35CX545JJAHX2CFYCCN63RSQY5RRHEJCR4DC2", "signer": null, "weight": 1,
                         "raw": {"signer": {"column": "state_changes.signer_account_id", "hex": "78aa", "error": "too short"}},
                         "source": {"table": "state_changes", "category": "SIGNER", "reason": "ADD"}}],
            "unmapped": [{"table": "state_changes", "category": "X", "row": {"to_id": 1}, "note": "no kind"}]}}),
        json!({"id": "4", "status": "record", "record": {"type": "unattributed", "unmapped": []}}),
        json!({"id": "4", "status": "ok", "result": {"records": 2}}),
        json!({"id": "5", "status": "error", "error": {"class": "target", "message": "db down"}}),
        json!({"id": "0", "status": "ok", "result": {
            "protocol": 2, "adapter": "a", "adapter_version": "1", "target": "wallet-backend",
            "target_version": "e967ce8b7362b79c277e700e789181d76fbb5967",
            "delivery_modes": ["rpc"], "default_delivery": "rpc",
            "bootstrap": "checkpoint", "vocab_version": 2, "protocols": {"min": 1, "max": 30},
            "claims": {"changes": []}, "capabilities": ["dump"]}}),
        json!({"id": "6", "status": "table", "table": {"name": "transactions_accounts",
            "columns": [{"name": "tx_to_id", "type": "bigint"}, {"name": "account_id", "type": "bytea"}]}}),
        json!({"id": "6", "status": "row", "row": {"table": "transactions_accounts",
            "cells": ["279172878336", "30aa"]}}),
        json!({"id": "6", "status": "row", "row": {"table": "transactions_accounts",
            "cells": ["279172878337", null]}}),
        json!({"id": "6", "status": "ok", "result": {"tables": 1, "rows": 2}}),
    ];
    for l in &lines {
        assert_valid(&v, l);
        let _: shapekit_harness::protocol::Response = serde_json::from_value(l.clone()).unwrap();
    }
    let bad = json!({"id": "4", "status": "record", "record": {"type": "transaction", "ledger": 1,
        "hash": "00", "participants": [], "changes": [{"kind": "not-a-kind"}]}});
    assert!(!v.is_valid(&bad));
}

#[test]
fn the_wallet_backend_reproductions_table_loads_and_names_real_entries() {
    let table = shapekit_harness::reproduce::load(
        &root().join("adapters/walletbackend/reproductions.toml"),
    )
    .unwrap();
    let manifest = shapekit_manifest::build::load(&root().join("corpus/manifest.json")).unwrap();
    for r in &table.reproduction {
        for c in &r.case {
            for e in &c.expect {
                let entry = manifest
                    .entries
                    .iter()
                    .find(|m| m.id == e.entry)
                    .unwrap_or_else(|| panic!("{}: no entry {}", r.id, e.entry));
                assert_eq!(
                    entry.source.chain.as_deref(),
                    Some(r.chain.as_str()),
                    "{}",
                    e.entry
                );
            }
        }
    }
}
