//! The four generated regions: field table, corpus entries by field, caps table, checklist.

use std::collections::BTreeMap;

use shapekit_manifest::Entry;
use shapekit_registry::{Citation, Derivation, Field, Limit, ValidatedByCore};

use crate::{CORPUS_ANCHOR, CorpusLinks, GuideError, Sources, mdx};

pub fn field_table(
    src: &Sources,
    by_field: &BTreeMap<&str, Vec<&Entry>>,
) -> Result<String, GuideError> {
    let fields = &src.registry.fields.fields;
    let count = |v: ValidatedByCore| fields.iter().filter(|f| f.validated_by_core == v).count();
    let mut out = format!(
        "{} fields: {} validated, {} partly validated, {} not validated by stellar-core.\n\n",
        fields.len(),
        count(ValidatedByCore::Yes),
        count(ValidatedByCore::Partial),
        count(ValidatedByCore::No),
    );
    out.push_str(
        "| Field | Core validates | What core checks | Risk if trusted, and handling | Source | Corpus |\n",
    );
    out.push_str("| --- | --- | --- | --- | --- | --- |\n");
    for f in fields {
        let n = by_field.get(f.id.as_str()).map_or(0, Vec::len);
        let corpus = if n == 0 {
            "none".to_string()
        } else {
            format!("[{n}](#{CORPUS_ANCHOR})")
        };
        out.push_str(&format!(
            "| {} | {} | {} | **Risk:** {}<br />**Handling:** {} | {} | {} |\n",
            field_cell(f)?,
            validated_word(f.validated_by_core),
            sentence(&f.checks),
            sentence(&f.risk),
            sentence(&f.handling),
            citations_cell(&f.citations)?,
            corpus,
        ));
    }
    Ok(out)
}

pub fn corpus_by_field(
    src: &Sources,
    by_field: &BTreeMap<&str, Vec<&Entry>>,
) -> Result<String, GuideError> {
    let mut out = String::new();
    for f in &src.registry.fields.fields {
        let Some(entries) = by_field.get(f.id.as_str()) else {
            continue;
        };
        let mut links = Vec::with_capacity(entries.len());
        for e in entries {
            links.push(entry_link(e, src.links)?);
        }
        out.push_str(&format!(
            "- {} ({}): {}\n",
            mdx::code(&f.id)?,
            entries.len(),
            links.join(", ")
        ));
    }
    let uncovered: Vec<&Field> = src
        .registry
        .fields
        .fields
        .iter()
        .filter(|f| f.validated_by_core != ValidatedByCore::Yes)
        .filter(|f| !by_field.contains_key(f.id.as_str()))
        .collect();
    if !uncovered.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        let ids = uncovered
            .iter()
            .map(|f| mdx::code(&f.id))
            .collect::<Result<Vec<_>, _>>()?;
        out.push_str(&format!(
            "Fields that core does not validate, or validates only in part, and that no corpus entry exercises yet: {}.\n",
            ids.join(", ")
        ));
    }
    Ok(out)
}

pub fn caps_table(src: &Sources) -> Result<String, GuideError> {
    let mut out = String::from("| Cap | Applies to | Recommended | Derived from | Why |\n");
    out.push_str("| --- | --- | --- | --- | --- |\n");
    for l in &src.registry.limits.limits {
        out.push_str(&format!(
            "| {} | {} | {} {} | {} | {} |\n",
            limit_cell(l)?,
            sentence(&l.applies_to),
            l.recommended,
            mdx::text(&l.unit),
            derivations_cell(&l.derived_from)?,
            sentence(&l.rationale),
        ));
    }
    Ok(out)
}

