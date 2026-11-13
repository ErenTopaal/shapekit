//! `interpret`: what each transaction in a ledger changed, read only from the meta.
//!
//! The result is a pure function of the `LedgerCloseMeta` and the network passphrase. No
//! outside state is consulted, so the same ledger always gives the same facts whether it is
//! read alone or in the middle of a chain.

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use sha2::{Digest, Sha256};
use stellar_xdr::{
    AccountEntry, AccountEntryExt, AccountEntryExtensionV1Ext, AccountId, Asset, AssetCode,
    ContractDataDurability, ContractDataEntry, ContractEventType, ContractExecutable,
    FeeBumpTransactionInnerTx, LedgerCloseMeta, LedgerEntry, LedgerEntryChange, LedgerEntryData,
    LedgerKey, Limits, OperationBody, OperationType, ScAddress, ScVal, TransactionEnvelope,
    TransactionExt, TransactionResultResult, TrustLineAsset, WriteXdr,
};

use crate::addr::{self, ParticipantSet};
use crate::declared::declared;
use crate::network::Network;
use crate::state::describe_key;
use crate::view::{EventStage, LedgerView, Scope, TxView, ViewError};
use crate::vocab::*;

#[derive(Debug, thiserror::Error)]
pub enum InterpretError {
    #[error(transparent)]
    View(#[from] ViewError),
    #[error("ledger {seq} tx {tx}: {message}")]
    Tx {
        seq: u32,
        tx: String,
        message: String,
    },
}

/// The facts for every transaction in apply order.
pub fn interpret(lcm: &LedgerCloseMeta, network: &Network) -> Result<Vec<TxFacts>, InterpretError> {
    Ok(interpret_ledger(lcm, network)?.transactions)
}

/// The facts for the whole ledger: transactions, upgrades and evictions.
pub fn interpret_ledger(
    lcm: &LedgerCloseMeta,
    network: &Network,
) -> Result<LedgerFacts, InterpretError> {
    let view = LedgerView::new(lcm, network)?;
    let ctx = Context::build(&view, network);
    let seq = view.seq();
    let mut transactions = Vec::with_capacity(view.txs.len());
    for tx in &view.txs {
        transactions.push(tx_facts(&ctx, seq, tx)?);
    }
    let mut upgrades = Vec::new();
    for (i, up) in view.upgrades.iter().enumerate() {
        let mut changes = Vec::new();
        let up_diffs = diffs(&up.changes).map_err(|message| InterpretError::Tx {
            seq,
            tx: format!("upgrade {i}"),
            message,
        })?;
        for d in up_diffs {
            entry_changes(
                &ctx,
                &d,
                Scope::Upgrade(i as u32),
                None,
                &mut changes,
                &mut ParticipantSet::default(),
            )
            .map_err(|message| InterpretError::Tx {
                seq,
                tx: format!("upgrade {i}"),
                message,
            })?;
        }
        sort_changes(&mut changes);
        upgrades.push(UpgradeFacts {
            upgrade: serde_json::to_value(&up.upgrade).expect("an xdr value is plain json"),
            changes,
        });
    }
    Ok(LedgerFacts {
        ledger_seq: seq,
        ledger_hash: hex::encode(view.hash),
        protocol: view.header.ledger_version,
        close_time: Int::from(view.header.scp_value.close_time.0),
        transactions,
        upgrades,
        evicted: view.evicted.iter().map(describe_key).collect(),
    })
}

/// Ledger-wide lookups the interpreter needs: which contracts are Stellar Asset Contracts
/// (proved by recomputing their id from an asset seen in the ledger) and which key a TTL
/// key hash belongs to.
pub(crate) struct Context {
    sac: BTreeMap<[u8; 32], Asset>,
    ttl_targets: BTreeMap<[u8; 32], LedgerKey>,
}

impl Context {
    pub(crate) fn build(view: &LedgerView<'_>, network: &Network) -> Self {
        let mut assets: BTreeSet<Asset> = BTreeSet::new();
        assets.insert(Asset::Native);
        let mut keys: BTreeSet<LedgerKey> = BTreeSet::new();
        let note_entry = |e: &LedgerEntry,
                          assets: &mut BTreeSet<Asset>,
                          keys: &mut BTreeSet<LedgerKey>| {
            match &e.data {
                LedgerEntryData::Trustline(t) => {
                    if let Some(a) = trustline_plain_asset(&t.asset) {
                        assets.insert(a);
                    }
                }
                LedgerEntryData::Offer(o) => {
                    assets.insert(o.selling.clone());
                    assets.insert(o.buying.clone());
                }
                LedgerEntryData::ClaimableBalance(c) => {
                    assets.insert(c.asset.clone());
                }
                LedgerEntryData::LiquidityPool(p) => {
                    let stellar_xdr::LiquidityPoolEntryBody::LiquidityPoolConstantProduct(cp) =
                        &p.body;
                    assets.insert(cp.params.asset_a.clone());
                    assets.insert(cp.params.asset_b.clone());
                }
                LedgerEntryData::ContractData(_) | LedgerEntryData::ContractCode(_) => {
                    keys.insert(e.to_key());
                }
                _ => {}
            }
        };
        let mut lists: Vec<&[LedgerEntryChange]> = Vec::new();
        for tx in &view.txs {
            for l in tx.all_lists() {
                lists.push(l.changes);
            }
        }
        for up in view.upgrades {
            lists.push(&up.changes);
        }
        for list in lists {
            for c in list {
                match c {
                    LedgerEntryChange::Created(e)
                    | LedgerEntryChange::Updated(e)
                    | LedgerEntryChange::State(e)
                    | LedgerEntryChange::Restored(e) => note_entry(e, &mut assets, &mut keys),
                    LedgerEntryChange::Removed(k) => {
                        if matches!(k, LedgerKey::ContractData(_) | LedgerKey::ContractCode(_)) {
                            keys.insert(k.clone());
                        }
                    }
                }
            }
        }
        for tx in &view.txs {
            for e in tx.events() {
                if let Some(a) = event_asset_topic(e.event).and_then(|s| parse_sep11(&s)) {
                    assets.insert(a);
                }
            }
            envelope_assets_and_keys(tx.envelope, &mut assets, &mut keys);
        }
        for k in view.evicted {
            if matches!(k, LedgerKey::ContractData(_) | LedgerKey::ContractCode(_)) {
                keys.insert(k.clone());
            }
        }
        let sac = assets
            .into_iter()
            .map(|a| (network.asset_contract_id(&a), a))
            .collect();
        let ttl_targets = keys
            .into_iter()
            .map(|k| {
                let h: [u8; 32] =
                    Sha256::digest(k.to_xdr(Limits::none()).expect("a key encodes")).into();
                (h, k)
            })
            .collect();
        Context { sac, ttl_targets }
    }

