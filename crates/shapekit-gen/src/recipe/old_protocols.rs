//! Shapes only ledgers below some protocol carry, recorded on fresh networks
//! at those protocols. Below protocol 13 core accepts only v0 envelopes, which
//! `ChainCtx::classic` and `ChainCtx::sign` build there. Every chain runs
//! without classic events or their backfill, so the meta is TransactionMeta v2
//! inside LedgerCloseMeta v0, what a protocol 11 to 17 ledger held when core
//! closed it. Each condition is read from stellar-core v30.0.0 and named next to
//! its step.
//!
//! - [`IssuerChecks`], `classic.failed-tx` at protocol 12: the issuer
//!   existence checks removed in protocol 13 (`ManageOfferOpFrameBase::checkOfferValid`,
//!   `PathPaymentOpFrameBase::checkIssuer`, `PaymentOpFrame` mapping the path
//!   payment code), the trust-not-required check removed in 16
//!   (`AllowTrustOpFrame::isAuthRevocationValid`) and the self checks that
//!   protocol 16 moved into doCheckValid as MALFORMED
//!   (`TrustFlagsOpFrameBase::doApply`, `ChangeTrustOpFrame::doApply`).
//! - [`ManageDataProtocol1`], `classic.failed-tx` at protocol 1: ManageData,
//!   which `ManageDataOpFrame::doCheckValid` refuses before protocol 2. As a
//!   validity check it stops the transaction at submission, so no ledger can
//!   hold the result; the refused envelope is the only record.
//! - [`ChangeTrustNative`], `classic.failed-tx` at protocol 9: a native
//!   ChangeTrust line, which `ChangeTrustOpFrame::doCheckValid` accepts before
//!   protocol 10 and `doApply` throws on, so the ledger holds tx_internal_error.
//! - [`Inflation`], `cap26.inflation` at 11 and 12: the inflation operation
//!   paying a voted destination and answering NOT_TIME once the weekly rounds
//!   have caught up with the close time, and at 12 the same operation refused
//!   with opNOT_SUPPORTED (`InflationOpFrame::isOpSupported`).
//! - [`ClaimAtomV0`], `cap03.offer-liabilities` at 17: offers crossed by
//!   manage-sell, manage-buy and both path payments, whose claim atoms core
//!   writes as CLAIM_ATOM_TYPE_V0 before protocol 18 (`makeClaimAtom`,
//!   TransactionUtils.cpp).

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use shapekit_core_driver::config::close_time;
use shapekit_core_driver::keys::{account_id, muxed_account};
use stellar_xdr::{
    AllowTrustOp, AlphaNum4, Asset, AssetCode, AssetCode4, ChangeTrustAsset, ChangeTrustOp,
    ClaimAtom, DataValue, InflationResult, LedgerCloseMeta, ManageBuyOfferOp, ManageBuyOfferResult,
    ManageDataOp, ManageDataResult, ManageSellOfferOp, ManageSellOfferResult, OperationBody,
    OperationResult, OperationResultTr, PathPaymentStrictReceiveOp, PathPaymentStrictReceiveResult,
    PathPaymentStrictSendOp, PathPaymentStrictSendResult, PaymentOp, Price, SetOptionsOp, String64,
    TransactionResultResult,
};

use super::failed_tx::codes::op_code;
use super::{ChainCtx, Recipe, Selector, Spec};

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

/// One XLM, and one unit of every credit asset, in stroops.
const UNIT: i64 = 10_000_000;

fn alphanum4(code: &[u8; 4], issuer: &SigningKey) -> Asset {
    Asset::CreditAlphanum4(AlphaNum4 {
        asset_code: AssetCode4(*code),
        issuer: account_id(&issuer.verifying_key()),
    })
}

fn trust(asset: &Asset) -> Result<OperationBody> {
    let Asset::CreditAlphanum4(a) = asset else {
        bail!("only alphanum4 assets are trusted here");
    };
    Ok(OperationBody::ChangeTrust(ChangeTrustOp {
        line: ChangeTrustAsset::CreditAlphanum4(a.clone()),
        limit: i64::MAX,
    }))
}

