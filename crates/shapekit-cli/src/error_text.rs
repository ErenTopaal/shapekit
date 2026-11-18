//! one line for an error and its causes, each cause printed once.

use std::fmt::Write as _;

/// renders `err` and every cause after it, joined with ": ".
///
/// many library errors put their source in their own message (`"reading {path}: {source}"`) and
/// also return it from `source()`, so anyhow's `{:#}` prints that cause twice. their own message
/// has to keep the source, because they are also printed alone with `{}` (in reports, findings
/// and logs). so the chain is joined here instead, and a cause whose text the line already ends
/// with is left out: it is the one the previous message spelled out.
pub fn chain(err: &(dyn std::error::Error + 'static)) -> String {
    let mut out = String::new();
    for cause in std::iter::successors(Some(err), |e| e.source()) {
        let text = cause.to_string();
        if out.is_empty() {
            out = text;
        } else if !text.is_empty() && !out.ends_with(&text) {
            let _ = write!(out, ": {text}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::chain;
    use std::fmt;

    // a library error shaped like the thiserror ones: its message ends with its source, and
    // source() returns the same error
    #[derive(Debug)]
    struct Reading {
        path: &'static str,
        source: std::io::Error,
    }

    impl fmt::Display for Reading {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "reading {}: {}", self.path, self.source)
        }
    }

    impl std::error::Error for Reading {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.source)
        }
    }

    fn missing() -> std::io::Error {
        std::io::Error::from(std::io::ErrorKind::NotFound)
    }

    #[test]
    fn a_cause_the_message_already_carries_prints_once() {
        let err = anyhow::Error::new(Reading {
            path: "/x.json",
            source: missing(),
        })
        .context("validating the manifest");
        let cause = missing().to_string();
        let text = chain(err.as_ref());
        assert_eq!(
            text,
            format!("validating the manifest: reading /x.json: {cause}")
        );
        assert_eq!(text.matches(&cause).count(), 1);
    }

    #[test]
    fn a_cause_the_message_leaves_out_is_appended() {
        let err = anyhow::Error::new(missing()).context("opening /x.json");
        assert_eq!(
            chain(err.as_ref()),
            format!("opening /x.json: {}", missing())
        );
    }
}
