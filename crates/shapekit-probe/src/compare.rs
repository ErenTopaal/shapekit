//! `registry/fields.toml` against the measured verdicts. Each of its rows names a field in
//! words and an XDR path that can reach through union arms and list several fields; here
//! every row is tied to the struct fields of `validation.json` it speaks about, and to the
//! carrier and values that put the row's question to core. A row that says `yes` agrees when
//! every one of its sites is validated, a row that says `no` when none is, and a `partial` row
//! when at least one is.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, ensure};
use shapekit_contracts::validation::{FieldVerdict, ProbeAttempt, ValidationTable};

/// Where a row's question is put to core.
pub struct Site {
    /// The `validation.json` path.
    pub path: &'static str,
    /// The carrier whose attempts answer it, empty for every carrier of the path.
    pub carrier: &'static str,
    /// Only the replacements whose name contains this, the base attempt always included.
    pub only: Option<&'static str>,
    /// Leaves out the replacements whose name contains this.
    pub except: Option<&'static str>,
    /// Only the base attempt: the row is about the transaction the carrier builds, not about
    /// a value of the field.
    pub bases: bool,
}

const fn at(path: &'static str) -> Site {
    Site {
        path,
        carrier: "",
        only: None,
        except: None,
        bases: false,
    }
}

const fn bases(path: &'static str, carrier: &'static str) -> Site {
    Site {
        path,
        carrier,
        only: None,
        except: None,
        bases: true,
    }
}

const fn via(path: &'static str, carrier: &'static str) -> Site {
    Site {
        path,
        carrier,
        only: None,
        except: None,
        bases: false,
    }
}

const fn only(path: &'static str, carrier: &'static str, name: &'static str) -> Site {
    Site {
        path,
        carrier,
        only: Some(name),
        except: None,
        bases: false,
    }
}

const fn except(path: &'static str, carrier: &'static str, name: &'static str) -> Site {
    Site {
        path,
        carrier,
        only: None,
        except: Some(name),
        bases: false,
    }
}

