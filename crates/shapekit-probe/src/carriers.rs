//! Carriers: for every struct the envelope can reach, a valid transaction that holds one
//! instance of it, built for one attempt's own accounts. The base transaction of a carrier
//! succeeds unless the carrier says core refuses every transaction holding the struct.

use anyhow::{Context, Result, bail};
use ed25519_dalek::SigningKey;
use serde_json::json;
use sha2::{Digest, Sha256};
use shapekit_core_driver::chain::Applied;
use shapekit_core_driver::config::close_time;
use shapekit_core_driver::keys::{
    account_id, account_strkey, ed25519_signer, muxed_account, signed_payload_signer,
};
use shapekit_core_driver::tx::{BASE_FEE, fee_bump};
use stellar_xdr::{
    AllowTrustOp, Asset, AssetCode, AssetCode4, BeginSponsoringFutureReservesOp, BumpSequenceOp,
    ChangeTrustAsset, ChangeTrustOp, ClaimClaimableBalanceOp, ClaimPredicate, ClaimableBalanceId,
    Claimant, ClaimantV0, ClawbackClaimableBalanceOp, ClawbackOp, CreateClaimableBalanceOp,
    CreateClaimableBalanceResult, CreatePassiveSellOfferOp, FeeBumpTransactionEnvelope,
    LedgerBounds, LedgerKey, LedgerKeyAccount, LedgerKeyClaimableBalance, LedgerKeyData,
    LedgerKeyLiquidityPool, LedgerKeyOffer, LedgerKeyTrustLine,
    LiquidityPoolConstantProductParameters, LiquidityPoolDepositOp, LiquidityPoolParameters,
    LiquidityPoolWithdrawOp, ManageBuyOfferOp, ManageDataOp, ManageOfferSuccessResultOffer,
    ManageSellOfferOp, ManageSellOfferResult, MuxedAccount, MuxedAccountMed25519, OperationBody,
    OperationResult, OperationResultTr, PathPaymentStrictReceiveOp, PathPaymentStrictSendOp,
    PaymentOp, PoolId, Preconditions, PreconditionsV2, Price, RevokeSponsorshipOp,
    RevokeSponsorshipOpSigner, SequenceNumber, SetOptionsOp, SetTrustLineFlagsOp, Signer,
    TimeBounds, TimePoint, TransactionEnvelope, TransactionResultResult, TransactionV0Envelope,
    TrustLineAsset, Uint256, WriteXdr,
};

use crate::sign::SignPlan;
use crate::values::Replacement;
use crate::world::{Subject, World, WorldKind, create};

/// The base transaction of one attempt.
pub struct Base {
    /// Unsigned or partly signed; [`crate::sign::sign`] completes it by `plan`.
    pub env: TransactionEnvelope,
    pub plan: SignPlan,
    /// JSON pointer to the struct instance inside the envelope's XDR-JSON; `None` finds the
    /// one object holding exactly the struct's keys.
    pub at: Option<String>,
}