pub fn checklist(src: &Sources) -> Result<String, GuideError> {
    let mut out = String::new();
    for r in &src.registry.lint_rules.rules {
        out.push_str(&format!(
            "- [ ] {} {}: {}. {}\n",
            mdx::code(&r.id)?,
            mdx::code(&r.name)?,
            mdx::text(&r.description),
            mdx::text(&r.checklist),
        ));
    }
    for item in &src.checklist.items {
        let mut line = format!(
            "- [ ] **{}** {}",
            mdx::text(&item.title),
            mdx::text(&item.text)
        );
        if !item.fields.is_empty() {
            let ids = item
                .fields
                .iter()
                .map(|f| mdx::code(f))
                .collect::<Result<Vec<_>, _>>()?;
            line.push_str(&format!(" Fields: {}.", ids.join(", ")));
        }
        if !item.limits.is_empty() {
            let ids = item
                .limits
                .iter()
                .map(|l| mdx::code(l))
                .collect::<Result<Vec<_>, _>>()?;
            line.push_str(&format!(" Caps: {}.", ids.join(", ")));
        }
        if !item.links.is_empty() {
            let links: Vec<String> = item
                .links
                .iter()
                .map(|l| format!("[{}]({})", mdx::text(&l.label), l.url))
                .collect();
            line.push_str(&format!(" Sources: {}.", links.join(", ")));
        }
        out.push_str(&line);
        out.push('\n');
    }
    Ok(out)
}

/// Where an entry's record lives inside the corpus directory: `entry.json` next to an xdr
/// entry's variants, or the vector file of a non-xdr entry.
pub fn entry_path(e: &Entry) -> Result<String, GuideError> {
    if let Some(v) = &e.vector {
        return Ok(v.path.clone());
    }
    let (_, variant) = e.variants.iter().next().ok_or_else(|| {
        GuideError::EntryFile(format!("{} has neither variants nor a vector file", e.id))
    })?;
    let dir = variant
        .xdr_file
        .rsplit_once('/')
        .map(|(d, _)| d)
        .ok_or_else(|| {
            GuideError::EntryFile(format!(
                "{} variant file {} has no directory",
                e.id, variant.xdr_file
            ))
        })?;
    Ok(format!("{dir}/{}", shapekit_manifest::build::ENTRY_FILE))
}

fn entry_link(e: &Entry, links: &CorpusLinks) -> Result<String, GuideError> {
    let label = mdx::text(&e.id);
    Ok(match links {
        CorpusLinks::Plain => mdx::code(&e.id)?,
        CorpusLinks::Relative(base) | CorpusLinks::Url(base) => {
            format!(
                "[{label}]({}/{})",
                base.trim_end_matches('/'),
                entry_path(e)?
            )
        }
    })
}

fn field_cell(f: &Field) -> Result<String, GuideError> {
    let protocols = match (f.protocol_min, f.protocol_max) {
        (0, None) => "all protocols".to_string(),
        (min, None) => format!("protocol {min} and later"),
        (min, Some(max)) => format!("protocols {min} to {max}"),
    };
    Ok(format!(
        "{}<br />{}<br />{protocols}",
        mdx::code(&f.id)?,
        mdx::code(&f.xdr_path)?
    ))
}

fn validated_word(v: ValidatedByCore) -> &'static str {
    match v {
        ValidatedByCore::Yes => "yes",
        ValidatedByCore::Partial => "partial",
        ValidatedByCore::No => "no",
    }
}

fn limit_cell(l: &Limit) -> Result<String, GuideError> {
    let mut cell = mdx::code(&l.id)?;
    if l.network_setting {
        cell.push_str("<br />network setting");
    }
    if let Some(p) = l.protocol_min {
        cell.push_str(&format!("<br />protocol {p} and later"));
    }
    Ok(cell)
}

fn citations_cell(citations: &[Citation]) -> Result<String, GuideError> {
    let mut parts = Vec::with_capacity(citations.len());
    for c in citations {
        let url = format!(
            "{}{}",
            source_url(&c.repo, &c.git_ref, &c.path),
            line_anchor(&c.lines)?
        );
        parts.push(format!("[{}]({url})", mdx::code(&c.symbol)?));
    }
    Ok(parts.join("<br />"))
}

