//! `harness-report.json` and `HARNESS.md`, and the repeated-run memory check.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::compare::Verdict;
use crate::corpus::Selection;
use crate::memory;
use crate::protocol::{ADAPTER_PROTOCOL_VERSION, DescribeResult};
use crate::run::{ChainResult, EntryRow, RunOptions, logs_dir, run_round, verdict_digest};

pub const REPORT_FILE: &str = "harness-report.json";
pub const MARKDOWN_FILE: &str = "HARNESS.md";
pub const REPORT_SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Round {
    pub round: u32,
    /// Peak resident memory of the harness process during the round (`VmHWM`, reset at the
    /// start of the round).
    pub peak_rss_bytes: u64,
    pub rss_after_bytes: u64,
    pub seconds: f64,
    /// Whether every verdict equals the first round's.
    pub same_verdicts_as_first: bool,
    /// Chains of this round that could not be run, with the reason.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chain_errors: Vec<String>,
    /// Per `<chain> (<network>)`, the target's peak memory as the adapter reported it in
    /// `status` (its container's peak). Chains whose adapter reported none are absent.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub target_peak_memory_bytes: BTreeMap<String, u64>,
}

/// A later round's target peak on a chain may exceed the first round's on that chain by
/// the larger of `--memory-slack` of it and this floor. A container's peak includes page
/// cache and database buffers that vary from run to run by tens of MiB, so the floor is
/// wider than the harness's own.
pub const TARGET_SLACK_MIN_BYTES: u64 = 64 * 1024 * 1024;

/// The indexer side of the memory evidence: the target's peak per chain against the
/// configured limit, and across `--repeat` rounds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TargetMemoryCheck {
    pub limit_bytes: u64,
    /// The highest peak of any chain in any round.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peak_bytes: Option<u64>,
    /// No chain's peak exceeded `limit_bytes`. Such a chain also stops with reason `oom`.
    pub within_limit: bool,
    /// No chain's peak in a later round grew past its first-round peak plus slack.
    pub flat: bool,
    /// Chains whose adapter reported no peak, so there is no evidence for them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unreported: Vec<String>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryCheck {
    /// Allowed growth of a later round's peak over the first round's.
    pub slack_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_bytes: Option<u64>,
    pub flat: bool,
    pub within_budget: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Totals {
    pub entries: usize,
    pub by_verdict: BTreeMap<String, usize>,
    pub by_reason: BTreeMap<String, usize>,
    /// Selected entries that cannot be replayed (no chain, or no oracle file).
    pub without_chain: usize,
    pub without_oracle: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub schema: u32,
    pub shapekit_version: String,
    pub adapter_protocol: u32,
    pub oracle_vocab_version: u32,
    pub adapter_command: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub describe: Option<DescribeResult>,
    pub manifest_sha256: String,
    pub memory_limit_bytes: u64,
    pub rounds: Vec<Round>,
    pub memory: MemoryCheck,
    /// Absent in reports written before the target's memory was checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_memory: Option<TargetMemoryCheck>,
    pub totals: Totals,
    /// The first round's chains and entries.
    pub chains: Vec<ChainResult>,
    pub entries: Vec<EntryRow>,
}

impl Report {
    /// Whether the run itself worked: every chain ran (verdicts are findings, not failures)
    /// and the memory checks hold, the harness's own and the target's growth across rounds.
    /// A target over its limit is a finding (verdict `mishandled`, reason `oom`).
    pub fn ok(&self) -> bool {
        self.chains
            .iter()
            .all(|c| c.error.is_none() && c.dump_error.is_none())
            && self.memory.flat
            && self.memory.within_budget
            && self.target_memory.as_ref().is_none_or(|t| t.flat)
            && self
                .rounds
                .iter()
                .all(|r| r.same_verdicts_as_first && r.chain_errors.is_empty())
    }
}

pub struct MemoryOptions {
    pub rounds: u32,
    pub slack_fraction: f64,
    pub slack_min_bytes: u64,
    pub budget_bytes: Option<u64>,
}

