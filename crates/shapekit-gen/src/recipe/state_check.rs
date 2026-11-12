//! The oracle's `LedgerState` against core's own answers, while the node is still up.
//!
//! The state is rebuilt from genesis by applying the chain's canonical meta, then every key
//! any change in the chain touched is asked of core's query server (`getledgerentry`) at the
//! head ledger. The answer core gives is derived from the oracle state with the rules of
//! `QueryServer::getLedgerEntry` (v30.0.0): a live Soroban entry whose ttl has passed is
//! `archived` when persistent and `not-found` when temporary, and archived entries carry a
//! live-until of 0.
//!
//! TTL keys are refused by the query server ("TTL keys are not allowed"), so a touched TTL is
//! checked through its owning entry: the owner is queried and core's `liveUntilLedgerSeq`
//! must equal the oracle's ttl value. What that cannot see: a ttl's `lastModifiedLedgerSeq`,
//! and the value of a ttl whose owner is no longer live (core reports 0 then).
//!
//! The two config settings core rewrites at every close without meta
//! ([`written_without_meta`]: the eviction iterator and the state size window) are compared
//! by presence only; their values are not in the meta, so the oracle cannot know them.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use shapekit_core_driver::meta::FramedLcm;
use shapekit_core_driver::node::{CoreNode, EntryState, QueriedEntry};
use shapekit_core_driver::recorder::canonical_frame;
use shapekit_oracle::state::written_without_meta;
use shapekit_oracle::view::LedgerView;
use shapekit_oracle::{LedgerState, Network};
use stellar_xdr::{
    ContractDataDurability, Hash, LedgerEntry, LedgerEntryChange, LedgerEntryData, LedgerKey,
    LedgerKeyTtl, Limits, WriteXdr,
};

/// Keys per `getledgerentry` request; core sets no bound, this keeps a request body small.
const QUERY_BATCH: usize = 200;
/// How long to wait for the query server's snapshot to reach the last closed ledger.
const SNAPSHOT_WAIT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Default)]
pub struct StateCheck {
    pub ledgers_applied: usize,
    /// Non-ttl keys asked of core and compared.
    pub keys_compared: usize,
    /// Touched ttl keys whose value was compared through the owner's live-until.
    pub ttls_compared: usize,
    /// Touched ttl keys of owners that are not live at the head, so core reports 0.
    pub ttls_not_live: usize,
    /// Keys compared by presence only, see [`written_without_meta`].
    pub presence_only: usize,
}

#[derive(Debug, PartialEq)]
struct Expected {
    state: EntryState,
    entry: Option<LedgerEntry>,
    live_until: Option<u32>,
}

fn ttl_key(key: &LedgerKey) -> Result<LedgerKey> {
    let bytes = key
        .to_xdr(Limits::none())
        .context("encoding a ledger key")?;
    Ok(LedgerKey::Ttl(LedgerKeyTtl {
        key_hash: Hash(Sha256::digest(bytes).into()),
    }))
}

fn is_soroban(key: &LedgerKey) -> bool {
    matches!(key, LedgerKey::ContractData(_) | LedgerKey::ContractCode(_))
}

fn change_key(c: &LedgerEntryChange) -> LedgerKey {
    match c {
        LedgerEntryChange::Created(e)
        | LedgerEntryChange::Updated(e)
        | LedgerEntryChange::State(e)
        | LedgerEntryChange::Restored(e) => e.to_key(),
        LedgerEntryChange::Removed(k) => k.clone(),
    }
}

fn touched(view: &LedgerView<'_>, out: &mut BTreeSet<LedgerKey>) {
    for tx in &view.txs {
        for list in tx.all_lists() {
            out.extend(list.changes.iter().map(change_key));
        }
    }
    for up in view.upgrades {
        out.extend(up.changes.iter().map(change_key));
    }
    out.extend(view.evicted.iter().cloned());
}

