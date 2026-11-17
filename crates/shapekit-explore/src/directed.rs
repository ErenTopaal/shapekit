//! Setup moves: short sequences of transactions drawn from the world that put the chain in a
//! state ordinary operations rarely build on their own, followed by the operations that act
//! on that state. An issuer that sets its authorization flags before a holder opens a
//! trustline, a funded liquidity pool, an account funded right at its reserve, two
//! transactions in one ledger where the first changes the second's account. Each move names
//! the arms it leads to, and a move is drawn by how many of those the run still misses.
//!
//! Every step of a move is built when the move reaches it, from the ledgers so far, so a
//! claimable balance id or a trustline balance comes from what core wrote. The episode then
//! goes on drawing ordinary steps on top, and the explorer keeps it as a seed as usual.

use std::collections::{BTreeSet, VecDeque};

use serde_json::{Value, json};
use shapekit_contracts::recipe_ir::{FeeBump, Step, TxExpect};
use shapekit_core_driver::config::close_time;

use crate::rng::Rng;
use crate::world::{AssetRef, CODES4, CODES12, World};

/// One step of a move: a step run and closed on its own, or two transactions applied in one
/// ledger in the order given.
#[derive(Debug, Clone)]
pub enum Phase {
    Step(Step),
    Pair(Step, Box<Step>),
}

type Gen = Box<dyn FnMut(&World, &mut Rng) -> Option<Phase>>;

pub struct Move {
    pub kind: Kind,
    plan: VecDeque<Gen>,
}

