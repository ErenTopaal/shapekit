//! Recipe IR, `corpus/recipes/<family>/<name>.json`: a recipe as data. The steps never name a
//! network: keys are derived from the recipe id and a role, and values that depend on the
//! network or the chain's state are written as symbolic references, so one file records the
//! same chain under every passphrase.
//!
//! Steps that submit a transaction (`fund`, `tx`, `upload`, `deploy`, `invoke`,
//! `host-function`, `extend-ttl`, `restore`) queue it; the
//! next `close` closes one ledger holding everything queued since the previous close. A `tx`
//! or `invoke` expected to be `rejected` is refused at submission and never queued.
//!
//! # Symbolic references
//!
//! Inside XDR-JSON values (`operations`, `memo`, `cond`, `args`, `key`, `config`, `function`,
//! `keys`, auth `entries`), a string
//! that starts with `@` is a reference the interpreter replaces before encoding:
//!
//! | reference | replaced by |
//! |---|---|
//! | `@account:<role>` | the role's account, `G...` |
//! | `@muxed:<role>:<id>` | the role's account muxed with the decimal u64 `id`, `M...` |
//! | `@contract:<label>` | the contract a `deploy` step labelled so created, or a `host-function` step whose function creates one (`create_contract`, `create_contract_v2`, the Stellar asset contract included), read from its return address, `C...` |
//! | `@muxed-contract:<label>:<id>` | the contract `@contract:<label>` names, muxed with the decimal u64 `id`, `W...` |
//! | `@wasm:<label>` | the hex hash of the code an `upload` step labelled so uploaded |
//! | `@seq:<role>` or `@seq:<role>+<n>` | the role's current sequence number, plus `n` |
//! | `@ledger-start:<n>` | the starting sequence number of ledger last-closed + `n`, `(last + n) << 32` |
//!
//! A string that starts with `@@` stands for the same string with one `@` removed. The role
//! `root` is the network's root account.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Contract, is_hex, parse_decimal};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecipeIr {
    pub schema_version: u32,
    /// Derives every role's key; recipes sharing an id share their accounts.
    pub id: String,
    /// The `registry/families.toml` family every capture belongs to.
    pub family: String,
    /// The protocol the chain is upgraded to after genesis.
    pub protocol: u32,
    #[serde(default)]
    pub node: NodeOptions,
    /// The chain runs below its family's `protocol_valid.min` on purpose, to record contrast
    /// entries; each is flagged `below-protocol-valid`.
    #[serde(default)]
    pub below_protocol_valid: bool,
    pub steps: Vec<Step>,
}

/// How the standalone core is configured before the first step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodeOptions {
    /// Apply testnet's Soroban network settings before the first step (protocol 30 only).
    #[serde(default)]
    pub soroban_settings: bool,
    /// Start core with its high Soroban limit override; implied by `soroban_settings`.
    #[serde(default)]
    pub soroban_limits: bool,
    /// `TESTING_MINIMUM_PERSISTENT_ENTRY_LIFETIME`, at least 10; core's default when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_persistent_ttl: Option<u32>,
    /// `EMIT_CLASSIC_EVENTS`.
    #[serde(default = "yes")]
    pub emit_classic_events: bool,
    /// `BACKFILL_STELLAR_ASSET_EVENTS`.
    #[serde(default = "yes")]
    pub backfill_stellar_asset_events: bool,
    /// `EMIT_LEDGER_CLOSE_META_EXT_V1`: core writes `LedgerCloseMetaExt` v1 only with it, and
    /// core v30.0.0 leaves it off.
    #[serde(default, skip_serializing_if = "is_false")]
    pub emit_ledger_close_meta_ext_v1: bool,
    /// `EMIT_SOROBAN_TRANSACTION_META_EXT_V1`: core writes `SorobanTransactionMetaExt` v1 only
    /// with it, and core v30.0.0 leaves it off.
    #[serde(default, skip_serializing_if = "is_false")]
    pub emit_soroban_transaction_meta_ext_v1: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_true(b: &bool) -> bool {
    *b
}

fn yes() -> bool {
    true
}

