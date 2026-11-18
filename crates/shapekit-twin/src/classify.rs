//! Why the regions no chain and no core test reaches are unreached.
//!
//! The core coverage summary marks a region `unknown` when neither a corpus
//! chain nor one of core's `[tx]` tests reaches it. Most of these are not
//! holes in the corpus: they are checks that only fire when core is broken,
//! logging, code behind configuration the chains never use, or code for
//! protocols no chain runs. This module sorts each unknown line range into
//! one of five classes, first by static rules over core's source, then by
//! hand labels for what the rules do not decide. Everything left is the
//! `ordinary` class: code an ordinary transaction may reach that no chain
//! builds yet, the explorer's target list.
//!
//! Every range records how it was classified: the rule that decided it, a
//! hand label, or `residual` (no rule applied and no one labelled it).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use shapekit_contracts::CoreRef;
use shapekit_contracts::core_coverage::{CoreCoverage, RegionClass};
use shapekit_contracts::gates::{Branch, Comparison, Gate, GateList};

use crate::gates::{FunctionSpan, GateScan};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Cause {
    /// A check that only fires when core's own state is inconsistent: a
    /// throw, an assertion, an abort.
    Defensive,
    /// Logging, metrics, tracing or debug output.
    Diagnostics,
    /// Code that runs only under configuration the chains never set, or only
    /// in test builds.
    ConfigOrTest,
    /// Code behind a protocol gate whose side no chain's protocol is on.
    ProtocolOutsideChains,
    /// None of the above: reachable from ordinary transactions as far as the
    /// rules and labels can tell, and not built by any chain yet.
    Ordinary,
}

impl Cause {
    pub const ALL: [Cause; 5] = [
        Cause::Defensive,
        Cause::Diagnostics,
        Cause::ConfigOrTest,
        Cause::ProtocolOutsideChains,
        Cause::Ordinary,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Cause::Defensive => "defensive",
            Cause::Diagnostics => "diagnostics",
            Cause::ConfigOrTest => "config-or-test",
            Cause::ProtocolOutsideChains => "protocol-outside-chains",
            Cause::Ordinary => "ordinary",
        }
    }
}

/// One hand label, from `twin/unknown-labels.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Label {
    pub file: String,
    /// Only the ranges inside this function (the exact name the classifier
    /// reports); all residual ranges of the file when absent.
    #[serde(default)]
    pub function: Option<String>,
    /// Only this one range, when given.
    #[serde(default)]
    pub line_start: Option<u32>,
    #[serde(default)]
    pub line_end: Option<u32>,
    pub cause: Cause,
    pub reason: String,
}