    fn sac_asset(&self, contract: &ScAddress) -> Option<&Asset> {
        match contract {
            ScAddress::Contract(c) => self.sac.get(&c.0.0),
            _ => None,
        }
    }
}

fn envelope_assets_and_keys(
    env: &TransactionEnvelope,
    assets: &mut BTreeSet<Asset>,
    keys: &mut BTreeSet<LedgerKey>,
) {
    let (ops, ext) = envelope_ops(env);
    for op in ops {
        use OperationBody as B;
        match &op.body {
            B::Payment(o) => {
                assets.insert(o.asset.clone());
            }
            B::Clawback(o) => {
                assets.insert(o.asset.clone());
            }
            B::SetTrustLineFlags(o) => {
                assets.insert(o.asset.clone());
            }
            B::InvokeHostFunction(o) => {
                if let stellar_xdr::HostFunction::CreateContract(a) = &o.host_function
                    && let stellar_xdr::ContractIdPreimage::Asset(asset) = &a.contract_id_preimage
                {
                    assets.insert(asset.clone());
                }
                if let stellar_xdr::HostFunction::CreateContractV2(a) = &o.host_function
                    && let stellar_xdr::ContractIdPreimage::Asset(asset) = &a.contract_id_preimage
                {
                    assets.insert(asset.clone());
                }
            }
            _ => {}
        }
    }
    if let Some(TransactionExt::V1(d)) = ext {
        for k in d
            .resources
            .footprint
            .read_only
            .iter()
            .chain(d.resources.footprint.read_write.iter())
        {
            if matches!(k, LedgerKey::ContractData(_) | LedgerKey::ContractCode(_)) {
                keys.insert(k.clone());
            }
        }
    }
}

pub(crate) fn envelope_ops(
    env: &TransactionEnvelope,
) -> (&[stellar_xdr::Operation], Option<&TransactionExt>) {
    match env {
        TransactionEnvelope::TxV0(e) => (&e.tx.operations, None),
        TransactionEnvelope::Tx(e) => (&e.tx.operations, Some(&e.tx.ext)),
        TransactionEnvelope::TxFeeBump(e) => {
            let FeeBumpTransactionInnerTx::Tx(inner) = &e.tx.inner_tx;
            (&inner.tx.operations, Some(&inner.tx.ext))
        }
    }
}

fn trustline_plain_asset(a: &TrustLineAsset) -> Option<Asset> {
    match a {
        TrustLineAsset::Native => Some(Asset::Native),
        TrustLineAsset::CreditAlphanum4(x) => Some(Asset::CreditAlphanum4(x.clone())),
        TrustLineAsset::CreditAlphanum12(x) => Some(Asset::CreditAlphanum12(x.clone())),
        TrustLineAsset::PoolShare(_) => None,
    }
}

// parses the SEP-11 string a SAC puts in its event topics; only ever used as a candidate
// whose contract id is then recomputed, so a wrong parse cannot mislabel a contract
fn parse_sep11(s: &str) -> Option<Asset> {
    if s == "native" {
        return Some(Asset::Native);
    }
    let (code, issuer) = s.split_once(':')?;
    let issuer = AccountId::from_str(issuer).ok()?;
    match AssetCode::from_str(code).ok()? {
        AssetCode::CreditAlphanum4(c) => Some(Asset::CreditAlphanum4(stellar_xdr::AlphaNum4 {
            asset_code: c,
            issuer,
        })),
        AssetCode::CreditAlphanum12(c) => Some(Asset::CreditAlphanum12(stellar_xdr::AlphaNum12 {
            asset_code: c,
            issuer,
        })),
    }
}

/// One key's movement within a change list: value before (from the key's first change,
/// absent when that is a `CREATED`) and after (from its last).
pub(crate) struct Diff<'a> {
    pub key: LedgerKey,
    pub before: Option<&'a LedgerEntry>,
    pub after: Option<&'a LedgerEntry>,
    pub restored: bool,
    pub removed: bool,
}

/// Reads `changes` key by key in the order given, which must chain (see [`crate::order`]),
/// so a key with two groups gives the right before and after.
pub(crate) fn diffs(changes: &[LedgerEntryChange]) -> Result<Vec<Diff<'_>>, String> {
    let mut out: Vec<Diff<'_>> = Vec::new();
    let mut at: BTreeMap<LedgerKey, usize> = BTreeMap::new();
    for c in crate::order::raw_order(changes)? {
        let key = match c {
            LedgerEntryChange::Created(e)
            | LedgerEntryChange::Updated(e)
            | LedgerEntryChange::State(e)
            | LedgerEntryChange::Restored(e) => e.to_key(),
            LedgerEntryChange::Removed(k) => k.clone(),
        };
        let mut first = false;
        let i = *at.entry(key.clone()).or_insert_with(|| {
            first = true;
            out.push(Diff {
                key,
                before: None,
                after: None,
                restored: false,
                removed: false,
            });
            out.len() - 1
        });
        let d = &mut out[i];
        // only the first sight of a key gives its value before the list: a second group's
        // STATE is the value the first group left, and a leading CREATED means it was absent
        match c {
            LedgerEntryChange::State(e) => {
                if first {
                    d.before = Some(e);
                }
                d.after = Some(e);
            }
            LedgerEntryChange::Restored(e) => {
                if first {
                    d.before = Some(e);
                }
                d.after = Some(e);
                d.restored = true;
            }
            LedgerEntryChange::Created(e) | LedgerEntryChange::Updated(e) => {
                d.after = Some(e);
                d.removed = false;
            }
            LedgerEntryChange::Removed(_) => {
                d.after = None;
                d.removed = true;
            }
        }
    }
    Ok(out)
}

