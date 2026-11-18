//! Witness entries: every attempt's envelope, and the ledger it was applied in, as entries of
//! the `probe.envelope-fields` family in the candidate lane.

use std::path::Path;

use anyhow::{Context, Result};
use shapekit_contracts::validation::{ApplyStatus, ProbeAttempt, SubmitStatus};
use shapekit_core_driver::canonical::canonicalize_lcm;
use shapekit_gen::entry_io::write_xdr_entry;
use shapekit_manifest::build::{ENTRIES_DIR, ENTRY_FILE};
use shapekit_manifest::xdr::render_json;
use shapekit_manifest::{
    CoreRejection, DecodeExpectation, Entry, EntryKind, EntryProtocol, Expected, Normalization,
    RejectionStage, Source, Validity, Variant, Variants,
};
use shapekit_manifest::{TESTNET_PASSPHRASE, sha256_hex};
use shapekit_registry::SourceClass;
use stellar_xdr::{Limits, WriteXdr};

use crate::engine::Evidence;

pub const FAMILY: &str = "probe.envelope-fields";

/// The family of the limit finder's entries.
pub const LIMITS_FAMILY: &str = "probe.limits";

/// The chain an attempt ran on, by protocol and world.
fn chain(family: &str, e: &Evidence) -> String {
    format!("{family}.{}", e.chain)
}

fn testnet() -> Variants {
    Variants {
        testnet: Some(Variant {
            network_passphrase: Some(TESTNET_PASSPHRASE.to_owned()),
            xdr_file: String::new(),
            xdr_sha256: String::new(),
            expected_json_file: None,
            expected_json_sha256: None,
            oracle_file: None,
            oracle_sha256: None,
        }),
        ..Default::default()
    }
}

fn protocol(p: u32) -> EntryProtocol {
    EntryProtocol {
        introduced: 0,
        generated_at: Some(p),
        valid_min: p,
        valid_max: None,
    }
}

fn outcome(a: &ProbeAttempt) -> String {
    match (a.submission, a.applied) {
        (SubmitStatus::Undecodable, _) => {
            "core's tx endpoint could not decode the bytes".to_owned()
        }
        (_, ApplyStatus::NotApplied) => format!(
            "refused at submission ({}, {})",
            format!("{:?}", a.submission).to_lowercase(),
            a.result_code.as_deref().unwrap_or("no result")
        ),
        (_, ApplyStatus::Failed) => format!(
            "applied and failed, {}",
            a.result_code.as_deref().unwrap_or("-")
        ),
        (_, ApplyStatus::Succeeded) => "applied and succeeded".to_owned(),
    }
}

fn base_entry(family: &str, e: &Evidence, id: String, xdr_type: &str) -> Entry {
    Entry {
        id,
        kind: EntryKind::Xdr,
        family: family.to_owned(),
        caps: Vec::new(),
        protocol: protocol(e.protocol),
        xdr_type: Some(xdr_type.to_owned()),
        lcm_version: None,
        tx_meta_version: None,
        source: Source {
            class: SourceClass::LocalCore,
            recipe: Some(family.to_owned()),
            chain: Some(chain(family, e)),
            ledger_seq: None,
            tx_hash: None,
            tx_index: None,
            upstream: None,
            provenance_url: None,
            normalization: None,
            core_rejection: None,
            recipe_ir: None,
        },
        synthetic: false,
        passphrase_dependent: true,
        variants: testnet(),
        vector: None,
        expected: Expected {
            validity: Validity::Valid,
            decode: DecodeExpectation::MustAccept,
            roundtrip: true,
        },
        flags: Vec::new(),
        fields_exercised: Vec::new(),
        arms: Vec::new(),
        linked_issues: Vec::new(),
        notes: Some(format!(
            "{} of {}{}, attempt {:?}: {}",
            if family == FAMILY { "probe" } else { "limit" },
            e.field,
            match &e.attempt.carrier {
                Some(c) => format!(" in carrier {c}"),
                None => String::new(),
            },
            e.variant,
            outcome(&e.attempt)
        )),
        obligations: Vec::new(),
    }
}

/// Writes the entries of one attempt and returns their witness references.
pub fn write(candidate: &Path, e: &Evidence) -> Result<Vec<String>> {
    write_in(candidate, FAMILY, e)
}

