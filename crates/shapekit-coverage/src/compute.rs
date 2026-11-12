//! The coverage numbers: per CAP, per reachable union arm, per family fragment and per field
//! that core does not fully validate.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Serialize;
use shapekit_manifest::{
    DecodeExpectation, Entry, EntryKind, Manifest, Problem, Validity, decode_limits,
    fragment_matches, reachable_arms, shape_signature,
};
use shapekit_registry::{Registry, SourceClass, ValidatedByCore};
use stellar_xdr::ReadXdr;

use crate::CoverageError;
use crate::uncovered::{Blocker, PlannedShape, UNCOVERED_FILE, UncoveredFile};

/// Format version of `coverage.json`.
pub const COVERAGE_SCHEMA_VERSION: u32 = 1;

/// The union arms counted for coverage are the ones reachable from this type.
pub const ROOT_TYPE: &str = "LedgerCloseMeta";

// how many entry ids a covered arm lists; the counts carry the rest
const ARM_EXAMPLES: usize = 3;

#[derive(Debug, Clone, Serialize)]
pub struct Coverage {
    pub schema_version: u32,
    pub corpus_version: String,
    pub stellar_xdr: String,
    pub root_type: &'static str,
    pub summary: Summary,
    pub caps: Vec<CapRow>,
    pub arms: Arms,
    pub families: Vec<FamilyRow>,
    pub fields: Vec<FieldRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub entries: usize,
    pub core_emitted_entries: usize,
    pub entries_by_source_class: BTreeMap<&'static str, usize>,
    pub caps: usize,
    pub caps_indexer_visible: usize,
    pub caps_indexer_visible_core_emitted: usize,
    pub caps_indexer_visible_with_reason: usize,
    pub arms_reachable: usize,
    pub arms_covered: usize,
    pub arms_uncovered: usize,
    pub arms_uncovered_by_blocker: BTreeMap<Blocker, usize>,
    pub families: usize,
    pub fragments: usize,
    pub fragments_covered: usize,
    pub fragments_uncovered: usize,
    pub fields_not_fully_validated: usize,
    pub fields_not_fully_validated_exercised: usize,
}