fn op_results(r: &TransactionResultResult) -> Result<&[OperationResult]> {
    match r {
        TransactionResultResult::TxFailed(ops) | TransactionResultResult::TxSuccess(ops) => {
            Ok(ops.as_slice())
        }
        other => bail!("core answered {other:?}, expected operation results"),
    }
}

/// Submits a one-operation transaction from `source` that core must include
/// and fail with `want` (an XDR-JSON code such as `payment.no_issuer`), then
/// records its ledger.
fn fail_and_capture(
    ctx: &mut ChainCtx,
    slug: &str,
    source: &SigningKey,
    op: OperationBody,
    want: &str,
    spec: Spec,
) -> Result<()> {
    let env = ctx.classic(source, vec![op])?;
    let applied = ctx
        .submit_and_close_failing(&env)
        .context(slug.to_string())?;
    let ops = op_results(&applied.result.result).context(slug.to_string())?;
    let got = op_code(ops.first().context("no operation result")?)?;
    ensure!(got == want, "{slug}: core answered {got}, expected {want}");
    let notes = format!("{} (core: {got})", spec.notes);
    ctx.capture(slug, Selector::ledger_of(&applied), Spec { notes, ..spec })
}

/// `classic.failed-tx` below its valid range: apply-time failure codes core
/// stopped giving at protocol 13 or 16, all reachable at protocol 12.
pub struct IssuerChecks {
    pub protocol: u32,
}

impl Recipe for IssuerChecks {
    fn id(&self) -> &str {
        "classic.failed-tx.issuer-checks"
    }

    fn family(&self) -> &str {
        super::failed_tx::FAMILY
    }

    fn protocol(&self) -> u32 {
        self.protocol
    }

    fn emit_classic_events(&self) -> bool {
        false
    }

    fn backfill_stellar_asset_events(&self) -> bool {
        false
    }

    fn below_protocol_valid(&self) -> bool {
        true
    }

