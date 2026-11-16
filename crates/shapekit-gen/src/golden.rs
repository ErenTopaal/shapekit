//! stellar-core's `--check-lcm` goldens (R3): the hash index of the whole upstream set, import
//! of the smallest golden ledger per family fragment as `core-golden` entries, and the
//! structural agreement check between golden entries and local-core or history entries.
//!
//! Golden bytes are imported exactly as core wrote them. They are not normalised: core's own
//! `--check-lcm` compares the raw files, and the literal agreement job reruns that comparison.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shapekit_manifest::framing::FramedReader;
use shapekit_manifest::signature::XdrSchema;
use shapekit_manifest::xdr::{self, render_json};
use shapekit_manifest::{
    DecodeExpectation, Entry, EntryKind, EntryProtocol, Expected, Manifest, Source, Upstream,
    Validity, Variant, Variants, decode_limits, fragment_matches, sha256_hex, shape_signature,
};
use shapekit_registry::{Family, Registry, SourceClass};
use stellar_xdr::{
    InnerTransactionResultResult, LedgerCloseMeta, LedgerEntryChange, LedgerEntryChanges,
    OperationResult, TransactionMeta, TransactionResultResult,
};

use crate::entry_io;

/// The passphrase every golden was captured under (core's test network).
pub const GOLDEN_PASSPHRASE: &str = "(V) (;,,;) (V)";
/// The tier a release build of core reads, and the one the release corpus imports.
pub const TIER: &str = "test-lcm-current";
/// The tier core's next-protocol build reads. Only imported into the separate `next/`
/// rehearsal tree, never into the release corpus.
pub const NEXT_TIER: &str = "test-lcm-next";
/// The golden tiers stellar-core keeps.
pub const TIERS: [&str; 2] = [TIER, NEXT_TIER];
pub const CORE_REPO: &str = "https://github.com/stellar/stellar-core";
/// The arguments of core's own literal check, as ci-build.sh at v30.0.0 runs it.
pub const CHECK_LCM_ARGS: &str =
    "test '[tx]' --ll fatal -r simple --disable-dots --rng-seed 12345 --check-lcm <dir>";
const LCM_TYPE: &str = "LedgerCloseMeta";
const STAMP_PROTOCOL: &str = "!cfg protocol version";
const STAMP_SEED: &str = "!rng seed";
const STAMP_ALL_VERSIONS: &str = "!test all versions";
const STAMP_VERSIONS: &str = "!versions to test";
// the largest golden file at v30.0.0 is 5.3 MB; a record cannot be larger than its file
const MAX_RECORD: usize = 64 << 20;
/// Golden ledgers above this size are not imported as entries. The one ledger above it at
/// v30.0.0 is 2.6 MB of XDR and 79 MB of XDR-JSON; the fragments only it carries are listed in
/// the golden index as not imported, with the upstream file that holds them.
pub const MAX_IMPORT_BYTES: usize = 1 << 20;

/// The header keys core stamps into every `index.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stamps {
    pub protocol: u32,
    pub rng_seed: u64,
    pub test_all_versions: bool,
    pub versions_to_test: Vec<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GoldenDir {
    pub name: String,
    pub index_path: String,
    pub index_sha256: String,
    pub stamps: Stamps,
}