impl Default for NodeOptions {
    fn default() -> Self {
        Self {
            soroban_settings: false,
            soroban_limits: false,
            min_persistent_ttl: None,
            emit_classic_events: true,
            backfill_stellar_asset_events: true,
            emit_ledger_close_meta_ext_v1: false,
            emit_soroban_transaction_meta_ext_v1: false,
        }
    }
}

/// One step, tagged by `step`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "step", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Step {
    /// The root account creates an account for each role, in one transaction.
    Fund {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        roles: Vec<String>,
        /// Starting balance of each, decimal stroops.
        balance: String,
    },
    /// A classic transaction.
    Tx {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        /// The role whose account is the transaction source and pays the fee.
        source: String,
        /// Decimal u64: the source is muxed with this id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_mux_id: Option<String>,
        /// XDR-JSON `Operation` values.
        operations: Vec<Value>,
        /// Roles that sign, in order; the source alone when empty.
        #[serde(default)]
        signers: Vec<String>,
        /// XDR-JSON `Memo`; none when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        memo: Option<Value>,
        /// Fee in stroops; the interpreter's default when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fee: Option<u32>,
        /// XDR-JSON `Preconditions`; the interpreter's default time bounds when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cond: Option<Value>,
        #[serde(default)]
        expect: TxExpect,
        /// Wraps the transaction in a fee bump envelope (CAP-15) before it is submitted.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fee_bump: Option<FeeBump>,
    },
    /// Closes one ledger with everything queued since the last close.
    Close {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        /// Labels of transaction steps queued since the previous close, in the order they
        /// must apply; no constraint when empty. Core orders a ledger's transactions by
        /// envelope hash, which differs per passphrase, so the interpreter fixes the order
        /// through the memo id of each named transaction, which therefore carries no memo.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        order: Vec<String>,
    },
    /// Arms ledger upgrades, then closes the ledger that applies them.
    Upgrade {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        protocol_version: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base_fee: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base_reserve: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_tx_set_size: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_soroban_tx_set_size: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        flags: Option<u32>,
        /// XDR-JSON `ConfigSettingEntry` values, applied as one config upgrade set.
        #[serde(default)]
        config: Vec<Value>,
    },
    /// Uploads contract code.
    Upload {
        label: String,
        source: String,
        /// The wasm file, relative to the repository root.
        wasm: String,
        /// Hex sha256 of the file, which is also the code's hash on chain.
        sha256: String,
    },
    /// Creates a contract instance of uploaded code.
    Deploy {
        label: String,
        source: String,
        /// Label of the `upload` step.
        code: String,
        /// 32 bytes, hex.
        salt: String,
        /// XDR-JSON `ScVal` constructor arguments.
        #[serde(default)]
        constructor_args: Vec<Value>,
    },
    /// Calls a contract function, footprint and resources from simulation.
    Invoke {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        source: String,
        /// Label of the `deploy` step, or a `@contract:` reference.
        contract: String,
        function: String,
        /// XDR-JSON `ScVal` arguments.
        #[serde(default)]
        args: Vec<Value>,
        /// How the call is authorized; the source account alone when absent.
        #[serde(default, skip_serializing_if = "AuthSpec::is_source")]
        auth: AuthSpec,
        /// Replaces simulated resource values; with `simulate: false`, every value.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resources: Option<ResourceOverride>,
        /// Simulate to find the footprint and resources. With `false` the transaction is
        /// built from `footprint` and `resources` alone, so core sees exactly what the recipe
        /// says, for example too few instructions for the call.
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        simulate: bool,
        /// The footprint, required with `simulate: false` and refused otherwise.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        footprint: Option<Footprint>,
        #[serde(default)]
        expect: TxExpect,
    },
    /// Any host function, built from its XDR-JSON: contract creation, uploads with odd
    /// arguments, calls with hand-written authorization.
    HostFunction {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        source: String,
        /// XDR-JSON `HostFunction`.
        function: Value,
        #[serde(default, skip_serializing_if = "AuthSpec::is_source")]
        auth: AuthSpec,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resources: Option<ResourceOverride>,
        /// As for `invoke`.
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        simulate: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        footprint: Option<Footprint>,
        #[serde(default)]
        expect: TxExpect,
    },
    /// Extends the TTL of ledger entries; `keys` are the read-only footprint, resources come
    /// from simulation.
    ExtendTtl {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        source: String,
        /// XDR-JSON `LedgerKey` values.
        keys: Vec<Value>,
        extend_to: u32,
        /// Replaces simulated resource values.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resources: Option<ResourceOverride>,
        #[serde(default)]
        expect: TxExpect,
    },
    /// Restores archived ledger entries; `keys` are the read-write footprint, resources come
    /// from simulation.
    Restore {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        source: String,
        /// XDR-JSON `LedgerKey` values.
        keys: Vec<Value>,
        /// Replaces simulated resource values.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resources: Option<ResourceOverride>,
        #[serde(default)]
        expect: TxExpect,
    },
    /// Records bytes of the chain as corpus entry `<family>.p<protocol>-<slug>`.
    Capture {
        slug: String,
        select: Selection,
        notes: String,
        /// Field ids from `registry/fields.toml`.
        #[serde(default)]
        fields: Vec<String>,
        #[serde(default)]
        flags: Vec<String>,
        /// Linked issues; the family's when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        issues: Option<Vec<String>>,
        /// The bytes do not decode under the shared decode limits.
        #[serde(default)]
        must_reject: bool,
    },
}

