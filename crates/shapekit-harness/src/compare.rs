//! Comparing what a target exported for a transaction with what the oracle says the
//! transaction did, and turning the differences into a verdict per corpus entry.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use shapekit_oracle::vocab::{ChangeScope, EventCoverage, TxFacts};

use crate::protocol::{ChangeClaim, Claims, TxRecord, Unmapped};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    Pass,
    Dropped,
    Mishandled,
    NotEvaluated,
    OutOfScope,
}

impl Verdict {
    pub fn name(self) -> &'static str {
        match self {
            Verdict::Pass => "pass",
            Verdict::Dropped => "dropped",
            Verdict::Mishandled => "mishandled",
            Verdict::NotEvaluated => "not-evaluated",
            Verdict::OutOfScope => "out-of-scope",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [
            Verdict::Pass,
            Verdict::Dropped,
            Verdict::Mishandled,
            Verdict::NotEvaluated,
            Verdict::OutOfScope,
        ]
        .into_iter()
        .find(|v| v.name() == s)
    }
}

/// Why an entry is `mishandled`, in precedence order: an earlier reason wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    Halted,
    Crashed,
    Oom,
    Timeout,
    UndecodableAddress,
    WrongValue,
}

impl Reason {
    pub fn name(self) -> &'static str {
        match self {
            Reason::Halted => "halted",
            Reason::Crashed => "crashed",
            Reason::Oom => "oom",
            Reason::Timeout => "timeout",
            Reason::UndecodableAddress => "undecodable-address",
            Reason::WrongValue => "wrong-value",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [
            Reason::Halted,
            Reason::Crashed,
            Reason::Oom,
            Reason::Timeout,
            Reason::UndecodableAddress,
            Reason::WrongValue,
        ]
        .into_iter()
        .find(|r| r.name() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProblemKind {
    /// An in-scope fact with nothing in the export.
    Dropped,
    /// An in-scope fact exported with other values.
    WrongValue,
    /// Exported, but the meta does not show it.
    Extra,
    /// Stored address bytes that decode to no strkey.
    UndecodableAddress,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Problem {
    pub kind: ProblemKind,
    pub tx: String,
    /// `participant` or a vocabulary kind.
    pub fact: String,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub got: Option<Value>,
}

/// The comparison of one transaction.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TxComparison {
    pub tx: String,
    /// Oracle facts inside the claims: participants plus changes.
    pub in_scope: usize,
    pub problems: Vec<Problem>,
    pub unmapped: Vec<Unmapped>,
    /// Protocol oddities in the export that do not change the verdict, for triage.
    pub notes: Vec<String>,
    /// Claimed facts the meta of this transaction cannot show, so neither the oracle's nor
    /// the export's side of them was compared (see [`uncomparable_claims`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_comparable: Vec<String>,
}

/// The fields that identify one change of a kind, so an exported change with other values
/// can be paired with the expected one. Documented in `docs/adapter-interface.md`.
pub fn key_fields(kind: &str) -> &'static [&'static str] {
    match kind {
        "account-created"
        | "account-removed"
        | "thresholds"
        | "account-flags"
        | "home-domain"
        | "inflation-destination"
        | "sequence" => &["account"],
        "balance-change" => &["holder", "asset", "held_in"],
        "trustline-created" | "trustline-removed" | "trustline-flags" | "trustline-limit" => {
            &["account", "asset"]
        }
        "signer-added" | "signer-removed" | "signer-weight" => &["account", "signer"],
        "data-entry" => &["account", "name_hex"],
        "sponsorship" => &["target", "signer"],
        "claimable-balance-created"
        | "claimable-balance-claimed"
        | "claimable-balance-clawed-back" => &["balance_id"],
        "pool-shares" => &["account", "pool"],
        "liquidity-pool" => &["pool"],
        "offer" => &["offer_id"],
        "contract-deployed" => &["contract"],
        "contract-code" => &["hash"],
        "contract-data" => &["contract", "key_xdr"],
        "ttl" => &["key_hash"],
        "config-setting" => &["setting"],
        "trade" => &["counterparty", "offer_id"],
        "contract-event" => &["contract", "stage"],
        "token-event" => &["event", "contract", "from", "to"],
        _ => &[],
    }
}

fn field<'a>(m: &'a Map<String, Value>, name: &str) -> &'a Value {
    m.get(name).unwrap_or(&Value::Null)
}