    fn run(&self, ctx: &mut ChainCtx) -> Result<()> {
        ensure!(
            self.protocol < 13,
            "the issuer checks exist only before protocol 13"
        );
        ctx.fund(&["issuer", "holder", "destination"], BALANCE)?;
        let issuer = ctx.account("issuer");
        let holder = ctx.account("holder");
        let destination = ctx.account("destination");
        // never funded: its assets name an issuer the ledger does not hold
        let ghost = ctx.account("ghost-issuer");
        let opn = alphanum4(b"OPN\0", &issuer);
        let gho = alphanum4(b"GHO\0", &ghost);
        let env = ctx.classic(&holder, vec![trust(&opn)?])?;
        ctx.submit_and_close(&env)?;

        let offer_fields = &[
            "op.manage_offer.assets_amount_price",
            "tx.v0.source_account_ed25519",
        ];
        let sell = |selling: &Asset, buying: &Asset| {
            OperationBody::ManageSellOffer(ManageSellOfferOp {
                selling: selling.clone(),
                buying: buying.clone(),
                amount: UNIT,
                price: Price { n: 1, d: 1 },
                offer_id: 0,
            })
        };
        let buy = |selling: &Asset, buying: &Asset| {
            OperationBody::ManageBuyOffer(ManageBuyOfferOp {
                selling: selling.clone(),
                buying: buying.clone(),
                buy_amount: UNIT,
                price: Price { n: 1, d: 1 },
                offer_id: 0,
            })
        };
        fail_and_capture(
            ctx,
            "manage-sell-offer-sell-no-issuer",
            &holder,
            sell(&gho, &Asset::Native),
            "manage_sell_offer.sell_no_issuer",
            Spec::new("a sell offer selling an asset whose issuer account does not exist, in a v0 envelope at protocol 12; the holder has no trustline for it either, and the issuer check comes first")
                .fields(offer_fields),
        )?;
        fail_and_capture(
            ctx,
            "manage-sell-offer-buy-no-issuer",
            &holder,
            sell(&Asset::Native, &gho),
            "manage_sell_offer.buy_no_issuer",
            Spec::new("a sell offer buying an asset whose issuer account does not exist, in a v0 envelope at protocol 12")
                .fields(offer_fields),
        )?;
        fail_and_capture(
            ctx,
            "manage-buy-offer-sell-no-issuer",
            &holder,
            buy(&gho, &Asset::Native),
            "manage_buy_offer.sell_no_issuer",
            Spec::new("a buy offer selling an asset whose issuer account does not exist, in a v0 envelope at protocol 12")
                .fields(offer_fields),
        )?;
        fail_and_capture(
            ctx,
            "manage-buy-offer-buy-no-issuer",
            &holder,
            buy(&Asset::Native, &gho),
            "manage_buy_offer.buy_no_issuer",
            Spec::new("a buy offer buying an asset whose issuer account does not exist, in a v0 envelope at protocol 12")
                .fields(offer_fields),
        )?;

        let dest = muxed_account(&destination.verifying_key());
        fail_and_capture(
            ctx,
            "path-payment-strict-receive-no-issuer",
            &holder,
            OperationBody::PathPaymentStrictReceive(PathPaymentStrictReceiveOp {
                send_asset: Asset::Native,
                send_max: UNIT,
                destination: dest.clone(),
                dest_asset: gho.clone(),
                dest_amount: UNIT,
                path: Default::default(),
            }),
            "path_payment_strict_receive.no_issuer",
            Spec::new("a strict-receive path payment of XLM into an asset whose issuer account does not exist; the result names that asset, which neither the source nor the destination holds")
                .fields(&["op.path_payment.destination", "op.path_payment.amounts", "op.path_payment.path"]),
        )?;
        fail_and_capture(
            ctx,
            "path-payment-strict-send-no-issuer",
            &holder,
            OperationBody::PathPaymentStrictSend(PathPaymentStrictSendOp {
                send_asset: Asset::Native,
                send_amount: UNIT,
                destination: dest.clone(),
                dest_asset: gho.clone(),
                dest_min: 1,
                path: Default::default(),
            }),
            "path_payment_strict_send.no_issuer",
            Spec::new("a strict-send path payment (protocol 12, its first protocol) of XLM into an asset whose issuer account does not exist; the result names that asset")
                .fields(&["op.path_payment.destination", "op.path_payment.amounts", "op.path_payment.path"]),
        )?;
        fail_and_capture(
            ctx,
            "payment-no-issuer",
            &holder,
            OperationBody::Payment(PaymentOp {
                destination: dest,
                asset: gho,
                amount: UNIT,
            }),
            "payment.no_issuer",
            Spec::new("a payment of an asset whose issuer account does not exist to an account that is not the issuer; core runs it as a path payment and maps the code")
                .fields(&["op.payment.destination", "op.payment.asset", "op.payment.amount"]),
        )?;

        let opn_code = AssetCode::CreditAlphanum4(AssetCode4(*b"OPN\0"));
        fail_and_capture(
            ctx,
            "allow-trust-trust-not-required",
            &issuer,
            OperationBody::AllowTrust(AllowTrustOp {
                trustor: account_id(&holder.verifying_key()),
                asset: opn_code.clone(),
                authorize: 1,
            }),
            "allow_trust.trust_not_required",
            Spec::new("AllowTrust from an issuer without AUTH_REQUIRED on a trustline that exists; before protocol 16 core refuses it at apply instead of setting the flag")
                .fields(&["op.allow_trust"]),
        )?;
        fail_and_capture(
            ctx,
            "allow-trust-self-not-allowed",
            &issuer,
            OperationBody::AllowTrust(AllowTrustOp {
                trustor: account_id(&issuer.verifying_key()),
                asset: opn_code,
                authorize: 1,
            }),
            "allow_trust.self_not_allowed",
            Spec::new("AllowTrust naming the issuer itself as trustor; from protocol 16 this is MALFORMED and never reaches a ledger")
                .fields(&["op.allow_trust"]),
        )?;
        fail_and_capture(
            ctx,
            "change-trust-self-not-allowed",
            &issuer,
            trust(&opn)?,
            "change_trust.self_not_allowed",
            Spec::new("ChangeTrust by an issuer to its own asset; from protocol 16 this is MALFORMED and never reaches a ledger")
                .fields(&["op.change_trust.line", "op.change_trust.limit"]),
        )?;
        Ok(())
    }
}

/// `classic.failed-tx` at protocol 1, the only protocol without ManageData:
/// core refuses it at submission, which is all that can be recorded.
pub struct ManageDataProtocol1 {
    pub protocol: u32,
}

