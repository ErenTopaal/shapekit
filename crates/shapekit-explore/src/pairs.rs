//! Pairs of recipes that start from one chain state, differ in one transaction, and leave
//! core's ledger entries byte for byte the same: what an indexer stores for the two must
//! then agree too, which is the route-equivalence relation of the indexer harness.
//!
//! A pair comes from rewriting one applied transaction of a recipe in a way that names the
//! same thing differently: a destination as a muxed account instead of the account, an
//! operation source written out instead of left to the transaction, a muxed transaction
//! source. Both recipes run from the same prefix; the pair is kept when the entries the last
//! ledger changed, as the ledger leaves them, are equal.

use std::collections::BTreeMap;

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use shapekit_contracts::recipe_ir::{Outcome, Step};
use shapekit_core_driver::canonical::{change_key, change_lists};
use stellar_xdr::{LedgerCloseMeta, LedgerEntryChange, Limits, WriteXdr};

/// The mux id rewrites use.
pub const MUX_ID: u64 = 1234;

/// One way to name the same thing differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rewrite {
    /// Destinations of payments, path payments and merges as muxed accounts.
    MuxedDestination,
    /// Operation sources written out as the transaction's source.
    ExplicitOperationSource,
    /// The transaction source as a muxed account.
    MuxedTransactionSource,
}

impl Rewrite {
    pub const ALL: [Rewrite; 3] = [
        Rewrite::MuxedDestination,
        Rewrite::ExplicitOperationSource,
        Rewrite::MuxedTransactionSource,
    ];

    /// The rewritten step, or `None` when the rewrite changes nothing in it.
    pub fn apply(self, step: &Step) -> Option<Step> {
        let Step::Tx {
            source,
            source_mux_id,
            operations,
            ..
        } = step
        else {
            return None;
        };
        let mut out = step.clone();
        let Step::Tx {
            operations: new_ops,
            source_mux_id: new_mux,
            ..
        } = &mut out
        else {
            unreachable!()
        };
        let changed = match self {
            Rewrite::MuxedTransactionSource => {
                if source_mux_id.is_some() {
                    false
                } else {
                    *new_mux = Some(MUX_ID.to_string());
                    true
                }
            }
            Rewrite::ExplicitOperationSource => {
                let mut any = false;
                for op in new_ops.iter_mut() {
                    if op.get("source_account").is_some_and(Value::is_null) {
                        op["source_account"] = Value::String(format!("@account:{source}"));
                        any = true;
                    }
                }
                any
            }
            Rewrite::MuxedDestination => {
                let mut any = false;
                for op in new_ops.iter_mut() {
                    let Some(body) = op.get_mut("body").and_then(Value::as_object_mut) else {
                        continue;
                    };
                    for (arm, v) in body.iter_mut() {
                        let slot = match arm.as_str() {
                            "account_merge" => Some(v),
                            "payment"
                            | "path_payment_strict_receive"
                            | "path_payment_strict_send" => v.get_mut("destination"),
                            _ => None,
                        };
                        if let Some(slot) = slot
                            && let Some(role) =
                                slot.as_str().and_then(|s| s.strip_prefix("@account:"))
                        {
                            *slot = Value::String(format!("@muxed:{role}:{MUX_ID}"));
                            any = true;
                        }
                    }
                }
                any
            }
        };
        let _ = operations;
        changed.then_some(out)
    }
}