fn tx_facts(ctx: &Context, seq: u32, tx: &TxView<'_>) -> Result<TxFacts, InterpretError> {
    let hash = hex::encode(tx.result.transaction_hash.0);
    let err = |message: String| InterpretError::Tx {
        seq,
        tx: hash.clone(),
        message,
    };
    let mut parts = ParticipantSet::default();
    let mut changes = Vec::new();

    // fee source: the one account fee processing touched
    let fee_diffs = diffs(tx.fee_processing).map_err(err)?;
    let fee_accounts: BTreeSet<String> = fee_diffs
        .iter()
        .filter_map(|d| match &d.key {
            LedgerKey::Account(k) => Some(addr::account(&k.account_id)),
            _ => None,
        })
        .collect();
    let fee_source = match fee_accounts.len() {
        1 => fee_accounts.into_iter().next().expect("one element"),
        n => {
            return Err(err(format!(
                "fee processing touches {n} accounts, expected one"
            )));
        }
    };
    parts.add(fee_source.clone(), AddressKind::Account, None, "fee-source");

    let mut fee_delta: i128 = 0;
    for list in tx.all_lists() {
        let op_index = match list.scope {
            Scope::Operation(i) => Some(i),
            _ => None,
        };
        for d in diffs(list.changes).map_err(err)? {
            if matches!(list.scope, Scope::FeeProcessing | Scope::PostApplyFee)
                && let LedgerKey::Account(k) = &d.key
                && addr::account(&k.account_id) == fee_source
            {
                fee_delta += native_amount(ctx, d.after) - native_amount(ctx, d.before);
            }
            let op_type = op_index.and_then(|i| {
                envelope_ops(tx.envelope)
                    .0
                    .get(i as usize)
                    .map(|op| op.body.discriminant())
            });
            entry_changes(ctx, &d, list.scope, op_type, &mut changes, &mut parts).map_err(err)?;
        }
    }

    for e in tx.events() {
        if e.event.type_ == ContractEventType::Diagnostic {
            continue;
        }
        if let Some(id) = &e.event.contract_id {
            parts.add(
                addr::contract(id),
                AddressKind::Contract,
                None,
                "event-contract",
            );
        }
        let stellar_xdr::ContractEventBody::V0(body) = &e.event.body;
        // the only meta source of participant mux ids. sac and classic events never carry
        // a muxed topic (cap-67 puts the id in to_muxed_id), but any contract may emit one
        for t in body.topics.iter() {
            if let ScVal::Address(a) = t {
                let (s, kind, mux) = addr::sc_address(a);
                parts.add(s, kind, mux, "event-address");
            }
        }
        let (scope, op_index) = match e.stage {
            EventStage::Operation(i) => (ChangeScope::Operation, Some(i)),
            EventStage::BeforeAllTxs => (ChangeScope::FeeProcessing, None),
            EventStage::AfterTx => (ChangeScope::TxAfter, None),
            EventStage::AfterAllTxs => (ChangeScope::PostApplyFee, None),
        };
        let Some(te) = token_event(ctx, e.stage, e.event) else {
            changes.push(StateChange {
                scope,
                op_index,
                change: Change::ContractEvent {
                    stage: stage_name(e.stage).into(),
                    contract: e.event.contract_id.as_ref().map(addr::contract),
                    topics_xdr: body.topics.iter().map(key_xdr).collect(),
                    data_xdr: key_xdr(&body.data),
                },
            });
            continue;
        };
        changes.push(StateChange {
            scope,
            op_index,
            change: Change::TokenEvent(te),
        });
    }

    trades(&tx.result.result.result, &mut changes);
    sort_changes(&mut changes);
    let (successful, result, inner_hash) = result_summary(&tx.result.result.result);
    Ok(TxFacts {
        hash: hash.clone(),
        apply_index: tx.index,
        successful,
        result,
        inner_hash,
        fee: Fee {
            source: fee_source,
            charged: Int::from(tx.result.result.fee_charged),
            balance_delta: Int(fee_delta),
        },
        participants: parts.into_vec(),
        changes,
        declared: declared(tx.envelope),
        events: Some(event_coverage(tx.meta)),
    })
}

fn event_coverage(meta: &stellar_xdr::TransactionMeta) -> EventCoverage {
    match meta {
        stellar_xdr::TransactionMeta::V4(_) => EventCoverage::Unified,
        stellar_xdr::TransactionMeta::V3(m) if m.soroban_meta.is_some() => {
            EventCoverage::SorobanOperation
        }
        _ => EventCoverage::None,
    }
}

// the order of entries inside one change list is not meaningful (core release builds emit
// it from a hash map, see the phase 3 decisions), so entry facts are sorted by content within
// their scope; events and trades keep their order, which is consensus data
fn sort_changes(changes: &mut [StateChange]) {
    let rank = |s: ChangeScope| match s {
        ChangeScope::FeeProcessing => 0,
        ChangeScope::TxBefore => 1,
        ChangeScope::Operation => 2,
        ChangeScope::TxAfter => 3,
        ChangeScope::PostApplyFee => 4,
        ChangeScope::Upgrade => 5,
    };
    let mut keyed: Vec<((u8, u32, bool, String), StateChange)> = changes
        .iter()
        .map(|c| {
            let event = matches!(
                c.change,
                Change::TokenEvent(_) | Change::ContractEvent { .. } | Change::Trade { .. }
            );
            let text = if event {
                String::new()
            } else {
                serde_json::to_string(c).expect("a change is plain json")
            };
            (
                (rank(c.scope), c.op_index.unwrap_or(0), event, text),
                c.clone(),
            )
        })
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    for (slot, (_, c)) in changes.iter_mut().zip(keyed) {
        *slot = c;
    }
}

fn result_summary(r: &TransactionResultResult) -> (bool, String, Option<String>) {
    let ok = matches!(
        r,
        TransactionResultResult::TxSuccess(_) | TransactionResultResult::TxFeeBumpInnerSuccess(_)
    );
    let inner = match r {
        TransactionResultResult::TxFeeBumpInnerSuccess(p)
        | TransactionResultResult::TxFeeBumpInnerFailed(p) => {
            Some(hex::encode(p.transaction_hash.0))
        }
        _ => None,
    };
    (ok, snake(r.name()), inner)
}

