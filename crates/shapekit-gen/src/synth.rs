//! `shapekit synth build`: hand-crafted corpus entries. Every entry written here is
//! `synthetic = true` with `source.class = synthetic`, and every expectation is computed by a
//! reference implementation at build time (stellar-xdr, stellar-strkey, soroban-env-host),
//! never typed in by hand. The build refuses to write a vector the reference disagrees with.
//!
//! Output, relative to the corpus directory:
//! - `kind = xdr`: `entries/<family>/<id>/entry.json`, `any.xdr` and, for entries that must
//!   decode, `any.expected.json`.
//! - every other kind: `vectors/<kind>/<id>.json` holding `{entry, cases}`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use shapekit_manifest::build::{ENTRIES_DIR, ENTRY_FILE, VECTORS_DIR};
use shapekit_manifest::{
    DECODE_DEPTH, DecodeExpectation, Entry, EntryKind, EntryProtocol, Expected, Limits, Source,
    Validity, Variant, Variants, VectorFile, sha256_hex, xdr,
};
use shapekit_registry::{Registry, SourceClass};

use crate::{synth_native, synth_order, synth_strkey, synth_textid, synth_xdr};

/// Every id this module writes starts with `<family>.synth-`, so synthetic entries never
/// collide with recipe, golden or history entries of the same family.
pub const SLUG_PREFIX: &str = "synth-";

/// Deep values (nesting past a thousand decode levels) are built, encoded, decoded and
/// rendered recursively, which needs more stack than a default thread has.
const STACK_BYTES: usize = 256 * 1024 * 1024;

pub const LINK_JS_1766: &str = "https://github.com/stellar/js-stellar-sdk/pull/1766";
pub const LINK_JS_1785: &str = "https://github.com/stellar/js-stellar-sdk/issues/1785";
pub const LINK_JS_1789: &str = "https://github.com/stellar/js-stellar-sdk/issues/1789";
pub const LINK_PY_1246: &str = "https://github.com/StellarCN/py-stellar-base/issues/1246";
pub const LINK_DOTNET_257: &str = "https://github.com/Beans-BV/dotnet-stellar-sdk/issues/257";
pub const LINK_RFC_8259: &str = "https://www.rfc-editor.org/rfc/rfc8259#section-6";
pub const LINK_RS_581: &str = "https://github.com/stellar/rs-stellar-xdr/pull/581";
pub const LINK_GHSA_3XHM: &str =
    "https://github.com/stellar/rs-stellar-xdr/security/advisories/GHSA-3xhm-p452-9wjh";

/// The manifest record of one synthetic entry before its files are known.
#[derive(Debug, Clone)]
pub struct Spec {
    pub family: &'static str,
    pub slug: String,
    pub kind: EntryKind,
    pub validity: Validity,
    pub decode: DecodeExpectation,
    pub flags: Vec<&'static str>,
    pub linked_issues: Vec<&'static str>,
    pub fields: Vec<&'static str>,
    /// The family's caps when `None`.
    pub caps: Option<Vec<&'static str>>,
    /// The family's protocol range when `None`.
    pub valid_min: Option<u32>,
    pub notes: String,
}

impl Spec {
    pub fn new(family: &'static str, slug: impl Into<String>, kind: EntryKind) -> Spec {
        Spec {
            family,
            slug: slug.into(),
            kind,
            validity: Validity::Valid,
            decode: DecodeExpectation::MustAccept,
            flags: Vec::new(),
            linked_issues: Vec::new(),
            fields: Vec::new(),
            caps: None,
            valid_min: None,
            notes: String::new(),
        }
    }

    pub fn id(&self) -> String {
        format!("{}.{SLUG_PREFIX}{}", self.family, self.slug)
    }

    pub fn implementation_defined(mut self, validity: Validity) -> Spec {
        self.decode = DecodeExpectation::ImplementationDefined;
        self.validity = validity;
        self
    }

    pub fn reject(mut self, validity: Validity) -> Spec {
        self.decode = DecodeExpectation::MustReject;
        self.validity = validity;
        self
    }

