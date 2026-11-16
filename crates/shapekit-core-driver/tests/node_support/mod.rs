//! Shared setup for the docker-backed node tests.

// each test binary uses a different subset of these helpers
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use shapekit_core_driver::config::{CoreImage, NodeConfig};

pub const TESTNET: &str = "Test SDF Network ; September 2015";
pub const MAINNET: &str = "Public Global Stellar Network ; September 2015";

pub fn pins_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pins.toml")
}

pub fn config(passphrase: &str, protocol: u32) -> NodeConfig {
    NodeConfig {
        passphrase: passphrase.to_owned(),
        image: CoreImage::from_pins(&pins_path()).expect("core image pin"),
        protocol,
        soroban_high_limits: true,
        emit_classic_events: true,
        backfill_stellar_asset_events: true,
        diagnostic_events: true,
        min_persistent_ttl: None,
        invariant_checks: false,
        emit_ledger_close_meta_ext_v1: false,
        limits: Default::default(),
    }
}

/// True when docker still knows an object by this name.
pub fn docker_object_exists(kind: &str, name: &str) -> bool {
    std::process::Command::new("docker")
        .args([kind, "inspect", name])
        .output()
        .expect("docker runs")
        .status
        .success()
}
