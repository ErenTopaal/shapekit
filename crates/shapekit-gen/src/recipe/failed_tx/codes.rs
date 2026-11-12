//! One failing operation per apply-time result code: each in its own
//! transaction, against accounts the chain sets up first, with core's code
//! checked before the ledger is captured. The codes are the ones
//! `registry/uncovered.toml` named as producible but unrecorded; the
//! condition behind each is read from the operation's `doApply` at
//! stellar-core v30.0.0 and noted next to its case.
//!
//! Accounts and what they hold before the first failing transaction:
//!
//! | role          | state                                                              |
//! |---------------|--------------------------------------------------------------------|
//! | `issuer-open` | no flags; issues OPN, SLF and NOB                                  |
//! | `issuer-auth` | AUTH_REQUIRED, AUTH_REVOCABLE, AUTH_CLAWBACK_ENABLED; issues AUT   |
//! | `holder`      | OPN, NOB, AUT (authorized), pool shares of XLM/AUT                 |
//! | `plain`       | nothing but XLM                                                    |
//! | `unauth`      | an AUT line never authorized                                       |
//! | `deauth`      | 100 AUT on a line cut back to maintain-liabilities, XLM/AUT shares |
//! | `full`        | an OPN line at its limit                                           |
//! | `trader`      | an offer selling SLF for XLM                                       |
//! | `poor`        | 1 XLM above its reserve; an OPN line and a sponsored AUT line      |
//! | `sponsor`     | sponsors poor's AUT line                                           |
//! | `immutable`   | AUTH_IMMUTABLE                                                     |
//! | `lp`          | all its OPN in the XLM/OPN pool                                    |
//! | `lp-capped`   | 100 OPN and an XLM/OPN pool share line with a limit of one stroop  |
//! | `whale`       | one OPN, one SLF and an OPN/SLF pool share line                    |
//!
//! `issuer-open` fills the OPN/SLF pool to `i64::MAX` on both sides: an
//! issuer's own balance of its asset is unbounded (`TrustLineWrapper`'s
//! issuer implementation), which is the only way a pool reaches the bound.

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use shapekit_core_driver::keys::{account_id, ed25519_signer, muxed_account};
use stellar_xdr::{
    AccountEntryExt, AccountEntryExtensionV1Ext, AllowTrustOp, AlphaNum4, Asset, AssetCode,
    AssetCode4, BeginSponsoringFutureReservesOp, ChangeTrustAsset, ChangeTrustOp,
    ClaimClaimableBalanceOp, ClaimPredicate, ClaimableBalanceId, Claimant, ClaimantV0,
    ClawbackClaimableBalanceOp, ClawbackOp, CreateAccountOp, CreateClaimableBalanceOp,
    CreateClaimableBalanceResult, CreatePassiveSellOfferOp, Hash, LedgerEntryChange,
    LedgerEntryData, LedgerKey, LedgerKeyClaimableBalance, LedgerKeyData, LedgerKeyTrustLine,
    Limits, LiquidityPoolConstantProductParameters, LiquidityPoolDepositOp,
    LiquidityPoolParameters, LiquidityPoolWithdrawOp, ManageBuyOfferOp, ManageDataOp,
    ManageSellOfferOp, OperationBody, OperationResult, OperationResultTr,
    PathPaymentStrictReceiveOp, PathPaymentStrictSendOp, PaymentOp, PoolId, Price,
    RevokeSponsorshipOp, SetOptionsOp, SetTrustLineFlagsOp, Signer, StringM, TransactionMeta,
    TransactionResultResult, TrustLineAsset, WriteXdr,
};

use super::super::{ChainCtx, Selector, Spec};

/// Starting balance of the well-funded accounts, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

/// One XLM, and one unit of every credit asset, in stroops.
const UNIT: i64 = 10_000_000;

/// Base reserve at genesis, 10 XLM (`GENESIS_LEDGER_BASE_RESERVE`,
/// stellar-core v30.0.0); recipes never upgrade it.
const BASE_RESERVE: i64 = 100_000_000;

/// Poor's balance: the reserve of an account with one subentry plus 1 XLM,
/// a tenth of a base reserve, enough for every fee it pays and too little
/// for any new entry.
const POOR_BALANCE: i64 = 3 * BASE_RESERVE + UNIT;

/// The constant-product fee core accepts, 30 basis points.
const POOL_FEE: i32 = 30;

const AUTHORIZED: u32 = 1;
const MAINTAIN: u32 = 2;
const AUTH_REQUIRED: u32 = 1;
const AUTH_REVOCABLE: u32 = 2;
const AUTH_IMMUTABLE: u32 = 4;
const AUTH_CLAWBACK: u32 = 8;

const ONE: Price = Price { n: 1, d: 1 };

/// The price band of deposits that should pass the price check.
const HALF: Price = Price { n: 1, d: 2 };
const TWICE: Price = Price { n: 2, d: 1 };

fn credit(code: &[u8; 3], issuer: &SigningKey) -> Asset {
    Asset::CreditAlphanum4(AlphaNum4 {
        asset_code: AssetCode4([code[0], code[1], code[2], 0]),
        issuer: account_id(&issuer.verifying_key()),
    })
}

fn line(asset: &Asset) -> Result<TrustLineAsset> {
    match asset {
        Asset::CreditAlphanum4(a) => Ok(TrustLineAsset::CreditAlphanum4(a.clone())),
        other => bail!("{other:?} has no credit trust line"),
    }
}

fn trust_limit(asset: &Asset, limit: i64) -> Result<OperationBody> {
    let line = match asset {
        Asset::CreditAlphanum4(a) => ChangeTrustAsset::CreditAlphanum4(a.clone()),
        other => bail!("trust line to {other:?} is not used here"),
    };
    Ok(OperationBody::ChangeTrust(ChangeTrustOp { line, limit }))
}

fn trust(asset: &Asset) -> Result<OperationBody> {
    trust_limit(asset, i64::MAX)
}

fn params(a: &Asset, b: &Asset) -> LiquidityPoolParameters {
    LiquidityPoolParameters::LiquidityPoolConstantProduct(LiquidityPoolConstantProductParameters {
        asset_a: a.clone(),
        asset_b: b.clone(),
        fee: POOL_FEE,
    })
}

fn pool_id(p: &LiquidityPoolParameters) -> Result<PoolId> {
    Ok(PoolId(Hash(
        Sha256::digest(p.to_xdr(Limits::none())?).into(),
    )))
}

fn trust_pool(p: &LiquidityPoolParameters, limit: i64) -> OperationBody {
    OperationBody::ChangeTrust(ChangeTrustOp {
        line: ChangeTrustAsset::PoolShare(p.clone()),
        limit,
    })
}

fn pay(to: &SigningKey, asset: &Asset, amount: i64) -> OperationBody {
    OperationBody::Payment(PaymentOp {
        destination: muxed_account(&to.verifying_key()),
        asset: asset.clone(),
        amount,
    })
}

fn receive(to: &SigningKey, send: &Asset, max: i64, dest: &Asset, amount: i64) -> OperationBody {
    OperationBody::PathPaymentStrictReceive(PathPaymentStrictReceiveOp {
        send_asset: send.clone(),
        send_max: max,
        destination: muxed_account(&to.verifying_key()),
        dest_asset: dest.clone(),
        dest_amount: amount,
        path: Default::default(),
    })
}

fn send(to: &SigningKey, send: &Asset, amount: i64, dest: &Asset, min: i64) -> OperationBody {
    OperationBody::PathPaymentStrictSend(PathPaymentStrictSendOp {
        send_asset: send.clone(),
        send_amount: amount,
        destination: muxed_account(&to.verifying_key()),
        dest_asset: dest.clone(),
        dest_min: min,
        path: Default::default(),
    })
}