impl Move {
    /// The next phase, building each from the world as it is now; `None` when the move is
    /// over. A step that cannot be built from this world (its claimable balance never
    /// appeared) is skipped.
    pub fn next(&mut self, world: &World, rng: &mut Rng) -> Option<Phase> {
        while let Some(mut g) = self.plan.pop_front() {
            if let Some(p) = g(world, rng) {
                return Some(p);
            }
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Authorization,
    Clawback,
    Pool,
    Reserve,
    Ordered,
    Predicate,
    Inflation,
    LineFull,
    DestFull,
    Signers,
}

const KINDS: &[Kind] = &[
    Kind::Authorization,
    Kind::Clawback,
    Kind::Pool,
    Kind::Reserve,
    Kind::Ordered,
    Kind::Predicate,
    Kind::Inflation,
    Kind::LineFull,
    Kind::DestFull,
    Kind::Signers,
];

const ORDERED_ARMS: &[&str] = &[
    "TransactionResultResult:tx_bad_seq",
    "TransactionResultResult:tx_bad_auth",
    "TransactionResultResult:tx_bad_auth_extra",
    "TransactionResultResult:tx_bad_min_seq_age_or_gap",
    "InnerTransactionResultResult:tx_bad_seq",
    "InnerTransactionResultResult:tx_bad_auth",
    "InnerTransactionResultResult:tx_bad_auth_extra",
    "InnerTransactionResultResult:tx_bad_min_seq_age_or_gap",
    "InnerTransactionResultResult:tx_no_account",
];

impl Kind {
    /// The arms a move of this kind leads to.
    pub fn targets(self) -> &'static [&'static str] {
        match self {
            Kind::Authorization => &[
                "PaymentResult:not_authorized",
                "PaymentResult:src_not_authorized",
                "PathPaymentStrictSendResult:not_authorized",
                "PathPaymentStrictSendResult:src_not_authorized",
                "PathPaymentStrictReceiveResult:not_authorized",
                "PathPaymentStrictReceiveResult:src_not_authorized",
                "ManageBuyOfferResult:buy_not_authorized",
                "ManageBuyOfferResult:sell_not_authorized",
                "ManageSellOfferResult:buy_not_authorized",
                "ManageSellOfferResult:sell_not_authorized",
                "ClaimClaimableBalanceResult:not_authorized",
                "CreateClaimableBalanceResult:not_authorized",
                "ChangeTrustResult:not_auth_maintain_liabilities",
                "LiquidityPoolDepositResult:not_authorized",
                "SetTrustLineFlagsResult:invalid_state",
            ],
            Kind::Clawback => &[
                "ClawbackResult:success",
                "ClawbackResult:underfunded",
                "ClawbackClaimableBalanceResult:success",
                "ClaimableBalanceEntryExt:v1",
                "ClaimableBalanceEntryExtensionV1Ext:v0",
            ],
            Kind::Pool => &[
                "LiquidityPoolDepositResult:success",
                "LiquidityPoolDepositResult:bad_price",
                "LiquidityPoolDepositResult:line_full",
                "LiquidityPoolDepositResult:pool_full",
                "LiquidityPoolWithdrawResult:success",
                "LiquidityPoolWithdrawResult:line_full",
                "LiquidityPoolWithdrawResult:under_minimum",
                "ClaimAtom:liquidity_pool",
                "ChangeTrustResult:cannot_delete",
                "PathPaymentStrictSendResult:line_full",
                "AllowTrustResult:low_reserve",
                "SetTrustLineFlagsResult:low_reserve",
                "ClaimClaimableBalanceResult:trustline_frozen",
                "LiquidityPoolDepositResult:trustline_frozen",
                "LiquidityPoolWithdrawResult:trustline_frozen",
            ],
            Kind::Reserve => &[
                "ManageSellOfferResult:low_reserve",
                "ManageBuyOfferResult:low_reserve",
                "RevokeSponsorshipResult:low_reserve",
            ],
            Kind::Ordered => ORDERED_ARMS,
            Kind::Predicate => &[
                "ClaimPredicate:and",
                "ClaimPredicate:or",
                "ClaimPredicate:not",
            ],
            Kind::Inflation => &["InflationResult:not_time"],
            Kind::LineFull => &[
                "ClaimClaimableBalanceResult:line_full",
                "PathPaymentStrictSendResult:line_full",
            ],
            Kind::DestFull => &["AccountMergeResult:dest_full"],
            Kind::Signers => &["SetOptionsResult:too_many_signers"],
        }
    }

    /// Whether core runs the operations this move needs at `protocol`.
    fn runs_at(self, protocol: u32, orderable: bool) -> bool {
        match self {
            // clawback and set_trust_line_flags arrived in 17 (CAP-35)
            Kind::Clawback => protocol >= 17,
            // liquidity pools arrived in 18 (CAP-38)
            Kind::Pool => protocol >= 18,
            // claimable balances arrived in 14 (CAP-23)
            Kind::Predicate | Kind::LineFull => protocol >= 14,
            Kind::Ordered => orderable,
            // inflation is refused from 12 on (CAP-26)
            Kind::Inflation => protocol < 12,
            // liabilities, which dest_full counts against, arrived in 10
            Kind::DestFull => protocol >= 10,
            Kind::Authorization | Kind::Reserve | Kind::Signers => true,
        }
    }
}

/// A move whose arms the run still misses, drawn by how many it misses; `None` when no move
/// at this protocol leads to a missing arm. `orderable` says whether the next ledger can
/// take an ordered pair, `freeze` whether a frozen keys upgrade can be armed.
pub fn start(
    world: &World,
    rng: &mut Rng,
    missing: &BTreeSet<String>,
    orderable: bool,
    freeze: bool,
) -> Option<Move> {
    let usable: Vec<Kind> = KINDS
        .iter()
        .copied()
        .filter(|k| k.runs_at(world.protocol, orderable))
        // a fund step pays the interpreter's default fee, which a raised base fee refuses
        .filter(|k| world.base_fee <= 100 || matches!(k, Kind::Predicate | Kind::Inflation))
        .collect();
    let weights: Vec<u64> = usable
        .iter()
        .map(|k| {
            let mut targets: Vec<&str> = k.targets().to_vec();
            if *k == Kind::Pool && !freeze {
                targets.retain(|t| !t.ends_with("trustline_frozen"));
            }
            targets.iter().filter(|t| missing.contains(**t)).count() as u64
        })
        .collect();
    if weights.iter().all(|w| *w == 0) {
        return None;
    }
    let kind = usable[rng.weighted(&weights)];
    let plan = match kind {
        Kind::Authorization => authorization(world, rng)?,
        Kind::Clawback => clawback(world, rng)?,
        Kind::Pool => pool(world, rng, freeze)?,
        Kind::Reserve => reserve(world, rng)?,
        Kind::Ordered => ordered(world, rng, missing)?,
        Kind::Predicate => predicate(world, rng)?,
        Kind::Inflation => inflation(world, rng)?,
        Kind::LineFull => line_full(world, rng)?,
        Kind::DestFull => dest_full(world, rng)?,
        Kind::Signers => signers(world, rng)?,
    };
    Some(Move { kind, plan })
}

// account flags (Stellar-ledger-entries.x)
const AUTH_REQUIRED: u32 = 1;
const AUTH_REVOCABLE: u32 = 2;
const AUTH_IMMUTABLE: u32 = 4;
const AUTH_CLAWBACK: u32 = 8;

const MAX: i64 = i64::MAX;
/// What a fresh holder is funded with: enough for a few trustlines at any reserve the
/// upgrades draw, and fees.
const HOLDER_BALANCE: i64 = 10_000_000_000;

fn acct(role: &str) -> Value {
    Value::String(format!("@account:{role}"))
}

fn op(source: Option<&str>, body: Value) -> Value {
    json!({ "source_account": source.map(acct), "body": body })
}

/// A transaction from `source`; `signers` sign it, the source alone when empty. The fee
/// follows a raised base fee the way [`crate::action::transaction`] does.
fn tx(world: &World, source: &str, ops: Vec<Value>, signers: &[&str]) -> Step {
    let per_op = world.base_fee.max(100);
    let fee = (per_op > 100).then(|| per_op * ops.len() as u32);
    Step::Tx {
        label: None,
        source: source.to_owned(),
        source_mux_id: None,
        operations: ops,
        signers: signers.iter().map(|s| (*s).to_owned()).collect(),
        memo: None,
        fee,
        cond: None,
        fee_bump: None,
        expect: TxExpect::default(),
    }
}

fn fund(roles: &[String], balance: i64) -> Phase {
    Phase::Step(Step::Fund {
        label: None,
        roles: roles.to_vec(),
        balance: balance.to_string(),
    })
}

fn amount(v: i64) -> Value {
    Value::String(v.to_string())
}

fn price(n: i32, d: i32) -> Value {
    json!({ "n": n, "d": d })
}

fn set_options(set_flags: Option<u32>, signer: Option<Value>, thresholds: Option<u32>) -> Value {
    json!({ "set_options": {
        "clear_flags": null,
        "high_threshold": thresholds,
        "home_domain": null,
        "inflation_dest": null,
        "low_threshold": thresholds,
        "master_weight": null,
        "med_threshold": thresholds,
        "set_flags": set_flags,
        "signer": signer,
    }})
}

fn change_trust(line: Value, limit: i64) -> Value {
    json!({ "change_trust": { "line": line, "limit": amount(limit) } })
}

fn pool_share(world: &World, x: &AssetRef, y: &AssetRef) -> Option<(String, Value)> {
    world.pool(x, y).ok().flatten()
}

fn pool_id_json(hex_id: &str) -> Option<Value> {
    let bytes: [u8; 32] = hex::decode(hex_id).ok()?.try_into().ok()?;
    serde_json::to_value(stellar_xdr::PoolId(stellar_xdr::Hash(bytes))).ok()
}

fn payment(to: &str, asset: &AssetRef, v: i64) -> Value {
    json!({ "payment": { "destination": acct(to), "asset": asset.json(), "amount": amount(v) } })
}

fn strict_send(to: &str, send: &AssetRef, v: i64, dest: &AssetRef) -> Value {
    json!({ "path_payment_strict_send": {
        "send_asset": send.json(), "send_amount": amount(v), "destination": acct(to),
        "dest_asset": dest.json(), "dest_min": amount(1), "path": [],
    }})
}

fn strict_receive(to: &str, send: &AssetRef, max: i64, dest: &AssetRef, v: i64) -> Value {
    json!({ "path_payment_strict_receive": {
        "send_asset": send.json(), "send_max": amount(max), "destination": acct(to),
        "dest_asset": dest.json(), "dest_amount": amount(v), "path": [],
    }})
}

fn sell_offer(selling: &AssetRef, buying: &AssetRef, v: i64) -> Value {
    json!({ "manage_sell_offer": {
        "selling": selling.json(), "buying": buying.json(), "amount": amount(v),
        "price": price(1, 1), "offer_id": amount(0),
    }})
}

fn buy_offer(selling: &AssetRef, buying: &AssetRef, v: i64) -> Value {
    json!({ "manage_buy_offer": {
        "selling": selling.json(), "buying": buying.json(), "buy_amount": amount(v),
        "price": price(1, 1), "offer_id": amount(0),
    }})
}

fn create_balance(asset: &AssetRef, v: i64, claimants: Vec<(String, Value)>) -> Value {
    let claimants: Vec<Value> = claimants
        .into_iter()
        .map(|(d, p)| json!({ "claimant_type_v0": { "destination": acct(&d), "predicate": p } }))
        .collect();
    json!({ "create_claimable_balance": {
        "asset": asset.json(), "amount": amount(v), "claimants": claimants,
    }})
}

/// A live claimable balance of `asset` that `claimant` can claim.
fn balance_for(world: &World, asset: &AssetRef, claimant: &str) -> Option<Value> {
    let want = asset.json();
    world
        .balance_assets
        .iter()
        .find(|(_, (a, c))| *a == want && c.iter().any(|r| r == claimant))
        .and_then(|(id, _)| serde_json::from_str(id).ok())
}

/// Authorizes (`flag` 1), keeps liabilities only (2) or deauthorizes (0) `trustor`'s line,
/// with set_trust_line_flags from protocol 17 and allow_trust before; allow_trust reaches
/// the liabilities-only state from 13 (CAP-18).
fn authorize(world: &World, trustor: &str, asset: &AssetRef, flag: u32, allow: bool) -> Value {
    if world.protocol >= 17 && !allow {
        let (set, clear) = match flag {
            1 => (1, 2),
            2 => (2, 1),
            _ => (0, 3),
        };
        json!({ "set_trust_line_flags": {
            "trustor": acct(trustor), "asset": asset.json(), "clear_flags": clear, "set_flags": set,
        }})
    } else {
        let flag = if flag == 2 && world.protocol < 13 {
            0
        } else {
            flag
        };
        let code = asset.code.clone().unwrap_or_default();
        json!({ "allow_trust": { "trustor": acct(trustor), "asset": code, "authorize": flag } })
    }
}

/// An issuer role whose flags can still change, and one of its assets.
fn issuer(world: &World, rng: &mut Rng) -> Option<(String, AssetRef)> {
    let cands: Vec<String> = world
        .issuers
        .iter()
        .filter(|i| world.funded.contains(*i))
        .filter(|i| world.account_flags.get(*i).copied().unwrap_or(0) & AUTH_IMMUTABLE == 0)
        .cloned()
        .collect();
    if cands.is_empty() {
        return None;
    }
    let i = rng.pick(&cands).clone();
    let code = if rng.chance(1, 2) {
        CODES4[0]
    } else {
        CODES12[0]
    };
    let a = AssetRef {
        code: Some(code.to_owned()),
        issuer: Some(i.clone()),
    };
    Some((i, a))
}

fn flags(world: &World, role: &str) -> u32 {
    world.account_flags.get(role).copied().unwrap_or(0)
}

/// A funded role other than `not`.
fn other(world: &World, rng: &mut Rng, not: &[&str]) -> Option<String> {
    let c: Vec<&String> = world
        .funded
        .iter()
        .filter(|r| !not.contains(&r.as_str()) && !r.starts_with('n'))
        .collect();
    (!c.is_empty()).then(|| (*rng.pick(&c)).clone())
}

/// A phase from a closure that needs nothing from the world.
fn fixed(p: Phase) -> Gen {
    let mut p = Some(p);
    Box::new(move |_, _| p.take())
}

/// Sets `want` on the issuer's account when it is not all set yet.
fn issuer_flags(i: &str, want: u32) -> Gen {
    let i = i.to_owned();
    Box::new(move |w, _| {
        (flags(w, &i) & want != want).then(|| {
            Phase::Step(tx(
                w,
                &i,
                vec![op(None, set_options(Some(want), None, None))],
                &[],
            ))
        })
    })
}

/// Authorizes `holders`' lines to `asset` when its issuer requires authorization.
fn authorize_if_required(i: &str, holders: &[String], asset: &AssetRef) -> Gen {
    let (i, holders, asset) = (i.to_owned(), holders.to_vec(), asset.clone());
    Box::new(move |w, _| {
        if flags(w, &i) & AUTH_REQUIRED == 0 {
            return None;
        }
        let ops = holders
            .iter()
            .map(|h| op(None, authorize(w, h, &asset, 1, false)))
            .collect();
        Some(Phase::Step(tx(w, &i, ops, &[])))
    })
}

/// One transaction from `source` with one operation, built when the move reaches it.
fn one(source: &str, body: impl Fn(&World) -> Option<Value> + 'static) -> Gen {
    let source = source.to_owned();
    Box::new(move |w, _| Some(Phase::Step(tx(w, &source, vec![op(None, body(w)?)], &[]))))
}

/// Holders that open trustlines to `assets`, in one transaction from the first.
fn open_lines(holders: &[String], assets: &[(AssetRef, i64)]) -> Gen {
    let holders = holders.to_vec();
    let assets = assets.to_vec();
    Box::new(move |w, _| {
        let mut ops = Vec::new();
        for (n, h) in holders.iter().enumerate() {
            for (a, limit) in &assets {
                let src = (n > 0).then_some(h.as_str());
                ops.push(op(src, change_trust(a.json(), *limit)));
            }
        }
        let signers: Vec<&str> = holders.iter().map(String::as_str).collect();
        Some(Phase::Step(tx(w, &holders[0], ops, &signers)))
    })
}

fn fresh(world: &World, n: usize) -> Vec<String> {
    (0..n).map(|i| world.fresh_role(i)).collect()
}

/// An issuer requires authorization before two new holders trust its asset; payments,
/// offers and claimable balances meet the unauthorized lines, then one line keeps only its
/// liabilities and its holder tries to move the asset on.
fn authorization(world: &World, rng: &mut Rng) -> Option<VecDeque<Gen>> {
    let (i, x) = issuer(world, rng)?;
    let hs = fresh(world, 2);
    let (h1, h2) = (hs[0].clone(), hs[1].clone());
    let native = AssetRef::native();
    let p = world.protocol;
    let allow = p < 17 || rng.chance(1, 3);
    let mut plan: VecDeque<Gen> = VecDeque::new();
    plan.push_back(issuer_flags(&i, AUTH_REQUIRED | AUTH_REVOCABLE));
    plan.push_back(fixed(fund(&hs, HOLDER_BALANCE)));
    plan.push_back(open_lines(&hs, &[(x.clone(), MAX)]));
    // the unauthorized lines
    {
        let (h1c, xc) = (h1.clone(), x.clone());
        plan.push_back(one(&i, move |_| Some(payment(&h1c, &xc, 100))));
    }
    if p >= 12 {
        let (h1c, xc) = (h1.clone(), x.clone());
        plan.push_back(one(&i, move |_| Some(strict_send(&h1c, &xc, 100, &xc))));
        let (h1c, xc) = (h1.clone(), x.clone());
        plan.push_back(one(&i, move |_| {
            Some(strict_receive(&h1c, &xc, 1000, &xc, 100))
        }));
    }
    if p >= 14 {
        let (h1c, xc) = (h1.clone(), x.clone());
        plan.push_back(one(&i, move |_| {
            Some(create_balance(
                &xc,
                100,
                vec![(h1c.clone(), json!("unconditional"))],
            ))
        }));
        let xc = x.clone();
        let h1c = h1.clone();
        plan.push_back(one(&h1, move |w| {
            let id = balance_for(w, &xc, &h1c)?;
            Some(json!({ "claim_claimable_balance": { "balance_id": id } }))
        }));
    }
    for (sell, buy) in [(x.clone(), native.clone()), (native.clone(), x.clone())] {
        let (s, b) = (sell.clone(), buy.clone());
        plan.push_back(one(&h1, move |_| Some(sell_offer(&s, &b, 10))));
        plan.push_back(one(&h1, move |_| Some(buy_offer(&sell, &buy, 10))));
    }
    if p >= 18 {
        let xc = x.clone();
        plan.push_back(one(&h1, move |w| {
            let (_, share) = pool_share(w, &xc, &AssetRef::native())?;
            Some(change_trust(json!({ "pool_share": share }), MAX))
        }));
    }
    // authorized and paid
    {
        let (i2, hs2, x2) = (i.clone(), hs.clone(), x.clone());
        plan.push_back(Box::new(move |w, _| {
            let ops = hs2
                .iter()
                .map(|h| op(None, authorize(w, h, &x2, 1, allow)))
                .collect();
            Some(Phase::Step(tx(w, &i2, ops, &[])))
        }));
        let (i2, hs2, x2) = (i.clone(), hs.clone(), x.clone());
        plan.push_back(Box::new(move |w, _| {
            let ops = hs2
                .iter()
                .map(|h| op(None, payment(h, &x2, 1000)))
                .collect();
            Some(Phase::Step(tx(w, &i2, ops, &[])))
        }));
    }
    if p >= 17 {
        // the line is authorized: adding the liabilities-only flag sets both
        let (h2c, xc) = (h2.clone(), x.clone());
        plan.push_back(one(&i, move |_| {
            Some(json!({ "set_trust_line_flags": {
                "trustor": acct(&h2c), "asset": xc.json(), "clear_flags": 0, "set_flags": 2,
            }}))
        }));
    }
    if p >= 18 {
        let xc = x.clone();
        plan.push_back(one(&h1, move |w| {
            let (_, share) = pool_share(w, &xc, &AssetRef::native())?;
            Some(change_trust(json!({ "pool_share": share }), MAX))
        }));
        let xc = x.clone();
        plan.push_back(one(&h1, move |w| {
            deposit(w, &xc, &AssetRef::native(), 100, (1, 2), (2, 1))
        }));
    }
    // liabilities only (or no authorization below 13): the holder cannot send it on
    {
        let (h1c, xc) = (h1.clone(), x.clone());
        plan.push_back(one(&i, move |w| Some(authorize(w, &h1c, &xc, 2, allow))));
    }
    {
        let (h2c, xc) = (h2.clone(), x.clone());
        plan.push_back(one(&h1, move |_| Some(payment(&h2c, &xc, 10))));
    }
    if p >= 12 {
        let (h2c, xc) = (h2.clone(), x.clone());
        plan.push_back(one(&h1, move |_| Some(strict_send(&h2c, &xc, 10, &xc))));
        let (h2c, xc) = (h2.clone(), x.clone());
        plan.push_back(one(&h1, move |_| {
            Some(strict_receive(&h2c, &xc, 100, &xc, 10))
        }));
    }
    if p >= 14 {
        let (h2c, xc) = (h2.clone(), x.clone());
        plan.push_back(one(&h1, move |_| {
            Some(create_balance(
                &xc,
                10,
                vec![(h2c.clone(), json!("unconditional"))],
            ))
        }));
    }
    if p >= 18 {
        let xc = x.clone();
        plan.push_back(one(&h1, move |w| {
            deposit(w, &xc, &AssetRef::native(), 10, (1, 2), (2, 1))
        }));
    }
    {
        // the line holds a balance now, so offers selling it reach the authorization check
        let xc = x.clone();
        plan.push_back(one(&h1, move |_| {
            Some(sell_offer(&xc, &AssetRef::native(), 5))
        }));
        let xc = x.clone();
        plan.push_back(one(&h1, move |_| {
            Some(buy_offer(&xc, &AssetRef::native(), 5))
        }));
    }
    Some(plan)
}

/// A deposit into the pool of `x` and `y`, `v` of each at most, within the price bounds.
fn deposit(
    world: &World,
    x: &AssetRef,
    y: &AssetRef,
    v: i64,
    min: (i32, i32),
    max: (i32, i32),
) -> Option<Value> {
    let (id, _) = pool_share(world, x, y)?;
    Some(json!({ "liquidity_pool_deposit": {
        "liquidity_pool_id": pool_id_json(&id)?, "max_amount_a": amount(v), "max_amount_b": amount(v),
        "min_price": price(min.0, min.1), "max_price": price(max.0, max.1),
    }}))
}

fn withdraw(world: &World, x: &AssetRef, y: &AssetRef, v: i64, min: i64) -> Option<Value> {
    let (id, _) = pool_share(world, x, y)?;
    Some(json!({ "liquidity_pool_withdraw": {
        "liquidity_pool_id": pool_id_json(&id)?, "amount": amount(v),
        "min_amount_a": amount(min), "min_amount_b": amount(min),
    }}))
}

/// An issuer enables clawback before a holder trusts its asset; the issuer claws back part
/// of the holder's balance, more than it, and a claimable balance the holder creates.
fn clawback(world: &World, rng: &mut Rng) -> Option<VecDeque<Gen>> {
    let (i, x) = issuer(world, rng)?;
    let hs = fresh(world, 1);
    let h = hs[0].clone();
    let to = other(world, rng, &[&i])?;
    let mut plan: VecDeque<Gen> = VecDeque::new();
    plan.push_back(issuer_flags(&i, AUTH_REVOCABLE | AUTH_CLAWBACK));
    plan.push_back(fixed(fund(&hs, HOLDER_BALANCE)));
    plan.push_back(open_lines(&hs, &[(x.clone(), MAX)]));
    plan.push_back(authorize_if_required(&i, &hs, &x));
    {
        let (hc, xc) = (h.clone(), x.clone());
        plan.push_back(one(&i, move |_| Some(payment(&hc, &xc, 1_000_000))));
    }
    for v in [10, 1_000_000_000_000] {
        let (hc, xc) = (h.clone(), x.clone());
        plan.push_back(one(&i, move |_| {
            Some(json!({ "clawback": { "asset": xc.json(), "from": acct(&hc), "amount": amount(v) } }))
        }));
    }
    {
        let (xc, toc) = (x.clone(), to.clone());
        plan.push_back(one(&h, move |_| {
            Some(create_balance(
                &xc,
                100,
                vec![(toc.clone(), json!("unconditional"))],
            ))
        }));
    }
    {
        let xc = x.clone();
        plan.push_back(one(&i, move |w| {
            let id = balance_for(w, &xc, &to)?;
            Some(json!({ "clawback_claimable_balance": { "balance_id": id } }))
        }));
    }
    Some(plan)
}

/// A pool of two assets of one issuer and three holders: one funds it, one has a pool share
/// line too small for what it deposits, one deposits nearly everything that fits in an
/// int64. Then withdrawals, a path payment across the pool, a revocation that cannot place
/// the redeemed shares, and from protocol 26 a frozen trustline.
fn pool(world: &World, rng: &mut Rng, freeze: bool) -> Option<VecDeque<Gen>> {
    let (i, _) = issuer(world, rng)?;
    let x = AssetRef {
        code: Some(CODES4[0].to_owned()),
        issuer: Some(i.clone()),
    };
    let y = AssetRef {
        code: Some(CODES12[0].to_owned()),
        issuer: Some(i.clone()),
    };
    let hs = fresh(world, 3);
    let (h, g, k) = (hs[0].clone(), hs[1].clone(), hs[2].clone());
    let mut plan: VecDeque<Gen> = VecDeque::new();
    plan.push_back(issuer_flags(&i, AUTH_REVOCABLE));
    plan.push_back(fixed(fund(&hs, HOLDER_BALANCE)));
    plan.push_back(open_lines(&hs, &[(x.clone(), MAX), (y.clone(), MAX)]));
    {
        let (i2, hs2, x2, y2) = (i.clone(), hs.clone(), x.clone(), y.clone());
        plan.push_back(Box::new(move |w, _| {
            if flags(w, &i2) & AUTH_REQUIRED == 0 {
                return None;
            }
            let mut ops = Vec::new();
            for hh in &hs2 {
                for a in [&x2, &y2] {
                    ops.push(op(None, authorize(w, hh, a, 1, false)));
                }
            }
            Some(Phase::Step(tx(w, &i2, ops, &[])))
        }));
    }
    {
        let (hs2, x2, y2) = (hs.clone(), x.clone(), y.clone());
        plan.push_back(Box::new(move |w, _| {
            let (_, share) = pool_share(w, &x2, &y2)?;
            let ops = hs2
                .iter()
                .enumerate()
                .map(|(n, hh)| {
                    let limit = if n == 1 { 1000 } else { MAX };
                    op(
                        (n > 0).then_some(hh.as_str()),
                        change_trust(json!({ "pool_share": share }), limit),
                    )
                })
                .collect();
            let signers: Vec<&str> = hs2.iter().map(String::as_str).collect();
            Some(Phase::Step(tx(w, &hs2[0], ops, &signers)))
        }));
    }
    // the asset line is in use by the pool share line
    {
        let xc = x.clone();
        plan.push_back(one(&h, move |_| Some(change_trust(xc.json(), 0))));
    }
    {
        let (i2, x2, y2) = (i.clone(), x.clone(), y.clone());
        let (h2, g2, k2) = (h.clone(), g.clone(), k.clone());
        plan.push_back(Box::new(move |w, _| {
            let mut ops = Vec::new();
            for (who, v) in [
                (&h2, 1_000_000_000_000),
                (&g2, 1_000_000_000_000),
                (&k2, MAX - 1),
            ] {
                for a in [&x2, &y2] {
                    ops.push(op(None, payment(who, a, v)));
                }
            }
            Some(Phase::Step(tx(w, &i2, ops, &[])))
        }));
    }
    {
        let (xc, yc) = (x.clone(), y.clone());
        plan.push_back(one(&h, move |w| {
            deposit(w, &xc, &yc, 1_000_000_000, (1, 2), (2, 1))
        }));
    }
    {
        let (xc, yc) = (x.clone(), y.clone());
        plan.push_back(one(&g, move |w| {
            deposit(w, &xc, &yc, 1_000_000_000, (1, 2), (2, 1))
        }));
        let (xc, yc) = (x.clone(), y.clone());
        plan.push_back(one(&g, move |w| {
            deposit(w, &xc, &yc, 100, (10, 1), (20, 1))
        }));
        let (xc, yc) = (x.clone(), y.clone());
        plan.push_back(one(&k, move |w| {
            deposit(w, &xc, &yc, MAX - 1, (1, 2), (2, 1))
        }));
    }
    {
        let (xc, yc) = (x.clone(), y.clone());
        plan.push_back(one(&h, move |w| withdraw(w, &xc, &yc, 100, MAX)));
        let (xc, yc) = (x.clone(), y.clone());
        plan.push_back(one(&h, move |w| withdraw(w, &xc, &yc, 1000, 1)));
    }
    {
        // across the pool, and into a line already at its limit
        let (gc, xc, yc) = (g.clone(), x.clone(), y.clone());
        plan.push_back(one(&h, move |_| Some(strict_send(&gc, &xc, 1000, &yc))));
        let (kc, xc, yc) = (k.clone(), x.clone(), y.clone());
        plan.push_back(one(&h, move |_| {
            Some(strict_send(&kc, &yc, 1_000_000, &xc))
        }));
    }
    {
        // the holder's line filled to its limit, then a withdrawal into it
        let (i2, h2, x2) = (i.clone(), h.clone(), x.clone());
        plan.push_back(Box::new(move |w, _| {
            let key = (h2.clone(), serde_json::to_string(&x2.json()).ok()?);
            let (bal, limit, _) = *w.lines.get(&key)?;
            let room = limit.checked_sub(bal)?;
            (room > 0)
                .then(|| Phase::Step(tx(w, &i2, vec![op(None, payment(&h2, &x2, room))], &[])))
        }));
        let (xc, yc) = (x.clone(), y.clone());
        plan.push_back(one(&h, move |w| withdraw(w, &xc, &yc, 1000, 0)));
    }
    if freeze {
        let (hc, xc) = (h.clone(), x.clone());
        plan.push_back(one(&i, move |_| {
            Some(create_balance(
                &xc,
                100,
                vec![(hc.clone(), json!("unconditional"))],
            ))
        }));
        let (hc, xc) = (h.clone(), x.clone());
        plan.push_back(Box::new(move |w, _| {
            let key = json!({ "trustline": { "account_id": acct(&hc), "asset": xc.json() } });
            let key = w.key_xdr(&key)?;
            Some(Phase::Step(crate::action::freeze_key(key)))
        }));
        let (xc, yc) = (x.clone(), y.clone());
        plan.push_back(one(&h, move |w| deposit(w, &xc, &yc, 100, (1, 2), (2, 1))));
        let (xc, yc) = (x.clone(), y.clone());
        plan.push_back(one(&h, move |w| withdraw(w, &xc, &yc, 100, 0)));
        let (hc, xc) = (h.clone(), x.clone());
        plan.push_back(one(&h, move |w| {
            let id = balance_for(w, &xc, &hc)?;
            Some(json!({ "claim_claimable_balance": { "balance_id": id } }))
        }));
    } else {
        // the redeemed pool shares become claimable balances, sponsored by an account in a
        // sponsorship sandwich that holds no reserve to spare (TransactionUtils.cpp,
        // removeOffersAndPoolShareTrustLines)
        let ps = vec![world.fresh_role(3)];
        let p = ps[0].clone();
        plan.push_back(Box::new(move |w, _| Some(fund(&ps, w.min_balance(0)))));
        for allow in [false, true] {
            let (i2, h2, x2, p2) = (i.clone(), h.clone(), x.clone(), p.clone());
            plan.push_back(Box::new(move |w, _| {
                let ops = vec![
                    op(Some(&p2), json!({ "begin_sponsoring_future_reserves": { "sponsored_id": acct(&h2) } })),
                    op(None, authorize(w, &h2, &x2, 0, allow)),
                    op(Some(&h2), json!("end_sponsoring_future_reserves")),
                ];
                Some(Phase::Step(tx(w, &i2, ops, &[&i2, &p2, &h2])))
            }));
        }
    }
    Some(plan)
}

/// Accounts funded right above the reserve they need: one opens a trustline and then has no
/// reserve left for an offer, one gets a sponsored trustline whose sponsorship is revoked.
fn reserve(world: &World, rng: &mut Rng) -> Option<VecDeque<Gen>> {
    let (i, x) = issuer(world, rng)?;
    let ps = vec![world.fresh_role(0)];
    let qs = vec![world.fresh_role(1)];
    let (p, q) = (ps[0].clone(), qs[0].clone());
    let s = other(world, rng, &[&i])?;
    let margin = 10 * i64::from(world.base_fee.max(100));
    let mut plan: VecDeque<Gen> = VecDeque::new();
    plan.push_back(Box::new(move |w, _| {
        Some(fund(&ps, w.min_balance(1) + margin))
    }));
    plan.push_back(open_lines(std::slice::from_ref(&p), &[(x.clone(), MAX)]));
    plan.push_back(authorize_if_required(&i, std::slice::from_ref(&p), &x));
    {
        let (pc, xc) = (p.clone(), x.clone());
        plan.push_back(one(&i, move |_| Some(payment(&pc, &xc, 1000))));
    }
    {
        let xc = x.clone();
        plan.push_back(one(&p, move |_| {
            Some(sell_offer(&xc, &AssetRef::native(), 10))
        }));
        let xc = x.clone();
        plan.push_back(one(&p, move |_| {
            Some(buy_offer(&xc, &AssetRef::native(), 10))
        }));
    }
    if world.protocol >= 14 {
        plan.push_back(Box::new(move |w, _| {
            Some(fund(&qs, w.min_balance(0) + margin))
        }));
        let (s2, q2, x2) = (s.clone(), q.clone(), x.clone());
        plan.push_back(Box::new(move |w, _| {
            let ops = vec![
                op(
                    None,
                    json!({ "begin_sponsoring_future_reserves": { "sponsored_id": acct(&q2) } }),
                ),
                op(Some(&q2), change_trust(x2.json(), MAX)),
                op(Some(&q2), json!("end_sponsoring_future_reserves")),
            ];
            Some(Phase::Step(tx(w, &s2, ops, &[&s2, &q2])))
        }));
        let (q2, x2) = (q.clone(), x.clone());
        plan.push_back(one(&s, move |_| {
            Some(
                json!({ "revoke_sponsorship": { "ledger_entry": { "trustline": {
                    "account_id": acct(&q2), "asset": x2.json(),
                }}}}),
            )
        }));
    }
    Some(plan)
}

/// Two transactions in one ledger: the first, from another account, changes the second's
/// account (bumps its sequence, touches its sequence ledger, raises or lowers its
/// thresholds, merges it away), so the second fails at apply though it was valid when queued.
/// The second is wrapped in a fee bump when the inner result is what the run misses.
fn ordered(world: &World, rng: &mut Rng, missing: &BTreeSet<String>) -> Option<VecDeque<Gen>> {
    #[derive(Clone, Copy)]
    enum Effect {
        Seq,
        Gap,
        Auth,
        Extra,
        Merge,
    }
    let code = |e: Effect| match e {
        Effect::Seq => "tx_bad_seq",
        Effect::Gap => "tx_bad_min_seq_age_or_gap",
        Effect::Auth => "tx_bad_auth",
        Effect::Extra => "tx_bad_auth_extra",
        Effect::Merge => "tx_no_account",
    };
    let mut effects = vec![Effect::Seq, Effect::Auth, Effect::Extra, Effect::Merge];
    // a sequence ledger exists from 19 (CAP-21)
    if world.protocol >= 19 {
        effects.push(Effect::Gap);
    }
    let want = |e: Effect, inner: bool| {
        let union = if inner {
            "InnerTransactionResultResult"
        } else {
            "TransactionResultResult"
        };
        missing.contains(&format!("{union}:{}", code(e)))
    };
    // fee bumps wrap v1 envelopes, which core takes from 13 on
    let bumps = world.protocol >= 13;
    let weights: Vec<u64> = effects
        .iter()
        .map(|e| u64::from(want(*e, false)) + u64::from(bumps && want(*e, true)))
        .collect();
    if weights.iter().all(|w| *w == 0) {
        return None;
    }
    let effect = effects[rng.weighted(&weights)];
    let inner = bumps && (want(effect, true) && (!want(effect, false) || rng.chance(1, 2)));
    let first = other(world, rng, &[])?;
    let payer = other(world, rng, &[&first])?;
    let vs = fresh(world, 2);
    let (v, z) = (vs[0].clone(), vs[1].clone());
    let mut plan: VecDeque<Gen> = VecDeque::new();
    plan.push_back(fixed(fund(&vs, HOLDER_BALANCE)));
    if matches!(effect, Effect::Extra) {
        // thresholds of 2 need both signatures when the second is queued; core stops
        // counting once a threshold is met, so at lower thresholds the second signature
        // would already be extra at submission
        let zc = z.clone();
        plan.push_back(one(&v, move |_| {
            Some(set_options(
                None,
                Some(json!({ "key": acct(&zc), "weight": 1 })),
                Some(2),
            ))
        }));
    }
    plan.push_back(Box::new(move |w, _| {
        let body = match effect {
            Effect::Seq => json!({ "bump_sequence": { "bump_to": format!("@seq:{v}+5") } }),
            // a bump to a lower number changes only the sequence ledger and time
            Effect::Gap => json!({ "bump_sequence": { "bump_to": "0" } }),
            Effect::Auth => set_options(None, None, Some(2)),
            // back to 1: the master key alone meets it, and the other signature is extra
            Effect::Extra => set_options(None, None, Some(1)),
            Effect::Merge => json!({ "account_merge": acct(&first) }),
        };
        let signers: Vec<&str> = if matches!(effect, Effect::Extra) {
            vec![&v, &z]
        } else {
            vec![&v]
        };
        let mut a_signers = vec![first.as_str()];
        a_signers.extend(&signers);
        let a = tx(w, &first, vec![op(Some(&v), body)], &a_signers);
        let mut b = tx(
            w,
            &v,
            vec![op(None, payment(&first, &AssetRef::native(), 1))],
            &signers,
        );
        if let Step::Tx { cond, fee_bump, .. } = &mut b {
            if matches!(effect, Effect::Gap) {
                *cond = Some(json!({ "v2": {
                    "time_bounds": null, "ledger_bounds": null, "min_seq_num": null,
                    "min_seq_age": "0", "min_seq_ledger_gap": 1, "extra_signers": [],
                }}));
            }
            if inner {
                let per_op = i64::from(w.base_fee.max(100));
                *fee_bump = Some(FeeBump {
                    source: payer.clone(),
                    fee: (per_op > 100).then_some(per_op * 2),
                    signers: Vec::new(),
                });
            }
        }
        Some(Phase::Pair(a, Box::new(b)))
    }));
    Some(plan)
}

/// A claimable balance whose claimants hold two-member and negated predicates, then a claim.
fn predicate(world: &World, rng: &mut Rng) -> Option<VecDeque<Gen>> {
    let src = other(world, rng, &[])?;
    let dests: Vec<String> = (0..3).filter_map(|_| other(world, rng, &[&src])).collect();
    if dests.len() < 3 {
        return None;
    }
    let now = close_time(world.last_closed + 1);
    let leaf = |rng: &mut Rng| -> Value {
        match rng.below(4) {
            0 => json!("unconditional"),
            1 => {
                json!({ "before_relative_time": (*rng.pick(&[1u64, 86_400, 100_000_000])).to_string() })
            }
            2 => json!({ "before_absolute_time": (now + 864_000).to_string() }),
            _ => json!({ "before_absolute_time": (now - 172_800).to_string() }),
        }
    };
    let and = json!({ "and": [leaf(rng), leaf(rng)] });
    let or = json!({ "or": [leaf(rng), leaf(rng)] });
    let not = json!({ "not": leaf(rng) });
    let claimants = vec![
        (dests[0].clone(), and),
        (dests[1].clone(), or),
        (dests[2].clone(), not),
    ];
    let mut plan: VecDeque<Gen> = VecDeque::new();
    plan.push_back(one(&src, move |_| {
        Some(create_balance(
            &AssetRef::native(),
            10_000_000,
            claimants.clone(),
        ))
    }));
    let claimer = rng.pick(&dests).clone();
    let c2 = claimer.clone();
    plan.push_back(one(&claimer, move |w| {
        let id = balance_for(w, &AssetRef::native(), &c2)?;
        Some(json!({ "claim_claimable_balance": { "balance_id": id } }))
    }));
    Some(plan)
}

/// The start of inflation (InflationOpFrame.cpp, 1 July 2014) and its period, one week.
const INFLATION_START: u64 = 1_404_172_800;
const INFLATION_FREQUENCY: u64 = 604_800;
const MAX_OPS: usize = 100;

/// Inflation runs once a week of close time since 2014; a chain whose ledgers start in 2026
/// is hundreds of weeks behind, so transactions of many inflation operations catch up until
/// one meets a week that has not come yet.
fn inflation(world: &World, rng: &mut Rng) -> Option<VecDeque<Gen>> {
    let src = other(world, rng, &[])?;
    let mut plan: VecDeque<Gen> = VecDeque::new();
    let done = std::rc::Rc::new(std::cell::Cell::new(false));
    for _ in 0..12 {
        let src = src.clone();
        let done = done.clone();
        plan.push_back(Box::new(move |w, _| {
            if done.get() {
                return None;
            }
            let close = close_time(w.last_closed + 1);
            let due = close.saturating_sub(INFLATION_START) / INFLATION_FREQUENCY + 1;
            let left = due.saturating_sub(u64::from(w.inflation_seq)) as usize;
            let n = if left < MAX_OPS {
                done.set(true);
                left + 1
            } else {
                MAX_OPS
            };
            let ops = (0..n).map(|_| op(None, json!("inflation"))).collect();
            Some(Phase::Step(tx(w, &src, ops, &[])))
        }));
    }
    Some(plan)
}

/// A holder whose line has room for less than a claimable balance or a payment sends it.
fn line_full(world: &World, rng: &mut Rng) -> Option<VecDeque<Gen>> {
    let (i, x) = issuer(world, rng)?;
    let hs = fresh(world, 1);
    let h = hs[0].clone();
    let mut plan: VecDeque<Gen> = VecDeque::new();
    plan.push_back(fixed(fund(&hs, HOLDER_BALANCE)));
    plan.push_back(open_lines(&hs, &[(x.clone(), 100)]));
    plan.push_back(authorize_if_required(&i, &hs, &x));
    {
        let (hc, xc) = (h.clone(), x.clone());
        plan.push_back(one(&i, move |_| {
            Some(create_balance(
                &xc,
                1000,
                vec![(hc.clone(), json!("unconditional"))],
            ))
        }));
        let (hc, xc) = (h.clone(), x.clone());
        plan.push_back(one(&h, move |w| {
            let id = balance_for(w, &xc, &hc)?;
            Some(json!({ "claim_claimable_balance": { "balance_id": id } }))
        }));
    }
    if world.protocol >= 12 {
        let (hc, xc) = (h.clone(), x.clone());
        plan.push_back(one(&i, move |_| Some(strict_send(&hc, &xc, 1000, &xc))));
    }
    Some(plan)
}

/// The merge destination's native buying liabilities leave less room than the merged
/// balance: an issuer offers its asset for nearly every lumen an int64 can hold, then a new
/// account merges into it.
fn dest_full(world: &World, rng: &mut Rng) -> Option<VecDeque<Gen>> {
    let (i, x) = issuer(world, rng)?;
    let ms = fresh(world, 1);
    let m = ms[0].clone();
    let mut plan: VecDeque<Gen> = VecDeque::new();
    plan.push_back(fixed(fund(&ms, HOLDER_BALANCE)));
    {
        let i2 = i.clone();
        plan.push_back(one(&i, move |w| {
            let bal = *w.native.get(&i2)?;
            let v = MAX - bal - HOLDER_BALANCE / 10;
            Some(sell_offer(&x, &AssetRef::native(), v))
        }));
    }
    plan.push_back(one(&m, move |_| Some(json!({ "account_merge": acct(&i) }))));
    Some(plan)
}

/// One more signer than an account can hold (MAX_SIGNERS, 20), in one transaction.
fn signers(world: &World, rng: &mut Rng) -> Option<VecDeque<Gen>> {
    let ss = fresh(world, 1);
    let s = ss[0].clone();
    let keys: Vec<Value> = (0..21)
        .map(|_| {
            let k: [u8; 32] = rng.bytes(32).try_into().expect("32 bytes");
            serde_json::to_value(stellar_xdr::SignerKey::HashX(stellar_xdr::Uint256(k)))
                .expect("a signer key serializes")
        })
        .collect();
    let mut plan: VecDeque<Gen> = VecDeque::new();
    plan.push_back(fixed(fund(&ss, HOLDER_BALANCE)));
    plan.push_back(Box::new(move |w, _| {
        let ops = keys
            .iter()
            .map(|k| {
                op(
                    None,
                    set_options(None, Some(json!({ "key": k, "weight": 1 })), None),
                )
            })
            .collect();
        Some(Phase::Step(tx(w, &s, ops, &[])))
    }));
    Some(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shapekit_core_driver::keys::{account_strkey, derive_key};

    fn resolve(v: &Value, w: &World) -> Value {
        match v {
            Value::String(s) if s.starts_with("@account:") => Value::String(account_strkey(
                &derive_key(&w.recipe_id, &s[9..]).verifying_key(),
            )),
            Value::String(s) if s.starts_with("@seq:") => Value::String("5".into()),
            Value::Array(a) => Value::Array(a.iter().map(|x| resolve(x, w)).collect()),
            Value::Object(m) => {
                Value::Object(m.iter().map(|(k, x)| (k.clone(), resolve(x, w))).collect())
            }
            other => other.clone(),
        }
    }

    fn check(step: &Step, w: &World, failures: &mut Vec<String>) {
        if let Step::Tx {
            operations, cond, ..
        } = step
        {
            for o in operations {
                let r = resolve(o, w);
                if let Err(e) = serde_json::from_value::<stellar_xdr::Operation>(r.clone()) {
                    failures.push(format!("{e}: {r}"));
                }
            }
            if let Some(c) = cond
                && let Err(e) = serde_json::from_value::<stellar_xdr::Preconditions>(c.clone())
            {
                failures.push(format!("{e}: {c}"));
            }
        }
    }

    #[test]
    fn every_move_builds_operations_that_decode() {
        let mut failures = Vec::new();
        let mut seen = BTreeSet::new();
        let missing: BTreeSet<String> = KINDS
            .iter()
            .flat_map(|k| k.targets().iter().map(|t| (*t).to_owned()))
            .collect();
        for protocol in [9, 11, 13, 17, 19, 30] {
            for seed in 0..60 {
                let mut w = World::new(
                    "explore.test",
                    "Test SDF Network ; September 2015",
                    protocol,
                );
                for r in ["a", "b", "c", "d", "e"] {
                    w.add_role(r);
                }
                w.issuers = vec!["a".into(), "b".into()];
                w.native.insert("a".into(), 100_000_000_000);
                w.native.insert("b".into(), 100_000_000_000);
                let mut rng = Rng::new(seed);
                let Some(mut m) = start(&w, &mut rng, &missing, true, protocol >= 26) else {
                    continue;
                };
                seen.insert(format!("{:?}", m.kind));
                while let Some(p) = m.next(&w, &mut rng) {
                    match p {
                        Phase::Step(Step::Fund { roles, .. }) => {
                            for r in roles {
                                w.add_role(&r);
                            }
                        }
                        Phase::Step(s) => check(&s, &w, &mut failures),
                        Phase::Pair(a, b) => {
                            check(&a, &w, &mut failures);
                            check(&b, &w, &mut failures);
                        }
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
        assert_eq!(seen.len(), KINDS.len(), "{seen:?}");
    }
}