/// The fee bump envelope around a `tx` step's transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FeeBump {
    /// The role whose account pays the outer fee.
    pub source: String,
    /// The outer fee in stroops; the interpreter's default when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fee: Option<i64>,
    /// Roles that sign the fee bump, in order; the fee source alone when empty.
    #[serde(default)]
    pub signers: Vec<String>,
}

/// How a Soroban transaction is authorized, tagged by `mode`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "kebab-case", deny_unknown_fields)]
pub enum AuthSpec {
    /// The source account authorizes everything, as invocations did before auth modes.
    #[default]
    Source,
    /// Simulation in record mode writes the entries, which the named roles sign. The
    /// signatures expire at the last closed ledger plus 100.
    Record {
        /// Record entries for authorizations that are not at the root of the call tree.
        #[serde(default)]
        allow_non_root: bool,
        /// Write `address_v2` credentials instead of `address`.
        #[serde(default)]
        address_v2: bool,
        /// The role whose address appears in an entry, mapped to the roles that sign for it;
        /// an empty list means the role itself.
        #[serde(default)]
        signers: BTreeMap<String, Vec<String>>,
        /// The role whose address appears in an entry, mapped to the roles that sign as its
        /// delegates. A role with a non-empty list gets `address_with_delegates` credentials.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        delegates: BTreeMap<String, Vec<String>>,
    },
    /// The entries as written, simulated in enforce mode.
    Entries {
        /// XDR-JSON `SorobanAuthorizationEntry` values.
        entries: Vec<Value>,
        /// Each role signs the entry whose address is its account.
        #[serde(default)]
        sign: Vec<String>,
    },
}

impl AuthSpec {
    pub fn is_source(&self) -> bool {
        *self == AuthSpec::Source
    }
}

/// A Soroban transaction's footprint, written out instead of simulated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Footprint {
    /// XDR-JSON `LedgerKey` values.
    #[serde(default)]
    pub read_only: Vec<Value>,
    /// XDR-JSON `LedgerKey` values.
    #[serde(default)]
    pub read_write: Vec<Value>,
}

/// Resource values that replace what simulation found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourceOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_read_bytes: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_bytes: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_fee: Option<i64>,
}

/// What a transaction must come to.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TxExpect {
    #[serde(default)]
    pub outcome: Outcome,
    /// XDR-JSON `TransactionResultResult` the result must equal, when given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    /// Included in the next ledger and successful.
    #[default]
    Success,
    /// Included in the next ledger and failed there.
    Failed,
    /// Refused at submission.
    Rejected,
}