/// What core's query server answers for `key` if the oracle state is right.
fn expected(state: &LedgerState, key: &LedgerKey, head: u32) -> Result<Expected> {
    if let Some(e) = state.get(key) {
        if !is_soroban(key) {
            return Ok(Expected {
                state: EntryState::Live,
                entry: Some(e.clone()),
                live_until: None,
            });
        }
        let ttl = state.get(&ttl_key(key)?);
        let Some(LedgerEntry {
            data: LedgerEntryData::Ttl(ttl),
            ..
        }) = ttl
        else {
            bail!("the oracle holds a live soroban entry without a ttl: {key:?}");
        };
        if ttl.live_until_ledger_seq >= head {
            return Ok(Expected {
                state: EntryState::Live,
                entry: Some(e.clone()),
                live_until: Some(ttl.live_until_ledger_seq),
            });
        }
        let persistent = match key {
            LedgerKey::ContractData(d) => d.durability == ContractDataDurability::Persistent,
            _ => true,
        };
        return Ok(if persistent {
            Expected {
                state: EntryState::Archived,
                entry: Some(e.clone()),
                live_until: Some(0),
            }
        } else {
            Expected {
                state: EntryState::NotFound,
                entry: None,
                live_until: None,
            }
        });
    }
    if let Some(e) = state.get_archived(key) {
        return Ok(Expected {
            state: EntryState::Archived,
            entry: Some(e.clone()),
            live_until: Some(0),
        });
    }
    Ok(Expected {
        state: EntryState::NotFound,
        entry: None,
        live_until: None,
    })
}

fn query_at(node: &CoreNode, keys: &[LedgerKey], head: u32) -> Result<Vec<QueriedEntry>> {
    let started = Instant::now();
    loop {
        let got = node.ledger_entries(keys)?;
        if got.ledger_seq == head {
            return Ok(got.entries);
        }
        // the query server swaps its snapshot in after the close returns
        ensure!(
            got.ledger_seq < head && started.elapsed() < SNAPSHOT_WAIT,
            "core's query server answers at ledger {}, the chain's head is {head}",
            got.ledger_seq
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Applies `frames` (the whole recorded chain, from ledger 2) to a genesis state under
/// `passphrase` and compares the state at the last frame with what `node` reports. Any
/// difference is an error that names every differing key.
pub fn check_against_core(
    node: &CoreNode,
    frames: &[FramedLcm],
    passphrase: &str,
) -> Result<StateCheck> {
    let network = Network::new(passphrase);
    let mut state = LedgerState::genesis(&network);
    let mut keys = BTreeSet::new();
    let mut report = StateCheck::default();
    for f in frames {
        let canonical = canonical_frame(f)?;
        state
            .apply_ledger(&canonical.meta, &network)
            .with_context(|| format!("oracle state at ledger {}", f.ledger_seq))?;
        let view = LedgerView::new(&canonical.meta, &network)?;
        touched(&view, &mut keys);
        report.ledgers_applied += 1;
    }
    let head = state.head_seq().context("no ledger applied")?;

    // ttls are checked through their owners, so find each touched ttl's owner by hash
    let mut owners: BTreeMap<LedgerKey, LedgerKey> = BTreeMap::new();
    let candidates = keys
        .iter()
        .chain(state.iter().map(|(k, _)| k))
        .filter(|k| is_soroban(k));
    for k in candidates {
        owners.insert(ttl_key(k)?, k.clone());
    }
    let mut query: BTreeSet<LedgerKey> = keys
        .iter()
        .filter(|k| !matches!(k, LedgerKey::Ttl(_)))
        .cloned()
        .collect();
    let mut ttl_owners = Vec::new();
    for k in keys.iter().filter(|k| matches!(k, LedgerKey::Ttl(_))) {
        let Some(owner) = owners.get(k) else {
            bail!("touched ttl {k:?} has no owner among the touched or live entries");
        };
        query.insert(owner.clone());
        ttl_owners.push(owner.clone());
    }
    let query: Vec<LedgerKey> = query.into_iter().collect();

    let mut answers: BTreeMap<LedgerKey, QueriedEntry> = BTreeMap::new();
    for batch in query.chunks(QUERY_BATCH) {
        let got = query_at(node, batch, head)?;
        answers.extend(batch.iter().cloned().zip(got));
    }

    let mut diffs = Vec::new();
    for key in &query {
        let want = expected(&state, key, head)?;
        let got = &answers[key];
        let same = if written_without_meta(key) {
            report.presence_only += 1;
            want.state == got.state
        } else {
            want.state == got.state
                && want.entry == got.entry
                && want.live_until == got.live_until_ledger_seq
        };
        if !same {
            diffs.push(format!(
                "{key:?}: oracle {:?} {:?} live-until {:?}, core {:?} {:?} live-until {:?}",
                want.state,
                want.entry,
                want.live_until,
                got.state,
                got.entry,
                got.live_until_ledger_seq
            ));
        }
    }
    for owner in &ttl_owners {
        if answers[owner].state == EntryState::Live {
            report.ttls_compared += 1;
        } else {
            report.ttls_not_live += 1;
        }
    }
    report.keys_compared = query.len();
    if !diffs.is_empty() {
        bail!(
            "oracle state at ledger {head} differs from core on {} of {} keys:\n{}",
            diffs.len(),
            query.len(),
            diffs.join("\n")
        );
    }
    Ok(report)
}