fn snake(camel: &str) -> String {
    let mut out = String::new();
    for (i, c) in camel.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn scope_of(scope: Scope) -> (ChangeScope, Option<u32>) {
    match scope {
        Scope::FeeProcessing => (ChangeScope::FeeProcessing, None),
        Scope::TxBefore => (ChangeScope::TxBefore, None),
        Scope::Operation(i) => (ChangeScope::Operation, Some(i)),
        Scope::TxAfter => (ChangeScope::TxAfter, None),
        Scope::PostApplyFee => (ChangeScope::PostApplyFee, None),
        Scope::Upgrade(_) => (ChangeScope::Upgrade, None),
    }
}

/// Native lumens an entry holds, by core's `getAssetBalance` rule: account balance,
/// native claimable balance, native pool reserve, or the `amount` of a native Stellar
/// Asset Contract balance entry.
pub(crate) fn native_amount(ctx: &Context, e: Option<&LedgerEntry>) -> i128 {
    let Some(e) = e else { return 0 };
    holdings(ctx, e)
        .into_iter()
        .filter(|h| h.asset == "native")
        .map(|h| h.amount)
        .sum()
}

struct Holding {
    holder: String,
    asset: String,
    held_in: &'static str,
    amount: i128,
}

fn holdings(ctx: &Context, e: &LedgerEntry) -> Vec<Holding> {
    match &e.data {
        LedgerEntryData::Account(a) => vec![Holding {
            holder: addr::account(&a.account_id),
            asset: "native".into(),
            held_in: "account",
            amount: a.balance.into(),
        }],
        LedgerEntryData::Trustline(t) => match &t.asset {
            TrustLineAsset::PoolShare(_) => vec![],
            other => vec![Holding {
                holder: addr::account(&t.account_id),
                asset: trustline_asset_id(other),
                held_in: "trustline",
                amount: t.balance.into(),
            }],
        },
        LedgerEntryData::ClaimableBalance(c) => vec![Holding {
            holder: addr::balance_id(&c.balance_id),
            asset: asset_id(&c.asset),
            held_in: "claimable-balance",
            amount: c.amount.into(),
        }],
        LedgerEntryData::LiquidityPool(p) => {
            let stellar_xdr::LiquidityPoolEntryBody::LiquidityPoolConstantProduct(cp) = &p.body;
            let holder = addr::pool(&p.liquidity_pool_id);
            vec![
                Holding {
                    holder: holder.clone(),
                    asset: asset_id(&cp.params.asset_a),
                    held_in: "liquidity-pool",
                    amount: cp.reserve_a.into(),
                },
                Holding {
                    holder,
                    asset: asset_id(&cp.params.asset_b),
                    held_in: "liquidity-pool",
                    amount: cp.reserve_b.into(),
                },
            ]
        }
        LedgerEntryData::ContractData(d) => sac_balance(ctx, d).into_iter().collect(),
        _ => vec![],
    }
}

// a Stellar Asset Contract balance entry: key vec[Symbol("Balance"), holder], value a map
// whose first entry is amount: i128 (the layout core's getAssetBalance reads)
fn sac_balance(ctx: &Context, d: &ContractDataEntry) -> Option<Holding> {
    let asset = ctx.sac_asset(&d.contract)?;
    let ScVal::Vec(Some(key)) = &d.key else {
        return None;
    };
    let first = key.0.first()?;
    if !matches!(first, ScVal::Symbol(s) if s.0.as_slice() == b"Balance") {
        return None;
    }
    let holder = match key.0.get(1) {
        Some(ScVal::Address(a)) => addr::sc_address(a).0,
        _ => return None,
    };
    let ScVal::Map(Some(m)) = &d.val else {
        return None;
    };
    let amount_entry = m.0.first()?;
    if !matches!(&amount_entry.key, ScVal::Symbol(s) if s.0.as_slice() == b"amount") {
        return None;
    }
    let ScVal::I128(parts) = &amount_entry.val else {
        return None;
    };
    Some(Holding {
        holder,
        asset: asset_id(asset),
        held_in: "contract",
        amount: i128_of(parts),
    })
}

fn i128_of(p: &stellar_xdr::Int128Parts) -> i128 {
    (i128::from(p.hi) << 64) | i128::from(p.lo)
}

// the change to report: removal, creation or update; a restore that changed nothing more
// reports as a restore
fn action(d: &Diff<'_>) -> Option<Action> {
    if d.before.is_none() && d.after.is_none() {
        // created and removed again within one list: nothing moved
        None
    } else if d.removed {
        Some(Action::Removed)
    } else if d.before.is_none() && d.after.is_some() {
        Some(Action::Created)
    } else if d.before != d.after {
        Some(Action::Updated)
    } else if d.restored {
        Some(Action::Restored)
    } else {
        None
    }
}

fn entry_changes(
    ctx: &Context,
    d: &Diff<'_>,
    scope: Scope,
    op_type: Option<OperationType>,
    out: &mut Vec<StateChange>,
    parts: &mut ParticipantSet,
) -> Result<(), String> {
    let (cs, op_index) = scope_of(scope);
    let mut push = |change: Change| {
        out.push(StateChange {
            scope: cs,
            op_index,
            change,
        })
    };
    let Some(act) = action(d) else { return Ok(()) };
    if let Some((owner, kind)) = d
        .after
        .or(d.before)
        .and_then(addr::entry_owner)
        .or_else(|| addr::key_owner(&d.key))
    {
        parts.add(owner, kind, None, "entry-changed");
    }

    // sponsorship of the entry itself
    let sb = d.before.and_then(addr::sponsor);
    let sa = d.after.and_then(addr::sponsor);
    if sb != sa {
        push(Change::Sponsorship {
            target: describe_key(&d.key),
            signer: None,
            sponsor_before: sb,
            sponsor_after: sa,
        });
    }

    // holdings, keyed by holder and asset so an entry holding two assets (a pool) gives two
    let mut before: BTreeMap<(String, String), (i128, &'static str)> = BTreeMap::new();
    let mut after: BTreeMap<(String, String), (i128, &'static str)> = BTreeMap::new();
    for h in d.before.map(|e| holdings(ctx, e)).unwrap_or_default() {
        before.insert((h.holder, h.asset), (h.amount, h.held_in));
    }
    for h in d.after.map(|e| holdings(ctx, e)).unwrap_or_default() {
        after.insert((h.holder, h.asset), (h.amount, h.held_in));
    }
    let hkeys: BTreeSet<&(String, String)> = before.keys().chain(after.keys()).collect();
    for k in hkeys {
        let (b, held_b) = before.get(k).copied().unwrap_or((0, ""));
        let (a, held_a) = after.get(k).copied().unwrap_or((0, ""));
        if a != b {
            let held_in = if held_a.is_empty() { held_b } else { held_a };
            if held_in == "contract" {
                parts.add(k.0.clone(), holder_kind(&k.0), None, "entry-changed");
            }
            push(Change::BalanceChange {
                holder: k.0.clone(),
                asset: k.1.clone(),
                held_in: held_in.to_string(),
                before: Int(b),
                after: Int(a),
                delta: Int(a - b),
            });
        }
    }

    match (d.before.map(|e| &e.data), d.after.map(|e| &e.data)) {
        (b, a) if matches!(b.or(a), Some(LedgerEntryData::Account(_))) => {
            account_changes(b, a, act, &mut push);
        }
        (b, a) if matches!(b.or(a), Some(LedgerEntryData::Trustline(_))) => {
            let t = match a.or(b) {
                Some(LedgerEntryData::Trustline(t)) => t,
                _ => unreachable!("matched a trustline"),
            };
            let tb = match b {
                Some(LedgerEntryData::Trustline(t)) => Some(t),
                _ => None,
            };
            let ta = match a {
                Some(LedgerEntryData::Trustline(t)) => Some(t),
                _ => None,
            };
            let account = addr::account(&t.account_id);
            let asset = trustline_asset_id(&t.asset);
            if let TrustLineAsset::PoolShare(p) = &t.asset {
                parts.add(
                    addr::pool(p),
                    AddressKind::LiquidityPool,
                    None,
                    "entry-changed",
                );
                let bb = tb.map_or(0, |t| i128::from(t.balance));
                let ab = ta.map_or(0, |t| i128::from(t.balance));
                if bb != ab {
                    push(Change::PoolShares {
                        account: account.clone(),
                        pool: addr::pool(p),
                        before: Int(bb),
                        after: Int(ab),
                        delta: Int(ab - bb),
                    });
                }
            }
            match (tb, ta) {
                (None, Some(n)) if act == Action::Created => push(Change::TrustlineCreated {
                    account,
                    asset,
                    limit: Int::from(n.limit),
                    flags: n.flags,
                }),
                (_, None) => push(Change::TrustlineRemoved { account, asset }),
                (Some(o), Some(n)) => {
                    if o.flags != n.flags {
                        push(Change::TrustlineFlags {
                            account: account.clone(),
                            asset: asset.clone(),
                            before: o.flags,
                            after: n.flags,
                        });
                    }
                    if o.limit != n.limit {
                        push(Change::TrustlineLimit {
                            account,
                            asset,
                            before: Int::from(o.limit),
                            after: Int::from(n.limit),
                        });
                    }
                }
                _ => {}
            }
        }
        (b, a) if matches!(b.or(a), Some(LedgerEntryData::Offer(_))) => {
            let ob = match b {
                Some(LedgerEntryData::Offer(o)) => Some(o),
                _ => None,
            };
            let oa = match a {
                Some(LedgerEntryData::Offer(o)) => Some(o),
                _ => None,
            };
            let o = oa.or(ob).expect("matched an offer");
            push(Change::Offer {
                action: act,
                offer_id: Int::from(o.offer_id),
                seller: addr::account(&o.seller_id),
                selling: asset_id(&o.selling),
                buying: asset_id(&o.buying),
                amount_before: ob.map(|o| Int::from(o.amount)),
                amount_after: oa.map(|o| Int::from(o.amount)),
                price_n: o.price.n,
                price_d: o.price.d,
            });
        }
        (b, a) if matches!(b.or(a), Some(LedgerEntryData::Data(_))) => {
            let db = match b {
                Some(LedgerEntryData::Data(x)) => Some(x),
                _ => None,
            };
            let da = match a {
                Some(LedgerEntryData::Data(x)) => Some(x),
                _ => None,
            };
            let x = da.or(db).expect("matched a data entry");
            push(Change::DataEntry {
                account: addr::account(&x.account_id),
                action: act,
                name_hex: hex::encode(x.data_name.as_slice()),
                value_before_hex: db.map(|x| hex::encode(x.data_value.as_slice())),
                value_after_hex: da.map(|x| hex::encode(x.data_value.as_slice())),
            });
        }
        (b, a) if matches!(b.or(a), Some(LedgerEntryData::ClaimableBalance(_))) => {
            let c = match a.or(b) {
                Some(LedgerEntryData::ClaimableBalance(c)) => c,
                _ => unreachable!("matched a claimable balance"),
            };
            let id = addr::balance_id(&c.balance_id);
            match act {
                Action::Created => push(Change::ClaimableBalanceCreated {
                    balance_id: id,
                    asset: asset_id(&c.asset),
                    amount: Int::from(c.amount),
                    claimants: c
                        .claimants
                        .iter()
                        .map(|cl| {
                            let stellar_xdr::Claimant::ClaimantTypeV0(v) = cl;
                            addr::account(&v.destination)
                        })
                        .collect(),
                }),
                Action::Removed => match op_type {
                    Some(OperationType::ClaimClaimableBalance) => {
                        push(Change::ClaimableBalanceClaimed {
                            balance_id: id,
                            asset: asset_id(&c.asset),
                            amount: Int::from(c.amount),
                        })
                    }
                    Some(OperationType::ClawbackClaimableBalance) => {
                        push(Change::ClaimableBalanceClawedBack {
                            balance_id: id,
                            asset: asset_id(&c.asset),
                            amount: Int::from(c.amount),
                        })
                    }
                    other => {
                        return Err(format!(
                            "claimable balance {id} removed by {other:?}, which core never does"
                        ));
                    }
                },
                Action::Updated | Action::Restored => {}
            }
        }
        (b, a) if matches!(b.or(a), Some(LedgerEntryData::LiquidityPool(_))) => {
            let cp = |x: Option<&LedgerEntryData>| match x {
                Some(LedgerEntryData::LiquidityPool(p)) => {
                    let stellar_xdr::LiquidityPoolEntryBody::LiquidityPoolConstantProduct(cp) =
                        &p.body;
                    Some(cp.clone())
                }
                _ => None,
            };
            let (pb, pa) = (cp(b), cp(a));
            let p = pa.as_ref().or(pb.as_ref()).expect("matched a pool");
            let id = match a.or(b) {
                Some(LedgerEntryData::LiquidityPool(p)) => addr::pool(&p.liquidity_pool_id),
                _ => unreachable!("matched a pool"),
            };
            let v =
                |x: &Option<stellar_xdr::LiquidityPoolEntryConstantProduct>,
                 f: fn(&stellar_xdr::LiquidityPoolEntryConstantProduct) -> i64| {
                    Int::from(x.as_ref().map_or(0, f))
                };
            push(Change::LiquidityPool {
                pool: id,
                action: act,
                asset_a: asset_id(&p.params.asset_a),
                asset_b: asset_id(&p.params.asset_b),
                reserve_a_before: v(&pb, |c| c.reserve_a),
                reserve_a_after: v(&pa, |c| c.reserve_a),
                reserve_b_before: v(&pb, |c| c.reserve_b),
                reserve_b_after: v(&pa, |c| c.reserve_b),
                shares_before: v(&pb, |c| c.total_pool_shares),
                shares_after: v(&pa, |c| c.total_pool_shares),
            });
        }
        _ => {}
    }

    // a restore followed by a write in the same list is two facts: the entry came back,
    // then it changed
    let soroban_restore_then_write = d.restored && act != Action::Restored;
    match &d.key {
        LedgerKey::ContractData(k) => {
            let contract = addr::sc_address(&k.contract).0;
            if act == Action::Created
                && matches!(k.key, ScVal::LedgerKeyContractInstance)
                && let Some(LedgerEntryData::ContractData(cd)) = d.after.map(|e| &e.data)
                && let ScVal::ContractInstance(inst) = &cd.val
            {
                push(Change::ContractDeployed {
                    contract: contract.clone(),
                    executable: executable(&inst.executable),
                });
            }
            if soroban_restore_then_write {
                push(Change::ContractData {
                    contract: contract.clone(),
                    action: Action::Restored,
                    durability: durability(k.durability),
                    key_xdr: key_xdr(&k.key),
                });
            }
            push(Change::ContractData {
                contract,
                action: act,
                durability: durability(k.durability),
                key_xdr: key_xdr(&k.key),
            });
        }
        LedgerKey::ContractCode(k) => {
            if soroban_restore_then_write {
                push(Change::ContractCode {
                    action: Action::Restored,
                    hash: hex::encode(k.hash.0),
                });
            }
            push(Change::ContractCode {
                action: act,
                hash: hex::encode(k.hash.0),
            })
        }
        LedgerKey::Ttl(k) => {
            let lu = |x: Option<&LedgerEntry>| match x.map(|e| &e.data) {
                Some(LedgerEntryData::Ttl(t)) => Some(t.live_until_ledger_seq),
                _ => None,
            };
            let target = ctx.ttl_targets.get(&k.key_hash.0);
            if let Some(LedgerKey::ContractData(cd)) = target {
                let (a, kind, _) = addr::sc_address(&cd.contract);
                parts.add(a, kind, None, "entry-changed");
            }
            if soroban_restore_then_write {
                push(Change::Ttl {
                    action: Action::Restored,
                    key_hash: hex::encode(k.key_hash.0),
                    target: target.map(describe_key),
                    before: None,
                    after: lu(d.before),
                });
            }
            push(Change::Ttl {
                action: act,
                key_hash: hex::encode(k.key_hash.0),
                target: target.map(describe_key),
                before: lu(d.before),
                after: lu(d.after),
            });
        }
        LedgerKey::ConfigSetting(k) => push(Change::ConfigSetting {
            action: act,
            setting: snake(k.config_setting_id.name()),
        }),
        _ => {}
    }
    Ok(())
}

fn durability(d: ContractDataDurability) -> String {
    match d {
        ContractDataDurability::Temporary => "temporary".into(),
        ContractDataDurability::Persistent => "persistent".into(),
    }
}

fn key_xdr(k: &ScVal) -> String {
    k.to_xdr_base64(Limits::none())
        .expect("a decoded value re-encodes")
}

fn holder_kind(address: &str) -> AddressKind {
    match address.as_bytes().first() {
        Some(b'C') => AddressKind::Contract,
        Some(b'L') => AddressKind::LiquidityPool,
        Some(b'B') => AddressKind::ClaimableBalance,
        _ => AddressKind::Account,
    }
}

fn executable(e: &ContractExecutable) -> String {
    match e {
        ContractExecutable::Wasm(h) => format!("wasm:{}", hex::encode(h.0)),
        ContractExecutable::StellarAsset => "stellar-asset".into(),
        ContractExecutable::ExternalRef(r) => format!(
            "external-ref:{}:{}",
            addr::sc_address(&r.executable_owner).0,
            hex::encode(r.tag.0.as_slice())
        ),
    }
}

fn account_changes(
    b: Option<&LedgerEntryData>,
    a: Option<&LedgerEntryData>,
    act: Action,
    push: &mut impl FnMut(Change),
) {
    let ab = match b {
        Some(LedgerEntryData::Account(x)) => Some(x),
        _ => None,
    };
    let aa = match a {
        Some(LedgerEntryData::Account(x)) => Some(x),
        _ => None,
    };
    match (ab, aa) {
        (None, Some(n)) if act == Action::Created => {
            push(Change::AccountCreated {
                account: addr::account(&n.account_id),
                starting_balance: Int::from(n.balance),
            });
            signers_diff(&empty_account(n), n, push);
        }
        (Some(o), None) => push(Change::AccountRemoved {
            account: addr::account(&o.account_id),
            final_balance: Int::from(o.balance),
        }),
        (Some(o), Some(n)) => {
            let account = addr::account(&n.account_id);
            if o.seq_num != n.seq_num {
                push(Change::Sequence {
                    account: account.clone(),
                    before: Int::from(o.seq_num.0),
                    after: Int::from(n.seq_num.0),
                });
            }
            let th = |x: &AccountEntry| Thresholds {
                low: x.thresholds.0[1],
                medium: x.thresholds.0[2],
                high: x.thresholds.0[3],
            };
            if o.thresholds.0[1..] != n.thresholds.0[1..] {
                push(Change::Thresholds {
                    account: account.clone(),
                    before: th(o),
                    after: th(n),
                });
            }
            if o.flags != n.flags {
                push(Change::AccountFlags {
                    account: account.clone(),
                    before: o.flags,
                    after: n.flags,
                });
            }
            if o.home_domain != n.home_domain {
                push(Change::HomeDomain {
                    account: account.clone(),
                    before: String::from_utf8_lossy(o.home_domain.as_slice()).into_owned(),
                    after: String::from_utf8_lossy(n.home_domain.as_slice()).into_owned(),
                });
            }
            if o.inflation_dest != n.inflation_dest {
                push(Change::InflationDestination {
                    account: account.clone(),
                    before: o.inflation_dest.as_ref().map(addr::account),
                    after: n.inflation_dest.as_ref().map(addr::account),
                });
            }
            signers_diff(o, n, push);
        }
        _ => {}
    }
}

fn empty_account(n: &AccountEntry) -> AccountEntry {
    AccountEntry {
        account_id: n.account_id.clone(),
        balance: 0,
        seq_num: n.seq_num.clone(),
        num_sub_entries: 0,
        inflation_dest: None,
        flags: 0,
        home_domain: Default::default(),
        thresholds: stellar_xdr::Thresholds([1, 0, 0, 0]),
        signers: Default::default(),
        ext: AccountEntryExt::V0,
    }
}

// signer set, master weight and signer sponsors; signerSponsoringIDs is parallel to signers
fn signers_diff(o: &AccountEntry, n: &AccountEntry, push: &mut impl FnMut(Change)) {
    let account = addr::account(&n.account_id);
    if o.thresholds.0[0] != n.thresholds.0[0] {
        push(Change::SignerWeight {
            account: account.clone(),
            signer: account.clone(),
            before: o.thresholds.0[0].into(),
            after: n.thresholds.0[0].into(),
        });
    }
    let list = |x: &AccountEntry| -> BTreeMap<String, (u32, Option<String>)> {
        let sponsors = signer_sponsors(x);
        x.signers
            .iter()
            .enumerate()
            .map(|(i, s)| {
                (
                    s.key.to_string(),
                    (s.weight, sponsors.get(i).cloned().flatten()),
                )
            })
            .collect()
    };
    let (lo, ln) = (list(o), list(n));
    for (k, (w, sp)) in &ln {
        match lo.get(k) {
            None => {
                push(Change::SignerAdded {
                    account: account.clone(),
                    signer: k.clone(),
                    weight: *w,
                });
                if sp.is_some() {
                    push(Change::Sponsorship {
                        target: format!("account {account}"),
                        signer: Some(k.clone()),
                        sponsor_before: None,
                        sponsor_after: sp.clone(),
                    });
                }
            }
            Some((ow, osp)) => {
                if ow != w {
                    push(Change::SignerWeight {
                        account: account.clone(),
                        signer: k.clone(),
                        before: *ow,
                        after: *w,
                    });
                }
                if osp != sp {
                    push(Change::Sponsorship {
                        target: format!("account {account}"),
                        signer: Some(k.clone()),
                        sponsor_before: osp.clone(),
                        sponsor_after: sp.clone(),
                    });
                }
            }
        }
    }
    for (k, (w, sp)) in &lo {
        if !ln.contains_key(k) {
            push(Change::SignerRemoved {
                account: account.clone(),
                signer: k.clone(),
                weight: *w,
            });
            if sp.is_some() {
                push(Change::Sponsorship {
                    target: format!("account {account}"),
                    signer: Some(k.clone()),
                    sponsor_before: sp.clone(),
                    sponsor_after: None,
                });
            }
        }
    }
}

fn signer_sponsors(x: &AccountEntry) -> Vec<Option<String>> {
    match &x.ext {
        AccountEntryExt::V1(v1) => match &v1.ext {
            AccountEntryExtensionV1Ext::V2(v2) => v2
                .signer_sponsoring_i_ds
                .iter()
                .map(|s| s.0.as_ref().map(addr::account))
                .collect(),
            AccountEntryExtensionV1Ext::V0 => vec![],
        },
        AccountEntryExt::V0 => vec![],
    }
}

fn stage_name(stage: EventStage) -> &'static str {
    match stage {
        EventStage::BeforeAllTxs => "before-all-txs",
        EventStage::AfterTx => "after-tx",
        EventStage::AfterAllTxs => "after-all-txs",
        EventStage::Operation(_) => "operation",
    }
}

// trades are consensus data in the operation results: every offer or pool an exchange
// crossed, with what changed hands
fn trades(r: &TransactionResultResult, changes: &mut Vec<StateChange>) {
    use stellar_xdr::{
        ClaimAtom, InnerTransactionResultResult as I, ManageBuyOfferResult, ManageSellOfferResult,
        OperationResult, OperationResultTr as T, PathPaymentStrictReceiveResult,
        PathPaymentStrictSendResult,
    };
    let ops: &[OperationResult] = match r {
        TransactionResultResult::TxSuccess(ops) => ops,
        TransactionResultResult::TxFeeBumpInnerSuccess(p) => match &p.result.result {
            I::TxSuccess(ops) => ops,
            _ => return,
        },
        _ => return,
    };
    for (i, op) in ops.iter().enumerate() {
        let OperationResult::OpInner(tr) = op else {
            continue;
        };
        let atoms: &[ClaimAtom] = match tr {
            T::ManageSellOffer(ManageSellOfferResult::Success(s))
            | T::CreatePassiveSellOffer(ManageSellOfferResult::Success(s))
            | T::ManageBuyOffer(ManageBuyOfferResult::Success(s)) => &s.offers_claimed,
            T::PathPaymentStrictReceive(PathPaymentStrictReceiveResult::Success(s)) => &s.offers,
            T::PathPaymentStrictSend(PathPaymentStrictSendResult::Success(s)) => &s.offers,
            _ => continue,
        };
        for a in atoms {
            let (counterparty, offer_id, sa, sn, ba, bn) = match a {
                ClaimAtom::V0(x) => (
                    addr::muxed(&stellar_xdr::MuxedAccount::Ed25519(
                        x.seller_ed25519.clone(),
                    ))
                    .0,
                    Some(x.offer_id),
                    &x.asset_sold,
                    x.amount_sold,
                    &x.asset_bought,
                    x.amount_bought,
                ),
                ClaimAtom::OrderBook(x) => (
                    addr::account(&x.seller_id),
                    Some(x.offer_id),
                    &x.asset_sold,
                    x.amount_sold,
                    &x.asset_bought,
                    x.amount_bought,
                ),
                ClaimAtom::LiquidityPool(x) => (
                    addr::pool(&x.liquidity_pool_id),
                    None,
                    &x.asset_sold,
                    x.amount_sold,
                    &x.asset_bought,
                    x.amount_bought,
                ),
            };
            changes.push(StateChange {
                scope: ChangeScope::Operation,
                op_index: Some(i as u32),
                change: Change::Trade {
                    counterparty,
                    offer_id: offer_id.map(Int::from),
                    sold_asset: asset_id(sa),
                    sold_amount: Int::from(sn),
                    bought_asset: asset_id(ba),
                    bought_amount: Int::from(bn),
                },
            });
        }
    }
}

fn event_asset_topic(e: &stellar_xdr::ContractEvent) -> Option<String> {
    let stellar_xdr::ContractEventBody::V0(body) = &e.body;
    if body.topics.len() < 2 {
        return None;
    }
    match body.topics.last() {
        Some(ScVal::String(s)) => Some(String::from_utf8_lossy(s.0.as_slice()).into_owned()),
        _ => None,
    }
}

/// Reads CAP-67 / SEP-41 token events. Topic layouts differ by protocol (v3 SAC events put
/// the admin in mint, clawback and set_authorized topics; CAP-67 removed it), so the
/// subject is located by position from the end, never by a fixed index. Anything that
/// does not match a known shape is not a token event; its addresses still count as
/// participants.
fn token_event(
    ctx: &Context,
    stage: EventStage,
    e: &stellar_xdr::ContractEvent,
) -> Option<TokenEvent> {
    let stellar_xdr::ContractEventBody::V0(body) = &e.body;
    let name = match body.topics.first()? {
        ScVal::Symbol(s) => std::str::from_utf8(s.0.as_slice()).ok()?.to_string(),
        _ => return None,
    };
    let contract_id = e.contract_id.as_ref()?;
    let asset_topic = event_asset_topic(e);
    let addrs: Vec<String> = body
        .topics
        .iter()
        .skip(1)
        .filter_map(|t| match t {
            ScVal::Address(a) => Some(addr::sc_address(a).0),
            _ => None,
        })
        .collect();
    let contract_addr = ScAddress::Contract(contract_id.clone());
    let asset = ctx.sac_asset(&contract_addr).map(asset_id);
    let mut te = TokenEvent {
        event: name.clone(),
        stage: stage_name(stage).into(),
        contract: addr::contract(contract_id),
        asset,
        asset_topic,
        from: None,
        to: None,
        spender: None,
        amount: None,
        to_muxed_id: None,
        authorized: None,
        live_until_ledger: None,
    };
    let amount_data = |d: &ScVal| -> Option<(i128, Option<ToMuxedId>)> {
        match d {
            ScVal::I128(p) => Some((i128_of(p), None)),
            ScVal::Map(Some(m)) => {
                let mut amount = None;
                let mut mux = None;
                for en in m.0.iter() {
                    match (&en.key, &en.val) {
                        (ScVal::Symbol(k), ScVal::I128(p)) if k.0.as_slice() == b"amount" => {
                            amount = Some(i128_of(p))
                        }
                        (ScVal::Symbol(k), v) if k.0.as_slice() == b"to_muxed_id" => {
                            mux = Some(match v {
                                ScVal::U64(id) => ToMuxedId::U64(Int::from(*id)),
                                ScVal::String(s) => ToMuxedId::String(hex::encode(s.0.as_slice())),
                                ScVal::Bytes(b) => ToMuxedId::Bytes(hex::encode(b.0.as_slice())),
                                _ => return None,
                            })
                        }
                        _ => return None,
                    }
                }
                Some((amount?, mux))
            }
            _ => None,
        }
    };
    match name.as_str() {
        "transfer" => {
            if addrs.len() != 2 {
                return None;
            }
            let (amount, mux) = amount_data(&body.data)?;
            te.from = Some(addrs[0].clone());
            te.to = Some(addrs[1].clone());
            te.amount = Some(Int(amount));
            te.to_muxed_id = mux;
        }
        "mint" => {
            let (amount, mux) = amount_data(&body.data)?;
            te.to = Some(addrs.last()?.clone());
            te.amount = Some(Int(amount));
            te.to_muxed_id = mux;
        }
        "burn" | "clawback" | "fee" => {
            let (amount, _) = amount_data(&body.data)?;
            let from = if name == "burn" {
                addrs.first()?
            } else {
                addrs.last()?
            };
            te.from = Some(from.clone());
            te.amount = Some(Int(amount));
        }
        "set_authorized" => {
            let ScVal::Bool(b) = body.data else {
                return None;
            };
            te.to = Some(addrs.last()?.clone());
            te.authorized = Some(b);
        }
        "approve" => {
            if addrs.len() != 2 {
                return None;
            }
            let ScVal::Vec(Some(v)) = &body.data else {
                return None;
            };
            let (ScVal::I128(p), ScVal::U32(lu)) = (v.0.first()?, v.0.get(1)?) else {
                return None;
            };
            te.from = Some(addrs[0].clone());
            te.spender = Some(addrs[1].clone());
            te.amount = Some(Int(i128_of(p)));
            te.live_until_ledger = Some(*lu);
        }
        _ => return None,
    }
    Some(te)
}

#[cfg(test)]
mod tests {
    use stellar_xdr::{
        LedgerEntryExt, PublicKey, SequenceNumber, Signer, SignerKey, String32, Uint256,
    };

    use super::*;

    fn acct(seq: i64, preauth: bool) -> LedgerEntry {
        let signers: Vec<Signer> = if preauth {
            vec![Signer {
                key: SignerKey::PreAuthTx(Uint256([9; 32])),
                weight: 1,
            }]
        } else {
            vec![]
        };
        LedgerEntry {
            last_modified_ledger_seq: 1,
            data: LedgerEntryData::Account(AccountEntry {
                account_id: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([7; 32]))),
                balance: 10,
                seq_num: SequenceNumber(seq),
                num_sub_entries: u32::from(preauth),
                inflation_dest: None,
                flags: 0,
                home_domain: String32::default(),
                thresholds: stellar_xdr::Thresholds([1, 0, 0, 0]),
                signers: signers.try_into().unwrap(),
                ext: AccountEntryExt::V0,
            }),
            ext: LedgerEntryExt::V0,
        }
    }

    fn kinds(changes: &[LedgerEntryChange]) -> Vec<Change> {
        let ctx = Context {
            sac: BTreeMap::new(),
            ttl_targets: BTreeMap::new(),
        };
        let mut out = Vec::new();
        let mut parts = ParticipantSet::default();
        for d in diffs(changes).unwrap() {
            entry_changes(&ctx, &d, Scope::TxBefore, None, &mut out, &mut parts).unwrap();
        }
        out.into_iter().map(|c| c.change).collect()
    }

    /// A fee bump over its own source holding a pre-auth signer for the fee-bump hash: core
    /// removes the signer in one group, then the inner transaction bumps the sequence.
    #[test]
    fn two_groups_keep_the_first_groups_change() {
        let raw = vec![
            LedgerEntryChange::State(acct(8, true)),
            LedgerEntryChange::Updated(acct(8, false)),
            LedgerEntryChange::State(acct(8, false)),
            LedgerEntryChange::Updated(acct(9, false)),
        ];
        let ds = diffs(&raw).unwrap();
        assert_eq!(ds.len(), 1);
        assert_eq!(ds[0].before, Some(&acct(8, true)));
        assert_eq!(ds[0].after, Some(&acct(9, false)));
        let got = kinds(&raw);
        assert!(
            matches!(got.as_slice(), [Change::Sequence { before, after, .. }, Change::SignerRemoved { weight: 1, .. }]
                if *before == Int::from(8i64) && *after == Int::from(9i64)),
            "{got:?}"
        );
    }

    #[test]
    fn created_then_updated_in_a_second_group_is_a_creation() {
        let raw = vec![
            LedgerEntryChange::Created(acct(8, false)),
            LedgerEntryChange::State(acct(8, false)),
            LedgerEntryChange::Updated(acct(9, false)),
        ];
        let ds = diffs(&raw).unwrap();
        assert_eq!(ds[0].before, None);
        assert_eq!(ds[0].after, Some(&acct(9, false)));
        let got = kinds(&raw);
        assert!(
            matches!(
                got.as_slice(),
                [Change::BalanceChange { .. }, Change::AccountCreated { .. }]
            ),
            "{got:?}"
        );
    }

    #[test]
    fn created_then_removed_reports_nothing() {
        let raw = vec![
            LedgerEntryChange::Created(acct(8, false)),
            LedgerEntryChange::State(acct(8, false)),
            LedgerEntryChange::Removed(acct(8, false).to_key()),
        ];
        assert!(kinds(&raw).is_empty());
    }
}
