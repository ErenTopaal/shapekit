//! The short markdown summary a workflow posts on a pull request. It starts with
//! [`crate::summary_marker`] so the workflow can update its own comment instead of adding one
//! per push, and stays far below GitHub's comment size limit by listing only the first rows.

use std::fmt::Write;

use crate::{Change, GroupKind, Report, RunKind};

const CHANGE_ROWS: usize = 15;
const GROUP_ROWS: usize = 15;

/// Text for a markdown table cell.
fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

pub fn render(r: &Report, html_name: Option<&str>) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "{}", crate::summary_marker(r.kind));
    let _ = writeln!(s, "### shapekit {}: {}\n", r.kind.name(), cell(&r.subject));
    match &r.comparison {
        Some(c) => {
            let _ = writeln!(
                s,
                "Compared with the {}: **{} new**, {} changed, **{} fixed**, {} unchanged{}.\n",
                c.kind,
                c.new.len(),
                c.changed.len(),
                c.fixed.len(),
                c.unchanged,
                if c.not_rerun == 0 {
                    String::new()
                } else {
                    format!(", {} not in this run", c.not_rerun)
                }
            );
            table(&mut s, "New failures", &c.new, r.kind);
            table(&mut s, "Changed failures", &c.changed, r.kind);
            table(&mut s, "Fixed", &c.fixed, r.kind);
        }
        None => {
            let _ = writeln!(
                s,
                "No baseline was given, so this summary cannot tell new failures from old ones.\n"
            );
        }
    }
    let _ = writeln!(
        s,
        "{} (corpus {}).\n",
        crate::grouping_line(r),
        cell(&r.corpus)
    );
    if !r.groups.is_empty() {
        let _ = writeln!(
            s,
            "| group | failures | finding | smallest entry |\n|---|---|---|---|"
        );
        for g in r.groups.iter().take(GROUP_ROWS) {
            let finding = match (&g.page, &g.link) {
                (Some(p), Some(l)) => format!("[record]({p}), [upstream]({l})"),
                (Some(p), None) => format!("[record]({p})"),
                (None, Some(l)) => format!("[upstream]({l})"),
                (None, None) => match g.kind {
                    GroupKind::Untriaged => "none yet".into(),
                    _ => "not filed".into(),
                },
            };
            let _ = writeln!(
                s,
                "| **{}** {} | {} | {finding} | `{}` |",
                cell(&g.id),
                cell(&g.title),
                g.occurrences.len(),
                cell(g.minimal_entry.as_deref().unwrap_or(""))
            );
        }
        if r.groups.len() > GROUP_ROWS {
            let _ = writeln!(
                s,
                "\n{} more groups in the full report.",
                r.groups.len() - GROUP_ROWS
            );
        }
        s.push('\n');
    }
    if !r.input_problems.is_empty() {
        let _ = writeln!(
            s,
            "The input reports {} problem(s); the full report lists them.\n",
            r.input_problems.len()
        );
    }
    if let Some(h) = html_name {
        let _ = writeln!(
            s,
            "The full report, with the guide sections and a reproduce command per group, is `{h}` in this run's artifact."
        );
    }
    s
}

fn table(s: &mut String, title: &str, list: &[Change], kind: RunKind) {
    if list.is_empty() {
        return;
    }
    let _ = writeln!(s, "<details><summary>{title} ({})</summary>\n", list.len());
    match kind {
        RunKind::Harness => {
            let _ = writeln!(
                s,
                "| entry | network | before | now | group |\n|---|---|---|---|---|"
            );
        }
        RunKind::Conformance => {
            let _ = writeln!(
                s,
                "| entry | testee | check | before | now | group |\n|---|---|---|---|---|---|"
            );
        }
    }
    for c in list.iter().take(CHANGE_ROWS) {
        let check = match kind {
            RunKind::Harness => String::new(),
            RunKind::Conformance => format!(" `{}` |", cell(c.check.as_deref().unwrap_or(""))),
        };
        let _ = writeln!(
            s,
            "| `{}` | {} |{check} {} | {} | {} |",
            cell(&c.entry),
            cell(&c.at),
            cell(c.before.as_deref().unwrap_or("not failing")),
            cell(c.now.as_deref().unwrap_or("")),
            cell(c.group.as_deref().unwrap_or(""))
        );
    }
    if list.len() > CHANGE_ROWS {
        let _ = writeln!(s, "\n{} more in the full report.", list.len() - CHANGE_ROWS);
    }
    let _ = writeln!(s, "\n</details>\n");
}