    pub fn validity(mut self, validity: Validity) -> Spec {
        self.validity = validity;
        self
    }

    pub fn flags(mut self, flags: &[&'static str]) -> Spec {
        self.flags.extend_from_slice(flags);
        self
    }

    pub fn links(mut self, links: &[&'static str]) -> Spec {
        self.linked_issues.extend_from_slice(links);
        self
    }

    pub fn fields(mut self, fields: &[&'static str]) -> Spec {
        self.fields.extend_from_slice(fields);
        self
    }

    pub fn caps(mut self, caps: &[&'static str]) -> Spec {
        self.caps = Some(caps.to_vec());
        self
    }

    pub fn valid_from(mut self, protocol: u32) -> Spec {
        self.valid_min = Some(protocol);
        self
    }

    pub fn notes(mut self, notes: impl Into<String>) -> Spec {
        self.notes = notes.into();
        self
    }

    /// The manifest record, with caps and protocol range taken from the registry family.
    pub fn entry(&self, registry: &Registry, xdr_type: Option<&str>) -> Result<Entry> {
        let family = registry
            .family(self.family)
            .with_context(|| format!("family {} is not in families.toml", self.family))?;
        let caps = match &self.caps {
            Some(c) => c.iter().map(|s| s.to_string()).collect(),
            None => family.caps.clone(),
        };
        let valid_min = self.valid_min.unwrap_or(family.protocol_valid.min);
        let mut flags: Vec<String> = self.flags.iter().map(|s| s.to_string()).collect();
        flags.sort();
        flags.dedup();
        let mut fields: Vec<String> = self.fields.iter().map(|s| s.to_string()).collect();
        fields.sort();
        fields.dedup();
        let mut links: Vec<String> = self.linked_issues.iter().map(|s| s.to_string()).collect();
        links.sort();
        links.dedup();
        Ok(Entry {
            id: self.id(),
            kind: self.kind,
            family: self.family.to_string(),
            caps,
            protocol: EntryProtocol {
                introduced: family.protocol_introduced,
                generated_at: None,
                valid_min,
                valid_max: family.protocol_valid.max,
            },
            xdr_type: xdr_type.map(str::to_string),
            lcm_version: None,
            tx_meta_version: None,
            source: Source {
                class: SourceClass::Synthetic,
                recipe: None,
                chain: None,
                ledger_seq: None,
                tx_hash: None,
                tx_index: None,
                upstream: None,
                provenance_url: None,
                normalization: None,
                core_rejection: None,
            },
            synthetic: true,
            passphrase_dependent: false,
            variants: Variants::default(),
            vector: None,
            expected: Expected {
                validity: self.validity,
                decode: self.decode,
                roundtrip: self.decode == DecodeExpectation::MustAccept,
            },
            flags,
            fields_exercised: fields,
            linked_issues: links,
            notes: (!self.notes.is_empty()).then(|| self.notes.clone()),
        })
    }
}

/// One `kind = xdr` vector before it is checked against stellar-xdr.
pub struct XdrItem {
    pub spec: Spec,
    pub xdr_type: &'static str,
    pub bytes: Vec<u8>,
    /// The `caps-limits.toml` id whose `recommended` value is the decode depth the expectation
    /// holds under. The length limit is always the input length.
    pub depth_limit: &'static str,
}

/// One non-xdr vector: its record and its cases.
pub struct VectorItem {
    pub spec: Spec,
    pub cases: Vec<Value>,
}

/// A checked `kind = xdr` entry ready to write.
#[derive(Debug, Clone)]
pub struct XdrOut {
    pub entry: Entry,
    pub bytes: Vec<u8>,
    pub expected_json: Option<String>,
    pub limits: Limits,
}

/// Everything `synth build` writes, in id order.
#[derive(Debug, Clone)]
pub struct Synth {
    pub xdr: Vec<XdrOut>,
    pub vectors: Vec<VectorFile>,
}

impl Synth {
    /// Entry counts by kind name.
    pub fn counts(&self) -> BTreeMap<&'static str, usize> {
        let mut out = BTreeMap::new();
        for e in self.xdr.iter().map(|x| &x.entry) {
            *out.entry(kind_dir(e.kind)).or_insert(0) += 1;
        }
        for v in &self.vectors {
            *out.entry(kind_dir(v.entry.kind)).or_insert(0) += 1;
        }
        out
    }
}

pub fn kind_dir(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Xdr => "xdr",
        EntryKind::Ordering => "ordering",
        EntryKind::Strkey => "strkey",
        EntryKind::TextualId => "textual-id",
        EntryKind::NativeProjection => "native-projection",
    }
}

/// The decode limits a vector names: depth from `caps-limits.toml`, length the input length.
pub fn limits_for(registry: &Registry, depth_limit: &str, len: usize) -> Result<Limits> {
    let limit = registry
        .limits
        .limits
        .iter()
        .find(|l| l.id == depth_limit)
        .with_context(|| format!("limit {depth_limit} is not in caps-limits.toml"))?;
    ensure!(
        limit.unit == "levels",
        "limit {depth_limit} is in {}, not decode levels",
        limit.unit
    );
    let depth = u32::try_from(limit.recommended)
        .with_context(|| format!("limit {depth_limit} does not fit a decode depth"))?;
    Ok(Limits { depth, len })
}

/// Generates every synthetic entry. Runs on a thread with a large stack.
pub fn generate(registry: &Registry) -> Result<Synth> {
    let registry = registry.clone();
    std::thread::Builder::new()
        .name("synth".into())
        .stack_size(STACK_BYTES)
        .spawn(move || generate_here(&registry))
        .context("starting the synth thread")?
        .join()
        .map_err(|_| anyhow::anyhow!("the synth thread panicked"))?
}

fn generate_here(registry: &Registry) -> Result<Synth> {
    let mut xdr_out = Vec::new();
    for item in synth_xdr::items(registry)? {
        xdr_out.push(check_xdr(registry, item)?);
    }
    let mut vector_items = Vec::new();
    vector_items.extend(synth_order::items()?);
    vector_items.extend(synth_strkey::items()?);
    vector_items.extend(synth_textid::items()?);
    vector_items.extend(synth_native::items()?);
    let mut vectors = Vec::new();
    for item in vector_items {
        ensure!(
            !item.cases.is_empty(),
            "vector {} has no cases",
            item.spec.id()
        );
        vectors.push(VectorFile {
            entry: item.spec.entry(registry, None)?,
            cases: item.cases,
        });
    }
    xdr_out.sort_by(|a, b| a.entry.id.cmp(&b.entry.id));
    vectors.sort_by(|a, b| a.entry.id.cmp(&b.entry.id));
    let mut seen = BTreeSet::new();
    for id in xdr_out
        .iter()
        .map(|x| &x.entry.id)
        .chain(vectors.iter().map(|v| &v.entry.id))
    {
        ensure!(
            seen.insert(id.clone()),
            "synthetic id {id} is generated twice"
        );
    }
    Ok(Synth {
        xdr: xdr_out,
        vectors,
    })
}

/// Holds the vector to its expectation with stellar-xdr 30.0.0: a must-reject vector has to
/// fail to decode under the limits it names, a must-accept vector has to round-trip under the
/// validator's limits and gets its XDR-JSON.
fn check_xdr(registry: &Registry, item: XdrItem) -> Result<XdrOut> {
    let id = item.spec.id();
    let limits = limits_for(registry, item.depth_limit, item.bytes.len())?;
    let mut spec = item.spec;
    let direct = synth_xdr::decode_direct(item.xdr_type, &item.bytes, limits.clone())?;
    let expected_json = match spec.decode {
        DecodeExpectation::MustReject => {
            let err = match direct {
                Ok(()) => bail!(
                    "{id}: stellar-xdr {} decodes this must-reject vector as {} under depth {} and len {}",
                    shapekit_manifest::STELLAR_XDR_VERSION.pkg,
                    item.xdr_type,
                    limits.depth,
                    limits.len
                ),
                Err(err) => err,
            };
            ensure!(
                xdr::decode(item.xdr_type, &item.bytes, limits.clone()).is_err(),
                "{id}: the toolkit decodes a vector stellar-xdr rejects directly"
            );
            let why = format!(
                "stellar-xdr {} rejects it ({}::from_xdr) with {err:?} under Limits {{ depth: {} ({}), len: {} (the input length) }}.",
                shapekit_manifest::STELLAR_XDR_VERSION.pkg,
                item.xdr_type,
                limits.depth,
                item.depth_limit,
                limits.len
            );
            spec.notes = join_notes(&spec.notes, &why);
            None
        }
        DecodeExpectation::MustAccept => {
            if let Err(err) = direct {
                bail!("{id}: stellar-xdr rejects this must-accept vector: {err:?}");
            }
            let accept = Limits {
                depth: DECODE_DEPTH,
                len: item.bytes.len(),
            };
            xdr::roundtrip_check(item.xdr_type, &item.bytes, accept.clone())
                .with_context(|| format!("{id}: round trip under the validator's limits"))?;
            Some(
                xdr::to_xdr_json(item.xdr_type, &item.bytes, accept)
                    .with_context(|| format!("{id}: rendering XDR-JSON"))?,
            )
        }
        DecodeExpectation::ImplementationDefined => {
            // the entry records what the reference does without making it the rule
            let verdict = match (
                direct,
                xdr::roundtrip_check(item.xdr_type, &item.bytes, limits.clone()),
            ) {
                (Err(err), _) => format!("rejects it with {err:?}"),
                (Ok(()), Ok(())) => "decodes it and re-encodes the same bytes".to_string(),
                (Ok(()), Err(err)) => format!("decodes it but does not round-trip ({err})"),
            };
            let why = format!(
                "stellar-xdr {} {verdict} under Limits {{ depth: {} ({}), len: {} }}.",
                shapekit_manifest::STELLAR_XDR_VERSION.pkg,
                limits.depth,
                item.depth_limit,
                limits.len
            );
            spec.notes = join_notes(&spec.notes, &why);
            None
        }
    };
    let mut entry = spec.entry(registry, Some(item.xdr_type))?;
    let dir = entry_dir(&entry);
    entry.variants.any = Some(Variant {
        network_passphrase: None,
        xdr_file: format!("{dir}/any.xdr"),
        xdr_sha256: sha256_hex(&item.bytes),
        expected_json_file: expected_json
            .as_ref()
            .map(|_| format!("{dir}/any.expected.json")),
        expected_json_sha256: expected_json.as_ref().map(|j| sha256_hex(j.as_bytes())),
        oracle_file: None,
        oracle_sha256: None,
    });
    Ok(XdrOut {
        entry,
        bytes: item.bytes,
        expected_json,
        limits,
    })
}

fn join_notes(a: &str, b: &str) -> String {
    if a.is_empty() {
        b.to_string()
    } else {
        format!("{a} {b}")
    }
}

/// `entries/<family>/<id>`, relative to the corpus directory.
pub fn entry_dir(entry: &Entry) -> String {
    format!("{ENTRIES_DIR}/{}/{}", entry.family, entry.id)
}

/// `vectors/<kind>/<id>.json`, relative to the corpus directory.
pub fn vector_path(entry: &Entry) -> String {
    format!("{VECTORS_DIR}/{}/{}.json", kind_dir(entry.kind), entry.id)
}

/// What a write changed.
#[derive(Debug, Default)]
pub struct WriteReport {
    pub written: usize,
    pub removed: Vec<String>,
}

/// Writes `synth` under `corpus` and removes synthetic entries and vectors left over from an
/// earlier build that this build no longer produces. Non-synthetic files are never touched.
pub fn write(corpus: &Path, synth: &Synth) -> Result<WriteReport> {
    let mut report = WriteReport::default();
    let keep: BTreeSet<&str> = synth
        .xdr
        .iter()
        .map(|x| x.entry.id.as_str())
        .chain(synth.vectors.iter().map(|v| v.entry.id.as_str()))
        .collect();
    for (id, path) in existing_synthetic(corpus)? {
        if !keep.contains(id.as_str()) {
            if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            }
            .with_context(|| format!("removing stale {}", path.display()))?;
            report.removed.push(id);
        }
    }
    for x in &synth.xdr {
        let dir = corpus.join(entry_dir(&x.entry));
        if dir.exists() {
            // the directory is rebuilt whole so no file from an older shape of the entry stays
            std::fs::remove_dir_all(&dir).with_context(|| format!("clearing {}", dir.display()))?;
        }
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let variant = x
            .entry
            .variants
            .any
            .as_ref()
            .context("a synthetic xdr entry has its any variant")?;
        write_file(corpus, &variant.xdr_file, &x.bytes)?;
        if let (Some(path), Some(json)) = (&variant.expected_json_file, &x.expected_json) {
            write_file(corpus, path, json.as_bytes())?;
        }
        let entry_json = xdr::render_json(&serde_json::to_value(&x.entry)?);
        write_file(
            corpus,
            &format!("{}/{ENTRY_FILE}", entry_dir(&x.entry)),
            entry_json.as_bytes(),
        )?;
        report.written += 1;
    }
    for v in &synth.vectors {
        let text = xdr::render_json(&serde_json::to_value(v)?);
        write_file(corpus, &vector_path(&v.entry), text.as_bytes())?;
        report.written += 1;
    }
    Ok(report)
}

fn write_file(corpus: &Path, rel: &str, bytes: &[u8]) -> Result<()> {
    let path = corpus.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))
}