/// Runs the selection `memory.rounds` times in this process and builds the report from the
/// first round, with every round's peak memory.
pub fn run(sel: &Selection, opts: &RunOptions, out: &Path, mem: &MemoryOptions) -> Result<Report> {
    let mut rounds = Vec::new();
    let mut first = None;
    let mut first_digest = None;
    let mut unreported_all = std::collections::BTreeSet::new();
    for round in 1..=mem.rounds.max(1) {
        let mut o = opts.clone();
        o.logs = logs_dir(out, round, mem.rounds);
        memory::reset_peak()?;
        let started = Instant::now();
        let result = run_round(&sel.jobs, &o)?;
        let rss = memory::rss()?;
        let digest = verdict_digest(&result.entries);
        let same = first_digest.as_ref().is_none_or(|d| *d == digest);
        rounds.push(Round {
            round,
            peak_rss_bytes: rss.peak,
            rss_after_bytes: rss.current,
            seconds: started.elapsed().as_secs_f64(),
            same_verdicts_as_first: same,
            chain_errors: result
                .chains
                .iter()
                .filter_map(|c| {
                    c.error
                        .as_ref()
                        .map(|e| format!("{} ({}): {e}", c.chain, c.network))
                })
                .collect(),
            target_peak_memory_bytes: result
                .chains
                .iter()
                .filter_map(|c| {
                    c.target_peak_memory_bytes
                        .map(|p| (format!("{} ({})", c.chain, c.network), p))
                })
                .collect(),
        });
        let unreported: Vec<String> = result
            .chains
            .iter()
            .filter(|c| c.error.is_none() && c.target_peak_memory_bytes.is_none())
            .map(|c| format!("{} ({})", c.chain, c.network))
            .collect();
        unreported_all.extend(unreported);
        eprintln!(
            "harness: round {round}: peak rss {} MiB, {:.1}s",
            rss.peak / (1024 * 1024),
            started.elapsed().as_secs_f64()
        );
        if first.is_none() {
            first = Some(result);
            first_digest = Some(digest);
        }
    }
    let first = first.expect("at least one round ran");
    let memory = memory_check(&rounds, mem);
    let target_memory = target_memory_check(
        &rounds,
        mem,
        opts.memory_limit_bytes,
        unreported_all.into_iter().collect(),
    );
    let mut by_verdict = BTreeMap::new();
    let mut by_reason = BTreeMap::new();
    for e in &first.entries {
        *by_verdict
            .entry(e.result.verdict.name().to_owned())
            .or_default() += 1;
        if let Some(r) = e.result.reason {
            *by_reason.entry(r.name().to_owned()).or_default() += 1;
        }
    }
    Ok(Report {
        schema: REPORT_SCHEMA,
        shapekit_version: env!("CARGO_PKG_VERSION").into(),
        adapter_protocol: first
            .describe
            .as_ref()
            .map_or(ADAPTER_PROTOCOL_VERSION, |d| d.protocol),
        oracle_vocab_version: shapekit_oracle::vocab::ORACLE_VOCAB_VERSION,
        adapter_command: opts.adapter.clone(),
        describe: first.describe,
        manifest_sha256: sel.manifest_sha256.clone(),
        memory_limit_bytes: opts.memory_limit_bytes,
        rounds,
        memory,
        target_memory: Some(target_memory),
        totals: Totals {
            entries: first.entries.len(),
            by_verdict,
            by_reason,
            without_chain: sel.without_chain,
            without_oracle: sel.without_oracle,
        },
        chains: first.chains,
        entries: first.entries,
    })
}

/// A later round's peak may exceed the first's by the larger of a fraction of it and a
/// fixed floor: allocator noise is real, steady growth across rounds is what this catches.
pub fn memory_check(rounds: &[Round], mem: &MemoryOptions) -> MemoryCheck {
    let base = rounds.first().map_or(0, |r| r.peak_rss_bytes);
    let slack = ((base as f64 * mem.slack_fraction) as u64).max(mem.slack_min_bytes);
    let grown: Vec<String> = rounds
        .iter()
        .skip(1)
        .filter(|r| r.peak_rss_bytes > base + slack)
        .map(|r| format!("round {} peaked at {} bytes", r.round, r.peak_rss_bytes))
        .collect();
    let over: Vec<String> = mem
        .budget_bytes
        .map(|b| {
            rounds
                .iter()
                .filter(|r| r.peak_rss_bytes > b)
                .map(|r| format!("round {} peaked at {} bytes", r.round, r.peak_rss_bytes))
                .collect()
        })
        .unwrap_or_default();
    let mut detail = format!(
        "first round peak {base} bytes, allowed growth {slack} bytes over {} round(s)",
        rounds.len()
    );
    if !grown.is_empty() {
        detail.push_str(&format!("; grew: {}", grown.join(", ")));
    }
    if !over.is_empty() {
        detail.push_str(&format!("; over budget: {}", over.join(", ")));
    }
    MemoryCheck {
        slack_bytes: slack,
        budget_bytes: mem.budget_bytes,
        flat: grown.is_empty(),
        within_budget: over.is_empty(),
        detail,
    }
}

