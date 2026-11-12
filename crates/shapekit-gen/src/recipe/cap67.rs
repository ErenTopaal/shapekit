//! `cap67.unified-events`: CAP-67 asset events from classic operations
//! (transfer, mint, burn, clawback, set_authorized), the transaction-level
//! fee events, and the `{amount, to_muxed_id}` map data written for a muxed
//! destination or for a destination account paid with a memo.
//!
//! Three chains run the same classic steps:
//!
//! - protocol 30, `EMIT_CLASSIC_EVENTS` on as on the public networks, plus
//!   one Soroban invocation for the refund stage of the fee events and an
//!   issuer transfer through the asset's SAC to a muxed contract (a mint
//!   with `to_muxed_id`)
//! - protocol 22 with both event flags off: the meta core wrote for those
//!   operations in protocol 22's day, TransactionMeta v3 with no events
//! - protocol 22 with `BACKFILL_STELLAR_ASSET_EVENTS`: TransactionMeta v4
//!   with the same events, which is what a current core replaying old
//!   history emits (and what the public data lake holds for old ledgers)
//!
//! The protocol 22 chains sit below the family's valid range and their
//! entries are flagged `below-protocol-valid`. Every capture is a whole
//! ledger: the envelope of a classic operation holds a `MuxedAccount`, not
//! an `ScAddress`, so the shapes of this family live in the meta.

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use shapekit_core_driver::chain::Applied;
use shapekit_core_driver::keys::{
    account_address, account_id, muxed_account, muxed_account_with_id,
};
use stellar_xdr::{
    AccountFlags, Asset, ChangeTrustAsset, ChangeTrustOp, ClaimClaimableBalanceOp, ClaimPredicate,
    ClaimableBalanceId, Claimant, ClaimantV0, ClawbackClaimableBalanceOp, ClawbackOp,
    ContractEvent, ContractEventBody, CreateClaimableBalanceResult, Hash, Limits,
    LiquidityPoolConstantProductParameters, LiquidityPoolDepositOp, LiquidityPoolParameters, Memo,
    MuxedAccount, MuxedContract, OperationBody, OperationResult, OperationResultTr,
    PathPaymentStrictReceiveOp, PaymentOp, PoolId, Price, ScAddress, ScVal, SetOptionsOp,
    SetTrustLineFlagsOp, TransactionEventStage, TransactionMeta, TransactionResultResult,
    TrustLineFlags, WriteXdr,
};

use super::cap73::{call, credit, deploy_sac, i128_val};
use super::scval::{BALANCE, deploy_echo};
use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "cap67.unified-events";

/// Liquidity pool fee in basis points, the only one core accepts.
const POOL_FEE: i32 = 30;

/// Units of the credit asset, in stroops of 10^-7.
const UNIT: i64 = 10_000_000;

/// Which events the chain's node emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Events {
    /// Classic events as the public networks emit them from protocol 23.
    Live,
    /// Both flags off, the meta of a pre-23 ledger in its day.
    PreCap67,
    /// `BACKFILL_STELLAR_ASSET_EVENTS`: v4 meta and events below 23.
    Backfill,
}

pub struct UnifiedEvents {
    pub protocol: u32,
    pub events: Events,
}

impl UnifiedEvents {
    fn slug(&self, s: &str) -> String {
        match self.events {
            Events::Backfill => format!("backfill-{s}"),
            Events::Live | Events::PreCap67 => s.to_owned(),
        }
    }

    fn emits(&self) -> bool {
        self.events != Events::PreCap67
    }
}

fn first_topic(e: &ContractEvent) -> Option<String> {
    let ContractEventBody::V0(b) = &e.body;
    match b.topics.first() {
        Some(ScVal::Symbol(s)) => Some(String::from_utf8_lossy(s.as_slice()).into_owned()),
        _ => None,
    }
}

/// The data of the operation event named `name`.
fn event_data(applied: &Applied, name: &str) -> Result<ScVal> {
    let TransactionMeta::V4(m) = &applied.meta else {
        bail!("ledger {}: no v4 meta", applied.ledger);
    };
    m.operations
        .iter()
        .flat_map(|o| o.events.iter())
        .find(|e| first_topic(e).as_deref() == Some(name))
        .map(|e| {
            let ContractEventBody::V0(b) = &e.body;
            b.data.clone()
        })
        .with_context(|| format!("ledger {}: no {name} event", applied.ledger))
}

