//! Protocol gates, `registry/generated/gates.json`: every call of core's protocol version
//! comparisons outside its tests, and the status of both branches of each.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::obligations::{ObligationKind, ObligationStatus, status_problems};
use crate::{Contract, CoreRef, sorted_unique};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GateList {
    pub schema_version: u32,
    /// The core source the gates were read from.
    pub core: CoreRef,
    /// Sorted by (file, line, column), each place once.
    pub gates: Vec<Gate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Gate {
    /// Path in core's repository, `src/transactions/SetOptionsOpFrame.cpp`.
    pub file: String,
    /// 1-based line of the comparison call.
    pub line: u32,
    /// 1-based column of the call, telling apart two gates on one line.
    pub column: u32,
    /// The function the call sits in, `SetOptionsOpFrame::doCheckValid`.
    pub function: String,
    pub comparison: Comparison,
    /// N of the `ProtocolVersion::V_N` constant compared against.
    pub version: u32,
    /// The branch where the comparison is true and the one where it is false, in that order.
    pub sides: Vec<GateSide>,
}

/// The helper core calls, from `src/util/ProtocolVersion.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Comparison {
    /// `protocolVersionIsBefore(v, V_N)`: true below N.
    IsBefore,
    /// `protocolVersionStartsFrom(v, V_N)`: true from N on.
    StartsFrom,
    /// `protocolVersionEquals(v, V_N)`: true at N only.
    Equals,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Branch {
    True,
    False,
}

impl Branch {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::True => "true",
            Self::False => "false",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GateSide {
    pub branch: Branch,
    pub status: ObligationStatus,
    /// Recipes or entries that run this branch. Required for `met`.
    #[serde(default)]
    pub witnesses: Vec<String>,
    /// Why the branch cannot run (required for `unmeetable`), or what is known about an open
    /// one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

impl Gate {
    /// The obligation id of one side in the ledger:
    /// `gate-side:<file>:<line>:<column>:<true|false>`.
    pub fn side_obligation_id(&self, branch: Branch) -> String {
        crate::obligations::Obligation::id_for(
            ObligationKind::GateSide,
            &format!(
                "{}:{}:{}:{}",
                self.file,
                self.line,
                self.column,
                branch.as_str()
            ),
        )
    }

    /// The protocols where `branch` runs, as an inclusive minimum and an optional inclusive
    /// maximum.
    pub fn protocols(&self, branch: Branch) -> Vec<(u32, Option<u32>)> {
        let n = self.version;
        let below = (0, n.checked_sub(1));
        let from = (n, None);
        match (self.comparison, branch) {
            (Comparison::IsBefore, Branch::True) | (Comparison::StartsFrom, Branch::False) => {
                if n == 0 { vec![] } else { vec![below] }
            }
            (Comparison::IsBefore, Branch::False) | (Comparison::StartsFrom, Branch::True) => {
                vec![from]
            }
            (Comparison::Equals, Branch::True) => vec![(n, Some(n))],
            (Comparison::Equals, Branch::False) => {
                let mut v = Vec::new();
                if n > 0 {
                    v.push(below);
                }
                v.push((n + 1, None));
                v
            }
        }
    }
}

impl Contract for GateList {
    const SCHEMA_FILE: &'static str = "gates.schema.json";
    const VERSION: u32 = 1;
    const TITLE: &'static str = "shapekit protocol gates";
    const DESCRIPTION: &'static str = "registry/generated/gates.json: every protocol version comparison in core's non-test source, with the status and witnesses of its true and its false branch.";

    fn schema_version(&self) -> u32 {
        self.schema_version
    }

    fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.core.problems(&mut out);
        sorted_unique(
            "gate",
            &self.gates,
            |g| (g.file.clone(), g.line, g.column),
            &mut out,
        );
        for g in &self.gates {
            let what = format!("gate {}:{}:{}", g.file, g.line, g.column);
            if g.line == 0 || g.column == 0 {
                out.push(format!("{what}: line and column are 1-based"));
            }
            let branches: Vec<Branch> = g.sides.iter().map(|s| s.branch).collect();
            if branches != [Branch::True, Branch::False] {
                out.push(format!(
                    "{what} must list its true side, then its false side; it lists {branches:?}"
                ));
            }
            for s in &g.sides {
                status_problems(
                    &format!("{what} {} side", s.branch.as_str()),
                    s.status,
                    &s.witnesses,
                    s.evidence.as_deref(),
                    &mut out,
                );
            }
        }
        out
    }
}
