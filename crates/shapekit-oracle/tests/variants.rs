//! The oracle says the same thing about the testnet and the mainnet variant of every entry.
//!
//! Both variants come from the same recipe run under two passphrases, so their facts may
//! differ only in values the network id feeds. Those are masked by unification: walking
//! both oracle files together, each network-dependent value of the testnet file is bound
//! to the value at the same position in the mainnet file, and the binding must be one to
//! one across the whole file (placeholder `#n` for the n-th binding). What may bind:
//!
//! - 64 hex digits: transaction, inner and ledger hashes (the network id is in every
//!   signature payload, and the ledger hash covers the transaction set), TTL key hashes and
//!   config upgrade set hashes (hashes of keys and values that hold contract ids);
//! - `C...` contract ids: `HashIdPreimage::ContractId` starts with the network id;
//! - the genesis root `G...`, whose seed is the network id; it is pre-bound to the other
//!   network's root, so no other account may differ;
//! - `T...` pre-auth signers, which are transaction hashes;
//! - `L...` and `B...` pool and claimable balance ids, which hash assets and operation ids
//!   that can name the root or a contract.
//!
//! Strings that hold such values inside (an asset `CODE:G...`, `pool-share:<hex>`, entry
//! descriptions) are compared token by token. Base64 XDR (`key_xdr`, `data_xdr`,
//! `topics_xdr`) and long hex XDR tokens are compared as bytes after every bound value's raw
//! 32 bytes are replaced by its placeholder, so an embedded contract id is masked and
//! anything else must be byte equal. `network_passphrase` is the variant's identity and is
//! skipped. Arrays are compared position by position; an array the oracle sorts by content
//! (participants, entry changes of one list) may come out in another order when the sort
//! key is a masked id, so if the positional match fails its elements are matched as a set
//! under the same binding, and such arrays are counted. `apply_index` binds one to one as
//! well: core applies transactions in an order derived from their hashes.
//!
//! Soroban fees can be network dependent too: recipes take resources and fees from
//! simulation, which runs the host over network-derived contract ids, and the fee charged
//! for soroban.auth-trees fanout-323 differs by 25 stroops between the variants. In every
//! Soroban transaction (one with a post-apply fee change or a contract-data, contract-code
//! or ttl change) `fee.charged`, `fee.balance_delta` and the amounts of its fee-processing
//! and post-apply-fee changes are masked, and since such a difference carries into every
//! later balance, native account `before` and `after` are masked everywhere while their
//! `delta` outside fee scopes is still compared. Balance continuity itself is checked by
//! applying every chain to a `LedgerState` (tests/families.rs).
//!
//! One exception follows from the apply order. When two transactions of a ledger write the same
//! contract data key, the one applied first creates it and the other updates it, and their
//! refunds differ with the work done, so their facts legitimately differ between the
//! networks (cap63's conflicting pair). Such transactions are taken out of the positional
//! comparison; for them the same fee sources must succeed or fail alike in both variants,
//! and each shared key must see the same multiset of actions.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;
use shapekit_manifest::Entry;
use shapekit_oracle::{Network, addr};

mod common;