/// The sites of every row of `registry/fields.toml`, by row id.
pub fn sites() -> Vec<(&'static str, Vec<Site>)> {
    vec![
        (
            "tx.envelope.type",
            vec![
                bases("TransactionV0Envelope.tx", "p12"),
                bases("TransactionV0Envelope.tx", "p30"),
                bases("TransactionV1Envelope.tx", ""),
                bases("FeeBumpTransactionEnvelope.tx", ""),
            ],
        ),
        (
            "tx.v0.source_account_ed25519",
            vec![via("TransactionV0.sourceAccountEd25519", "p12")],
        ),
        (
            "tx.signatures",
            vec![at("TransactionV1Envelope.signatures")],
        ),
        ("tx.signatures.hint", vec![at("DecoratedSignature.hint")]),
        (
            "tx.signatures.signature",
            vec![at("DecoratedSignature.signature")],
        ),
        (
            "tx.source_account",
            vec![via("Transaction.sourceAccount", "payment")],
        ),
        (
            "tx.source_account.mux_id",
            vec![via("MuxedAccountMed25519.id", "tx-source")],
        ),
        ("tx.fee", vec![via("Transaction.fee", "payment")]),
        ("tx.seq_num", vec![via("Transaction.seqNum", "payment")]),
        ("tx.cond.type", vec![via("Transaction.cond", "payment")]),
        (
            "tx.cond.time_bounds",
            vec![at("TimeBounds.minTime"), at("TimeBounds.maxTime")],
        ),
        (
            "tx.cond.ledger_bounds",
            vec![at("LedgerBounds.minLedger"), at("LedgerBounds.maxLedger")],
        ),
        ("tx.cond.min_seq_num", vec![at("PreconditionsV2.minSeqNum")]),
        ("tx.cond.min_seq_age", vec![at("PreconditionsV2.minSeqAge")]),
        (
            "tx.cond.min_seq_ledger_gap",
            vec![at("PreconditionsV2.minSeqLedgerGap")],
        ),
        (
            "tx.cond.extra_signers",
            vec![at("PreconditionsV2.extraSigners")],
        ),
        (
            "signer_key.signed_payload.payload",
            vec![
                only("SignerKeyEd25519SignedPayload.payload", "", "empty"),
                except("SignerKeyEd25519SignedPayload.payload", "", "empty"),
            ],
        ),
        (
            "tx.memo.type",
            vec![
                via("Transaction.memo", "invoke"),
                via("Transaction.memo", "payment"),
            ],
        ),
        (
            "tx.memo.text",
            vec![only("Transaction.memo", "payment", "arm text")],
        ),
        (
            "tx.memo.id",
            vec![only("Transaction.memo", "payment", "arm id")],
        ),
        (
            "tx.memo.hash",
            vec![only("Transaction.memo", "payment", "arm hash")],
        ),
        (
            "tx.memo.return",
            vec![only("Transaction.memo", "payment", "arm return")],
        ),
        (
            "tx.operations",
            vec![via("Transaction.operations", "payment")],
        ),
        ("tx.ext", vec![via("Transaction.ext", "payment")]),
        (
            "tx.soroban.resource_fee",
            vec![at("SorobanTransactionData.resourceFee")],
        ),
        (
            "tx.soroban.resources.limits",
            vec![
                at("SorobanResources.instructions"),
                at("SorobanResources.diskReadBytes"),
                at("SorobanResources.writeBytes"),
            ],
        ),
        (
            "tx.soroban.footprint",
            vec![
                except("LedgerFootprint.readOnly", "echo", "never touches"),
                only("LedgerFootprint.readOnly", "echo", "never touches"),
                except("LedgerFootprint.readWrite", "echo", "never touches"),
                only("LedgerFootprint.readWrite", "echo", "never touches"),
            ],
        ),
        (
            "tx.soroban.ext.archived_soroban_entries",
            vec![at("SorobanResourcesExtV0.archivedSorobanEntries")],
        ),
        (
            "feebump.fee_source",
            vec![at("FeeBumpTransaction.feeSource")],
        ),
        (
            "feebump.fee_source.mux_id",
            vec![via("MuxedAccountMed25519.id", "fee-source")],
        ),
        ("feebump.fee", vec![at("FeeBumpTransaction.fee")]),
        (
            "feebump.signatures",
            vec![at("FeeBumpTransactionEnvelope.signatures")],
        ),
        ("feebump.inner_tx", vec![at("FeeBumpTransaction.innerTx")]),
        (
            "op.source_account",
            vec![via("Operation.sourceAccount", "payment")],
        ),
        (
            "op.source_account.mux_id",
            vec![via("MuxedAccountMed25519.id", "op-source")],
        ),
        (
            "op.create_account.destination",
            vec![at("CreateAccountOp.destination")],
        ),
        (
            "op.create_account.starting_balance",
            vec![at("CreateAccountOp.startingBalance")],
        ),
        ("op.payment.destination", vec![at("PaymentOp.destination")]),
        (
            "op.payment.destination.mux_id",
            vec![via("MuxedAccountMed25519.id", "payment")],
        ),
        (
            "op.payment.asset",
            vec![
                via("PaymentOp.asset", "native"),
                only("PaymentOp.asset", "credit", "issuer no longer exists"),
            ],
        ),
        ("op.payment.amount", vec![at("PaymentOp.amount")]),
        (
            "op.path_payment.destination",
            vec![
                at("PathPaymentStrictReceiveOp.destination"),
                at("PathPaymentStrictSendOp.destination"),
            ],
        ),
        (
            "op.path_payment.destination.mux_id",
            vec![via("MuxedAccountMed25519.id", "path-payment")],
        ),
        (
            "op.path_payment.amounts",
            vec![
                at("PathPaymentStrictReceiveOp.sendMax"),
                at("PathPaymentStrictReceiveOp.destAmount"),
                at("PathPaymentStrictSendOp.sendAmount"),
                at("PathPaymentStrictSendOp.destMin"),
            ],
        ),
        (
            "op.path_payment.path",
            vec![
                only("PathPaymentStrictReceiveOp.path", "", "code is empty"),
                except("PathPaymentStrictReceiveOp.path", "", "code is empty"),
                only("PathPaymentStrictSendOp.path", "", "code is empty"),
                except("PathPaymentStrictSendOp.path", "", "code is empty"),
            ],
        ),
        (
            "op.manage_offer.offer_id",
            vec![
                at("ManageSellOfferOp.offerID"),
                at("ManageBuyOfferOp.offerID"),
            ],
        ),
        (
            "op.manage_offer.assets_amount_price",
            vec![
                at("ManageSellOfferOp.selling"),
                at("ManageSellOfferOp.buying"),
                at("ManageSellOfferOp.amount"),
                at("ManageSellOfferOp.price"),
                at("ManageBuyOfferOp.selling"),
                at("ManageBuyOfferOp.buying"),
                at("ManageBuyOfferOp.buyAmount"),
                at("ManageBuyOfferOp.price"),
                at("CreatePassiveSellOfferOp.selling"),
                at("CreatePassiveSellOfferOp.buying"),
                at("CreatePassiveSellOfferOp.amount"),
                at("CreatePassiveSellOfferOp.price"),
                at("Price.n"),
                at("Price.d"),
            ],
        ),
        (
            "op.set_options.home_domain",
            vec![
                except("SetOptionsOp.homeDomain", "", "printable ascii"),
                only("SetOptionsOp.homeDomain", "", "printable ascii"),
            ],
        ),
        (
            "op.set_options.inflation_dest",
            vec![at("SetOptionsOp.inflationDest")],
        ),
        (
            "op.set_options.signer",
            vec![
                at("SetOptionsOp.signer"),
                at("Signer.key"),
                at("Signer.weight"),
            ],
        ),
        (
            "op.set_options.thresholds_and_flags",
            vec![
                at("SetOptionsOp.clearFlags"),
                at("SetOptionsOp.setFlags"),
                at("SetOptionsOp.masterWeight"),
                at("SetOptionsOp.lowThreshold"),
                at("SetOptionsOp.medThreshold"),
                at("SetOptionsOp.highThreshold"),
            ],
        ),
        ("op.change_trust.line", vec![at("ChangeTrustOp.line")]),
        (
            "op.change_trust.line.liquidity_pool",
            vec![
                at("LiquidityPoolConstantProductParameters.assetA"),
                at("LiquidityPoolConstantProductParameters.assetB"),
                at("LiquidityPoolConstantProductParameters.fee"),
            ],
        ),
        ("op.change_trust.limit", vec![at("ChangeTrustOp.limit")]),
        (
            "op.allow_trust",
            vec![
                at("AllowTrustOp.trustor"),
                at("AllowTrustOp.asset"),
                at("AllowTrustOp.authorize"),
            ],
        ),
        (
            "op.manage_data.data_name",
            vec![
                except("ManageDataOp.dataName", "", "printable ascii"),
                only("ManageDataOp.dataName", "", "printable ascii"),
            ],
        ),
        // the row is about the bytes of a value; an absent value is the delete arm, which
        // the row describes on its own (null deletes the entry)
        (
            "op.manage_data.data_value",
            vec![except("ManageDataOp.dataValue", "", "absent")],
        ),
        (
            "op.bump_sequence.bump_to",
            vec![at("BumpSequenceOp.bumpTo")],
        ),
        (
            "op.create_claimable_balance.claimants.destination",
            vec![at("ClaimantV0.destination")],
        ),
        (
            "op.create_claimable_balance.claimants.predicate",
            vec![at("ClaimantV0.predicate")],
        ),
        (
            "op.create_claimable_balance.asset_amount",
            vec![
                at("CreateClaimableBalanceOp.asset"),
                at("CreateClaimableBalanceOp.amount"),
            ],
        ),
        (
            "op.claim_claimable_balance.balance_id",
            vec![at("ClaimClaimableBalanceOp.balanceID")],
        ),
        ("op.clawback.from", vec![at("ClawbackOp.from")]),
        (
            "op.clawback.from.mux_id",
            vec![via("MuxedAccountMed25519.id", "clawback")],
        ),
        (
            "op.clawback.asset_amount",
            vec![at("ClawbackOp.asset"), at("ClawbackOp.amount")],
        ),
        (
            "op.clawback_claimable_balance.balance_id",
            vec![at("ClawbackClaimableBalanceOp.balanceID")],
        ),
        (
            "op.set_trust_line_flags",
            vec![
                at("SetTrustLineFlagsOp.trustor"),
                at("SetTrustLineFlagsOp.asset"),
                at("SetTrustLineFlagsOp.clearFlags"),
                at("SetTrustLineFlagsOp.setFlags"),
            ],
        ),
        (
            "op.begin_sponsoring.sponsored_id",
            vec![
                via("BeginSponsoringFutureReservesOp.sponsoredID", "peer"),
                bases("BeginSponsoringFutureReservesOp.sponsoredID", "peer"),
                bases("BeginSponsoringFutureReservesOp.sponsoredID", "create"),
            ],
        ),
        (
            "op.revoke_sponsorship",
            vec![
                at("RevokeSponsorshipOpSigner.accountID"),
                at("RevokeSponsorshipOpSigner.signerKey"),
                at("LedgerKeyAccount.accountID"),
                at("LedgerKeyTrustLine.accountID"),
                at("LedgerKeyTrustLine.asset"),
                at("LedgerKeyOffer.sellerID"),
                at("LedgerKeyOffer.offerID"),
                at("LedgerKeyData.accountID"),
                at("LedgerKeyData.dataName"),
                at("LedgerKeyClaimableBalance.balanceID"),
            ],
        ),
        (
            "op.account_merge.destination",
            vec![only("Operation.body", "merge", "same arm")],
        ),
        (
            "op.liquidity_pool_deposit",
            vec![
                at("LiquidityPoolDepositOp.liquidityPoolID"),
                at("LiquidityPoolDepositOp.maxAmountA"),
                at("LiquidityPoolDepositOp.maxAmountB"),
                at("LiquidityPoolDepositOp.minPrice"),
                at("LiquidityPoolDepositOp.maxPrice"),
            ],
        ),
        (
            "op.liquidity_pool_withdraw",
            vec![
                at("LiquidityPoolWithdrawOp.liquidityPoolID"),
                at("LiquidityPoolWithdrawOp.amount"),
                at("LiquidityPoolWithdrawOp.minAmountA"),
                at("LiquidityPoolWithdrawOp.minAmountB"),
            ],
        ),
        (
            "op.invoke_host_function.invoke_contract.contract_address",
            vec![at("InvokeContractArgs.contractAddress")],
        ),
        (
            "op.invoke_host_function.invoke_contract.function_name",
            vec![at("InvokeContractArgs.functionName")],
        ),
        (
            "op.invoke_host_function.invoke_contract.args",
            vec![
                except("InvokeContractArgs.args", "", "which echo takes"),
                only("InvokeContractArgs.args", "", "which echo takes"),
            ],
        ),
        (
            "op.invoke_host_function.upload_wasm",
            vec![only(
                "InvokeHostFunctionOp.hostFunction",
                "upload",
                "same arm",
            )],
        ),
        (
            "op.invoke_host_function.create_contract.preimage",
            vec![
                at("CreateContractArgs.contractIDPreimage"),
                at("CreateContractArgsV2.contractIDPreimage"),
                at("ContractIDPreimageFromAddress.address"),
                at("ContractIDPreimageFromAddress.salt"),
            ],
        ),
        (
            "op.invoke_host_function.create_contract_v2.constructor_args",
            vec![at("CreateContractArgsV2.constructorArgs")],
        ),
        (
            "op.invoke_host_function.auth",
            vec![
                except(
                    "InvokeHostFunctionOp.auth",
                    "auth",
                    "first element repeated",
                ),
                only(
                    "InvokeHostFunctionOp.auth",
                    "auth",
                    "first element repeated",
                ),
            ],
        ),
        (
            "auth.credentials.source_account",
            vec![via("SorobanAuthorizationEntry.credentials", "source")],
        ),
        (
            "auth.credentials.address.address",
            vec![
                via("SorobanAddressCredentials.address", "matched"),
                only("SorobanAddressCredentials.address", "unmatched", "same arm"),
            ],
        ),
        (
            "auth.credentials.address.nonce",
            vec![
                via("SorobanAddressCredentials.nonce", "matched"),
                via("SorobanAddressCredentials.nonce", "unmatched"),
            ],
        ),
        (
            "auth.credentials.address.signature_expiration_ledger",
            vec![
                via(
                    "SorobanAddressCredentials.signatureExpirationLedger",
                    "matched",
                ),
                via(
                    "SorobanAddressCredentials.signatureExpirationLedger",
                    "unmatched",
                ),
            ],
        ),
        (
            "auth.credentials.address.signature",
            vec![
                via("SorobanAddressCredentials.signature", "matched"),
                only(
                    "SorobanAddressCredentials.signature",
                    "unmatched",
                    "same arm, present",
                ),
            ],
        ),
        (
            "auth.credentials.address_with_delegates.delegates",
            vec![
                via(
                    "SorobanAddressCredentialsWithDelegates.delegates",
                    "matched",
                ),
                except(
                    "SorobanAddressCredentialsWithDelegates.delegates",
                    "unmatched",
                    "first element repeated",
                ),
            ],
        ),
        (
            "auth.root_invocation",
            vec![
                via("SorobanAuthorizationEntry.rootInvocation", "address"),
                via("SorobanAuthorizationEntry.rootInvocation", "unmatched"),
                via("SorobanAuthorizedInvocation.function", "matched"),
                via("SorobanAuthorizedInvocation.function", "unmatched"),
                via("SorobanAuthorizedInvocation.subInvocations", "matched"),
                via("SorobanAuthorizedInvocation.subInvocations", "unmatched"),
            ],
        ),
        (
            "op.extend_footprint_ttl.extend_to",
            vec![at("ExtendFootprintTTLOp.extendTo")],
        ),
        (
            "op.restore_footprint",
            vec![
                via("LedgerFootprint.readOnly", "restore"),
                via("LedgerFootprint.readWrite", "restore"),
            ],
        ),
    ]
}

