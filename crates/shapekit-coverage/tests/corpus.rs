//! Checks over the committed corpus: coverage is current with every gap explained, and the
//! RFP's list of shapes that already tripped one SDK is present.

use std::path::{Path, PathBuf};

use serde_json::Value;
use shapekit_coverage::{EntryShape, entry_shapes, is_core_emitted};
use shapekit_manifest::{Entry, EntryKind, Manifest, fragment_matches};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn corpus() -> PathBuf {
    root().join("corpus")
}

fn manifest() -> Manifest {
    shapekit_coverage::load_manifest(&corpus()).unwrap()
}

// what `shapekit coverage --check` enforces, so a stale or unexplained report fails the tests
#[test]
fn committed_coverage_is_current_and_every_gap_has_a_reason() {
    let (report, mut problems) =
        shapekit_coverage::run(&corpus(), &root().join("registry")).unwrap();
    problems.extend(shapekit_coverage::stale(
        &corpus(),
        &shapekit_coverage::render(&report),
    ));
    let text: Vec<String> = problems.iter().map(ToString::to_string).collect();
    assert!(text.is_empty(), "coverage problems:\n{}", text.join("\n"));
    assert_eq!(report.arms.reachable, 499);
    assert_eq!(
        report.arms.covered.len() + report.arms.uncovered.len(),
        report.arms.reachable
    );
}

fn flagged<'a>(m: &'a Manifest, flag: &str) -> Vec<&'a Entry> {
    m.entries
        .iter()
        .filter(|e| e.flags.iter().any(|f| f == flag))
        .collect()
}

fn vector_cases(e: &Entry) -> Vec<Value> {
    let file = e
        .vector
        .as_ref()
        .expect("a non-xdr entry has a vector file");
    let v: Value =
        serde_json::from_str(&std::fs::read_to_string(corpus().join(&file.path)).unwrap()).unwrap();
    v["cases"].as_array().cloned().unwrap_or_default()
}

fn core_emitted_matching<'a>(
    shapes: &'a [EntryShape<'a>],
    fragments: &[&str],
) -> Vec<&'a EntryShape<'a>> {
    shapes
        .iter()
        .filter(|s| s.core_emitted() && fragments.iter().all(|f| fragment_matches(&s.signature, f)))
        .collect()
}

