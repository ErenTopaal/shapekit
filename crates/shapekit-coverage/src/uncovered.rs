//! `registry/uncovered.toml`: why an indexer-visible CAP, a reachable union arm or a family
//! fragment has no core-emitted corpus entry.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::CoverageError;

pub const UNCOVERED_FILE: &str = "uncovered.toml";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UncoveredFile {
    pub schema_version: u32,
    #[serde(default, rename = "cap")]
    pub caps: Vec<CapReason>,
    #[serde(default, rename = "arm")]
    pub arms: Vec<ArmReason>,
    #[serde(default, rename = "fragment")]
    pub fragments: Vec<FragmentReason>,
    #[serde(default, rename = "planned")]
    pub planned: Vec<PlannedShape>,
    #[serde(default, rename = "history-only-arm")]
    pub history_only_arms: Vec<HistoryOnlyArm>,
}

/// A union arm the corpus covers only with public network history, because a local core
/// cannot write it. Coverage requires it to be covered, and only by history entries; the
/// explorer leaves it out of the arms it aims for, so this table is the one list of them.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryOnlyArm {
    pub arm: String,
    /// Why a local core never writes it, with the source file and lines at the pinned tag.
    pub reason: String,
}

/// Why no core-emitted entry exists, coarse enough to count and fine enough to act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Blocker {
    /// core refuses the transaction before it can reach a ledger, so the arm exists only in a
    /// submit-time result.
    RefusedBeforeLedger,
    /// only ledgers below some protocol carry it, and no recipe records a chain that old;
    /// public history from those protocols holds it.
    OldProtocolOnly,
    /// only a public network can hold it, and no capture of it exists yet.
    HistoryOnly,
    /// core at the pinned version has a code path for it, but a private standalone network
    /// cannot reach that path.
    NotProducibleLocally,
    /// the pinned core has no code path that ever produces it.
    NeverEmitted,
    /// the core goldens hold it, but the importer takes one golden per family fragment and no
    /// fragment names it.
    NotImported,
    /// a local core can produce it and no recipe records it yet.
    RecipeGap,
}

impl Blocker {
    /// The name `uncovered.toml` spells the blocker with.
    pub fn name(self) -> &'static str {
        match self {
            Blocker::RefusedBeforeLedger => "refused-before-ledger",
            Blocker::OldProtocolOnly => "old-protocol-only",
            Blocker::HistoryOnly => "history-only",
            Blocker::NotProducibleLocally => "not-producible-locally",
            Blocker::NeverEmitted => "never-emitted",
            Blocker::NotImported => "not-imported",
            Blocker::RecipeGap => "recipe-gap",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapReason {
    pub id: String,
    pub blocker: Blocker,
    pub reason: String,
}

/// One reason shared by a group of arms, each named in full so a reader can grep for it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArmReason {
    pub arms: Vec<String>,
    pub blocker: Blocker,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FragmentReason {
    pub family: String,
    pub fragment: String,
    pub blocker: Blocker,
    pub reason: String,
}

/// A shape a family's notes plan for that no signature fragment can express, so fragment
/// coverage cannot show it missing.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedShape {
    pub family: String,
    pub shape: String,
    pub blocker: Blocker,
    pub reason: String,
}

impl UncoveredFile {
    pub fn load(registry_dir: &Path) -> Result<UncoveredFile, CoverageError> {
        let path = registry_dir.join(UNCOVERED_FILE);
        let text = std::fs::read_to_string(&path).map_err(|source| CoverageError::Io {
            path: path.clone(),
            source,
        })?;
        toml::from_str(&text).map_err(|e| CoverageError::Parse {
            path,
            message: e.to_string(),
        })
    }

    pub fn cap(&self, id: &str) -> Option<&CapReason> {
        self.caps.iter().find(|c| c.id == id)
    }

    pub fn arm(&self, arm: &str) -> Option<&ArmReason> {
        self.arms.iter().find(|r| r.arms.iter().any(|a| a == arm))
    }

    pub fn history_only(&self, arm: &str) -> Option<&HistoryOnlyArm> {
        self.history_only_arms.iter().find(|r| r.arm == arm)
    }

    pub fn fragment(&self, family: &str, fragment: &str) -> Option<&FragmentReason> {
        self.fragments
            .iter()
            .find(|r| r.family == family && r.fragment == fragment)
    }
}