/// A disagreement settled by reading core's source.
#[derive(Debug, Clone)]
pub struct Resolution {
    pub row: &'static str,
    /// What the source establishes about the field, whatever the probe could measure.
    pub verdict: Measured,
    /// The places in core's source the reading rests on, `file:lines`.
    pub citation: &'static str,
    /// What the source says and why the measured verdict differs, with file and lines.
    pub reading: &'static str,
}

/// Every disagreement the source explains. A `partial` row is settled as mixed only when the
/// unchecked part needs state or a context the probe cannot build; the reading says which
/// core lines show the missing check and why the probe cannot reach it. A resolution for a row that agrees is stale and
/// fails the comparison.
pub fn resolutions() -> Vec<Resolution> {
    vec![
        Resolution {
            row: "tx.envelope.type",
            verdict: Measured::Validated,
            citation: "stellar-core v30.0.0 src/transactions/TransactionFrame.cpp:1257-1265; src/overlay/Peer.cpp:183-185; src/main/CommandHandler.cpp:1074-1084; src/transactions/TransactionBridge.cpp:18-47",
            reading: "core refuses a v0 envelope from protocol 13 in TransactionFrame::commonValidPreSeqNum (stellar-core v30.0.0 src/transactions/TransactionFrame.cpp 1257-1265), which envelopes from peers reach unchanged (src/overlay/Peer.cpp 183-185). the probe submits through core's tx endpoint, which first turns a v0 envelope into the equivalent v1 envelope (src/main/CommandHandler.cpp 1074-1084, src/transactions/TransactionBridge.cpp 18-47), so the v0 base applies at protocol 30 too and the ledger holds its v1 form. the refusal is out of the endpoint's reach, not absent; the row now says so",
        },
        Resolution {
            row: "tx.cond.type",
            verdict: Measured::Validated,
            citation: "stellar-core v30.0.0 src/transactions/TransactionFrame.cpp:1266-1272",
            reading: "the only check of the precondition type is the protocol 19 gate: PRECOND_V2 before protocol 19 is txNOT_SUPPORTED (stellar-core v30.0.0 src/transactions/TransactionFrame.cpp 1266-1272). at protocol 30, where the probe runs, every arm is accepted; the values inside an arm are checked under their own rows (time bounds, ledger bounds, minimum sequence fields, extra signers)",
        },
        Resolution {
            row: "tx.signatures.hint",
            verdict: Measured::Mixed,
            citation: "stellar-core v30.0.0 src/transactions/SignatureUtils.cpp:38-45; src/transactions/SignatureUtils.cpp:127-136",
            reading: "SignatureUtils::verify compares only the last four bytes of the signer's key with the hint (doesHintMatch, a memcmp of 4 bytes) before it verifies the signature, so a hint that equals another signer's last four bytes passes the filter and the signature counts for whichever signer it verifies against: the hint does not identify the signer. the probe cannot reach it, because it needs two signer keys sharing their last four bytes, a search of about 2^32 key generations",
        },
    ]
}

