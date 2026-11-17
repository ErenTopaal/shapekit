//! The obligation ledger, `corpus/coverage/obligations.json`: every obligation the producers
//! wrote, merged with the witnesses the corpus and candidate entries name.
//!
//! Sources, all relative to the repository root:
//! - the fragment files `registry/obligation-sources.toml` lists, each an obligation ledger a
//!   producer wrote (core test sections, seeded faults, and so on). Every `*.obligations.json`
//!   under `conformance/`, `registry/`, `corpus/` and `candidate/` must be listed there;
//! - one `gap` obligation per row of `registry/gaps/*.toml`, `gap:<language>:<row id>`;
//! - two `gate-side` obligations per gate of `registry/generated/gates.json`, one per branch,
//!   carrying the side's status, witnesses and evidence;
//! - the `obligations` of every entry of `corpus/manifest.json` (witness `entry:<id>`) and
//!   `candidate/manifest.json` (witness `candidate:<id>`).
//!
//! An obligation an entry names becomes met with that entry as witness; one no fragment lists
//! is added met. A witness on an obligation a fragment calls unmeetable is a problem.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use shapekit_contracts::obligations::Obligation;
use shapekit_contracts::{
    Contract, GateList, ObligationKind, ObligationLedger, ObligationStatus, WitnessRef,
};
use shapekit_manifest::Problem;

use crate::CoverageError;

pub const SOURCES_FILE: &str = "obligation-sources.toml";
/// The protocol gate list, relative to the registry directory.
pub const GATES_FILE: &str = "generated/gates.json";
pub const LEDGER_JSON: &str = "obligations.json";
const FRAGMENT_SUFFIX: &str = ".obligations.json";
const FRAGMENT_DIRS: [&str; 4] = ["conformance", "registry", "corpus", "candidate"];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourcesFile {
    schema_version: u32,
    files: Vec<String>,
}

/// Obligations of one kind by status, as the summary shows them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KindRow {
    pub kind: &'static str,
    /// Met obligations of a kind whose met ones rest on one kind of evidence; absent for
    /// `gate-side`, whose two kinds of evidence are never summed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub met: Option<usize>,
    pub unmeetable: usize,
    pub open: usize,
    /// `gate-side` only: the met sides a clang branch count witnesses (a `chain:` witness of
    /// `twin gate-witnesses`), kept apart from those inferred from an execution count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub met_branch_count: Option<usize>,
    /// `gate-side` only: the met sides whose witnesses are all recipe runs at a protocol that
    /// takes the side, so the side is inferred from an execution count of the gate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub met_inferred: Option<usize>,
}

/// How a met gate side is witnessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateEvidence {
    /// At least one `chain:` witness: a clang branch count on the twin.
    BranchCount,
    /// Every witness is `recipe:<path>@p<N>`: an execution count at a protocol taking the side.
    Inferred,
}

/// The kind of evidence behind a met gate side, or `None` when its witnesses are neither form.
pub fn gate_evidence(o: &Obligation) -> Option<GateEvidence> {
    if o.witnesses.iter().any(|w| w.starts_with("chain:")) {
        Some(GateEvidence::BranchCount)
    } else if !o.witnesses.is_empty() && o.witnesses.iter().all(|w| w.starts_with("recipe:")) {
        Some(GateEvidence::Inferred)
    } else {
        None
    }
}

pub fn rows(ledger: &ObligationLedger) -> Vec<KindRow> {
    let gate_met = |e: GateEvidence| {
        ledger
            .obligations
            .iter()
            .filter(|o| {
                o.kind == ObligationKind::GateSide
                    && o.status == ObligationStatus::Met
                    && gate_evidence(o) == Some(e)
            })
            .count()
    };
    ObligationKind::ALL
        .iter()
        .map(|&k| {
            let gate = k == ObligationKind::GateSide;
            // a met gate side the evidence rule does not place is reported as a problem by
            // gate_evidence_problems and is in neither count
            KindRow {
                kind: k.as_str(),
                met: (!gate).then(|| ledger.count(k, ObligationStatus::Met)),
                unmeetable: ledger.count(k, ObligationStatus::Unmeetable),
                open: ledger.count(k, ObligationStatus::Open),
                met_branch_count: gate.then(|| gate_met(GateEvidence::BranchCount)),
                met_inferred: gate.then(|| gate_met(GateEvidence::Inferred)),
            }
        })
        .collect()
}

