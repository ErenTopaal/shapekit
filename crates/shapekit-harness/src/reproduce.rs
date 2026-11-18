//! `shapekit harness reproduce`: pairs of target builds over named chains, with the verdicts
//! named entries must get. The table format is in `docs/adapter-interface.md`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::compare::{Reason, Verdict};
use crate::corpus::{Filter, select};
use crate::relations::carry::ProblemKind;
use crate::report::{self, MemoryOptions};
use crate::run::RunOptions;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    pub adapter: AdapterSpec,
    pub reproduction: Vec<Reproduction>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterSpec {
    /// Program and arguments; `{commit}` is replaced by each case's commit.
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reproduction {
    pub id: String,
    pub issue: String,
    pub chain: String,
    pub network: String,
    #[serde(default)]
    pub notes: Option<String>,
    pub case: Vec<Case>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub label: String,
    pub commit: String,
    pub expect: Vec<Expect>,
    /// The carry check's outcome on the case's chain: the exact sets of `table.column` with
    /// truncated and with fabricated values. Needs an adapter that offers `dump`.
    #[serde(default)]
    pub carry: Option<CarryExpect>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CarryExpect {
    pub truncated: Vec<String>,
    pub fabricated: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expect {
    pub entry: String,
    pub verdict: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectResult {
    pub entry: String,
    pub expected: String,
    pub observed: String,
    pub detail: String,
    pub holds: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarryResult {
    pub expected_truncated: Vec<String>,
    pub observed_truncated: Vec<String>,
    pub expected_fabricated: Vec<String>,
    pub observed_fabricated: Vec<String>,
    /// Why the chain has no carry report, when it has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing: Option<String>,
    pub holds: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseResult {
    pub reproduction: String,
    pub issue: String,
    pub chain: String,
    pub network: String,
    pub label: String,
    pub commit: String,
    pub report: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub expectations: Vec<ExpectResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carry: Option<CarryResult>,
    pub holds: bool,
}

pub fn load(path: &Path) -> Result<Table> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let table: Table =
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    for r in &table.reproduction {
        for c in &r.case {
            for e in &c.expect {
                if Verdict::parse(&e.verdict).is_none() {
                    bail!("{} {}: unknown verdict {:?}", r.id, c.label, e.verdict);
                }
                if let Some(x) = &e.reason
                    && Reason::parse(x).is_none()
                {
                    bail!("{} {}: unknown reason {x:?}", r.id, c.label);
                }
            }
        }
    }
    if !table.adapter.command.iter().any(|a| a.contains("{commit}")) {
        bail!(
            "{}: adapter.command has no {{commit}} placeholder",
            path.display()
        );
    }
    Ok(table)
}

fn label(verdict: &str, reason: Option<&str>) -> String {
    match reason {
        Some(r) => format!("{verdict} ({r})"),
        None => verdict.to_owned(),
    }
}

/// Runs every case of `table` (only reproductions named in `only`, when given) and writes
/// each case's report under `out/<id>/<label>/`.
pub fn run(
    table: &Table,
    only: &[String],
    corpus: &Path,
    base: &RunOptions,
    out: &Path,
) -> Result<Vec<CaseResult>> {
    unknown_ids(table, only)?;
    let mut results = Vec::new();
    for r in table
        .reproduction
        .iter()
        .filter(|r| only.is_empty() || only.contains(&r.id))
    {
        let sel = select(
            corpus,
            &Filter {
                families: &[],
                chains: std::slice::from_ref(&r.chain),
                networks: std::slice::from_ref(&r.network),
            },
        )?;
        for c in &r.case {
            let dir: PathBuf = out.join(&r.id).join(&c.label);
            let mut opts = base.clone();
            opts.adapter = table
                .adapter
                .command
                .iter()
                .map(|a| a.replace("{commit}", &c.commit))
                .collect();
            eprintln!("reproduce: {} {} ({})", r.id, c.label, c.commit);
            let rep = report::run(
                &sel,
                &opts,
                &dir,
                &MemoryOptions {
                    rounds: 1,
                    slack_fraction: 0.1,
                    slack_min_bytes: 16 * 1024 * 1024,
                    budget_bytes: None,
                },
            )?;
            report::write(&rep, &dir)?;
            let error = rep.chains.iter().find_map(|ch| ch.error.clone());
            let mut expectations = Vec::new();
            for e in &c.expect {
                let expected = label(&e.verdict, e.reason.as_deref());
                let row = rep.entries.iter().find(|x| x.entry == e.entry);
                let (observed, detail, holds) = match row {
                    None => (
                        "absent".to_owned(),
                        format!("entry {} is not in the run", e.entry),
                        false,
                    ),
                    Some(x) => {
                        let v = x.result.verdict.name();
                        let rs = x.result.reason.map(|r| r.name());
                        let holds = v == e.verdict
                            && e.reason.as_deref().is_none_or(|want| rs == Some(want));
                        (label(v, rs), x.result.detail.clone(), holds)
                    }
                };
                expectations.push(ExpectResult {
                    entry: e.entry.clone(),
                    expected,
                    observed,
                    detail,
                    holds,
                });
            }
            let carry = c.carry.as_ref().map(|want| {
                let chain = rep
                    .chains
                    .iter()
                    .find(|ch| ch.chain == r.chain && ch.network == r.network);
                let sorted = |v: &[String]| {
                    let mut v = v.to_vec();
                    v.sort();
                    v.dedup();
                    v
                };
                let (exp_t, exp_f) = (sorted(&want.truncated), sorted(&want.fabricated));
                match chain.and_then(|ch| ch.carry.as_ref()) {
                    Some(cr) => {
                        let (obs_t, obs_f) = (
                            cr.columns_with(ProblemKind::Truncated),
                            cr.columns_with(ProblemKind::Fabricated),
                        );
                        CarryResult {
                            holds: obs_t == exp_t && obs_f == exp_f,
                            expected_truncated: exp_t,
                            observed_truncated: obs_t,
                            expected_fabricated: exp_f,
                            observed_fabricated: obs_f,
                            missing: None,
                        }
                    }
                    None => CarryResult {
                        expected_truncated: exp_t,
                        observed_truncated: Vec::new(),
                        expected_fabricated: exp_f,
                        observed_fabricated: Vec::new(),
                        missing: Some(
                            chain
                                .and_then(|ch| ch.dump_error.clone())
                                .unwrap_or_else(|| "the adapter took no dump".into()),
                        ),
                        holds: false,
                    },
                }
            });
            let holds = error.is_none()
                && expectations.iter().all(|e| e.holds)
                && carry.as_ref().is_none_or(|c| c.holds);
            results.push(CaseResult {
                reproduction: r.id.clone(),
                issue: r.issue.clone(),
                chain: r.chain.clone(),
                network: r.network.clone(),
                label: c.label.clone(),
                commit: c.commit.clone(),
                report: dir.join(report::REPORT_FILE).display().to_string(),
                error,
                expectations,
                carry,
                holds,
            });
        }
    }
    Ok(results)
}

/// The first 12 characters of a commit, never splitting a character.
fn short(commit: &str) -> String {
    commit.chars().take(12).collect()
}

pub fn markdown(results: &[CaseResult]) -> String {
    let mut s = String::from(
        "# Reproductions\n\n| reproduction | case | commit | entry | expected | observed | holds |\n|---|---|---|---|---|---|---|\n",
    );
    for c in results {
        for e in &c.expectations {
            s.push_str(&format!(
                "| {} | {} | `{}` | {} | {} | {} | {} |\n",
                c.reproduction,
                c.label,
                short(&c.commit),
                e.entry,
                e.expected,
                e.observed,
                if e.holds { "yes" } else { "no" }
            ));
        }
        if let Some(x) = &c.carry {
            let show = |v: &[String]| {
                if v.is_empty() {
                    "none".to_owned()
                } else {
                    v.join(", ")
                }
            };
            s.push_str(&format!(
                "| {} | {} | `{}` | carry check | truncated: {}; fabricated: {} | {} | {} |\n",
                c.reproduction,
                c.label,
                short(&c.commit),
                show(&x.expected_truncated),
                show(&x.expected_fabricated),
                match &x.missing {
                    Some(m) => format!("no carry report: {}", m.replace('|', "\\|")),
                    None => format!(
                        "truncated: {}; fabricated: {}",
                        show(&x.observed_truncated),
                        show(&x.observed_fabricated)
                    ),
                },
                if x.holds { "yes" } else { "no" }
            ));
        }
        if let Some(err) = &c.error {
            s.push_str(&format!(
                "| {} | {} | `{}` | | | error: {} | no |\n",
                c.reproduction,
                c.label,
                short(&c.commit),
                err.replace('|', "\\|").replace('\n', " ")
            ));
        }
    }
    s
}

pub fn write(results: &[CaseResult], out: &Path) -> Result<()> {
    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    let mut text = serde_json::to_string_pretty(results).context("rendering reproductions")?;
    text.push('\n');
    let json = out.join("reproductions.json");
    std::fs::write(&json, text).with_context(|| format!("writing {}", json.display()))?;
    let md = out.join("REPRODUCTIONS.md");
    std::fs::write(&md, markdown(results)).with_context(|| format!("writing {}", md.display()))?;
    Ok(())
}

/// Fails on an id in `only` the table does not hold: a misspelled id would otherwise
/// select nothing and pass.
fn unknown_ids(table: &Table, only: &[String]) -> Result<()> {
    let unknown: Vec<&str> = only
        .iter()
        .filter(|id| !table.reproduction.iter().any(|r| &r.id == *id))
        .map(String::as_str)
        .collect();
    if !unknown.is_empty() {
        bail!(
            "no reproduction has the id {}; the table holds {}",
            unknown.join(", "),
            table
                .reproduction
                .iter()
                .map(|r| r.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{AdapterSpec, Reproduction, Table, short, unknown_ids};

    #[test]
    fn an_unknown_only_id_is_an_error() {
        let table = Table {
            adapter: AdapterSpec {
                command: vec!["a".into()],
            },
            reproduction: vec![Reproduction {
                id: "wb-716".into(),
                issue: "i".into(),
                chain: "c".into(),
                network: "testnet".into(),
                notes: None,
                case: vec![],
            }],
        };
        assert!(unknown_ids(&table, &[]).is_ok());
        assert!(unknown_ids(&table, &["wb-716".into()]).is_ok());
        let e = unknown_ids(&table, &["wb-761".into()])
            .unwrap_err()
            .to_string();
        assert!(e.contains("no reproduction has the id wb-761"), "{e}");
        assert!(e.contains("wb-716"), "{e}");
    }

    #[test]
    fn short_commits_never_split_a_character() {
        assert_eq!(short("e967ce8b7362b79c277e"), "e967ce8b7362");
        assert_eq!(short("abc"), "abc");
        assert_eq!(short("ééééééééééééé"), "éééééééééééé");
    }
}