/// A recorded reason for missing core-emitted coverage.
#[derive(Debug, Clone, Serialize)]
pub struct Reason {
    pub blocker: Blocker,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapRow {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<u32>,
    pub status: String,
    pub indexer_visible: bool,
    /// The registry's reason when the CAP is not indexer-visible.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_visible_reason: Option<String>,
    pub families: Vec<String>,
    /// Entries tagged with this CAP.
    pub entries: usize,
    pub core_emitted_entries: usize,
    pub entries_by_source_class: BTreeMap<&'static str, usize>,
    /// Entries holding each variant; `vector` counts the non-xdr kinds, which have none.
    pub entries_by_variant: BTreeMap<&'static str, usize>,
    /// Protocols the core-emitted entries were generated at.
    pub protocols_core_emitted: Vec<u32>,
    /// Protocols any entry was generated at.
    pub protocols_all: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uncovered: Option<Reason>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Arms {
    pub reachable: usize,
    pub covered: Vec<CoveredArm>,
    pub uncovered: Vec<UncoveredArm>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CoveredArm {
    pub arm: String,
    pub core_emitted_entries: usize,
    pub by_source_class: BTreeMap<&'static str, usize>,
    pub examples: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UncoveredArm {
    pub arm: String,
    /// Synthetic or not-emittable entries that hold the arm; they show the shape, not that
    /// core emits it.
    pub non_core_entries: Vec<String>,
    /// Entries whose submit-time result from core holds the arm.
    pub refused_by_core: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uncovered: Option<Reason>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FamilyRow {
    pub id: String,
    pub title: String,
    pub caps: Vec<String>,
    /// Entries whose `family` is this one.
    pub entries_by_source_class: BTreeMap<&'static str, usize>,
    /// Source classes the registry expects that no entry of the family has.
    pub missing_source_classes: Vec<&'static str>,
    pub fragments: Vec<FragmentRow>,
    /// Shapes the family plans that no fragment can express and the corpus lacks.
    pub planned: Vec<PlannedShape>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FragmentRow {
    pub fragment: String,
    pub covered: bool,
    /// Core-emitted entries of any family whose signature matches the fragment.
    pub core_emitted: Vec<String>,
    pub non_core_entries: Vec<String>,
    pub refused_by_core: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uncovered: Option<Reason>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldRow {
    pub id: String,
    pub xdr_path: String,
    pub validated_by_core: ValidatedByCore,
    pub entries: Vec<String>,
    pub core_emitted_entries: usize,
    pub entries_by_source_class: BTreeMap<&'static str, usize>,
}

/// What one manifest entry contributes to coverage.
pub struct EntryShape<'a> {
    pub entry: &'a Entry,
    /// Union of the signatures of every variant; empty when the entry is not decoded.
    pub signature: BTreeSet<String>,
    /// Signature of the `TransactionResult` core returned when it refused the entry.
    pub refused: BTreeSet<String>,
}

impl EntryShape<'_> {
    pub fn core_emitted(&self) -> bool {
        is_core_emitted(self.entry)
    }
}

/// Bytes core wrote into a ledger: any non-synthetic xdr entry that core did not refuse.
pub fn is_core_emitted(entry: &Entry) -> bool {
    entry.kind == EntryKind::Xdr
        && entry.source.class != SourceClass::Synthetic
        && entry.expected.validity == Validity::Valid
}

/// Decodes every entry that is expected to decode and computes its signature.
pub fn entry_shapes<'a>(
    manifest: &'a Manifest,
    corpus: &Path,
) -> Result<Vec<EntryShape<'a>>, CoverageError> {
    // entries nest to the depth caps (1000 levels and more), and decoding and walking them
    // recurses once per level, which overflows a default thread stack in debug builds
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("coverage-shapes".into())
            .stack_size(STACK_BYTES)
            .spawn_scoped(scope, || {
                manifest
                    .entries
                    .iter()
                    .map(|entry| entry_shape(entry, corpus))
                    .collect()
            })
            .map_err(|e| CoverageError::Entry(format!("starting the decode thread: {e}")))?
            .join()
            .map_err(|_| CoverageError::Entry("the decode thread panicked".into()))?
    })
}

const STACK_BYTES: usize = 256 << 20;

fn entry_shape<'a>(entry: &'a Entry, corpus: &Path) -> Result<EntryShape<'a>, CoverageError> {
    let mut signature = BTreeSet::new();
    let mut refused = BTreeSet::new();
    let decodes =
        entry.kind == EntryKind::Xdr && entry.expected.decode != DecodeExpectation::MustReject;
    if decodes {
        let ty = entry.xdr_type.as_deref().ok_or_else(|| {
            CoverageError::Entry(format!("{}: kind xdr with no xdr_type", entry.id))
        })?;
        for (name, variant) in entry.variants.iter() {
            let path = corpus.join(&variant.xdr_file);
            let bytes = std::fs::read(&path).map_err(|source| CoverageError::Io {
                path: path.clone(),
                source,
            })?;
            match shape_signature(ty, &bytes, decode_limits(ty, bytes.len())) {
                Ok(sig) => signature.extend(sig),
                // an implementation-defined entry may legitimately fail to decode here
                Err(_) if entry.expected.decode == DecodeExpectation::ImplementationDefined => {}
                Err(err) => {
                    return Err(CoverageError::Entry(format!(
                        "{} variant {name}: {err}",
                        entry.id
                    )));
                }
            }
        }
    }
    if let Some(r) = &entry.source.core_rejection {
        let limits = decode_limits("TransactionResult", r.result_xdr.len());
        let result = stellar_xdr::TransactionResult::from_xdr_base64(&r.result_xdr, limits)
            .map_err(|e| {
                CoverageError::Entry(format!("{}: core_rejection.result_xdr: {e}", entry.id))
            })?;
        let sig = shapekit_manifest::signature::signature_of(
            &stellar_xdr::Type::TransactionResult(Box::new(result)),
        )
        .map_err(|e| {
            CoverageError::Entry(format!("{}: core_rejection signature: {e}", entry.id))
        })?;
        refused.extend(sig);
    }
    Ok(EntryShape {
        entry,
        signature,
        refused,
    })
}