fn sell(selling: &Asset, buying: &Asset, amount: i64, price: Price, id: i64) -> OperationBody {
    OperationBody::ManageSellOffer(ManageSellOfferOp {
        selling: selling.clone(),
        buying: buying.clone(),
        amount,
        price,
        offer_id: id,
    })
}

fn buy(selling: &Asset, buying: &Asset, amount: i64, price: Price, id: i64) -> OperationBody {
    OperationBody::ManageBuyOffer(ManageBuyOfferOp {
        selling: selling.clone(),
        buying: buying.clone(),
        buy_amount: amount,
        price,
        offer_id: id,
    })
}

fn flags(trustor: &SigningKey, asset: &Asset, clear: u32, set: u32) -> OperationBody {
    OperationBody::SetTrustLineFlags(SetTrustLineFlagsOp {
        trustor: account_id(&trustor.verifying_key()),
        asset: asset.clone(),
        clear_flags: clear,
        set_flags: set,
    })
}

fn allow(trustor: &SigningKey, code: &[u8; 3], authorize: u32) -> OperationBody {
    OperationBody::AllowTrust(AllowTrustOp {
        trustor: account_id(&trustor.verifying_key()),
        asset: AssetCode::CreditAlphanum4(AssetCode4([code[0], code[1], code[2], 0])),
        authorize,
    })
}

fn set_options() -> SetOptionsOp {
    SetOptionsOp {
        inflation_dest: None,
        clear_flags: None,
        set_flags: None,
        master_weight: None,
        low_threshold: None,
        med_threshold: None,
        high_threshold: None,
        home_domain: None,
        signer: None,
    }
}

fn data(name: &str, value: Option<&[u8]>) -> Result<OperationBody> {
    Ok(OperationBody::ManageData(ManageDataOp {
        data_name: StringM::try_from(name.as_bytes().to_vec())?.into(),
        data_value: value.map(|v| v.to_vec().try_into()).transpose()?,
    }))
}

fn deposit(pool: &PoolId, a: i64, b: i64, min: Price, max: Price) -> OperationBody {
    OperationBody::LiquidityPoolDeposit(LiquidityPoolDepositOp {
        liquidity_pool_id: pool.clone(),
        max_amount_a: a,
        max_amount_b: b,
        min_price: min,
        max_price: max,
    })
}

fn withdraw(pool: &PoolId, amount: i64, min_a: i64, min_b: i64) -> OperationBody {
    OperationBody::LiquidityPoolWithdraw(LiquidityPoolWithdrawOp {
        liquidity_pool_id: pool.clone(),
        amount,
        min_amount_a: min_a,
        min_amount_b: min_b,
    })
}

fn claimable(asset: &Asset, amount: i64, claimants: &[&SigningKey]) -> Result<OperationBody> {
    let claimants: Vec<Claimant> = claimants
        .iter()
        .map(|k| {
            Claimant::ClaimantTypeV0(ClaimantV0 {
                destination: account_id(&k.verifying_key()),
                predicate: ClaimPredicate::Unconditional,
            })
        })
        .collect();
    Ok(OperationBody::CreateClaimableBalance(
        CreateClaimableBalanceOp {
            asset: asset.clone(),
            amount,
            claimants: claimants.try_into()?,
        },
    ))
}

fn begin(sponsored: &SigningKey) -> OperationBody {
    OperationBody::BeginSponsoringFutureReserves(BeginSponsoringFutureReservesOp {
        sponsored_id: account_id(&sponsored.verifying_key()),
    })
}

fn create(to: &SigningKey, balance: i64) -> OperationBody {
    OperationBody::CreateAccount(CreateAccountOp {
        destination: account_id(&to.verifying_key()),
        starting_balance: balance,
    })
}

fn trustline_key(k: &SigningKey, asset: &Asset) -> Result<LedgerKey> {
    Ok(LedgerKey::Trustline(LedgerKeyTrustLine {
        account_id: account_id(&k.verifying_key()),
        asset: line(asset)?,
    }))
}

fn data_key(k: &SigningKey, name: &str) -> Result<LedgerKey> {
    Ok(LedgerKey::Data(LedgerKeyData {
        account_id: account_id(&k.verifying_key()),
        data_name: StringM::try_from(name.as_bytes().to_vec())?.into(),
    }))
}

fn balance_key(id: &ClaimableBalanceId) -> LedgerKey {
    LedgerKey::ClaimableBalance(LedgerKeyClaimableBalance {
        balance_id: id.clone(),
    })
}

/// A balance id no transaction ever creates: a hash of a fixed string.
fn absent_balance() -> ClaimableBalanceId {
    ClaimableBalanceId::ClaimableBalanceIdTypeV0(Hash(
        Sha256::digest(b"shapekit/classic.failed-tx/no-such-balance").into(),
    ))
}

/// The code of one operation result as XDR-JSON names it:
/// `payment.line_full` for an inner result, `op_bad_auth` for an
/// operation-level one.
pub(crate) fn op_code(r: &OperationResult) -> Result<String> {
    use serde_json::Value;
    let arm = |v: &Value| -> Option<String> {
        match v {
            Value::String(s) => Some(s.clone()),
            Value::Object(m) if m.len() == 1 => m.keys().next().cloned(),
            _ => None,
        }
    };
    let v = serde_json::to_value(r)?;
    match &v {
        Value::String(s) => Ok(s.clone()),
        Value::Object(m) => {
            let inner = m
                .get("op_inner")
                .with_context(|| format!("operation result {v}"))?;
            let Value::Object(tr) = inner else {
                bail!("operation result {v}");
            };
            let (op, code) = tr.iter().next().context("empty operation result")?;
            let code = arm(code).with_context(|| format!("operation result {v}"))?;
            Ok(format!("{op}.{code}"))
        }
        _ => bail!("operation result {v}"),
    }
}

fn op_results(r: &TransactionResultResult) -> Result<&[OperationResult]> {
    match r {
        TransactionResultResult::TxFailed(ops) | TransactionResultResult::TxSuccess(ops) => {
            Ok(ops.as_slice())
        }
        other => bail!("core answered {other:?}, expected operation results"),
    }
}

/// One failing transaction: its operations (with optional operation
/// sources), the extra keys that sign, and the code core must give the
/// operation at `at`.
struct Fail<'a> {
    slug: &'a str,
    source: &'a SigningKey,
    ops: Vec<(Option<&'a SigningKey>, OperationBody)>,
    cosigners: Vec<&'a SigningKey>,
    at: usize,
    want: &'a str,
    notes: &'a str,
    fields: &'a [&'a str],
}

impl<'a> Fail<'a> {
    fn one(
        slug: &'a str,
        source: &'a SigningKey,
        op: OperationBody,
        want: &'a str,
        notes: &'a str,
        fields: &'a [&'a str],
    ) -> Self {
        Self {
            slug,
            source,
            ops: vec![(None, op)],
            cosigners: Vec::new(),
            at: 0,
            want,
            notes,
            fields,
        }
    }

    fn run(self, ctx: &mut ChainCtx) -> Result<()> {
        let mut b = ctx.tx(self.source)?;
        for (src, op) in self.ops {
            b = b.operation(src.map(|k| muxed_account(&k.verifying_key())), op);
        }
        let mut keys = vec![self.source];
        keys.extend(self.cosigners);
        let env = ctx.sign(b.build()?, &keys)?;
        let applied = ctx
            .submit_and_close_failing(&env)
            .with_context(|| self.slug.to_string())?;
        let ops = op_results(&applied.result.result).with_context(|| self.slug.to_string())?;
        let got = op_code(
            ops.get(self.at)
                .with_context(|| format!("{}: no result for operation {}", self.slug, self.at))?,
        )?;
        ensure!(
            got == self.want,
            "{}: core answered {got}, expected {}",
            self.slug,
            self.want
        );
        ctx.capture(
            self.slug,
            Selector::ledger_of(&applied),
            Spec::new(format!("{} (core: {got})", self.notes)).fields(self.fields),
        )
    }
}

