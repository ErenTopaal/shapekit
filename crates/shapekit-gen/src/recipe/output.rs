//! Turns the captures of a chain's network runs into entries, checks them,
//! and writes entries and chains into a corpus directory.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use shapekit_manifest::build::{ENTRIES_DIR, ENTRY_FILE};
use shapekit_manifest::xdr::{self, render_json, roundtrip_check, to_xdr_json};
use shapekit_manifest::{
    DecodeExpectation, Entry, EntryKind, EntryProtocol, Expected, Limits, Normalization,
    RejectionStage, Source, Variant, Variants, decode_limits, fragment_matches, sha256_hex,
    shape_signature,
};
use shapekit_registry::{Family, SourceClass};

use super::Network;
use super::ctx::Captured;

pub const CHAINS_DIR: &str = "chains";

/// One entry ready to write: its record and the bytes of each variant.
#[derive(Debug, Clone)]
pub(crate) struct PendingEntry {
    pub entry: Entry,
    pub bytes: BTreeMap<Network, Vec<u8>>,
}

/// What the automated checks found for one written entry.
#[derive(Debug, Clone)]
pub struct EntryCheck {
    pub id: String,
    /// Fragments of the family matched by every variant's signature.
    pub matched_fragments: Vec<String>,
    pub passphrase_dependent: bool,
}

/// What a chain's two runs agree on, reported per chain.
#[derive(Debug, Clone)]
pub struct ChainCheck {
    pub chain_id: String,
    pub entries: Vec<EntryCheck>,
}

fn entry_dir(root: &Path, family: &str, id: &str) -> PathBuf {
    root.join(ENTRIES_DIR).join(family).join(id)
}

fn read_entry(path: &Path) -> Result<Entry> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))
}

fn variant_slot(v: &mut Variants, n: Network) -> &mut Option<Variant> {
    match n {
        Network::Testnet => &mut v.testnet,
        Network::Mainnet => &mut v.mainnet,
    }
}

fn variant_of(v: &Variants, n: Network) -> Option<&Variant> {
    match n {
        Network::Testnet => v.testnet.as_ref(),
        Network::Mainnet => v.mainnet.as_ref(),
    }
}

/// Merges the captures of one chain. `runs` holds the networks that ran
/// now; a network that did not run is taken from the entry already in
/// `corpus`, so passphrase dependence is always decided on both variants.
pub(crate) fn merge(
    family: &Family,
    recipe_id: &str,
    chain_id: &str,
    protocol: u32,
    runs: &BTreeMap<Network, Vec<Captured>>,
    corpus: &Path,
) -> Result<Vec<PendingEntry>> {
    let mut slugs: Option<Vec<&str>> = None;
    for (n, caps) in runs {
        let these: Vec<&str> = caps.iter().map(|c| c.slug.as_str()).collect();
        match &slugs {
            None => slugs = Some(these),
            Some(s) => ensure!(
                *s == these,
                "{chain_id}: the {} run captured {these:?}, another run captured {s:?}",
                n.name()
            ),
        }
    }
    let slugs = slugs.context("no network ran")?;

    let mut out = Vec::new();
    for (i, slug) in slugs.iter().enumerate() {
        let id = format!("{}.{slug}", family.id);
        let caps: BTreeMap<Network, &Captured> = runs.iter().map(|(n, c)| (*n, &c[i])).collect();
        let first = caps.values().next().expect("at least one run");
        for (n, c) in &caps {
            ensure!(
                c.selected.xdr_type == first.selected.xdr_type
                    && c.ledger_seq == first.ledger_seq
                    && c.selected.tx_index == first.selected.tx_index
                    && c.validity == first.validity
                    && c.rejection
                        .as_ref()
                        .map(|r| (&r.stage, &r.result_code, &r.result_xdr))
                        == first.rejection.as_ref().map(|r| (
                            &r.stage,
                            &r.result_code,
                            &r.result_xdr
                        )),
                "{id}: the {} run disagrees with another run on type, ledger, index or core's answer",
                n.name()
            );
        }

        let mut bytes: BTreeMap<Network, Vec<u8>> = caps
            .iter()
            .map(|(n, c)| (*n, c.selected.bytes.clone()))
            .collect();
        for n in Network::BOTH {
            if bytes.contains_key(&n) {
                continue;
            }
            let dir = entry_dir(corpus, &family.id, &id);
            let old = read_entry(&dir.join(ENTRY_FILE)).with_context(|| {
                format!(
                    "{id}: the {} variant is not being generated and no committed entry holds it; run both networks",
                    n.name()
                )
            })?;
            let v = variant_of(&old.variants, n).with_context(|| {
                format!("{id}: the committed entry has no {} variant", n.name())
            })?;
            let b = std::fs::read(corpus.join(&v.xdr_file))
                .with_context(|| format!("reading {}", v.xdr_file))?;
            ensure!(
                sha256_hex(&b) == v.xdr_sha256,
                "{id}: committed {} does not match its hash",
                v.xdr_file
            );
            bytes.insert(n, b);
        }

        let passphrase_dependent = bytes.values().collect::<BTreeSet<_>>().len() > 1;
        // a transaction hash covers the network id, so the variants never
        // share one; it is derivable from each variant's envelope
        let mut notes = first.spec.notes.clone();
        // a refused envelope never reached a ledger; say which ledger core was on
        let refused = first
            .rejection
            .as_ref()
            .is_some_and(|r| r.stage == RejectionStage::Submit);
        if refused {
            notes.push_str(&format!(
                ". submitted with ledger {} as the last closed ledger",
                first.ledger_seq
            ));
        }
        let mut variants = Variants::default();
        for n in bytes.keys() {
            *variant_slot(&mut variants, *n) = Some(Variant {
                network_passphrase: Some(n.passphrase().to_string()),
                xdr_file: String::new(),
                xdr_sha256: String::new(),
                expected_json_file: None,
                expected_json_sha256: None,
                oracle_file: None,
                oracle_sha256: None,
            });
        }
        let s = &first.spec;
        let entry = Entry {
            id,
            kind: EntryKind::Xdr,
            family: family.id.clone(),
            caps: family.caps.clone(),
            protocol: EntryProtocol {
                introduced: family.protocol_introduced,
                generated_at: Some(protocol),
                valid_min: family.protocol_valid.min,
                valid_max: family.protocol_valid.max,
            },
            xdr_type: Some(first.selected.xdr_type.to_string()),
            lcm_version: first.selected.lcm_version,
            tx_meta_version: first.selected.tx_meta_version,
            source: Source {
                class: SourceClass::LocalCore,
                recipe: Some(recipe_id.to_string()),
                chain: Some(chain_id.to_string()),
                ledger_seq: (!refused).then_some(first.ledger_seq),
                tx_hash: None,
                tx_index: first.selected.tx_index,
                upstream: None,
                provenance_url: None,
                normalization: first
                    .selected
                    .normalized
                    .then_some(Normalization::CoreNormalizeMeta),
                core_rejection: first.rejection.clone(),
            },
            synthetic: false,
            passphrase_dependent,
            variants,
            vector: None,
            expected: Expected {
                validity: first.validity,
                decode: if s.must_reject {
                    DecodeExpectation::MustReject
                } else {
                    DecodeExpectation::MustAccept
                },
                roundtrip: !s.must_reject,
            },
            flags: s.flags.clone(),
            fields_exercised: s.fields_exercised.clone(),
            linked_issues: s
                .linked_issues
                .clone()
                .unwrap_or_else(|| family.linked_issues.clone()),
            notes: (!notes.is_empty()).then_some(notes),
        };
        out.push(PendingEntry { entry, bytes });
    }
    Ok(out)
}