impl Recipe for ManageDataProtocol1 {
    fn id(&self) -> &str {
        "classic.failed-tx.manage-data"
    }

    fn family(&self) -> &str {
        super::failed_tx::FAMILY
    }

    fn protocol(&self) -> u32 {
        self.protocol
    }

    fn emit_classic_events(&self) -> bool {
        false
    }

    fn backfill_stellar_asset_events(&self) -> bool {
        false
    }

    fn below_protocol_valid(&self) -> bool {
        true
    }

    fn run(&self, ctx: &mut ChainCtx) -> Result<()> {
        ensure!(self.protocol < 2, "ManageData exists from protocol 2");
        let root = ctx.root().clone();
        let env = ctx.classic(
            &root,
            vec![OperationBody::ManageData(ManageDataOp {
                data_name: String64("shapekit".try_into()?),
                data_value: Some(DataValue(b"protocol 1".to_vec().try_into()?)),
            })],
        )?;
        let r = ctx.capture_rejected(
            "manage-data-not-supported-yet",
            &env,
            Spec::new("ManageData in a v0 envelope at protocol 1, refused at submission with MANAGE_DATA_NOT_SUPPORTED_YET; the check is in doCheckValid, so the tx queue and tx set validation keep it out of every ledger")
                .fields(&["op.manage_data.data_name", "op.manage_data.data_value", "tx.envelope.type"]),
        )?;
        let ops = op_results(&r.result)?;
        ensure!(
            matches!(
                ops.first(),
                Some(OperationResult::OpInner(OperationResultTr::ManageData(
                    ManageDataResult::NotSupportedYet
                )))
            ),
            "core answered {:?}, expected manage_data not_supported_yet",
            r.result
        );
        Ok(())
    }
}

/// `classic.failed-tx` at protocol 9: ChangeTrust with a native line. Before
/// protocol 10 `ChangeTrustOpFrame::doCheckValid` lets a native line through,
/// `doApply` throws, and `TransactionFrame::applyOperations` turns the
/// exception into txINTERNAL_ERROR because `HALT_ON_INTERNAL_TRANSACTION_ERROR`
/// is off by default (`Config.cpp`). The release build closes the ledger with
/// that result, so the native line reaches a tx set.
pub struct ChangeTrustNative {
    pub protocol: u32,
}

impl Recipe for ChangeTrustNative {
    fn id(&self) -> &str {
        "classic.failed-tx.change-trust-native"
    }

    fn family(&self) -> &str {
        super::failed_tx::FAMILY
    }

    fn protocol(&self) -> u32 {
        self.protocol
    }

    fn emit_classic_events(&self) -> bool {
        false
    }

    fn backfill_stellar_asset_events(&self) -> bool {
        false
    }

    fn below_protocol_valid(&self) -> bool {
        true
    }

    fn run(&self, ctx: &mut ChainCtx) -> Result<()> {
        ensure!(
            self.protocol < 10,
            "doCheckValid refuses a native change trust line from protocol 10"
        );
        ctx.fund(&["holder"], BALANCE)?;
        let holder = ctx.account("holder");
        let env = ctx.classic(
            &holder,
            vec![OperationBody::ChangeTrust(ChangeTrustOp {
                line: ChangeTrustAsset::Native,
                limit: i64::MAX,
            })],
        )?;
        let applied = ctx.submit_and_close_failing(&env)?;
        ensure!(
            matches!(
                applied.result.result,
                TransactionResultResult::TxInternalError
            ),
            "core answered {:?}, expected tx_internal_error",
            applied.result.result
        );
        ctx.capture(
            "change-trust-native-internal-error",
            Selector::transaction(&applied),
            Spec::new("ChangeTrust with a native line in a v0 envelope at protocol 9: doCheckValid accepts it before protocol 10, doApply throws, and core records tx_internal_error in the ledger, charging the fee and consuming the sequence number")
                .fields(&["op.change_trust.line", "op.change_trust.limit", "tx.v0.source_account_ed25519"]),
        )?;
        Ok(())
    }
}

pub const INFLATION_FAMILY: &str = "cap26.inflation";