/// Submits a setup transaction that must succeed.
fn ok(
    ctx: &mut ChainCtx,
    source: &SigningKey,
    ops: Vec<(Option<&SigningKey>, OperationBody)>,
    cosigners: &[&SigningKey],
) -> Result<stellar_xdr::TransactionResultResult> {
    let mut b = ctx.tx(source)?;
    for (src, op) in ops {
        b = b.operation(src.map(|k| muxed_account(&k.verifying_key())), op);
    }
    let mut keys = vec![source];
    keys.extend_from_slice(cosigners);
    let env = ctx.sign(b.build()?, &keys)?;
    Ok(ctx.submit_and_close(&env)?.result.result)
}

fn mine(ops: Vec<OperationBody>) -> Vec<(Option<&'static SigningKey>, OperationBody)> {
    ops.into_iter().map(|o| (None, o)).collect()
}

fn created_balance(r: &TransactionResultResult) -> Result<ClaimableBalanceId> {
    op_results(r)?
        .iter()
        .find_map(|op| match op {
            OperationResult::OpInner(OperationResultTr::CreateClaimableBalance(
                CreateClaimableBalanceResult::Success(id),
            )) => Some(id.clone()),
            _ => None,
        })
        .context("no claimable balance was created")
}

struct Keys {
    issuer_open: SigningKey,
    issuer_auth: SigningKey,
    holder: SigningKey,
    plain: SigningKey,
    unauth: SigningKey,
    deauth: SigningKey,
    full: SigningKey,
    trader: SigningKey,
    poor: SigningKey,
    sponsor: SigningKey,
    immutable: SigningKey,
    lp: SigningKey,
    lp_capped: SigningKey,
    whale: SigningKey,
    maker: SigningKey,
    ghost: SigningKey,
    ghost_issuer: SigningKey,
}

struct World {
    k: Keys,
    opn: Asset,
    slf: Asset,
    nob: Asset,
    aut: Asset,
    gho: Asset,
    open_pool: LiquidityPoolParameters,
    auth_pool: LiquidityPoolParameters,
    big_pool: LiquidityPoolParameters,
    /// OPN to plain and full, not clawback-enabled.
    opn_balance: ClaimableBalanceId,
    /// AUT to unauth.
    aut_balance: ClaimableBalanceId,
}

const FUNDED: [&str; 14] = [
    "issuer-open",
    "issuer-auth",
    "holder",
    "plain",
    "unauth",
    "deauth",
    "full",
    "trader",
    "sponsor",
    "immutable",
    "lp",
    "lp-capped",
    "whale",
    "maker",
];

fn setup(ctx: &mut ChainCtx) -> Result<World> {
    ctx.fund(&FUNDED, BALANCE)?;
    ctx.fund(&["poor"], POOR_BALANCE)?;
    let a = |r: &str| ctx.account(r);
    let k = Keys {
        issuer_open: a("issuer-open"),
        issuer_auth: a("issuer-auth"),
        holder: a("holder"),
        plain: a("plain"),
        unauth: a("unauth"),
        deauth: a("deauth"),
        full: a("full"),
        trader: a("trader"),
        poor: a("poor"),
        sponsor: a("sponsor"),
        immutable: a("immutable"),
        lp: a("lp"),
        lp_capped: a("lp-capped"),
        whale: a("whale"),
        maker: a("maker"),
        // derived, never funded
        ghost: a("ghost-account"),
        ghost_issuer: a("ghost-issuer"),
    };
    let opn = credit(b"OPN", &k.issuer_open);
    let slf = credit(b"SLF", &k.issuer_open);
    let nob = credit(b"NOB", &k.issuer_open);
    let aut = credit(b"AUT", &k.issuer_auth);
    let gho = credit(b"GHO", &k.ghost_issuer);
    let open_pool = params(&Asset::Native, &opn);
    let auth_pool = params(&Asset::Native, &aut);
    let big_pool = params(&opn, &slf);

    ok(
        ctx,
        &k.issuer_auth,
        mine(vec![OperationBody::SetOptions(SetOptionsOp {
            set_flags: Some(AUTH_REQUIRED | AUTH_REVOCABLE | AUTH_CLAWBACK),
            ..set_options()
        })]),
        &[],
    )?;
    ok(
        ctx,
        &k.immutable,
        mine(vec![OperationBody::SetOptions(SetOptionsOp {
            set_flags: Some(AUTH_IMMUTABLE),
            ..set_options()
        })]),
        &[],
    )?;

    // trust lines, one transaction per account
    ok(
        ctx,
        &k.holder,
        mine(vec![trust(&opn)?, trust(&nob)?, trust(&aut)?]),
        &[],
    )?;
    ok(ctx, &k.unauth, mine(vec![trust(&aut)?]), &[])?;
    ok(ctx, &k.deauth, mine(vec![trust(&aut)?]), &[])?;
    ok(ctx, &k.full, mine(vec![trust_limit(&opn, 10 * UNIT)?]), &[])?;
    ok(ctx, &k.trader, mine(vec![trust(&slf)?]), &[])?;
    ok(ctx, &k.lp, mine(vec![trust(&opn)?]), &[])?;
    ok(ctx, &k.lp_capped, mine(vec![trust(&opn)?]), &[])?;
    ok(ctx, &k.whale, mine(vec![trust(&opn)?, trust(&slf)?]), &[])?;
    ok(ctx, &k.poor, mine(vec![trust(&opn)?]), &[])?;
    // poor's AUT line is sponsored, so revoking that sponsorship needs a
    // reserve poor does not have
    ok(
        ctx,
        &k.sponsor,
        vec![
            (None, begin(&k.poor)),
            (Some(&k.poor), trust(&aut)?),
            (Some(&k.poor), OperationBody::EndSponsoringFutureReserves),
        ],
        &[&k.poor],
    )?;

    ok(
        ctx,
        &k.issuer_auth,
        mine(vec![
            flags(&k.holder, &aut, 0, AUTHORIZED),
            flags(&k.deauth, &aut, 0, AUTHORIZED),
            pay(&k.holder, &aut, 1000 * UNIT),
            pay(&k.deauth, &aut, 100 * UNIT),
            flags(&k.deauth, &aut, AUTHORIZED, MAINTAIN),
        ]),
        &[],
    )?;
    ok(
        ctx,
        &k.issuer_open,
        mine(vec![
            pay(&k.holder, &opn, 1000 * UNIT),
            pay(&k.full, &opn, 10 * UNIT),
            pay(&k.trader, &slf, 100 * UNIT),
            pay(&k.lp, &opn, 100 * UNIT),
            pay(&k.lp_capped, &opn, 100 * UNIT),
            pay(&k.whale, &opn, UNIT),
            pay(&k.whale, &slf, UNIT),
            trust_pool(&big_pool, i64::MAX),
            deposit(&pool_id(&big_pool)?, i64::MAX, i64::MAX, ONE, ONE),
        ]),
        &[],
    )?;
    ok(
        ctx,
        &k.trader,
        mine(vec![sell(&slf, &Asset::Native, 10 * UNIT, ONE, 0)]),
        &[],
    )?;
    ok(
        ctx,
        &k.holder,
        mine(vec![
            trust_pool(&auth_pool, i64::MAX),
            deposit(&pool_id(&auth_pool)?, 100 * UNIT, 100 * UNIT, ONE, ONE),
        ]),
        &[],
    )?;
    // a line at maintain-liabilities may still take pool shares
    ok(
        ctx,
        &k.deauth,
        mine(vec![trust_pool(&auth_pool, i64::MAX)]),
        &[],
    )?;
    ok(
        ctx,
        &k.lp,
        mine(vec![
            trust_pool(&open_pool, i64::MAX),
            deposit(&pool_id(&open_pool)?, 100 * UNIT, 100 * UNIT, ONE, ONE),
        ]),
        &[],
    )?;
    ok(
        ctx,
        &k.lp_capped,
        mine(vec![trust_pool(&open_pool, 1)]),
        &[],
    )?;
    ok(
        ctx,
        &k.whale,
        mine(vec![trust_pool(&big_pool, i64::MAX)]),
        &[],
    )?;
    let r = ok(
        ctx,
        &k.holder,
        mine(vec![claimable(&opn, 20 * UNIT, &[&k.plain, &k.full])?]),
        &[],
    )?;
    let opn_balance = created_balance(&r)?;
    let r = ok(
        ctx,
        &k.holder,
        mine(vec![claimable(&aut, UNIT, &[&k.unauth])?]),
        &[],
    )?;
    let aut_balance = created_balance(&r)?;

    Ok(World {
        k,
        opn,
        slf,
        nob,
        aut,
        gho,
        open_pool,
        auth_pool,
        big_pool,
        opn_balance,
        aut_balance,
    })
}

