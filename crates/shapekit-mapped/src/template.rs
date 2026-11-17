//! `{name}` placeholders in the mapping file's strings. `{{` and `}}` stand for literal braces.

use std::collections::BTreeMap;

use anyhow::{Result, bail};

/// The values placeholders resolve to, by name: variables, `fact.<name>`, `service.<name>.ip`
/// and the rest documented in docs/mapping-adapter.md.
#[derive(Debug, Clone, Default)]
pub struct Context {
    values: BTreeMap<String, String>,
}

impl Context {
    pub fn set(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.values.insert(name.into(), value.into());
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    /// Replaces every placeholder in `s`. A placeholder with no value is an error naming it,
    /// so a typo never reaches the target as literal text.
    pub fn expand(&self, s: &str) -> Result<String> {
        let mut out = String::with_capacity(s.len());
        let mut rest = s;
        while let Some(i) = rest.find(['{', '}']) {
            out.push_str(&rest[..i]);
            let tail = &rest[i..];
            if let Some(t) = tail.strip_prefix("{{") {
                out.push('{');
                rest = t;
                continue;
            }
            if let Some(t) = tail.strip_prefix("}}") {
                out.push('}');
                rest = t;
                continue;
            }
            if tail.starts_with('}') {
                bail!("unmatched }} in {s:?}");
            }
            let Some(end) = tail.find('}') else {
                bail!("unclosed {{ in {s:?}");
            };
            let name = &tail[1..end];
            match self.values.get(name) {
                Some(v) => out.push_str(v),
                None => bail!("{{{name}}} in {s:?} has no value here"),
            }
            rest = &tail[end + 1..];
        }
        out.push_str(rest);
        Ok(out)
    }

    pub fn expand_all(&self, items: &[String]) -> Result<Vec<String>> {
        items.iter().map(|s| self.expand(s)).collect()
    }

    pub fn expand_map(&self, m: &BTreeMap<String, String>) -> Result<BTreeMap<String, String>> {
        m.iter()
            .map(|(k, v)| Ok((k.clone(), self.expand(v)?)))
            .collect()
    }
}

/// The placeholder names `s` uses, for checking a mapping before anything runs.
pub fn names(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find('{') {
        let tail = &rest[i..];
        if let Some(t) = tail.strip_prefix("{{") {
            rest = t;
            continue;
        }
        let Some(end) = tail.find('}') else { break };
        out.push(tail[1..end].to_owned());
        rest = &tail[end + 1..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_and_refuses_unknown_names() {
        let mut c = Context::default();
        c.set("commit", "abc");
        c.set("service.db.ip", "10.0.0.2");
        assert_eq!(
            c.expand("x:{commit}@{service.db.ip} {{lit}}").unwrap(),
            "x:abc@10.0.0.2 {lit}"
        );
        assert!(c.expand("{nope}").is_err());
        assert!(c.expand("{open").is_err());
        assert_eq!(names("a {b} {{c}} {d.e}"), vec!["b", "d.e"]);
    }
}