fn satisfies(m: &Map<String, Value>, claim: &ChangeClaim, only_present: bool) -> bool {
    claim.filter.iter().all(|(name, allowed)| {
        if only_present && !m.contains_key(name) {
            return true;
        }
        allowed.contains(field(m, name))
    })
}

fn project(m: &Map<String, Value>, fields: &[String]) -> Value {
    Value::Object(
        fields
            .iter()
            .map(|f| (f.clone(), field(m, f).clone()))
            .collect(),
    )
}

fn key_of(m: &Map<String, Value>, kind: &str, claim: &ChangeClaim) -> Vec<Value> {
    key_fields(kind)
        .iter()
        .filter(|k| claim.fields.iter().any(|f| f == *k))
        .map(|k| field(m, k).clone())
        .collect()
}

/// Kinds the oracle reads only from events.
pub const EVENT_KINDS: &[&str] = &["token-event", "contract-event"];

/// The change scope an event of `stage` lands in, as the oracle assigns it.
fn stage_scope(stage: &str) -> Option<ChangeScope> {
    match stage {
        "before-all-txs" => Some(ChangeScope::FeeProcessing),
        "operation" => Some(ChangeScope::Operation),
        "after-tx" => Some(ChangeScope::TxAfter),
        "after-all-txs" => Some(ChangeScope::PostApplyFee),
        _ => None,
    }
}

/// Indexes of the event claims the transaction's meta cannot show. A claim on an event kind
/// is comparable only when the meta's event coverage includes every scope the claim can
/// match: the scopes of the stages its `where` lists, or every event scope when it does not
/// restrict `stage`. Without this, a target that derives transfers from operations looks
/// wrong on a v3 classic transaction, whose meta has no events at all (CAP-67 started
/// classic events in protocol 23), when nothing about the target can be judged there.
pub fn uncomparable_claims(coverage: EventCoverage, claims: &Claims) -> Vec<usize> {
    const ALL: [ChangeScope; 4] = [
        ChangeScope::FeeProcessing,
        ChangeScope::Operation,
        ChangeScope::TxAfter,
        ChangeScope::PostApplyFee,
    ];
    claims
        .changes
        .iter()
        .enumerate()
        .filter(|(_, c)| EVENT_KINDS.contains(&c.kind.as_str()))
        .filter(|(_, c)| {
            let scopes: Vec<ChangeScope> = match c.filter.get("stage") {
                Some(stages) => stages
                    .iter()
                    .filter_map(|v| v.as_str().and_then(stage_scope))
                    .collect(),
                None => ALL.to_vec(),
            };
            scopes.iter().any(|s| !coverage.covers(*s))
        })
        .map(|(i, _)| i)
        .collect()
}

/// The first claim of `kind` whose filter the change satisfies.
fn claim_for(claims: &Claims, kind: &str, m: &Map<String, Value>, exported: bool) -> Option<usize> {
    claims
        .changes
        .iter()
        .position(|c| c.kind == kind && satisfies(m, c, exported))
}