/// Which bytes a capture takes, tagged by `kind`. `of` names a labelled step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Selection {
    /// The `LedgerCloseMeta` of the ledger the step closed in (a `close` or `upgrade` step,
    /// or a step whose transaction a later close applied).
    Ledger { of: String },
    /// The step's transaction: its envelope and its result meta, two entries.
    Transaction { of: String },
    /// The ledger entry under `key` (XDR-JSON `LedgerKey`) as the step's ledger left it.
    LedgerEntry { of: String, key: Value },
    /// The envelope of a step refused at submission, with core's result.
    Rejected { of: String },
}

impl Selection {
    pub fn of(&self) -> &str {
        match self {
            Self::Ledger { of }
            | Self::Transaction { of }
            | Self::LedgerEntry { of, .. }
            | Self::Rejected { of } => of,
        }
    }
}

/// A parsed symbolic reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymRef {
    Account(String),
    Muxed {
        role: String,
        id: u64,
    },
    Contract(String),
    /// `@muxed-contract:<label>:<id>`: the `W` strkey of the contract the label made, muxed
    /// with `id`. Contract ids depend on the network id, so no literal can stand in for it.
    MuxedContract {
        label: String,
        id: u64,
    },
    Wasm(String),
    Seq {
        role: String,
        add: i64,
    },
    LedgerStart(u32),
    /// `@@...`: the string itself, one `@` removed.
    Literal(String),
}

impl SymRef {
    /// `None` for a string that is not a reference (does not start with `@`); an error for one
    /// that starts with `@` but does not parse.
    pub fn parse(s: &str) -> Option<Result<Self, String>> {
        let body = s.strip_prefix('@')?;
        if body.starts_with('@') {
            return Some(Ok(Self::Literal(body.to_owned())));
        }
        let bad = || format!("symbolic reference {s:?} does not parse");
        let Some((kind, rest)) = body.split_once(':') else {
            return Some(Err(bad()));
        };
        let name = |n: &str| {
            if is_name(n) {
                Ok(n.to_owned())
            } else {
                Err(bad())
            }
        };
        Some(match kind {
            "account" => name(rest).map(Self::Account),
            "contract" => name(rest).map(Self::Contract),
            "wasm" => name(rest).map(Self::Wasm),
            "muxed" => rest.split_once(':').ok_or_else(bad).and_then(|(role, id)| {
                let id = parse_decimal(id)
                    .and_then(|v| u64::try_from(v).ok())
                    .ok_or_else(bad)?;
                Ok(Self::Muxed {
                    role: name(role)?,
                    id,
                })
            }),
            "muxed-contract" => rest
                .split_once(':')
                .ok_or_else(bad)
                .and_then(|(label, id)| {
                    let id = parse_decimal(id)
                        .and_then(|v| u64::try_from(v).ok())
                        .ok_or_else(bad)?;
                    Ok(Self::MuxedContract {
                        label: name(label)?,
                        id,
                    })
                }),
            "seq" => {
                let (role, add) = match rest.split_once('+') {
                    Some((r, n)) => (
                        r,
                        parse_decimal(n)
                            .and_then(|v| i64::try_from(v).ok())
                            .filter(|v| *v >= 0)
                            .ok_or_else(bad),
                    ),
                    None => (rest, Ok(0)),
                };
                add.and_then(|add| {
                    Ok(Self::Seq {
                        role: name(role)?,
                        add,
                    })
                })
            }
            "ledger-start" => parse_decimal(rest)
                .and_then(|v| u32::try_from(v).ok())
                .map(Self::LedgerStart)
                .ok_or_else(bad),
            _ => Err(bad()),
        })
    }
}

impl fmt::Display for SymRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Account(r) => write!(f, "@account:{r}"),
            Self::Muxed { role, id } => write!(f, "@muxed:{role}:{id}"),
            Self::Contract(l) => write!(f, "@contract:{l}"),
            Self::MuxedContract { label, id } => write!(f, "@muxed-contract:{label}:{id}"),
            Self::Wasm(l) => write!(f, "@wasm:{l}"),
            Self::Seq { role, add: 0 } => write!(f, "@seq:{role}"),
            Self::Seq { role, add } => write!(f, "@seq:{role}+{add}"),
            Self::LedgerStart(n) => write!(f, "@ledger-start:{n}"),
            Self::Literal(s) => write!(f, "@{s}"),
        }
    }
}