/// The measured verdict of one site: whether its attempts end in more than one way.
/// The attempts of `v` in `carrier` (the empty id for a struct's only carrier) that put the
/// site's question to core.
fn picked_in<'a>(v: &'a FieldVerdict, s: &Site, carrier: &str) -> Vec<&'a ProbeAttempt> {
    v.attempts
        .iter()
        .filter(|a| a.carrier.as_deref().unwrap_or_default() == carrier)
        .filter(|a| {
            let name = a.variant.as_str();
            if name.starts_with("base, ") {
                return true;
            }
            if s.bases {
                return false;
            }
            s.only.is_none_or(|o| name.contains(o)) && s.except.is_none_or(|x| !name.contains(x))
        })
        .collect()
}

/// Whether the attempts end in more than one way.
fn differ(attempts: &[&ProbeAttempt]) -> bool {
    let mut outcomes: Vec<_> = attempts.iter().map(|a| a.outcome()).collect();
    outcomes.sort();
    outcomes.dedup();
    outcomes.len() > 1
}

/// One row of the comparison.
/// A verdict in the words of `field-verdicts.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Measured {
    Validated,
    NotValidated,
    /// The sites of a row disagree.
    Mixed,
}

/// One row of the comparison.
#[derive(Debug, Clone)]
pub struct Compared {
    /// The `registry/fields.toml` row id.
    pub row: String,
    /// What the row says: `yes`, `partial` or `no`.
    pub registry: String,
    pub measured: Measured,
    /// Each site and what core answered there, in the row's order.
    pub sites: Vec<SiteVerdict>,
    pub agrees: bool,
    pub settled_by: Option<Resolution>,
}