impl Label {
    fn matches(&self, file: &str, function: Option<&str>, ls: u32, le: u32) -> bool {
        self.file == file
            && self.function.as_deref().is_none_or(|f| Some(f) == function)
            && self.line_start.is_none_or(|l| l == ls)
            && self.line_end.is_none_or(|l| l == le)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LabelFile {
    schema_version: u32,
    #[serde(default)]
    label: Vec<Label>,
}

pub fn read_labels(path: &Path) -> Result<Vec<Label>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let f: LabelFile =
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    if f.schema_version != SCHEMA_VERSION {
        bail!(
            "{} has schema_version {}, expected {SCHEMA_VERSION}",
            path.display(),
            f.schema_version
        );
    }
    Ok(f.label)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Classified {
    pub file: String,
    pub line_start: u32,
    pub line_end: u32,
    /// The function the range sits in, when it sits in one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
    pub cause: Cause,
    /// `rule:<name>`, `hand` or `residual`.
    pub decided_by: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Count {
    pub cause: Cause,
    pub by_rule: usize,
    pub by_hand: usize,
    pub residual: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnknownRegions {
    pub schema_version: u32,
    pub core: CoreRef,
    /// The protocols the measured chains run, which decide the protocol rule.
    pub chain_protocols: Vec<u32>,
    pub counts: Vec<Count>,
    /// Every `unknown` range of the core coverage summary, in its order.
    pub ranges: Vec<Classified>,
}

/// The protocol of a `<id>.p<N>` chain reference.
fn chain_protocol(chain: &str) -> Option<u32> {
    chain.rsplit_once(".p").and_then(|(_, n)| n.parse().ok())
}

fn strip_comment(l: &str) -> &str {
    match l.find("//") {
        Some(i) => &l[..i],
        None => l,
    }
}

/// Statements of the range body, comments and braces removed.
fn statements(body: &[&str]) -> Vec<String> {
    let joined: String = body
        .iter()
        .map(|l| strip_comment(l).trim())
        .filter(|l| !l.is_empty() && *l != "{" && *l != "}")
        .collect::<Vec<_>>()
        .join(" ");
    joined
        .split(';')
        .map(|s| {
            s.trim()
                .trim_matches(|c| c == '{' || c == '}')
                .trim()
                .to_owned()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

const DEFENSIVE: [&str; 9] = [
    "throw ",
    "throw;",
    "releaseAssert",
    "abort()",
    "printErrorAndAbort",
    "std::terminate",
    "dbgAbort",
    "assertThreadIsMain",
    "std::runtime_error",
];

fn diagnostic_statement(s: &str) -> bool {
    let s = s.trim_start_matches(['(', ' ']);
    s.starts_with("CLOG_")
        || s.starts_with("LOG_")
        || s.starts_with("ZoneScoped")
        || s.starts_with("ZoneNamed")
        || s.starts_with("ZoneText")
        || s.starts_with("ZoneValue")
        || s.starts_with("TracyPlot")
        || s.starts_with("FrameMark")
        || {
            let head = s.split('(').next().unwrap_or("");
            head.contains("Metrics") || head.contains("metrics") || head.ends_with(".Mark")
        }
}

fn diagnostic_function(name: &str) -> bool {
    let last = name.rsplit("::").next().unwrap_or(name);
    let lower = last.to_ascii_lowercase();
    lower == "tostring"
        || lower.ends_with("tostring")
        || lower.starts_with("tojson")
        || lower.starts_with("getjson")
        || lower.starts_with("dump")
        || lower.starts_with("print")
        || lower.starts_with("log")
        || lower.starts_with("debug")
        || lower.starts_with("describe")
        || lower.contains("metrics")
        || last.starts_with("operator<<")
}

fn test_function(name: &str) -> bool {
    name.contains("ForTesting") || name.contains("Testing") || name.contains("ForTest")
}

/// Functions of a node mode the chains never run: the postgres backend (the
/// chains run sqlite), catchup from history, and the offline utility
/// commands.
fn mode_function(name: &str) -> Option<&'static str> {
    if name.contains("Postgres") {
        Some("postgres backend, the chains run sqlite")
    } else if name.contains("Catchup") {
        Some("catchup from history, the chains never catch up")
    } else if name.ends_with("ForUtils") {
        Some("offline utility commands, not a running node")
    } else {
        None
    }
}

/// Configuration a header names: a `getConfig()` or config object field in
/// capitals, or one of core's test-only knobs.
fn names_config(text: &str) -> bool {
    text.contains("getConfig()")
        || text.contains("mConfig.")
        || text.contains("mAppConfig.")
        || text.contains("Config::")
        || text.contains("FOR_TESTING")
        || text.contains("TESTING_")
        || text.contains("ARTIFICIALLY_")
        || text.contains("isInMemoryMode")
        || text.contains("PARALLEL_LEDGER_APPLY")
        || ["cfg.", "config.", "mCfg."].iter().any(|p| {
            text.match_indices(p).any(|(i, _)| {
                text[i + p.len()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_uppercase())
            })
        })
}

/// The protocols where a gate's `branch` runs, as an inclusive range list.
fn branch_runs(g: &Gate, branch: Branch, p: u32) -> bool {
    g.protocols(branch)
        .iter()
        .any(|(lo, hi)| p >= *lo && hi.is_none_or(|h| p <= h))
}

/// Whether the range is the body of an `if` whose whole condition is one gate
/// call, possibly negated, and which side of the gate that body is.
fn gate_body<'g>(lines: &[&str], ls: u32, gates: &'g [&'g Gate]) -> Option<(&'g Gate, Branch)> {
    for g in gates {
        if g.line > ls || ls - g.line > 4 {
            continue;
        }
        let header: String = lines[(g.line - 1) as usize..(ls - 1) as usize]
            .iter()
            .map(|l| strip_comment(l).trim())
            .collect::<Vec<_>>()
            .join(" ");
        let first = lines[(ls - 1) as usize].trim();
        let mut text = header.clone();
        if first == "{" || first.starts_with('{') {
            text.push_str(" {");
        } else if ls > g.line {
            // the body starts on a line of its own after the header
        } else {
            continue;
        }
        let t = text.trim();
        let t = t.strip_prefix('}').map(str::trim).unwrap_or(t);
        let t = t.strip_prefix("else").map(str::trim).unwrap_or(t);
        let Some(rest) = t.strip_prefix("if") else {
            continue;
        };
        let rest = rest.trim().strip_prefix('(')?.trim();
        let (negated, rest) = match rest.strip_prefix('!') {
            Some(r) => (true, r.trim()),
            None => (false, rest),
        };
        if !rest.starts_with("protocolVersion") {
            continue;
        }
        // the call's own parentheses, then exactly the closing one of the if
        let open = rest.find('(')?;
        let mut depth = 0;
        let mut end = None;
        for (i, c) in rest.char_indices().skip(open) {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let after = rest[end? + 1..].trim();
        if !(after == ")" || after == ") {") {
            continue;
        }
        let branch = if negated { Branch::False } else { Branch::True };
        return Some((g, branch));
    }
    None
}

/// Classifies every `unknown` range of `coverage`.
pub fn classify(
    core_src: &Path,
    coverage: &CoreCoverage,
    gates: &GateList,
    scan: &GateScan,
    labels: &[Label],
) -> Result<UnknownRegions> {
    // the summary carries chain references only, so each must name its protocol: a chain
    // left out here would make the protocol rule call its sides unrun
    let mut protocols: BTreeSet<u32> = BTreeSet::new();
    for c in &coverage.chains {
        protocols.insert(chain_protocol(c).with_context(|| {
            format!("chain {c} of the coverage summary names no protocol (<id>.p<N>)")
        })?);
    }
    if protocols.is_empty() {
        bail!("the coverage summary names no chain with a protocol");
    }
    let mut by_file: BTreeMap<&str, Vec<&Gate>> = BTreeMap::new();
    for g in &gates.gates {
        by_file.entry(g.file.as_str()).or_default().push(g);
    }
    let mut functions: BTreeMap<&str, Vec<&FunctionSpan>> = BTreeMap::new();
    for f in &scan.functions {
        functions.entry(f.file.as_str()).or_default().push(f);
    }
    let mut label_used = vec![false; labels.len()];
    let mut sources: BTreeMap<String, String> = BTreeMap::new();
    let mut ranges = Vec::new();
    for r in coverage
        .unreached
        .iter()
        .filter(|r| r.class == RegionClass::Unknown)
    {
        if !sources.contains_key(&r.file) {
            let path = core_src.join(&r.file);
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            sources.insert(r.file.clone(), text);
        }
        let text = &sources[&r.file];
        let lines: Vec<&str> = text.split('\n').collect();
        if r.line_end as usize > lines.len() {
            bail!(
                "{}:{}-{} is past the end of the file; is the checkout at {}?",
                r.file,
                r.line_start,
                r.line_end,
                coverage.core.commit
            );
        }
        let body = &lines[(r.line_start - 1) as usize..r.line_end as usize];
        let function = functions.get(r.file.as_str()).and_then(|fs| {
            fs.iter()
                .filter(|f| f.line_start <= r.line_start && f.line_end >= r.line_end)
                .min_by_key(|f| f.line_end - f.line_start)
                .map(|f| f.function.clone())
        });
        let header_from = r.line_start.saturating_sub(3).max(1);
        let header: String = lines[(header_from - 1) as usize..(r.line_start - 1) as usize]
            .iter()
            .map(|l| strip_comment(l))
            .collect::<Vec<_>>()
            .join(" ");
        let stmts = statements(body);
        let body_text: String = body
            .iter()
            .map(|l| strip_comment(l))
            .collect::<Vec<_>>()
            .join(" ");

        let decided: Option<(Cause, &str, String)> = if scan
            .tests_only_lines
            .get(&r.file)
            .is_some_and(|ls| (r.line_start..=r.line_end).any(|l| ls.contains(&l)))
        {
            Some((
                Cause::ConfigOrTest,
                "build-tests-only",
                "compiled only with BUILD_TESTS, absent from the release build".into(),
            ))
        } else if let Some(k) = DEFENSIVE.iter().find(|k| body_text.contains(**k)) {
            Some((
                Cause::Defensive,
                "defensive-statement",
                format!("the range holds `{}`", k.trim()),
            ))
        } else if !stmts.is_empty() && stmts.iter().all(|s| diagnostic_statement(s)) {
            Some((
                Cause::Diagnostics,
                "diagnostic-statements",
                "every statement logs, traces or updates a metric".into(),
            ))
        } else if let Some(f) = function.as_deref().filter(|f| diagnostic_function(f)) {
            Some((
                Cause::Diagnostics,
                "diagnostic-function",
                format!("inside {f}"),
            ))
        } else if let Some(f) = function.as_deref().filter(|f| test_function(f)) {
            Some((Cause::ConfigOrTest, "test-function", format!("inside {f}")))
        } else if let Some((f, why)) = function
            .as_deref()
            .and_then(|f| mode_function(f).map(|w| (f, w)))
        {
            Some((
                Cause::ConfigOrTest,
                "mode-function",
                format!("inside {f}: {why}"),
            ))
        } else if names_config(&header) {
            Some((
                Cause::ConfigOrTest,
                "config-condition",
                "the condition before the range reads node configuration".into(),
            ))
        } else if let Some((g, branch)) = by_file
            .get(r.file.as_str())
            .and_then(|gs| gate_body(&lines, r.line_start, gs))
            && !protocols.iter().any(|p| branch_runs(g, branch, *p))
        {
            Some((
                Cause::ProtocolOutsideChains,
                "gate-branch",
                format!(
                    "{} side of the gate at line {} ({} V_{}), which no chain protocol is on",
                    branch.as_str(),
                    g.line,
                    match g.comparison {
                        Comparison::IsBefore => "is-before",
                        Comparison::StartsFrom => "starts-from",
                        Comparison::Equals => "equals",
                    },
                    g.version
                ),
            ))
        } else {
            None
        };
        let label = labels
            .iter()
            .position(|l| l.matches(&r.file, function.as_deref(), r.line_start, r.line_end));
        let (cause, decided_by, reason) = match (decided, label) {
            (Some((c, rule, why)), _) => (c, format!("rule:{rule}"), why),
            (None, Some(i)) => {
                label_used[i] = true;
                (labels[i].cause, "hand".to_owned(), labels[i].reason.clone())
            }
            (None, None) => (
                Cause::Ordinary,
                "residual".to_owned(),
                "no rule applies and no hand label".to_owned(),
            ),
        };
        ranges.push(Classified {
            file: r.file.clone(),
            line_start: r.line_start,
            line_end: r.line_end,
            function,
            cause,
            decided_by,
            reason,
        });
    }
    let stale: Vec<String> = labels
        .iter()
        .zip(&label_used)
        .filter(|(_, used)| !**used)
        .map(|(l, _)| {
            format!(
                "{}{}{}",
                l.file,
                l.function
                    .as_deref()
                    .map(|f| format!(" {f}"))
                    .unwrap_or_default(),
                l.line_start
                    .map(|a| format!(":{a}-{}", l.line_end.unwrap_or(a)))
                    .unwrap_or_default()
            )
        })
        .collect();
    if !stale.is_empty() {
        bail!(
            "hand labels that match no residual range: {}",
            stale.join(", ")
        );
    }
    let counts = Cause::ALL
        .iter()
        .map(|&c| Count {
            cause: c,
            by_rule: ranges
                .iter()
                .filter(|r| r.cause == c && r.decided_by.starts_with("rule:"))
                .count(),
            by_hand: ranges
                .iter()
                .filter(|r| r.cause == c && r.decided_by == "hand")
                .count(),
            residual: ranges
                .iter()
                .filter(|r| r.cause == c && r.decided_by == "residual")
                .count(),
        })
        .collect();
    Ok(UnknownRegions {
        schema_version: SCHEMA_VERSION,
        core: coverage.core.clone(),
        chain_protocols: protocols.into_iter().collect(),
        counts,
        ranges,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statement_rules() {
        assert!(diagnostic_statement("CLOG_INFO(Tx, \"x {}\", y)"));
        assert!(diagnostic_statement("mMetrics.mFoo.Mark()"));
        assert!(!diagnostic_statement("return false"));
        assert!(names_config("if (mApp.getConfig().MODE_X)"));
        assert!(names_config("if (cfg.EMIT_CLASSIC_EVENTS)"));
        assert!(!names_config("if (cfg.size() > 0)"));
        assert_eq!(chain_protocol("chain:cap26.inflation.p11"), Some(11));
        assert!(diagnostic_function("TransactionFrame::toString"));
        assert!(!diagnostic_function("TransactionFrame::apply"));
    }

    #[test]
    fn a_chain_without_a_protocol_is_refused_not_dropped() {
        let core = CoreRef {
            version: "v30.0.0".into(),
            commit: "0e49751aa9ddf892302cca02cea7f3f5d97df9f1".into(),
            image: None,
        };
        let coverage = CoreCoverage {
            schema_version: 1,
            core: core.clone(),
            chains: vec!["chain:a.p30".into(), "chain:pairs.x.a".into()],
            files: Vec::new(),
            unreached: Vec::new(),
        };
        let gates = GateList {
            schema_version: 1,
            core,
            gates: Vec::new(),
        };
        let e = classify(
            Path::new("/nonexistent"),
            &coverage,
            &gates,
            &GateScan::default(),
            &[],
        )
        .unwrap_err();
        assert!(format!("{e:#}").contains("chain:pairs.x.a"), "{e:#}");
    }

    #[test]
    fn gate_bodies_and_their_side() {
        let g = Gate {
            file: "f".into(),
            line: 2,
            column: 9,
            function: "x".into(),
            comparison: Comparison::IsBefore,
            version: 10,
            sides: vec![],
        };
        let gs = [&g];
        let src = [
            "void x() {",
            "    if (protocolVersionIsBefore(v, ProtocolVersion::V_10))",
            "    {",
            "        a();",
            "    }",
        ];
        let (_, b) = gate_body(&src, 3, &gs).unwrap();
        assert_eq!(b, Branch::True);
        assert!(branch_runs(&g, Branch::True, 9));
        assert!(!branch_runs(&g, Branch::True, 11));
        let neg = [
            "void x() {",
            "    if (!protocolVersionIsBefore(v, ProtocolVersion::V_10))",
            "    {",
        ];
        assert_eq!(gate_body(&neg, 3, &gs).unwrap().1, Branch::False);
        let compound = [
            "void x() {",
            "    if (protocolVersionIsBefore(v, ProtocolVersion::V_10) && y)",
            "    {",
        ];
        assert!(gate_body(&compound, 3, &gs).is_none());
    }
}