/// One ledger inside a golden file.
#[derive(Debug, Clone, Serialize)]
pub struct GoldenRecord {
    pub ledger_seq: u32,
    pub ledger_version: u32,
    pub lcm_version: u32,
    pub bytes: usize,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GoldenFile {
    /// Path inside the core source tree, for example `test-lcm-current/MergeTests/<hash>.xdr`.
    pub path: String,
    pub dir: String,
    pub sha256: String,
    pub bytes: usize,
    pub test: String,
    pub section: String,
    pub stamps: Stamps,
    pub network_passphrase: String,
    /// Number of ledgers in the file and the sequence range they cover.
    pub ledgers: usize,
    pub ledger_seqs: [u32; 2],
    #[serde(skip)]
    pub records: Vec<GoldenRecord>,
    /// Corpus entries imported from this file, filled in by [`import`].
    pub imported_entries: Vec<String>,
}

/// A decoded golden ledger with its signature, kept in memory for selection.
pub struct GoldenLcm {
    pub file: usize,
    pub record: usize,
    pub bytes: Vec<u8>,
    pub signature: BTreeSet<String>,
    pub tx_meta_version: Option<u32>,
}

pub struct GoldenSet {
    pub tag: String,
    /// [`TIER`] or [`NEXT_TIER`].
    pub tier: String,
    pub commit: String,
    pub dirs: Vec<GoldenDir>,
    pub files: Vec<GoldenFile>,
    pub lcms: Vec<GoldenLcm>,
}

/// The commit `tag` names in the git checkout at `core_src`, required to be the checked-out
/// commit so the index never records a tag for files from another revision.
pub fn checkout_commit(core_src: &Path, tag: &str) -> Result<String> {
    checkout_commit_tier(core_src, TIER, tag)
}

/// [`checkout_commit`] for one of [`TIERS`]; only that tier must be free of local changes.
pub fn checkout_commit_tier(core_src: &Path, tier: &str, tag: &str) -> Result<String> {
    ensure!(
        TIERS.contains(&tier),
        "unknown golden tier {tier:?}, expected one of {TIERS:?}"
    );
    let rev = |spec: &str| -> Result<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(core_src)
            .args(["rev-parse", "--verify", spec])
            .output()
            .context("running git")?;
        ensure!(
            out.status.success(),
            "git rev-parse {spec} in {} failed: {}",
            core_src.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
        Ok(String::from_utf8(out.stdout)?.trim().to_string())
    };
    let head = rev("HEAD^{commit}")?;
    let tagged = rev(&format!("refs/tags/{tag}^{{commit}}"))?;
    ensure!(
        head == tagged,
        "{} is checked out at {head}, but tag {tag} is {tagged}",
        core_src.display()
    );
    let dirty = Command::new("git")
        .arg("-C")
        .arg(core_src)
        .args(["status", "--porcelain", "--", tier])
        .output()
        .context("running git status")?;
    ensure!(dirty.status.success(), "git status failed");
    ensure!(
        dirty.stdout.is_empty(),
        "{tier} in {} has local changes",
        core_src.display()
    );
    Ok(head)
}

/// Reads every directory of the current tier, checks that index and files agree, and decodes,
/// round-trips and signs every ledger.
pub fn scan(core_src: &Path, tag: &str, commit: &str) -> Result<GoldenSet> {
    scan_tier(core_src, TIER, tag, commit)
}

/// [`scan`] over one of [`TIERS`].
pub fn scan_tier(core_src: &Path, tier_name: &str, tag: &str, commit: &str) -> Result<GoldenSet> {
    ensure!(
        TIERS.contains(&tier_name),
        "unknown golden tier {tier_name:?}, expected one of {TIERS:?}"
    );
    let tier = core_src.join(tier_name);
    let schema = XdrSchema::for_type(LCM_TYPE)?;
    let mut set = GoldenSet {
        tag: tag.to_string(),
        tier: tier_name.to_string(),
        commit: commit.to_string(),
        dirs: Vec::new(),
        files: Vec::new(),
        lcms: Vec::new(),
    };
    for dir in sorted_children(&tier)? {
        if !dir.is_dir() {
            bail!("{} is not a directory", dir.display());
        }
        let name = file_name(&dir)?;
        let index_path = dir.join("index.json");
        let index_bytes = std::fs::read(&index_path)
            .with_context(|| format!("reading {}", index_path.display()))?;
        let index: serde_json::Map<String, Value> = serde_json::from_slice(&index_bytes)
            .with_context(|| format!("parsing {}", index_path.display()))?;
        let stamps = stamps(&index).with_context(|| format!("in {}", index_path.display()))?;
        let mut listed: BTreeMap<String, String> = BTreeMap::new();
        for (key, value) in &index {
            if key.starts_with('!') {
                continue;
            }
            ensure!(
                key.len() == 16 && key.bytes().all(|b| b.is_ascii_hexdigit()),
                "{}: key {key} is not a 16 hex digit file hash",
                index_path.display()
            );
            let human = value
                .as_str()
                .with_context(|| format!("{}: {key} is not a string", index_path.display()))?;
            listed.insert(key.clone(), human.to_string());
        }
        let mut present = BTreeSet::new();
        for file in sorted_children(&dir)? {
            let fname = file_name(&file)?;
            if fname == "index.json" {
                continue;
            }
            let hash = fname
                .strip_suffix(".xdr")
                .with_context(|| format!("unexpected file {}", file.display()))?;
            present.insert(hash.to_string());
        }
        let listed_keys: BTreeSet<String> = listed.keys().cloned().collect();
        ensure!(
            listed_keys == present,
            "{}: index lists {:?} without a file and has files {:?} it does not list",
            index_path.display(),
            listed_keys.difference(&present).collect::<Vec<_>>(),
            present.difference(&listed_keys).collect::<Vec<_>>()
        );
        set.dirs.push(GoldenDir {
            name: name.clone(),
            index_path: format!("{tier_name}/{name}/index.json"),
            index_sha256: sha256_hex(&index_bytes),
            stamps: stamps.clone(),
        });
        for (hash, human) in listed {
            let rel = format!("{tier_name}/{name}/{hash}.xdr");
            let bytes =
                std::fs::read(core_src.join(&rel)).with_context(|| format!("reading {rel}"))?;
            let (test, section) = match human.split_once('|') {
                Some((t, s)) => (t.to_string(), s.to_string()),
                None => (human.clone(), String::new()),
            };
            let file_idx = set.files.len();
            let mut records = Vec::new();
            for (i, record) in FramedReader::new(bytes.as_slice(), MAX_RECORD).enumerate() {
                let record = record.with_context(|| format!("{rel}: framing"))?;
                let limits = decode_limits(LCM_TYPE, record.len());
                xdr::roundtrip_check(LCM_TYPE, &record, limits.clone())
                    .with_context(|| format!("{rel} record {i}"))?;
                let lcm = decode_lcm(&record)?;
                let signature = schema
                    .signature(&serde_json::to_value(&lcm)?)
                    .with_context(|| format!("{rel} record {i}: signature"))?;
                records.push(GoldenRecord {
                    ledger_seq: ledger_seq(&lcm),
                    ledger_version: ledger_version(&lcm),
                    lcm_version: lcm_version(&lcm),
                    bytes: record.len(),
                    sha256: sha256_hex(&record),
                });
                set.lcms.push(GoldenLcm {
                    file: file_idx,
                    record: i,
                    bytes: record,
                    signature,
                    tx_meta_version: tx_meta_version(&lcm),
                });
            }
            ensure!(!records.is_empty(), "{rel} holds no ledgers");
            let seqs = records.iter().map(|r| r.ledger_seq);
            let ledger_seqs = [seqs.clone().min().unwrap_or(0), seqs.max().unwrap_or(0)];
            set.files.push(GoldenFile {
                path: rel,
                dir: name.clone(),
                sha256: sha256_hex(&bytes),
                bytes: bytes.len(),
                test,
                section,
                stamps: stamps.clone(),
                network_passphrase: GOLDEN_PASSPHRASE.to_string(),
                ledgers: records.len(),
                ledger_seqs,
                records,
                imported_entries: Vec::new(),
            });
        }
    }
    ensure!(
        !set.files.is_empty(),
        "{} holds no golden files",
        tier.display()
    );
    Ok(set)
}

fn stamps(index: &serde_json::Map<String, Value>) -> Result<Stamps> {
    let get = |k: &str| index.get(k).with_context(|| format!("missing {k:?}"));
    let protocol = get(STAMP_PROTOCOL)?
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .context("protocol stamp is not a u32")?;
    let rng_seed = get(STAMP_SEED)?
        .as_u64()
        .context("rng seed is not a number")?;
    let test_all_versions = get(STAMP_ALL_VERSIONS)?
        .as_bool()
        .context("all-versions stamp is not a bool")?;
    let versions_to_test = get(STAMP_VERSIONS)?
        .as_array()
        .context("versions stamp is not an array")?
        .iter()
        .map(|v| v.as_u64().and_then(|v| u32::try_from(v).ok()))
        .collect::<Option<Vec<u32>>>()
        .context("versions stamp holds a non-u32")?;
    Ok(Stamps {
        protocol,
        rng_seed,
        test_all_versions,
        versions_to_test,
    })
}

/// One golden ledger chosen for a family, with the fragments it is the smallest carrier of.
#[derive(Debug, Clone)]
pub struct Selection {
    pub family: String,
    pub lcm: usize,
    pub fragments: Vec<String>,
}

/// A family fragment that only golden ledgers above [`MAX_IMPORT_BYTES`] carry.
#[derive(Debug, Clone, Serialize)]
pub struct NotImported {
    pub family: String,
    pub fragment: String,
    pub path: String,
    pub record: usize,
    pub bytes: usize,
    pub reason: String,
}

/// For every family fragment, the smallest golden ledger whose signature matches it (ties
/// broken by file path and record), among ledgers of at most [`MAX_IMPORT_BYTES`] whose
/// protocol lies in the family's `protocol_valid` range. A ledger chosen for several fragments
/// of one family is one selection. Families no golden matches get no selection; fragments
/// matched only by larger ledgers, or only by ledgers below the family's range, are returned as
/// not imported.
pub fn select(set: &GoldenSet, registry: &Registry) -> Result<(Vec<Selection>, Vec<NotImported>)> {
    let families = registry
        .families
        .as_ref()
        .context("registry has no families.toml")?;
    let mut out = Vec::new();
    let mut skipped = Vec::new();
    // a family without caps holds vectors of the xdr definitions, not a protocol shape, so no
    // golden ledger is imported into it
    for family in families.families.iter().filter(|f| !f.caps.is_empty()) {
        let mut chosen: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        for fragment in &family.signature {
            let smallest = |in_range_only: bool| {
                set.lcms
                    .iter()
                    .enumerate()
                    .filter(|(_, l)| {
                        !in_range_only || in_family_range(family, ledger_version_of(set, l))
                    })
                    .filter(|(_, l)| fragment_matches(&l.signature, fragment))
                    .min_by(|(_, a), (_, b)| {
                        a.bytes
                            .len()
                            .cmp(&b.bytes.len())
                            .then_with(|| set.files[a.file].path.cmp(&set.files[b.file].path))
                            .then_with(|| a.record.cmp(&b.record))
                    })
            };
            // a ledger below the family's range would carry the family's cap tags at a protocol
            // where those caps did not exist yet, so such ledgers are only reported
            let best = smallest(true);
            if best.is_none()
                && let Some((_, l)) = smallest(false)
            {
                skipped.push(NotImported {
                    family: family.id.clone(),
                    fragment: fragment.clone(),
                    path: set.files[l.file].path.clone(),
                    record: l.record,
                    bytes: l.bytes.len(),
                    reason: format!(
                        "only golden ledgers outside the family's protocol range carry it (smallest at protocol {})",
                        ledger_version_of(set, l)
                    ),
                });
                continue;
            }
            match best {
                Some((i, l)) if l.bytes.len() <= MAX_IMPORT_BYTES => {
                    chosen.entry(i).or_default().push(fragment.clone());
                }
                Some((_, l)) => skipped.push(NotImported {
                    family: family.id.clone(),
                    fragment: fragment.clone(),
                    path: set.files[l.file].path.clone(),
                    record: l.record,
                    bytes: l.bytes.len(),
                    reason: format!(
                        "the smallest golden ledger carrying it is above the {MAX_IMPORT_BYTES} byte import limit"
                    ),
                }),
                None => {}
            }
        }
        for (lcm, fragments) in chosen {
            out.push(Selection {
                family: family.id.clone(),
                lcm,
                fragments,
            });
        }
    }
    Ok((out, skipped))
}

fn ledger_version_of(set: &GoldenSet, lcm: &GoldenLcm) -> u32 {
    set.files[lcm.file].records[lcm.record].ledger_version
}

fn in_family_range(family: &Family, protocol: u32) -> bool {
    protocol >= family.protocol_valid.min && family.protocol_valid.max.is_none_or(|m| protocol <= m)
}

/// The family's caps that exist at `protocol`: a cap whose registry protocol is above it
/// describes nothing the ledger can hold. Caps without a protocol in the registry are kept.
pub fn caps_at(family: &Family, registry: &Registry, protocol: u32) -> Result<Vec<String>> {
    let caps: Vec<String> = family
        .caps
        .iter()
        .filter(|id| {
            registry
                .cap(id)
                .and_then(|c| c.protocol)
                .is_none_or(|p| p <= protocol)
        })
        .cloned()
        .collect();
    ensure!(
        !caps.is_empty(),
        "family {} has no cap that exists at protocol {protocol}",
        family.id
    );
    Ok(caps)
}

pub fn entry_id(family: &str, file: &GoldenFile, record: usize) -> String {
    let hash = file
        .path
        .rsplit('/')
        .next()
        .and_then(|n| n.strip_suffix(".xdr"))
        .unwrap_or_default();
    format!("{family}.golden-{hash}-{record}")
}

/// What [`import`] wrote.
#[derive(Debug, Default)]
pub struct ImportReport {
    pub removed: Vec<String>,
    pub written: BTreeMap<String, Vec<String>>,
    pub not_imported: Vec<NotImported>,
    pub index_path: PathBuf,
}

/// Writes `corpus/goldens/core-<tag>/index.json` and one entry per selection, after removing
/// every existing `core-golden` entry so a re-import leaves no stale ones.
pub fn import(set: &mut GoldenSet, registry: &Registry, corpus: &Path) -> Result<ImportReport> {
    let (selections, not_imported) = select(set, registry)?;
    let mut report = ImportReport {
        removed: entry_io::remove_entries_of_class(corpus, SourceClass::CoreGolden)?,
        not_imported,
        ..Default::default()
    };
    for sel in &selections {
        let family = registry
            .family(&sel.family)
            .with_context(|| format!("unknown family {}", sel.family))?;
        let lcm = &set.lcms[sel.lcm];
        let file = &set.files[lcm.file];
        let id = entry_id(&family.id, file, lcm.record);
        let caps = caps_at(family, registry, file.records[lcm.record].ledger_version)?;
        let entry = golden_entry(set, family, sel, &id, caps)?;
        entry_io::write_xdr_entry(corpus, entry, "any", &lcm.bytes)?;
        report
            .written
            .entry(family.id.clone())
            .or_default()
            .push(id.clone());
        set.files[lcm.file].imported_entries.push(id);
    }
    for f in &mut set.files {
        f.imported_entries.sort();
    }
    let dir = corpus.join("goldens").join(format!("core-{}", set.tag));
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    report.index_path = dir.join("index.json");
    std::fs::write(
        &report.index_path,
        render_json(&index_json(set, &report.not_imported)?),
    )
    .with_context(|| format!("writing {}", report.index_path.display()))?;
    Ok(report)
}

fn golden_entry(
    set: &GoldenSet,
    family: &Family,
    sel: &Selection,
    id: &str,
    caps: Vec<String>,
) -> Result<Entry> {
    let lcm = &set.lcms[sel.lcm];
    let file = &set.files[lcm.file];
    let rec = &file.records[lcm.record];
    let section = if file.section.is_empty() {
        String::new()
    } else {
        format!(", section \"{}\"", file.section)
    };
    Ok(Entry {
        obligations: Vec::new(),
        id: id.to_string(),
        kind: EntryKind::Xdr,
        family: family.id.clone(),
        caps,
        protocol: EntryProtocol {
            introduced: family.protocol_introduced,
            generated_at: Some(rec.ledger_version),
            valid_min: family.protocol_valid.min,
            valid_max: family.protocol_valid.max,
        },
        xdr_type: Some(LCM_TYPE.to_string()),
        lcm_version: Some(rec.lcm_version),
        tx_meta_version: lcm.tx_meta_version,
        source: Source {
            recipe_ir: None,
            class: SourceClass::CoreGolden,
            recipe: None,
            chain: None,
            ledger_seq: Some(rec.ledger_seq),
            tx_hash: None,
            tx_index: None,
            upstream: Some(Upstream {
                repo: CORE_REPO.to_string(),
                git_ref: set.tag.clone(),
                path: file.path.clone(),
                sha256: file.sha256.clone(),
            }),
            provenance_url: Some(format!("{CORE_REPO}/blob/{}/{}", set.commit, file.path)),
            normalization: None,
            core_rejection: None,
        },
        synthetic: false,
        // ledger and transaction hashes are bound to the test network id
        passphrase_dependent: true,
        variants: Variants {
            any: Some(Variant {
                network_passphrase: Some(GOLDEN_PASSPHRASE.to_string()),
                xdr_file: String::new(),
                xdr_sha256: String::new(),
                expected_json_file: None,
                expected_json_sha256: None,
                oracle_file: None,
                oracle_sha256: None,
            }),
            ..Default::default()
        },
        vector: None,
        expected: Expected {
            validity: Validity::Valid,
            decode: DecodeExpectation::MustAccept,
            roundtrip: true,
        },
        flags: Vec::new(),
        fields_exercised: Vec::new(),
        linked_issues: Vec::new(),
        notes: Some(format!(
            "core test \"{}\"{section}, ledger {} (record {} of {} in the upstream file). smallest golden ledger matching {}",
            file.test,
            rec.ledger_seq,
            lcm.record + 1,
            file.records.len(),
            sel.fragments.join(", ")
        )),
    })
}

fn index_json(set: &GoldenSet, not_imported: &[NotImported]) -> Result<Value> {
    #[derive(Serialize)]
    struct Index<'a> {
        upstream: UpstreamRef<'a>,
        network_passphrase: &'a str,
        check_lcm: &'a str,
        max_import_bytes: usize,
        directories: &'a [GoldenDir],
        files: &'a [GoldenFile],
        not_imported: &'a [NotImported],
        totals: Totals,
    }
    #[derive(Serialize)]
    struct UpstreamRef<'a> {
        repo: &'a str,
        tag: &'a str,
        commit: &'a str,
        tier: &'a str,
    }
    #[derive(Serialize)]
    struct Totals {
        directories: usize,
        files: usize,
        bytes: usize,
        ledgers: usize,
        imported_entries: usize,
    }
    let index = Index {
        upstream: UpstreamRef {
            repo: CORE_REPO,
            tag: &set.tag,
            commit: &set.commit,
            tier: &set.tier,
        },
        network_passphrase: GOLDEN_PASSPHRASE,
        check_lcm: CHECK_LCM_ARGS,
        max_import_bytes: MAX_IMPORT_BYTES,
        directories: &set.dirs,
        files: &set.files,
        not_imported,
        totals: Totals {
            directories: set.dirs.len(),
            files: set.files.len(),
            bytes: set.files.iter().map(|f| f.bytes).sum(),
            ledgers: set.lcms.len(),
            imported_entries: set.files.iter().map(|f| f.imported_entries.len()).sum(),
        },
    };
    Ok(serde_json::to_value(index)?)
}

