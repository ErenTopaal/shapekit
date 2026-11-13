//! The run report (`report.json`, `FUZZ.md`, `timing.json`) and one reproducer per class in
//! the corpus vector layout: `<id>/entry.json`, `<id>/any.xdr`, plus `<id>/class.json` with
//! what the fuzzer saw. `report.json` holds nothing that depends on the clock, so two runs
//! with one seed and one input budget compare equal; the wall-clock figures are in
//! `timing.json` and `FUZZ.md`.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{Value, json};
use shapekit_manifest::{DecodeExpectation, Entry, Validity, sha256_hex};

use crate::oracle::Side;
use crate::types::FuzzType;
use crate::{Config, Minimized, Outcome, class_id};

pub struct Written {
    pub new: Vec<String>,
    pub known: Vec<String>,
}

/// The expectation core's behaviour fixes, where it fixes one, and where it comes from. For
/// envelopes core's decode is stellar-xdr at the envelope depth (`can_parse_transaction`
/// refuses what it cannot read from protocol 28); the types core only produces are left to
/// the implementation, and so are bytes stellar-xdr reads but does not write back unchanged,
/// which no conforming encoder emits.
fn expectation(
    ty: FuzzType,
    rust: &Side,
    input: &[u8],
) -> (DecodeExpectation, Validity, &'static str) {
    match rust.accepted() {
        Some(again) if again != input => (
            DecodeExpectation::ImplementationDefined,
            Validity::NotEmittable,
            "implementation-defined: stellar-xdr reads these bytes but writes different ones back, so they are not canonical and no conforming encoder emits them",
        ),
        Some(_) if ty.core_decodes() => (
            DecodeExpectation::MustAccept,
            Validity::Valid,
            "core's verdict: stellar-xdr TransactionEnvelope::from_xdr at depth 1000 (xdr.envelope.depth) reads and round-trips them; validity here is decode validity only",
        ),
        Some(_) => (
            DecodeExpectation::ImplementationDefined,
            Validity::Valid,
            "implementation-defined: core produces this type and never decodes untrusted bytes of it",
        ),
        None if ty.core_decodes() => (
            DecodeExpectation::MustReject,
            Validity::InvalidByCore,
            "core's verdict: from protocol 28 core refuses an envelope stellar-xdr cannot read at depth 1000 (can_parse_transaction)",
        ),
        None => (
            DecodeExpectation::ImplementationDefined,
            Validity::NotEmittable,
            "implementation-defined: core produces this type and never decodes untrusted bytes of it",
        ),
    }
}

fn verdict(s: &Side) -> Value {
    match s {
        Side::Accepted(b) => {
            json!({"verdict": "accept", "reencoded_len": b.len(), "reencoded_sha256": sha256_hex(b)})
        }
        Side::Rejected(m) => json!({"verdict": "reject", "error": m}),
        Side::Panicked(m) => json!({"verdict": "panic", "error": m}),
        Side::Failed(m) => json!({"verdict": "testee-failure", "error": m}),
    }
}

/// The reproducer's manifest entry. `protocol` is the newest protocol both sides know
/// (versions.toml's max_protocol), the only one the verdicts were observed under.
pub fn entry_json(m: &Minimized, go_key: &str, protocol: u32) -> Result<(String, Value)> {
    let c = &m.class;
    let id = class_id(c.ty, c.kind, &c.signature);
    let (decode, validity, source) = expectation(c.ty, &m.rust, &m.reproducer);
    let roundtrip = decode == DecodeExpectation::MustAccept;
    let notes = format!(
        "differential fuzz reproducer, minimised to {} bytes from {}. stellar-xdr {} {}; \
         go-stellar-sdk ({go_key}) {}. both decoded with depth {} and the input length as \
         length limit. expected: {source}. first seen: worker {} iteration {} ({}), {} inputs \
         in this class during the run.",
        m.reproducer.len(),
        c.input.len(),
        stellar_xdr::VERSION.pkg,
        match m.rust.accepted() {
            Some(b) if b != m.reproducer.as_slice() =>
                format!("{} that differ from the input", m.rust.describe()),
            _ => m.rust.describe(),
        },
        m.go.describe(),
        c.ty.depth(),
        c.first.worker,
        c.first.iteration,
        c.first.origin.slug(),
        c.count,
    );
    let value = json!({
        "caps": [],
        "expected": {
            "decode": decode,
            "roundtrip": roundtrip,
            "validity": validity,
        },
        "family": "fuzz",
        "fields_exercised": [],
        "flags": ["differential-fuzz", c.kind.slug()],
        "id": id,
        "kind": "xdr",
        "linked_issues": [],
        "notes": notes,
        "passphrase_dependent": false,
        "protocol": {"introduced": protocol, "valid_min": protocol},
        "source": {"class": "synthetic"},
        "synthetic": true,
        "variants": {"any": {
            "xdr_file": format!("{id}/any.xdr"),
            "xdr_sha256": sha256_hex(&m.reproducer),
        }},
        "xdr_type": c.ty.name(),
    });
    // the manifest's own record type is the format check
    let entry: Entry = serde_json::from_value(value.clone())
        .with_context(|| format!("{id}: the reproducer record is not a manifest entry"))?;
    Ok((id, serde_json::to_value(entry)?))
}

