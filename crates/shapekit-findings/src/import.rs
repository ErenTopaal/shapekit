//! Committing the run outputs findings are collected from. Harness reports, divergence
//! matrices and linter results are written under the gitignored `target/`, so a clean
//! checkout could not show that `findings.json` follows from them. `import` copies each one
//! the triage declares under `[[inputs.import]]` to `findings/inputs/`, reduced to what
//! collect reads, and records where it came from in `findings/inputs/provenance.json`.
//!
//! Reductions:
//! - harness report: none, the report is copied byte for byte;
//! - divergence matrix: only failing cells are kept, with only their failing checks, and
//!   check details cut to [`DETAIL_CHARS`] characters; and the checks of implementation-defined
//!   entries, with the answer each SDK gave, for the triage's answer matchers. Collect reads
//!   nothing else, so the collection from the reduced matrix equals the one from the full
//!   matrix;
//! - linter result: none.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::inputs::{self, MatrixView};
use crate::triage::{ImportDef, InputKind, Triage};

pub const PROVENANCE_SCHEMA: u32 = 1;
/// The provenance file of the main triage; another triage names its own `[inputs] dir`.
pub const PROVENANCE_FILE: &str = "findings/inputs/provenance.json";
/// A failing check's detail can hold a whole XDR-JSON document; this much identifies it.
pub const DETAIL_CHARS: usize = 400;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    pub schema: u32,
    pub inputs: Vec<ProvenanceEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProvenanceEntry {
    pub kind: InputKind,
    /// The committed file, relative to the repository root.
    pub path: String,
    pub sha256: String,
    /// The run output it was reduced from.
    pub from: String,
    pub from_sha256: String,
    /// The command that wrote `from`.
    pub command: String,
    /// What the reduction dropped.
    pub reduction: String,
    /// What the input describes, from its own content.
    pub describes: String,
}

fn truncate(s: &str, chars: usize) -> String {
    match s.char_indices().nth(chars) {
        None => s.to_owned(),
        Some((i, _)) => format!("{}... ({} bytes in all)", &s[..i], s.len()),
    }
}

/// `m` without the cells and checks collect never reads. An implementation-defined answer
/// is kept whole, so two answers compare equal exactly when the SDKs gave the same one.
pub fn reduce_matrix(mut m: MatrixView) -> MatrixView {
    for e in &mut m.entries {
        e.cells.retain(|_, c| {
            c.is_failure() || c.checks.iter().any(|k| k.is_implementation_defined())
        });
        for c in e.cells.values_mut() {
            c.checks
                .retain(|k| k.result != "pass" || k.is_implementation_defined());
            for k in &mut c.checks {
                k.detail = k.detail.as_deref().map(|d| truncate(d, DETAIL_CHARS));
                if !k.is_implementation_defined() {
                    k.actual = None;
                }
            }
        }
    }
    m.entries.retain(|e| !e.cells.is_empty());
    m
}

fn pretty<T: Serialize>(v: &T) -> Result<Vec<u8>> {
    let mut out = serde_json::to_vec_pretty(v)?;
    out.push(b'\n');
    Ok(out)
}