/// How a carrier's base transaction ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseOutcome {
    Succeeds,
    /// Core refuses every transaction that holds the struct, whatever its fields say; the
    /// reason cites core's source.
    Refused(&'static str),
    /// Core includes it and the operation fails, whatever the fields say.
    Fails(&'static str),
}

pub struct Carrier {
    pub structure: &'static str,
    /// Tells the carriers of one struct apart, empty for a struct's only carrier.
    pub id: &'static str,
    /// Where the struct sits, in words.
    pub context: &'static str,
    /// The chain the carrier runs on.
    pub world: WorldKind,
    pub outcome: BaseOutcome,
    pub build: fn(&mut World, &Subject) -> Result<Base>,
    /// Values the type alone does not give but the field's meaning does, added after the
    /// type's replacements: a time bound already past, a claimant named twice. Called with
    /// the field's `.x` name.
    pub extra: Extra,
}

/// Called with the world, the attempt's accounts, the field's `.x` name, its base value and
/// the signed base envelope as XDR-JSON.
pub type Extra =
    fn(&World, &Subject, &str, &serde_json::Value, &serde_json::Value) -> Result<Vec<Replacement>>;

pub fn no_extra(
    _: &World,
    _: &Subject,
    _: &str,
    _: &serde_json::Value,
    _: &serde_json::Value,
) -> Result<Vec<Replacement>> {
    Ok(Vec::new())
}

pub fn extra(name: &str, value: serde_json::Value) -> Replacement {
    Replacement {
        name: name.to_owned(),
        value,
        xdr: None,
        before: None,
    }
}

/// One lumen.
pub const XLM: i64 = 10_000_000;

fn plan(keys: &[&SigningKey]) -> SignPlan {
    SignPlan {
        tx: keys.iter().map(|k| (*k).clone()).collect(),
        ..Default::default()
    }
}

fn simple(w: &World, s: &Subject, body: OperationBody) -> Result<Base> {
    Ok(Base {
        env: World::unsigned(w.tx(&s.source, vec![(None, body)])?),
        plan: plan(&[&s.source]),
        at: None,
    })
}

fn payment(to: &SigningKey) -> OperationBody {
    OperationBody::Payment(PaymentOp {
        destination: muxed_account(&to.verifying_key()),
        asset: Asset::Native,
        amount: XLM,
    })
}

fn acc(k: &SigningKey) -> stellar_xdr::AccountId {
    account_id(&k.verifying_key())
}

/// The latest close time a time bound in a base transaction allows.
fn later(w: &World) -> u64 {
    close_time(w.driver.last_closed() + 1) + 1_000_000
}

fn trust(asset: &Asset) -> ChangeTrustAsset {
    match asset {
        Asset::Native => ChangeTrustAsset::Native,
        Asset::CreditAlphanum4(a) => ChangeTrustAsset::CreditAlphanum4(a.clone()),
        Asset::CreditAlphanum12(a) => ChangeTrustAsset::CreditAlphanum12(a.clone()),
    }
}

fn change_trust(asset: &Asset) -> OperationBody {
    OperationBody::ChangeTrust(ChangeTrustOp {
        line: trust(asset),
        limit: i64::MAX,
    })
}

/// `who` trusts the shared credit and holds `amount` of it.
fn hold_credit(w: &mut World, who: &SigningKey, amount: i64) -> Result<()> {
    let asset = w.credit4();
    hold(w, who, asset, amount)
}

/// `who` trusts `asset` of the shared issuer and holds `amount` of it.
fn hold(w: &mut World, who: &SigningKey, asset: Asset, amount: i64) -> Result<()> {
    w.classic_ok(who, vec![change_trust(&asset)], &[])?;
    let issuer = w.issuer.clone();
    w.classic_ok(
        &issuer,
        vec![OperationBody::Payment(PaymentOp {
            destination: muxed_account(&who.verifying_key()),
            asset,
            amount,
        })],
        &[],
    )?;
    Ok(())
}

/// The subject's own asset `SUBJ`, with `flags` set on the subject.
fn own_asset(s: &Subject) -> Asset {
    Asset::CreditAlphanum4(stellar_xdr::AlphaNum4 {
        asset_code: AssetCode4(*b"SUBJ"),
        issuer: acc(&s.source),
    })
}

fn set_flags(flags: u32) -> OperationBody {
    OperationBody::SetOptions(SetOptionsOp {
        set_flags: Some(flags),
        ..Default::default()
    })
}

const AUTH_REQUIRED: u32 = 1;
const AUTH_REVOCABLE: u32 = 2;
const AUTH_CLAWBACK: u32 = 8;

fn op_result(a: &Applied, i: usize) -> Result<&OperationResultTr> {
    let TransactionResultResult::TxSuccess(rs) = &a.result.result else {
        bail!(
            "expected a successful transaction, got {:?}",
            a.result.result
        );
    };
    match rs.get(i) {
        Some(OperationResult::OpInner(tr)) => Ok(tr),
        other => bail!("operation {i}: {other:?}"),
    }
}

/// `creator` creates a native claimable balance of one lumen claimable by `claimant`.
fn claimable(
    w: &mut World,
    creator: &SigningKey,
    claimant: &SigningKey,
    asset: Asset,
) -> Result<ClaimableBalanceId> {
    let a = w.classic_ok(
        creator,
        vec![OperationBody::CreateClaimableBalance(
            CreateClaimableBalanceOp {
                asset,
                amount: XLM,
                claimants: vec![Claimant::ClaimantTypeV0(ClaimantV0 {
                    destination: acc(claimant),
                    predicate: ClaimPredicate::Unconditional,
                })]
                .try_into()?,
            },
        )],
        &[],
    )?;
    match op_result(&a, 0)? {
        OperationResultTr::CreateClaimableBalance(CreateClaimableBalanceResult::Success(id)) => {
            Ok(id.clone())
        }
        other => bail!("creating a claimable balance: {other:?}"),
    }
}

fn sell_offer(selling: Asset, buying: Asset) -> OperationBody {
    OperationBody::ManageSellOffer(ManageSellOfferOp {
        selling,
        buying,
        amount: 10 * XLM,
        price: Price { n: 1, d: 1 },
        offer_id: 0,
    })
}

fn pool_params(w: &World) -> LiquidityPoolParameters {
    LiquidityPoolParameters::LiquidityPoolConstantProduct(LiquidityPoolConstantProductParameters {
        asset_a: Asset::Native,
        asset_b: w.credit4(),
        fee: 30,
    })
}

fn pool_id(w: &World) -> Result<PoolId> {
    let bytes = pool_params(w).to_xdr(stellar_xdr::Limits::none())?;
    Ok(PoolId(stellar_xdr::Hash(Sha256::digest(bytes).into())))
}

/// `who` holds PRB and a pool share trustline of the native/PRB pool.
fn pool_member(w: &mut World, who: &SigningKey) -> Result<()> {
    hold_credit(w, who, 1000 * XLM)?;
    let params = pool_params(w);
    w.classic_ok(
        who,
        vec![OperationBody::ChangeTrust(ChangeTrustOp {
            line: ChangeTrustAsset::PoolShare(params),
            limit: i64::MAX,
        })],
        &[],
    )?;
    Ok(())
}

fn deposit(w: &World) -> Result<OperationBody> {
    Ok(OperationBody::LiquidityPoolDeposit(
        LiquidityPoolDepositOp {
            liquidity_pool_id: pool_id(w)?,
            max_amount_a: 100 * XLM,
            max_amount_b: 100 * XLM,
            min_price: Price { n: 1, d: 1000 },
            max_price: Price { n: 1000, d: 1 },
        },
    ))
}

fn revoke(key: LedgerKey) -> OperationBody {
    OperationBody::RevokeSponsorship(RevokeSponsorshipOp::LedgerEntry(key))
}

fn begin(sponsored: &SigningKey) -> OperationBody {
    OperationBody::BeginSponsoringFutureReserves(BeginSponsoringFutureReservesOp {
        sponsored_id: acc(sponsored),
    })
}

/// The subject sponsors what `sponsored` does in `body`.
fn sponsor(
    w: &mut World,
    s: &Subject,
    sponsored: &SigningKey,
    body: OperationBody,
) -> Result<Applied> {
    let src = s.source.clone();
    w.ops_ok(
        &src,
        vec![
            (None, begin(sponsored)),
            (Some(sponsored), body),
            (Some(sponsored), OperationBody::EndSponsoringFutureReserves),
        ],
        &[sponsored],
    )
}

fn credit_trustline(w: &World) -> TrustLineAsset {
    match w.credit4() {
        Asset::CreditAlphanum4(a) => TrustLineAsset::CreditAlphanum4(a),
        _ => unreachable!("credit4 is an alphanum4 asset"),
    }
}

fn v2(w: &World, s: &Subject) -> Result<PreconditionsV2> {
    let seq = w.next_seq(&s.source)? - 1;
    Ok(PreconditionsV2 {
        time_bounds: Some(TimeBounds {
            min_time: TimePoint(0),
            max_time: TimePoint(later(w)),
        }),
        ledger_bounds: Some(LedgerBounds {
            min_ledger: 0,
            max_ledger: w.driver.last_closed() + 1000,
        }),
        min_seq_num: Some(SequenceNumber(seq)),
        min_seq_age: stellar_xdr::Duration(0),
        min_seq_ledger_gap: 0,
        extra_signers: vec![ed25519_signer(&s.peers[1].verifying_key())].try_into()?,
    })
}

fn with_cond(w: &World, s: &Subject, cond: Preconditions, signers: &[&SigningKey]) -> Result<Base> {
    let mut tx = w.tx(&s.source, vec![(None, payment(&s.peers[0]))])?;
    tx.cond = cond;
    Ok(Base {
        env: World::unsigned(tx),
        plan: plan(signers),
        at: None,
    })
}

/// The subject pays peer 0 ten units of `asset`; both hold a trustline.
fn credit_payment(w: &mut World, s: &Subject, asset: Asset, arm: &str) -> Result<Base> {
    let src = s.source.clone();
    hold(w, &src, asset.clone(), 100 * XLM)?;
    let peer = s.peers[0].clone();
    w.classic_ok(&peer, vec![change_trust(&asset)], &[])?;
    let b = simple(
        w,
        s,
        OperationBody::Payment(PaymentOp {
            destination: muxed_account(&peer.verifying_key()),
            asset,
            amount: 10 * XLM,
        }),
    )?;
    Ok(Base {
        at: Some(format!("/tx/tx/operations/0/body/payment/asset/{arm}")),
        ..b
    })
}

fn pay_base(w: &mut World, s: &Subject) -> Result<Base> {
    simple(w, s, payment(&s.peers[0]))
}

fn muxed7(k: &SigningKey) -> MuxedAccount {
    MuxedAccount::MuxedEd25519(MuxedAccountMed25519 {
        id: 7,
        ed25519: Uint256(k.verifying_key().to_bytes()),
    })
}

/// The subject issues `SUBJ` with clawback enabled and peer 0 holds 100 of it. Returns peer 0.
fn clawback_setup(w: &mut World, s: &Subject) -> Result<SigningKey> {
    let (src, peer) = (s.source.clone(), s.peers[0].clone());
    w.classic_ok(&src, vec![set_flags(AUTH_REVOCABLE | AUTH_CLAWBACK)], &[])?;
    w.classic_ok(&peer, vec![change_trust(&own_asset(s))], &[])?;
    w.classic_ok(
        &src,
        vec![OperationBody::Payment(PaymentOp {
            destination: muxed_account(&peer.verifying_key()),
            asset: own_asset(s),
            amount: 100 * XLM,
        })],
        &[],
    )?;
    Ok(peer)
}

/// A path holding one asset that is not valid: an alphanumeric code of zero bytes.
fn path_extra(
    w: &World,
    _: &Subject,
    f: &str,
    _: &serde_json::Value,
    _: &serde_json::Value,
) -> Result<Vec<Replacement>> {
    Ok(match f {
        "path" => vec![extra(
            "one asset whose four letter code is empty",
            json!([{"credit_alphanum4": {
                "asset_code": "",
                "issuer": account_strkey(&w.issuer.verifying_key()),
            }}]),
        )],
        _ => Vec::new(),
    })
}

/// Two keys whose public keys end in the same four bytes, so a signature hint names both:
/// the first collision among keys derived from a fixed sequence of seeds. A birthday search,
/// about 2^16 keys; deterministic, so every run finds the same pair.
pub fn hint_collision() -> &'static (SigningKey, SigningKey) {
    static PAIR: std::sync::OnceLock<(SigningKey, SigningKey)> = std::sync::OnceLock::new();
    PAIR.get_or_init(|| {
        let mut seen: std::collections::HashMap<[u8; 4], u64> = std::collections::HashMap::new();
        let key = |i: u64| {
            shapekit_core_driver::keys::derive_key(
                crate::KEY_DOMAIN,
                &format!("hint-collision-{i}"),
            )
        };
        for i in 0.. {
            let k = key(i);
            let pk = k.verifying_key().to_bytes();
            let suffix: [u8; 4] = pk[28..].try_into().expect("four bytes");
            if let Some(&j) = seen.get(&suffix) {
                return (key(j), k);
            }
            seen.insert(suffix, i);
        }
        unreachable!("the search runs until it finds a pair")
    })
}