/// Round trip and family membership of every variant: each variant's shape
/// signature must match at least one of the family's fragments.
pub(crate) fn check_entry(family: &Family, p: &PendingEntry) -> Result<EntryCheck> {
    let id = &p.entry.id;
    let t = p.entry.xdr_type.as_deref().expect("merge sets xdr_type");
    let must_reject = p.entry.expected.decode == DecodeExpectation::MustReject;
    let mut matched: Option<BTreeSet<String>> = None;
    for (n, bytes) in &p.bytes {
        let limits = decode_limits(t, bytes.len());
        // a must-reject entry is still a member of its family by shape; its bytes were just
        // built here and core read them, so they are decoded without a depth bound for that
        let sig_limits = if must_reject {
            ensure!(
                xdr::decode(t, bytes, limits).is_err(),
                "{id} {}: must-reject, but it decodes under the shared limits",
                n.name()
            );
            Limits {
                depth: u32::MAX,
                len: bytes.len(),
            }
        } else {
            roundtrip_check(t, bytes, limits.clone())
                .with_context(|| format!("{id} {}: round trip", n.name()))?;
            limits
        };
        let sig = shape_signature(t, bytes, sig_limits)
            .with_context(|| format!("{id} {}: shape signature", n.name()))?;
        let these: BTreeSet<String> = family
            .signature
            .iter()
            .filter(|f| fragment_matches(&sig, f))
            .cloned()
            .collect();
        ensure!(
            !these.is_empty(),
            "{id} {}: its {t} matches none of the signature fragments of {}, so it is not an entry of that family",
            n.name(),
            family.id
        );
        matched = Some(match matched {
            None => these,
            Some(m) => m.intersection(&these).cloned().collect(),
        });
    }
    Ok(EntryCheck {
        id: id.clone(),
        matched_fragments: matched.unwrap_or_default().into_iter().collect(),
        passphrase_dependent: p.entry.passphrase_dependent,
    })
}

