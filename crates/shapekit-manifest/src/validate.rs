//! Manifest rules: registry references, provenance, file hashes and decoded content.

use std::collections::BTreeSet;
use std::path::{Component, Path};

use shapekit_registry::{Problem, Registry, SourceClass};
use stellar_xdr::ReadXdr;

use crate::model::*;
use crate::{decode_limits, sha256_hex, xdr};

/// Every problem with `manifest`, checked against `registry` and the files under `root`
/// (the directory that holds `manifest.json`). Empty means valid.
///
/// Content checks run for `kind = xdr` entries that must decode: every variant must decode
/// under [`crate::decode_depth`] for its type, re-encode to the same bytes when `expected.roundtrip` is set,
/// and render to exactly its expected XDR-JSON file. `must-reject` entries are not decoded
/// here, because whether they are rejected depends on the caps in `caps-limits.toml`.
pub fn validate(manifest: &Manifest, registry: &Registry, root: &Path) -> Vec<Problem> {
    let mut out = Vec::new();
    header(manifest, &mut out);
    if registry.families.is_none() {
        out.push(Problem::new(
            "manifest",
            "families.toml missing, so entry families cannot be checked",
        ));
    }
    for entry in &manifest.entries {
        Checker {
            entry,
            registry,
            root,
            at: format!("manifest {}", entry.id),
            out: &mut out,
        }
        .run();
    }
    out
}

fn header(manifest: &Manifest, out: &mut Vec<Problem>) {
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION {
        out.push(Problem::new(
            "manifest",
            format!(
                "schema_version is {}, this build reads {MANIFEST_SCHEMA_VERSION}",
                manifest.schema_version
            ),
        ));
    }
    if manifest.corpus_version.trim().is_empty() {
        out.push(Problem::new("manifest", "corpus_version is empty"));
    }
    let tiers: Vec<u32> = manifest
        .entries
        .iter()
        .filter_map(|e| e.protocol.generated_at)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if manifest.protocol_tiers != tiers {
        out.push(Problem::new(
            "manifest",
            format!(
                "protocol_tiers is {:?} but the entries were generated at {tiers:?}",
                manifest.protocol_tiers
            ),
        ));
    }
    for pair in manifest.entries.windows(2) {
        if pair[0].id >= pair[1].id {
            out.push(Problem::new(
                format!("manifest {}", pair[1].id),
                format!(
                    "entries are not sorted by unique id ({} comes after {})",
                    pair[1].id, pair[0].id
                ),
            ));
        }
    }
}

struct Checker<'a> {
    entry: &'a Entry,
    registry: &'a Registry,
    root: &'a Path,
    at: String,
    out: &'a mut Vec<Problem>,
}