/// The carriers of the classic structs.
pub fn classic() -> Vec<Carrier> {
    let c = |structure, context, build| Carrier {
        structure,
        id: "",
        context,
        world: WorldKind::Current,
        outcome: BaseOutcome::Succeeds,
        build,
        extra: no_extra,
    };
    vec![
        c(
            "TransactionV1Envelope",
            "a payment's v1 envelope",
            |w, s| {
                let mut b = pay_base(w, s)?;
                b.at = Some("/tx".into());
                Ok(b)
            },
        ),
        Carrier {
            id: "payment",
            ..c("Transaction", "a payment", |w, s| {
                let mut b = pay_base(w, s)?;
                b.at = Some("/tx/tx".into());
                Ok(b)
            })
        },
        Carrier {
            id: "payment",
            ..c(
                "DecoratedSignature",
                "the source's signature on a payment",
                |w, s| {
                    let mut b = pay_base(w, s)?;
                    b.at = Some("/tx/signatures/0".into());
                    Ok(b)
                },
            )
        },
        Carrier {
            id: "collision",
            extra: |w, _, f, _, env| {
                Ok(match f {
                    // the other signer's signature, under the hint the base carries: both keys
                    // end in the same four bytes, so the hint names both
                    "signature" => {
                        let (_, b) = hint_collision();
                        let env: TransactionEnvelope = serde_json::from_value(env.clone())?;
                        let TransactionEnvelope::Tx(e) = env else {
                            bail!("the collision carrier builds a v1 envelope");
                        };
                        let hash =
                            shapekit_core_driver::tx::transaction_hash(&e.tx, &w.network_id())?;
                        let sig = shapekit_core_driver::tx::sign_hash(b, &hash);
                        vec![extra(
                            "the other signer sharing the hint signs instead",
                            serde_json::to_value(&sig.signature)?,
                        )]
                    }
                    _ => Vec::new(),
                })
            },
            ..c(
                "DecoratedSignature",
                "a payment signed by one of two signers of the source whose keys end in the same four bytes",
                |w, s| {
                    let (a, b) = hint_collision();
                    let src = s.source.clone();
                    let signer = |k: &SigningKey| {
                        OperationBody::SetOptions(SetOptionsOp {
                            signer: Some(Signer {
                                key: ed25519_signer(&k.verifying_key()),
                                weight: 1,
                            }),
                            ..Default::default()
                        })
                    };
                    w.classic_ok(&src, vec![signer(a), signer(b)], &[])?;
                    let tx = w.tx(&s.source, vec![(None, payment(&s.peers[0]))])?;
                    Ok(Base {
                        env: World::unsigned(tx),
                        plan: plan(&[a]),
                        at: Some("/tx/signatures/0".into()),
                    })
                },
            )
        },
        Carrier {
            id: "payment",
            ..c("Operation", "a payment's operation", |w, s| {
                let mut b = pay_base(w, s)?;
                b.at = Some("/tx/tx/operations/0".into());
                Ok(b)
            })
        },
        Carrier {
            id: "merge",
            ..c("Operation", "a merge of the source into a peer", |w, s| {
                let mut b = simple(
                    w,
                    s,
                    OperationBody::AccountMerge(muxed_account(&s.peers[0].verifying_key())),
                )?;
                b.at = Some("/tx/tx/operations/0".into());
                Ok(b)
            })
        },
        Carrier {
            id: "native",
            ..c("PaymentOp", "a native payment", pay_base)
        },
        Carrier {
            id: "credit",
            extra: |_, s, f, _, _| {
                Ok(match f {
                    "asset" => vec![extra(
                        "an asset whose issuer no longer exists",
                        json!({"credit_alphanum4": {
                            "asset_code": "DEAD",
                            "issuer": account_strkey(&s.fresh("dead-issuer").verifying_key()),
                        }}),
                    )],
                    _ => Vec::new(),
                })
            },
            ..c(
                "PaymentOp",
                "a payment of the shared credit, the source also holding an asset whose issuer merged away",
                |w, s| {
                    let src = s.source.clone();
                    let peer = s.peers[0].clone();
                    hold_credit(w, &src, 100 * XLM)?;
                    let prb = w.credit4();
                    w.classic_ok(&peer, vec![change_trust(&prb)], &[])?;
                    // an issuer that pays the source and the peer can then hold its asset, and
                    // merges away: a payment of that asset needs no living issuer
                    let dead = s.fresh("dead-issuer");
                    let asset = Asset::CreditAlphanum4(stellar_xdr::AlphaNum4 {
                        asset_code: AssetCode4(*b"DEAD"),
                        issuer: acc(&dead),
                    });
                    w.classic_ok(
                        &src,
                        vec![create(&dead, 100 * XLM), change_trust(&asset)],
                        &[],
                    )?;
                    w.classic_ok(&peer, vec![change_trust(&asset)], &[])?;
                    w.classic_ok(
                        &dead,
                        vec![
                            OperationBody::Payment(PaymentOp {
                                destination: muxed_account(&src.verifying_key()),
                                asset: asset.clone(),
                                amount: 100 * XLM,
                            }),
                            OperationBody::AccountMerge(muxed_account(&s.peers[2].verifying_key())),
                        ],
                        &[],
                    )?;
                    simple(
                        w,
                        s,
                        OperationBody::Payment(PaymentOp {
                            destination: muxed_account(&peer.verifying_key()),
                            asset: prb,
                            amount: 10 * XLM,
                        }),
                    )
                },
            )
        },
        c(
            "AlphaNum4",
            "a payment of the shared four letter credit to a peer that trusts it",
            |w, s| credit_payment(w, s, w.credit4(), "credit_alphanum4"),
        ),
        c(
            "AlphaNum12",
            "a payment of the shared twelve letter credit to a peer that trusts it",
            |w, s| credit_payment(w, s, w.credit12(), "credit_alphanum12"),
        ),
        Carrier {
            id: "tx-source",
            ..c(
                "MuxedAccountMed25519",
                "a payment's muxed source account, id 7",
                |w, s| {
                    let mut tx = w.tx(&s.source, vec![(None, payment(&s.peers[0]))])?;
                    tx.source_account = muxed7(&s.source);
                    Ok(Base {
                        env: World::unsigned(tx),
                        plan: plan(&[&s.source]),
                        at: Some("/tx/tx/source_account".into()),
                    })
                },
            )
        },
        Carrier {
            id: "op-source",
            ..c(
                "MuxedAccountMed25519",
                "a payment's muxed operation source, id 7",
                |w, s| {
                    let mut tx = w.tx(&s.source, vec![(None, payment(&s.peers[0]))])?;
                    let mut ops = tx.operations.to_vec();
                    ops[0].source_account = Some(muxed7(&s.source));
                    tx.operations = ops.try_into()?;
                    Ok(Base {
                        env: World::unsigned(tx),
                        plan: plan(&[&s.source]),
                        at: Some("/tx/tx/operations/0/source_account".into()),
                    })
                },
            )
        },
        Carrier {
            id: "payment",
            ..c(
                "MuxedAccountMed25519",
                "a payment's muxed destination, id 7",
                |w, s| {
                    let b = simple(
                        w,
                        s,
                        OperationBody::Payment(PaymentOp {
                            destination: muxed7(&s.peers[0]),
                            asset: Asset::Native,
                            amount: XLM,
                        }),
                    )?;
                    Ok(Base {
                        at: Some("/tx/tx/operations/0/body/payment/destination".into()),
                        ..b
                    })
                },
            )
        },
        Carrier {
            id: "path-payment",
            ..c(
                "MuxedAccountMed25519",
                "a strict receive path payment's muxed destination, id 7",
                |w, s| {
                    let b = simple(
                        w,
                        s,
                        OperationBody::PathPaymentStrictReceive(PathPaymentStrictReceiveOp {
                            send_asset: Asset::Native,
                            send_max: 2 * XLM,
                            destination: muxed7(&s.peers[0]),
                            dest_asset: Asset::Native,
                            dest_amount: XLM,
                            path: Default::default(),
                        }),
                    )?;
                    Ok(Base {
                        at: Some(
                            "/tx/tx/operations/0/body/path_payment_strict_receive/destination"
                                .into(),
                        ),
                        ..b
                    })
                },
            )
        },
        Carrier {
            id: "clawback",
            ..c(
                "MuxedAccountMed25519",
                "a clawback from a muxed holder, id 7",
                |w, s| {
                    let peer = clawback_setup(w, s)?;
                    let b = simple(
                        w,
                        s,
                        OperationBody::Clawback(ClawbackOp {
                            asset: own_asset(s),
                            from: muxed7(&peer),
                            amount: 10 * XLM,
                        }),
                    )?;
                    Ok(Base {
                        at: Some("/tx/tx/operations/0/body/clawback/from".into()),
                        ..b
                    })
                },
            )
        },
        Carrier {
            id: "fee-source",
            ..c(
                "MuxedAccountMed25519",
                "a fee bump's muxed fee source, id 7",
                |w, s| {
                    let mut b = fee_bumped(w, s, "/tx_fee_bump/tx/fee_source")?;
                    let TransactionEnvelope::TxFeeBump(e) = &mut b.env else {
                        bail!("a fee bump");
                    };
                    e.tx.fee_source = muxed7(&s.peers[2]);
                    Ok(b)
                },
            )
        },
        Carrier {
            extra: |_, _, f, _, _| {
                Ok(match f {
                    "maxTime" => vec![extra("a close time already past, 1", json!("1"))],
                    _ => Vec::new(),
                })
            },
            ..c("TimeBounds", "a payment's time bounds", |w, s| {
                let tb = TimeBounds {
                    min_time: TimePoint(0),
                    max_time: TimePoint(later(w)),
                };
                with_cond(w, s, Preconditions::Time(tb), &[&s.source])
            })
        },
        c(
            "PreconditionsV2",
            "a payment's v2 preconditions, every field set, one extra signer",
            |w, s| {
                let p = v2(w, s)?;
                with_cond(w, s, Preconditions::V2(p), &[&s.source, &s.peers[1]])
            },
        ),
        Carrier {
            extra: |w, _, f, _, _| {
                Ok(match f {
                    "maxLedger" => {
                        let next = w.driver.last_closed() + 1;
                        vec![extra(
                            &format!("the ledger the attempt closes in, {next}"),
                            json!(next),
                        )]
                    }
                    _ => Vec::new(),
                })
            },
            ..c("LedgerBounds", "a payment's ledger bounds", |w, s| {
                let p = PreconditionsV2 {
                    ledger_bounds: Some(LedgerBounds {
                        min_ledger: 0,
                        max_ledger: w.driver.last_closed() + 1000,
                    }),
                    ..Default::default()
                };
                with_cond(w, s, Preconditions::V2(p), &[&s.source])
            })
        },
        c(
            "SignerKeyEd25519SignedPayload",
            "a payment's extra signer, a 32 byte signed payload",
            |w, s| {
                let payload = [0x5a; 32];
                let p = PreconditionsV2 {
                    extra_signers: vec![signed_payload_signer(
                        &s.peers[1].verifying_key(),
                        &payload,
                    )?]
                    .try_into()?,
                    ..Default::default()
                };
                let mut b = with_cond(w, s, Preconditions::V2(p), &[&s.source])?;
                let at = "/tx/tx/cond/v2/extra_signers/0".to_string();
                b.plan.payloads.push((s.peers[1].clone(), at.clone()));
                b.at = Some(at);
                Ok(b)
            },
        ),
        c(
            "FeeBumpTransactionEnvelope",
            "a fee bump of a payment",
            |w, s| fee_bumped(w, s, "/tx_fee_bump"),
        ),
        c("FeeBumpTransaction", "a fee bump of a payment", |w, s| {
            fee_bumped(w, s, "/tx_fee_bump/tx")
        }),
        Carrier {
            id: "p12",
            world: WorldKind::V0,
            ..c(
                "TransactionV0Envelope",
                "a payment's v0 envelope at protocol 12",
                |w, s| v0(w, s, "/tx_v0"),
            )
        },
        Carrier {
            id: "p30",
            ..c(
                "TransactionV0Envelope",
                "a payment's v0 envelope at protocol 30, which core's tx endpoint turns into a v1 envelope before it validates it (stellar-core v30.0.0 src/main/CommandHandler.cpp 1074-1081)",
                |w, s| v0(w, s, "/tx_v0"),
            )
        },
        Carrier {
            id: "p12",
            world: WorldKind::V0,
            ..c(
                "TransactionV0",
                "a payment in a v0 envelope at protocol 12",
                |w, s| v0(w, s, "/tx_v0/tx"),
            )
        },
        Carrier {
            id: "p30",
            ..c(
                "TransactionV0",
                "a payment in a v0 envelope at protocol 30, which core's tx endpoint turns into a v1 envelope before it validates it (stellar-core v30.0.0 src/main/CommandHandler.cpp 1074-1081)",
                |w, s| v0(w, s, "/tx_v0/tx"),
            )
        },
        Carrier {
            extra: |_, s, f, _, _| {
                Ok(match f {
                    "destination" => vec![
                        extra(
                            "the source itself",
                            json!(account_strkey(&s.source.verifying_key())),
                        ),
                        extra(
                            "an account that exists, peer 0",
                            json!(account_strkey(&s.peers[0].verifying_key())),
                        ),
                    ],
                    _ => Vec::new(),
                })
            },
            ..c(
                "CreateAccountOp",
                "a create account of a new key",
                |w, s| simple(w, s, create(&s.fresh("created"), 100 * XLM)),
            )
        },
        Carrier {
            extra: path_extra,
            ..c(
                "PathPaymentStrictReceiveOp",
                "a native to native path payment, no path",
                |w, s| {
                    simple(
                        w,
                        s,
                        OperationBody::PathPaymentStrictReceive(PathPaymentStrictReceiveOp {
                            send_asset: Asset::Native,
                            send_max: 2 * XLM,
                            destination: muxed_account(&s.peers[0].verifying_key()),
                            dest_asset: Asset::Native,
                            dest_amount: XLM,
                            path: Default::default(),
                        }),
                    )
                },
            )
        },
        Carrier {
            extra: path_extra,
            ..c(
                "PathPaymentStrictSendOp",
                "a native to native path payment, no path",
                |w, s| {
                    simple(
                        w,
                        s,
                        OperationBody::PathPaymentStrictSend(PathPaymentStrictSendOp {
                            send_asset: Asset::Native,
                            send_amount: XLM,
                            destination: muxed_account(&s.peers[0].verifying_key()),
                            dest_asset: Asset::Native,
                            dest_min: XLM,
                            path: Default::default(),
                        }),
                    )
                },
            )
        },
        c(
            "ManageSellOfferOp",
            "a new offer selling lumens for the shared credit",
            |w, s| {
                let src = s.source.clone();
                let a = w.credit4();
                w.classic_ok(&src, vec![change_trust(&a)], &[])?;
                simple(w, s, sell_offer(Asset::Native, a))
            },
        ),
        c("Price", "the price of a new sell offer", |w, s| {
            let src = s.source.clone();
            let a = w.credit4();
            w.classic_ok(&src, vec![change_trust(&a)], &[])?;
            simple(w, s, sell_offer(Asset::Native, a))
        }),
        c(
            "ManageBuyOfferOp",
            "a new offer buying the shared credit for lumens",
            |w, s| {
                let src = s.source.clone();
                let a = w.credit4();
                w.classic_ok(&src, vec![change_trust(&a)], &[])?;
                simple(
                    w,
                    s,
                    OperationBody::ManageBuyOffer(ManageBuyOfferOp {
                        selling: Asset::Native,
                        buying: a,
                        buy_amount: 10 * XLM,
                        price: Price { n: 1, d: 1 },
                        offer_id: 0,
                    }),
                )
            },
        ),
        c(
            "CreatePassiveSellOfferOp",
            "a passive offer selling lumens for the shared credit",
            |w, s| {
                let src = s.source.clone();
                let a = w.credit4();
                w.classic_ok(&src, vec![change_trust(&a)], &[])?;
                simple(
                    w,
                    s,
                    OperationBody::CreatePassiveSellOffer(CreatePassiveSellOfferOp {
                        selling: Asset::Native,
                        buying: a,
                        amount: 10 * XLM,
                        price: Price { n: 1, d: 1 },
                    }),
                )
            },
        ),
        Carrier {
            extra: |_, s, f, _, _| {
                Ok(match f {
                    "signer" => vec![extra(
                        "the source's own key, weight 1",
                        json!({"key": account_strkey(&s.source.verifying_key()), "weight": 1}),
                    )],
                    "homeDomain" => vec![extra(
                        "printable ascii that is no domain name",
                        json!("no domain, just text!"),
                    )],
                    _ => Vec::new(),
                })
            },
            ..c(
                "SetOptionsOp",
                "a set options with every field set",
                |w, s| {
                    simple(
                        w,
                        s,
                        OperationBody::SetOptions(SetOptionsOp {
                            inflation_dest: Some(acc(&s.source)),
                            clear_flags: Some(0),
                            set_flags: Some(0),
                            master_weight: Some(1),
                            low_threshold: Some(0),
                            med_threshold: Some(0),
                            high_threshold: Some(0),
                            home_domain: Some(stellar_xdr::String32("probe.example".try_into()?)),
                            signer: Some(Signer {
                                key: ed25519_signer(&s.peers[0].verifying_key()),
                                weight: 1,
                            }),
                        }),
                    )
                },
            )
        },
        c("Signer", "a signer added by set options", |w, s| {
            simple(
                w,
                s,
                OperationBody::SetOptions(SetOptionsOp {
                    signer: Some(Signer {
                        key: ed25519_signer(&s.peers[0].verifying_key()),
                        weight: 1,
                    }),
                    ..Default::default()
                }),
            )
        }),
        c(
            "ChangeTrustOp",
            "a new trustline to the shared credit",
            |w, s| {
                let a = w.credit4();
                simple(w, s, change_trust(&a))
            },
        ),
        c(
            "LiquidityPoolConstantProductParameters",
            "a new pool share trustline, native and the shared credit, fee 30",
            |w, s| {
                let src = s.source.clone();
                let a = w.credit4();
                w.classic_ok(&src, vec![change_trust(&a)], &[])?;
                let params = pool_params(w);
                simple(
                    w,
                    s,
                    OperationBody::ChangeTrust(ChangeTrustOp {
                        line: ChangeTrustAsset::PoolShare(params),
                        limit: i64::MAX,
                    }),
                )
            },
        ),
        c(
            "AllowTrustOp",
            "an issuer with auth required authorizing a trustline",
            |w, s| {
                let (src, peer) = (s.source.clone(), s.peers[0].clone());
                w.classic_ok(&src, vec![set_flags(AUTH_REQUIRED | AUTH_REVOCABLE)], &[])?;
                w.classic_ok(&peer, vec![change_trust(&own_asset(s))], &[])?;
                simple(
                    w,
                    s,
                    OperationBody::AllowTrust(AllowTrustOp {
                        trustor: acc(&peer),
                        asset: AssetCode::CreditAlphanum4(AssetCode4(*b"SUBJ")),
                        authorize: 1,
                    }),
                )
            },
        ),
        c(
            "SetTrustLineFlagsOp",
            "an issuer with auth required authorizing a trustline",
            |w, s| {
                let (src, peer) = (s.source.clone(), s.peers[0].clone());
                w.classic_ok(&src, vec![set_flags(AUTH_REQUIRED | AUTH_REVOCABLE)], &[])?;
                w.classic_ok(&peer, vec![change_trust(&own_asset(s))], &[])?;
                simple(
                    w,
                    s,
                    OperationBody::SetTrustLineFlags(SetTrustLineFlagsOp {
                        trustor: acc(&peer),
                        asset: own_asset(s),
                        clear_flags: 0,
                        set_flags: 1,
                    }),
                )
            },
        ),
        Carrier {
            extra: |_, _, f, _, _| {
                Ok(match f {
                    "dataName" => vec![extra(
                        "printable ascii of any content",
                        json!("a name, with spaces & signs?"),
                    )],
                    _ => Vec::new(),
                })
            },
            ..c("ManageDataOp", "a new data entry", |w, s| {
                simple(
                    w,
                    s,
                    OperationBody::ManageData(ManageDataOp {
                        data_name: stellar_xdr::String64("probe".try_into()?),
                        data_value: Some(vec![1u8].try_into()?),
                    }),
                )
            })
        },
        c(
            "BumpSequenceOp",
            "a bump of the source's sequence number by 10",
            |w, s| {
                let to = w.next_seq(&s.source)? + 9;
                simple(
                    w,
                    s,
                    OperationBody::BumpSequence(BumpSequenceOp {
                        bump_to: SequenceNumber(to),
                    }),
                )
            },
        ),
        c(
            "CreateClaimableBalanceOp",
            "a native claimable balance with one unconditional claimant",
            create_balance,
        ),
        Carrier {
            id: "two",
            extra: |_, s, f, _, _| {
                Ok(match f {
                    "destination" => vec![extra(
                        "the other claimant's destination",
                        json!(account_strkey(&s.peers[1].verifying_key())),
                    )],
                    _ => Vec::new(),
                })
            },
            ..c(
                "ClaimantV0",
                "the first of two claimants of a native claimable balance",
                |w, s| {
                    let claimant = |k: &SigningKey| {
                        Claimant::ClaimantTypeV0(ClaimantV0 {
                            destination: acc(k),
                            predicate: ClaimPredicate::Unconditional,
                        })
                    };
                    let b = simple(
                        w,
                        s,
                        OperationBody::CreateClaimableBalance(CreateClaimableBalanceOp {
                            asset: Asset::Native,
                            amount: XLM,
                            claimants: vec![claimant(&s.peers[0]), claimant(&s.peers[1])]
                                .try_into()?,
                        }),
                    )?;
                    Ok(Base {
                        at: Some(
                            "/tx/tx/operations/0/body/create_claimable_balance/claimants/0/claimant_type_v0"
                                .into(),
                        ),
                        ..b
                    })
                },
            )
        },
        Carrier {
            id: "one",
            ..c(
                "ClaimantV0",
                "the claimant of a native claimable balance",
                create_balance,
            )
        },
        c(
            "ClaimClaimableBalanceOp",
            "a claim of a balance created for the source",
            |w, s| {
                let peer = s.peers[0].clone();
                let id = claimable(w, &peer, &s.source, Asset::Native)?;
                simple(
                    w,
                    s,
                    OperationBody::ClaimClaimableBalance(ClaimClaimableBalanceOp {
                        balance_id: id,
                    }),
                )
            },
        ),
        c(
            "ClawbackOp",
            "an issuer with clawback taking back part of a holding",
            |w, s| {
                let (src, peer) = (s.source.clone(), s.peers[0].clone());
                w.classic_ok(&src, vec![set_flags(AUTH_REVOCABLE | AUTH_CLAWBACK)], &[])?;
                w.classic_ok(&peer, vec![change_trust(&own_asset(s))], &[])?;
                w.classic_ok(
                    &src,
                    vec![OperationBody::Payment(PaymentOp {
                        destination: muxed_account(&peer.verifying_key()),
                        asset: own_asset(s),
                        amount: 100 * XLM,
                    })],
                    &[],
                )?;
                simple(
                    w,
                    s,
                    OperationBody::Clawback(ClawbackOp {
                        asset: own_asset(s),
                        from: muxed_account(&peer.verifying_key()),
                        amount: 10 * XLM,
                    }),
                )
            },
        ),
        c(
            "ClawbackClaimableBalanceOp",
            "an issuer with clawback taking back a balance of its asset",
            |w, s| {
                let (src, peer) = (s.source.clone(), s.peers[0].clone());
                w.classic_ok(&src, vec![set_flags(AUTH_REVOCABLE | AUTH_CLAWBACK)], &[])?;
                w.classic_ok(&peer, vec![change_trust(&own_asset(s))], &[])?;
                let id = claimable(w, &src, &peer, own_asset(s))?;
                simple(
                    w,
                    s,
                    OperationBody::ClawbackClaimableBalance(ClawbackClaimableBalanceOp {
                        balance_id: id,
                    }),
                )
            },
        ),
        Carrier {
            id: "create",
            ..c(
                "BeginSponsoringFutureReservesOp",
                "the source sponsoring an account the same transaction creates, which does not exist when core checks the operation",
                |w, s| {
                    let fresh = s.fresh("sponsored");
                    let tx = w.tx(
                        &s.source,
                        vec![
                            (None, begin(&fresh)),
                            (None, create(&fresh, 100 * XLM)),
                            (Some(&fresh), OperationBody::EndSponsoringFutureReserves),
                        ],
                    )?;
                    Ok(Base {
                        env: World::unsigned(tx),
                        plan: plan(&[&s.source, &fresh]),
                        at: Some(
                            "/tx/tx/operations/0/body/begin_sponsoring_future_reserves".into(),
                        ),
                    })
                },
            )
        },
        Carrier {
            id: "peer",
            ..c(
                "BeginSponsoringFutureReservesOp",
                "the source sponsoring a data entry of a peer",
                |w, s| {
                    let peer = &s.peers[0];
                    let tx = w.tx(
                        &s.source,
                        vec![
                            (None, begin(peer)),
                            (
                                Some(peer),
                                OperationBody::ManageData(ManageDataOp {
                                    data_name: stellar_xdr::String64("sponsored".try_into()?),
                                    data_value: Some(vec![1u8].try_into()?),
                                }),
                            ),
                            (Some(peer), OperationBody::EndSponsoringFutureReserves),
                        ],
                    )?;
                    Ok(Base {
                        env: World::unsigned(tx),
                        plan: plan(&[&s.source, peer]),
                        at: None,
                    })
                },
            )
        },
        c(
            "RevokeSponsorshipOpSigner",
            "the sponsor of a peer's signer revoking the sponsorship",
            |w, s| {
                let peer = s.peers[0].clone();
                let key = ed25519_signer(&s.peers[1].verifying_key());
                sponsor(
                    w,
                    s,
                    &peer,
                    OperationBody::SetOptions(SetOptionsOp {
                        signer: Some(Signer {
                            key: key.clone(),
                            weight: 1,
                        }),
                        ..Default::default()
                    }),
                )?;
                simple(
                    w,
                    s,
                    OperationBody::RevokeSponsorship(RevokeSponsorshipOp::Signer(
                        RevokeSponsorshipOpSigner {
                            account_id: acc(&peer),
                            signer_key: key,
                        },
                    )),
                )
            },
        ),
        c(
            "LedgerKeyAccount",
            "the sponsor of an account it created revoking the sponsorship",
            |w, s| {
                let fresh = s.fresh("sponsored");
                let src = s.source.clone();
                w.ops_ok(
                    &src,
                    vec![
                        (None, begin(&fresh)),
                        (None, create(&fresh, 100 * XLM)),
                        (Some(&fresh), OperationBody::EndSponsoringFutureReserves),
                    ],
                    &[&fresh],
                )?;
                simple(
                    w,
                    s,
                    revoke(LedgerKey::Account(LedgerKeyAccount {
                        account_id: acc(&fresh),
                    })),
                )
            },
        ),
        c(
            "LedgerKeyTrustLine",
            "the sponsor of a peer's trustline revoking the sponsorship",
            |w, s| {
                let peer = s.peers[0].clone();
                let a = w.credit4();
                sponsor(w, s, &peer, change_trust(&a))?;
                simple(
                    w,
                    s,
                    revoke(LedgerKey::Trustline(LedgerKeyTrustLine {
                        account_id: acc(&peer),
                        asset: credit_trustline(w),
                    })),
                )
            },
        ),
        c(
            "LedgerKeyOffer",
            "the sponsor of a peer's offer revoking the sponsorship",
            |w, s| {
                let peer = s.peers[0].clone();
                let a = w.credit4();
                w.classic_ok(&peer, vec![change_trust(&a)], &[])?;
                let applied = sponsor(w, s, &peer, sell_offer(Asset::Native, a))?;
                let offer_id = match op_result(&applied, 1)? {
                    OperationResultTr::ManageSellOffer(ManageSellOfferResult::Success(r)) => {
                        match &r.offer {
                            ManageOfferSuccessResultOffer::Created(o) => o.offer_id,
                            other => bail!("the sponsored offer was not created: {other:?}"),
                        }
                    }
                    other => bail!("the sponsored offer: {other:?}"),
                };
                simple(
                    w,
                    s,
                    revoke(LedgerKey::Offer(LedgerKeyOffer {
                        seller_id: acc(&peer),
                        offer_id,
                    })),
                )
            },
        ),
        c(
            "LedgerKeyData",
            "the sponsor of a peer's data entry revoking the sponsorship",
            |w, s| {
                let peer = s.peers[0].clone();
                sponsor(
                    w,
                    s,
                    &peer,
                    OperationBody::ManageData(ManageDataOp {
                        data_name: stellar_xdr::String64("sponsored".try_into()?),
                        data_value: Some(vec![1u8].try_into()?),
                    }),
                )?;
                simple(
                    w,
                    s,
                    revoke(LedgerKey::Data(LedgerKeyData {
                        account_id: acc(&peer),
                        data_name: stellar_xdr::String64("sponsored".try_into()?),
                    })),
                )
            },
        ),
        c(
            "LedgerKeyClaimableBalance",
            "the sponsor of a claimable balance handing the sponsorship to a peer",
            |w, s| {
                let (src, peer) = (s.source.clone(), s.peers[0].clone());
                let id = claimable(w, &src, &s.peers[1], Asset::Native)?;
                let tx = w.tx(
                    &src,
                    vec![
                        (Some(&peer), begin(&src)),
                        (
                            None,
                            revoke(LedgerKey::ClaimableBalance(LedgerKeyClaimableBalance {
                                balance_id: id,
                            })),
                        ),
                        (None, OperationBody::EndSponsoringFutureReserves),
                    ],
                )?;
                Ok(Base {
                    env: World::unsigned(tx),
                    plan: plan(&[&src, &peer]),
                    at: None,
                })
            },
        ),
        Carrier {
            outcome: BaseOutcome::Refused(
                "RevokeSponsorshipOpFrame::doCheckValid answers REVOKE_SPONSORSHIP_MALFORMED for a liquidity pool key (stellar-core v30.0.0 src/transactions/RevokeSponsorshipOpFrame.cpp 444-450); a footprint refuses it too (TransactionFrame.cpp 871-883)",
            ),
            ..c(
                "LedgerKeyLiquidityPool",
                "a revoke sponsorship of the native and shared credit pool",
                |w, s| {
                    let id = pool_id(w)?;
                    simple(
                        w,
                        s,
                        revoke(LedgerKey::LiquidityPool(LedgerKeyLiquidityPool {
                            liquidity_pool_id: id,
                        })),
                    )
                },
            )
        },
        c(
            "LiquidityPoolDepositOp",
            "a deposit into the native and shared credit pool",
            |w, s| {
                let src = s.source.clone();
                pool_member(w, &src)?;
                let d = deposit(w)?;
                simple(w, s, d)
            },
        ),
        c(
            "LiquidityPoolWithdrawOp",
            "a withdrawal from the native and shared credit pool",
            |w, s| {
                let src = s.source.clone();
                pool_member(w, &src)?;
                let d = deposit(w)?;
                w.classic_ok(&src, vec![d], &[])?;
                let id = pool_id(w)?;
                simple(
                    w,
                    s,
                    OperationBody::LiquidityPoolWithdraw(LiquidityPoolWithdrawOp {
                        liquidity_pool_id: id,
                        amount: XLM,
                        min_amount_a: 1,
                        min_amount_b: 1,
                    }),
                )
            },
        ),
    ]
}