#[derive(Clone, Default)]
struct Binding {
    t2m: BTreeMap<String, String>,
    m2t: BTreeMap<String, String>,
    // (field, testnet text, mainnet text), checked once every binding is known
    deferred: Vec<(String, String, String)>,
    permuted: usize,
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_long_hex(s: &str) -> bool {
    s.len() > 64
        && s.len().is_multiple_of(2)
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The atom kind of a network-dependent value, if `s` is one.
fn atom(s: &str) -> Option<char> {
    if is_hex64(s) {
        return Some('h');
    }
    let c = s.chars().next()?;
    if !matches!(c, 'C' | 'G' | 'T' | 'L' | 'B') {
        return None;
    }
    stellar_strkey::Strkey::from_string(s).ok().map(|_| c)
}

/// Raw bytes the value has inside XDR.
fn raw(s: &str) -> Vec<u8> {
    use stellar_strkey::Strkey as K;
    if is_hex64(s) {
        return hex::decode(s).unwrap();
    }
    match K::from_string(s).unwrap() {
        K::PublicKeyEd25519(k) => k.0.to_vec(),
        K::Contract(k) => k.0.to_vec(),
        K::PreAuthTx(k) => k.0.to_vec(),
        K::LiquidityPool(k) => k.0.to_vec(),
        K::ClaimableBalance(stellar_strkey::ClaimableBalance::V0(k)) => k.to_vec(),
        other => panic!("not an atom: {other:?}"),
    }
}

impl Binding {
    fn bind(&mut self, t: &str, m: &str) -> bool {
        let (Some(kt), Some(km)) = (atom(t), atom(m)) else {
            return false;
        };
        // a G may only differ as the root, which is pre-bound
        if kt != km || (kt == 'G' && t != m && self.t2m.get(t).map(String::as_str) != Some(m)) {
            return false;
        }
        match (self.t2m.get(t), self.m2t.get(m)) {
            (Some(x), Some(y)) => x == m && y == t,
            (None, None) => {
                self.t2m.insert(t.to_string(), m.to_string());
                self.m2t.insert(m.to_string(), t.to_string());
                true
            }
            _ => false,
        }
    }

    fn strings(&mut self, field: &str, t: &str, m: &str) -> bool {
        if t == m {
            if atom(t).is_some() {
                return self.bind(t, m);
            }
            return true;
        }
        if matches!(field, "key_xdr" | "data_xdr" | "topics_xdr") {
            self.deferred
                .push((field.to_string(), t.to_string(), m.to_string()));
            return true;
        }
        if self.bind(t, m) {
            return true;
        }
        let split = |s: &str| -> Vec<String> { s.split([':', ' ']).map(str::to_string).collect() };
        let (a, b) = (split(t), split(m));
        if a.len() < 2 || a.len() != b.len() {
            return false;
        }
        for (x, y) in a.iter().zip(&b) {
            if x == y {
                if atom(x).is_some() && !self.bind(x, y) {
                    return false;
                }
                continue;
            }
            if is_long_hex(x) && is_long_hex(y) && x.len() == y.len() {
                self.deferred.push(("hex".into(), x.clone(), y.clone()));
                continue;
            }
            if !self.bind(x, y) {
                return false;
            }
        }
        true
    }

    fn values(&mut self, field: &str, t: &Value, m: &Value) -> bool {
        match (t, m) {
            (Value::Number(a), Value::Number(b)) if field == "apply_index" => {
                let (a, b) = (format!("i{a}"), format!("i{b}"));
                match (self.t2m.get(&a), self.m2t.get(&b)) {
                    (Some(x), Some(y)) => *x == b && *y == a,
                    (None, None) => {
                        self.t2m.insert(a.clone(), b.clone());
                        self.m2t.insert(b, a);
                        true
                    }
                    _ => false,
                }
            }
            (Value::String(a), Value::String(b)) => self.strings(field, a, b),
            (Value::Array(a), Value::Array(b)) => {
                if a.len() != b.len() {
                    return false;
                }
                let mut trial = self.clone();
                if a.iter().zip(b).all(|(x, y)| trial.values(field, x, y)) {
                    *self = trial;
                    return true;
                }
                let mut trial = self.clone();
                let mut used = vec![false; b.len()];
                for x in a {
                    let mut hit = None;
                    for (j, y) in b.iter().enumerate() {
                        if used[j] {
                            continue;
                        }
                        let mut t2 = trial.clone();
                        if t2.values(field, x, y) {
                            hit = Some((j, t2));
                            break;
                        }
                    }
                    let Some((j, t2)) = hit else {
                        return false;
                    };
                    used[j] = true;
                    trial = t2;
                }
                trial.permuted += 1;
                *self = trial;
                true
            }
            (Value::Object(a), Value::Object(b)) => {
                if a.len() != b.len() || a.keys().ne(b.keys()) {
                    return false;
                }
                a.iter()
                    .all(|(k, x)| k == "network_passphrase" || self.values(k, x, &b[k.as_str()]))
            }
            _ => t == m,
        }
    }

    /// Byte comparison of the deferred XDR with every bound value masked.
    fn check_deferred(&self) -> Result<(), String> {
        let mut pairs: Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> = Vec::new();
        for (i, (t, m)) in self.t2m.iter().enumerate() {
            if t == m || t.starts_with('i') && atom(t).is_none() {
                continue;
            }
            let mut mark = vec![0xee; 32];
            mark[..8].copy_from_slice(&(i as u64).to_be_bytes());
            pairs.push((raw(t), raw(m), mark));
        }
        for (field, t, m) in &self.deferred {
            let decode = |s: &str| -> Vec<u8> {
                if field == "hex" {
                    hex::decode(s).unwrap()
                } else {
                    base64_decode(s)
                }
            };
            let (mut a, mut b) = (decode(t), decode(m));
            for (rt, rm, mark) in &pairs {
                replace(&mut a, rt, mark);
                replace(&mut b, rm, mark);
            }
            if a != b {
                return Err(format!("{field}: {t} vs {m} differ beyond masked ids"));
            }
        }
        Ok(())
    }
}

fn replace(buf: &mut [u8], from: &[u8], to: &[u8]) {
    if from.len() != to.len() || buf.len() < from.len() {
        return;
    }
    let mut i = 0;
    while i + from.len() <= buf.len() {
        if &buf[i..i + from.len()] == from {
            buf[i..i + from.len()].copy_from_slice(to);
            i += from.len();
        } else {
            i += 1;
        }
    }
}

fn base64_decode(s: &str) -> Vec<u8> {
    use stellar_xdr::{Limits, ReadXdr, ScVal, WriteXdr};
    // every base64 field the oracle writes is one ScVal
    ScVal::from_xdr_base64(s, Limits::none())
        .unwrap_or_else(|e| panic!("{s}: {e}"))
        .to_xdr(Limits::none())
        .unwrap()
}

const MASK: &str = "#fee-dependent";

fn mask_soroban_fees(file: &mut Value) {
    for l in file["ledgers"].as_array_mut().unwrap() {
        for tx in l["transactions"].as_array_mut().unwrap() {
            let changes = tx["changes"].as_array().unwrap();
            let soroban = changes.iter().any(|c| {
                c["scope"] == "post-apply-fee"
                    || matches!(
                        c["kind"].as_str(),
                        Some("contract-data" | "contract-code" | "ttl")
                    )
            });
            if soroban {
                tx["fee"]["charged"] = MASK.into();
                tx["fee"]["balance_delta"] = MASK.into();
            }
            for c in tx["changes"].as_array_mut().unwrap() {
                let fee_scope = matches!(
                    c["scope"].as_str(),
                    Some("fee-processing" | "post-apply-fee")
                );
                if soroban && fee_scope {
                    for k in ["delta", "amount"] {
                        if c.get(k).is_some() {
                            c[k] = MASK.into();
                        }
                    }
                }
                if c["kind"] == "balance-change"
                    && c["asset"] == "native"
                    && c["held_in"] == "account"
                {
                    c["before"] = MASK.into();
                    c["after"] = MASK.into();
                }
            }
        }
    }
}

fn op_data_writes(tx: &Value) -> Vec<(String, String, String)> {
    tx["changes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] == "contract-data" && c["scope"] == "operation")
        .map(|c| {
            (
                c["contract"].as_str().unwrap().to_string(),
                c["key_xdr"].as_str().unwrap().to_string(),
                c["action"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

/// Takes the transactions that write a contract data key another transaction of the same
/// ledger writes out of both files and checks them by outcome. Keys are compared by
/// position of the contract within the ledger's own shared keys, which is what the masking
/// would do. Returns how many transactions were taken out.
fn split_shared_writes(t: &mut Value, m: &mut Value) -> Result<usize, String> {
    let mut taken = 0;
    let (tl, ml) = (
        t["ledgers"].as_array_mut().unwrap(),
        m["ledgers"].as_array_mut().unwrap(),
    );
    for (lt, lm) in tl.iter_mut().zip(ml.iter_mut()) {
        let shared_sources = |l: &Value| -> Vec<String> {
            let txs = l["transactions"].as_array().unwrap();
            let mut writers: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
            for tx in txs {
                for (c, k, _) in op_data_writes(tx) {
                    writers
                        .entry((c, k))
                        .or_default()
                        .push(tx["fee"]["source"].as_str().unwrap().to_string());
                }
            }
            let mut out: Vec<String> = writers
                .into_values()
                .filter(|w| w.len() > 1)
                .flatten()
                .collect();
            out.sort();
            out.dedup();
            out
        };
        let (st, sm) = (shared_sources(lt), shared_sources(lm));
        if st != sm {
            return Err(format!(
                "transactions writing a shared key differ: {st:?} vs {sm:?}"
            ));
        }
        if st.is_empty() {
            continue;
        }
        let take = |l: &mut Value| -> Vec<Value> {
            let txs = l["transactions"].as_array_mut().unwrap();
            let (out, keep): (Vec<Value>, Vec<Value>) = txs
                .drain(..)
                .partition(|tx| st.iter().any(|s| tx["fee"]["source"] == *s));
            *txs = keep;
            out
        };
        let (gt, gm) = (take(lt), take(lm));
        let outcome = |g: &[Value]| -> (Vec<(String, bool)>, Vec<String>) {
            let mut ok: Vec<(String, bool)> = g
                .iter()
                .map(|tx| {
                    (
                        tx["fee"]["source"].as_str().unwrap().to_string(),
                        tx["successful"].as_bool().unwrap(),
                    )
                })
                .collect();
            ok.sort();
            let mut actions: Vec<String> = g
                .iter()
                .flat_map(op_data_writes)
                .map(|(_, _, a)| a)
                .collect();
            actions.sort();
            (ok, actions)
        };
        if outcome(&gt) != outcome(&gm) {
            return Err(format!(
                "shared-key writers differ in outcome: {:?} vs {:?}",
                outcome(&gt),
                outcome(&gm)
            ));
        }
        taken += gt.len();
    }
    Ok(taken)
}

fn oracle_json(rel: &str) -> Value {
    let p = common::corpus().join(rel);
    let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    serde_json::from_str(&text).unwrap()
}

fn entry_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for item in std::fs::read_dir(dir).unwrap() {
        let p = item.unwrap().path();
        if p.is_dir() {
            entry_files(&p, out);
        } else if p.file_name().is_some_and(|n| n == "entry.json") {
            out.push(p);
        }
    }
}

#[test]
fn oracle_structure_is_identical_between_testnet_and_mainnet() {
    let roots = (
        addr::account(&Network::new(common::TESTNET).root_account()),
        addr::account(&Network::new(common::MAINNET).root_account()),
    );
    let mut files = Vec::new();
    entry_files(&common::corpus().join("entries"), &mut files);
    files.sort();
    let mut compared = 0;
    let mut permuted = 0;
    let mut shared = 0;
    let mut masked: BTreeMap<char, usize> = BTreeMap::new();
    let mut failures = Vec::new();
    for f in files {
        let e: Entry = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        let (Some(t), Some(m)) = (e.variants.testnet.as_ref(), e.variants.mainnet.as_ref()) else {
            continue;
        };
        let (Some(tf), Some(mf)) = (&t.oracle_file, &m.oracle_file) else {
            continue;
        };
        compared += 1;
        let mut b = Binding::default();
        b.t2m.insert(roots.0.clone(), roots.1.clone());
        b.m2t.insert(roots.1.clone(), roots.0.clone());
        let (mut tj, mut mj) = (oracle_json(tf), oracle_json(mf));
        mask_soroban_fees(&mut tj);
        mask_soroban_fees(&mut mj);
        let split_shared = match split_shared_writes(&mut tj, &mut mj) {
            Ok(n) => n,
            Err(msg) => {
                failures.push(format!("{}: {msg}", e.id));
                continue;
            }
        };
        if !b.values("", &tj, &mj) {
            failures.push(format!("{}: structure differs", e.id));
            continue;
        }
        if let Err(msg) = b.check_deferred() {
            failures.push(format!("{}: {msg}", e.id));
            continue;
        }
        permuted += b.permuted;
        for (t, m) in &b.t2m {
            // the root is pre-bound in every entry, so it is not counted
            if t != m && *t != roots.0 {
                *masked.entry(atom(t).unwrap_or('i')).or_default() += 1;
            }
        }
        shared += split_shared;
    }
    println!(
        "{compared} entries compared; masked values by kind {masked:?}; \
         {permuted} arrays matched as sets; {shared} transactions writing a key another \
         transaction of their ledger writes compared by outcome"
    );
    assert!(compared > 0);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
