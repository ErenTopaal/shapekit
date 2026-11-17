//! The report as one HTML file: no scripts, no external styles, fonts or images, so it opens
//! from disk, from a CI artifact or as an attachment the same way.

use std::fmt::Write;

use crate::{Change, Comparison, Group, GroupKind, Report, RunKind};

/// Occurrences shown per group; the rest are counted.
const OCCURRENCE_ROWS: usize = 200;
/// Changes shown per class; the rest are counted.
const CHANGE_ROWS: usize = 300;

pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// A link only for http and https addresses, so nothing in a run can inject a script url.
fn link(url: &str, text: &str) -> String {
    if url.starts_with("https://") || url.starts_with("http://") {
        format!("<a href=\"{}\">{}</a>", esc(url), esc(text))
    } else {
        esc(text)
    }
}

/// Inline code spans for text that marks them with backticks.
fn ticks(s: &str) -> String {
    let mut out = String::new();
    for (i, part) in s.split('`').enumerate() {
        if i % 2 == 1 {
            let _ = write!(out, "<code>{}</code>", esc(part));
        } else {
            out.push_str(&esc(part));
        }
    }
    out
}

const STYLE: &str = r#"
:root{color-scheme:light dark;--bg:#fff;--fg:#1c1f24;--muted:#5b6270;--line:#d9dde3;--card:#f6f7f9;--accent:#1f5fbf;--bad:#b3261e;--warn:#8a5a00;--good:#1b6e3a}
@media (prefers-color-scheme:dark){:root{--bg:#121417;--fg:#e6e8eb;--muted:#9aa2ae;--line:#2c3138;--card:#1a1d21;--accent:#7fb0ff;--bad:#ff8a80;--warn:#e3b341;--good:#6fcf97}}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.5 system-ui,-apple-system,"Segoe UI",sans-serif}
main{max-width:1100px;margin:0 auto;padding:24px 20px 64px}
h1{font-size:1.5rem;margin:0 0 4px}h2{font-size:1.2rem;margin:32px 0 8px}h3{font-size:1.05rem;margin:0}
a{color:var(--accent)}code,pre{font:13px/1.45 ui-monospace,SFMono-Regular,Menlo,monospace}
code{background:var(--card);padding:0 3px;border-radius:3px}
pre{background:var(--card);border:1px solid var(--line);border-radius:6px;padding:8px 10px;overflow-x:auto;white-space:pre-wrap;overflow-wrap:anywhere}
.muted{color:var(--muted)}
.tiles{display:flex;flex-wrap:wrap;gap:10px;margin:16px 0}
.tile{border:1px solid var(--line);border-radius:8px;padding:8px 14px;min-width:120px}
.tile b{display:block;font-size:1.4rem}
.group{border:1px solid var(--line);border-radius:8px;padding:14px 16px;margin:12px 0}
.group header{display:flex;flex-wrap:wrap;align-items:baseline;gap:8px}
.badge{display:inline-block;border:1px solid var(--line);border-radius:99px;padding:0 8px;font-size:12px;color:var(--muted)}
.badge.halt,.badge.data-loss,.badge.new{color:var(--bad);border-color:var(--bad)}
.badge.wrong-data,.badge.robustness,.badge.changed{color:var(--warn);border-color:var(--warn)}
.badge.fixed{color:var(--good);border-color:var(--good)}
dl{display:grid;grid-template-columns:max-content 1fr;gap:4px 14px;margin:10px 0}dt{color:var(--muted)}dd{margin:0}
table{border-collapse:collapse;width:100%;font-size:13px;margin:6px 0}
th,td{border-bottom:1px solid var(--line);padding:4px 6px;text-align:left;vertical-align:top}
th{color:var(--muted);font-weight:600}
details{margin:6px 0}summary{cursor:pointer;color:var(--muted)}
nav ul{padding-left:18px}
"#;

pub fn render(r: &Report) -> String {
    let mut s = String::new();
    let title = format!("shapekit {} report: {}", r.kind.name(), r.subject);
    let _ = write!(
        s,
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'\">\
         <title>{}</title><style>{STYLE}</style></head><body><main>\n",
        esc(&title)
    );
    let _ = writeln!(s, "<h1>{}</h1>", esc(&title));
    let _ = writeln!(
        s,
        "<p class=\"muted\">Corpus {} · input <code>{}</code> (sha256 <code>{}</code>)</p>",
        esc(&r.corpus),
        esc(&r.input.path),
        esc(&r.input.sha256)
    );

    let count = |k: GroupKind| r.groups.iter().filter(|g| g.kind == k).count();
    let _ = writeln!(s, "<div class=\"tiles\">");
    tile(&mut s, r.failures, "failures");
    tile(&mut s, count(GroupKind::Finding), "known findings");
    tile(&mut s, count(GroupKind::Candidate), "candidate findings");
    tile(&mut s, count(GroupKind::Untriaged), "untriaged groups");
    if let Some(c) = &r.comparison {
        tile(&mut s, c.new.len(), "new");
        tile(&mut s, c.changed.len(), "changed");
        tile(&mut s, c.fixed.len(), "fixed");
        tile(&mut s, c.unchanged, "unchanged");
    }
    let _ = writeln!(s, "</div>");
    let _ = writeln!(
        s,
        "<p>Each group below is one root cause. A known finding is triaged in the shapekit \
         findings record and links its upstream issue or decision; a candidate finding is \
         triaged but not yet in the record; an untriaged group holds failures no finding \
         matches, grouped by corpus family and by what failed. Every group names its \
         smallest corpus entry and the command that reruns it.</p>"
    );

    if let Some(c) = &r.comparison {
        comparison(&mut s, c, r.kind, &r.site);
    }

    if r.groups.is_empty() {
        let _ = writeln!(s, "<h2>Failures</h2><p>The run has no failures.</p>");
    } else {
        let _ = writeln!(s, "<h2>Failures by root cause</h2><nav><ul>");
        for g in &r.groups {
            let _ = writeln!(
                s,
                "<li><a href=\"#{}\">{}</a> {} <span class=\"muted\">({} failure{})</span></li>",
                esc(&g.id),
                esc(&g.id),
                ticks(&g.title),
                g.occurrences.len(),
                if g.occurrences.len() == 1 { "" } else { "s" }
            );
        }
        let _ = writeln!(s, "</ul></nav>");
        for g in &r.groups {
            group(&mut s, g, &r.site);
        }
    }

    let _ = writeln!(s, "<h2>Totals</h2>");
    for (name, counts) in &r.totals {
        let _ = write!(s, "<table><tr><th>{}</th><th>count</th></tr>", esc(name));
        for (k, n) in counts {
            let _ = write!(s, "<tr><td>{}</td><td>{n}</td></tr>", esc(k));
        }
        let _ = writeln!(s, "</table>");
    }
    if !r.input_problems.is_empty() {
        let _ = writeln!(
            s,
            "<h2>Problems the input reports</h2><details><summary>{} problem(s): runs that did \
             not complete, failures outside the known-failures list, or an input from another \
             corpus</summary><ul>",
            r.input_problems.len()
        );
        for p in &r.input_problems {
            let _ = writeln!(s, "<li>{}</li>", ticks(p));
        }
        let _ = writeln!(s, "</ul></details>");
    }
    let _ = writeln!(
        s,
        "<p class=\"muted\">Written by <code>shapekit report</code>. Findings: {}. Guide: {}.</p>",
        link(&format!("{}/dashboard/findings", r.site), "findings list"),
        link(
            &format!("{}{}", r.site, crate::GUIDE_PATH),
            "indexer hardening guide"
        )
    );
    s.push_str("</main></body></html>\n");
    s
}

/// A corpus entry, linked to its page in the explorer.
fn entry_link(site: &str, entry: &str) -> String {
    format!(
        "<a href=\"{}\"><code>{}</code></a>",
        esc(&crate::entry_page(site, entry)),
        esc(entry)
    )
}

fn tile(s: &mut String, n: usize, label: &str) {
    let _ = write!(s, "<div class=\"tile\"><b>{n}</b>{}</div>", esc(label));
}

fn comparison(s: &mut String, c: &Comparison, kind: RunKind, site: &str) {
    let _ = writeln!(
        s,
        "<h2>Compared with the {}</h2><p class=\"muted\"><code>{}</code>: {} new, {} changed, \
         {} fixed, {} unchanged{}.</p>",
        esc(&c.kind),
        esc(&c.path),
        c.new.len(),
        c.changed.len(),
        c.fixed.len(),
        c.unchanged,
        if c.not_rerun == 0 {
            String::new()
        } else {
            format!(
                "; {} baseline failure(s) are not in this run, so nothing is known about them",
                c.not_rerun
            )
        }
    );
    for (name, list) in [
        ("new", &c.new),
        ("changed", &c.changed),
        ("fixed", &c.fixed),
    ] {
        if list.is_empty() {
            continue;
        }
        let _ = writeln!(
            s,
            "<details{}><summary><span class=\"badge {name}\">{name}</span> {}</summary>",
            if name == "fixed" { "" } else { " open" },
            list.len()
        );
        changes(s, list, kind, site);
        let _ = writeln!(s, "</details>");
    }
}

fn changes(s: &mut String, list: &[Change], kind: RunKind, site: &str) {
    let at = match kind {
        RunKind::Harness => "network",
        RunKind::Conformance => "testee",
    };
    let _ = write!(s, "<table><tr><th>entry</th><th>{at}</th>");
    if kind == RunKind::Conformance {
        let _ = write!(s, "<th>check</th>");
    }
    let _ = write!(s, "<th>before</th><th>now</th><th>group</th></tr>");
    for c in list.iter().take(CHANGE_ROWS) {
        let _ = write!(
            s,
            "<tr><td>{}</td><td>{}</td>",
            entry_link(site, &c.entry),
            esc(&c.at)
        );
        if kind == RunKind::Conformance {
            let _ = write!(
                s,
                "<td><code>{}</code></td>",
                esc(c.check.as_deref().unwrap_or(""))
            );
        }
        let group = c
            .group
            .as_ref()
            .map(|g| format!("<a href=\"#{}\">{}</a>", esc(g), esc(g)))
            .unwrap_or_default();
        let _ = writeln!(
            s,
            "<td>{}</td><td>{}</td><td>{group}</td></tr>",
            esc(c.before.as_deref().unwrap_or("not failing")),
            esc(c.now.as_deref().unwrap_or("")),
        );
    }
    let _ = writeln!(s, "</table>");
    if list.len() > CHANGE_ROWS {
        let _ = writeln!(
            s,
            "<p class=\"muted\">{} more not shown.</p>",
            list.len() - CHANGE_ROWS
        );
    }
}

fn group(s: &mut String, g: &Group, site: &str) {
    let kind = match g.kind {
        GroupKind::Finding => "finding",
        GroupKind::Candidate => "candidate finding",
        GroupKind::Untriaged => "untriaged",
    };
    let _ = writeln!(
        s,
        "<section class=\"group\" id=\"{}\"><header><h3>{}</h3><span>{}</span>\
         <span class=\"badge\">{kind}</span>",
        esc(&g.id),
        esc(&g.id),
        ticks(&g.title)
    );
    for b in [&g.severity, &g.status].into_iter().flatten() {
        let _ = write!(s, "<span class=\"badge {}\">{}</span>", esc(b), esc(b));
    }
    let _ = writeln!(s, "</header>");
    if let Some(c) = &g.cause {
        let _ = writeln!(s, "<p>{}</p>", ticks(c));
    } else if g.kind == GroupKind::Untriaged {
        let _ = writeln!(
            s,
            "<p class=\"muted\">No finding matches these failures. They are grouped by corpus \
             family and by what failed, which does not prove one root cause.</p>"
        );
    }
    let _ = writeln!(s, "<dl>");
    let mut links = Vec::new();
    if let Some(p) = &g.page {
        links.push(link(p, "finding on the site"));
    }
    if let Some(l) = &g.link {
        links.push(link(l, l.trim_start_matches("https://github.com/")));
    }
    if let Some(d) = &g.decision {
        links.push(esc(d));
    }
    if !links.is_empty() {
        let _ = writeln!(s, "<dt>finding</dt><dd>{}</dd>", links.join(" · "));
    }
    if let Some(m) = &g.minimal_entry {
        let _ = writeln!(s, "<dt>smallest entry</dt><dd>{}</dd>", entry_link(site, m));
    }
    if let Some(r) = &g.reproduce {
        let _ = writeln!(
            s,
            "<dt>reproduce</dt><dd><pre><code>{}</code></pre></dd>",
            esc(r)
        );
    }
    if let Some(r) = &g.recipe {
        let _ = writeln!(s, "<dt>recorded by</dt><dd><code>{}</code></dd>", esc(r));
    }
    if !g.caps.is_empty() {
        let _ = writeln!(s, "<dt>CAPs</dt><dd>{}</dd>", esc(&g.caps.join(", ")));
    }
    let _ = writeln!(
        s,
        "<dt>entries</dt><dd>{} corpus entr{}</dd>",
        g.entries.len(),
        if g.entries.len() == 1 { "y" } else { "ies" }
    );
    let _ = writeln!(s, "<dt>guide</dt><dd>");
    if g.guide.is_empty() {
        let _ = write!(
            s,
            "<span class=\"muted\">The entries name no field of the guide's field table, so \
             no guide section is linked.</span>"
        );
    } else {
        let _ = write!(s, "<ul>");
        for r in &g.guide {
            let _ = write!(
                s,
                "<li>{}: {}</li>",
                link(&r.url, &r.section),
                ticks(&r.title)
            );
        }
        let _ = write!(s, "</ul>");
    }
    let _ = writeln!(s, "</dd></dl>");
    if !g.fields.is_empty() {
        let _ = writeln!(
            s,
            "<details><summary>{} field(s) from the guide's field table</summary><table>\
             <tr><th>field</th><th>validated by core</th><th>risk</th><th>handling</th></tr>",
            g.fields.len()
        );
        for f in &g.fields {
            let _ = writeln!(
                s,
                "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(&f.id),
                esc(&f.validated_by_core),
                ticks(&f.risk),
                ticks(&f.handling)
            );
        }
        let _ = writeln!(s, "</table></details>");
    }
    let _ = writeln!(
        s,
        "<details><summary>{} failure(s)</summary><table><tr><th>entry</th><th>on</th>\
         <th>at</th><th>what</th></tr>",
        g.occurrences.len()
    );
    for o in g.occurrences.iter().take(OCCURRENCE_ROWS) {
        let _ = writeln!(
            s,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            o.entry
                .as_deref()
                .map(|e| entry_link(site, e))
                .unwrap_or_default(),
            esc(&o.on),
            esc(&o.at),
            esc(&o.what)
        );
    }
    let _ = writeln!(s, "</table>");
    if g.occurrences.len() > OCCURRENCE_ROWS {
        let _ = writeln!(
            s,
            "<p class=\"muted\">{} more not shown.</p>",
            g.occurrences.len() - OCCURRENCE_ROWS
        );
    }
    let _ = writeln!(s, "</details></section>");
}
