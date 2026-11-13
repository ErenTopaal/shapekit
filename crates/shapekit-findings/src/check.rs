//! `shapekit findings check`: fails while any failure lacks a finding or any finding lacks
//! an upstream link or a recorded decision.

use std::path::Path;

use crate::collect::{Collection, FindingRecord};
use crate::triage::Status;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outcome {
    pub ok: bool,
    /// Every reason the check fails, one per line.
    pub problems: Vec<String>,
    /// Findings waiting to be filed (status `draft`).
    pub pending: Vec<String>,
    /// The members of `problems` that only say a finding is still a draft.
    pub drafts: Vec<String>,
}

impl Outcome {
    /// Whether every problem is a draft waiting to be filed, so a caller that allows drafts
    /// can pass while anything else still fails.
    pub fn only_drafts(&self) -> bool {
        self.problems.iter().all(|p| self.drafts.contains(p))
    }
}

fn https(link: &Option<String>) -> bool {
    link.as_deref().is_some_and(|l| l.starts_with("https://"))
}

/// What is missing for `f` to count as mapped to an upstream decision, if anything.
pub fn missing_decision(f: &FindingRecord) -> Option<String> {
    match f.status {
        Status::Draft => Some(format!(
            "{} is a draft ({}): not filed, no link",
            f.id,
            f.draft.as_deref().unwrap_or("no draft file")
        )),
        Status::Filed | Status::DuplicateOf if !https(&f.link) => Some(format!(
            "{} is {} without an https link",
            f.id,
            f.status.name()
        )),
        Status::PrivateReport if f.reference.as_deref().is_none_or(str::is_empty) => {
            Some(format!("{} is a private report without a reference", f.id))
        }
        Status::WontFix if f.reason.as_deref().is_none_or(str::is_empty) => {
            Some(format!("{} is wont-fix without a reason", f.id))
        }
        _ => None,
    }
}

/// `fresh` with the hash of every input the committed record also lists replaced by the
/// committed hash. A run's reduced output still holds its timings and memory peaks, so its
/// hash changes with every run even when every verdict is the same; comparing with this
/// judges the findings a run gives, not its measurements.
pub fn with_committed_hashes(fresh: &Collection, written: Option<&Collection>) -> Collection {
    let mut out = fresh.clone();
    if let Some(w) = written {
        for i in &mut out.inputs {
            if let Some(c) = w
                .inputs
                .iter()
                .find(|c| c.path == i.path && c.kind == i.kind)
            {
                i.sha256.clone_from(&c.sha256);
            }
        }
    }
    out
}

