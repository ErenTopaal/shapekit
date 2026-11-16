//! The in-process host replays committed corpus chains exactly as core
//! recorded them.

use std::path::{Path, PathBuf};

use shapekit_core_driver::keys::network_id;
use shapekit_core_driver::meta::read_all;
use shapekit_twin::host_cov::{CoreResult, Verdict, replay};

fn chain(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/chains")
        .join(name)
        .join("testnet")
}

fn verdicts(name: &str) -> Vec<(CoreResult, Verdict)> {
    let lcms: Vec<_> = read_all(&chain(name).join("ledgers.xdr"))
        .unwrap()
        .into_iter()
        .map(|f| f.meta)
        .collect();
    replay(&lcms, network_id("Test SDF Network ; September 2015"))
        .unwrap()
        .into_iter()
        .map(|c| (c.core, c.verdict))
        .collect()
}

#[test]
fn sac_trust_chain_is_verified_including_classic_writes() {
    // the stellar asset contract writes accounts and trustlines, whose state
    // depends on the transaction's own sequence bump before the call
    let v = verdicts("cap73.sac-trust.p30");
    assert_eq!(v.len(), 8);
    assert!(v.iter().all(|(_, v)| *v == Verdict::Verified), "{v:?}");
}

#[test]
fn a_failure_after_a_successful_call_is_reported_unverified() {
    let v = verdicts("soroban.invoke-basics.p30");
    let unverified: Vec<_> = v
        .iter()
        .filter(|(_, v)| matches!(v, Verdict::Unverified(_)))
        .collect();
    assert_eq!(unverified.len(), 1);
    assert_eq!(unverified[0].0, CoreResult::ResourceLimitExceeded);
    assert_eq!(
        v.iter().filter(|(_, v)| *v == Verdict::Verified).count(),
        v.len() - 1
    );
}

#[test]
fn restores_are_skipped_not_guessed() {
    let v = verdicts("cap66.archival.p30");
    let skipped = v
        .iter()
        .filter(|(_, v)| matches!(v, Verdict::Skipped(_)))
        .count();
    assert_eq!(skipped, 1);
    assert!(
        v.iter()
            .all(|(_, v)| matches!(v, Verdict::Verified | Verdict::Skipped(_)))
    );
}
