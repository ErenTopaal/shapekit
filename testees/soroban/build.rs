//! Reads the soroban-sdk and soroban-env-host versions this build links from the lock file
//! next to the version's manifest, so hello reports what cargo resolved and not a constant.
//! Every build step runs `cargo build --locked`, so the lock file is what is linked.

use std::path::Path;

fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let lock = Path::new(&dir).join("Cargo.lock");
    println!("cargo:rerun-if-changed={}", lock.display());
    let text = std::fs::read_to_string(&lock)
        .unwrap_or_else(|e| panic!("reading {}: {e}", lock.display()));
    for (name, var) in [
        ("soroban-sdk", "SHAPEKIT_SOROBAN_SDK_VERSION"),
        ("soroban-env-host", "SHAPEKIT_SOROBAN_ENV_HOST_VERSION"),
        ("stellar-xdr", "SHAPEKIT_STELLAR_XDR_VERSION"),
    ] {
        let versions = locked_versions(&text, name);
        assert!(
            versions.len() == 1,
            "{} locks {} versions of {name}: {versions:?}",
            lock.display(),
            versions.len()
        );
        println!("cargo:rustc-env={var}={}", versions[0]);
    }
}

fn locked_versions(lock: &str, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut lines = lock.lines();
    while let Some(line) = lines.next() {
        if line.trim() == format!("name = \"{name}\"")
            && let Some(v) = lines.next()
            && let Some(v) = v.trim().strip_prefix("version = \"")
            && let Some(v) = v.strip_suffix('"')
        {
            out.push(v.to_string());
        }
    }
    out
}