/// Compares a fresh scan with a committed golden index: the commit, every directory's
/// `index.json` hash and every file's path and hash must be identical. Returns the differences.
pub fn verify_index(set: &GoldenSet, committed: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let commit = committed
        .pointer("/upstream/commit")
        .and_then(Value::as_str);
    if commit != Some(set.commit.as_str()) {
        out.push(format!(
            "index records commit {commit:?}, the checkout is {}",
            set.commit
        ));
    }
    let tier = committed.pointer("/upstream/tier").and_then(Value::as_str);
    if tier != Some(set.tier.as_str()) {
        out.push(format!(
            "index records tier {tier:?}, the scan read {}",
            set.tier
        ));
    }
    let pairs = |key: &str, path_key: &str, hash_key: &str| -> BTreeMap<String, String> {
        committed
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| {
                Some((
                    v.get(path_key)?.as_str()?.to_string(),
                    v.get(hash_key)?.as_str()?.to_string(),
                ))
            })
            .collect()
    };
    let want_files = pairs("files", "path", "sha256");
    let want_dirs = pairs("directories", "index_path", "index_sha256");
    let have_files: BTreeMap<String, String> = set
        .files
        .iter()
        .map(|f| (f.path.clone(), f.sha256.clone()))
        .collect();
    let have_dirs: BTreeMap<String, String> = set
        .dirs
        .iter()
        .map(|d| (d.index_path.clone(), d.index_sha256.clone()))
        .collect();
    for (what, want, have) in [
        ("file", want_files, have_files),
        ("index", want_dirs, have_dirs),
    ] {
        for (path, hash) in &want {
            match have.get(path) {
                None => out.push(format!(
                    "{what} {path} is in the committed index but not upstream"
                )),
                Some(h) if h != hash => out.push(format!(
                    "{what} {path} has sha256 {h} upstream, the committed index says {hash}"
                )),
                Some(_) => {}
            }
        }
        for path in have.keys().filter(|p| !want.contains_key(*p)) {
            out.push(format!(
                "{what} {path} is upstream but not in the committed index"
            ));
        }
    }
    out
}