/// Weekly inflation rounds (`INFLATION_FREQUENCY`, InflationOpFrame.cpp).
const INFLATION_FREQUENCY: u64 = 7 * 24 * 60 * 60;
/// 2014-07-01, the close time of round zero (`INFLATION_START_TIME`).
const INFLATION_START_TIME: u64 = 1_404_172_800;
/// Operations per transaction and, at genesis, per ledger (maxTxSetSize).
const MAX_OPS: u64 = 100;

/// `cap26.inflation`: below protocol 12 a contrast chain with the operation
/// working; at 12 the refusal CAP-0026 introduced.
pub struct Inflation {
    pub protocol: u32,
}

fn inflation_seq(lcm: &LedgerCloseMeta) -> Result<u32> {
    match lcm {
        LedgerCloseMeta::V0(m) => Ok(m.ledger_header.header.inflation_seq),
        other => bail!("expected a v0 ledger below protocol 20, got {other:?}"),
    }
}

/// Successful rounds an inflation run could make in a ledger closing at
/// `close`, with `seq` rounds already paid: core pays round `seq` once the
/// close time reaches `INFLATION_START_TIME + seq * INFLATION_FREQUENCY`.
fn rounds_due(close: u64, seq: u32) -> u64 {
    let next = INFLATION_START_TIME + u64::from(seq) * INFLATION_FREQUENCY;
    if close < next {
        0
    } else {
        (close - next) / INFLATION_FREQUENCY + 1
    }
}

impl Recipe for Inflation {
    fn id(&self) -> &str {
        INFLATION_FAMILY
    }

    fn family(&self) -> &str {
        INFLATION_FAMILY
    }

    fn protocol(&self) -> u32 {
        self.protocol
    }

    fn emit_classic_events(&self) -> bool {
        false
    }

    fn backfill_stellar_asset_events(&self) -> bool {
        false
    }

    fn below_protocol_valid(&self) -> bool {
        self.protocol < 12
    }

    fn run(&self, ctx: &mut ChainCtx) -> Result<()> {
        if self.protocol >= 12 {
            return self.refused(ctx);
        }
        ctx.fund(&["winner", "caller"], BALANCE)?;
        let winner = ctx.account("winner");
        let caller = ctx.account("caller");
        let root = ctx.root().clone();
        // the root holds nearly every lumen, so its vote alone clears the 0.05% bar
        let env = ctx.classic(
            &root,
            vec![OperationBody::SetOptions(SetOptionsOp {
                inflation_dest: Some(account_id(&winner.verifying_key())),
                clear_flags: None,
                set_flags: None,
                master_weight: None,
                low_threshold: None,
                med_threshold: None,
                high_threshold: None,
                home_domain: None,
                signer: None,
            })],
        )?;
        ctx.submit_and_close(&env)?;

        let env = ctx.classic(&caller, vec![OperationBody::Inflation])?;
        let applied = ctx.submit_and_close(&env)?;
        let ops = op_results(&applied.result.result)?;
        match ops.first() {
            Some(OperationResult::OpInner(OperationResultTr::Inflation(
                InflationResult::Success(payouts),
            ))) => ensure!(
                payouts.len() == 1
                    && payouts[0].destination == account_id(&winner.verifying_key())
                    && payouts[0].amount > 0,
                "inflation should pay the root's inflation destination alone, paid {payouts:?}"
            ),
            other => bail!("expected inflation success, core answered {other:?}"),
        }
        ctx.capture(
            "inflation-payout",
            Selector::ledger_of(&applied),
            Spec::new(format!(
                "an inflation operation at protocol {} paying the one account the root voted for; the payout is a balance change on an account the envelope never names",
                self.protocol
            ))
            .fields(&["op.set_options.inflation_dest", "tx.v0.source_account_ed25519"]),
        )?;

        // the virtual clock starts in 2026, so hundreds of weekly rounds are due before
        // core can answer NOT_TIME; pay them off in full transactions first
        loop {
            let next = ctx.driver().last_closed() + 1;
            let seq = inflation_seq(ctx.driver().last_meta())?;
            let due = rounds_due(close_time(next), seq);
            if due == 0 {
                break;
            }
            let n = due.min(MAX_OPS) as usize;
            let env = ctx.classic(&caller, vec![OperationBody::Inflation; n])?;
            let applied = ctx.submit_and_close(&env)?;
            let paid = inflation_seq(ctx.driver().last_meta())?;
            ensure!(
                paid == seq + n as u32,
                "ledger {}: {n} inflation operations should advance the round from {seq}, it is {paid}",
                applied.ledger
            );
        }
        fail_and_capture(
            ctx,
            "inflation-not-time",
            &caller,
            OperationBody::Inflation,
            "inflation.not_time",
            Spec::new(format!(
                "an inflation operation at protocol {} after every weekly round up to the close time has been paid",
                self.protocol
            ))
            .fields(&["tx.v0.source_account_ed25519"]),
        )
    }
}