/// Roles, labels and slugs: lowercase letters, digits and dashes, starting with a letter.
pub fn is_name(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Every symbolic reference inside a JSON value.
pub fn references(value: &Value, out: &mut Vec<Result<SymRef, String>>) {
    match value {
        Value::String(s) => out.extend(SymRef::parse(s)),
        Value::Array(items) => items.iter().for_each(|v| references(v, out)),
        Value::Object(map) => map.values().for_each(|v| references(v, out)),
        _ => {}
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LabelKind {
    Submitted { rejected: bool },
    Closed,
    Upload,
    Deploy,
}

struct Checker<'a> {
    out: &'a mut Vec<String>,
    roles: BTreeSet<String>,
    labels: BTreeMap<String, (usize, LabelKind)>,
    /// Index of the last close (`close` or `upgrade`) seen so far.
    last_close: Option<usize>,
    /// Labels of `tx` steps that set a memo.
    with_memo: BTreeSet<String>,
    /// Labels of `host-function` steps whose function creates a contract.
    creators: BTreeSet<String>,
}

impl Checker<'_> {
    fn role(&mut self, at: &str, role: &str, defining: bool) {
        if !is_name(role) {
            self.out
                .push(format!("{at}: role {role:?} is not a lowercase name"));
        } else if defining {
            if role == "root" {
                self.out.push(format!(
                    "{at}: root is the network's root account and is never funded"
                ));
            } else if !self.roles.insert(role.to_owned()) {
                self.out.push(format!("{at}: role {role} is funded twice"));
            }
        } else if role != "root" && !self.roles.contains(role) {
            self.out.push(format!(
                "{at}: role {role} is used before a fund step creates it"
            ));
        }
    }

    fn define(&mut self, at: &str, i: usize, label: Option<&String>, kind: LabelKind) {
        let Some(label) = label else { return };
        if !is_name(label) {
            self.out
                .push(format!("{at}: label {label:?} is not a lowercase name"));
        }
        if self.labels.insert(label.clone(), (i, kind)).is_some() {
            self.out.push(format!("{at}: label {label} is used twice"));
        }
    }

    fn want(
        &mut self,
        at: &str,
        label: &str,
        ok: impl Fn(LabelKind) -> bool,
        what: &str,
    ) -> Option<(usize, LabelKind)> {
        match self.labels.get(label).copied() {
            Some(found) if ok(found.1) => Some(found),
            Some(_) => {
                self.out.push(format!("{at}: {label} is not {what}"));
                None
            }
            None => {
                self.out
                    .push(format!("{at}: no earlier step is labelled {label}"));
                None
            }
        }
    }

    fn auth(&mut self, at: &str, auth: &AuthSpec) {
        match auth {
            AuthSpec::Source => {}
            AuthSpec::Record {
                signers, delegates, ..
            } => {
                for (role, by) in signers.iter().chain(delegates) {
                    self.role(at, role, false);
                    for s in by {
                        self.role(at, s, false);
                    }
                }
            }
            AuthSpec::Entries { entries, sign } => {
                if entries.is_empty() {
                    self.out
                        .push(format!("{at}: auth mode entries lists no entry"));
                }
                for s in sign {
                    self.role(at, s, false);
                }
                self.values(at, entries);
            }
        }
    }

    /// A step that does not simulate must say everything simulation would have found.
    fn unsimulated(
        &mut self,
        at: &str,
        simulate: bool,
        footprint: Option<&Footprint>,
        r: Option<&ResourceOverride>,
        auth: &AuthSpec,
    ) {
        if simulate {
            if footprint.is_some() {
                self.out.push(format!(
                    "{at}: a footprint is only written with simulate: false; simulation finds it otherwise"
                ));
            }
            return;
        }
        match footprint {
            None => self
                .out
                .push(format!("{at}: simulate: false needs an explicit footprint")),
            Some(f) => {
                self.values(at, &f.read_only);
                self.values(at, &f.read_write);
            }
        }
        let full = r.is_some_and(|r| {
            r.instructions.is_some()
                && r.disk_read_bytes.is_some()
                && r.write_bytes.is_some()
                && r.resource_fee.is_some()
        });
        if !full {
            self.out.push(format!(
                "{at}: simulate: false needs instructions, disk_read_bytes, write_bytes and resource_fee in resources"
            ));
        }
        if matches!(auth, AuthSpec::Record { .. }) {
            self.out.push(format!(
                "{at}: auth mode record writes its entries by simulation, so it needs simulate: true"
            ));
        }
    }

    fn resources(&mut self, at: &str, r: Option<&ResourceOverride>) {
        if let Some(r) = r
            && r.resource_fee.is_some_and(|f| f < 0)
        {
            self.out
                .push(format!("{at}: the resource fee cannot be negative"));
        }
    }

    fn values<'v>(&mut self, at: &str, values: impl IntoIterator<Item = &'v Value>) {
        let mut refs = Vec::new();
        for v in values {
            references(v, &mut refs);
        }
        for r in refs {
            match r {
                Err(e) => self.out.push(format!("{at}: {e}")),
                Ok(
                    SymRef::Account(role) | SymRef::Muxed { role, .. } | SymRef::Seq { role, .. },
                ) => self.role(at, &role, false),
                Ok(SymRef::Contract(l) | SymRef::MuxedContract { label: l, .. }) => {
                    if !self.creators.contains(&l) {
                        self.want(
                            at,
                            &l,
                            |k| k == LabelKind::Deploy,
                            "a deploy step or a host-function step that creates a contract",
                        );
                    }
                }
                Ok(SymRef::Wasm(l)) => {
                    self.want(at, &l, |k| k == LabelKind::Upload, "an upload step");
                }
                Ok(SymRef::LedgerStart(_) | SymRef::Literal(_)) => {}
            }
        }
    }
}