const PAY: &[&str] = &[
    "op.payment.destination",
    "op.payment.asset",
    "op.payment.amount",
];
const PATH: &[&str] = &["op.path_payment.destination", "op.path_payment.amounts"];
const OFFER: &[&str] = &[
    "op.manage_offer.assets_amount_price",
    "op.manage_offer.offer_id",
];
const TRUST: &[&str] = &["op.change_trust.line", "op.change_trust.limit"];
const POOL_TRUST: &[&str] = &["op.change_trust.line.liquidity_pool"];
const CREATE: &[&str] = &[
    "op.create_account.destination",
    "op.create_account.starting_balance",
];
const MERGE: &[&str] = &["op.account_merge.destination"];
const DATA: &[&str] = &["op.manage_data.data_name", "op.manage_data.data_value"];
const CB_CREATE: &[&str] = &[
    "op.create_claimable_balance.asset_amount",
    "op.create_claimable_balance.claimants.destination",
];
const CB_CLAIM: &[&str] = &["op.claim_claimable_balance.balance_id"];
const CB_CLAWBACK: &[&str] = &["op.clawback_claimable_balance.balance_id"];
const CLAWBACK: &[&str] = &["op.clawback.from", "op.clawback.asset_amount"];
const DEPOSIT: &[&str] = &["op.liquidity_pool_deposit"];
const WITHDRAW: &[&str] = &["op.liquidity_pool_withdraw"];
const REVOKE: &[&str] = &["op.revoke_sponsorship"];
const ALLOW: &[&str] = &["op.allow_trust"];
const STLF: &[&str] = &["op.set_trust_line_flags"];
const SET_OPTIONS: &[&str] = &["op.set_options.thresholds_and_flags"];

pub(super) fn run(ctx: &mut ChainCtx) -> Result<()> {
    let w = setup(ctx)?;
    payments(ctx, &w)?;
    path_payments(ctx, &w)?;
    offers(ctx, &w)?;
    accounts(ctx, &w)?;
    trust_lines(ctx, &w)?;
    trust_flags(ctx, &w)?;
    claimable_balances(ctx, &w)?;
    pools(ctx, &w)?;
    sponsorship(ctx, &w)?;
    passive_offer_by_operation_source(ctx, &w)?;
    data_removed(ctx, &w)?;
    work_limit(ctx, &w)
}

fn payments(ctx: &mut ChainCtx, w: &World) -> Result<()> {
    let k = &w.k;
    for f in [
        Fail::one(
            "payment-line-full",
            &k.holder,
            pay(&k.full, &w.opn, 20 * UNIT),
            "payment.line_full",
            "a payment of 20 OPN to a line whose limit is 10 and already holds 10",
            PAY,
        ),
        Fail::one(
            "payment-no-trust",
            &k.holder,
            pay(&k.plain, &w.opn, UNIT),
            "payment.no_trust",
            "a payment of OPN to an account without an OPN line",
            PAY,
        ),
        Fail::one(
            "payment-not-authorized",
            &k.holder,
            pay(&k.unauth, &w.aut, UNIT),
            "payment.not_authorized",
            "a payment of AUT to a line its issuer never authorized",
            PAY,
        ),
        Fail::one(
            "payment-src-no-trust",
            &k.plain,
            pay(&k.holder, &w.opn, UNIT),
            "payment.src_no_trust",
            "a payment of OPN from an account without an OPN line",
            PAY,
        ),
        Fail::one(
            "payment-src-not-authorized",
            &k.deauth,
            pay(&k.holder, &w.aut, UNIT),
            "payment.src_not_authorized",
            "a payment of AUT from a line cut back to maintain-liabilities",
            PAY,
        ),
    ] {
        f.run(ctx)?;
    }
    Ok(())
}