/// The target's per-chain peaks against `limit` and across rounds. Growth is judged per
/// chain, since chains differ in size far more than rounds of one chain should.
pub fn target_memory_check(
    rounds: &[Round],
    mem: &MemoryOptions,
    limit: u64,
    unreported: Vec<String>,
) -> TargetMemoryCheck {
    let peak = rounds
        .iter()
        .flat_map(|r| r.target_peak_memory_bytes.values().copied())
        .max();
    let over: Vec<String> = rounds
        .iter()
        .flat_map(|r| {
            r.target_peak_memory_bytes
                .iter()
                .filter(|(_, p)| **p > limit)
                .map(move |(c, p)| format!("{c} in round {} peaked at {p} bytes", r.round))
        })
        .collect();
    let mut grown = Vec::new();
    if let Some(first) = rounds.first() {
        for r in rounds.iter().skip(1) {
            for (c, p) in &r.target_peak_memory_bytes {
                let Some(base) = first.target_peak_memory_bytes.get(c) else {
                    continue;
                };
                let slack =
                    ((*base as f64 * mem.slack_fraction) as u64).max(TARGET_SLACK_MIN_BYTES);
                if *p > base + slack {
                    grown.push(format!(
                        "{c} peaked at {p} bytes in round {}, {base} in round 1 (slack {slack})",
                        r.round
                    ));
                }
            }
        }
    }
    let mut detail = match peak {
        Some(p) => format!("target peak {p} bytes of a {limit}-byte limit"),
        None => format!("the adapter reported no target peak memory (limit {limit} bytes)"),
    };
    if rounds.len() > 1 {
        detail.push_str(&format!(
            "; per chain a later round may exceed round 1 by {:.0}% or {} MiB, whichever is larger",
            mem.slack_fraction * 100.0,
            TARGET_SLACK_MIN_BYTES >> 20
        ));
    }
    if !over.is_empty() {
        detail.push_str(&format!("; over the limit: {}", over.join(", ")));
    }
    if !grown.is_empty() {
        detail.push_str(&format!("; grew: {}", grown.join(", ")));
    }
    if !unreported.is_empty() {
        detail.push_str(&format!("; no peak reported for {}", unreported.join(", ")));
    }
    TargetMemoryCheck {
        limit_bytes: limit,
        peak_bytes: peak,
        within_limit: over.is_empty(),
        flat: grown.is_empty(),
        unreported,
        detail,
    }
}

fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

