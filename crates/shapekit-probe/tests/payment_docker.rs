//! The eleven single-field changes of one payment against the pinned core.

use std::path::Path;

use shapekit_contracts::validation::{ApplyStatus, SubmitStatus};
use shapekit_core_driver::config::CoreImage;
use shapekit_core_driver::smoke::{self, TESTNET_PASSPHRASE};
use shapekit_probe::{Probe, variants};

#[test]
#[ignore = "needs docker and the pinned stellar-core image; run with --include-ignored"]
fn payment_variants_get_core_s_known_answers() {
    let pins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pins.toml");
    let mut cfg = smoke::node_config(TESTNET_PASSPHRASE, CoreImage::from_pins(&pins).unwrap());
    cfg.soroban_high_limits = false;
    cfg.invariant_checks = true;
    let work = tempfile::tempdir().unwrap();
    let all = variants::payment();
    let mut probe = Probe::start(cfg, work.path(), all.len()).unwrap();
    let got: Vec<(String, SubmitStatus, Option<String>, ApplyStatus)> = all
        .iter()
        .map(|v| {
            let (a, _) = probe.attempt(v).unwrap();
            (a.variant, a.submission, a.result_code, a.applied)
        })
        .collect();
    let ok = |n: &str| {
        (
            n.to_owned(),
            SubmitStatus::Pending,
            Some("tx_success".to_owned()),
            ApplyStatus::Succeeded,
        )
    };
    let refused = |n: &str, code: &str| {
        (
            n.to_owned(),
            SubmitStatus::Error,
            Some(code.to_owned()),
            ApplyStatus::NotApplied,
        )
    };
    assert_eq!(
        got,
        vec![
            ok("base payment, no memo"),
            ok("memo id 2^64-1"),
            ok("memo hash of 32 x 0xff"),
            ok("memo return of 32 x 0x00"),
            ok("muxed destination id 0"),
            ok("muxed destination id 2^64-1"),
            ok("muxed source id 2^63"),
            refused("sequence number one too high", "tx_bad_seq"),
            refused("fee 99 stroops", "tx_insufficient_fee"),
            refused("max time bound in the past", "tx_too_late"),
            refused("signed by another key", "tx_bad_auth"),
        ]
    );
}