fn path_payments(ctx: &mut ChainCtx, w: &World) -> Result<()> {
    let k = &w.k;
    let xlm = Asset::Native;
    let big = 2_000_000 * UNIT;
    // strict receive checks the destination first, then the path, then the
    // source; strict send the source first and the destination last
    // (PathPaymentStrictReceiveOpFrame, PathPaymentStrictSendOpFrame doApply)
    for f in [
        Fail::one(
            "path-receive-no-destination",
            &k.holder,
            receive(&k.ghost, &w.opn, UNIT, &w.opn, UNIT),
            "path_payment_strict_receive.no_destination",
            "a strict-receive path payment of OPN to an account that does not exist",
            PATH,
        ),
        Fail::one(
            "path-receive-no-trust",
            &k.holder,
            receive(&k.plain, &w.opn, UNIT, &w.opn, UNIT),
            "path_payment_strict_receive.no_trust",
            "a strict-receive path payment of OPN to an account without an OPN line",
            PATH,
        ),
        Fail::one(
            "path-receive-not-authorized",
            &k.holder,
            receive(&k.unauth, &w.aut, UNIT, &w.aut, UNIT),
            "path_payment_strict_receive.not_authorized",
            "a strict-receive path payment of AUT to an unauthorized line",
            PATH,
        ),
        Fail::one(
            "path-receive-line-full",
            &k.holder,
            receive(&k.full, &w.opn, 20 * UNIT, &w.opn, 20 * UNIT),
            "path_payment_strict_receive.line_full",
            "a strict-receive path payment of 20 OPN to a full line",
            PATH,
        ),
        Fail::one(
            "path-receive-src-no-trust",
            &k.plain,
            receive(&k.holder, &w.opn, UNIT, &w.opn, UNIT),
            "path_payment_strict_receive.src_no_trust",
            "a strict-receive path payment of OPN from an account without an OPN line",
            PATH,
        ),
        Fail::one(
            "path-receive-src-not-authorized",
            &k.deauth,
            receive(&k.holder, &w.aut, UNIT, &w.aut, UNIT),
            "path_payment_strict_receive.src_not_authorized",
            "a strict-receive path payment of AUT from a line at maintain-liabilities",
            PATH,
        ),
        Fail::one(
            "path-receive-underfunded",
            &k.holder,
            receive(&k.issuer_open, &w.opn, big, &w.opn, big),
            "path_payment_strict_receive.underfunded",
            "a strict-receive path payment of more OPN than the source holds, back to the issuer",
            PATH,
        ),
        Fail::one(
            "path-receive-too-few-offers",
            &k.holder,
            receive(&k.holder, &xlm, 10 * UNIT, &w.nob, UNIT),
            "path_payment_strict_receive.too_few_offers",
            "a strict-receive path payment from XLM to NOB, which no offer or pool sells",
            PATH,
        ),
        Fail::one(
            "path-receive-offer-cross-self",
            &k.trader,
            receive(&k.trader, &xlm, 10 * UNIT, &w.slf, UNIT),
            "path_payment_strict_receive.offer_cross_self",
            "a strict-receive path payment from XLM to SLF whose best offer is the source's own",
            PATH,
        ),
        Fail::one(
            "path-send-no-destination",
            &k.holder,
            send(&k.ghost, &w.opn, UNIT, &w.opn, UNIT),
            "path_payment_strict_send.no_destination",
            "a strict-send path payment of OPN to an account that does not exist",
            PATH,
        ),
        Fail::one(
            "path-send-no-trust",
            &k.holder,
            send(&k.plain, &w.opn, UNIT, &w.opn, UNIT),
            "path_payment_strict_send.no_trust",
            "a strict-send path payment of OPN to an account without an OPN line",
            PATH,
        ),
        Fail::one(
            "path-send-not-authorized",
            &k.holder,
            send(&k.unauth, &w.aut, UNIT, &w.aut, UNIT),
            "path_payment_strict_send.not_authorized",
            "a strict-send path payment of AUT to an unauthorized line",
            PATH,
        ),
        Fail::one(
            "path-send-line-full",
            &k.holder,
            send(&k.full, &w.opn, 20 * UNIT, &w.opn, 20 * UNIT),
            "path_payment_strict_send.line_full",
            "a strict-send path payment of 20 OPN to a full line",
            PATH,
        ),
        Fail::one(
            "path-send-src-no-trust",
            &k.plain,
            send(&k.holder, &w.opn, UNIT, &w.opn, UNIT),
            "path_payment_strict_send.src_no_trust",
            "a strict-send path payment of OPN from an account without an OPN line",
            PATH,
        ),
        Fail::one(
            "path-send-src-not-authorized",
            &k.deauth,
            send(&k.holder, &w.aut, UNIT, &w.aut, UNIT),
            "path_payment_strict_send.src_not_authorized",
            "a strict-send path payment of AUT from a line at maintain-liabilities",
            PATH,
        ),
        Fail::one(
            "path-send-underfunded",
            &k.holder,
            send(&k.issuer_open, &w.opn, big, &w.opn, big),
            "path_payment_strict_send.underfunded",
            "a strict-send path payment of more OPN than the source holds, back to the issuer",
            PATH,
        ),
        Fail::one(
            "path-send-too-few-offers",
            &k.holder,
            send(&k.holder, &xlm, UNIT, &w.nob, 1),
            "path_payment_strict_send.too_few_offers",
            "a strict-send path payment from XLM to NOB, which no offer or pool sells",
            PATH,
        ),
        Fail::one(
            "path-send-offer-cross-self",
            &k.trader,
            send(&k.trader, &xlm, UNIT, &w.slf, 1),
            "path_payment_strict_send.offer_cross_self",
            "a strict-send path payment from XLM to SLF whose best offer is the source's own",
            PATH,
        ),
        Fail::one(
            "path-send-under-destmin",
            &k.holder,
            send(&k.holder, &xlm, UNIT, &w.opn, 1000 * UNIT),
            "path_payment_strict_send.under_destmin",
            "a strict-send path payment of 1 XLM through the XLM/OPN pool asking for at least 1000 OPN",
            PATH,
        ),
    ] {
        f.run(ctx)?;
    }
    Ok(())
}

type OfferOp = fn(&Asset, &Asset, i64, Price, i64) -> OperationBody;

fn offers(ctx: &mut ChainCtx, w: &World) -> Result<()> {
    let k = &w.k;
    let xlm = Asset::Native;
    let absent = 1 << 40;
    // ManageOfferOpFrameBase::checkOfferValid runs first (selling line,
    // zero balance, selling authorization, buying line, buying
    // authorization), then not_found, the new entry's reserve, and the
    // liabilities against limit and balance
    for (op, make) in [("sell", sell as OfferOp), ("buy", buy as OfferOp)] {
        let code = |c: &str| format!("manage_{op}_offer.{c}");
        let slug = |s: &str| format!("manage-{op}-{s}");
        let cases = [
            (
                "buy-no-trust",
                &k.plain,
                make(&xlm, &w.opn, UNIT, ONE, 0),
                "buy_no_trust",
                "an offer buying OPN from an account without an OPN line",
            ),
            (
                "buy-not-authorized",
                &k.unauth,
                make(&xlm, &w.aut, UNIT, ONE, 0),
                "buy_not_authorized",
                "an offer buying AUT on a line its issuer never authorized",
            ),
            (
                "cross-self",
                &k.trader,
                make(&xlm, &w.slf, UNIT, ONE, 0),
                "cross_self",
                "an offer buying SLF at the price of the source's own SLF offer",
            ),
            (
                "line-full",
                &k.full,
                make(&xlm, &w.opn, UNIT, ONE, 0),
                "line_full",
                "an offer buying OPN on a line already at its limit",
            ),
            (
                "low-reserve",
                &k.poor,
                make(&xlm, &w.opn, UNIT, ONE, 0),
                "low_reserve",
                "a new offer from an account without the reserve for one more subentry",
            ),
            (
                "not-found",
                &k.holder,
                make(&xlm, &w.opn, UNIT, ONE, absent),
                "not_found",
                "an update of offer id 2^40, which the source does not have",
            ),
            (
                "sell-no-trust",
                &k.plain,
                make(&w.opn, &xlm, UNIT, ONE, 0),
                "sell_no_trust",
                "an offer selling OPN from an account without an OPN line",
            ),
            (
                "sell-not-authorized",
                &k.deauth,
                make(&w.aut, &xlm, UNIT, ONE, 0),
                "sell_not_authorized",
                "an offer selling AUT from a line holding 100 AUT at maintain-liabilities",
            ),
            (
                "underfunded",
                &k.poor,
                make(&w.opn, &xlm, UNIT, ONE, 0),
                "underfunded",
                "an offer selling OPN from a line holding none",
            ),
        ];
        for (s, source, body, c, notes) in cases {
            let (slug, want) = (slug(s), code(c));
            Fail::one(&slug, source, body, &want, notes, OFFER).run(ctx)?;
        }
    }
    Ok(())
}

