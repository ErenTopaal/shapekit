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
use super::run::BELOW_PROTOCOL_VALID;

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

/// The protocol range of a local-core entry. A contrast entry (`below-protocol-valid`) holds
/// bytes of the protocols before its family's shape existed, so its range runs from the
/// protocol it was recorded at to the last one before the family's `protocol_valid.min`;
/// every other entry takes the family's range.
pub(crate) fn entry_protocol(
    family: &Family,
    generated_at: u32,
    below_protocol_valid: bool,
) -> EntryProtocol {
    if below_protocol_valid {
        return EntryProtocol {
            introduced: family.protocol_introduced,
            generated_at: Some(generated_at),
            valid_min: generated_at,
            valid_max: Some(family.protocol_valid.min.saturating_sub(1)),
        };
    }
    EntryProtocol {
        introduced: family.protocol_introduced,
        generated_at: Some(generated_at),
        valid_min: family.protocol_valid.min,
        valid_max: family.protocol_valid.max,
    }
}

/// Merges the captures of one chain. `runs` holds the networks that ran
/// now; a network that did not run is taken from the entry already in
/// `corpus`, so passphrase dependence is always decided on both variants.
/// The kept variant is used only when the committed entry was made by the
/// same recipe, chain and protocol and describes the same capture (see
/// [`kept_variant_mismatch`]); otherwise both networks must run.
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
        let mut kept: Vec<(Network, Entry)> = Vec::new();
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
            kept.push((n, old));
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
            obligations: Vec::new(),
            id,
            kind: EntryKind::Xdr,
            family: family.id.clone(),
            caps: family.caps.clone(),
            protocol: entry_protocol(
                family,
                protocol,
                s.flags.iter().any(|f| f == BELOW_PROTOCOL_VALID),
            ),
            xdr_type: Some(first.selected.xdr_type.to_string()),
            lcm_version: first.selected.lcm_version,
            tx_meta_version: first.selected.tx_meta_version,
            source: Source {
                recipe_ir: None,
                // a refused envelope is the generator's bytes, never core output
                class: if refused {
                    SourceClass::LocalCoreRefused
                } else {
                    SourceClass::LocalCore
                },
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
                    .then_some(Normalization::KeyStable),
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
            fields_exercised: fields_carried(first, &s.fields_exercised)?,
            linked_issues: s
                .linked_issues
                .clone()
                .unwrap_or_else(|| family.linked_issues.clone()),
            notes: (!notes.is_empty()).then_some(notes),
        };
        for (n, old) in &kept {
            if let Some(why) = kept_variant_mismatch(old, &entry) {
                bail!(
                    "{}: the committed {} variant was not made by this capture ({why}); regenerate both networks",
                    entry.id,
                    n.name()
                );
            }
        }
        out.push(PendingEntry { entry, bytes });
    }
    Ok(out)
}

/// The fields of `spec` that the captured bytes carry. A recipe states the fields of the
/// transaction it built, and the envelope capture carries all of them. The meta capture
/// of the same transaction holds the result and the ledger changes, not the envelope, so
/// it keeps only a field whose value core also writes into a ledger entry: a signed
/// payload signer key, which lands in the account's signer list. Every other row of
/// fields.toml describes envelope data that meta never repeats.
fn fields_carried(c: &Captured, fields: &[String]) -> Result<Vec<String>> {
    if c.selected.suffix != "-meta" {
        return Ok(fields.to_vec());
    }
    let mut out = Vec::new();
    for f in fields {
        if f == SIGNED_PAYLOAD_FIELD && carries_signed_payload(c)? {
            out.push(f.clone());
        }
    }
    Ok(out)
}

const SIGNED_PAYLOAD_FIELD: &str = "signer_key.signed_payload.payload";

/// Whether the bytes hold a signed payload signer key anywhere. xdr-json renders a signer
/// key as its strkey, so a P strkey string is the signed payload arm.
fn carries_signed_payload(c: &Captured) -> Result<bool> {
    let t = c.selected.xdr_type;
    let bytes = &c.selected.bytes;
    let value = xdr::decode(t, bytes, decode_limits(t, bytes.len()))
        .and_then(|v| xdr::json_value(&v))
        .with_context(|| format!("{}: decoding the {t} to find signer keys", c.slug))?;
    fn walk(v: &Value) -> bool {
        match v {
            Value::String(s) => stellar_strkey::ed25519::SignedPayload::from_string(s).is_ok(),
            Value::Array(a) => a.iter().any(walk),
            Value::Object(o) => o.values().any(walk),
            _ => false,
        }
    }
    Ok(walk(&value))
}