// structural agreement

/// For one entry: per operation type of every successful transaction, the set of
/// `<change>:<ledger entry type>` kinds its operation meta holds, for example
/// `account_merge -> {removed:account, state:account, updated:account}`.
pub type OpChangeKinds = BTreeMap<String, BTreeSet<String>>;

/// One successful operation: its type and the sorted multiset of its change kinds.
pub type Observation = (String, Vec<String>);

pub fn op_change_kinds(lcm: &LedgerCloseMeta) -> Result<OpChangeKinds> {
    let mut out = OpChangeKinds::new();
    for (op, kinds) in ledger_observations(lcm)? {
        out.entry(op).or_default().extend(kinds);
    }
    Ok(out)
}

/// The distinct (operation type, change-kind multiset) pairs of every successful operation in
/// a ledger.
pub fn ledger_observations(lcm: &LedgerCloseMeta) -> Result<BTreeSet<Observation>> {
    let results: Vec<(&TransactionResultResult, &TransactionMeta)> = match lcm {
        LedgerCloseMeta::V0(v) => v
            .tx_processing
            .iter()
            .map(|t| (&t.result.result.result, &t.tx_apply_processing))
            .collect(),
        LedgerCloseMeta::V1(v) => v
            .tx_processing
            .iter()
            .map(|t| (&t.result.result.result, &t.tx_apply_processing))
            .collect(),
        LedgerCloseMeta::V2(v) => v
            .tx_processing
            .iter()
            .map(|t| (&t.result.result.result, &t.tx_apply_processing))
            .collect(),
    };
    observations_of(results)
}