/// Every entry a ledger changed, as the ledger leaves it: its XDR, or `None` when removed.
pub fn post_state(lcm: &LedgerCloseMeta) -> Result<BTreeMap<Vec<u8>, Option<Vec<u8>>>> {
    let mut out = BTreeMap::new();
    for list in change_lists(lcm) {
        for c in list.iter() {
            let key = change_key(c).to_xdr(Limits::none())?;
            match c {
                LedgerEntryChange::State(_) => {}
                LedgerEntryChange::Removed(_) => {
                    out.insert(key, None);
                }
                LedgerEntryChange::Created(e)
                | LedgerEntryChange::Updated(e)
                | LedgerEntryChange::Restored(e) => {
                    out.insert(key, Some(e.to_xdr(Limits::none())?));
                }
            }
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
pub struct EntryRow {
    /// XDR-JSON of the key.
    pub key: Value,
    /// Size of the entry as the first recipe leaves it, 0 when removed.
    pub bytes: usize,
    pub equal: bool,
}

/// Compares two post-states.
pub fn compare(
    a: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    b: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
) -> Result<Vec<EntryRow>> {
    use stellar_xdr::ReadXdr;
    let mut keys: Vec<&Vec<u8>> = a.keys().chain(b.keys()).collect();
    keys.sort();
    keys.dedup();
    let mut rows = Vec::new();
    for k in keys {
        let key = stellar_xdr::LedgerKey::from_xdr(k, Limits::none())?;
        let (x, y) = (a.get(k), b.get(k));
        rows.push(EntryRow {
            key: serde_json::to_value(&key)?,
            bytes: x.and_then(|v| v.as_ref()).map_or(0, Vec::len),
            equal: x == y,
        });
    }
    Ok(rows)
}

/// Whether a step is a transaction core applied successfully.
pub fn applied_ok(step: &Step) -> bool {
    matches!(
        step,
        Step::Tx { expect, .. } if expect.outcome == Outcome::Success
    )
}

/// One candidate pair: a recipe's prefix and two versions of its next transaction.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub from: String,
    pub step: usize,
    pub rewrite: Rewrite,
    pub ir: shapekit_contracts::RecipeIr,
    pub a: Step,
    pub b: Step,
}

/// Every candidate in `recipes` (name and recipe): each successful transaction that a close
/// follows, under each rewrite that changes it.
pub fn candidates(recipes: &[(String, shapekit_contracts::RecipeIr)]) -> Vec<Candidate> {
    let mut out = Vec::new();
    for (name, ir) in recipes {
        for (i, step) in ir.steps.iter().enumerate() {
            if !applied_ok(step) || !matches!(ir.steps.get(i + 1), Some(Step::Close { .. })) {
                continue;
            }
            for r in Rewrite::ALL {
                if let Some(b) = r.apply(step) {
                    out.push(Candidate {
                        from: name.clone(),
                        step: i,
                        rewrite: r,
                        ir: ir.clone(),
                        a: step.clone(),
                        b,
                    });
                }
            }
        }
    }
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct PairReport {
    pub pair: String,
    pub from: String,
    pub step: usize,
    pub rewrite: Rewrite,
    /// Both versions were applied successfully.
    pub both_applied: bool,
    /// Every entry the last ledger changed is the same bytes under both.
    pub records_equal: bool,
    pub entries: Vec<EntryRow>,
}

/// Runs one candidate: both versions from the prefix, compared on the last ledger's
/// entries. Returns the report and the two recipes as they ran.
pub fn run_candidate(
    env: &crate::episode::Env<'_>,
    c: &Candidate,
    pair: &str,
) -> Result<(
    PairReport,
    shapekit_contracts::RecipeIr,
    shapekit_contracts::RecipeIr,
)> {
    use crate::episode::{self, Spec};
    let prefix = &c.ir.steps[..c.step];
    let mut runs = Vec::new();
    for tx in [&c.a, &c.b] {
        let mut steps = prefix.to_vec();
        steps.push(tx.clone());
        steps.push(Step::Close {
            label: None,
            order: Vec::new(),
        });
        let spec = Spec {
            id: c.ir.id.clone(),
            protocol: c.ir.protocol,
            max_protocol: c.ir.protocol,
            node: c.ir.node.clone(),
            prefix: steps,
            actions: 0,
            contracts: false,
            seed: 0,
            setup: true,
        };
        let mut menu = crate::action::OpMenu {
            arms: Vec::new(),
            novelty: Vec::new(),
        };
        runs.push(episode::run(env, &spec, &mut menu)?);
    }
    let last_ok = |ep: &episode::Episode| {
        ep.ir
            .steps
            .iter()
            .rev()
            .find(|s| matches!(s, Step::Tx { .. }))
            .is_some_and(applied_ok)
            && ep.ir.steps.len() >= c.step + 2
    };
    let both_applied = last_ok(&runs[0]) && last_ok(&runs[1]);
    let rows = compare(
        &post_state(&runs[0].last_ledger)?,
        &post_state(&runs[1].last_ledger)?,
    )?;
    let records_equal = both_applied && rows.iter().all(|r| r.equal);
    let b = runs.pop().expect("two runs").ir;
    let a = runs.pop().expect("two runs").ir;
    Ok((
        PairReport {
            pair: pair.to_owned(),
            from: c.from.clone(),
            step: c.step,
            rewrite: c.rewrite,
            both_applied,
            records_equal,
            entries: rows,
        },
        a,
        b,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shapekit_contracts::recipe_ir::TxExpect;

    fn tx(ops: Vec<Value>) -> Step {
        Step::Tx {
            label: None,
            source: "a".into(),
            source_mux_id: None,
            operations: ops,
            signers: Vec::new(),
            memo: None,
            fee: None,
            cond: None,
            fee_bump: None,
            expect: TxExpect::default(),
        }
    }

    #[test]
    fn rewrites_touch_only_what_they_name() {
        let pay = serde_json::json!({"source_account": null, "body": {"payment": {"destination": "@account:b", "asset": "native", "amount": "1"}}});
        let bump = serde_json::json!({"source_account": null, "body": {"bump_sequence": {"bump_to": "1"}}});
        let s = tx(vec![pay, bump.clone()]);
        let Some(Step::Tx { operations, .. }) = Rewrite::MuxedDestination.apply(&s) else {
            panic!("a payment destination is rewritten")
        };
        assert_eq!(
            operations[0]["body"]["payment"]["destination"],
            "@muxed:b:1234"
        );
        assert_eq!(operations[1], bump);
        assert!(
            Rewrite::MuxedDestination
                .apply(&tx(vec![bump.clone()]))
                .is_none()
        );
        let Some(Step::Tx { operations, .. }) = Rewrite::ExplicitOperationSource.apply(&s) else {
            panic!("null sources are written out")
        };
        assert!(
            operations
                .iter()
                .all(|o| o["source_account"] == "@account:a")
        );
        let Some(Step::Tx { source_mux_id, .. }) = Rewrite::MuxedTransactionSource.apply(&s) else {
            panic!("the source is muxed")
        };
        assert_eq!(source_mux_id.as_deref(), Some("1234"));
    }
}