/// [`write`] into `family`.
pub fn write_in(candidate: &Path, family: &str, e: &Evidence) -> Result<Vec<String>> {
    let id = format!("{family}.{}", e.slug);
    let a = &e.attempt;
    let mut entry = base_entry(family, e, id.clone(), "TransactionEnvelope");
    let mut out = vec![format!("candidate:{id}")];
    if a.submission == SubmitStatus::Undecodable {
        entry.source.class = SourceClass::Synthetic;
        entry.source.recipe = None;
        entry.source.chain = None;
        entry.synthetic = true;
        entry.expected = Expected {
            validity: Validity::NotEmittable,
            decode: DecodeExpectation::MustReject,
            roundtrip: false,
        };
        write_raw(candidate, family, entry, &e.envelope)?;
        return Ok(out);
    }
    let rejection = |stage| -> Result<CoreRejection> {
        Ok(CoreRejection {
            stage,
            result_code: a
                .result_code
                .clone()
                .context("a refused or failed attempt has a result code")?,
            result_xdr: a
                .result_xdr
                .clone()
                .context("a refused or failed attempt has a result")?,
        })
    };
    match a.applied {
        ApplyStatus::NotApplied => {
            if a.submission != SubmitStatus::Error {
                // core declined to queue the envelope without a result (try again later: its
                // declared resources exceed what a ledger takes), so it is no core output and
                // no refusal core can be quoted for; the bytes stay as the probe's own
                entry.source.class = SourceClass::Synthetic;
                entry.source.recipe = None;
                entry.source.chain = None;
                entry.synthetic = true;
                entry.expected.validity = Validity::NotEmittable;
                write_xdr_entry(candidate, entry, "testnet", &e.envelope)
                    .with_context(|| format!("writing {id}"))?;
                return Ok(out);
            }
            entry.source.class = SourceClass::LocalCoreRefused;
            entry.source.core_rejection = Some(rejection(RejectionStage::Submit)?);
            entry.expected.validity = Validity::InvalidByCore;
        }
        ApplyStatus::Failed => {
            entry.source.core_rejection = Some(rejection(RejectionStage::Apply)?);
            entry.expected.validity = Validity::InvalidByCore;
        }
        ApplyStatus::Succeeded => {}
    }
    if let Some((seq, _, hash)) = &e.ledger {
        entry.source.ledger_seq = Some(*seq);
        entry.source.tx_hash = Some(hex::encode(hash));
    }
    write_xdr_entry(candidate, entry, "testnet", &e.envelope)
        .with_context(|| format!("writing {id}"))?;

    if let Some((seq, meta, hash)) = &e.ledger {
        let lid = format!("{id}-ledger");
        let mut meta = (**meta).clone();
        canonicalize_lcm(&mut meta)?;
        let mut l = base_entry(family, e, lid.clone(), "LedgerCloseMeta");
        l.lcm_version = Some(match &meta {
            stellar_xdr::LedgerCloseMeta::V0(_) => 0,
            stellar_xdr::LedgerCloseMeta::V1(_) => 1,
            stellar_xdr::LedgerCloseMeta::V2(_) => 2,
        });
        l.source.ledger_seq = Some(*seq);
        l.source.tx_hash = Some(hex::encode(hash));
        l.source.normalization = Some(Normalization::KeyStable);
        write_xdr_entry(candidate, l, "testnet", &meta.to_xdr(Limits::none())?)
            .with_context(|| format!("writing {lid}"))?;
        out.push(format!("candidate:{lid}"));
    }
    Ok(out)
}

/// An entry whose bytes no decoder reads, so it has no XDR-JSON beside it.
fn write_raw(candidate: &Path, family: &str, mut entry: Entry, bytes: &[u8]) -> Result<()> {
    let rel = format!("{ENTRIES_DIR}/{family}/{}", entry.id);
    let dir = candidate.join(&rel);
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    std::fs::write(dir.join("testnet.xdr"), bytes)?;
    let v = entry
        .variants
        .testnet
        .as_mut()
        .expect("testnet() sets the testnet variant");
    v.xdr_file = format!("{rel}/testnet.xdr");
    v.xdr_sha256 = sha256_hex(bytes);
    std::fs::write(
        dir.join(ENTRY_FILE),
        render_json(&serde_json::to_value(&entry)?),
    )?;
    Ok(())
}

/// Removes every entry of `family`, before a run writes them again.
pub fn clear(candidate: &Path, family: &str) -> Result<()> {
    let dir = candidate.join(ENTRIES_DIR).join(family);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
    }
    Ok(())
}