/// A problem for each met gate side whose witnesses are neither a branch count nor recipe runs
/// only, so every public count of met sides says which evidence it rests on.
pub fn gate_evidence_problems(ledger: &ObligationLedger) -> Vec<Problem> {
    ledger
        .obligations
        .iter()
        .filter(|o| {
            o.kind == ObligationKind::GateSide
                && o.status == ObligationStatus::Met
                && gate_evidence(o).is_none()
        })
        .map(|o| {
            Problem::new(
                o.id.clone(),
                "met, but its witnesses are neither a chain: branch count nor recipe: runs only",
            )
        })
        .collect()
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> CoverageError + '_ {
    move |source| CoverageError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn parse(path: &Path, message: impl ToString) -> CoverageError {
    CoverageError::Parse {
        path: path.to_path_buf(),
        message: message.to_string(),
    }
}

/// Every `*.obligations.json` under the fragment directories, relative to `root`.
fn fragment_files(root: &Path) -> Result<Vec<String>, CoverageError> {
    let mut out = Vec::new();
    for dir in FRAGMENT_DIRS {
        let base = root.join(dir);
        if !base.is_dir() {
            continue;
        }
        for item in walkdir::WalkDir::new(&base).sort_by_file_name() {
            let item = item.map_err(|e| parse(&base, e))?;
            let name = item.file_name().to_string_lossy();
            if item.file_type().is_file() && name.ends_with(FRAGMENT_SUFFIX) {
                let rel = item.path().strip_prefix(root).expect("walked under root");
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    Ok(out)
}

/// Gap rows: `(language, row id, why there is no vector)`.
fn gap_rows(registry: &Path) -> Result<Vec<(String, String, Option<String>)>, CoverageError> {
    let dir = registry.join("gaps");
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(io(&dir))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    files.sort();
    for path in files {
        let text = std::fs::read_to_string(&path).map_err(io(&path))?;
        let value: toml::Table = toml::from_str(&text).map_err(|e| parse(&path, e))?;
        let language = value
            .get("language")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| parse(&path, "no language"))?
            .to_owned();
        let gaps = value
            .get("gap")
            .and_then(toml::Value::as_array)
            .ok_or_else(|| parse(&path, "no [[gap]] rows"))?;
        for g in gaps {
            let id = g
                .get("id")
                .and_then(toml::Value::as_str)
                .ok_or_else(|| parse(&path, "a [[gap]] row has no id"))?;
            let no_vector = g
                .get("no_vector")
                .and_then(toml::Value::as_str)
                .map(str::to_owned);
            out.push((language.clone(), id.to_owned(), no_vector));
        }
    }
    Ok(out)
}

fn entry_obligations(
    manifest: &Path,
    prefix: &str,
) -> Result<Vec<(String, String)>, CoverageError> {
    let m = shapekit_manifest::build::load(manifest).map_err(|e| parse(manifest, e))?;
    Ok(m.entries
        .iter()
        .flat_map(|e| {
            e.obligations
                .iter()
                .map(move |o| (o.clone(), format!("{prefix}:{}", e.id)))
        })
        .collect())
}

fn kind_of(id: &str) -> Option<ObligationKind> {
    let prefix = id.split(':').next()?;
    ObligationKind::ALL
        .into_iter()
        .find(|k| k.as_str() == prefix)
}

/// The merged ledger, and every problem with its sources. `root` is the repository root,
/// `corpus` and `registry` the directories the coverage run reads.
pub fn build(
    root: &Path,
    corpus: &Path,
    registry: &Path,
) -> Result<(ObligationLedger, Vec<Problem>), CoverageError> {
    let mut problems = Vec::new();
    let at = |what: &str| format!("obligations ({what})");
    let sources_path = registry.join(SOURCES_FILE);
    let text = std::fs::read_to_string(&sources_path).map_err(io(&sources_path))?;
    let sources: SourcesFile = toml::from_str(&text).map_err(|e| parse(&sources_path, e))?;
    if sources.schema_version != 1 {
        return Err(parse(&sources_path, "schema_version is not 1"));
    }

    let mut map: BTreeMap<String, Obligation> = BTreeMap::new();
    for rel in &sources.files {
        let path = root.join(rel);
        let fragment =
            shapekit_contracts::load::<ObligationLedger>(&path).map_err(|e| parse(&path, e))?;
        for o in fragment.obligations {
            if map.contains_key(&o.id) {
                problems.push(Problem::new(
                    at(rel),
                    format!("{} is listed by more than one fragment", o.id),
                ));
                continue;
            }
            map.insert(o.id.clone(), o);
        }
    }
    for rel in fragment_files(root)? {
        if !sources.files.contains(&rel) {
            problems.push(Problem::new(
                at(SOURCES_FILE),
                format!(
                    "{rel} is an obligation fragment that registry/{SOURCES_FILE} does not list"
                ),
            ));
        }
    }

    let gates_path = registry.join(GATES_FILE);
    if gates_path.is_file() {
        let gates =
            shapekit_contracts::load::<GateList>(&gates_path).map_err(|e| parse(&gates_path, e))?;
        for g in &gates.gates {
            for side in &g.sides {
                let oid = g.side_obligation_id(side.branch);
                if map.contains_key(&oid) {
                    problems.push(Problem::new(
                        at(GATES_FILE),
                        format!("{oid} is also listed by a fragment"),
                    ));
                    continue;
                }
                map.insert(
                    oid.clone(),
                    Obligation {
                        id: oid,
                        kind: ObligationKind::GateSide,
                        status: side.status,
                        witnesses: side.witnesses.clone(),
                        evidence: side.evidence.clone(),
                    },
                );
            }
        }
    }

    for (language, id, no_vector) in gap_rows(registry)? {
        let oid = Obligation::id_for(ObligationKind::Gap, &format!("{language}:{id}"));
        map.entry(oid.clone()).or_insert_with(|| Obligation {
            id: oid,
            kind: ObligationKind::Gap,
            status: ObligationStatus::Open,
            witnesses: Vec::new(),
            evidence: Some(no_vector.unwrap_or_else(|| {
                format!("registry/gaps/{language}.toml has vectors for it, but no corpus or candidate entry carries them")
            })),
        });
    }

    let mut named = entry_obligations(&corpus.join("manifest.json"), "entry")?;
    let candidate = root.join("candidate/manifest.json");
    if candidate.is_file() {
        named.extend(entry_obligations(&candidate, "candidate")?);
    }
    for (oid, witness) in named {
        let Some(kind) = kind_of(&oid) else {
            problems.push(Problem::new(
                at(&witness),
                format!("{oid} is not <kind>:<name> with a ledger kind"),
            ));
            continue;
        };
        let o = map.entry(oid.clone()).or_insert_with(|| Obligation {
            id: oid.clone(),
            kind,
            status: ObligationStatus::Open,
            witnesses: Vec::new(),
            evidence: None,
        });
        match o.status {
            ObligationStatus::Unmeetable => problems.push(Problem::new(
                at(&witness),
                format!("{oid} is unmeetable in its fragment, yet {witness} witnesses it"),
            )),
            _ => {
                o.status = ObligationStatus::Met;
                o.witnesses.push(witness);
            }
        }
    }

    let mut obligations: Vec<Obligation> = map.into_values().collect();
    for o in &mut obligations {
        o.witnesses.sort_by_key(|w| WitnessRef::parse(w).ok());
        o.witnesses.dedup();
    }
    let ledger = ObligationLedger {
        schema_version: ObligationLedger::VERSION,
        obligations,
    };
    for p in ledger.problems() {
        problems.push(Problem::new(at("ledger"), p));
    }
    Ok((ledger, problems))
}

/// The committed text: one obligation per line, so the file stays small and diffs stay
/// readable at tens of thousands of obligations.
pub fn render(ledger: &ObligationLedger) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{{\"schema_version\":{},\"obligations\":[",
        ledger.schema_version
    );
    let n = ledger.obligations.len();
    for (i, o) in ledger.obligations.iter().enumerate() {
        let line = serde_json::to_string(o).expect("an obligation is plain json");
        let _ = writeln!(out, "{line}{}", if i + 1 < n { "," } else { "" });
    }
    out.push_str("]}\n");
    out
}

#[cfg(test)]
mod gate_evidence_tests {
    use super::*;
    use shapekit_contracts::ObligationLedger;

    fn side(id: &str, status: ObligationStatus, witnesses: &[&str]) -> Obligation {
        Obligation {
            id: id.into(),
            kind: ObligationKind::GateSide,
            status,
            witnesses: witnesses.iter().map(|w| w.to_string()).collect(),
            evidence: None,
        }
    }

    #[test]
    fn met_gate_sides_are_split_by_evidence_and_never_summed() {
        let ledger = ObligationLedger {
            schema_version: 1,
            obligations: vec![
                side(
                    "a",
                    ObligationStatus::Met,
                    &["chain:c.p18", "recipe:r.json@p18"],
                ),
                side("b", ObligationStatus::Met, &["recipe:r.json@p19"]),
                side("c", ObligationStatus::Open, &[]),
                side("d", ObligationStatus::Met, &["entry:e"]),
            ],
        };
        let row = rows(&ledger)
            .into_iter()
            .find(|r| r.kind == "gate-side")
            .unwrap();
        assert_eq!(row.met, None);
        assert_eq!(row.met_branch_count, Some(1));
        assert_eq!(row.met_inferred, Some(1));
        assert_eq!(row.open, 1);
        let p = gate_evidence_problems(&ledger);
        assert_eq!(p.len(), 1);
        assert!(p[0].to_string().contains('d'), "{}", p[0]);
    }
}