fn derivations_cell(derived: &[Derivation]) -> Result<String, GuideError> {
    let mut parts = Vec::with_capacity(derived.len());
    for d in derived {
        parts.push(match d {
            Derivation::SourceCode(s) => format!(
                "[{}]({}) = {}",
                mdx::code(&s.symbol)?,
                source_url(&s.repo, &s.git_ref, &s.path),
                mdx::code(&value_text(&s.value)?)?
            ),
            Derivation::Measurement(m) => format!(
                "[{}]({}) = {}, measured: {}",
                mdx::code(&m.symbol)?,
                source_url(&m.repo, &m.git_ref, &m.path),
                mdx::code(&value_text(&m.value)?)?,
                mdx::text(&m.measured_with)
            ),
            Derivation::NetworkSetting(n) => format!(
                "{} = {} on {}, read {} at ledger {}",
                mdx::code(&n.setting)?,
                mdx::code(&value_text(&n.value)?)?,
                mdx::text(&n.network),
                mdx::text(&n.read_at),
                n.ledger
            ),
        });
    }
    Ok(parts.join("<br />"))
}

fn value_text(v: &toml::Value) -> Result<String, GuideError> {
    match v {
        toml::Value::String(s) => Ok(s.clone()),
        toml::Value::Integer(i) => Ok(i.to_string()),
        other => Err(GuideError::Unrenderable(format!(
            "a derivation value must be a string or an integer, not {other}"
        ))),
    }
}

fn source_url(repo: &str, git_ref: &str, path: &str) -> String {
    format!("https://github.com/{repo}/blob/{git_ref}/{path}")
}

/// GitHub line anchor for a citation's `lines`: `"a-b"`, a single line number, or `[a, b]`.
fn line_anchor(lines: &toml::Value) -> Result<String, GuideError> {
    let bad = || GuideError::Unrenderable(format!("citation lines {lines} is not a line range"));
    match lines {
        toml::Value::Integer(n) if *n > 0 => Ok(format!("#L{n}")),
        toml::Value::String(s) => {
            let (a, b) = s.split_once('-').ok_or_else(bad)?;
            let a: u32 = a.trim().parse().map_err(|_| bad())?;
            let b: u32 = b.trim().parse().map_err(|_| bad())?;
            if a == 0 || b < a {
                return Err(bad());
            }
            Ok(format!("#L{a}-L{b}"))
        }
        toml::Value::Array(pair) => match pair.as_slice() {
            [toml::Value::Integer(a), toml::Value::Integer(b)] if *a > 0 && b >= a => {
                Ok(format!("#L{a}-L{b}"))
            }
            _ => Err(bad()),
        },
        _ => Err(bad()),
    }
}

/// Registry prose is written lowercase; a table cell reads better as a sentence. Only a first
/// word made of lowercase letters is capitalised, so identifiers such as `toAccountID` stay as
/// they are, and a missing final full stop is added.
fn sentence(s: &str) -> String {
    let s = s.trim();
    let first: String = s.chars().take_while(|c| !c.is_whitespace()).collect();
    let mut out = if first == "xdr" {
        format!("XDR{}", &s[first.len()..])
    } else if !first.is_empty() && first.chars().all(|c| c.is_ascii_lowercase()) {
        let mut chars = s.chars();
        let head = chars.next().map(|c| c.to_ascii_uppercase());
        head.into_iter().chain(chars).collect()
    } else {
        s.to_string()
    };
    if !out.ends_with(['.', '!', '?']) {
        out.push('.');
    }
    mdx::text(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_anchors() {
        assert_eq!(
            line_anchor(&toml::Value::String("12-40".into())).unwrap(),
            "#L12-L40"
        );
        assert_eq!(line_anchor(&toml::Value::Integer(7)).unwrap(), "#L7");
        let pair = toml::Value::Array(vec![toml::Value::Integer(3), toml::Value::Integer(9)]);
        assert_eq!(line_anchor(&pair).unwrap(), "#L3-L9");
        assert!(line_anchor(&toml::Value::String("9-3".into())).is_err());
        assert!(line_anchor(&toml::Value::String("x".into())).is_err());
        assert!(line_anchor(&toml::Value::Boolean(true)).is_err());
    }

    #[test]
    fn sentences() {
        assert_eq!(sentence("never checked"), "Never checked.");
        assert_eq!(sentence("toAccountID drops it"), "toAccountID drops it.");
        assert_eq!(sentence("xdr limit only."), "XDR limit only.");
        assert_eq!(sentence("a <= b"), "A \\<= b.");
    }
}