/// One struct field in one carrier, and whether the attempts that answer the row there end
/// in more than one way.
#[derive(Debug, Clone)]
pub struct SiteVerdict {
    pub field: String,
    /// The carrier id; empty for a struct's only carrier, which has none.
    pub carrier: String,
    /// Which attempts count: `every value`, `values named "<text>"`, `values not named
    /// "<text>"` or `the base transactions` (then the verdict compares the bases of all the
    /// row's sites).
    pub values: String,
    pub validated: bool,
    pub attempts: usize,
}

/// The carrier name of a struct probed in only one transaction, whose attempts carry no
/// carrier id in validation.json.
pub const SINGLE: &str = "single";

/// `registry/generated/field-verdicts.json`, in the format the consumers agreed on:
///
/// - `rows` sorted by `id`, one per `registry/fields.toml` row;
/// - `measured` is `validated` when every site validates, `not-validated` when none does and
///   `mixed` when the sites disagree;
/// - a site is one struct field in one carrier transaction. `carrier` is the carrier id the
///   probe gave that transaction, or `single` for a struct it probed in only one; it is never
///   empty. A row on every carrier of a field lists one site per carrier;
/// - `settled_by_source` is set when core's source settles a row the measurement disagrees
///   with: its `verdict` (`validated`, `not-validated` or `mixed`) and the `file:lines` the
///   reading rests on.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldVerdicts {
    pub schema: u32,
    /// The table the verdicts were measured in.
    pub source: String,
    /// Sorted by id, one per `registry/fields.toml` row.
    pub rows: Vec<RowVerdict>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowVerdict {
    pub id: String,
    pub measured: Measured,
    pub sites: Vec<SiteRecord>,
    pub settled_by_source: Option<SourceVerdict>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteRecord {
    pub field: String,
    pub carrier: String,
    pub verdict: Measured,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceVerdict {
    pub verdict: Measured,
    pub reading: String,
}

fn verdict_of(validated: bool) -> Measured {
    if validated {
        Measured::Validated
    } else {
        Measured::NotValidated
    }
}

/// The verdict file of a comparison, measured in the table at `source`.
pub fn field_verdicts(rows: &[Compared], source: &str) -> FieldVerdicts {
    let mut rows: Vec<RowVerdict> = rows
        .iter()
        .map(|c| RowVerdict {
            id: c.row.clone(),
            measured: c.measured,
            sites: c
                .sites
                .iter()
                .map(|s| SiteRecord {
                    field: s.field.clone(),
                    carrier: if s.carrier.is_empty() {
                        SINGLE.to_owned()
                    } else {
                        s.carrier.clone()
                    },
                    verdict: verdict_of(s.validated),
                })
                .collect(),
            settled_by_source: c.settled_by.as_ref().map(|r| SourceVerdict {
                verdict: r.verdict,
                reading: r.citation.to_owned(),
            }),
        })
        .collect();
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    FieldVerdicts {
        schema: 1,
        source: source.to_owned(),
        rows,
    }
}

fn values(s: &Site) -> String {
    match (s.bases, s.only, s.except) {
        (true, _, _) => "the base transactions".into(),
        (false, Some(o), _) => format!("values named \"{o}\""),
        (false, None, Some(x)) => format!("values not named \"{x}\""),
        (false, None, None) => "every value".into(),
    }
}

/// The carriers a field's attempts name, the empty id for a struct's only carrier.
fn carriers_of(v: &FieldVerdict) -> Vec<String> {
    let mut c: Vec<String> = v
        .attempts
        .iter()
        .map(|a| a.carrier.clone().unwrap_or_default())
        .collect();
    c.sort();
    c.dedup();
    c
}

/// Compares every row of `fields` with the table.
pub fn compare(table: &ValidationTable, fields: &toml::Table) -> Result<Vec<Compared>> {
    let by_path: BTreeMap<&str, &FieldVerdict> =
        table.fields.iter().map(|f| (f.path.as_str(), f)).collect();
    let sites: BTreeMap<&str, Vec<Site>> = sites().into_iter().collect();
    let resolutions = resolutions();
    let rows = fields
        .get("field")
        .and_then(|f| f.as_array())
        .context("fields.toml has no [[field]] rows")?;
    let mut out = Vec::new();
    for r in rows {
        let id = r
            .get("id")
            .and_then(|v| v.as_str())
            .context("a row without id")?;
        let said = r
            .get("validated_by_core")
            .and_then(|v| v.as_str())
            .with_context(|| format!("{id} has no validated_by_core"))?;
        let ss = sites
            .get(id)
            .with_context(|| format!("no sites for fields.toml row {id}"))?;
        let mut measured: Vec<SiteVerdict> = Vec::new();
        let mut pooled = Vec::new();
        for s in ss {
            let v = by_path
                .get(s.path)
                .with_context(|| format!("{id}: validation.json has no {}", s.path))?;
            // a site on every carrier of a field is answered carrier by carrier, each with its
            // own base transaction
            let carriers = if s.carrier.is_empty() {
                carriers_of(v)
            } else {
                vec![s.carrier.to_owned()]
            };
            for c in carriers {
                let p = picked_in(v, s, &c);
                if s.bases {
                    // a row about the transaction itself compares the bases of its sites
                    ensure!(p.len() == 1, "{id}: {} [{c}] has {} bases", s.path, p.len());
                    pooled.extend(p);
                    measured.push(SiteVerdict {
                        field: s.path.to_owned(),
                        carrier: c,
                        values: values(s),
                        validated: false,
                        attempts: 1,
                    });
                    continue;
                }
                ensure!(
                    p.len() >= 2,
                    "{id}: {} attempts match {} [{c}]",
                    p.len(),
                    s.path
                );
                measured.push(SiteVerdict {
                    field: s.path.to_owned(),
                    carrier: c,
                    values: values(s),
                    validated: differ(&p),
                    attempts: p.len(),
                });
            }
        }
        if !pooled.is_empty() {
            ensure!(pooled.len() >= 2, "{id}: a row compares two bases or more");
            // bases of different envelope types succeed under different codes (a fee bump's
            // is tx_fee_bump_inner_success), so they compare by whether core applied them
            let mut ends: Vec<_> = pooled.iter().map(|a| (a.submission, a.applied)).collect();
            ends.sort();
            ends.dedup();
            for m in measured
                .iter_mut()
                .filter(|m| m.values == "the base transactions")
            {
                m.validated = ends.len() > 1;
                m.attempts = pooled.len();
            }
        }
        let all = measured.iter().all(|m| m.validated);
        let none = !measured.iter().any(|m| m.validated);
        let agrees = match said {
            "yes" => all,
            "no" => none,
            // partial means the sites disagree: core checks the field at some sites and not
            // at others. a row whose sites all validate agrees only through a settlement
            "partial" => !all && !none,
            other => bail!("{id}: validated_by_core {other:?}"),
        };
        let settled_by = resolutions.iter().find(|x| x.row == id).cloned();
        out.push(Compared {
            row: id.to_owned(),
            registry: said.to_owned(),
            measured: match (all, none) {
                (true, _) => Measured::Validated,
                (_, true) => Measured::NotValidated,
                _ => Measured::Mixed,
            },
            sites: measured,
            agrees,
            settled_by,
        });
    }
    for id in sites.keys() {
        if !out.iter().any(|c| c.row == *id) {
            bail!("sites name {id}, which fields.toml does not have");
        }
    }
    for x in &resolutions {
        if !out.iter().any(|c| c.row == x.row) {
            bail!(
                "a resolution names {}, which fields.toml does not have",
                x.row
            );
        }
    }
    Ok(out)
}

/// One `field` obligation per envelope field, met by the attempts' witness entries: the
/// fragment `registry/generated/validation.obligations.json` the obligation ledger reads.
pub fn obligations(table: &ValidationTable) -> shapekit_contracts::ObligationLedger {
    use shapekit_contracts::obligations::Obligation;
    use shapekit_contracts::{ObligationKind, ObligationStatus};
    let obligations = table
        .fields
        .iter()
        .map(|f| {
            let mut witnesses: Vec<String> = f
                .attempts
                .iter()
                .flat_map(|a| a.witnesses.iter().cloned())
                .collect();
            witnesses.sort();
            witnesses.dedup();
            let mut outcomes: Vec<_> = f.attempts.iter().map(ProbeAttempt::outcome).collect();
            outcomes.sort();
            outcomes.dedup();
            Obligation {
                id: format!("field:{}", f.path),
                kind: ObligationKind::Field,
                status: ObligationStatus::Met,
                witnesses,
                evidence: Some(format!(
                    "{}: {} values tried, {} different answers from core",
                    match f.verdict {
                        shapekit_contracts::validation::Verdict::Validated => "validated",
                        shapekit_contracts::validation::Verdict::Partial => "partial",
                        shapekit_contracts::validation::Verdict::NotValidated => "not validated",
                    },
                    f.attempts.len(),
                    outcomes.len()
                )),
            }
        })
        .collect();
    shapekit_contracts::ObligationLedger {
        schema_version: 1,
        obligations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_site_names_an_envelope_field() {
        let schema = shapekit_schema::Schema::pinned();
        let fields = crate::fields::envelope_fields(&schema).unwrap();
        for (row, ss) in sites() {
            assert!(!ss.is_empty(), "{row}");
            for s in ss {
                assert!(
                    fields.iter().any(|f| f.path == s.path),
                    "{row}: {} is no envelope field",
                    s.path
                );
            }
        }
    }

    #[test]
    fn every_registry_row_has_sites() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../registry/fields.toml"
        ))
        .unwrap();
        let t: toml::Table = text.parse().unwrap();
        let ids: Vec<String> = t["field"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(ids.len(), 88);
        let s: Vec<&str> = sites().into_iter().map(|(r, _)| r).collect();
        for id in &ids {
            assert!(s.contains(&id.as_str()), "{id} has no sites");
        }
        assert_eq!(s.len(), ids.len());
    }
}