impl Checker<'_> {
    fn problem(&mut self, message: impl Into<String>) {
        self.out.push(Problem::new(self.at.clone(), message));
    }

    fn run(&mut self) {
        self.identity();
        self.references();
        self.provenance();
        self.protocol();
        self.lists();
        match self.entry.kind {
            EntryKind::Xdr => self.xdr_entry(),
            _ => self.vector_entry(),
        }
    }

    fn identity(&mut self) {
        let e = self.entry;
        match e.id.strip_prefix(&format!("{}.", e.family)) {
            Some(slug)
                if !slug.is_empty()
                    && slug.bytes().all(|b| {
                        b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_'
                    }) => {}
            _ => self.problem(format!(
                "id must be <family>.<slug> with family {} and a lowercase slug",
                e.family
            )),
        }
        let is_synthetic_class = e.source.class == SourceClass::Synthetic;
        if e.synthetic != is_synthetic_class {
            self.problem(format!(
                "synthetic is {} but source.class is {}",
                e.synthetic,
                e.source.class.as_str()
            ));
        }
    }

    fn references(&mut self) {
        let e = self.entry;
        if self.registry.families.is_some() && self.registry.family(&e.family).is_none() {
            self.problem(format!("unknown family {}", e.family));
        }
        if e.caps.is_empty() {
            self.problem("caps is empty");
        }
        for cap in &e.caps {
            if self.registry.cap(cap).is_none() {
                self.problem(format!("unknown cap {cap}"));
            }
        }
        for field in &e.fields_exercised {
            if self.registry.field(field).is_none() {
                self.problem(format!("unknown field {field}"));
            }
        }
    }

    fn provenance(&mut self) {
        let s = &self.entry.source;
        let missing: Vec<&str> = match s.class {
            SourceClass::CoreGolden => vec![("upstream", s.upstream.is_some())],
            SourceClass::LocalCore => {
                vec![("recipe", s.recipe.is_some()), ("chain", s.chain.is_some())]
            }
            SourceClass::LocalCoreRefused => vec![
                ("recipe", s.recipe.is_some()),
                ("chain", s.chain.is_some()),
                ("core_rejection", s.core_rejection.is_some()),
            ],
            SourceClass::TestnetHistory | SourceClass::MainnetHistory => vec![
                ("ledger_seq", s.ledger_seq.is_some()),
                ("provenance_url", s.provenance_url.is_some()),
            ],
            SourceClass::Synthetic => vec![],
        }
        .into_iter()
        .filter(|(_, present)| !present)
        .map(|(k, _)| k)
        .collect();
        if !missing.is_empty() {
            self.problem(format!(
                "a {} entry needs source.{}",
                s.class.as_str(),
                missing.join(" and source.")
            ));
        }
        if let Some(up) = &s.upstream
            && !is_sha256(&up.sha256)
        {
            self.problem("source.upstream.sha256 is not 64 lowercase hex characters");
        }
        self.normalization();
        self.core_rejection();
    }

    fn normalization(&mut self) {
        let e = self.entry;
        let local = e.source.class.is_local();
        if e.source.normalization.is_some() && !local {
            self.problem("source.normalization is only recorded for local-core entries");
        }
        let carries_meta = e
            .xdr_type
            .as_deref()
            .is_some_and(|t| META_TYPES.contains(&t));
        if local && carries_meta && e.source.normalization.is_none() {
            self.problem(
                "a local-core entry carrying meta needs source.normalization, core orders its change lists per process",
            );
        }
    }

    fn core_rejection(&mut self) {
        let e = self.entry;
        let invalid = e.expected.validity == Validity::InvalidByCore;
        let Some(r) = &e.source.core_rejection else {
            if invalid && e.source.class == SourceClass::LocalCore {
                self.problem("an invalid-by-core local-core entry needs source.core_rejection");
            }
            return;
        };
        // bytes refused at submission never reached a ledger, so they must not be labelled
        // as core output; bytes core applied and failed did reach one
        let refused = r.stage == RejectionStage::Submit;
        match e.source.class {
            SourceClass::LocalCore if refused => self.problem(
                "an envelope core refused at submission is local-core-refused, not local-core",
            ),
            SourceClass::LocalCoreRefused if !refused => {
                self.problem("a local-core-refused entry needs source.core_rejection.stage submit")
            }
            _ => {}
        }
        if !invalid {
            self.problem(
                "source.core_rejection is set but expected.validity is not invalid-by-core",
            );
        }
        let limits = decode_limits("TransactionResult", r.result_xdr.len());
        let decoded = stellar_xdr::TransactionResult::from_xdr_base64(&r.result_xdr, limits)
            .map_err(|e| e.to_string());
        match decoded.and_then(|t| xdr::result_code(&t).map_err(|e| e.to_string())) {
            Ok(code) if code == r.result_code => {}
            Ok(code) => self.problem(format!(
                "source.core_rejection.result_code is {} but result_xdr holds {code}",
                r.result_code
            )),
            Err(err) => self.problem(format!(
                "source.core_rejection.result_xdr is not a TransactionResult: {err}"
            )),
        }
    }

    fn protocol(&mut self) {
        let p = self.entry.protocol;
        if p.valid_max.is_some_and(|max| max < p.valid_min) {
            self.problem(format!(
                "protocol.valid_max {:?} is below valid_min {}",
                p.valid_max, p.valid_min
            ));
        }
        let contrast = self.entry.flags.iter().any(|f| f == BELOW_PROTOCOL_VALID);
        if contrast {
            // a contrast entry shows a ledger from before the shape existed, so its range ends
            // before the protocol that introduced the shape
            match p.valid_max {
                None => self.problem(format!(
                    "a {BELOW_PROTOCOL_VALID} entry needs protocol.valid_max"
                )),
                Some(max) if max >= p.introduced => self.problem(format!(
                    "a {BELOW_PROTOCOL_VALID} entry has protocol.valid_max {max}, not below protocol.introduced {}",
                    p.introduced
                )),
                Some(_) => {}
            }
        } else if p.valid_max.is_some_and(|max| max < p.introduced) {
            self.problem("protocol.valid_max is before protocol.introduced");
        }
        if let Some(at) = p.generated_at
            && (at < p.valid_min || p.valid_max.is_some_and(|max| at > max))
        {
            self.problem(format!(
                "protocol.generated_at {at} is outside valid_min {} to valid_max {:?}; bytes core emitted at a protocol are valid there",
                p.valid_min, p.valid_max
            ));
        }
    }

    fn lists(&mut self) {
        let e = self.entry;
        for flag in &e.flags {
            let kebab = !flag.is_empty()
                && flag
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
            if !kebab {
                self.problem(format!("flag {flag:?} is not lowercase kebab-case"));
            }
        }
        for url in &e.linked_issues {
            if !url.starts_with("https://") {
                self.problem(format!("linked issue {url} is not a URL"));
            }
        }
        if e.expected.decode == DecodeExpectation::MustReject && e.expected.roundtrip {
            self.problem("a must-reject entry cannot expect a round trip");
        }
    }

    // reads a referenced file and checks its hash; None means a problem was recorded
    fn checked_file(&mut self, what: &str, path: &str, sha256: Option<&str>) -> Option<Vec<u8>> {
        if !is_safe_relative(path) {
            self.problem(format!(
                "{what} {path} is not a relative path inside the corpus"
            ));
            return None;
        }
        let bytes = match std::fs::read(self.root.join(path)) {
            Ok(b) => b,
            Err(e) => {
                self.problem(format!("{what} {path} cannot be read: {e}"));
                return None;
            }
        };
        let Some(expected) = sha256 else {
            self.problem(format!("{what} {path} has no sha256"));
            return None;
        };
        let actual = sha256_hex(&bytes);
        if actual != expected {
            self.problem(format!(
                "{what} {path} has sha256 {actual}, manifest says {expected}"
            ));
            return None;
        }
        Some(bytes)
    }

    fn xdr_entry(&mut self) {
        let e = self.entry;
        if e.vector.is_some() {
            self.problem("an xdr entry has no vector file");
        }
        let type_name = match &e.xdr_type {
            None => {
                self.problem("an xdr entry needs xdr_type");
                None
            }
            Some(t) => match xdr::type_variant(t) {
                Ok(_) => Some(t.as_str()),
                Err(err) => {
                    self.problem(err.to_string());
                    None
                }
            },
        };
        if e.variants.is_empty() {
            self.problem("an xdr entry needs at least one variant");
        }
        self.passphrases();
        let must_accept = e.expected.decode == DecodeExpectation::MustAccept;
        for (name, v) in e.variants.iter() {
            let xdr_bytes = self.checked_file(
                &format!("{name} xdr_file"),
                &v.xdr_file,
                Some(&v.xdr_sha256),
            );
            let expected_json = match &v.expected_json_file {
                Some(path) => self.checked_file(
                    &format!("{name} expected_json_file"),
                    path,
                    v.expected_json_sha256.as_deref(),
                ),
                None => {
                    if must_accept {
                        self.problem(format!(
                            "{name}: a must-accept xdr entry needs expected_json_file"
                        ));
                    }
                    None
                }
            };
            if let Some(path) = &v.oracle_file {
                self.checked_file(
                    &format!("{name} oracle_file"),
                    path,
                    v.oracle_sha256.as_deref(),
                );
            }
            if let (true, Some(t), Some(bytes)) = (must_accept, type_name, xdr_bytes) {
                self.content(name, t, &bytes, expected_json.as_deref());
            }
        }
    }

    fn content(
        &mut self,
        variant: &str,
        type_name: &str,
        bytes: &[u8],
        expected_json: Option<&[u8]>,
    ) {
        let limits = decode_limits(type_name, bytes.len());
        if self.entry.expected.roundtrip
            && let Err(err) = xdr::roundtrip_check(type_name, bytes, limits.clone())
        {
            self.problem(format!("{variant}: round trip failed: {err}"));
            return;
        }
        let Some(expected) = expected_json else {
            return;
        };
        match xdr::to_xdr_json(type_name, bytes, limits) {
            Ok(json) if json.as_bytes() == expected => {}
            Ok(_) => self.problem(format!(
                "{variant}: decoded XDR-JSON differs from expected_json_file"
            )),
            Err(err) => self.problem(format!("{variant}: {err}")),
        }
    }

    fn passphrases(&mut self) {
        let e = self.entry;
        for (name, v, want) in [
            ("testnet", &e.variants.testnet, TESTNET_PASSPHRASE),
            ("mainnet", &e.variants.mainnet, MAINNET_PASSPHRASE),
        ] {
            if let Some(v) = v
                && v.network_passphrase.as_deref() != Some(want)
            {
                self.problem(format!("{name} variant must record passphrase {want:?}"));
            }
        }
        if e.variants.any.is_some()
            && (e.variants.testnet.is_some() || e.variants.mainnet.is_some())
        {
            self.problem("an any variant cannot sit next to testnet or mainnet variants");
        }
        if e.variants.any.is_some()
            && e.passphrase_dependent
            && e.source.class != SourceClass::CoreGolden
        {
            self.problem("passphrase_dependent bytes need testnet or mainnet variants, not any");
        }
        if let (Some(t), Some(m)) = (&e.variants.testnet, &e.variants.mainnet) {
            let differ = t.xdr_sha256 != m.xdr_sha256;
            if e.passphrase_dependent != differ {
                self.problem(format!(
                    "passphrase_dependent is {} but the testnet and mainnet bytes are {}",
                    e.passphrase_dependent,
                    if differ { "different" } else { "identical" }
                ));
            }
        }
    }

    fn vector_entry(&mut self) {
        let e = self.entry;
        if !e.variants.is_empty() {
            self.problem("only xdr entries have variants");
        }
        let Some(vector) = &e.vector else {
            self.problem("a vector entry needs its vector file");
            return;
        };
        let Some(bytes) = self.checked_file("vector file", &vector.path, Some(&vector.sha256))
        else {
            return;
        };
        let file: VectorFile = match serde_json::from_slice(&bytes) {
            Ok(f) => f,
            Err(err) => {
                self.problem(format!("vector file {} does not parse: {err}", vector.path));
                return;
            }
        };
        let mut stripped = e.clone();
        stripped.vector = None;
        if file.entry != stripped {
            self.problem(format!(
                "vector file {} holds a different entry record than the manifest",
                vector.path
            ));
        }
        if file.cases.is_empty() {
            self.problem(format!("vector file {} has no cases", vector.path));
        }
    }
}

/// Types whose bytes hold ledger-entry change lists.
const META_TYPES: [&str; 4] = [
    "LedgerCloseMeta",
    "TransactionMeta",
    "TransactionResultMeta",
    "TransactionResultMetaV1",
];

fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A non-empty path made only of normal components: no root, no `..`, no backslash.
pub fn is_safe_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}
