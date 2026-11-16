//! Core coverage summary, `corpus/coverage/core-coverage.json`: which regions of core's code
//! the corpus chains reach, and the list of those they do not (the R list).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Contract, CoreRef, sorted_unique, witness};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CoreCoverage {
    pub schema_version: u32,
    /// The coverage build of core that ran the chains.
    pub core: CoreRef,
    /// The chains measured, as `chain:<id>` witness references, sorted.
    pub chains: Vec<String>,
    /// Per source file, sorted by path, each path once.
    pub files: Vec<FileCoverage>,
    /// The regions no measured chain reaches, sorted by (file, line_start, line_end).
    pub unreached: Vec<Region>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileCoverage {
    /// Path in core's repository.
    pub path: String,
    pub regions_reached: u64,
    pub regions_unreached: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub file: String,
    pub line_start: u32,
    pub line_end: u32,
    pub class: RegionClass,
    /// The core tests that reach the region, `[tx] payment/merge` style names. Required for
    /// `reachable-by-core-tests`, empty for `unknown`.
    #[serde(default)]
    pub core_tests: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RegionClass {
    /// Some core test reaches it, so it is reachable and a recipe can be searched for.
    ReachableByCoreTests,
    /// Neither a chain nor a core test reaches it; whether anything can is not known.
    Unknown,
}

impl Contract for CoreCoverage {
    const SCHEMA_FILE: &'static str = "core-coverage.schema.json";
    const VERSION: u32 = 1;
    const TITLE: &'static str = "shapekit core coverage summary";
    const DESCRIPTION: &'static str = "corpus/coverage/core-coverage.json: reached and unreached region counts per core source file for the measured chains, and every unreached region with the core tests that reach it.";

    fn schema_version(&self) -> u32 {
        self.schema_version
    }

    fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.core.problems(&mut out);
        sorted_unique("chain", &self.chains, Clone::clone, &mut out);
        witness::check_all("chains", &self.chains, &mut out);
        for c in &self.chains {
            if !c.starts_with("chain:") {
                out.push(format!("chains lists {c}, which is not a chain: reference"));
            }
        }
        sorted_unique("file", &self.files, |f| f.path.clone(), &mut out);
        sorted_unique(
            "region",
            &self.unreached,
            |r| (r.file.clone(), r.line_start, r.line_end),
            &mut out,
        );
        for r in &self.unreached {
            let what = format!("region {}:{}-{}", r.file, r.line_start, r.line_end);
            if r.line_start == 0 || r.line_start > r.line_end {
                out.push(format!("{what}: lines are 1-based and start <= end"));
            }
            if self
                .files
                .binary_search_by(|f| f.path.cmp(&r.file))
                .is_err()
            {
                out.push(format!("{what}: its file has no files entry"));
            }
            match (r.class, r.core_tests.is_empty()) {
                (RegionClass::ReachableByCoreTests, true) => {
                    out.push(format!(
                        "{what} is reachable-by-core-tests but names no test"
                    ));
                }
                (RegionClass::Unknown, false) => {
                    out.push(format!(
                        "{what} is unknown but names core tests that reach it"
                    ));
                }
                _ => {}
            }
        }
        for f in &self.files {
            let listed = self.unreached.iter().filter(|r| r.file == f.path).count() as u64;
            if listed > 0 && f.regions_unreached == 0 {
                out.push(format!(
                    "file {} has unreached regions listed but counts none",
                    f.path
                ));
            }
        }
        out
    }
}