/// Checks the meta of a classic transaction against the chain's mode: no
/// events and v3 meta before CAP-67; otherwise v4 meta whose operation
/// events are named `ops` in order and whose transaction events start
/// with the fee charge.
fn ensure_events(applied: &Applied, emits: bool, ops: &[&str]) -> Result<()> {
    let l = applied.ledger;
    if !emits {
        ensure!(
            matches!(applied.meta, TransactionMeta::V3(_)),
            "ledger {l}: expected TransactionMeta v3 with events off, got v{}",
            meta_version(&applied.meta)
        );
        return Ok(());
    }
    let TransactionMeta::V4(m) = &applied.meta else {
        bail!(
            "ledger {l}: expected TransactionMeta v4, got v{}",
            meta_version(&applied.meta)
        );
    };
    let names: Vec<String> = m
        .operations
        .iter()
        .flat_map(|o| o.events.iter())
        .map(|e| first_topic(e).unwrap_or_default())
        .collect();
    ensure!(
        names == ops,
        "ledger {l}: operation events {names:?}, expected {ops:?}"
    );
    let fee = m.events.first().context("no transaction events")?;
    ensure!(
        fee.stage == TransactionEventStage::BeforeAllTxs
            && first_topic(&fee.event).as_deref() == Some("fee"),
        "ledger {l}: the first transaction event is not the fee charge: {fee:?}"
    );
    Ok(())
}

fn meta_version(m: &TransactionMeta) -> u32 {
    match m {
        TransactionMeta::V0(_) => 0,
        TransactionMeta::V1(_) => 1,
        TransactionMeta::V2(_) => 2,
        TransactionMeta::V3(_) => 3,
        TransactionMeta::V4(_) => 4,
    }
}

fn payment(to: MuxedAccount, asset: Asset, amount: i64) -> OperationBody {
    OperationBody::Payment(PaymentOp {
        destination: to,
        asset,
        amount,
    })
}

fn change_trust(line: ChangeTrustAsset) -> OperationBody {
    OperationBody::ChangeTrust(ChangeTrustOp {
        line,
        limit: i64::MAX,
    })
}

fn line(asset: &Asset) -> Result<ChangeTrustAsset> {
    Ok(match asset {
        Asset::CreditAlphanum4(a) => ChangeTrustAsset::CreditAlphanum4(a.clone()),
        other => bail!("not a four letter credit asset: {other:?}"),
    })
}

fn set_flags(trustor: &SigningKey, asset: &Asset, clear: u32, set: u32) -> OperationBody {
    OperationBody::SetTrustLineFlags(SetTrustLineFlagsOp {
        trustor: account_id(&trustor.verifying_key()),
        asset: asset.clone(),
        clear_flags: clear,
        set_flags: set,
    })
}

fn create_balance(asset: &Asset, amount: i64, to: &SigningKey) -> Result<OperationBody> {
    Ok(OperationBody::CreateClaimableBalance(
        stellar_xdr::CreateClaimableBalanceOp {
            asset: asset.clone(),
            amount,
            claimants: vec![Claimant::ClaimantTypeV0(ClaimantV0 {
                destination: account_id(&to.verifying_key()),
                predicate: ClaimPredicate::Unconditional,
            })]
            .try_into()?,
        },
    ))
}

fn balance_id(applied: &Applied) -> Result<ClaimableBalanceId> {
    if let TransactionResultResult::TxSuccess(ops) = &applied.result.result
        && let Some(OperationResult::OpInner(OperationResultTr::CreateClaimableBalance(
            CreateClaimableBalanceResult::Success(id),
        ))) = ops.first()
    {
        return Ok(id.clone());
    }
    bail!(
        "ledger {}: no claimable balance id in {:?}",
        applied.ledger,
        applied.result.result
    )
}