/// Why a committed entry's variant cannot stand next to a fresh capture, if it cannot.
/// Recipes carry no version number, so "the same recipe" is judged by everything the
/// capture decides: recipe and chain, protocol, ledger and transaction position, type and
/// meta versions, what core answered, and what the recipe states about the entry.
pub(crate) fn kept_variant_mismatch(old: &Entry, fresh: &Entry) -> Option<String> {
    let (o, f) = (&old.source, &fresh.source);
    let checks: [(&str, bool); 11] = [
        ("recipe", o.recipe == f.recipe),
        ("chain", o.chain == f.chain),
        (
            "generated_at protocol",
            old.protocol.generated_at == fresh.protocol.generated_at,
        ),
        ("ledger_seq", o.ledger_seq == f.ledger_seq),
        ("tx_index", o.tx_index == f.tx_index),
        ("xdr_type", old.xdr_type == fresh.xdr_type),
        (
            "lcm or meta version",
            old.lcm_version == fresh.lcm_version && old.tx_meta_version == fresh.tx_meta_version,
        ),
        ("core's answer", o.core_rejection == f.core_rejection),
        ("expected verdicts", old.expected == fresh.expected),
        ("flags", old.flags == fresh.flags),
        (
            "fields exercised",
            old.fields_exercised == fresh.fields_exercised,
        ),
    ];
    let differ: Vec<&str> = checks
        .iter()
        .filter(|(_, same)| !same)
        .map(|(what, _)| *what)
        .collect();
    (!differ.is_empty()).then(|| format!("{} differ", differ.join(", ")))
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
        if e.source.class.is_local() && e.source.chain.as_deref() == Some(chain_id) {
            out.insert(e.id, path);
        }
    }
    Ok(out)
}

/// The moves an install made, so a failure part way can put the corpus back. Every
/// committed directory an install replaces is first moved aside into the staging
/// directory (one filesystem, so each step is a rename), and only then is the staged one
/// moved in.
pub(crate) struct Journal {
    backups: PathBuf,
    /// In order: where a new directory went, and where the one it replaced was put.
    moves: Vec<(PathBuf, Option<PathBuf>)>,
    next: usize,
}

impl Journal {
    pub(crate) fn new(staging: &Path) -> Journal {
        Journal {
            backups: staging.join(".replaced"),
            moves: Vec::new(),
            next: 0,
        }
    }

    fn set_aside(&mut self, dir: &Path) -> Result<PathBuf> {
        std::fs::create_dir_all(&self.backups)
            .with_context(|| format!("creating {}", self.backups.display()))?;
        let to = self.backups.join(self.next.to_string());
        self.next += 1;
        std::fs::rename(dir, &to)
            .with_context(|| format!("moving {} aside to {}", dir.display(), to.display()))?;
        Ok(to)
    }

    /// Moves `from` to `to`, setting aside what `to` held.
    fn replace(&mut self, from: &Path, to: &Path) -> Result<()> {
        let old = if to.exists() {
            Some(self.set_aside(to)?)
        } else {
            None
        };
        // recorded before the move, so a failed move still restores what was set aside
        self.moves.push((to.to_path_buf(), old));
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::rename(from, to)
            .with_context(|| format!("moving {} to {}", from.display(), to.display()))
    }

    /// Removes `dir`, keeping it to restore.
    fn remove(&mut self, dir: &Path) -> Result<()> {
        let old = self.set_aside(dir)?;
        self.moves.push((dir.to_path_buf(), Some(old)));
        Ok(())
    }

    /// Undoes every move, newest first. Returns what could not be undone.
    pub(crate) fn roll_back(self) -> Vec<String> {
        let mut failed = Vec::new();
        for (at, old) in self.moves.into_iter().rev() {
            if at.exists()
                && let Err(e) = std::fs::remove_dir_all(&at)
            {
                failed.push(format!("removing {}: {e}", at.display()));
                continue;
            }
            if let Some(old) = old
                && let Err(e) = std::fs::rename(&old, &at)
            {
                failed.push(format!(
                    "restoring {} from {}: {e}",
                    at.display(),
                    old.display()
                ));
            }
        }
        failed
    }
}

/// Replaces the committed entries of a chain with the staged ones: removes
/// every committed local-core entry of the chain, then moves the staged
/// directories in. Refuses to replace an entry of another source class.
pub(crate) fn install_entries(
    journal: &mut Journal,
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
            if !old.source.class.is_local() || old.source.chain.as_deref() != Some(chain_id) {
                bail!(
                    "{id} already exists as a {} entry of chain {:?}; refusing to replace it",
                    old.source.class.as_str(),
                    old.source.chain
                );
            }
        }
    }
    for (_, dir) in chain_entries(corpus, family, chain_id)? {
        journal.remove(&dir)?;
    }
    for id in ids {
        let from = entry_dir(staging, family, id);
        let to = entry_dir(corpus, family, id);
        ensure!(
            !to.exists(),
            "{} exists without being an entry of chain {chain_id}; refusing to replace it",
            to.display()
        );
        journal.replace(&from, &to)?;
    }
    Ok(())
}