fn create_balance(w: &mut World, s: &Subject) -> Result<Base> {
    simple(
        w,
        s,
        OperationBody::CreateClaimableBalance(CreateClaimableBalanceOp {
            asset: Asset::Native,
            amount: XLM,
            claimants: vec![Claimant::ClaimantTypeV0(ClaimantV0 {
                destination: acc(&s.peers[0]),
                predicate: ClaimPredicate::Unconditional,
            })]
            .try_into()?,
        }),
    )
}

/// A payment from the subject, fee bumped by peer 2.
fn fee_bumped(w: &mut World, s: &Subject, at: &str) -> Result<Base> {
    let tx = w.tx(&s.source, vec![(None, payment(&s.peers[0]))])?;
    let inner = stellar_xdr::TransactionV1Envelope {
        tx,
        signatures: Default::default(),
    };
    let outer = fee_bump(
        inner,
        muxed_account(&s.peers[2].verifying_key()),
        4 * i64::from(BASE_FEE),
    );
    Ok(Base {
        env: TransactionEnvelope::TxFeeBump(FeeBumpTransactionEnvelope {
            tx: outer,
            signatures: Default::default(),
        }),
        plan: SignPlan {
            tx: vec![s.source.clone()],
            fee_bump: vec![s.peers[2].clone()],
            ..Default::default()
        },
        at: Some(at.into()),
    })
}

