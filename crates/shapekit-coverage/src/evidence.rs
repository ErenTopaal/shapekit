//! `registry/uncovered.toml` against the evidence generated since its reasons were read: the
//! candidate lane's entries (`candidate/manifest.json`), which the probes and recipes wrote on
//! a local core. A candidate entry core wrote into a ledger that holds an uncovered arm, CAP or
//! fragment either closes the record once it is promoted (`recipe-gap`, `not-imported`) or
//! contradicts the record's blocker, which says a local core cannot write it. A candidate
//! entry core refused whose result holds an arm recorded as `refused-before-ledger` confirms
//! that record.

use std::path::Path;

use serde::Serialize;
use shapekit_manifest::{Manifest, Problem};

use crate::CoverageError;
use shapekit_manifest::fragment_matches;

use crate::compute::{Coverage, EntryShape, entry_shapes, is_core_emitted};
use crate::uncovered::{Blocker, UNCOVERED_FILE};

/// The candidate lane's manifest, relative to the repository root.
pub const CANDIDATE_MANIFEST: &str = "candidate/manifest.json";

// how many candidate entry ids a row lists; the counts carry the rest
const EXAMPLES: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Standing {
    /// A candidate entry core wrote holds it; promoting that entry covers it.
    ClosesOnPromotion,
    /// A candidate entry core wrote holds it, but the blocker says a local core cannot.
    Contradicted,
    /// A candidate entry core refused holds the arm in its result, as the blocker says.
    ConfirmedByRefusal,
}

/// One uncovered record the candidate lane speaks about.
#[derive(Debug, Clone, Serialize)]
pub struct EvidenceRow {
    /// `cap CAP-0083`, `arm OperationResult:...` or `family <id> fragment <fragment>`.
    pub subject: String,
    pub blocker: Blocker,
    pub standing: Standing,
    /// Candidate entries that back the standing, the first few.
    pub candidate_entries: Vec<String>,
    pub candidate_entry_count: usize,
}

/// Whether a candidate entry core wrote may close a record with this blocker: the blocker
/// does not deny that a local core writes it.
fn closable(b: Blocker) -> bool {
    matches!(b, Blocker::RecipeGap | Blocker::NotImported)
}

/// What the candidate lane says about a record with `blocker`: whether a candidate entry core
/// wrote holds it, and whether one core refused holds it in its result.
pub fn standing(blocker: Blocker, written: bool, refused: bool) -> Option<Standing> {
    if written {
        Some(if closable(blocker) {
            Standing::ClosesOnPromotion
        } else {
            Standing::Contradicted
        })
    } else if refused && blocker == Blocker::RefusedBeforeLedger {
        Some(Standing::ConfirmedByRefusal)
    } else {
        None
    }
}

fn row(subject: String, blocker: Blocker, standing: Standing, ids: Vec<String>) -> EvidenceRow {
    EvidenceRow {
        subject,
        blocker,
        standing,
        candidate_entry_count: ids.len(),
        candidate_entries: ids.into_iter().take(EXAMPLES).collect(),
    }
}

fn ids<'a, 'b: 'a>(shapes: impl Iterator<Item = &'a EntryShape<'b>>) -> Vec<String> {
    shapes.map(|s| s.entry.id.clone()).collect()
}

/// The candidate entries that carry evidence: those core wrote, and those core refused with a
/// result. `None` when the repository has no candidate lane.
pub fn candidate_manifest(root: &Path) -> Result<Option<Manifest>, CoverageError> {
    let path = root.join(CANDIDATE_MANIFEST);
    if !path.is_file() {
        return Ok(None);
    }
    let mut m = shapekit_manifest::build::load(&path).map_err(|e| CoverageError::Parse {
        path: path.clone(),
        message: e.to_string(),
    })?;
    m.entries
        .retain(|e| is_core_emitted(e) || e.source.core_rejection.is_some());
    Ok(Some(m))
}

/// Compares every uncovered record of `report` with the candidate entries, and the problem of
/// each contradicted one.
pub fn compare(
    report: &Coverage,
    candidates: &Manifest,
    candidate_dir: &Path,
) -> Result<(Vec<EvidenceRow>, Vec<Problem>), CoverageError> {
    let shapes = entry_shapes(candidates, candidate_dir)?;
    let mut rows = Vec::new();
    let emitted = |hit: &dyn Fn(&EntryShape) -> bool| {
        ids(shapes.iter().filter(|s| s.core_emitted() && hit(s)))
    };
    let judge = |subject: String, blocker: Blocker, written: Vec<String>, refused: Vec<String>| {
        standing(blocker, !written.is_empty(), !refused.is_empty()).map(|st| {
            let ids = if st == Standing::ConfirmedByRefusal {
                refused
            } else {
                written
            };
            row(subject, blocker, st, ids)
        })
    };
    for c in &report.caps {
        if let Some(r) = &c.uncovered {
            let written = ids(candidates
                .entries
                .iter()
                .filter(|e| is_core_emitted(e) && e.caps.iter().any(|x| x == &c.id))
                .map(|e| {
                    shapes
                        .iter()
                        .find(|s| s.entry.id == e.id)
                        .expect("one shape per entry")
                }));
            rows.extend(judge(
                format!("cap {}", c.id),
                r.blocker,
                written,
                Vec::new(),
            ));
        }
    }
    for a in &report.arms.uncovered {
        if let Some(r) = &a.uncovered {
            let written = emitted(&|s| s.signature.contains(&a.arm));
            let refused = ids(shapes.iter().filter(|s| s.refused.contains(&a.arm)));
            rows.extend(judge(format!("arm {}", a.arm), r.blocker, written, refused));
        }
    }
    for f in &report.families {
        for frag in &f.fragments {
            if let Some(r) = &frag.uncovered {
                let written = emitted(&|s| fragment_matches(&s.signature, &frag.fragment));
                let refused = ids(shapes
                    .iter()
                    .filter(|s| fragment_matches(&s.refused, &frag.fragment)));
                rows.extend(judge(
                    format!("family {} fragment {}", f.id, frag.fragment),
                    r.blocker,
                    written,
                    refused,
                ));
            }
        }
    }
    let problems = rows
        .iter()
        .filter(|r| r.standing == Standing::Contradicted)
        .map(|r| {
            Problem::new(
                r.subject.clone(),
                format!(
                    "{UNCOVERED_FILE} gives blocker {}, but {} candidate entr{} that a local core wrote hold{} it ({}); correct the record",
                    r.blocker.name(),
                    r.candidate_entry_count,
                    if r.candidate_entry_count == 1 { "y" } else { "ies" },
                    if r.candidate_entry_count == 1 { "s" } else { "" },
                    r.candidate_entries.join(", ")
                ),
            )
        })
        .collect();
    Ok((rows, problems))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standings() {
        use Blocker::*;
        assert_eq!(
            standing(RecipeGap, true, false),
            Some(Standing::ClosesOnPromotion)
        );
        assert_eq!(
            standing(NotImported, true, true),
            Some(Standing::ClosesOnPromotion)
        );
        for b in [
            RefusedBeforeLedger,
            OldProtocolOnly,
            HistoryOnly,
            NotProducibleLocally,
            NeverEmitted,
        ] {
            assert_eq!(
                standing(b, true, false),
                Some(Standing::Contradicted),
                "{b:?}"
            );
        }
        assert_eq!(
            standing(RefusedBeforeLedger, false, true),
            Some(Standing::ConfirmedByRefusal)
        );
        assert_eq!(standing(NeverEmitted, false, true), None);
        assert_eq!(standing(RecipeGap, false, false), None);
    }
}