/// [`ledger_observations`] over (result, apply meta) pairs, for ledgers and transaction slices.
pub fn observations_of<'a>(
    results: impl IntoIterator<Item = (&'a TransactionResultResult, &'a TransactionMeta)>,
) -> Result<BTreeSet<Observation>> {
    let mut out = BTreeSet::new();
    for (result, meta) in results {
        let op_results = match result {
            TransactionResultResult::TxSuccess(ops) => ops,
            TransactionResultResult::TxFeeBumpInnerSuccess(inner) => match &inner.result.result {
                InnerTransactionResultResult::TxSuccess(ops) => ops,
                _ => continue,
            },
            _ => continue,
        };
        let op_changes = operation_changes(meta);
        ensure!(
            op_changes.len() == op_results.len(),
            "a successful transaction has {} operation results but {} operation metas",
            op_results.len(),
            op_changes.len()
        );
        for (op, changes) in op_results.iter().zip(op_changes) {
            let OperationResult::OpInner(tr) = op else {
                bail!(
                    "a successful transaction holds operation result {}",
                    op.name()
                );
            };
            let mut kinds: Vec<String> = changes.iter().map(change_kind).collect();
            kinds.sort();
            out.insert((snake(tr.name()), kinds));
        }
    }
    Ok(out)
}

