//! MDX text escaping and the marked regions the generated tables live in.

use std::collections::BTreeMap;

use crate::GuideError;

/// Escapes registry prose for MDX: markdown punctuation, JSX braces and angle brackets, and
/// table pipes. Newlines become spaces because every use is inside one table cell or list item.
/// An underscore between two letters or digits cannot start emphasis and is left bare, which is
/// also how prettier writes it, so the stellar-docs formatter leaves the output unchanged.
pub fn text(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() + 8);
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '_' => {
                let before = i.checked_sub(1).and_then(|j| chars.get(j));
                let after = chars.get(i + 1);
                let intraword = before.is_some_and(|b| b.is_ascii_alphanumeric())
                    && after.is_some_and(|a| a.is_ascii_alphanumeric());
                if !intraword {
                    out.push('\\');
                }
                out.push(c);
            }
            '\\' | '`' | '*' | '[' | ']' | '{' | '}' | '<' | '>' | '|' | '~' => {
                out.push('\\');
                out.push(c);
            }
            '\n' | '\r' => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}

/// An inline code span. Pipes still split table cells inside code, so they are escaped; the
/// registry never puts a backtick inside a code value, and one here would be a registry bug.
pub fn code(s: &str) -> Result<String, GuideError> {
    if s.contains('`') || s.contains('\n') {
        return Err(GuideError::Unrenderable(format!(
            "{s:?} cannot be an inline code span"
        )));
    }
    Ok(format!("`{}`", s.replace('|', "\\|")))
}

pub fn begin_marker(name: &str) -> String {
    format!("{{/* shapekit:begin {name} */}}")
}

pub fn end_marker(name: &str) -> String {
    format!("{{/* shapekit:end {name} */}}")
}

/// The text between each region's begin and end marker lines, keyed by region name. Every
/// name in `names` must appear exactly once, in a begin and end pair, and nothing else may
/// look like a marker.
pub fn extract(doc: &str, names: &[&str]) -> Result<BTreeMap<String, String>, GuideError> {
    let mut out = BTreeMap::new();
    for name in names {
        let (start, end) = locate(doc, name)?;
        out.insert((*name).to_string(), doc[start..end].to_string());
    }
    for line in doc.lines() {
        let t = line.trim();
        if t.starts_with("{/* shapekit:") {
            let known = names
                .iter()
                .any(|n| t == begin_marker(n) || t == end_marker(n));
            if !known {
                return Err(GuideError::Markers(format!("unknown marker line {t:?}")));
            }
        }
    }
    Ok(out)
}

/// Replaces the body of every named region with the given text.
pub fn splice(doc: &str, regions: &[(&str, String)]) -> Result<String, GuideError> {
    let names: Vec<&str> = regions.iter().map(|(n, _)| *n).collect();
    extract(doc, &names)?;
    let mut out = doc.to_string();
    for (name, body) in regions {
        let (start, end) = locate(&out, name)?;
        out.replace_range(start..end, body);
    }
    Ok(out)
}

/// Byte range of a region body: from the start of the line after the begin marker to the
/// start of the end marker line.
fn locate(doc: &str, name: &str) -> Result<(usize, usize), GuideError> {
    let begin = begin_marker(name);
    let end = end_marker(name);
    let begins = marker_lines(doc, &begin);
    let ends = marker_lines(doc, &end);
    match (begins.as_slice(), ends.as_slice()) {
        ([(_, b_end)], [(e_start, _)]) if b_end <= e_start => Ok((*b_end, *e_start)),
        ([_], [_]) => Err(GuideError::Markers(format!(
            "region {name} ends before it begins"
        ))),
        _ => Err(GuideError::Markers(format!(
            "region {name} needs exactly one {begin} line and one {end} line, found {} and {}",
            begins.len(),
            ends.len()
        ))),
    }
}

/// (start of line, start of next line) for every line equal to `marker` after trimming.
fn marker_lines(doc: &str, marker: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut pos = 0;
    for line in doc.split_inclusive('\n') {
        if line.trim() == marker {
            out.push((pos, pos + line.len()));
        }
        pos += line.len();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_mdx_specials() {
        assert_eq!(
            text("a <= b | {x} *y* _z_ [l] `c` ~"),
            "a \\<= b \\| \\{x\\} \\*y\\* \\_z\\_ \\[l\\] \\`c\\` \\~"
        );
        assert_eq!(
            text("tx_bad __check_auth a_"),
            "tx_bad \\_\\_check_auth a\\_"
        );
        assert_eq!(text("one\ntwo"), "one two");
    }

    #[test]
    fn code_spans_escape_pipes_and_refuse_backticks() {
        assert_eq!(code("A | B").unwrap(), "`A \\| B`");
        assert!(code("a`b").is_err());
    }

    #[test]
    fn splices_and_extracts() {
        let doc = "intro\n{/* shapekit:begin t */}\nold\n{/* shapekit:end t */}\ntail\n";
        let out = splice(doc, &[("t", "\nnew\n\n".to_string())]).unwrap();
        assert_eq!(
            out,
            "intro\n{/* shapekit:begin t */}\n\nnew\n\n{/* shapekit:end t */}\ntail\n"
        );
        assert_eq!(extract(&out, &["t"]).unwrap()["t"], "\nnew\n\n");
    }

    #[test]
    fn rejects_bad_markers() {
        let missing = "{/* shapekit:begin t */}\n";
        assert!(extract(missing, &["t"]).is_err());
        let twice = "{/* shapekit:begin t */}\n{/* shapekit:end t */}\n{/* shapekit:end t */}\n";
        assert!(extract(twice, &["t"]).is_err());
        let reversed = "{/* shapekit:end t */}\n{/* shapekit:begin t */}\n";
        assert!(extract(reversed, &["t"]).is_err());
        let stray = "{/* shapekit:begin t */}\n{/* shapekit:end t */}\n{/* shapekit:begin u */}\n";
        assert!(extract(stray, &["t"]).is_err());
    }
}