impl Contract for RecipeIr {
    const SCHEMA_FILE: &'static str = "recipe-ir.schema.json";
    const VERSION: u32 = 1;
    const TITLE: &'static str = "shapekit recipe IR";
    const DESCRIPTION: &'static str = "corpus/recipes/<family>/<name>.json: a recipe as network-independent steps (fund, tx, close, upgrade, upload, deploy, invoke, host-function, extend-ttl, restore, capture) with symbolic references for values that depend on the network or the chain's state.";

    fn schema_version(&self) -> u32 {
        self.schema_version
    }

    fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.id.is_empty() || self.family.is_empty() {
            out.push("a recipe needs an id and a family".into());
        }
        if let Some(ttl) = self.node.min_persistent_ttl
            && ttl < 10
        {
            out.push(format!(
                "min_persistent_ttl {ttl} is below core's floor of 10"
            ));
        }
        let mut c = Checker {
            out: &mut out,
            roles: BTreeSet::new(),
            labels: BTreeMap::new(),
            last_close: None,
            with_memo: BTreeSet::new(),
            creators: BTreeSet::new(),
        };
        let mut slugs = BTreeSet::new();
        for (i, step) in self.steps.iter().enumerate() {
            let at = format!("step {i}");
            match step {
                Step::Fund {
                    label,
                    roles,
                    balance,
                } => {
                    if roles.is_empty() {
                        c.out.push(format!("{at}: fund names no role"));
                    }
                    for r in roles {
                        c.role(&at, r, true);
                    }
                    if !parse_decimal(balance).is_some_and(|b| b > 0 && b <= i128::from(i64::MAX)) {
                        c.out
                            .push(format!("{at}: balance {balance:?} is not a positive i64"));
                    }
                    c.define(
                        &at,
                        i,
                        label.as_ref(),
                        LabelKind::Submitted { rejected: false },
                    );
                }
                Step::Tx {
                    label,
                    source,
                    source_mux_id,
                    operations,
                    signers,
                    memo,
                    cond,
                    expect,
                    fee_bump,
                    ..
                } => {
                    c.role(&at, source, false);
                    for s in signers {
                        c.role(&at, s, false);
                    }
                    if let Some(fb) = fee_bump {
                        c.role(&at, &fb.source, false);
                        for s in &fb.signers {
                            c.role(&at, s, false);
                        }
                        if fb.fee.is_some_and(|f| f <= 0) {
                            c.out
                                .push(format!("{at}: the fee bump fee must be positive"));
                        }
                    }
                    if let Some(id) = source_mux_id
                        && !parse_decimal(id).is_some_and(|v| u64::try_from(v).is_ok())
                    {
                        c.out
                            .push(format!("{at}: source_mux_id {id:?} is not a u64"));
                    }
                    if operations.is_empty() {
                        c.out
                            .push(format!("{at}: a transaction needs an operation"));
                    }
                    c.values(&at, operations.iter().chain(memo).chain(cond));
                    if let (Some(l), Some(_)) = (label, memo) {
                        c.with_memo.insert(l.clone());
                    }
                    c.define(
                        &at,
                        i,
                        label.as_ref(),
                        LabelKind::Submitted {
                            rejected: expect.outcome == Outcome::Rejected,
                        },
                    );
                }
                Step::Close { label, order } => {
                    let mut seen = BTreeSet::new();
                    for l in order {
                        if !seen.insert(l) {
                            c.out.push(format!("{at}: order names {l} twice"));
                            continue;
                        }
                        match c.labels.get(l).copied() {
                            Some((j, LabelKind::Submitted { rejected: false }))
                                if c.last_close.is_none_or(|lc| lc < j) =>
                            {
                                if c.with_memo.contains(l) {
                                    c.out.push(format!(
                                        "{at}: {l} is in the order, so it cannot carry a memo; the interpreter sets its memo id"
                                    ));
                                }
                            }
                            Some((_, LabelKind::Submitted { rejected: false })) => {
                                c.out.push(format!(
                                    "{at}: {l} was queued before the previous close, so this close does not apply it"
                                ))
                            }
                            Some(_) => c.out.push(format!(
                                "{at}: {l} is not a transaction step this close applies"
                            )),
                            None => c
                                .out
                                .push(format!("{at}: no earlier step is labelled {l}")),
                        }
                    }
                    c.define(&at, i, label.as_ref(), LabelKind::Closed);
                    c.last_close = Some(i);
                }
                Step::Upgrade {
                    label,
                    protocol_version,
                    base_fee,
                    base_reserve,
                    max_tx_set_size,
                    max_soroban_tx_set_size,
                    flags,
                    config,
                } => {
                    if protocol_version.is_none()
                        && base_fee.is_none()
                        && base_reserve.is_none()
                        && max_tx_set_size.is_none()
                        && max_soroban_tx_set_size.is_none()
                        && flags.is_none()
                        && config.is_empty()
                    {
                        c.out.push(format!("{at}: the upgrade arms nothing"));
                    }
                    c.values(&at, config);
                    c.define(&at, i, label.as_ref(), LabelKind::Closed);
                    c.last_close = Some(i);
                }
                Step::Upload {
                    label,
                    source,
                    wasm,
                    sha256,
                } => {
                    c.role(&at, source, false);
                    if wasm.is_empty() || wasm.starts_with('/') || wasm.contains("..") {
                        c.out.push(format!(
                            "{at}: wasm {wasm:?} must be a path inside the repository"
                        ));
                    }
                    if !is_hex(sha256, 64) {
                        c.out
                            .push(format!("{at}: sha256 {sha256:?} is not 64 hex digits"));
                    }
                    c.define(&at, i, Some(label), LabelKind::Upload);
                }
                Step::Deploy {
                    label,
                    source,
                    code,
                    salt,
                    constructor_args,
                } => {
                    c.role(&at, source, false);
                    c.want(&at, code, |k| k == LabelKind::Upload, "an upload step");
                    if !is_hex(salt, 64) {
                        c.out
                            .push(format!("{at}: salt {salt:?} is not 32 bytes of hex"));
                    }
                    c.values(&at, constructor_args);
                    c.define(&at, i, Some(label), LabelKind::Deploy);
                }
                Step::Invoke {
                    label,
                    source,
                    contract,
                    function,
                    args,
                    auth,
                    resources,
                    simulate,
                    footprint,
                    expect,
                } => {
                    c.role(&at, source, false);
                    c.auth(&at, auth);
                    c.resources(&at, resources.as_ref());
                    c.unsimulated(&at, *simulate, footprint.as_ref(), resources.as_ref(), auth);
                    match SymRef::parse(contract) {
                        Some(_) => c.values(&at, [&Value::String(contract.clone())]),
                        None => {
                            c.want(&at, contract, |k| k == LabelKind::Deploy, "a deploy step");
                        }
                    }
                    if function.is_empty() || function.len() > 32 {
                        c.out.push(format!(
                            "{at}: function name {function:?} is not 1 to 32 characters"
                        ));
                    }
                    c.values(&at, args);
                    c.define(
                        &at,
                        i,
                        label.as_ref(),
                        LabelKind::Submitted {
                            rejected: expect.outcome == Outcome::Rejected,
                        },
                    );
                }
                Step::HostFunction {
                    label,
                    source,
                    function,
                    auth,
                    resources,
                    simulate,
                    footprint,
                    expect,
                } => {
                    c.role(&at, source, false);
                    c.values(&at, [function]);
                    c.auth(&at, auth);
                    c.resources(&at, resources.as_ref());
                    c.unsimulated(&at, *simulate, footprint.as_ref(), resources.as_ref(), auth);
                    c.define(
                        &at,
                        i,
                        label.as_ref(),
                        LabelKind::Submitted {
                            rejected: expect.outcome == Outcome::Rejected,
                        },
                    );
                    // a rejected creation makes no contract to refer to
                    let creates = ["create_contract", "create_contract_v2"]
                        .iter()
                        .any(|k| function.get(k).is_some());
                    if let (Some(l), true, false) =
                        (label, creates, expect.outcome == Outcome::Rejected)
                    {
                        c.creators.insert(l.clone());
                    }
                }
                Step::ExtendTtl {
                    label,
                    source,
                    keys,
                    resources,
                    expect,
                    ..
                }
                | Step::Restore {
                    label,
                    source,
                    keys,
                    resources,
                    expect,
                } => {
                    c.role(&at, source, false);
                    c.resources(&at, resources.as_ref());
                    if keys.is_empty() {
                        c.out
                            .push(format!("{at}: the footprint needs at least one key"));
                    }
                    if let Step::ExtendTtl { extend_to: 0, .. } = step {
                        c.out.push(format!("{at}: extend_to must be above 0"));
                    }
                    c.values(&at, keys);
                    c.define(
                        &at,
                        i,
                        label.as_ref(),
                        LabelKind::Submitted {
                            rejected: expect.outcome == Outcome::Rejected,
                        },
                    );
                }
                Step::Capture {
                    slug,
                    select,
                    notes,
                    ..
                } => {
                    if !is_name(slug) {
                        c.out
                            .push(format!("{at}: slug {slug:?} is not a lowercase name"));
                    } else if !slugs.insert(slug.clone()) {
                        c.out.push(format!("{at}: slug {slug} is captured twice"));
                    }
                    if notes.trim().is_empty() {
                        c.out.push(format!("{at}: a capture needs notes"));
                    }
                    if let Selection::LedgerEntry { key, .. } = select {
                        c.values(&at, [key]);
                    }
                    let last_close = c.last_close;
                    let of = select.of();
                    let Some((j, kind)) = c.want(&at, of, |_| true, "") else {
                        continue;
                    };
                    match (select, kind) {
                        (Selection::Rejected { .. }, LabelKind::Submitted { rejected: true }) => {}
                        (Selection::Rejected { .. }, _) => c.out.push(format!(
                            "{at}: {of} is not a step expected to be rejected"
                        )),
                        (_, LabelKind::Submitted { rejected: true }) => c.out.push(format!(
                            "{at}: {of} is refused at submission, so only a rejected capture can take it"
                        )),
                        (Selection::Transaction { .. }, LabelKind::Closed) => c.out.push(format!(
                            "{at}: {of} is a close, which has no transaction of its own"
                        )),
                        (_, LabelKind::Closed) => {}
                        (_, _) if last_close.is_none_or(|l| l < j) => c.out.push(format!(
                            "{at}: no ledger has closed since {of}, so there is nothing to capture yet"
                        )),
                        _ => {}
                    }
                }
            }
        }
        out
    }
}