/// Writes one entry directory under `root`: per variant `<network>.xdr`
/// and `<network>.expected.json`, then `entry.json`. The directory must not
/// exist yet.
pub(crate) fn write_entry(root: &Path, p: &PendingEntry) -> Result<Entry> {
    let mut entry = p.entry.clone();
    let rel_dir = format!("{ENTRIES_DIR}/{}/{}", entry.family, entry.id);
    let dir = root.join(&rel_dir);
    ensure!(!dir.exists(), "{} already exists", dir.display());
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let t = entry.xdr_type.clone().expect("merge sets xdr_type");
    let must_accept = entry.expected.decode == DecodeExpectation::MustAccept;
    for (n, bytes) in &p.bytes {
        let xdr_name = format!("{}.xdr", n.name());
        std::fs::write(dir.join(&xdr_name), bytes)?;
        let v = variant_slot(&mut entry.variants, *n)
            .as_mut()
            .expect("merge creates a record per variant");
        v.xdr_file = format!("{rel_dir}/{xdr_name}");
        v.xdr_sha256 = sha256_hex(bytes);
        // a value a decoder must refuse has no rendering to compare against
        if must_accept {
            let json = to_xdr_json(&t, bytes, decode_limits(&t, bytes.len()))
                .with_context(|| format!("{}: rendering XDR-JSON", entry.id))?;
            let json_name = format!("{}.expected.json", n.name());
            std::fs::write(dir.join(&json_name), json.as_bytes())?;
            v.expected_json_file = Some(format!("{rel_dir}/{json_name}"));
            v.expected_json_sha256 = Some(sha256_hex(json.as_bytes()));
        }
    }
    let text = render_json(&serde_json::to_value(&entry)?);
    let path = dir.join(ENTRY_FILE);
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(entry)
}

/// Local-core entries of `chain_id` in `family`, by id, with their directories.
pub(crate) fn chain_entries(
    root: &Path,
    family: &str,
    chain_id: &str,
) -> Result<BTreeMap<String, PathBuf>> {
    let mut out = BTreeMap::new();
    let dir = root.join(ENTRIES_DIR).join(family);
    if !dir.exists() {
        return Ok(out);
    }
    for item in std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = item?.path();
        let file = path.join(ENTRY_FILE);
        if !file.is_file() {
            continue;
        }
        let e = read_entry(&file)?;
        if e.source.class == SourceClass::LocalCore && e.source.chain.as_deref() == Some(chain_id) {
            out.insert(e.id, path);
        }
    }
    Ok(out)
}

/// Replaces the committed entries of a chain with the staged ones: removes
/// every committed local-core entry of the chain, then moves the staged
/// directories in. Refuses to replace an entry of another source class.
pub(crate) fn install_entries(
    staging: &Path,
    corpus: &Path,
    family: &str,
    chain_id: &str,
    ids: &[String],
) -> Result<()> {
    for id in ids {
        let file = entry_dir(corpus, family, id).join(ENTRY_FILE);
        if file.exists() {
            let old = read_entry(&file)?;
            if old.source.class != SourceClass::LocalCore
                || old.source.chain.as_deref() != Some(chain_id)
            {
                bail!(
                    "{id} already exists as a {} entry of chain {:?}; refusing to replace it",
                    old.source.class.as_str(),
                    old.source.chain
                );
            }
        }
    }
    for (_, dir) in chain_entries(corpus, family, chain_id)? {
        std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
    }
    for id in ids {
        let from = entry_dir(staging, family, id);
        let to = entry_dir(corpus, family, id);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&from, &to)
            .with_context(|| format!("moving {} to {}", from.display(), to.display()))?;
    }
    Ok(())
}

/// Moves a staged chain directory over the committed one.
pub(crate) fn install_chain(
    staged: &Path,
    corpus: &Path,
    chain_id: &str,
    n: Network,
) -> Result<()> {
    let to = corpus.join(CHAINS_DIR).join(chain_id).join(n.name());
    if to.exists() {
        std::fs::remove_dir_all(&to).with_context(|| format!("removing {}", to.display()))?;
    }
    std::fs::create_dir_all(to.parent().expect("has a parent"))?;
    std::fs::rename(staged, &to)
        .with_context(|| format!("moving {} to {}", staged.display(), to.display()))
}

/// `chain.json` without the fields that record core's raw output, which
/// differs between runs in change order (see the recorder).
pub(crate) fn reproducible_chain_json(path: &Path) -> Result<Value> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let mut v: Value =
        serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))?;
    let obj = v.as_object_mut().context("chain.json is not an object")?;
    obj.remove("stream_sha256");
    if let Some(Value::Array(ledgers)) = obj.get_mut("ledgers") {
        for l in ledgers {
            if let Some(o) = l.as_object_mut() {
                o.remove("raw_sha256");
            }
        }
    }
    Ok(v)
}

/// `entry.json` without oracle references, which `shapekit oracle run` adds
/// after generation.
pub(crate) fn generator_entry_json(path: &Path) -> Result<Value> {
    let mut e = read_entry(path)?;
    for n in Network::BOTH {
        if let Some(v) = variant_slot(&mut e.variants, n) {
            v.oracle_file = None;
            v.oracle_sha256 = None;
        }
    }
    Ok(serde_json::to_value(&e)?)
}

/// The files of an entry the generator writes, relative to the corpus.
pub(crate) fn generator_files(path: &Path) -> Result<Vec<String>> {
    let e = read_entry(path)?;
    let mut out = Vec::new();
    for (_, v) in e.variants.iter() {
        out.push(v.xdr_file.clone());
        out.extend(v.expected_json_file.clone());
    }
    Ok(out)
}