/// Compares one transaction. `record` is what the target exported for its hash, if anything.
/// `vocab` maps each vocabulary kind to its field names (see
/// [`crate::protocol::vocabulary_fields`]).
pub fn compare_tx(
    oracle: &TxFacts,
    record: Option<&TxRecord>,
    claims: &Claims,
    vocab: &BTreeMap<String, Vec<String>>,
) -> TxComparison {
    let tx = oracle.hash.clone();
    let mut out = TxComparison {
        tx: tx.clone(),
        ..TxComparison::default()
    };
    // run.rs refuses oracle files without coverage, so None only reaches here from callers
    // that build facts by hand; those get the old behaviour of comparing everything
    let skipped = oracle
        .events
        .map(|c| uncomparable_claims(c, claims))
        .unwrap_or_default();
    for i in &skipped {
        let c = &claims.changes[*i];
        out.not_comparable.push(format!(
            "{}{}: the meta of {tx} carries {} events, so this claim cannot be checked here",
            c.kind,
            if c.filter.is_empty() {
                String::new()
            } else {
                format!(" where {}", serde_json::to_string(&c.filter).unwrap_or_default())
            },
            match oracle.events {
                Some(EventCoverage::SorobanOperation) => "only the invoked contract's operation",
                _ => "no",
            }
        ));
    }
    let problem = |kind, fact: &str, detail: String, expected, got| Problem {
        kind,
        tx: tx.clone(),
        fact: fact.to_owned(),
        detail,
        expected,
        got,
    };

    // participants
    let mut expected_participants = BTreeMap::new();
    if let Some(pc) = &claims.participants {
        for p in &oracle.participants {
            let kind = serde_json::to_value(p.kind).expect("address kind is plain json");
            if pc
                .address_kinds
                .iter()
                .any(|k| Some(k.as_str()) == kind.as_str())
            {
                let ids: BTreeSet<String> = p.mux_ids.iter().map(|i| i.0.to_string()).collect();
                expected_participants.insert(p.address.clone(), ids);
            }
        }
    }
    out.in_scope += expected_participants.len();

    // oracle changes inside a claim, per claim index
    let mut expected: BTreeMap<usize, Vec<Map<String, Value>>> = BTreeMap::new();
    for c in &oracle.changes {
        let Value::Object(m) = serde_json::to_value(c).expect("a state change is plain json")
        else {
            unreachable!("a state change serialises as an object")
        };
        let kind = field(&m, "kind").as_str().unwrap_or("").to_owned();
        if let Some(i) = claim_for(claims, &kind, &m, false)
            && !skipped.contains(&i)
        {
            expected.entry(i).or_default().push(m);
        }
    }
    out.in_scope += expected.values().map(Vec::len).sum::<usize>();

    let Some(record) = record else {
        for addr in expected_participants.keys() {
            out.problems.push(problem(
                ProblemKind::Dropped,
                "participant",
                format!(
                    "the target stored nothing for transaction {tx}; expected participant {addr}"
                ),
                Some(Value::String(addr.clone())),
                None,
            ));
        }
        for (i, list) in &expected {
            let claim = &claims.changes[*i];
            for m in list {
                out.problems.push(problem(
                    ProblemKind::Dropped,
                    &claim.kind,
                    format!("the target stored nothing for transaction {tx}"),
                    Some(project(m, &claim.fields)),
                    None,
                ));
            }
        }
        return out;
    };
    out.unmapped = record.unmapped.clone();

    // participants as exported
    let mut got_participants: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for p in &record.participants {
        if let Some(raw) = &p.raw
            && let Some(err) = &raw.error
        {
            out.problems.push(problem(
                ProblemKind::UndecodableAddress,
                "participant",
                format!(
                    "{} holds {} which does not decode: {err}",
                    raw.column, raw.hex
                ),
                None,
                Some(Value::String(raw.hex.clone())),
            ));
            continue;
        }
        match &p.address {
            Some(a) => {
                let ids = got_participants.entry(a.clone()).or_default();
                ids.extend(p.mux_ids.iter().map(|i| i.0.to_string()));
            }
            None => out.notes.push(format!(
                "a participant of {tx} has no address and no undecodable raw bytes"
            )),
        }
    }
    if let Some(pc) = &claims.participants {
        for (addr, ids) in &expected_participants {
            match got_participants.get(addr) {
                None => out.problems.push(problem(
                    ProblemKind::Dropped,
                    "participant",
                    format!("participant {addr} is missing"),
                    Some(Value::String(addr.clone())),
                    None,
                )),
                Some(got) if pc.mux_ids && got != ids => out.problems.push(problem(
                    ProblemKind::WrongValue,
                    "participant",
                    format!("participant {addr} has mux ids {got:?}, the ledger shows {ids:?}"),
                    Some(serde_json::json!({"address": addr, "mux_ids": ids})),
                    Some(serde_json::json!({"address": addr, "mux_ids": got})),
                )),
                Some(_) => {}
            }
        }
        for addr in got_participants.keys() {
            if expected_participants.contains_key(addr) {
                continue;
            }
            // an address the meta does not show; say whether the envelope named it
            let declared = oracle
                .declared
                .participants
                .iter()
                .find(|d| &d.address == addr)
                .map(|d| format!(", the envelope names it as {}", d.roles.join(", ")))
                .unwrap_or_default();
            let actual_other_kind = oracle.participants.iter().any(|p| &p.address == addr);
            if actual_other_kind {
                // a participant of a kind the target does not claim is not a wrong value
                out.notes.push(format!(
                    "participant {addr} was exported although its address kind is not claimed"
                ));
                continue;
            }
            out.problems.push(problem(
                ProblemKind::Extra,
                "participant",
                format!("participant {addr} is stored but the meta does not show it{declared}"),
                None,
                Some(Value::String(addr.clone())),
            ));
        }
    }

    // exported changes, per claim index
    let mut got: BTreeMap<usize, Vec<Map<String, Value>>> = BTreeMap::new();
    for c in &record.changes {
        for (name, raw) in &c.raw {
            if let Some(err) = &raw.error {
                out.problems.push(problem(
                    ProblemKind::UndecodableAddress,
                    &c.kind,
                    format!(
                        "{} field {name} ({}) holds {} which does not decode: {err}",
                        c.kind, raw.column, raw.hex
                    ),
                    None,
                    Some(Value::String(raw.hex.clone())),
                ));
            }
        }
        match vocab.get(&c.kind) {
            None => {
                out.notes.push(format!(
                    "exported kind {:?} is not in the vocabulary",
                    c.kind
                ));
                continue;
            }
            Some(known) => {
                for f in c.fields.keys() {
                    if f != "scope" && f != "op_index" && !known.contains(f) {
                        out.notes
                            .push(format!("exported {} has unknown field {f:?}", c.kind));
                    }
                }
            }
        }
        match claim_for(claims, &c.kind, &c.fields, true) {
            Some(i) if skipped.contains(&i) => {}
            Some(i) => got.entry(i).or_default().push(c.fields.clone()),
            None => out.notes.push(format!(
                "exported a {} that no claim covers: {}",
                c.kind,
                Value::Object(c.fields.clone())
            )),
        }
    }

    for (i, claim) in claims.changes.iter().enumerate() {
        let want = expected.remove(&i).unwrap_or_default();
        let have = got.remove(&i).unwrap_or_default();
        compare_claim(claim, want, have, &mut |kind, detail, e, g| {
            out.problems.push(problem(kind, &claim.kind, detail, e, g));
        });
    }
    out
}

