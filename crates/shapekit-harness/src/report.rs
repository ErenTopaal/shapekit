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
use crate::relations::carry::ProblemKind;
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
    /// The manifest's replay digest: what the run depends on, leaving out the entries'
    /// descriptive fields (`shapekit_manifest::replay::DESCRIPTIVE_FIELDS`). Absent in reports
    /// written before it was recorded, and for chains named with --chain-dir.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_replay_digest: Option<String>,
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
    /// The `--shard` the run replayed, `<index>/<count>`; absent for a whole run and for a
    /// report `merge` put together from every shard.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shard: Option<String>,
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
        manifest_replay_digest: sel.manifest_replay_digest.clone(),
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
        shard: None,
    })
}

/// One report from the reports of every shard of one run (`harness run --shard i/n`, each
/// `i` once): the chains and entries of all of them, verdict totals added up, and the memory
/// checks holding only where they hold in every shard. A round's seconds are the shards'
/// added up, its peaks the largest shard's. The shards must come from one corpus, one
/// adapter command, one target build and one round count.
pub fn merge(mut shards: Vec<Report>) -> Result<Report> {
    let first = shards.first().context("no report to merge")?.clone();
    let mut seen = std::collections::BTreeSet::new();
    let mut count = None;
    for r in &shards {
        let sh = r
            .shard
            .as_deref()
            .with_context(|| format!("a report of {} chains is not a shard", r.chains.len()))?;
        let (i, n) = sh
            .split_once('/')
            .and_then(|(i, n)| Some((i.parse::<u32>().ok()?, n.parse::<u32>().ok()?)))
            .with_context(|| format!("shard {sh:?} is not <index>/<count>"))?;
        if *count.get_or_insert(n) != n {
            anyhow::bail!("shard {sh} is not one of {} shards", count.unwrap_or(n));
        }
        if !seen.insert(i) {
            anyhow::bail!("shard {sh} is given twice");
        }
        let same = r.schema == first.schema
            && r.shapekit_version == first.shapekit_version
            && r.adapter_command == first.adapter_command
            && r.describe == first.describe
            && r.manifest_sha256 == first.manifest_sha256
            && r.manifest_replay_digest == first.manifest_replay_digest
            && r.memory_limit_bytes == first.memory_limit_bytes
            && r.rounds.len() == first.rounds.len()
            && r.totals.without_chain == first.totals.without_chain
            && r.totals.without_oracle == first.totals.without_oracle;
        if !same {
            anyhow::bail!(
                "shard {sh} is not of the same run as shard {}: the shapekit version, adapter, target, corpus, memory limit or round count differ",
                first.shard.as_deref().unwrap_or("?")
            );
        }
    }
    let n = count.unwrap_or(0);
    let missing: Vec<u32> = (1..=n).filter(|i| !seen.contains(i)).collect();
    if !missing.is_empty() {
        anyhow::bail!("shards {missing:?} of {n} are missing");
    }
    shards.sort_by_key(|r| r.shard.clone());

    let mut rounds = first.rounds.clone();
    for (k, round) in rounds.iter_mut().enumerate() {
        let all: Vec<&Round> = shards.iter().map(|r| &r.rounds[k]).collect();
        round.peak_rss_bytes = all.iter().map(|x| x.peak_rss_bytes).max().unwrap_or(0);
        round.rss_after_bytes = all.iter().map(|x| x.rss_after_bytes).max().unwrap_or(0);
        round.seconds = all.iter().map(|x| x.seconds).sum();
        round.same_verdicts_as_first = all.iter().all(|x| x.same_verdicts_as_first);
        round.chain_errors = all.iter().flat_map(|x| x.chain_errors.clone()).collect();
        round.target_peak_memory_bytes = all
            .iter()
            .flat_map(|x| x.target_peak_memory_bytes.clone())
            .collect();
    }
    let detail = |f: &dyn Fn(&Report) -> String| {
        shards
            .iter()
            .map(|r| format!("shard {}: {}", r.shard.as_deref().unwrap_or("?"), f(r)))
            .collect::<Vec<_>>()
            .join("; ")
    };
    let memory = MemoryCheck {
        slack_bytes: shards
            .iter()
            .map(|r| r.memory.slack_bytes)
            .max()
            .unwrap_or(0),
        budget_bytes: first.memory.budget_bytes,
        flat: shards.iter().all(|r| r.memory.flat),
        within_budget: shards.iter().all(|r| r.memory.within_budget),
        detail: detail(&|r| r.memory.detail.clone()),
    };
    let target_memory = if shards.iter().all(|r| r.target_memory.is_some()) {
        let t: Vec<&TargetMemoryCheck> = shards
            .iter()
            .filter_map(|r| r.target_memory.as_ref())
            .collect();
        Some(TargetMemoryCheck {
            limit_bytes: first.memory_limit_bytes,
            peak_bytes: t.iter().filter_map(|x| x.peak_bytes).max(),
            within_limit: t.iter().all(|x| x.within_limit),
            flat: t.iter().all(|x| x.flat),
            unreported: t.iter().flat_map(|x| x.unreported.clone()).collect(),
            detail: detail(&|r| {
                r.target_memory
                    .as_ref()
                    .map_or(String::new(), |t| t.detail.clone())
            }),
        })
    } else {
        None
    };
    let mut chains: Vec<ChainResult> = shards.iter().flat_map(|r| r.chains.clone()).collect();
    chains.sort_by(|a, b| (&a.chain, &a.network).cmp(&(&b.chain, &b.network)));
    for w in chains.windows(2) {
        if (&w[0].chain, &w[0].network) == (&w[1].chain, &w[1].network) {
            anyhow::bail!("chain {} ({}) is in two shards", w[0].chain, w[0].network);
        }
    }
    // the entries in the order a whole run writes them: by chain, network, ledger, then id
    let mut entries: Vec<EntryRow> = shards.iter().flat_map(|r| r.entries.clone()).collect();
    entries.sort_by(|a, b| {
        (&a.chain, &a.network, a.ledger, &a.entry).cmp(&(&b.chain, &b.network, b.ledger, &b.entry))
    });
    let mut totals = first.totals.clone();
    totals.entries = entries.len();
    totals.by_verdict.clear();
    totals.by_reason.clear();
    for r in &shards {
        for (k, v) in &r.totals.by_verdict {
            *totals.by_verdict.entry(k.clone()).or_default() += v;
        }
        for (k, v) in &r.totals.by_reason {
            *totals.by_reason.entry(k.clone()).or_default() += v;
        }
    }
    Ok(Report {
        rounds,
        memory,
        target_memory,
        totals,
        chains,
        entries,
        shard: None,
        ..first
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
        "Adapter protocol {}, oracle vocabulary {}, corpus manifest sha256 `{}`{}, target memory limit {} bytes.\n\n",
        r.adapter_protocol,
        r.oracle_vocab_version,
        r.manifest_sha256,
        r.manifest_replay_digest
            .as_ref()
            .map(|d| format!(" (replay digest `{d}`)"))
            .unwrap_or_default(),
        r.memory_limit_bytes
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
    if r.chains.iter().any(|c| c.carry.is_some()) {
        s.push_str("\n## Carry check\n\nEvery cell of the target's dump looked up among the identifiers and amounts the chain carried. Truncated and fabricated values are errors whatever the column means.\n\n| chain | network | known addresses | found | cells | truncated | fabricated | columns |\n|---|---|---|---|---|---|---|---|\n");
        for c in &r.chains {
            let Some(x) = &c.carry else { continue };
            let mut cols = x.columns_with(ProblemKind::Truncated);
            cols.extend(x.columns_with(ProblemKind::Fabricated));
            cols.sort();
            cols.dedup();
            s.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
                c.chain,
                c.network,
                x.known_addresses,
                x.addresses_found,
                x.cells,
                x.truncated,
                x.fabricated,
                cell(&cols.join(", "))
            ));
        }
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