impl Inflation {
    fn refused(&self, ctx: &mut ChainCtx) -> Result<()> {
        ctx.fund(&["caller"], BALANCE)?;
        let caller = ctx.account("caller");
        let env = ctx.classic(&caller, vec![OperationBody::Inflation])?;
        let r = ctx.capture_rejected(
            "inflation-not-supported",
            &env,
            Spec::new(format!(
                "an inflation operation at protocol {}, where CAP-0026 disabled it: core refuses the transaction at submission with opNOT_SUPPORTED",
                self.protocol
            ))
            .fields(&["tx.envelope.type"]),
        )?;
        let ops = op_results(&r.result)?;
        ensure!(
            matches!(ops.first(), Some(OperationResult::OpNotSupported)),
            "core answered {:?}, expected opNOT_SUPPORTED",
            r.result
        );
        Ok(())
    }
}

/// `cap03.offer-liabilities` at a protocol before 18, where every claim atom
/// is the v0 arm keyed by the seller's raw ed25519 key.
pub struct ClaimAtomV0 {
    pub protocol: u32,
}

fn all_v0(atoms: &[ClaimAtom], what: &str) -> Result<()> {
    ensure!(!atoms.is_empty(), "{what}: no offer was crossed");
    ensure!(
        atoms.iter().all(|a| matches!(a, ClaimAtom::V0(_))),
        "{what}: core wrote claim atoms {atoms:?}, expected only v0"
    );
    Ok(())
}

impl Recipe for ClaimAtomV0 {
    fn id(&self) -> &str {
        "cap03.offer-liabilities.claim-atom-v0"
    }

    fn family(&self) -> &str {
        super::cap03::FAMILY
    }

    fn protocol(&self) -> u32 {
        self.protocol
    }

    fn emit_classic_events(&self) -> bool {
        false
    }

    fn backfill_stellar_asset_events(&self) -> bool {
        false
    }