fn operation_changes(meta: &TransactionMeta) -> Vec<&LedgerEntryChanges> {
    match meta {
        TransactionMeta::V0(ops) => ops.iter().map(|o| &o.changes).collect(),
        TransactionMeta::V1(m) => m.operations.iter().map(|o| &o.changes).collect(),
        TransactionMeta::V2(m) => m.operations.iter().map(|o| &o.changes).collect(),
        TransactionMeta::V3(m) => m.operations.iter().map(|o| &o.changes).collect(),
        TransactionMeta::V4(m) => m.operations.iter().map(|o| &o.changes).collect(),
    }
}

fn change_kind(change: &LedgerEntryChange) -> String {
    let (kind, entry_type) = match change {
        LedgerEntryChange::Created(e) => ("created", e.data.name()),
        LedgerEntryChange::Updated(e) => ("updated", e.data.name()),
        LedgerEntryChange::Removed(k) => ("removed", k.name()),
        LedgerEntryChange::State(e) => ("state", e.data.name()),
        LedgerEntryChange::Restored(e) => ("restored", e.data.name()),
    };
    format!("{kind}:{}", snake(entry_type))
}

/// A golden (operation type, change kinds) combination accepted without a local or history
/// match, from `corpus/goldens/agree-exceptions.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgreeException {
    /// Golden entry id.
    pub golden: String,
    pub op: String,
    /// The change-kind multiset, in any order.
    pub kinds: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExceptionsFile {
    #[serde(default, rename = "exception")]
    exceptions: Vec<AgreeException>,
}

pub const EXCEPTIONS_FILE: &str = "goldens/agree-exceptions.toml";

/// Reads the exceptions file; a missing file means no exceptions. Every exception needs a
/// reason.
pub fn load_exceptions(path: &Path) -> Result<Vec<AgreeException>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let file: ExceptionsFile =
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    for e in &file.exceptions {
        ensure!(
            !e.reason.trim().is_empty(),
            "{}: the exception for {} {} has no reason",
            path.display(),
            e.golden,
            e.op
        );
    }
    Ok(file.exceptions)
}

/// A golden combination checked against the family's local-core and history entries.
#[derive(Debug, Clone)]
pub struct GoldenCombination {
    pub golden: String,
    pub op: String,
    pub kinds: Vec<String>,
    /// Entries observing the same combination; empty when unmatched.
    pub matched_by: Vec<String>,
    /// The reason from the exceptions file, when unmatched and excepted.
    pub excepted: Option<String>,
}

impl GoldenCombination {
    pub fn failed(&self) -> bool {
        self.matched_by.is_empty() && self.excepted.is_none()
    }
}