fn compare_claim(
    claim: &ChangeClaim,
    want: Vec<Map<String, Value>>,
    have: Vec<Map<String, Value>>,
    report: &mut dyn FnMut(ProblemKind, String, Option<Value>, Option<Value>),
) {
    let mut have: Vec<Option<Map<String, Value>>> = have.into_iter().map(Some).collect();
    let mut unmatched = Vec::new();
    for w in want {
        let pw = project(&w, &claim.fields);
        let hit = have
            .iter()
            .position(|h| h.as_ref().is_some_and(|h| project(h, &claim.fields) == pw));
        match hit {
            Some(j) => have[j] = None,
            None => unmatched.push(w),
        }
    }
    for w in unmatched {
        let key = key_of(&w, &claim.kind, claim);
        let pair = have.iter().position(|h| {
            h.as_ref()
                .is_some_and(|h| key_of(h, &claim.kind, claim) == key)
        });
        let expected = project(&w, &claim.fields);
        match pair {
            Some(j) => {
                let h = have[j].take().expect("paired with a remaining change");
                let got = project(&h, &claim.fields);
                let differs: Vec<&str> = claim
                    .fields
                    .iter()
                    .filter(|f| field(&w, f) != field(&h, f))
                    .map(String::as_str)
                    .collect();
                report(
                    ProblemKind::WrongValue,
                    format!("{} differs in {}", claim.kind, differs.join(", ")),
                    Some(expected),
                    Some(got),
                );
            }
            None => report(
                ProblemKind::Dropped,
                format!("{} is missing", claim.kind),
                Some(expected),
                None,
            ),
        }
    }
    for h in have.into_iter().flatten() {
        report(
            ProblemKind::Extra,
            format!("{} is stored but the meta does not show it", claim.kind),
            None,
            Some(project(&h, &claim.fields)),
        );
    }
}