/// A payment from the subject in a v0 envelope.
fn v0(w: &mut World, s: &Subject, at: &str) -> Result<Base> {
    let tx = w.tx(&s.source, vec![(None, payment(&s.peers[0]))])?;
    let tx = shapekit_core_driver::tx::to_v0(tx)?;
    Ok(Base {
        env: TransactionEnvelope::TxV0(TransactionV0Envelope {
            tx,
            signatures: Default::default(),
        }),
        plan: plan(&[&s.source]),
        at: Some(at.into()),
    })
}

/// Every object in `doc` whose keys are exactly `keys`, by JSON pointer.
pub fn locate(doc: &serde_json::Value, keys: &[String]) -> Vec<String> {
    fn walk(v: &serde_json::Value, at: String, keys: &[String], out: &mut Vec<String>) {
        match v {
            serde_json::Value::Object(m) => {
                if m.len() == keys.len() && keys.iter().all(|k| m.contains_key(k)) {
                    out.push(at.clone());
                }
                for (k, c) in m {
                    walk(
                        c,
                        format!("{at}/{}", k.replace('~', "~0").replace('/', "~1")),
                        keys,
                        out,
                    );
                }
            }
            serde_json::Value::Array(a) => {
                for (i, c) in a.iter().enumerate() {
                    walk(c, format!("{at}/{i}"), keys, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(doc, String::new(), keys, &mut out);
    out
}

/// The pointer of the carrier's struct instance in `doc`.
pub fn instance(
    doc: &serde_json::Value,
    base: &Base,
    keys: &[String],
    structure: &str,
) -> Result<String> {
    if let Some(at) = &base.at {
        doc.pointer(at)
            .with_context(|| format!("{structure}: nothing at {at}"))?;
        return Ok(at.clone());
    }
    let found = locate(doc, keys);
    match found.as_slice() {
        [one] => Ok(one.clone()),
        [] => bail!("{structure}: no object with keys {keys:?} in {doc}"),
        many => bail!(
            "{structure}: {} objects with keys {keys:?}: {many:?}",
            many.len()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hint_collision_is_two_keys_sharing_their_last_four_bytes() {
        let (a, b) = hint_collision();
        let (a, b) = (a.verifying_key().to_bytes(), b.verifying_key().to_bytes());
        assert_ne!(a, b);
        assert_eq!(a[28..], b[28..]);
    }
}