#[derive(Debug, Clone)]
pub struct FamilyAgreement {
    pub family: String,
    pub goldens: Vec<String>,
    pub others: Vec<(String, SourceClass)>,
    /// Operation types with a successful operation in both a golden and another entry.
    pub shared_ops: Vec<String>,
    pub combinations: Vec<GoldenCombination>,
    /// Combinations of shared operation types that only local-core or history entries show;
    /// informational.
    pub local_only: Vec<(String, Vec<String>, String)>,
    /// Family fragments matched by some golden and by some other entry.
    pub shared_fragments: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AgreeReport {
    pub families: Vec<FamilyAgreement>,
    pub golden_only_families: Vec<String>,
    /// Exceptions naming a golden combination that does not exist (a typo or a removed
    /// golden); these fail.
    pub unknown_exceptions: Vec<AgreeException>,
    /// Exceptions whose combination is now matched by a local or history entry; they can be
    /// deleted. Informational.
    pub stale_exceptions: Vec<AgreeException>,
    /// Exceptions naming a golden combination that is not compared at all: its family has
    /// no local-core or history entry, or none of them performs the operation. They are not
    /// stale, since nothing matches them; they excuse nothing today. Informational.
    pub inapplicable_exceptions: Vec<AgreeException>,
}

impl AgreeReport {
    pub fn failures(&self) -> usize {
        self.unknown_exceptions.len()
            + self
                .families
                .iter()
                .flat_map(|f| &f.combinations)
                .filter(|c| c.failed())
                .count()
    }
}

/// One comparable item: a whole ledger variant, or a transaction's result meta (with the
/// signature of its envelope entry when the corpus has one).
struct Unit {
    entry: String,
    id: String,
    class: SourceClass,
    signature: BTreeSet<String>,
    observations: BTreeSet<Observation>,
}

const SLICE_META_TYPES: [&str; 2] = ["TransactionResultMeta", "TransactionResultMetaV1"];

/// The structural agreement check. Change kinds depend on pre-state (an AllowTrust revoke
/// removes offers only when the holder has some), so entries are not compared pairwise.
/// Instead, per family and per operation type with a successful operation in both a golden and
/// a local-core or history entry, every distinct (operation type, change-kind multiset) that
/// the goldens show must be shown by at least one local-core or history entry of the family,
/// or be listed with a reason in the exceptions. Combinations only the local side shows are
/// reported, not failed.
pub fn agree(
    manifest: &Manifest,
    registry: &Registry,
    root: &Path,
    exceptions: &[AgreeException],
) -> Result<AgreeReport> {
    let mut by_family: BTreeMap<&str, (Vec<&Entry>, Vec<&Entry>)> = BTreeMap::new();
    for e in &manifest.entries {
        if e.kind != EntryKind::Xdr {
            continue;
        }
        let slot = by_family.entry(e.family.as_str()).or_default();
        match e.source.class {
            SourceClass::CoreGolden => slot.0.push(e),
            SourceClass::LocalCore
            | SourceClass::LocalCoreRefused
            | SourceClass::TestnetHistory
            | SourceClass::MainnetHistory => slot.1.push(e),
            SourceClass::Synthetic | SourceClass::CoreTestDump => {}
        }
    }
    let mut report = AgreeReport::default();
    let mut used = vec![false; exceptions.len()];
    let mut known = vec![false; exceptions.len()];
    // the exception's combination was compared and some local or history entry shows it
    let mut matched = vec![false; exceptions.len()];
    for (family_id, (goldens, others)) in by_family {
        let goldens = units(&goldens, &goldens, root)?;
        if goldens.is_empty() {
            continue;
        }
        let others = units(&others, &others, root)?;
        let golden_ops: BTreeSet<&str> = goldens
            .iter()
            .flat_map(|u| u.observations.iter().map(|(op, _)| op.as_str()))
            .collect();
        let other_ops: BTreeSet<&str> = others
            .iter()
            .flat_map(|u| u.observations.iter().map(|(op, _)| op.as_str()))
            .collect();
        for (i, x) in exceptions.iter().enumerate() {
            let mut kinds = x.kinds.clone();
            kinds.sort();
            if goldens.iter().any(|g| {
                g.entry == x.golden && g.observations.contains(&(x.op.clone(), kinds.clone()))
            }) {
                known[i] = true;
            }
        }
        if others.is_empty() {
            report.golden_only_families.push(family_id.to_string());
            continue;
        }
        let family = registry
            .family(family_id)
            .with_context(|| format!("unknown family {family_id}"))?;
        let shared_ops: Vec<String> = golden_ops
            .intersection(&other_ops)
            .map(|s| s.to_string())
            .collect();

        let mut combinations: BTreeMap<(String, Observation), GoldenCombination> = BTreeMap::new();
        for g in &goldens {
            for obs in &g.observations {
                if !shared_ops.contains(&obs.0) {
                    continue;
                }
                let key = (g.entry.clone(), obs.clone());
                if combinations.contains_key(&key) {
                    continue;
                }
                let matched_by: Vec<String> = others
                    .iter()
                    .filter(|o| o.observations.contains(obs))
                    .map(|o| o.id.clone())
                    .collect();
                let mut excepted = None;
                for (i, x) in exceptions.iter().enumerate() {
                    let mut kinds = x.kinds.clone();
                    kinds.sort();
                    if x.golden == g.entry && x.op == obs.0 && kinds == obs.1 {
                        if matched_by.is_empty() {
                            used[i] = true;
                            excepted = Some(x.reason.clone());
                        } else {
                            matched[i] = true;
                        }
                    }
                }
                combinations.insert(
                    key,
                    GoldenCombination {
                        golden: g.entry.clone(),
                        op: obs.0.clone(),
                        kinds: obs.1.clone(),
                        matched_by,
                        excepted,
                    },
                );
            }
        }
        let golden_obs: BTreeSet<&Observation> =
            goldens.iter().flat_map(|g| &g.observations).collect();
        let mut local_only = Vec::new();
        for o in &others {
            for obs in &o.observations {
                if shared_ops.contains(&obs.0) && !golden_obs.contains(obs) {
                    local_only.push((obs.0.clone(), obs.1.clone(), o.id.clone()));
                }
            }
        }
        let shared_fragments = family
            .signature
            .iter()
            .filter(|f| {
                goldens.iter().any(|g| fragment_matches(&g.signature, f))
                    && others.iter().any(|o| fragment_matches(&o.signature, f))
            })
            .cloned()
            .collect();
        report.families.push(FamilyAgreement {
            family: family_id.to_string(),
            goldens: dedup(goldens.iter().map(|g| g.entry.clone())),
            others: {
                let mut v: Vec<(String, SourceClass)> =
                    others.iter().map(|o| (o.entry.clone(), o.class)).collect();
                v.dedup();
                v
            },
            shared_ops,
            combinations: combinations.into_values().collect(),
            local_only,
            shared_fragments,
        });
    }
    for (i, x) in exceptions.iter().enumerate() {
        if !known[i] {
            report.unknown_exceptions.push(x.clone());
        } else if matched[i] {
            report.stale_exceptions.push(x.clone());
        } else if !used[i] {
            report.inapplicable_exceptions.push(x.clone());
        }
    }
    Ok(report)
}

fn dedup(items: impl Iterator<Item = String>) -> Vec<String> {
    let mut v: Vec<String> = items.collect();
    v.dedup();
    v
}

// a ledger entry gives one unit per variant. a result meta entry gives one unit per variant,
// and the envelope entry of the same transaction, when the corpus holds one, adds its
// signature. a sibling envelope is not required: recipes may capture only the meta, and the
// change kinds come from the meta alone. envelope entries are read only through their meta;
// other xdr types are not compared
fn units(entries: &[&Entry], siblings: &[&Entry], root: &Path) -> Result<Vec<Unit>> {
    let mut out = Vec::new();
    for e in entries {
        let xdr_type = e.xdr_type.as_deref().unwrap_or_default();
        if xdr_type == LCM_TYPE {
            for (variant, bytes) in variant_bytes(e, root)? {
                let lcm = decode_lcm(&bytes).with_context(|| e.id.clone())?;
                out.push(Unit {
                    entry: e.id.clone(),
                    id: format!("{} ({variant})", e.id),
                    class: e.source.class,
                    signature: shape_signature(
                        LCM_TYPE,
                        &bytes,
                        decode_limits(LCM_TYPE, bytes.len()),
                    )?,
                    observations: ledger_observations(&lcm)?,
                });
            }
        } else if SLICE_META_TYPES.contains(&xdr_type) {
            let envelope = siblings.iter().find(|s| {
                s.xdr_type.as_deref() == Some("TransactionEnvelope")
                    && s.family == e.family
                    && s.source.ledger_seq == e.source.ledger_seq
                    && same_transaction(&s.source, &e.source)
            });
            let envelopes = match envelope {
                Some(env) => variant_bytes(env, root)?,
                None => Vec::new(),
            };
            for (variant, bytes) in variant_bytes(e, root)? {
                let mut signature =
                    shape_signature(xdr_type, &bytes, decode_limits(xdr_type, bytes.len()))?;
                if let Some((_, env)) = envelopes.iter().find(|(v, _)| *v == variant) {
                    signature.extend(shape_signature(
                        "TransactionEnvelope",
                        env,
                        decode_limits("TransactionEnvelope", env.len()),
                    )?);
                }
                let observations =
                    match xdr::decode(xdr_type, &bytes, decode_limits(xdr_type, bytes.len()))? {
                        stellar_xdr::Type::TransactionResultMeta(m) => {
                            observations_of([(&m.result.result.result, &m.tx_apply_processing)])?
                        }
                        stellar_xdr::Type::TransactionResultMetaV1(m) => {
                            observations_of([(&m.result.result.result, &m.tx_apply_processing)])?
                        }
                        other => bail!("{} decoded as {}", e.id, other.name()),
                    };
                out.push(Unit {
                    entry: e.id.clone(),
                    id: format!("{} ({variant})", e.id),
                    class: e.source.class,
                    signature,
                    observations,
                });
            }
        }
    }
    Ok(out)
}

// history entries name their transaction by hash. local-core entries carry no hash, because it
// covers the network id and differs between the variants, so they pair by chain and position
fn same_transaction(a: &Source, b: &Source) -> bool {
    match (&a.tx_hash, &b.tx_hash) {
        (Some(x), Some(y)) => x == y,
        (None, None) => {
            a.chain.is_some()
                && a.chain == b.chain
                && a.tx_index.is_some()
                && a.tx_index == b.tx_index
        }
        _ => false,
    }
}

fn variant_bytes(entry: &Entry, root: &Path) -> Result<Vec<(&'static str, Vec<u8>)>> {
    entry
        .variants
        .iter()
        .map(|(name, v)| {
            let path = root.join(&v.xdr_file);
            let bytes =
                std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
            Ok((name, bytes))
        })
        .collect()
}

// shared xdr helpers

pub fn decode_lcm(bytes: &[u8]) -> Result<LedgerCloseMeta> {
    match xdr::decode(LCM_TYPE, bytes, decode_limits(LCM_TYPE, bytes.len()))? {
        stellar_xdr::Type::LedgerCloseMeta(lcm) => Ok(*lcm),
        other => bail!("decoded {} instead of LedgerCloseMeta", other.name()),
    }
}

pub fn ledger_seq(lcm: &LedgerCloseMeta) -> u32 {
    header(lcm).ledger_seq
}

pub fn ledger_version(lcm: &LedgerCloseMeta) -> u32 {
    header(lcm).ledger_version
}

pub fn header(lcm: &LedgerCloseMeta) -> &stellar_xdr::LedgerHeader {
    match lcm {
        LedgerCloseMeta::V0(v) => &v.ledger_header.header,
        LedgerCloseMeta::V1(v) => &v.ledger_header.header,
        LedgerCloseMeta::V2(v) => &v.ledger_header.header,
    }
}

pub fn lcm_version(lcm: &LedgerCloseMeta) -> u32 {
    match lcm {
        LedgerCloseMeta::V0(_) => 0,
        LedgerCloseMeta::V1(_) => 1,
        LedgerCloseMeta::V2(_) => 2,
    }
}

/// The TransactionMeta version shared by every transaction, `None` for an empty ledger.
/// Core emits one version per ledger, so mixed versions are reported as the highest.
pub fn tx_meta_version(lcm: &LedgerCloseMeta) -> Option<u32> {
    let version = |m: &TransactionMeta| match m {
        TransactionMeta::V0(_) => 0,
        TransactionMeta::V1(_) => 1,
        TransactionMeta::V2(_) => 2,
        TransactionMeta::V3(_) => 3,
        TransactionMeta::V4(_) => 4,
    };
    match lcm {
        LedgerCloseMeta::V0(v) => v
            .tx_processing
            .iter()
            .map(|t| version(&t.tx_apply_processing))
            .max(),
        LedgerCloseMeta::V1(v) => v
            .tx_processing
            .iter()
            .map(|t| version(&t.tx_apply_processing))
            .max(),
        LedgerCloseMeta::V2(v) => v
            .tx_processing
            .iter()
            .map(|t| version(&t.tx_apply_processing))
            .max(),
    }
}

// the crate's variant names are CamelCase; manifests and reports use XDR-JSON's snake case
pub(crate) fn snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn sorted_children(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| format!("listing {}", dir.display()))?;
    out.sort();
    Ok(out)
}

fn file_name(path: &Path) -> Result<String> {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
        .with_context(|| format!("{} has no utf-8 file name", path.display()))
}