// every synthetic entry already on disk, as (id, path to remove)
fn existing_synthetic(corpus: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    let entries = corpus.join(ENTRIES_DIR);
    if entries.exists() {
        for item in walkdir::WalkDir::new(&entries).sort_by_file_name() {
            let item = item.with_context(|| format!("walking {}", entries.display()))?;
            if item.file_name() != ENTRY_FILE || !item.file_type().is_file() {
                continue;
            }
            let entry: Entry = read_json(item.path())?;
            if entry.source.class == SourceClass::Synthetic {
                let dir = item
                    .path()
                    .parent()
                    .context("an entry.json has a parent directory")?;
                out.push((entry.id, dir.to_path_buf()));
            }
        }
    }
    let vectors = corpus.join(VECTORS_DIR);
    if vectors.exists() {
        for item in walkdir::WalkDir::new(&vectors).sort_by_file_name() {
            let item = item.with_context(|| format!("walking {}", vectors.display()))?;
            if !item.file_type().is_file() || item.path().extension().is_none_or(|x| x != "json") {
                continue;
            }
            let file: VectorFile = read_json(item.path())?;
            if file.entry.source.class == SourceClass::Synthetic {
                out.push((file.entry.id, item.path().to_path_buf()));
            }
        }
    }
    Ok(out)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))
}

/// Generates and writes. The caller rebuilds the manifest afterwards.
pub fn build(corpus: &Path, registry: &Registry) -> Result<(Synth, WriteReport)> {
    let synth = generate(registry)?;
    let report = write(corpus, &synth)?;
    Ok((synth, report))
}

/// `{"xdr": <base64>, "json": <SEP-51 value>}` for one XDR value, the form every vector case
/// uses to carry a value, so a testee can start from either.
pub fn repr<T>(value: &T) -> Result<Value>
where
    T: stellar_xdr::WriteXdr + serde::Serialize,
{
    let b64 = value
        .to_xdr_base64(Limits {
            depth: DECODE_DEPTH,
            len: usize::MAX,
        })
        .context("encoding a vector value")?;
    Ok(serde_json::json!({
        "xdr": b64,
        "json": serde_json::to_value(value).context("rendering a vector value")?,
    }))
}
