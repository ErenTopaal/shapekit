//! `shapekit conformance crosscheck`: every `expected_json` in the corpus is stellar-xdr's own
//! XDR-JSON rendering (the generator's version), so the rust testee at that version agrees with
//! it by construction. This command asks independent decoders, the go and js testees at their
//! current versions, to render the same bytes, and reports every entry where one of them
//! renders something else, classified by who disagrees with whom.
//!
//! The command succeeds only when it ran at least one check, every check was confirmed by at
//! least one decoder agreeing with `expected_json`, and every disagreement is one decoder
//! differing while another agrees, with that decoder's known-failures list already holding the
//! check. Two decoders that differ the same way point at stellar-xdr's rendering instead, and
//! fail it; a check no decoder rendered as expected is not cross-checked at all, and fails it.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::compare::{Outcome, ResultClass};
use crate::known;
use crate::plan::{self, Expect};
use crate::runner::{self, Selection};
use crate::testee::{ErrorClass, Reply};
use crate::versions::VersionsFile;

pub const CROSSCHECK_SCHEMA: u32 = 1;

/// The sdks whose decoders are independent of stellar-xdr.
pub const DEFAULT_SDKS: &[&str] = &["go", "js"];

pub struct CrosscheckOptions {
    pub root: PathBuf,
    pub manifest: PathBuf,
    pub versions: PathBuf,
    pub known_dir: PathBuf,
    /// Sdks to ask, each at its current version; `DEFAULT_SDKS` when empty.
    pub sdks: Vec<String>,
    pub selection: Selection,
    pub build: bool,
    /// Where crosscheck.json and CROSSCHECK.md go; nothing is written when `None`.
    pub out: Option<PathBuf>,
}

/// What one decoder answered for one check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "kebab-case")]
pub enum Answer {
    Agrees,
    /// Renders other json; `at` is the json pointer of the first difference.
    Differs {
        at: String,
        expected: Value,
        actual: Value,
    },
    Rejected {
        message: String,
    },
    Unsupported {
        reason: String,
    },
    Error {
        detail: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Disagreement {
    pub entry: String,
    pub check: String,
    /// Keyed by testee (`go@v0.9.0`).
    pub answers: BTreeMap<String, Answer>,
    pub class: Class,
    /// Known-failures issues that list this check, by testee.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub known: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Class {
    /// Two or more decoders render the same json and it is not `expected_json`.
    PossibleStellarXdrRendering,
    /// Two or more decoders differ, each differently.
    AllDiffer,
    /// One decoder differs and no other decoder rendered the value.
    OneDiffersAlone,
    /// One decoder differs while another agrees, and its known-failures list lacks the check.
    UnknownSdkDivergence,
    /// One decoder differs while another agrees, and its known-failures list holds the check.
    KnownSdkDivergence,
}

impl Class {
    pub fn as_str(self) -> &'static str {
        match self {
            Class::PossibleStellarXdrRendering => {
                "independent decoders agree with each other, not with expected_json: possible stellar-xdr rendering issue"
            }
            Class::AllDiffer => "every decoder that rendered it differs, each differently",
            Class::OneDiffersAlone => "one decoder differs and no other rendered the value",
            Class::UnknownSdkDivergence => {
                "one decoder differs while another agrees; not in its known-failures list"
            }
            Class::KnownSdkDivergence => {
                "one decoder differs while another agrees; already in its known-failures list"
            }
        }
    }

    /// Whether this class fails the command.
    pub fn fails(self) -> bool {
        self != Class::KnownSdkDivergence
    }
}

/// `crosscheck.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub schema_version: u32,
    pub corpus_version: String,
    pub generator_stellar_xdr: String,
    pub decoders: Vec<String>,
    /// decode_json checks with a canonical `expected_json`.
    pub checks: usize,
    /// How many checks each number of decoders confirmed (agreed with `expected_json`).
    pub confirmed_by: BTreeMap<usize, usize>,
    /// Per decoder, how many checks it answered other than agreeing or differing.
    pub not_rendered: BTreeMap<String, BTreeMap<String, usize>>,
    pub disagreements: Vec<Disagreement>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.problems().is_empty()
    }

    /// Why the command fails, one line each; empty when it passes.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.checks == 0 {
            out.push(
                "no decode_json check with a canonical expected_json was run, so nothing was cross-checked"
                    .to_string(),
            );
        }
        if let Some(n) = self.confirmed_by.get(&0).filter(|n| **n > 0) {
            out.push(format!(
                "{n} checks were confirmed by no decoder: none rendered expected_json"
            ));
        }
        for d in self.disagreements.iter().filter(|d| d.class.fails()) {
            out.push(format!("{} `{}`: {}", d.entry, d.check, d.class.as_str()));
        }
        out
    }
}