/// Id of the constant product pool of `a` and `b` (in core's order).
fn pool_id(a: &Asset, b: &Asset) -> Result<PoolId> {
    let params = LiquidityPoolParameters::LiquidityPoolConstantProduct(
        LiquidityPoolConstantProductParameters {
            asset_a: a.clone(),
            asset_b: b.clone(),
            fee: POOL_FEE,
        },
    );
    Ok(PoolId(Hash(
        Sha256::digest(params.to_xdr(Limits::none())?).into(),
    )))
}

fn memo_text(s: &str) -> Result<Memo> {
    Ok(Memo::Text(s.as_bytes().to_vec().try_into()?))
}

impl Recipe for UnifiedEvents {
    fn id(&self) -> &str {
        match self.events {
            Events::Backfill => "cap67.unified-events-backfill",
            Events::Live | Events::PreCap67 => FAMILY,
        }
    }

    fn family(&self) -> &str {
        FAMILY
    }

    fn protocol(&self) -> u32 {
        self.protocol
    }

    fn soroban_settings(&self) -> bool {
        self.events == Events::Live
    }

    fn emit_classic_events(&self) -> bool {
        self.emits()
    }

    fn backfill_stellar_asset_events(&self) -> bool {
        self.emits()
    }

    fn below_protocol_valid(&self) -> bool {
        self.events != Events::Live
    }