fn write_reproducer(
    dir: &Path,
    m: &Minimized,
    go_key: &str,
    seed: u64,
    protocol: u32,
) -> Result<String> {
    let (id, entry) = entry_json(m, go_key, protocol)?;
    let at = dir.join(&id);
    std::fs::create_dir_all(&at).with_context(|| format!("creating {}", at.display()))?;
    std::fs::write(at.join("any.xdr"), &m.reproducer)?;
    let mut text = serde_json::to_string_pretty(&entry)?;
    text.push('\n');
    std::fs::write(at.join("entry.json"), text)?;
    let class = json!({
        "id": id,
        "signature": m.class.signature,
        "xdr_type": m.class.ty.name(),
        "kind": m.class.kind.slug(),
        "rust": verdict(&m.rust),
        "go": verdict(&m.go),
        "go_testee": go_key,
        "stellar_xdr": stellar_xdr::VERSION.pkg,
        "run_seed": seed,
        "first_seen": {
            "worker": m.class.first.worker,
            "iteration": m.class.first.iteration,
            "origin": m.class.first.origin.slug(),
        },
        "count": m.class.count,
        "original_len": m.class.input.len(),
        "minimize_calls": m.minimize_calls,
    });
    let mut text = serde_json::to_string_pretty(&class)?;
    text.push('\n');
    std::fs::write(at.join("class.json"), text)?;
    Ok(id)
}

/// Writes every class under `<out>/reproducers/` and the classes not in `known` under
/// `findings`, then `report.json` and `FUZZ.md` under `out`.
pub fn write(
    cfg: &Config,
    outcome: &Outcome,
    known: &BTreeMap<String, String>,
    out: &Path,
    findings: Option<&Path>,
) -> Result<Written> {
    let go_key = outcome.go.key();
    let repro_dir = out.join("reproducers");
    if repro_dir.exists() {
        std::fs::remove_dir_all(&repro_dir)?;
    }
    std::fs::create_dir_all(&repro_dir)?;
    let mut written = Written {
        new: Vec::new(),
        known: Vec::new(),
    };
    let mut classes = Vec::new();
    for m in &outcome.classes {
        let id = write_reproducer(&repro_dir, m, &go_key, cfg.seed, outcome.protocol)?;
        let is_known = known.contains_key(&id);
        if is_known {
            written.known.push(id.clone());
        } else {
            if let Some(f) = findings {
                write_reproducer(f, m, &go_key, cfg.seed, outcome.protocol)?;
            }
            written.new.push(id.clone());
        }
        classes.push(json!({
            "id": id,
            "known": known.get(&id),
            "signature": m.class.signature,
            "xdr_type": m.class.ty.name(),
            "kind": m.class.kind.slug(),
            "count": m.class.count,
            "reproducer_len": m.reproducer.len(),
            "reproducer_hex": hex(&m.reproducer),
            "rust": verdict(&m.rust),
            "go": verdict(&m.go),
        }));
    }

    let secs = outcome.elapsed.as_secs_f64();
    let inputs = outcome.inputs();
    let mut per_type = serde_json::Map::new();
    for (t, s) in &outcome.stats {
        per_type.insert(
            t.name().into(),
            json!({
                "seeds": outcome.seeds.get(t).copied().unwrap_or(0),
                "inputs": s.inputs,
                "bytes": s.bytes,
                "both_accept": s.both_accept,
                "both_reject": s.both_reject,
                "filtered": s.filtered,
                "disagreements": s.disagreements,
                "generation_failures": s.generation_failures,
            }),
        );
    }
    let origins: BTreeMap<&str, u64> = outcome
        .origins
        .iter()
        .map(|(o, n)| (o.slug(), *n))
        .collect();
    let report = json!({
        "seed": cfg.seed,
        "jobs": cfg.jobs,
        "max_inputs": cfg.max_inputs,
        // false when the clock ended the run: then the inputs depend on the machine's speed
        "reproducible": !outcome.time_cut,
        "protocol": outcome.protocol,
        "inputs": inputs,
        "rust": format!("stellar-xdr {}", stellar_xdr::VERSION.pkg),
        "go": go_key,
        "types": per_type,
        "origins": origins,
        "classes": classes,
        "new_classes": written.new,
    });
    std::fs::create_dir_all(out)?;
    let mut text = serde_json::to_string_pretty(&report)?;
    text.push('\n');
    std::fs::write(out.join("report.json"), text)?;
    let timing = json!({
        "seconds": secs,
        "inputs_per_second": inputs as f64 / secs.max(1e-9),
    });
    let mut text = serde_json::to_string_pretty(&timing)?;
    text.push('\n');
    std::fs::write(out.join("timing.json"), text)?;
    std::fs::write(out.join("FUZZ.md"), markdown(cfg, outcome, known))?;
    Ok(written)
}

fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

fn markdown(cfg: &Config, outcome: &Outcome, known: &BTreeMap<String, String>) -> String {
    let secs = outcome.elapsed.as_secs_f64();
    let inputs = outcome.inputs();
    let mut s = String::new();
    let _ = writeln!(
        s,
        "# Differential fuzz: stellar-xdr {} and {}\n",
        stellar_xdr::VERSION.pkg,
        outcome.go.key()
    );
    let _ = writeln!(
        s,
        "Seed {}, {} workers, {:.0} s, {} inputs ({:.0} per second). {}\n",
        cfg.seed,
        cfg.jobs,
        secs,
        inputs,
        inputs as f64 / secs.max(1e-9),
        if outcome.time_cut {
            "The time budget ended the run, so it is not reproducible from the seed; give --max-inputs and enough --seconds for that."
        } else {
            "Every worker finished its share of --max-inputs, so the seed reproduces this run."
        }
    );
    let _ = writeln!(
        s,
        "| type | seeds | inputs | both accept | both reject | depth filter | disagree |"
    );
    let _ = writeln!(s, "|---|---:|---:|---:|---:|---:|---:|");
    for (t, st) in &outcome.stats {
        let filtered: u64 = st.filtered.values().sum();
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {} | {} | {} |",
            t.name(),
            outcome.seeds.get(t).copied().unwrap_or(0),
            st.inputs,
            st.both_accept,
            st.both_reject,
            filtered,
            st.disagreements
        );
    }
    let _ = writeln!(s);
    if outcome.classes.is_empty() {
        let _ = writeln!(
            s,
            "No disagreement outside the known depth-accounting divergence."
        );
        return s;
    }
    let _ = writeln!(s, "| class | inputs | reproducer | rust | go | status |");
    let _ = writeln!(s, "|---|---:|---:|---|---|---|");
    for m in &outcome.classes {
        let id = class_id(m.class.ty, m.class.kind, &m.class.signature);
        let status = known
            .get(&id)
            .map_or("new".to_string(), |w| format!("known: {w}"));
        let _ = writeln!(
            s,
            "| `{id}` | {} | {} bytes | {} | {} | {status} |",
            m.class.count,
            m.reproducer.len(),
            m.rust.describe().replace('|', "/"),
            m.go.describe().replace('|', "/"),
        );
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oracle::{Kind, rust_side};
    use crate::{Class, Origin, Sighting};

    fn minimized(ty: FuzzType, bytes: Vec<u8>, go: Side) -> Minimized {
        let rust = rust_side(ty, &bytes, ty.limits(bytes.len()));
        let kind = crate::oracle::classify(&rust, &go).expect("a disagreement");
        Minimized {
            class: Class {
                signature: crate::oracle::signature(ty, kind, &rust, &go),
                ty,
                kind,
                count: 1,
                input: bytes.clone(),
                input_worker: 0,
                first: Sighting {
                    worker: 0,
                    iteration: 1,
                    origin: Origin::Arbitrary,
                },
                rust: rust.clone(),
                go: go.clone(),
            },
            reproducer: bytes,
            minimize_calls: 0,
            rust,
            go,
        }
    }

    #[test]
    fn non_canonical_bytes_are_implementation_defined() {
        // an ScVal bool written as 2: stellar-xdr reads it and writes 0 back
        let m = minimized(
            FuzzType::ScVal,
            vec![0, 0, 0, 0, 0, 0, 0, 2],
            Side::Rejected("bool not 0 or 1".into()),
        );
        assert_eq!(m.class.kind, Kind::RustAcceptsGoRejects);
        let (id, entry) = entry_json(&m, "go@test", 30).unwrap();
        assert!(id.starts_with("fuzz.sc-val-rust-accepts-go-rejects-"));
        assert_eq!(entry["expected"]["decode"], "implementation-defined");
        assert_eq!(entry["expected"]["validity"], "not-emittable");
        assert_eq!(entry["expected"]["roundtrip"], false);
        assert_eq!(entry["kind"], "xdr");
        assert_eq!(entry["synthetic"], true);
    }

    #[test]
    fn envelopes_take_cores_verdict() {
        // three bytes of an envelope: stellar-xdr refuses, so core refuses
        let m = minimized(
            FuzzType::TransactionEnvelope,
            vec![0, 0, 0],
            Side::Accepted(vec![0, 0, 0, 0]),
        );
        let (_, entry) = entry_json(&m, "go@test", 30).unwrap();
        assert_eq!(entry["expected"]["decode"], "must-reject");
        assert_eq!(entry["expected"]["validity"], "invalid-by-core");
    }
}