/// The json pointer of the first difference between two values, with the two sides there.
pub fn first_difference(expected: &Value, actual: &Value) -> Option<(String, Value, Value)> {
    fn walk(e: &Value, a: &Value, at: &mut String) -> Option<(String, Value, Value)> {
        match (e, a) {
            (Value::Object(x), Value::Object(y)) => {
                for (k, ev) in x {
                    let len = at.len();
                    at.push('/');
                    at.push_str(&k.replace('~', "~0").replace('/', "~1"));
                    let found = match y.get(k) {
                        Some(av) => walk(ev, av, at),
                        None => Some((at.clone(), brief(ev), Value::String("(absent)".into()))),
                    };
                    if found.is_some() {
                        return found;
                    }
                    at.truncate(len);
                }
                y.iter().find(|(k, _)| !x.contains_key(*k)).map(|(k, av)| {
                    (
                        format!("{at}/{}", k.replace('~', "~0").replace('/', "~1")),
                        Value::String("(absent)".into()),
                        brief(av),
                    )
                })
            }
            (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
                for (i, (ev, av)) in x.iter().zip(y).enumerate() {
                    let len = at.len();
                    let _ = write!(at, "/{i}");
                    if let Some(found) = walk(ev, av, at) {
                        return Some(found);
                    }
                    at.truncate(len);
                }
                None
            }
            _ if e == a => None,
            _ => Some((at.clone(), brief(e), brief(a))),
        }
    }
    walk(expected, actual, &mut String::new())
}

/// Classifies the answers to one check, `known` naming the decoders whose list holds it;
/// `None` when no decoder differs. Two or more decoders differing are `AllDiffer` here and
/// become `PossibleStellarXdrRendering` when they render the same thing (`same_rendering`).
pub fn classify(
    answers: &BTreeMap<String, Answer>,
    known: &BTreeMap<String, String>,
) -> Option<Class> {
    let differ: Vec<&String> = answers
        .iter()
        .filter(|(_, a)| matches!(a, Answer::Differs { .. }))
        .map(|(k, _)| k)
        .collect();
    let agree = answers.values().any(|a| *a == Answer::Agrees);
    Some(match differ.as_slice() {
        [] => return None,
        [_] if !agree => Class::OneDiffersAlone,
        [one] if known.contains_key(*one) => Class::KnownSdkDivergence,
        [_] => Class::UnknownSdkDivergence,
        _ if same_rendering(answers) => Class::PossibleStellarXdrRendering,
        _ => Class::AllDiffer,
    })
}

// a value for a report line: deep renderings are cut to a short string
fn brief(v: &Value) -> Value {
    const MAX: usize = 300;
    let text = v.to_string();
    if text.len() <= MAX {
        return v.clone();
    }
    let mut cut = MAX;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    Value::String(format!("{}... ({} chars)", &text[..cut], text.len()))
}