/// Where the target stopped, if it did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stop {
    /// The first ledger the target did not ingest.
    pub ledger: u32,
    pub reason: Reason,
    pub message: String,
}

/// One entry's result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryVerdict {
    pub verdict: Verdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<Reason>,
    pub detail: String,
    pub problems: Vec<Problem>,
    pub unmapped: Vec<Unmapped>,
    pub notes: Vec<String>,
}

impl EntryVerdict {
    pub fn simple(verdict: Verdict, reason: Option<Reason>, detail: impl Into<String>) -> Self {
        EntryVerdict {
            verdict,
            reason,
            detail: detail.into(),
            problems: Vec::new(),
            unmapped: Vec::new(),
            notes: Vec::new(),
        }
    }
}

/// The verdict of an entry whose transactions the target ingested, from their comparisons.
pub fn verdict_of(comparisons: Vec<TxComparison>) -> EntryVerdict {
    let in_scope: usize = comparisons.iter().map(|c| c.in_scope).sum();
    let mut problems = Vec::new();
    let mut unmapped = Vec::new();
    let mut notes = Vec::new();
    let mut not_comparable = Vec::new();
    for c in comparisons {
        problems.extend(c.problems);
        unmapped.extend(c.unmapped);
        notes.extend(c.notes);
        not_comparable.extend(c.not_comparable);
    }
    let has = |k: ProblemKind| problems.iter().any(|p| p.kind == k);
    let count = |k: ProblemKind| problems.iter().filter(|p| p.kind == k).count();
    let (verdict, reason, detail) = if has(ProblemKind::UndecodableAddress) {
        (
            Verdict::Mishandled,
            Some(Reason::UndecodableAddress),
            format!(
                "{} stored address(es) do not decode",
                count(ProblemKind::UndecodableAddress)
            ),
        )
    } else if has(ProblemKind::WrongValue) || has(ProblemKind::Extra) {
        (
            Verdict::Mishandled,
            Some(Reason::WrongValue),
            format!(
                "{} wrong value(s), {} fact(s) stored that the meta does not show",
                count(ProblemKind::WrongValue),
                count(ProblemKind::Extra)
            ),
        )
    } else if has(ProblemKind::Dropped) {
        (
            Verdict::Dropped,
            None,
            format!(
                "{} of {in_scope} in-scope fact(s) missing",
                count(ProblemKind::Dropped)
            ),
        )
    } else if !not_comparable.is_empty() {
        // a pass would claim the event facts were checked; they were not
        (
            Verdict::OutOfScope,
            None,
            format!(
                "{in_scope} comparable fact(s) match, but {} claim(s) cannot be checked because the meta has no place for those events: {}",
                not_comparable.len(),
                not_comparable.join("; ")
            ),
        )
    } else if in_scope == 0 {
        (
            Verdict::OutOfScope,
            None,
            "no fact of the entry's transactions is inside the target's claims".into(),
        )
    } else {
        (
            Verdict::Pass,
            None,
            format!("{in_scope} in-scope fact(s) match"),
        )
    };
    EntryVerdict {
        verdict,
        reason,
        detail,
        problems,
        unmapped,
        notes,
    }
}

/// The verdict of an entry at `ledger` in a chain the target stopped in.
pub fn stopped_verdict(
    ledger: u32,
    stop: &Stop,
    first_entry_at_stop: Option<&str>,
) -> EntryVerdict {
    if ledger == stop.ledger {
        EntryVerdict::simple(
            Verdict::Mishandled,
            Some(stop.reason),
            format!(
                "ingestion stopped at ledger {} ({}): {}",
                stop.ledger,
                stop.reason.name(),
                stop.message
            ),
        )
    } else {
        let by = first_entry_at_stop
            .map(|e| format!("entry {e}"))
            .unwrap_or_else(|| "a ledger with no corpus entry".into());
        EntryVerdict::simple(
            Verdict::NotEvaluated,
            None,
            format!(
                "the target stopped at ledger {} ({}, {by}) before reaching ledger {ledger}",
                stop.ledger,
                stop.reason.name()
            ),
        )
    }
}