/// Moves a staged chain directory over the committed one.
pub(crate) fn install_chain(
    journal: &mut Journal,
    staged: &Path,
    corpus: &Path,
    chain_id: &str,
    n: Network,
) -> Result<()> {
    let to = corpus.join(CHAINS_DIR).join(chain_id).join(n.name());
    journal.replace(staged, &to)
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

/// The oracle files an entry records, relative to the corpus.
pub(crate) fn oracle_files(path: &Path) -> Result<BTreeSet<String>> {
    let e = read_entry(path)?;
    Ok(e.variants
        .iter()
        .filter_map(|(_, v)| v.oracle_file.clone())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn committed_local_core_entry() -> Entry {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/entries");
        for family in std::fs::read_dir(&root).unwrap() {
            for dir in std::fs::read_dir(family.unwrap().path()).unwrap() {
                let file = dir.unwrap().path().join(ENTRY_FILE);
                if let Ok(e) = read_entry(&file)
                    && e.source.class == SourceClass::LocalCore
                    && e.source.tx_index.is_some()
                {
                    return e;
                }
            }
        }
        panic!("the corpus has no local-core transaction entry");
    }

    fn committed_capture(family: &str, id: &str, suffix: &'static str) -> Captured {
        use super::super::ctx::Spec;
        use super::super::select::Selected;
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
        let dir = root.join("entries").join(family).join(id);
        let e = read_entry(&dir.join(ENTRY_FILE)).unwrap();
        Captured {
            slug: id.to_string(),
            ledger_seq: 0,
            selected: Selected {
                suffix,
                xdr_type: if suffix == "-meta" {
                    "TransactionResultMetaV1"
                } else {
                    "TransactionEnvelope"
                },
                bytes: std::fs::read(dir.join("testnet.xdr")).unwrap(),
                lcm_version: None,
                tx_meta_version: e.tx_meta_version,
                tx_index: None,
                tx_hash: None,
                normalized: false,
            },
            validity: e.expected.validity,
            rejection: None,
            spec: Spec::default(),
        }
    }

    #[test]
    fn meta_entries_keep_only_the_fields_their_bytes_carry() {
        let fields: Vec<String> = ["op.set_options.signer", "signer_key.signed_payload.payload"]
            .map(String::from)
            .to_vec();
        let family = "cap40.signed-payload-signer";
        let base = "cap40.signed-payload-signer.p30-set-options-payload-1";
        // the signer lands in the account's signer list, so the meta holds the payload
        let meta = committed_capture(family, &format!("{base}-meta"), "-meta");
        assert_eq!(
            fields_carried(&meta, &fields).unwrap(),
            vec!["signer_key.signed_payload.payload".to_string()]
        );
        let envelope = committed_capture(family, &format!("{base}-envelope"), "-envelope");
        assert_eq!(fields_carried(&envelope, &fields).unwrap(), fields);
        // preconditions live only in the envelope
        let meta = committed_capture(
            "cap21.preconditions",
            "cap21.preconditions.p30-all-preconditions-meta",
            "-meta",
        );
        let cond = vec![
            "tx.cond.type".to_string(),
            "tx.cond.extra_signers".to_string(),
        ];
        assert!(fields_carried(&meta, &cond).unwrap().is_empty());
    }

    #[test]
    fn a_kept_variant_must_come_from_the_same_capture() {
        let old = committed_local_core_entry();
        assert_eq!(kept_variant_mismatch(&old, &old), None);
        let mut moved = old.clone();
        moved.source.ledger_seq = moved.source.ledger_seq.map(|l| l + 1);
        moved.source.tx_index = moved.source.tx_index.map(|i| i + 1);
        let why = kept_variant_mismatch(&old, &moved).unwrap();
        assert!(
            why.contains("ledger_seq") && why.contains("tx_index"),
            "{why}"
        );
        let mut retyped = old.clone();
        retyped.xdr_type = Some("LedgerEntry".into());
        assert!(
            kept_variant_mismatch(&old, &retyped)
                .unwrap()
                .contains("xdr_type")
        );
        let mut other_recipe = old.clone();
        other_recipe.source.recipe = Some("another.recipe".into());
        assert!(kept_variant_mismatch(&old, &other_recipe).is_some());
        let mut restated = old.clone();
        restated.flags.push("new-flag".into());
        assert!(kept_variant_mismatch(&old, &restated).is_some());
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn a_failed_install_restores_the_corpus() {
        let dir = tempfile::tempdir().unwrap();
        let corpus = dir.path().join("corpus");
        let staging = dir.path().join("staging");
        write(&corpus.join("chains/c/testnet/a.txt"), "old chain");
        write(&staging.join("chains/c/testnet/a.txt"), "new chain");
        write(&corpus.join("entries/f/x/entry.json"), "old entry");
        let mut j = Journal::new(&staging);
        install_chain(
            &mut j,
            &staging.join("chains/c/testnet"),
            &corpus,
            "c",
            Network::Testnet,
        )
        .unwrap();
        j.remove(&corpus.join("entries/f/x")).unwrap();
        // a staged directory that does not exist makes the next move fail
        let err = j.replace(
            &staging.join("entries/f/missing"),
            &corpus.join("entries/f/missing"),
        );
        assert!(err.is_err());
        assert_eq!(j.roll_back(), Vec::<String>::new());
        let read = |p: &str| std::fs::read_to_string(corpus.join(p)).unwrap();
        assert_eq!(read("chains/c/testnet/a.txt"), "old chain");
        assert_eq!(read("entries/f/x/entry.json"), "old entry");
        assert!(!corpus.join("entries/f/missing").exists());
    }
}