pub fn run(opts: &CrosscheckOptions) -> Result<Report> {
    let file = VersionsFile::load(&opts.versions)?;
    let sdks: Vec<String> = if opts.sdks.is_empty() {
        DEFAULT_SDKS.iter().map(|s| s.to_string()).collect()
    } else {
        opts.sdks.clone()
    };
    let (manifest, corpus) = runner::load_manifest(&opts.manifest)?;
    let targets = file.select(&sdks, false)?;
    for t in &targets {
        ensure!(
            !(t.package == runner::REFERENCE_PACKAGE
                && t.version == manifest.generated_with.stellar_xdr),
            "{} rendered expected_json itself, so it cannot cross-check it",
            t.key()
        );
    }
    let selected = opts.selection.select(&manifest)?;
    ensure!(!selected.is_empty(), "the selection matches no entry");
    let mut lines = BTreeMap::new();
    for t in &targets {
        lines.insert(t.key(), known::load(&opts.known_dir, &t.sdk)?);
    }
    let mut testees = runner::start_all(&opts.root, &targets, opts.build)?;

    let mut checks = 0usize;
    let mut confirmed_by: BTreeMap<usize, usize> = BTreeMap::new();
    let mut not_rendered: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut disagreements = Vec::new();
    for entry in &selected {
        for check in
            plan::checks(entry, &corpus).with_context(|| format!("planning {}", entry.id))?
        {
            let Expect::Json(expected) = &check.expect else {
                continue;
            };
            if check.op != "decode_json" {
                continue;
            }
            checks += 1;
            let mut answers = BTreeMap::new();
            for t in testees.iter_mut() {
                let key = t.target().key();
                let answer = if !t.supports("decode_json") {
                    Answer::Unsupported {
                        reason: format!("{key} does not implement decode_json"),
                    }
                } else {
                    match t.request(check.request.clone())? {
                        Err(failure) => Answer::Error {
                            detail: failure.to_string(),
                        },
                        Ok(Reply::Unsupported(reason)) => Answer::Unsupported { reason },
                        Ok(Reply::Error {
                            class: ErrorClass::Rejected,
                            message,
                        }) => Answer::Rejected { message },
                        Ok(Reply::Error { class, message }) => Answer::Error {
                            detail: format!("{}: {message}", class.as_str()),
                        },
                        Ok(Reply::Ok(result)) => match result.get("json") {
                            None => Answer::Error {
                                detail: "protocol violation: result has no json".into(),
                            },
                            Some(actual) => match first_difference(expected, actual) {
                                None => Answer::Agrees,
                                Some((at, expected, actual)) => Answer::Differs {
                                    at,
                                    expected,
                                    actual,
                                },
                            },
                        },
                    }
                };
                let label = match &answer {
                    Answer::Agrees | Answer::Differs { .. } => None,
                    Answer::Rejected { .. } => Some("rejected"),
                    Answer::Unsupported { .. } => Some("unsupported"),
                    Answer::Error { .. } => Some("error"),
                };
                if let Some(label) = label {
                    *not_rendered
                        .entry(key.clone())
                        .or_default()
                        .entry(label.to_string())
                        .or_insert(0) += 1;
                }
                answers.insert(key, answer);
            }
            let agreeing = answers.values().filter(|a| **a == Answer::Agrees).count();
            *confirmed_by.entry(agreeing).or_insert(0) += 1;
            let mut known_issues = BTreeMap::new();
            for t in &targets {
                if let Some(l) = lines[&t.key()].iter().find(|l| {
                    // a decoder rendering other json answered wrong, it did not crash
                    l.result == Outcome::of(ResultClass::DivergesFromNetwork)
                        && l.covers_check(&entry.id, &t.version, &check.name)
                }) {
                    known_issues.insert(t.key(), l.issue.clone());
                }
            }
            let Some(class) = classify(&answers, &known_issues) else {
                continue;
            };
            disagreements.push(Disagreement {
                entry: entry.id.clone(),
                check: check.name.clone(),
                answers,
                class,
                known: known_issues,
            });
        }
    }
    drop(testees);
    let report = Report {
        schema_version: CROSSCHECK_SCHEMA,
        corpus_version: manifest.corpus_version.clone(),
        generator_stellar_xdr: manifest.generated_with.stellar_xdr.clone(),
        decoders: targets.iter().map(|t| t.key()).collect(),
        checks,
        confirmed_by,
        not_rendered,
        disagreements,
    };
    if let Some(out) = &opts.out {
        write_outputs(out, &report)?;
    }
    Ok(report)
}