// rfp section 3: "asset pairs whose issuers sort differently by strkey and by XDR bytes; muxed
// source and recipient accounts; muxed account and offer IDs that decode to a different ID
// than they encode; vectors at their maximum length; nested authorization entries; and maps
// whose keys collide after conversion"
#[test]
fn rfp_shapes_that_tripped_an_sdk_are_all_present() {
    let m = manifest();
    let shapes = entry_shapes(&m, &corpus()).unwrap();
    let mut missing = Vec::new();

    // issuer order: a pool trustline core accepted, and the ordering vector for sdks
    let issuer = flagged(&m, "issuer-order-differs");
    if !issuer.iter().any(|e| is_core_emitted(e)) {
        missing.push("core-emitted asset pair whose issuer order differs by strkey and bytes");
    }
    if !issuer.iter().any(|e| e.kind == EntryKind::Ordering) {
        missing.push("ordering vector for an issuer pair that sorts differently by strkey");
    }

    // a muxed transaction source and a muxed payment destination in one core-emitted entry
    if core_emitted_matching(
        &shapes,
        &[
            ".tx.source_account:muxed_ed25519",
            ".body:payment.destination:muxed_ed25519",
        ],
    )
    .is_empty()
    {
        missing.push("core-emitted muxed source and muxed recipient");
    }

    // the js-stellar-sdk#1785 inputs that encode a different id than the text says
    for (family, kind) in [
        ("cap27.muxed-accounts", "muxed"),
        ("cap03.offer-liabilities", "offer"),
    ] {
        let found = m.entries.iter().any(|e| {
            e.family == family
                && e.kind == EntryKind::TextualId
                && e.flags.iter().any(|f| f == "non-canonical-id")
                && vector_cases(e)
                    .iter()
                    .any(|c| c["kind"] == kind && c["input"]["text"] == "0x10")
        });
        if !found {
            missing.push(if kind == "muxed" {
                "non-canonical muxed id vector"
            } else {
                "non-canonical offer id vector"
            });
        }
    }

    // a core-emitted entry flagged at a vector's declared bound, with the bound in its shape
    let at_max = flagged(&m, "max-length-vec").into_iter().any(|e| {
        is_core_emitted(e)
            && shapes
                .iter()
                .find(|s| s.entry.id == e.id)
                .is_some_and(|s| s.signature.iter().any(|p| p.ends_with("#max")))
    });
    if !at_max {
        missing.push("core-emitted vector at its maximum length");
    }

    // an authorization tree core accepted with invocations nested at least three deep
    if core_emitted_matching(
        &shapes,
        &[".root_invocation.sub_invocations[].sub_invocations[].sub_invocations[]"],
    )
    .is_empty()
    {
        missing.push("core-emitted nested authorization entries");
    }

    // the js-stellar-sdk#1789 map and a map core stored with the same colliding keys
    let collide_vector = flagged(&m, "map-key-collision-after-native")
        .into_iter()
        .any(|e| {
            e.kind == EntryKind::NativeProjection
                && vector_cases(e)
                    .iter()
                    .any(|c| c["expected"]["rule"] == "no-entry-lost")
        });
    if !collide_vector {
        missing.push("native-projection vector with colliding map keys");
    }
    if !flagged(&m, "colliding-map-keys")
        .into_iter()
        .any(is_core_emitted)
    {
        missing.push("core-emitted map whose keys collide after native conversion");
    }

    assert!(missing.is_empty(), "missing rfp shapes: {missing:?}");
}

// the check must notice each kind of gap and each stale reason, not just pass on today's data
#[test]
fn check_fails_on_missing_and_stale_reasons() {
    use shapekit_coverage::uncovered::{ArmReason, Blocker, UncoveredFile};
    let m = manifest();
    let registry = shapekit_registry::Registry::load(&root().join("registry")).unwrap();
    let uncovered = UncoveredFile::load(&root().join("registry")).unwrap();
    let shapes = entry_shapes(&m, &corpus()).unwrap();
    let problems = |u: &UncoveredFile| -> Vec<String> {
        let (_, p) = shapekit_coverage::compute(&m, &registry, u, &shapes).unwrap();
        p.iter().map(ToString::to_string).collect()
    };
    assert!(problems(&uncovered).is_empty());

    let mut no_cap = uncovered.clone();
    no_cap.caps.retain(|c| c.id != "CAP-0083");
    assert!(
        problems(&no_cap)
            .iter()
            .any(|p| p.starts_with("cap CAP-0083"))
    );

    let mut no_arm = uncovered.clone();
    for r in &mut no_arm.arms {
        r.arms.retain(|a| a != "StellarValueExt:empty_tx_set_ms");
    }
    let p = problems(&no_arm);
    assert!(
        p.iter()
            .any(|p| p.starts_with("arm StellarValueExt:empty_tx_set_ms"))
    );

    let mut no_fragment = uncovered.clone();
    no_fragment
        .fragments
        .retain(|f| f.fragment != "BumpSequenceResult:bad_seq");
    assert!(
        problems(&no_fragment)
            .iter()
            .any(|p| p.contains("fragment BumpSequenceResult:bad_seq"))
    );

    let mut covered_arm = uncovered.clone();
    covered_arm.arms.push(ArmReason {
        arms: vec!["MuxedAccount:muxed_ed25519".into()],
        blocker: Blocker::RecipeGap,
        reason: "stale".into(),
    });
    assert!(
        problems(&covered_arm)
            .iter()
            .any(|p| p.contains("MuxedAccount:muxed_ed25519") && p.contains("now covered"))
    );
}