/// Computes the report and every problem `--check` fails on, other than staleness.
pub fn compute(
    manifest: &Manifest,
    registry: &Registry,
    uncovered: &UncoveredFile,
    shapes: &[EntryShape<'_>],
) -> Result<(Coverage, Vec<Problem>), CoverageError> {
    let mut problems = Vec::new();
    let families = registry
        .families
        .as_ref()
        .ok_or_else(|| CoverageError::Entry("registry has no families.toml".into()))?;

    let caps = cap_rows(manifest, registry, uncovered, &mut problems);
    let arms = arm_rows(shapes, uncovered, &mut problems)?;
    let family_rows = family_rows(shapes, registry, uncovered, &mut problems);
    let fields = field_rows(manifest, registry);
    stale_reasons(uncovered, &caps, &arms, &family_rows, &mut problems);

    let mut by_class = BTreeMap::new();
    for e in &manifest.entries {
        *by_class.entry(e.source.class.as_str()).or_insert(0) += 1;
    }
    let mut arms_by_blocker = BTreeMap::new();
    for a in &arms.uncovered {
        if let Some(r) = &a.uncovered {
            *arms_by_blocker.entry(r.blocker).or_insert(0) += 1;
        }
    }
    let visible: Vec<&CapRow> = caps.iter().filter(|c| c.indexer_visible).collect();
    let fragments: Vec<&FragmentRow> = family_rows.iter().flat_map(|f| &f.fragments).collect();
    let summary = Summary {
        entries: manifest.entries.len(),
        core_emitted_entries: manifest
            .entries
            .iter()
            .filter(|e| is_core_emitted(e))
            .count(),
        entries_by_source_class: by_class,
        caps: caps.len(),
        caps_indexer_visible: visible.len(),
        caps_indexer_visible_core_emitted: visible
            .iter()
            .filter(|c| c.core_emitted_entries > 0)
            .count(),
        caps_indexer_visible_with_reason: visible.iter().filter(|c| c.uncovered.is_some()).count(),
        arms_reachable: arms.reachable,
        arms_covered: arms.covered.len(),
        arms_uncovered: arms.uncovered.len(),
        arms_uncovered_by_blocker: arms_by_blocker,
        families: families.families.len(),
        fragments: fragments.len(),
        fragments_covered: fragments.iter().filter(|f| f.covered).count(),
        fragments_uncovered: fragments.iter().filter(|f| !f.covered).count(),
        fields_not_fully_validated: fields.len(),
        fields_not_fully_validated_exercised: fields
            .iter()
            .filter(|f| !f.entries.is_empty())
            .count(),
    };
    problems.sort();
    Ok((
        Coverage {
            schema_version: COVERAGE_SCHEMA_VERSION,
            corpus_version: manifest.corpus_version.clone(),
            stellar_xdr: shapekit_manifest::STELLAR_XDR_VERSION.pkg.to_string(),
            root_type: ROOT_TYPE,
            summary,
            caps,
            arms,
            families: family_rows,
            fields,
        },
        problems,
    ))
}

fn variant_names(entry: &Entry) -> Vec<&'static str> {
    if entry.kind != EntryKind::Xdr {
        return vec!["vector"];
    }
    entry.variants.iter().map(|(n, _)| n).collect()
}

