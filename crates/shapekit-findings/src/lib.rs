//! Findings: the record that every failure the tooling reports ends in an upstream issue,
//! a duplicate of one, a private report, or a recorded decision not to file.
//!
//! - `findings/triage.toml` ([`triage`]): the triaged root causes, written by hand. Each
//!   finding has an id, component, severity, status, the evidence for its status, and
//!   matchers that recognise its failures.
//! - `findings/findings.json` and `findings/FINDINGS.md`: written by `shapekit findings
//!   collect` ([`collect`], [`render`]) from the triage and the run outputs it names.
//! - `findings/issues/`: issue bodies drafted for filing. Nothing there is filed until a
//!   finding's status says so.
//!
//! # Inputs
//!
//! `[inputs]` names `harness` reports (`shapekit harness run ... --network both`),
//! conformance `matrices` (`shapekit conformance run --sdk <sdk> --all-versions`), `lint`
//! results (`indexerlint -format json ./...` in a checkout of the target at `version`) and
//! the corpus `manifest` (for each entry's CAPs and size). `findings.json` records the path
//! and sha256 of each, so a reader can tell which runs the record describes.
//!
//! # Failures
//!
//! - Harness: every entry with verdict `dropped`, `mishandled` or `not-evaluated`; one
//!   failure per listed problem, or one for an entry without problems (a halt, a crash, an
//!   entry after a halt). `pass` and `out-of-scope` are not failures.
//! - Conformance: every cell whose result is not `pass` or `unsupported`, on every version.
//! - Linter: every finding that is neither suppressed nor baselined.
//!
//! # Matching
//!
//! A failure is offered to the findings in file order and taken by the first whose matcher
//! accepts it; a matcher accepts when every member it sets matches. Harness matchers
//! ([`triage::HarnessMatch`]) select by target, entry pattern (`*` is any run of
//! characters), network, verdict and reason, and for problems by problem kind, fact, token
//! event, field prefixes, null fields and detail text. A matcher that sets a problem member
//! never takes a whole entry; one that sets none takes entries without problems.
//! Conformance matchers select by sdk, entry pattern and version (all when empty); linter
//! matchers by target, rule, function and optionally package.
//!
//! # Records
//!
//! Per finding: CAPs are the union over every matched entry (plus the triage's own);
//! the minimal entry is the smallest matched entry by file size unless the triage names
//! one; the reproduction command reruns that entry (its chain and network through the
//! target's adapter at the reported version, the newest failing SDK version, or the linter
//! command). Severity is `halt` (ingestion stops), `data-loss` (facts the ledger shows are
//! not stored), `wrong-data` (stored with other values, or invented), `divergence` (an SDK
//! answers differently from the network) or `robustness` (a missing or too wide limit).
//! Status is `draft` (an issue body, not decided), `filed` or `duplicate-of` (an https
//! link), `private-report` (a reference to a report made through the upstream's security
//! channel; the public record carries only the reference) or `wont-fix` (a reason).
//! Siblings are findings in different components that fail on one corpus entry, plus those
//! the triage names; the relation is symmetric.
//!
//! # Check
//!
//! `shapekit findings check` collects afresh and fails, listing every reason, when a failure
//! matches no finding; an input reports a problem that can hide failures (a harness chain
//! that did not run, a known-failures list out of step with the matrix, a failing cell not in
//! it); a finding matches nothing; a named draft file is missing; a finding has no decision
//! (`draft`, or the status's evidence is missing); or `findings/findings.json` differs from
//! the fresh collection. The last lines give totals and the findings pending filing by id.

pub mod check;
pub mod collect;
pub mod inputs;
pub mod render;
pub mod triage;

use anyhow::{Context, Result};

// divergence matrices hold XDR-JSON nested hundreds of levels deep, and serde recurses once
// per level
const STACK_BYTES: usize = 512 << 20;

/// Runs `f` on a thread with a stack large enough for deeply nested JSON.
pub fn parse_on_big_stack<T: Send + 'static>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    std::thread::Builder::new()
        .name("findings-parse".into())
        .stack_size(STACK_BYTES)
        .spawn(f)
        .context("starting the parse thread")?
        .join()
        .map_err(|_| anyhow::anyhow!("the parse thread panicked"))?
}