fn accounts(ctx: &mut ChainCtx, w: &World) -> Result<()> {
    let k = &w.k;
    let merge = |to: &SigningKey| OperationBody::AccountMerge(muxed_account(&to.verifying_key()));
    let signer = ed25519_signer(&ctx.account("poor-signer").verifying_key());
    for f in [
        Fail::one(
            "create-account-already-exist",
            &k.holder,
            create(&k.plain, UNIT),
            "create_account.already_exist",
            "create account naming an account that exists",
            CREATE,
        ),
        Fail::one(
            "create-account-low-reserve",
            &k.holder,
            create(&k.ghost, 1),
            "create_account.low_reserve",
            "create account with a starting balance of one stroop, below the new account's reserve",
            CREATE,
        ),
        Fail::one(
            "create-account-underfunded",
            &k.poor,
            create(&k.ghost, 2 * BASE_RESERVE),
            "create_account.underfunded",
            "create account with 20 XLM, the new account's reserve, from an account 1 XLM above its own",
            CREATE,
        ),
        Fail::one(
            "merge-has-sub-entries",
            &k.holder,
            merge(&k.plain),
            "account_merge.has_sub_entries",
            "a merge of an account holding trust lines",
            MERGE,
        ),
        Fail::one(
            "merge-immutable-set",
            &k.immutable,
            merge(&k.plain),
            "account_merge.immutable_set",
            "a merge of an account with AUTH_IMMUTABLE",
            MERGE,
        ),
        Fail::one(
            "merge-is-sponsor",
            &k.sponsor,
            merge(&k.plain),
            "account_merge.is_sponsor",
            "a merge of an account that sponsors a trust line",
            MERGE,
        ),
        Fail::one(
            "merge-no-account",
            &k.plain,
            merge(&k.ghost),
            "account_merge.no_account",
            "a merge into an account that does not exist",
            MERGE,
        ),
        Fail::one(
            "manage-data-name-not-found",
            &k.plain,
            data("missing", None)?,
            "manage_data.name_not_found",
            "manage data deleting a name the account never set",
            DATA,
        ),
        Fail::one(
            "manage-data-low-reserve",
            &k.poor,
            data("poor", Some(b"no reserve"))?,
            "manage_data.low_reserve",
            "manage data adding an entry to an account without the reserve for it",
            DATA,
        ),
        Fail::one(
            "set-options-invalid-inflation",
            &k.plain,
            OperationBody::SetOptions(SetOptionsOp {
                inflation_dest: Some(account_id(&k.ghost.verifying_key())),
                ..set_options()
            }),
            "set_options.invalid_inflation",
            "set options naming an inflation destination that does not exist",
            &["op.set_options.inflation_dest"],
        ),
        Fail::one(
            "set-options-cant-change",
            &k.immutable,
            OperationBody::SetOptions(SetOptionsOp {
                set_flags: Some(AUTH_REQUIRED),
                ..set_options()
            }),
            "set_options.cant_change",
            "set options setting AUTH_REQUIRED on an account with AUTH_IMMUTABLE",
            SET_OPTIONS,
        ),
        Fail::one(
            "set-options-auth-revocable-required",
            &k.plain,
            OperationBody::SetOptions(SetOptionsOp {
                set_flags: Some(AUTH_CLAWBACK),
                ..set_options()
            }),
            "set_options.auth_revocable_required",
            "set options setting AUTH_CLAWBACK_ENABLED without AUTH_REVOCABLE",
            SET_OPTIONS,
        ),
        Fail::one(
            "set-options-low-reserve",
            &k.poor,
            OperationBody::SetOptions(SetOptionsOp {
                signer: Some(Signer {
                    key: signer,
                    weight: 1,
                }),
                ..set_options()
            }),
            "set_options.low_reserve",
            "set options adding a signer to an account without the reserve for it",
            &["op.set_options.signer"],
        ),
    ] {
        f.run(ctx)?;
    }
    Ok(())
}

fn trust_lines(ctx: &mut ChainCtx, w: &World) -> Result<()> {
    let k = &w.k;
    for f in [
        Fail::one(
            "change-trust-cannot-delete",
            &k.lp,
            trust_limit(&w.opn, 0)?,
            "change_trust.cannot_delete",
            "change trust deleting an empty OPN line the account's XLM/OPN pool shares use",
            TRUST,
        ),
        Fail::one(
            "change-trust-invalid-limit",
            &k.plain,
            trust_limit(&w.opn, 0)?,
            "change_trust.invalid_limit",
            "change trust creating a new line with a limit of zero",
            TRUST,
        ),
        Fail::one(
            "change-trust-low-reserve",
            &k.poor,
            trust(&w.nob)?,
            "change_trust.low_reserve",
            "change trust creating a line on an account without the reserve for it",
            TRUST,
        ),
        Fail::one(
            "change-trust-no-issuer",
            &k.plain,
            trust(&w.gho)?,
            "change_trust.no_issuer",
            "change trust to an asset whose issuer does not exist",
            TRUST,
        ),
        Fail::one(
            "change-trust-not-auth-maintain-liabilities",
            &k.unauth,
            trust_pool(&w.auth_pool, i64::MAX),
            "change_trust.not_auth_maintain_liabilities",
            "change trust to the XLM/AUT pool share over an AUT line that is not authorized",
            POOL_TRUST,
        ),
        Fail::one(
            "change-trust-trust-line-missing",
            &k.plain,
            trust_pool(&w.open_pool, i64::MAX),
            "change_trust.trust_line_missing",
            "change trust to the XLM/OPN pool share from an account without an OPN line",
            POOL_TRUST,
        ),
    ] {
        f.run(ctx)?;
    }
    Ok(())
}

fn trust_flags(ctx: &mut ChainCtx, w: &World) -> Result<()> {
    let k = &w.k;
    for f in [
        Fail::one(
            "allow-trust-cant-revoke",
            &k.issuer_open,
            allow(&k.holder, b"OPN", 0),
            "allow_trust.cant_revoke",
            "allow trust revoking an OPN line, whose issuer lacks AUTH_REVOCABLE",
            ALLOW,
        ),
        Fail::one(
            "allow-trust-no-trust-line",
            &k.issuer_auth,
            allow(&k.plain, b"AUT", AUTHORIZED),
            "allow_trust.no_trust_line",
            "allow trust authorizing an account without an AUT line",
            ALLOW,
        ),
        Fail::one(
            "set-trust-line-flags-cant-revoke",
            &k.issuer_open,
            flags(&k.holder, &w.opn, AUTHORIZED, 0),
            "set_trust_line_flags.cant_revoke",
            "set trust line flags clearing AUTHORIZED on an OPN line, whose issuer lacks AUTH_REVOCABLE",
            STLF,
        ),
        Fail::one(
            "set-trust-line-flags-invalid-state",
            &k.issuer_auth,
            flags(&k.deauth, &w.aut, 0, AUTHORIZED),
            "set_trust_line_flags.invalid_state",
            "set trust line flags adding AUTHORIZED to a line at maintain-liabilities, leaving both set",
            STLF,
        ),
        Fail::one(
            "set-trust-line-flags-no-trust-line",
            &k.issuer_auth,
            flags(&k.plain, &w.aut, 0, AUTHORIZED),
            "set_trust_line_flags.no_trust_line",
            "set trust line flags on an account without an AUT line",
            STLF,
        ),
    ] {
        f.run(ctx)?;
    }
    // revoking holder's AUT line redeems its XLM/AUT pool shares into
    // claimable balances; inside poor's sponsorship of holder the balances
    // need poor's reserve (removeOffersAndPoolShareTrustLines,
    // TransactionUtils.cpp)
    for (slug, op, want, notes, fields) in [
        (
            "allow-trust-low-reserve",
            allow(&k.holder, b"AUT", 0),
            "allow_trust.low_reserve",
            "allow trust revoking a line with pool shares while an account without reserve sponsors the holder, so the claimable balances of the redeemed shares cannot be created",
            ALLOW,
        ),
        (
            "set-trust-line-flags-low-reserve",
            flags(&k.holder, &w.aut, AUTHORIZED, 0),
            "set_trust_line_flags.low_reserve",
            "set trust line flags revoking a line with pool shares while an account without reserve sponsors the holder, so the claimable balances of the redeemed shares cannot be created",
            STLF,
        ),
    ] {
        Fail {
            slug,
            source: &k.holder,
            ops: vec![
                (Some(&k.poor), begin(&k.holder)),
                (Some(&k.issuer_auth), op),
                (None, OperationBody::EndSponsoringFutureReserves),
            ],
            cosigners: vec![&k.poor, &k.issuer_auth],
            at: 1,
            want,
            notes,
            fields,
        }
        .run(ctx)?;
    }
    Ok(())
}

