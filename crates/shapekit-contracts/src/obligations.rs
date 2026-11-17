//! The obligation ledger, `corpus/coverage/obligations.json`: everything the corpus owes a
//! witness for, and whether it has one.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Contract, sorted_unique, witness};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObligationLedger {
    pub schema_version: u32,
    /// Sorted by id, each id once.
    pub obligations: Vec<Obligation>,
}

/// One obligation. Its id starts with its kind and a colon, `gate-side:...`, so ledgers
/// written by different producers merge without collisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Obligation {
    pub id: String,
    pub kind: ObligationKind,
    pub status: ObligationStatus,
    /// What meets it, as witness references (`entry:`, `candidate:`, `chain:`, `recipe:`,
    /// `dataset:`).
    /// Required for `met`, empty otherwise.
    #[serde(default)]
    pub witnesses: Vec<String>,
    /// Why it cannot be met (required for `unmeetable`), or what is known about an open one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ObligationKind {
    /// A decision point of the XDR schema, in its accept or its reject direction.
    Format,
    /// A point where a language's native types lose information an XDR value carries.
    Gap,
    /// A test section of core's own transaction tests.
    CoreTestSection,
    /// One side of a protocol version gate in core's source.
    GateSide,
    /// An envelope field core was asked about.
    Field,
    /// A published SDK version the corpus runs against.
    SdkVersion,
    /// A fault seeded into an SDK, which the corpus should catch.
    Seed,
}

impl ObligationKind {
    pub const ALL: [ObligationKind; 7] = [
        Self::Format,
        Self::Gap,
        Self::CoreTestSection,
        Self::GateSide,
        Self::Field,
        Self::SdkVersion,
        Self::Seed,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Format => "format",
            Self::Gap => "gap",
            Self::CoreTestSection => "core-test-section",
            Self::GateSide => "gate-side",
            Self::Field => "field",
            Self::SdkVersion => "sdk-version",
            Self::Seed => "seed",
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ObligationStatus {
    /// At least one witness meets it.
    Met,
    /// It cannot be met, and `evidence` says why.
    Unmeetable,
    /// Neither met nor shown unmeetable yet.
    Open,
}

impl ObligationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Met => "met",
            Self::Unmeetable => "unmeetable",
            Self::Open => "open",
        }
    }
}

/// The rules every status follows, shared with gate sides.
pub(crate) fn status_problems(
    what: &str,
    status: ObligationStatus,
    witnesses: &[String],
    evidence: Option<&str>,
    out: &mut Vec<String>,
) {
    witness::check_all(what, witnesses, out);
    match status {
        ObligationStatus::Met if witnesses.is_empty() => {
            out.push(format!("{what} is met but has no witness"));
        }
        ObligationStatus::Unmeetable | ObligationStatus::Open if !witnesses.is_empty() => {
            out.push(format!(
                "{what} is {} but lists witnesses; with a witness it is met",
                status.as_str()
            ));
        }
        _ => {}
    }
    if status == ObligationStatus::Unmeetable && evidence.is_none_or(|e| e.trim().is_empty()) {
        out.push(format!("{what} is unmeetable without evidence"));
    }
}

impl Obligation {
    /// `<kind>:<rest>`.
    pub fn id_for(kind: ObligationKind, rest: &str) -> String {
        format!("{}:{rest}", kind.as_str())
    }
}

impl ObligationLedger {
    /// How many obligations of `kind` have `status`.
    pub fn count(&self, kind: ObligationKind, status: ObligationStatus) -> usize {
        self.obligations
            .iter()
            .filter(|o| o.kind == kind && o.status == status)
            .count()
    }
}

impl Contract for ObligationLedger {
    const SCHEMA_FILE: &'static str = "obligations.schema.json";
    const VERSION: u32 = 1;
    const TITLE: &'static str = "shapekit obligation ledger";
    const DESCRIPTION: &'static str = "corpus/coverage/obligations.json: every obligation the corpus owes a witness for, with its status (met, unmeetable or open), its witnesses and its evidence.";

    fn schema_version(&self) -> u32 {
        self.schema_version
    }

    fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        sorted_unique("obligation", &self.obligations, |o| o.id.clone(), &mut out);
        for o in &self.obligations {
            let prefix = format!("{}:", o.kind.as_str());
            match o.id.strip_prefix(&prefix) {
                Some(rest) if !rest.is_empty() => {}
                _ => out.push(format!(
                    "obligation {} of kind {} must have an id {prefix}<name>",
                    o.id,
                    o.kind.as_str()
                )),
            }
            status_problems(
                &format!("obligation {}", o.id),
                o.status,
                &o.witnesses,
                o.evidence.as_deref(),
                &mut out,
            );
        }
        out
    }
}