/// Checks a fresh collection. `written` is the committed `findings.json` and `written_md`
/// the committed `FINDINGS.md`; both must equal what collect would write now, so the
/// published record cannot lag the runs it claims to describe.
pub fn check(
    root: &Path,
    fresh: &Collection,
    written: Option<&Collection>,
    written_md: Option<&str>,
) -> Outcome {
    let mut out = Outcome::default();
    for o in &fresh.unmapped {
        out.problems.push(format!(
            "unmapped {} failure on {}{} ({}): {}",
            o.source,
            o.on,
            o.entry
                .as_ref()
                .map(|e| format!(", entry {e}"))
                .unwrap_or_default(),
            o.at,
            o.what
        ));
    }
    out.problems.extend(fresh.input_problems.iter().cloned());
    for f in &fresh.findings {
        if f.occurrences.is_empty() {
            out.problems.push(format!(
                "{} matches nothing in the inputs: fixed upstream or a stale matcher; update the triage",
                f.id
            ));
        }
        if let Some(d) = &f.draft
            && !root.join(d).is_file()
        {
            out.problems
                .push(format!("{} names draft {d}, which does not exist", f.id));
        }
        if let Some(m) = missing_decision(f) {
            if f.status == Status::Draft {
                out.pending.push(f.id.clone());
                out.drafts.push(m.clone());
            }
            out.problems.push(m);
        }
    }
    match written {
        None => out
            .problems
            .push("findings/findings.json is missing; run `shapekit findings collect`".into()),
        Some(w) if w != fresh => out.problems.push(
            "findings/findings.json differs from a fresh collection of the inputs; run `shapekit findings collect`".into(),
        ),
        Some(_) => {}
    }
    match written_md {
        None => out
            .problems
            .push("findings/FINDINGS.md is missing; run `shapekit findings collect`".into()),
        Some(md) if md != crate::render::markdown(fresh) => out.problems.push(
            "findings/FINDINGS.md differs from a fresh rendering of the inputs; run `shapekit findings collect`".into(),
        ),
        Some(_) => {}
    }
    out.ok = out.problems.is_empty();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collect::{Collection, Occurrence, Summary};
    use crate::triage::Severity;

    fn finding(id: &str, status: Status, link: Option<&str>) -> FindingRecord {
        FindingRecord {
            id: id.into(),
            title: "t".into(),
            component: "c".into(),
            severity: Severity::DataLoss,
            caps: vec![],
            minimal_entry: None,
            reproduce: None,
            status,
            link: link.map(str::to_owned),
            reference: None,
            reason: None,
            draft: None,
            cause: "c".into(),
            siblings: vec![],
            related: vec![],
            entries: vec!["e".into()],
            occurrences: vec![Occurrence {
                source: "harness".into(),
                on: "t@1".into(),
                entry: Some("e".into()),
                at: "testnet".into(),
                what: "w".into(),
            }],
        }
    }

    fn md(c: &Collection) -> String {
        crate::render::markdown(c)
    }

    fn collection(findings: Vec<FindingRecord>) -> Collection {
        Collection {
            schema: 1,
            inputs: vec![],
            summary: Summary::default(),
            findings,
            unmapped: vec![],
            input_problems: vec![],
        }
    }

    #[test]
    fn committed_hashes_hide_only_the_input_hashes() {
        use crate::collect::InputRef;
        let input = |sha: &str, detail: &str| InputRef {
            kind: "harness".into(),
            path: "findings/inputs/h.json".into(),
            sha256: sha.into(),
            detail: detail.into(),
        };
        let mut written = collection(vec![finding("A", Status::Filed, Some("https://x"))]);
        written.inputs = vec![input("aaaa", "898 entries")];
        let mut fresh = written.clone();
        fresh.inputs = vec![input("bbbb", "898 entries")];
        assert_ne!(fresh, written);
        assert_eq!(with_committed_hashes(&fresh, Some(&written)), written);
        // anything else that differs still differs
        fresh.inputs = vec![input("bbbb", "899 entries")];
        assert_ne!(with_committed_hashes(&fresh, Some(&written)), written);
        let mut other = written.clone();
        other.findings[0].id = "B".into();
        assert_ne!(with_committed_hashes(&other, Some(&written)), written);
        // nothing committed: unchanged
        assert_eq!(with_committed_hashes(&fresh, None), fresh);
    }

    #[test]
    fn drafts_fail_and_are_counted_as_pending() {
        let c = collection(vec![
            finding("A", Status::Draft, None),
            finding(
                "B",
                Status::DuplicateOf,
                Some("https://github.com/o/r/issues/1"),
            ),
        ]);
        let out = check(Path::new("."), &c, Some(&c), Some(&md(&c)));
        assert!(!out.ok);
        assert_eq!(out.pending, ["A"]);
        assert_eq!(out.problems.len(), 1, "{:?}", out.problems);
        assert!(out.only_drafts());
        let mut stale = c.clone();
        stale.summary.failures += 1;
        let out = check(Path::new("."), &stale, Some(&c), Some(&md(&stale)));
        assert!(
            !out.only_drafts(),
            "a stale record is not a draft: {:?}",
            out.problems
        );
    }

    #[test]
    fn decisions_need_their_evidence() {
        let mut private = finding("P", Status::PrivateReport, None);
        let wont = finding("W", Status::WontFix, None);
        let filed = finding("F", Status::Filed, Some("http://example.org/1"));
        let c = collection(vec![private.clone(), wont, filed]);
        assert_eq!(
            check(Path::new("."), &c, Some(&c), Some(&md(&c)))
                .problems
                .len(),
            3
        );
        private.reference = Some("GHSA-xxxx".into());
        let c = collection(vec![private]);
        assert!(check(Path::new("."), &c, Some(&c), Some(&md(&c))).ok);
    }

    #[test]
    fn unmapped_failures_and_a_stale_record_fail() {
        let mut c = collection(vec![finding("B", Status::DuplicateOf, Some("https://x/1"))]);
        assert!(check(Path::new("."), &c, Some(&c), Some(&md(&c))).ok);
        let written = c.clone();
        c.unmapped.push(Occurrence {
            source: "conformance".into(),
            on: "go@v0.8.0".into(),
            entry: Some("e2".into()),
            at: "go@v0.8.0".into(),
            what: "diverges-from-network".into(),
        });
        let out = check(Path::new("."), &c, Some(&written), Some(&md(&c)));
        assert_eq!(out.problems.len(), 2, "{:?}", out.problems);
        assert!(!check(Path::new("."), &written, None, Some(&md(&written))).ok);
        assert!(!check(Path::new("."), &written, Some(&written), None).ok);
        assert!(!check(Path::new("."), &written, Some(&written), Some("# stale\n")).ok);
    }
}