fn cap_rows(
    manifest: &Manifest,
    registry: &Registry,
    uncovered: &UncoveredFile,
    problems: &mut Vec<Problem>,
) -> Vec<CapRow> {
    let families = registry
        .families
        .as_ref()
        .map(|f| f.families.as_slice())
        .unwrap_or(&[]);
    let mut rows = Vec::new();
    for cap in &registry.caps.caps {
        let tagged: Vec<&Entry> = manifest
            .entries
            .iter()
            .filter(|e| e.caps.iter().any(|c| c == &cap.id))
            .collect();
        let mut by_class = BTreeMap::new();
        let mut by_variant = BTreeMap::new();
        let mut core_protocols = BTreeSet::new();
        let mut all_protocols = BTreeSet::new();
        for e in &tagged {
            *by_class.entry(e.source.class.as_str()).or_insert(0) += 1;
            for v in variant_names(e) {
                *by_variant.entry(v).or_insert(0) += 1;
            }
            if let Some(p) = e.protocol.generated_at {
                all_protocols.insert(p);
                if is_core_emitted(e) {
                    core_protocols.insert(p);
                }
            }
        }
        let core_emitted = tagged.iter().filter(|e| is_core_emitted(e)).count();
        let reason = uncovered.cap(&cap.id).map(|r| Reason {
            blocker: r.blocker,
            reason: r.reason.clone(),
        });
        if cap.indexer_visible && core_emitted == 0 && reason.is_none() {
            problems.push(Problem::new(
                format!("cap {}", cap.id),
                format!(
                    "indexer-visible with no core-emitted entry and no reason in {UNCOVERED_FILE}"
                ),
            ));
        }
        rows.push(CapRow {
            id: cap.id.clone(),
            title: cap.title.clone(),
            protocol: cap.protocol,
            status: cap.status.clone(),
            indexer_visible: cap.indexer_visible,
            not_visible_reason: if cap.indexer_visible {
                None
            } else {
                cap.reason.clone()
            },
            families: families
                .iter()
                .filter(|f| f.caps.iter().any(|c| c == &cap.id))
                .map(|f| f.id.clone())
                .collect(),
            entries: tagged.len(),
            core_emitted_entries: core_emitted,
            entries_by_source_class: by_class,
            entries_by_variant: by_variant,
            protocols_core_emitted: core_protocols.into_iter().collect(),
            protocols_all: all_protocols.into_iter().collect(),
            uncovered: reason,
        });
    }
    rows
}

fn arm_rows(
    shapes: &[EntryShape<'_>],
    uncovered: &UncoveredFile,
    problems: &mut Vec<Problem>,
) -> Result<Arms, CoverageError> {
    let reachable = reachable_arms(ROOT_TYPE)
        .map_err(|e| CoverageError::Entry(format!("reachable arms: {e}")))?;
    let mut covered = Vec::new();
    let mut missing = Vec::new();
    for arm in &reachable {
        let holders: Vec<&EntryShape> = shapes
            .iter()
            .filter(|s| s.core_emitted() && s.signature.contains(arm))
            .collect();
        if holders.is_empty() {
            let reason = uncovered.arm(arm).map(|r| Reason {
                blocker: r.blocker,
                reason: r.reason.clone(),
            });
            if reason.is_none() {
                problems.push(Problem::new(
                    format!("arm {arm}"),
                    format!("no core-emitted entry and no reason in {UNCOVERED_FILE}"),
                ));
            }
            missing.push(UncoveredArm {
                arm: arm.clone(),
                non_core_entries: ids(shapes
                    .iter()
                    .filter(|s| !s.core_emitted() && s.signature.contains(arm))),
                refused_by_core: ids(shapes.iter().filter(|s| s.refused.contains(arm))),
                uncovered: reason,
            });
            continue;
        }
        let mut by_class = BTreeMap::new();
        for h in &holders {
            *by_class.entry(h.entry.source.class.as_str()).or_insert(0) += 1;
        }
        covered.push(CoveredArm {
            arm: arm.clone(),
            core_emitted_entries: holders.len(),
            by_source_class: by_class,
            examples: holders
                .iter()
                .take(ARM_EXAMPLES)
                .map(|h| h.entry.id.clone())
                .collect(),
        });
    }
    Ok(Arms {
        reachable: reachable.len(),
        covered,
        uncovered: missing,
    })
}

fn ids<'a, 'b: 'a>(shapes: impl Iterator<Item = &'a EntryShape<'b>>) -> Vec<String> {
    shapes.map(|s| s.entry.id.clone()).collect()
}