fn one(root: &Path, def: &ImportDef) -> Result<ProvenanceEntry> {
    let from = root.join(&def.from);
    let (bytes, from_sha) = inputs::read(&from)?;
    let (out, reduction, describes) = match def.kind {
        InputKind::Harness => {
            let r = inputs::harness_report(&bytes, &from)?;
            let describes = match &r.describe {
                Some(d) => format!(
                    "{} {}, {} entries, manifest {}",
                    d.target, d.target_version, r.totals.entries, r.manifest_sha256
                ),
                None => bail!("{} has no describe; it cannot be attributed", def.from),
            };
            (bytes, "none".to_owned(), describes)
        }
        InputKind::Conformance => {
            let path = from.clone();
            let m: MatrixView = crate::parse_on_big_stack(move || inputs::matrix(&bytes, &path))?;
            let describes = format!(
                "corpus {}, {}",
                m.corpus_version,
                m.testees
                    .iter()
                    .map(|t| t.key.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            let reduced = reduce_matrix(m);
            (
                pretty(&reduced)?,
                format!(
                    "failing cells only, with their failing checks; check details cut to {DETAIL_CHARS} characters; implementation-defined answers kept whole"
                ),
                describes,
            )
        }
        InputKind::Lint => {
            let r = inputs::lint(&bytes, &from)?;
            let describes = format!(
                "{} finding(s), {} baselined, {} suppressed",
                r.findings.len(),
                r.baselined.len(),
                r.suppressed.len()
            );
            (bytes, "none".to_owned(), describes)
        }
    };
    let to = root.join(&def.to);
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    std::fs::write(&to, &out).with_context(|| format!("writing {}", to.display()))?;
    Ok(ProvenanceEntry {
        kind: def.kind,
        path: def.to.clone(),
        sha256: shapekit_manifest::sha256_hex(&out),
        from: def.from.clone(),
        from_sha256: from_sha,
        command: def.command.clone(),
        reduction,
        describes,
    })
}

/// Imports every input the triage declares and writes the provenance file. All sources are
/// read before anything is written, so a missing run output leaves the committed inputs as
/// they were.
pub fn import(root: &Path, triage: &Triage) -> Result<Provenance> {
    if triage.inputs.import.is_empty() {
        bail!("the triage declares no [[inputs.import]], so there is nothing to import");
    }
    for def in &triage.inputs.import {
        let from = root.join(&def.from);
        if !from.is_file() {
            bail!(
                "{} does not exist; run `{}` first",
                from.display(),
                def.command
            );
        }
    }
    let mut entries = Vec::new();
    for def in &triage.inputs.import {
        entries.push(one(root, def).with_context(|| format!("importing {}", def.from))?);
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let p = Provenance {
        schema: PROVENANCE_SCHEMA,
        inputs: entries,
    };
    let path = root.join(triage.inputs.provenance_file());
    std::fs::write(&path, pretty(&p)?).with_context(|| format!("writing {}", path.display()))?;
    Ok(p)
}

/// Problems with the committed inputs: a declared import without a provenance record, a
/// record that disagrees with the declaration, or a committed file whose bytes changed
/// since it was imported. Inputs outside `findings/inputs/` are reported too, since a clean
/// checkout cannot collect from them.
pub fn verify(root: &Path, triage: &Triage) -> Vec<String> {
    let mut out = Vec::new();
    let inputs = &triage.inputs;
    for rel in inputs
        .harness
        .iter()
        .chain(&inputs.matrices)
        .chain(inputs.lint.iter().map(|l| &l.result))
    {
        if !Path::new(rel).starts_with(inputs.dir()) {
            out.push(format!(
                "input {rel} is not committed under {}/; declare it in [[inputs.import]] and run `shapekit findings import-inputs`",
                inputs.dir()
            ));
        } else if !inputs.import.iter().any(|i| &i.to == rel) {
            out.push(format!(
                "input {rel} has no [[inputs.import]], so its provenance is unknown"
            ));
        }
    }
    if inputs.import.is_empty() {
        return out;
    }
    let provenance_file = inputs.provenance_file();
    let path = root.join(&provenance_file);
    let prov: Provenance = match std::fs::read(&path)
        .map_err(|e| e.to_string())
        .and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string()))
    {
        Ok(p) => p,
        Err(e) => {
            out.push(format!(
                "{provenance_file}: {e}; run `shapekit findings import-inputs`"
            ));
            return out;
        }
    };
    for def in &inputs.import {
        let Some(rec) = prov.inputs.iter().find(|r| r.path == def.to) else {
            out.push(format!(
                "{} is declared but has no provenance record; run `shapekit findings import-inputs`",
                def.to
            ));
            continue;
        };
        if rec.kind != def.kind || rec.from != def.from || rec.command != def.command {
            out.push(format!(
                "{}: the provenance record (from {}, `{}`) is not the triage's declaration (from {}, `{}`); run `shapekit findings import-inputs`",
                def.to, rec.from, rec.command, def.from, def.command
            ));
        }
        match std::fs::read(root.join(&def.to)) {
            Ok(b) if shapekit_manifest::sha256_hex(&b) == rec.sha256 => {}
            Ok(_) => out.push(format!(
                "{} changed since it was imported (sha256 differs from {provenance_file})",
                def.to
            )),
            Err(e) => out.push(format!("{}: {e}", def.to)),
        }
    }
    for rec in &prov.inputs {
        if !inputs.import.iter().any(|d| d.to == rec.path) {
            out.push(format!(
                "{provenance_file} records {}, which the triage does not declare",
                rec.path
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inputs::{CellView, CheckView, MatrixEntry, TesteeView};
    use crate::triage::{Inputs, Triage};
    use std::collections::BTreeMap;

    fn cell(result: &str, checks: &[(&str, &str)]) -> CellView {
        CellView {
            result: result.into(),
            known_issue: None,
            checks: checks
                .iter()
                .map(|(c, r)| CheckView {
                    check: (*c).into(),
                    op: "decode".into(),
                    result: (*r).into(),
                    detail: Some("x".repeat(DETAIL_CHARS + 10)),
                    actual: Some(serde_json::json!({"xdr": "AAAA"})),
                })
                .collect(),
        }
    }

    fn matrix() -> MatrixView {
        MatrixView {
            corpus_version: "0.1.0".into(),
            testees: vec![TesteeView {
                key: "go@v1".into(),
                sdk: "go".into(),
                package: "p".into(),
                version: "v1".into(),
            }],
            entries: vec![
                MatrixEntry {
                    id: "a".into(),
                    cells: BTreeMap::from([("go@v1".into(), cell("pass", &[("decode", "pass")]))]),
                },
                MatrixEntry {
                    id: "b".into(),
                    cells: BTreeMap::from([(
                        "go@v1".into(),
                        cell(
                            "diverges-from-network",
                            &[("decode", "pass"), ("json", "diverges-from-network")],
                        ),
                    )]),
                },
            ],
            known_failure_problems: vec![],
        }
    }

    #[test]
    fn reduction_keeps_only_failures() {
        let r = reduce_matrix(matrix());
        assert_eq!(r.entries.len(), 1);
        let c = &r.entries[0].cells["go@v1"];
        assert_eq!(c.checks.len(), 1);
        assert_eq!(c.checks[0].check, "json");
        assert!(c.checks[0].detail.as_ref().unwrap().len() < DETAIL_CHARS + 40);
        assert_eq!(
            c.checks[0].actual, None,
            "a failing check's answer is not kept"
        );
    }

    #[test]
    fn reduction_keeps_implementation_defined_answers_whole() {
        let mut m = matrix();
        let mut c = cell("pass", &[("any/decode_json", "pass"), ("other", "pass")]);
        c.checks[0].detail = Some(format!(
            "{}: no answer is required, so none is judged",
            crate::inputs::IMPLEMENTATION_DEFINED
        ));
        let long = serde_json::json!({"json": "y".repeat(DETAIL_CHARS * 2)});
        c.checks[0].actual = Some(long.clone());
        m.entries.push(MatrixEntry {
            id: "c".into(),
            cells: BTreeMap::from([("go@v1".into(), c)]),
        });
        let r = reduce_matrix(m);
        assert_eq!(r.entries.len(), 2);
        let c = &r.entries[1].cells["go@v1"];
        assert_eq!(c.checks.len(), 1);
        assert_eq!(c.checks[0].check, "any/decode_json");
        assert_eq!(c.checks[0].actual, Some(long));
    }

    fn triage(from: &str) -> Triage {
        Triage {
            schema: 1,
            candidates: None,
            inputs: Inputs {
                manifest: "corpus/manifest.json".into(),
                harness: vec![],
                matrices: vec!["findings/inputs/conformance-go.json".into()],
                lint: vec![],
                import: vec![ImportDef {
                    kind: InputKind::Conformance,
                    from: from.into(),
                    to: "findings/inputs/conformance-go.json".into(),
                    command: "shapekit conformance run --sdk go --all-versions".into(),
                }],
                dir: None,
            },
            targets: BTreeMap::new(),
            findings: vec![],
        }
    }

    #[test]
    fn imports_then_detects_edits() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("target/c")).unwrap();
        std::fs::create_dir_all(root.join("findings")).unwrap();
        // the raw matrix carries fields collect does not read
        let mut raw = serde_json::to_value(matrix()).unwrap();
        raw["generated_by"] = "x".into();
        std::fs::write(
            root.join("target/c/m.json"),
            serde_json::to_vec(&raw).unwrap(),
        )
        .unwrap();
        let t = triage("target/c/m.json");
        crate::triage::validate(&t).unwrap();
        assert!(!verify(root, &t).is_empty(), "nothing imported yet");
        let p = import(root, &t).unwrap();
        assert_eq!(p.inputs.len(), 1);
        assert!(verify(root, &t).is_empty(), "{:?}", verify(root, &t));
        let to = root.join("findings/inputs/conformance-go.json");
        let parsed = inputs::matrix(&std::fs::read(&to).unwrap(), &to).unwrap();
        assert_eq!(parsed, reduce_matrix(matrix()));
        std::fs::write(&to, b"{}").unwrap();
        assert_eq!(verify(root, &t).len(), 1);
        // a missing source leaves the committed input alone
        let gone = triage("target/c/none.json");
        assert!(import(root, &gone).is_err());
        assert_eq!(std::fs::read(&to).unwrap(), b"{}");
    }
}