    fn run(&self, ctx: &mut ChainCtx) -> Result<()> {
        ensure!(
            (self.events == Events::Live) == (self.protocol >= 23),
            "events mode {:?} does not fit protocol {}",
            self.events,
            self.protocol
        );
        let emits = self.emits();
        let funded = ctx.fund(&["issuer", "alice", "bob", "carol"], BALANCE)?;
        if emits {
            // create_account moves lumens out of root: a transfer event
            ensure_events(&funded, emits, &["transfer"; 4])?;
        }
        let issuer = ctx.account("issuer");
        let alice = ctx.account("alice");
        let bob = ctx.account("bob");
        let carol = ctx.account("carol");
        let asset = credit(b"UNIF", &issuer);
        let mux = |k: &SigningKey| muxed_account(&k.verifying_key());
        let note = |s: &str| {
            Spec::new(if emits {
                s.to_owned()
            } else {
                format!(
                    "{s}. recorded at protocol 22 with classic events off, so the meta is v3 and carries none of them"
                )
            })
        };

        // clawback needs revocable; trustlines opened afterwards inherit the
        // clawback flag
        let flags = AccountFlags::RevocableFlag as u32 | AccountFlags::ClawbackEnabledFlag as u32;
        let env = ctx.classic(
            &issuer,
            vec![OperationBody::SetOptions(SetOptionsOp {
                inflation_dest: None,
                clear_flags: None,
                set_flags: Some(flags),
                master_weight: None,
                low_threshold: None,
                med_threshold: None,
                high_threshold: None,
                home_domain: None,
                signer: None,
            })],
        )?;
        ctx.submit_and_close(&env)?;
        let tx = ctx
            .tx(&alice)?
            .operation(None, change_trust(line(&asset)?))
            .operation(Some(mux(&bob)), change_trust(line(&asset)?))
            .build()?;
        let env = ctx.sign(tx, &[&alice, &bob])?;
        ctx.submit_and_close(&env)?;

        let env = ctx.classic(
            &issuer,
            vec![payment(mux(&alice), asset.clone(), 1000 * UNIT)],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ensure_events(&applied, emits, &["mint"])?;
        ctx.capture(
            &self.slug("payment-mint"),
            Selector::ledger_of(&applied),
            note("a payment from the issuer, which CAP-67 reports as a mint event")
                .fields(&["op.payment.destination", "op.payment.asset"]),
        )?;

        let env = ctx.classic(&alice, vec![payment(mux(&bob), asset.clone(), 100 * UNIT)])?;
        let applied = ctx.submit_and_close(&env)?;
        ensure_events(&applied, emits, &["transfer"])?;
        ctx.capture(
            &self.slug("payment-transfer"),
            Selector::ledger_of(&applied),
            note("a credit payment between two holders, a transfer event with plain amount data")
                .fields(&["op.payment.destination", "op.payment.asset"]),
        )?;

        let env = ctx.classic(&bob, vec![payment(mux(&issuer), asset.clone(), 10 * UNIT)])?;
        let applied = ctx.submit_and_close(&env)?;
        ensure_events(&applied, emits, &["burn"])?;
        ctx.capture(
            &self.slug("payment-burn"),
            Selector::ledger_of(&applied),
            note("a payment back to the issuer, which CAP-67 reports as a burn event")
                .fields(&["op.payment.destination", "op.payment.asset"]),
        )?;

        // core applies a payment to oneself without a ledger change
        let env = ctx.classic(&alice, vec![payment(mux(&alice), Asset::Native, UNIT)])?;
        let applied = ctx.submit_and_close(&env)?;
        ensure_events(&applied, emits, &["transfer"])?;
        ensure!(
            super::cap73::operation_changes(&applied.meta).is_empty(),
            "ledger {}: a self payment changed ledger entries",
            applied.ledger
        );
        ctx.capture(
            &self.slug("payment-to-self"),
            Selector::ledger_of(&applied),
            note("an XLM payment from an account to itself: no ledger entry changes, yet a transfer event whose from and to are the same account")
                .fields(&["op.payment.destination"]),
        )?;

        let mux_id = 1u64 << 53;
        let to = muxed_account_with_id(&bob.verifying_key(), mux_id);
        let env = ctx.classic(&alice, vec![payment(to, Asset::Native, 5 * UNIT)])?;
        let applied = ctx.submit_and_close(&env)?;
        ensure_events(&applied, emits, &["transfer"])?;
        if emits {
            ensure_muxed_data(&applied, "transfer", &ScVal::U64(mux_id))?;
        }
        ctx.capture(
            &self.slug("payment-muxed-destination"),
            Selector::ledger_of(&applied),
            note("an XLM payment to a muxed account with mux id 2^53; the transfer event data is {amount, to_muxed_id: u64}")
                .fields(&["op.payment.destination", "op.payment.destination.mux_id"]),
        )?;

        let memos: [(&str, Memo, ScVal, &str); 3] = [
            (
                "memo-id",
                Memo::Id(u64::MAX),
                ScVal::U64(u64::MAX),
                "tx.memo.id",
            ),
            (
                "memo-text",
                memo_text("unified events memo")?,
                ScVal::String(stellar_xdr::ScString("unified events memo".try_into()?)),
                "tx.memo.text",
            ),
            (
                "memo-hash",
                Memo::Hash(Hash(Sha256::digest(b"shapekit/cap67/memo-hash").into())),
                ScVal::Bytes(stellar_xdr::ScBytes(
                    Sha256::digest(b"shapekit/cap67/memo-hash")
                        .to_vec()
                        .try_into()?,
                )),
                "tx.memo.hash",
            ),
        ];
        for (slug, memo, want, field) in memos {
            let tx = ctx
                .tx(&alice)?
                .memo(memo)
                .operation(None, payment(mux(&bob), Asset::Native, UNIT))
                .build()?;
            let env = ctx.sign(tx, &[&alice])?;
            let applied = ctx.submit_and_close(&env)?;
            ensure_events(&applied, emits, &["transfer"])?;
            if emits {
                ensure_muxed_data(&applied, "transfer", &want)?;
            }
            ctx.capture(
                &self.slug(&format!("payment-{slug}")),
                Selector::ledger_of(&applied),
                note(&format!(
                    "an XLM payment to a plain account in a transaction with a {slug}: the memo value becomes to_muxed_id in the transfer event data"
                ))
                .fields(&[field, "tx.memo.type", "op.payment.destination"]),
            )?;
        }

        let mux_id = 7u64;
        let to = muxed_account_with_id(&bob.verifying_key(), mux_id);
        let env = ctx.classic(
            &alice,
            vec![OperationBody::PathPaymentStrictReceive(
                PathPaymentStrictReceiveOp {
                    send_asset: asset.clone(),
                    send_max: 3 * UNIT,
                    destination: to,
                    dest_asset: asset.clone(),
                    dest_amount: 3 * UNIT,
                    path: Default::default(),
                },
            )],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ensure_events(&applied, emits, &["transfer"])?;
        if emits {
            ensure_muxed_data(&applied, "transfer", &ScVal::U64(mux_id))?;
        }
        ctx.capture(
            &self.slug("path-payment-muxed-destination"),
            Selector::ledger_of(&applied),
            note("a path payment strict receive of a credit asset to a muxed account, a transfer event with to_muxed_id")
                .fields(&[
                    "op.path_payment.destination",
                    "op.path_payment.destination.mux_id",
                ]),
        )?;

        let env = ctx.classic(
            &issuer,
            vec![OperationBody::Clawback(ClawbackOp {
                asset: asset.clone(),
                from: mux(&alice),
                amount: 5 * UNIT,
            })],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ensure_events(&applied, emits, &["clawback"])?;
        ctx.capture(
            &self.slug("clawback"),
            Selector::ledger_of(&applied),
            note("the issuer claws back part of a holder's balance, a clawback event")
                .fields(&["op.clawback.from", "op.clawback.asset_amount"]),
        )?;

        let authorized = TrustLineFlags::AuthorizedFlag as u32;
        for (slug, clear, set) in [
            ("set-authorized-false", authorized, 0),
            ("set-authorized-true", 0, authorized),
        ] {
            let env = ctx.classic(&issuer, vec![set_flags(&bob, &asset, clear, set)])?;
            let applied = ctx.submit_and_close(&env)?;
            ensure_events(&applied, emits, &["set_authorized"])?;
            ctx.capture(
                &self.slug(slug),
                Selector::ledger_of(&applied),
                note(&format!(
                    "SetTrustLineFlags {} the authorized flag on a trustline, a set_authorized event with bool data",
                    if set == 0 { "clears" } else { "sets" }
                ))
                .fields(&["op.set_trust_line_flags"]),
            )?;
        }

        let env = ctx.classic(&alice, vec![create_balance(&asset, 20 * UNIT, &bob)?])?;
        let applied = ctx.submit_and_close(&env)?;
        ensure_events(&applied, emits, &["transfer"])?;
        let claimed = balance_id(&applied)?;
        ctx.capture(
            &self.slug("create-claimable-balance"),
            Selector::ledger_of(&applied),
            note("a claimable balance created from a holder: a transfer event to the claimable balance address (strkey B)")
                .fields(&["op.create_claimable_balance.asset_amount"]),
        )?;
        let env = ctx.classic(
            &bob,
            vec![OperationBody::ClaimClaimableBalance(
                ClaimClaimableBalanceOp {
                    balance_id: claimed,
                },
            )],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ensure_events(&applied, emits, &["transfer"])?;
        ctx.capture(
            &self.slug("claim-claimable-balance"),
            Selector::ledger_of(&applied),
            note("claiming the balance: a transfer event from the claimable balance address")
                .fields(&["op.claim_claimable_balance.balance_id"]),
        )?;

        let env = ctx.classic(&alice, vec![create_balance(&asset, 4 * UNIT, &bob)?])?;
        let applied = ctx.submit_and_close(&env)?;
        let clawed = balance_id(&applied)?;
        let env = ctx.classic(
            &issuer,
            vec![OperationBody::ClawbackClaimableBalance(
                ClawbackClaimableBalanceOp { balance_id: clawed },
            )],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ensure_events(&applied, emits, &["clawback"])?;
        ctx.capture(
            &self.slug("clawback-claimable-balance"),
            Selector::ledger_of(&applied),
            note("the issuer claws back a claimable balance: a clawback event from the claimable balance address")
                .fields(&["op.clawback_claimable_balance.balance_id"]),
        )?;

        let pool = pool_id(&Asset::Native, &asset)?;
        let share =
            ChangeTrustAsset::PoolShare(LiquidityPoolParameters::LiquidityPoolConstantProduct(
                LiquidityPoolConstantProductParameters {
                    asset_a: Asset::Native,
                    asset_b: asset.clone(),
                    fee: POOL_FEE,
                },
            ));
        let env = ctx.classic(&alice, vec![change_trust(share)])?;
        ctx.submit_and_close(&env)?;
        let env = ctx.classic(
            &alice,
            vec![OperationBody::LiquidityPoolDeposit(
                LiquidityPoolDepositOp {
                    liquidity_pool_id: pool,
                    max_amount_a: 100 * UNIT,
                    max_amount_b: 100 * UNIT,
                    min_price: Price { n: 1, d: 1 },
                    max_price: Price { n: 1, d: 1 },
                },
            )],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ensure_events(&applied, emits, &["transfer", "transfer"])?;
        ctx.capture(
            &self.slug("liquidity-pool-deposit"),
            Selector::ledger_of(&applied),
            note("a deposit into a constant product pool: one transfer event per asset to the liquidity pool address (strkey L)")
                .fields(&["op.liquidity_pool_deposit"]),
        )?;

        let mux_id = u64::MAX;
        let env = ctx.classic(
            &carol,
            vec![OperationBody::AccountMerge(muxed_account_with_id(
                &bob.verifying_key(),
                mux_id,
            ))],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ensure_events(&applied, emits, &["transfer"])?;
        if emits {
            ensure_muxed_data(&applied, "transfer", &ScVal::U64(mux_id))?;
        }
        ctx.capture(
            &self.slug("account-merge-muxed-destination"),
            Selector::ledger_of(&applied),
            note("an account merged into a muxed account with mux id 2^64-1: the remaining lumens move in a transfer event with to_muxed_id")
                .fields(&["op.account_merge.destination"]),
        )?;

        if self.events == Events::Live {
            let echo = deploy_echo(ctx, &alice, FAMILY)?;
            let applied = ctx
                .driver()
                .invoke(&alice, &echo, "echo", vec![ScVal::U32(67)])?;
            let TransactionMeta::V4(m) = &applied.meta else {
                bail!("ledger {}: no v4 meta", applied.ledger);
            };
            let stages: Vec<TransactionEventStage> = m.events.iter().map(|e| e.stage).collect();
            ensure!(
                stages
                    == [
                        TransactionEventStage::BeforeAllTxs,
                        TransactionEventStage::AfterAllTxs
                    ],
                "ledger {}: fee event stages {stages:?}, expected the charge and the refund",
                applied.ledger
            );
            ctx.capture(
                "soroban-fee-refund",
                Selector::ledger_of(&applied),
                Spec::new(
                    "a Soroban invocation whose transaction events are the fee charge before all transactions and the refund of unused resource fee after all transactions",
                )
                .fields(&["tx.soroban.resource_fee"]),
            )?;

            // the SAC side of the same events: an issuer transfer through the
            // asset's contract to a muxed contract address is a mint whose data
            // carries to_muxed_id, the case core's own CAP-84 test covers
            let (_, sac) = deploy_sac(ctx, &issuer, asset.clone())?;
            let mux_id = 67u64;
            let to = ScVal::Address(ScAddress::MuxedContract(MuxedContract {
                id: mux_id,
                contract_id: echo,
            }));
            let hf = call(
                &sac,
                "transfer",
                vec![
                    ScVal::Address(account_address(&issuer.verifying_key())),
                    to,
                    i128_val(i128::from(UNIT)),
                ],
            )?;
            let env = ctx.driver().soroban(&issuer, hf)?;
            let applied = ctx.submit_and_close(&env)?;
            ensure_events(&applied, emits, &["mint"])?;
            ensure_muxed_data(&applied, "mint", &ScVal::U64(mux_id))?;
            ctx.capture(
                "sac-issuer-transfer-to-muxed-contract",
                Selector::ledger_of(&applied),
                Spec::new(
                    "the issuer transfers its asset through the Stellar Asset Contract to a muxed contract address: a mint event whose data is {amount, to_muxed_id}",
                )
                .fields(&["op.invoke_host_function.invoke_contract.args"]),
            )?;
        }
        Ok(())
    }
}

/// The data of the `name` event must be `{amount, to_muxed_id: want}`.
fn ensure_muxed_data(applied: &Applied, name: &str, want: &ScVal) -> Result<()> {
    let data = event_data(applied, name)?;
    let ScVal::Map(Some(m)) = &data else {
        bail!(
            "ledger {}: {name} data is {data:?}, expected a map",
            applied.ledger
        );
    };
    let got = m
        .iter()
        .find(|e| matches!(&e.key, ScVal::Symbol(s) if s.as_slice() == b"to_muxed_id"))
        .map(|e| &e.val);
    ensure!(
        got == Some(want),
        "ledger {}: to_muxed_id is {got:?}, expected {want:?}",
        applied.ledger
    );
    Ok(())
}