fn family_rows(
    shapes: &[EntryShape<'_>],
    registry: &Registry,
    uncovered: &UncoveredFile,
    problems: &mut Vec<Problem>,
) -> Vec<FamilyRow> {
    let families = registry
        .families
        .as_ref()
        .map(|f| f.families.as_slice())
        .unwrap_or(&[]);
    let mut rows = Vec::new();
    for family in families {
        let mut by_class = BTreeMap::new();
        for s in shapes.iter().filter(|s| s.entry.family == family.id) {
            *by_class.entry(s.entry.source.class.as_str()).or_insert(0) += 1;
        }
        let missing_classes = family
            .source_classes
            .iter()
            .map(|c| c.as_str())
            .filter(|c| !by_class.contains_key(c))
            .collect();
        let mut fragments = Vec::new();
        for fragment in &family.signature {
            let core_emitted = ids(shapes
                .iter()
                .filter(|s| s.core_emitted() && fragment_matches(&s.signature, fragment)));
            let covered = !core_emitted.is_empty();
            let reason = uncovered.fragment(&family.id, fragment).map(|r| Reason {
                blocker: r.blocker,
                reason: r.reason.clone(),
            });
            if !covered && reason.is_none() {
                problems.push(Problem::new(
                    format!("family {} fragment {fragment}", family.id),
                    format!("no core-emitted entry and no reason in {UNCOVERED_FILE}"),
                ));
            }
            fragments.push(FragmentRow {
                fragment: fragment.clone(),
                covered,
                core_emitted,
                non_core_entries: ids(shapes
                    .iter()
                    .filter(|s| !s.core_emitted() && fragment_matches(&s.signature, fragment))),
                refused_by_core: ids(shapes
                    .iter()
                    .filter(|s| fragment_matches(&s.refused, fragment))),
                uncovered: if covered { None } else { reason },
            });
        }
        rows.push(FamilyRow {
            id: family.id.clone(),
            title: family.title.clone(),
            caps: family.caps.clone(),
            entries_by_source_class: by_class,
            missing_source_classes: missing_classes,
            fragments,
            planned: uncovered
                .planned
                .iter()
                .filter(|p| p.family == family.id)
                .cloned()
                .collect(),
        });
    }
    rows
}

fn field_rows(manifest: &Manifest, registry: &Registry) -> Vec<FieldRow> {
    registry
        .fields
        .fields
        .iter()
        .filter(|f| f.validated_by_core != ValidatedByCore::Yes)
        .map(|f| {
            let entries: Vec<&Entry> = manifest
                .entries
                .iter()
                .filter(|e| e.fields_exercised.iter().any(|x| x == &f.id))
                .collect();
            let mut by_class = BTreeMap::new();
            for e in &entries {
                *by_class.entry(e.source.class.as_str()).or_insert(0) += 1;
            }
            FieldRow {
                id: f.id.clone(),
                xdr_path: f.xdr_path.clone(),
                validated_by_core: f.validated_by_core,
                core_emitted_entries: entries.iter().filter(|e| is_core_emitted(e)).count(),
                entries: entries.iter().map(|e| e.id.clone()).collect(),
                entries_by_source_class: by_class,
            }
        })
        .collect()
}

// a reason that no longer explains a gap hides the next real one, so it must go
fn stale_reasons(
    uncovered: &UncoveredFile,
    caps: &[CapRow],
    arms: &Arms,
    families: &[FamilyRow],
    problems: &mut Vec<Problem>,
) {
    let at = |what: String| format!("{UNCOVERED_FILE} {what}");
    let mut seen = BTreeSet::new();
    for r in &uncovered.caps {
        if !seen.insert(r.id.as_str()) {
            problems.push(Problem::new(at(format!("cap {}", r.id)), "listed twice"));
        }
        match caps.iter().find(|c| c.id == r.id) {
            None => problems.push(Problem::new(
                at(format!("cap {}", r.id)),
                "not in caps.toml",
            )),
            Some(c) if !c.indexer_visible => problems.push(Problem::new(
                at(format!("cap {}", r.id)),
                "not indexer-visible; caps.toml already records why",
            )),
            Some(c) if c.core_emitted_entries > 0 => problems.push(Problem::new(
                at(format!("cap {}", r.id)),
                format!(
                    "has {} core-emitted entries, remove the reason",
                    c.core_emitted_entries
                ),
            )),
            Some(_) => {}
        }
    }
    let mut seen = BTreeSet::new();
    for r in &uncovered.arms {
        if r.reason.trim().is_empty() {
            problems.push(Problem::new(
                at(format!("arm {:?}", r.arms)),
                "empty reason",
            ));
        }
        for arm in &r.arms {
            if !seen.insert(arm.as_str()) {
                problems.push(Problem::new(at(format!("arm {arm}")), "listed twice"));
            }
            if arms.covered.iter().any(|c| &c.arm == arm) {
                problems.push(Problem::new(
                    at(format!("arm {arm}")),
                    "now covered by a core-emitted entry, remove it",
                ));
            } else if !arms.uncovered.iter().any(|u| &u.arm == arm) {
                problems.push(Problem::new(
                    at(format!("arm {arm}")),
                    format!("not a union arm reachable from {ROOT_TYPE}"),
                ));
            }
        }
    }
    let mut seen = BTreeSet::new();
    for r in &uncovered.fragments {
        let here = at(format!("family {} fragment {}", r.family, r.fragment));
        if !seen.insert((r.family.as_str(), r.fragment.as_str())) {
            problems.push(Problem::new(here.clone(), "listed twice"));
        }
        if r.reason.trim().is_empty() {
            problems.push(Problem::new(here.clone(), "empty reason"));
        }
        let row = families
            .iter()
            .find(|f| f.id == r.family)
            .and_then(|f| f.fragments.iter().find(|x| x.fragment == r.fragment));
        match row {
            None => problems.push(Problem::new(here, "no such fragment in families.toml")),
            Some(x) if x.covered => problems.push(Problem::new(
                here,
                "now covered by a core-emitted entry, remove it",
            )),
            Some(_) => {}
        }
    }
    for r in &uncovered.caps {
        if r.reason.trim().is_empty() {
            problems.push(Problem::new(at(format!("cap {}", r.id)), "empty reason"));
        }
    }
    for p in &uncovered.planned {
        let here = at(format!("planned {} {:?}", p.family, p.shape));
        if !families.iter().any(|f| f.id == p.family) {
            problems.push(Problem::new(
                here.clone(),
                "no such family in families.toml",
            ));
        }
        if p.shape.trim().is_empty() || p.reason.trim().is_empty() {
            problems.push(Problem::new(here, "empty shape or reason"));
        }
    }
    if uncovered.schema_version != shapekit_registry::SCHEMA_VERSION {
        problems.push(Problem::new(
            UNCOVERED_FILE,
            format!(
                "schema_version is {}, this build reads {}",
                uncovered.schema_version,
                shapekit_registry::SCHEMA_VERSION
            ),
        ));
    }
}
