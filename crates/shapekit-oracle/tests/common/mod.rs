//! Reading the committed corpus chains back through the oracle, for the per-family known
//! answers and the network comparison.
//!
//! Every chain is applied from core's genesis state with every change checked, and every
//! ledger passes the conservation check against the previous header, before a test sees
//! its facts. Accounts are re-derived the way the recipes derive them
//! (`shapekit_core_driver::keys::derive_key(recipe id, role)`), so an expected address never
//! comes from the oracle's own output.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use shapekit_manifest::Entry;
use shapekit_oracle::conservation::check_ledger;
use shapekit_oracle::*;
use stellar_xdr::LedgerHeader;

pub const TESTNET: &str = "Test SDF Network ; September 2015";
pub const MAINNET: &str = "Public Global Stellar Network ; September 2015";
pub const NETWORKS: [&str; 2] = ["testnet", "mainnet"];

pub fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
}

pub fn passphrase(network: &str) -> &'static str {
    match network {
        "testnet" => TESTNET,
        "mainnet" => MAINNET,
        other => panic!("no passphrase for {other}"),
    }
}

/// Every ledger of one chain variant, interpreted.
pub struct ChainFacts {
    pub network: Network,
    pub ledgers: BTreeMap<u32, LedgerFacts>,
}

type Cache = Mutex<BTreeMap<(String, String), Arc<ChainFacts>>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Applies `corpus/chains/<chain>/<network>/ledgers` from genesis, checks conservation of
/// every ledger and interprets it.
pub fn chain(chain: &str, network: &str) -> Arc<ChainFacts> {
    let k = (chain.to_string(), network.to_string());
    if let Some(c) = cache().lock().unwrap().get(&k) {
        return c.clone();
    }
    let net = Network::new(passphrase(network));
    let path = corpus()
        .join("chains")
        .join(chain)
        .join(network)
        .join(shapekit_pack::chain::LEDGERS_DIR);
    let lake =
        shapekit_pack::Lake::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut state = LedgerState::genesis(&net);
    let mut prev: Option<LedgerHeader> = None;
    let mut ledgers = BTreeMap::new();
    for raw in lake.ledgers() {
        let raw = raw.unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let lcm =
            decode_lcm(&raw.xdr).unwrap_or_else(|e| panic!("{chain} {network} {}: {e}", raw.seq));
        state
            .apply_ledger(&lcm, &net)
            .unwrap_or_else(|e| panic!("{chain} {network}: {e}"));
        check_ledger(&lcm, &net, prev.as_ref())
            .unwrap_or_else(|e| panic!("{chain} {network} ledger {}: {e}", raw.seq));
        prev = state.header().cloned();
        let facts = interpret_ledger(&lcm, &net)
            .unwrap_or_else(|e| panic!("{chain} {network} ledger {}: {e}", raw.seq));
        ledgers.insert(raw.seq, facts);
    }
    let c = Arc::new(ChainFacts {
        network: net,
        ledgers,
    });
    cache().lock().unwrap().insert(k, c.clone());
    c
}

/// Every chain id committed under `corpus/chains`.
pub fn chain_ids() -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(corpus().join("chains"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

pub fn entry(family: &str, id: &str) -> Entry {
    let path = corpus()
        .join("entries")
        .join(family)
        .join(id)
        .join("entry.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The ledger a chain entry was cut from, interpreted from the chain under `network`.
pub fn entry_ledger(family: &str, id: &str, network: &str) -> LedgerFacts {
    let e = entry(family, id);
    let chain_id = e
        .source
        .chain
        .as_deref()
        .unwrap_or_else(|| panic!("{id} names no chain"));
    let seq = e
        .source
        .ledger_seq
        .unwrap_or_else(|| panic!("{id} names no ledger"));
    chain(chain_id, network).ledgers[&seq].clone()
}

/// The transaction a `-meta` or `-envelope` entry was cut from.
pub fn entry_tx(family: &str, id: &str, network: &str) -> TxFacts {
    let e = entry(family, id);
    let index = e
        .source
        .tx_index
        .unwrap_or_else(|| panic!("{id} names no transaction"));
    let ledger = entry_ledger(family, id, network);
    ledger
        .transactions
        .into_iter()
        .find(|t| t.apply_index == index)
        .unwrap_or_else(|| panic!("{id}: no transaction {index} in its ledger"))
}

/// The only transaction of a ledger-level entry.
pub fn only_tx(family: &str, id: &str, network: &str) -> TxFacts {
    let mut l = entry_ledger(family, id, network);
    assert_eq!(l.transactions.len(), 1, "{id} {network}: one transaction");
    l.transactions.remove(0)
}

/// The account a recipe derives for `role`, as a G strkey.
pub fn account(recipe: &str, role: &str) -> String {
    let key = shapekit_core_driver::keys::derive_key(recipe, role);
    format!(
        "{}",
        stellar_strkey::ed25519::PublicKey(key.verifying_key().to_bytes())
    )
}

pub fn root(network: &str) -> String {
    addr::account(&Network::new(passphrase(network)).root_account())
}

pub fn changes_in(tx: &TxFacts, scope: ChangeScope) -> Vec<&Change> {
    tx.changes
        .iter()
        .filter(|c| c.scope == scope)
        .map(|c| &c.change)
        .collect()
}

pub fn op_changes(tx: &TxFacts) -> Vec<&Change> {
    changes_in(tx, ChangeScope::Operation)
}

pub fn token_events(tx: &TxFacts) -> Vec<&TokenEvent> {
    tx.changes
        .iter()
        .filter_map(|c| match &c.change {
            Change::TokenEvent(t) => Some(t),
            _ => None,
        })
        .collect()
}

pub fn kind(c: &Change) -> String {
    let v = serde_json::to_value(c).unwrap();
    v["kind"].as_str().unwrap().to_string()
}

/// Kinds of the changes outside fee processing, transaction-level sequence bumps and the
/// fee token event: what the transaction did beyond paying for itself.
pub fn effect_kinds(tx: &TxFacts) -> Vec<String> {
    tx.changes
        .iter()
        .filter(|c| {
            !matches!(
                c.scope,
                ChangeScope::FeeProcessing | ChangeScope::PostApplyFee
            )
        })
        .filter(|c| {
            !(c.scope == ChangeScope::TxBefore && matches!(c.change, Change::Sequence { .. }))
        })
        .filter(|c| !matches!(&c.change, Change::TokenEvent(t) if t.event == "fee"))
        .map(|c| kind(&c.change))
        .collect()
}

pub fn participant<'a>(tx: &'a TxFacts, address: &str) -> Option<&'a Participant> {
    tx.participants.iter().find(|p| p.address == address)
}

/// Native balance delta of `account` in the operations of `tx`.
pub fn native_op_delta(tx: &TxFacts, account: &str) -> i128 {
    op_changes(tx)
        .into_iter()
        .filter_map(|c| match c {
            Change::BalanceChange {
                holder,
                asset,
                held_in,
                delta,
                ..
            } if holder == account && asset == "native" && held_in == "account" => Some(delta.0),
            _ => None,
        })
        .sum()
}
