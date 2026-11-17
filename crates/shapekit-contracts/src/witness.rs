//! What a witness list points at. Every witness is a string with a prefix naming where the
//! witness lives, so a ledger can mix corpus entries, candidates and chains.

use std::fmt;

/// One witness reference.
///
/// | form | meaning |
/// |---|---|
/// | `entry:<id>` | an entry of `corpus/manifest.json` |
/// | `candidate:<id>` | an entry of `candidate/manifest.json`, not promoted yet |
/// | `chain:<id>` | a committed chain, `corpus/chains/<id>/` |
/// | `recipe:<path>` | a recipe file, relative to the repository root |
/// | `dataset:<path>` | a committed dataset run report (core's test metas through the sdks), relative to the repository root |
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum WitnessRef {
    Entry(String),
    Candidate(String),
    Chain(String),
    Recipe(String),
    Dataset(String),
}

impl WitnessRef {
    pub fn parse(s: &str) -> Result<Self, String> {
        let (prefix, rest) = s.split_once(':').ok_or_else(|| {
            format!("witness {s:?} has no prefix (entry:, candidate:, chain:, recipe:, dataset:)")
        })?;
        if rest.is_empty() || rest.chars().any(char::is_whitespace) {
            return Err(format!("witness {s:?} has an empty or spaced reference"));
        }
        let rest = rest.to_owned();
        match prefix {
            "entry" => Ok(Self::Entry(rest)),
            "candidate" => Ok(Self::Candidate(rest)),
            "chain" => Ok(Self::Chain(rest)),
            "recipe" => Ok(Self::Recipe(rest)),
            "dataset" => Ok(Self::Dataset(rest)),
            _ => Err(format!(
                "witness {s:?} has an unknown prefix {prefix:?} (entry:, candidate:, chain:, recipe:, dataset:)"
            )),
        }
    }
}

impl fmt::Display for WitnessRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Entry(s) => write!(f, "entry:{s}"),
            Self::Candidate(s) => write!(f, "candidate:{s}"),
            Self::Chain(s) => write!(f, "chain:{s}"),
            Self::Recipe(s) => write!(f, "recipe:{s}"),
            Self::Dataset(s) => write!(f, "dataset:{s}"),
        }
    }
}

pub(crate) fn check_all(what: &str, witnesses: &[String], out: &mut Vec<String>) {
    for w in witnesses {
        if let Err(e) = WitnessRef::parse(w) {
            out.push(format!("{what}: {e}"));
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for w in witnesses {
        if !seen.insert(w) {
            out.push(format!("{what}: witness {w} is listed twice"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WitnessRef;

    #[test]
    fn parses_and_prints_every_form() {
        for s in [
            "entry:cap01.bump-sequence.p30-bump-forward",
            "candidate:probe.memo.p30-id-max",
            "chain:cap40.signed-payload-signer.p30",
            "recipe:corpus/recipes/cap01.bump-sequence/p30.json",
            "dataset:conformance/dataset/dataset-report.json",
        ] {
            assert_eq!(WitnessRef::parse(s).unwrap().to_string(), s);
        }
        assert!(WitnessRef::parse("cap01.bump-sequence").is_err());
        assert!(WitnessRef::parse("entry:").is_err());
        assert!(WitnessRef::parse("ledger:5").is_err());
    }
}