fn claimable_balances(ctx: &mut ChainCtx, w: &World) -> Result<()> {
    let k = &w.k;
    let claim = |id: &ClaimableBalanceId| {
        OperationBody::ClaimClaimableBalance(ClaimClaimableBalanceOp {
            balance_id: id.clone(),
        })
    };
    let claw = |id: &ClaimableBalanceId| {
        OperationBody::ClawbackClaimableBalance(ClawbackClaimableBalanceOp {
            balance_id: id.clone(),
        })
    };
    let clawback = |from: &SigningKey, amount: i64| {
        OperationBody::Clawback(ClawbackOp {
            asset: w.aut.clone(),
            from: muxed_account(&from.verifying_key()),
            amount,
        })
    };
    let absent = absent_balance();
    for f in [
        Fail::one(
            "claim-does-not-exist",
            &k.plain,
            claim(&absent),
            "claim_claimable_balance.does_not_exist",
            "a claim of a balance id no transaction created",
            CB_CLAIM,
        ),
        Fail::one(
            "claim-no-trust",
            &k.plain,
            claim(&w.opn_balance),
            "claim_claimable_balance.no_trust",
            "a claim of an OPN balance by a claimant without an OPN line",
            CB_CLAIM,
        ),
        Fail::one(
            "claim-line-full",
            &k.full,
            claim(&w.opn_balance),
            "claim_claimable_balance.line_full",
            "a claim of 20 OPN by a claimant whose OPN line is at its limit",
            CB_CLAIM,
        ),
        Fail::one(
            "claim-not-authorized",
            &k.unauth,
            claim(&w.aut_balance),
            "claim_claimable_balance.not_authorized",
            "a claim of an AUT balance by a claimant whose AUT line is not authorized",
            CB_CLAIM,
        ),
        Fail::one(
            "create-claimable-balance-low-reserve",
            &k.poor,
            claimable(&Asset::Native, 1, &[&k.plain])?,
            "create_claimable_balance.low_reserve",
            "a claimable balance of one stroop from an account without the reserve for it",
            CB_CREATE,
        ),
        Fail::one(
            "create-claimable-balance-not-authorized",
            &k.deauth,
            claimable(&w.aut, UNIT, &[&k.plain])?,
            "create_claimable_balance.not_authorized",
            "a claimable balance of AUT from a line at maintain-liabilities",
            CB_CREATE,
        ),
        Fail::one(
            "create-claimable-balance-underfunded",
            &k.poor,
            claimable(&Asset::Native, 10 * UNIT, &[&k.plain])?,
            "create_claimable_balance.underfunded",
            "a claimable balance of 10 XLM from an account 1 XLM above its reserve",
            CB_CREATE,
        ),
        Fail::one(
            "clawback-claimable-balance-does-not-exist",
            &k.issuer_auth,
            claw(&absent),
            "clawback_claimable_balance.does_not_exist",
            "a clawback of a balance id no transaction created",
            CB_CLAWBACK,
        ),
        Fail::one(
            "clawback-claimable-balance-not-clawback-enabled",
            &k.issuer_open,
            claw(&w.opn_balance),
            "clawback_claimable_balance.not_clawback_enabled",
            "the OPN issuer clawing back an OPN balance created without clawback",
            CB_CLAWBACK,
        ),
        Fail::one(
            "clawback-claimable-balance-not-issuer",
            &k.issuer_auth,
            claw(&w.opn_balance),
            "clawback_claimable_balance.not_issuer",
            "the AUT issuer clawing back an OPN balance",
            CB_CLAWBACK,
        ),
        Fail::one(
            "clawback-no-trust",
            &k.issuer_auth,
            clawback(&k.plain, UNIT),
            "clawback.no_trust",
            "a clawback of AUT from an account without an AUT line",
            CLAWBACK,
        ),
        Fail::one(
            "clawback-underfunded",
            &k.issuer_auth,
            clawback(&k.holder, 2000 * UNIT),
            "clawback.underfunded",
            "a clawback of 2000 AUT from a line holding 900",
            CLAWBACK,
        ),
    ] {
        f.run(ctx)?;
    }
    Ok(())
}

fn pools(ctx: &mut ChainCtx, w: &World) -> Result<()> {
    let k = &w.k;
    let open = pool_id(&w.open_pool)?;
    let auth = pool_id(&w.auth_pool)?;
    let big = pool_id(&w.big_pool)?;
    for f in [
        Fail::one(
            "deposit-no-trust",
            &k.plain,
            deposit(&open, UNIT, UNIT, HALF, TWICE),
            "liquidity_pool_deposit.no_trust",
            "a deposit into the XLM/OPN pool from an account without its pool share line",
            DEPOSIT,
        ),
        Fail::one(
            "deposit-not-authorized",
            &k.deauth,
            deposit(&auth, UNIT, UNIT, HALF, TWICE),
            "liquidity_pool_deposit.not_authorized",
            "a deposit into the XLM/AUT pool over an AUT line at maintain-liabilities",
            DEPOSIT,
        ),
        Fail::one(
            "deposit-underfunded",
            &k.lp_capped,
            deposit(&open, 200 * UNIT, 200 * UNIT, HALF, TWICE),
            "liquidity_pool_deposit.underfunded",
            "a deposit of 200 OPN from a line holding 100",
            DEPOSIT,
        ),
        Fail::one(
            "deposit-bad-price",
            &k.lp_capped,
            deposit(
                &open,
                10 * UNIT,
                10 * UNIT,
                Price { n: 2, d: 1 },
                Price { n: 3, d: 1 },
            ),
            "liquidity_pool_deposit.bad_price",
            "a deposit at the pool's price of one asking for a price between 2 and 3",
            DEPOSIT,
        ),
        Fail::one(
            "deposit-line-full",
            &k.lp_capped,
            deposit(&open, 10 * UNIT, 10 * UNIT, HALF, TWICE),
            "liquidity_pool_deposit.line_full",
            "a deposit whose shares exceed the pool share line's limit of one stroop",
            DEPOSIT,
        ),
        Fail::one(
            "deposit-pool-full",
            &k.whale,
            deposit(&big, UNIT, UNIT, ONE, ONE),
            "liquidity_pool_deposit.pool_full",
            "a deposit into the OPN/SLF pool, whose reserves are both i64::MAX",
            DEPOSIT,
        ),
        Fail::one(
            "withdraw-no-trust",
            &k.plain,
            withdraw(&open, UNIT, 0, 0),
            "liquidity_pool_withdraw.no_trust",
            "a withdrawal from the XLM/OPN pool by an account without its pool share line",
            WITHDRAW,
        ),
        Fail::one(
            "withdraw-underfunded",
            &k.lp,
            withdraw(&open, 10_000 * UNIT, 0, 0),
            "liquidity_pool_withdraw.underfunded",
            "a withdrawal of more pool shares than the line holds",
            WITHDRAW,
        ),
        Fail::one(
            "withdraw-under-minimum",
            &k.lp,
            withdraw(&open, UNIT, 1000 * UNIT, 0),
            "liquidity_pool_withdraw.under_minimum",
            "a withdrawal of one share asking for at least 1000 XLM",
            WITHDRAW,
        ),
    ] {
        f.run(ctx)?;
    }
    // with the OPN line down to a limit of one stroop the withdrawn OPN
    // no longer fits
    ok(ctx, &k.lp, mine(vec![trust_limit(&w.opn, 1)?]), &[])?;
    Fail::one(
        "withdraw-line-full",
        &k.lp,
        withdraw(&open, 10 * UNIT, 0, 0),
        "liquidity_pool_withdraw.line_full",
        "a withdrawal whose OPN does not fit the OPN line's limit of one stroop",
        WITHDRAW,
    )
    .run(ctx)
}