    fn run(&self, ctx: &mut ChainCtx) -> Result<()> {
        ensure!(
            self.protocol < 18,
            "claim atoms are v0 only before protocol 18"
        );
        ctx.fund(&["issuer", "maker", "taker", "receiver"], BALANCE)?;
        let issuer = ctx.account("issuer");
        let maker = ctx.account("maker");
        let taker = ctx.account("taker");
        let receiver = ctx.account("receiver");
        let usd = alphanum4(b"USD\0", &issuer);
        for who in [&maker, &taker, &receiver] {
            let env = ctx.classic(who, vec![trust(&usd)?])?;
            ctx.submit_and_close(&env)?;
        }
        let env = ctx.classic(
            &issuer,
            vec![OperationBody::Payment(PaymentOp {
                destination: muxed_account(&maker.verifying_key()),
                asset: usd.clone(),
                amount: 1_000 * UNIT,
            })],
        )?;
        ctx.submit_and_close(&env)?;
        // one standing offer selling USD for XLM at 2 XLM per USD, deep enough for every taker
        let env = ctx.classic(
            &maker,
            vec![OperationBody::ManageSellOffer(ManageSellOfferOp {
                selling: usd.clone(),
                buying: Asset::Native,
                amount: 500 * UNIT,
                price: Price { n: 2, d: 1 },
                offer_id: 0,
            })],
        )?;
        ctx.submit_and_close(&env)?;

        let fields = &[
            "op.manage_offer.assets_amount_price",
            "op.manage_offer.offer_id",
        ];
        let env = ctx.classic(
            &taker,
            vec![OperationBody::ManageSellOffer(ManageSellOfferOp {
                selling: Asset::Native,
                buying: usd.clone(),
                amount: 20 * UNIT,
                price: Price { n: 1, d: 2 },
                offer_id: 0,
            })],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        match op_results(&applied.result.result)?.first() {
            Some(OperationResult::OpInner(OperationResultTr::ManageSellOffer(
                ManageSellOfferResult::Success(s),
            ))) => all_v0(&s.offers_claimed, "manage sell offer")?,
            other => bail!("expected a crossing sell offer, core answered {other:?}"),
        }
        ctx.capture(
            "manage-sell-offer-claim-atom-v0",
            Selector::ledger_of(&applied),
            Spec::new(format!(
                "a sell offer crossing the maker's offer at protocol {}: the claim atom is CLAIM_ATOM_TYPE_V0 with the seller as a raw ed25519 key",
                self.protocol
            ))
            .fields(fields),
        )?;

        let env = ctx.classic(
            &taker,
            vec![OperationBody::ManageBuyOffer(ManageBuyOfferOp {
                selling: Asset::Native,
                buying: usd.clone(),
                buy_amount: 5 * UNIT,
                price: Price { n: 2, d: 1 },
                offer_id: 0,
            })],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        match op_results(&applied.result.result)?.first() {
            Some(OperationResult::OpInner(OperationResultTr::ManageBuyOffer(
                ManageBuyOfferResult::Success(s),
            ))) => all_v0(&s.offers_claimed, "manage buy offer")?,
            other => bail!("expected a crossing buy offer, core answered {other:?}"),
        }
        ctx.capture(
            "manage-buy-offer-claim-atom-v0",
            Selector::ledger_of(&applied),
            Spec::new(format!(
                "a buy offer crossing the maker's offer at protocol {}, claim atom v0",
                self.protocol
            ))
            .fields(fields),
        )?;

        let pp_fields = &["op.path_payment.destination", "op.path_payment.amounts"];
        let dest = muxed_account(&receiver.verifying_key());
        let env = ctx.classic(
            &taker,
            vec![OperationBody::PathPaymentStrictReceive(
                PathPaymentStrictReceiveOp {
                    send_asset: Asset::Native,
                    send_max: 20 * UNIT,
                    destination: dest.clone(),
                    dest_asset: usd.clone(),
                    dest_amount: 5 * UNIT,
                    path: Default::default(),
                },
            )],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        match op_results(&applied.result.result)?.first() {
            Some(OperationResult::OpInner(OperationResultTr::PathPaymentStrictReceive(
                PathPaymentStrictReceiveResult::Success(s),
            ))) => all_v0(&s.offers, "strict receive")?,
            other => bail!("expected a strict receive through the book, core answered {other:?}"),
        }
        ctx.capture(
            "path-payment-strict-receive-claim-atom-v0",
            Selector::ledger_of(&applied),
            Spec::new(format!(
                "a strict-receive path payment of XLM into USD for a third account through the maker's offer at protocol {}, claim atom v0",
                self.protocol
            ))
            .fields(pp_fields),
        )?;

        let env = ctx.classic(
            &taker,
            vec![OperationBody::PathPaymentStrictSend(
                PathPaymentStrictSendOp {
                    send_asset: Asset::Native,
                    send_amount: 10 * UNIT,
                    destination: dest,
                    dest_asset: usd,
                    dest_min: 1,
                    path: Default::default(),
                },
            )],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        match op_results(&applied.result.result)?.first() {
            Some(OperationResult::OpInner(OperationResultTr::PathPaymentStrictSend(
                PathPaymentStrictSendResult::Success(s),
            ))) => all_v0(&s.offers, "strict send")?,
            other => bail!("expected a strict send through the book, core answered {other:?}"),
        }
        ctx.capture(
            "path-payment-strict-send-claim-atom-v0",
            Selector::ledger_of(&applied),
            Spec::new(format!(
                "a strict-send path payment of XLM into USD for a third account through the maker's offer at protocol {}, claim atom v0",
                self.protocol
            ))
            .fields(pp_fields),
        )?;
        Ok(())
    }
}
