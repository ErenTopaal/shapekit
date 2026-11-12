//! Shared setup for the docker-backed node tests.

// each test binary uses a different subset of these helpers
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use shapekit_core_driver::config::{CoreImage, NodeConfig};

pub const TESTNET: &str = "Test SDF Network ; September 2015";
pub const MAINNET: &str = "Public Global Stellar Network ; September 2015";

/// The tests need docker and the pinned core image, so they only run when
/// asked to. Returning early keeps `cargo test` usable on machines without
/// docker; the skip is printed so it never passes unnoticed.
pub fn enabled(test: &str) -> bool {
    let on = std::env::var("SHAPEKIT_DOCKER_TESTS").as_deref() == Ok("1");
    if !on {
        eprintln!("{test}: skipped, set SHAPEKIT_DOCKER_TESTS=1 to run against docker");
    }
    on
}

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