pub fn markdown(r: &Report) -> String {
    let mut s = String::new();
    s.push_str("# Harness report\n\n");
    if let Some(d) = &r.describe {
        s.push_str(&format!(
            "Target **{}** at `{}`, adapter {} {}.\n\n",
            d.target, d.target_version, d.adapter, d.adapter_version
        ));
    }
    s.push_str(&format!(
        "Adapter protocol {}, oracle vocabulary {}, corpus manifest sha256 `{}`, target memory limit {} bytes.\n\n",
        r.adapter_protocol, r.oracle_vocab_version, r.manifest_sha256, r.memory_limit_bytes
    ));
    s.push_str("| verdict | entries |\n|---|---|\n");
    for (k, v) in &r.totals.by_verdict {
        s.push_str(&format!("| {k} | {v} |\n"));
    }
    s.push_str(&format!(
        "\n{} entries replayed. Not replayable: {} without a chain, {} without an oracle file for the network.\n\n",
        r.totals.entries, r.totals.without_chain, r.totals.without_oracle
    ));
    s.push_str("## Memory\n\n| round | harness peak rss | rss after | seconds | same verdicts |\n|---|---|---|---|---|\n");
    for x in &r.rounds {
        s.push_str(&format!(
            "| {} | {} | {} | {:.1} | {} |\n",
            x.round,
            x.peak_rss_bytes,
            x.rss_after_bytes,
            x.seconds,
            if x.same_verdicts_as_first {
                "yes"
            } else {
                "no"
            }
        ));
    }
    for x in r.rounds.iter().filter(|x| !x.chain_errors.is_empty()) {
        s.push_str(&format!(
            "\nRound {} could not run {} chain(s): {}\n",
            x.round,
            x.chain_errors.len(),
            cell(&x.chain_errors.join("; "))
        ));
    }
    s.push_str(&format!(
        "\n{} Flat: {}. Within budget: {}.\n\n",
        r.memory.detail,
        if r.memory.flat { "yes" } else { "no" },
        if r.memory.within_budget { "yes" } else { "no" }
    ));
    if let Some(t) = &r.target_memory {
        s.push_str(&format!(
            "Target: {}. Within limit: {}. Flat: {}.\n\n",
            cell(&t.detail),
            if t.within_limit { "yes" } else { "no" },
            if t.flat { "yes" } else { "no" }
        ));
        if r.rounds.len() > 1 {
            s.push_str("| round | chain | target peak memory |\n|---|---|---|\n");
            for x in &r.rounds {
                for (c, p) in &x.target_peak_memory_bytes {
                    s.push_str(&format!("| {} | {} | {p} |\n", x.round, cell(c)));
                }
            }
            s.push('\n');
        }
    }
    s.push_str("## Chains\n\n| chain | network | protocol | delivery | bootstrap | last ingested | last ledger | stop | target peak memory | dump | error |\n|---|---|---|---|---|---|---|---|---|---|---|\n");
    for c in &r.chains {
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            c.chain,
            c.network,
            c.protocol,
            c.delivery.map_or("", |d| d.name()),
            c.bootstrap.map(|b| b.to_string()).unwrap_or_default(),
            c.last_ingested.map(|b| b.to_string()).unwrap_or_default(),
            c.last_ledger,
            c.stop
                .as_ref()
                .map(|x| cell(&format!("{} at {}", x.reason.name(), x.ledger)))
                .unwrap_or_default(),
            c.target_peak_memory_bytes
                .map(|b| b.to_string())
                .unwrap_or_default(),
            match (&c.dump, &c.dump_error) {
                (_, Some(e)) => cell(&format!("failed: {e}")),
                (Some(d), None) => format!("{} tables, {} rows", d.tables, d.rows),
                (None, None) => String::new(),
            },
            cell(c.error.as_deref().unwrap_or(""))
        ));
    }
    s.push_str("\n## Entries\n\n| entry | network | ledger | verdict | reason | detail |\n|---|---|---|---|---|---|\n");
    for e in &r.entries {
        let mut detail = e.result.detail.clone();
        for p in e.result.problems.iter().take(3) {
            detail.push_str(&format!("; {}", p.detail));
        }
        if e.result.problems.len() > 3 {
            detail.push_str(&format!("; {} more", e.result.problems.len() - 3));
        }
        if !e.result.unmapped.is_empty() {
            detail.push_str(&format!("; {} unmapped row(s)", e.result.unmapped.len()));
        }
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            e.entry,
            e.network,
            e.ledger,
            e.result.verdict.name(),
            e.result.reason.map_or("", |r| r.name()),
            cell(&detail)
        ));
    }
    s
}

/// Writes the report and its Markdown form into `out`; returns their paths.
pub fn write(r: &Report, out: &Path) -> Result<(PathBuf, PathBuf)> {
    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    let json = out.join(REPORT_FILE);
    let mut text = serde_json::to_string_pretty(r).context("rendering the report")?;
    text.push('\n');
    std::fs::write(&json, text).with_context(|| format!("writing {}", json.display()))?;
    let md = out.join(MARKDOWN_FILE);
    std::fs::write(&md, markdown(r)).with_context(|| format!("writing {}", md.display()))?;
    Ok((json, md))
}

/// Counts of a verdict in a report, for callers that summarise.
pub fn count(r: &Report, v: Verdict) -> usize {
    r.entries.iter().filter(|e| e.result.verdict == v).count()
}