fn sponsorship(ctx: &mut ChainCtx, w: &World) -> Result<()> {
    let k = &w.k;
    let revoke =
        |key: LedgerKey| OperationBody::RevokeSponsorship(RevokeSponsorshipOp::LedgerEntry(key));
    for f in [
        Fail::one(
            "revoke-does-not-exist",
            &k.plain,
            revoke(data_key(&k.plain, "missing")?),
            "revoke_sponsorship.does_not_exist",
            "a sponsorship revoke of a data entry that does not exist",
            REVOKE,
        ),
        Fail::one(
            "revoke-low-reserve",
            &k.sponsor,
            revoke(trustline_key(&k.poor, &w.aut)?),
            "revoke_sponsorship.low_reserve",
            "a sponsorship revoke handing a trust line back to an account without the reserve for it",
            REVOKE,
        ),
        Fail::one(
            "revoke-not-sponsor",
            &k.plain,
            revoke(trustline_key(&k.holder, &w.opn)?),
            "revoke_sponsorship.not_sponsor",
            "a sponsorship revoke of another account's unsponsored trust line",
            REVOKE,
        ),
        Fail::one(
            "revoke-only-transferable",
            &k.holder,
            revoke(balance_key(&w.opn_balance)),
            "revoke_sponsorship.only_transferable",
            "a sponsorship revoke of the source's own claimable balance outside a sponsorship, which can only be transferred",
            REVOKE,
        ),
    ] {
        f.run(ctx)?;
    }
    Ok(())
}

/// The account's entry as the meta of one transaction last wrote it.
fn account_in_meta<'m>(
    meta: &'m TransactionMeta,
    who: &SigningKey,
) -> Result<&'m stellar_xdr::AccountEntry> {
    let TransactionMeta::V4(m) = meta else {
        bail!("transaction meta v4 expected");
    };
    let id = account_id(&who.verifying_key());
    let mut found = None;
    for c in m.operations.iter().flat_map(|o| o.changes.iter()) {
        if let LedgerEntryChange::Created(e) | LedgerEntryChange::Updated(e) = c
            && let LedgerEntryData::Account(a) = &e.data
            && a.account_id == id
        {
            found = Some(a);
        }
    }
    found.context("the meta does not write the account")
}

/// A passive offer placed by an operation source that never sources a
/// transaction: its liabilities add AccountEntryExtensionV1 to the entry,
/// and with no sponsorship and no sequence bump of its own it never gets
/// v2 (prepareAccountEntryExtensionV1, maybeUpdateAccountOnLedgerSeqUpdate,
/// stellar-core v30.0.0).
fn passive_offer_by_operation_source(ctx: &mut ChainCtx, w: &World) -> Result<()> {
    let k = &w.k;
    let tx = ctx
        .tx(&k.holder)?
        .operation(
            Some(muxed_account(&k.maker.verifying_key())),
            trust(&w.opn)?,
        )
        .operation(
            Some(muxed_account(&k.maker.verifying_key())),
            OperationBody::CreatePassiveSellOffer(CreatePassiveSellOfferOp {
                selling: Asset::Native,
                buying: w.opn.clone(),
                amount: 10 * UNIT,
                // far above the pool's price of about one, so nothing crosses
                price: Price { n: 1000, d: 1 },
            }),
        )
        .build()?;
    let env = ctx.sign(tx, &[&k.holder, &k.maker])?;
    let applied = ctx.submit_and_close(&env)?;
    let ops = op_results(&applied.result.result)?;
    let got = op_code(ops.get(1).context("no result for the passive offer")?)?;
    // core writes a passive offer's result under the manage sell offer
    // arm: the frame applies a ManageSellOffer operation it builds from the
    // passive one, and the result arm follows that operation's type
    // (ManageSellOfferOpHolder, MutableTransactionResult.cpp, v30.0.0)
    ensure!(
        got == "manage_sell_offer.success",
        "the passive offer answered {got}"
    );
    let maker = account_in_meta(&applied.meta, &k.maker)?;
    match &maker.ext {
        AccountEntryExt::V1(v1) => ensure!(
            matches!(v1.ext, AccountEntryExtensionV1Ext::V0) && v1.liabilities.selling > 0,
            "maker's entry is {:?}, expected extension v1 with v0 and selling liabilities",
            maker.ext
        ),
        other => bail!("maker's entry has extension {other:?}, expected v1"),
    }
    ctx.capture(
        "passive-offer-operation-source",
        Selector::ledger_of(&applied),
        Spec::new(
            "a passive sell offer placed by an operation source that has never sourced a transaction: core reports its result under the manage_sell_offer arm, and the source's entry gains AccountEntryExtensionV1 with its own extension at v0",
        )
        .fields(&["op.source_account", "op.manage_offer.assets_amount_price"]),
    )
}

/// A data entry removed: the meta's removed change carries LedgerKey:data.
fn data_removed(ctx: &mut ChainCtx, w: &World) -> Result<()> {
    let k = &w.k;
    ok(
        ctx,
        &k.holder,
        mine(vec![data("removed", Some(b"for a while"))?]),
        &[],
    )?;
    let env = ctx.classic(&k.holder, vec![data("removed", None)?])?;
    let applied = ctx.submit_and_close(&env)?;
    let key = data_key(&k.holder, "removed")?;
    let TransactionMeta::V4(m) = &applied.meta else {
        bail!("transaction meta v4 expected");
    };
    ensure!(
        m.operations
            .iter()
            .flat_map(|o| o.changes.iter())
            .any(|c| matches!(c, LedgerEntryChange::Removed(r) if *r == key)),
        "the deletion's meta does not remove the data entry"
    );
    let got = ctx.driver().node().ledger_entries(&[key])?;
    ensure!(
        got.entries[0].entry.is_none(),
        "the data entry still exists after its deletion"
    );
    ctx.capture(
        "manage-data-removed",
        Selector::ledger_of(&applied),
        Spec::new(
            "manage data with no value deleting an existing entry, so the meta holds a removed change keyed by LedgerKey:data",
        )
        .fields(DATA),
    )
}

/// Offers of one unit each, more than a path payment may cross: core stops
/// at `MAX_OFFERS_TO_CROSS` (1000) and fails the operation with
/// op_exceeded_work_limit (PathPaymentOpFrameBase::convert, stellar-core
/// v30.0.0). An account holds at most 1000 subentries, so two makers share
/// them; one 100-operation transaction fills each ledger.
fn work_limit(ctx: &mut ChainCtx, w: &World) -> Result<()> {
    const PER_TX: usize = 100;
    const CROWD: [(&str, usize); 2] = [("crowd-0", 6), ("crowd-1", 5)];
    let k = &w.k;
    let crw = credit(b"CRW", &k.issuer_open);
    let roles: Vec<&str> = CROWD.iter().map(|(r, _)| *r).collect();
    ctx.fund(&roles, BALANCE)?;
    let makers: Vec<(SigningKey, usize)> = CROWD
        .iter()
        .map(|(r, txs)| (ctx.account(r), *txs))
        .collect();
    let mut offers = 0;
    for (maker, txs) in &makers {
        ok(ctx, maker, mine(vec![trust(&crw)?]), &[])?;
        let units = (txs * PER_TX) as i64;
        ok(
            ctx,
            &k.issuer_open,
            mine(vec![pay(maker, &crw, units * UNIT)]),
            &[],
        )?;
        for _ in 0..*txs {
            let ops = (0..PER_TX)
                .map(|_| sell(&crw, &Asset::Native, UNIT, ONE, 0))
                .collect();
            ok(ctx, maker, mine(ops), &[])?;
            offers += PER_TX;
        }
    }
    ensure!(offers > 1000, "only {offers} offers, the limit is 1000");
    ok(ctx, &k.holder, mine(vec![trust(&crw)?]), &[])?;
    let wanted = (offers as i64 - 50) * UNIT;
    Fail::one(
        "path-receive-exceeded-work-limit",
        &k.holder,
        receive(&k.holder, &Asset::Native, 2 * wanted, &crw, wanted),
        "op_exceeded_work_limit",
        "a strict-receive path payment from XLM to CRW needing 1050 of the 1100 one-unit CRW offers on the book, more than the 1000 core lets one operation cross",
        PATH,
    )
    .run(ctx)
}