// whether every differing decoder differs at the same place with the same value
fn same_rendering(answers: &BTreeMap<String, Answer>) -> bool {
    let mut seen: Option<(&String, &Value)> = None;
    for a in answers.values() {
        if let Answer::Differs { at, actual, .. } = a {
            match seen {
                None => seen = Some((at, actual)),
                Some((s_at, s_actual)) if s_at == at && s_actual == actual => {}
                Some(_) => return false,
            }
        }
    }
    seen.is_some()
}

pub fn write_outputs(dir: &Path, report: &Report) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let json = serde_json::to_string_pretty(report).context("rendering the crosscheck report")?;
    let json_path = dir.join("crosscheck.json");
    std::fs::write(&json_path, json + "\n")
        .with_context(|| format!("writing {}", json_path.display()))?;
    let md_path = dir.join("CROSSCHECK.md");
    std::fs::write(&md_path, render(report))
        .with_context(|| format!("writing {}", md_path.display()))?;
    Ok(())
}

fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

pub fn render(r: &Report) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# expected_json cross-check\n");
    let _ = writeln!(
        out,
        "Corpus {}. Every `expected_json` is stellar-xdr {}'s rendering; {} `decode_json` checks with a canonical `expected_json` were sent to {}. Generated by `shapekit conformance crosscheck`.\n",
        r.corpus_version,
        r.generator_stellar_xdr,
        r.checks,
        r.decoders.join(" and ")
    );
    let _ = writeln!(out, "## Confirmation\n");
    let _ = writeln!(out, "| decoders agreeing | checks |\n|---|---|");
    for (n, c) in &r.confirmed_by {
        let _ = writeln!(out, "| {n} of {} | {c} |", r.decoders.len());
    }
    if !r.not_rendered.is_empty() {
        let _ = writeln!(out, "\nChecks a decoder did not render:\n");
        for (k, by) in &r.not_rendered {
            let parts: Vec<String> = by.iter().map(|(l, n)| format!("{l} {n}")).collect();
            let _ = writeln!(out, "- {k}: {}", parts.join(", "));
        }
    }
    let _ = writeln!(out, "\n## Disagreements by class\n");
    let mut by_class: BTreeMap<Class, usize> = BTreeMap::new();
    for d in &r.disagreements {
        *by_class.entry(d.class).or_insert(0) += 1;
    }
    if by_class.is_empty() {
        let _ = writeln!(out, "None.");
    }
    for (c, n) in &by_class {
        let _ = writeln!(out, "- {n}: {}", c.as_str());
    }
    if !r.disagreements.is_empty() {
        let _ = writeln!(out, "\n## Disagreements\n");
        let mut header = String::from("| entry | check | class |");
        let mut rule = String::from("|---|---|---|");
        for k in &r.decoders {
            let _ = write!(header, " {k} |");
            rule.push_str("---|");
        }
        let _ = writeln!(out, "{header}\n{rule}");
        for d in &r.disagreements {
            let mut line = format!("| `{}` | `{}` | {} |", d.entry, d.check, d.class.as_str());
            for k in &r.decoders {
                let text = match d.answers.get(k) {
                    Some(Answer::Agrees) => "agrees".to_string(),
                    Some(Answer::Differs {
                        at,
                        expected,
                        actual,
                    }) => format!("differs at `{at}`: expected `{expected}`, renders `{actual}`"),
                    Some(Answer::Rejected { .. }) => "rejected".into(),
                    Some(Answer::Unsupported { .. }) => "unsupported".into(),
                    Some(Answer::Error { detail }) => format!("error: {detail}"),
                    None => "not run".into(),
                };
                let text = match d.known.get(k) {
                    Some(issue) => format!("{text} (known: {issue})"),
                    None => text,
                };
                let _ = write!(line, " {} |", cell(&text));
            }
            let _ = writeln!(out, "{line}");
        }
    }
    out
}
