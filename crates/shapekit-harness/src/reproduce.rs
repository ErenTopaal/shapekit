//! `shapekit harness reproduce`: pairs of target builds over named chains, with the verdicts
//! named entries must get. The table format is in `docs/adapter-interface.md`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::compare::{Reason, Verdict};
use crate::corpus::{Filter, select};
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
            let holds = error.is_none() && expectations.iter().all(|e| e.holds);
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

#[cfg(test)]
mod tests {
    use super::short;

    #[test]
    fn short_commits_never_split_a_character() {
        assert_eq!(short("e967ce8b7362b79c277e"), "e967ce8b7362");
        assert_eq!(short("abc"), "abc");
        assert_eq!(short("ééééééééééééé"), "éééééééééééé");
    }
}
