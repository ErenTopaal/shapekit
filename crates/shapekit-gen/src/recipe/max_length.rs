//! `classic.max-length`: every bounded vector an indexer reads on the
//! classic path, filled to the bound the xdr declares, each accepted by
//! core.
//!
//! | ledger   | content                                                           |
//! |----------|-------------------------------------------------------------------|
//! | 2        | protocol upgrade                                                  |
//! | 3        | root funds multisig, bulk, sink, issuer, maker, receiver and three hop makers |
//! | 4        | multisig adds 20 ed25519 signers and a medium threshold of 20 (`signers<20>`) |
//! | 5        | a payment from multisig carrying 20 signatures (`signatures<20>`) |
//! | 6        | 100 payments in one transaction (`operations<100>`)               |
//! | 7        | a payment with a 28 byte text memo (`string text<28>`)            |
//! | 8        | manage data with a 64 byte name and value (`string64`, `DataValue`) |
//! | 9        | maker and receiver trust six path assets                          |
//! | 10, 11   | issuer pays maker; maker posts one offer per hop                  |
//! | 12 to 20 | a credit-only route Q1 to Q4: trustlines, payments, one offer per maker |
//! | 21       | strict-receive path payment over a 5 asset path (`path<5>`), then |
//! |          | one over the credit-only route                                    |
//! | 22       | strict-send path payment over the 5 asset path                    |
//! | 23       | multisig sets a 32 byte home domain (`string32`), 20 signatures   |
//!
//! Core requires every home domain byte to be printable ascii, 0x20 to
//! 0x7e (`isStringValid` in src/util/types.cpp, checked by
//! `SetOptionsOpFrame::doCheckValid`, stellar-core v30.0.0), and nothing
//! more: the accepted domain holds a space, a slash and a tilde, so it is
//! no hostname. Two twins refused at submission with
//! `SET_OPTIONS_INVALID_HOME_DOMAIN` hold 0x7f and the valid UTF-8 `c3a9`.
//!
//! Ten claimants (`claimants<10>`) belong to `cap23.claimable-balances`,
//! which records them.

use anyhow::{Result, bail, ensure};
use ed25519_dalek::SigningKey;
use shapekit_core_driver::keys::{account_id, ed25519_signer, muxed_account};
use stellar_xdr::{
    AlphaNum4, Asset, AssetCode4, ChangeTrustAsset, ChangeTrustOp, LedgerEntryData, LedgerKey,
    LedgerKeyAccount, ManageDataOp, ManageSellOfferOp, Memo, OperationBody, OperationResult,
    OperationResultTr, PathPaymentStrictReceiveOp, PathPaymentStrictSendOp, PaymentOp, Price,
    SetOptionsOp, SetOptionsResult, Signer, StringM, TransactionEnvelope, TransactionResultResult,
};

use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "classic.max-length";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

/// One unit in stroops.
const UNIT: i64 = 10_000_000;

/// `MAX_SIGNERS`, `signatures<20>`, `MAX_OPS_PER_TX`, `string text<28>`,
/// `string64`, `DataValue` (`opaque<64>`) and `path<5>` in Stellar-*.x.
const MAX_SIGNERS: usize = 20;
const MAX_SIGNATURES: usize = 20;
const MAX_OPS: usize = 100;
const MAX_MEMO_TEXT: usize = 28;
const MAX_DATA: usize = 64;
const MAX_PATH: usize = 5;
const MAX_HOME_DOMAIN: usize = 32;

/// Hops of the credit-only route, as many as core's path payment golden.
const CREDIT_HOPS: usize = 3;

pub struct MaxLength {
    pub protocol: u32,
}

fn signer_role(i: usize) -> String {
    format!("signer-{i:02}")
}

/// Asset `<letter><i>` (i from 1 to 9) from one issuer: `P1` to `P6` on
/// the 5 asset path, `Q1` to `Q4` on the credit-only route.
fn credit(issuer: &SigningKey, letter: u8, i: usize) -> Asset {
    let mut code = [0u8; 4];
    code[0] = letter;
    code[1] = b'0' + i as u8;
    Asset::CreditAlphanum4(AlphaNum4 {
        asset_code: AssetCode4(code),
        issuer: account_id(&issuer.verifying_key()),
    })
}

fn trust(asset: &Asset) -> Result<OperationBody> {
    let Asset::CreditAlphanum4(a) = asset else {
        bail!("trust line to {asset:?} is not used here");
    };
    Ok(OperationBody::ChangeTrust(ChangeTrustOp {
        line: ChangeTrustAsset::CreditAlphanum4(a.clone()),
        limit: i64::MAX,
    }))
}

fn pay(to: &SigningKey, asset: &Asset, amount: i64) -> OperationBody {
    OperationBody::Payment(PaymentOp {
        destination: muxed_account(&to.verifying_key()),
        asset: asset.clone(),
        amount,
    })
}

/// A 32 byte home domain of printable ascii that is not a hostname, with
/// both ends of core's accepted range, 0x20 and 0x7e.
fn home_domain() -> Vec<u8> {
    b"~shapekit home/domain at bound ~".to_vec()
}

/// The accepted home domain with `bytes` written over its middle, keeping
/// the length at the bound.
fn home_domain_with(bytes: &[u8]) -> Vec<u8> {
    let mut d = home_domain();
    let at = MAX_HOME_DOMAIN / 2;
    d[at..at + bytes.len()].copy_from_slice(bytes);
    d
}

/// `n` printable ascii bytes, the same for every run.
fn text(n: usize, seed: &str) -> Vec<u8> {
    seed.bytes().cycle().take(n).collect()
}

impl Recipe for MaxLength {
    fn id(&self) -> &str {
        FAMILY
    }

    fn family(&self) -> &str {
        FAMILY
    }

    fn protocol(&self) -> u32 {
        self.protocol
    }

    fn run(&self, ctx: &mut ChainCtx) -> Result<()> {
        ctx.fund(
            &[
                "multisig",
                "bulk",
                "sink",
                "issuer",
                "maker",
                "receiver",
                "hop-maker-1",
                "hop-maker-2",
                "hop-maker-3",
            ],
            BALANCE,
        )?;
        let multisig = ctx.account("multisig");
        let bulk = ctx.account("bulk");
        let sink = ctx.account("sink");
        let issuer = ctx.account("issuer");
        let maker = ctx.account("maker");
        let receiver = ctx.account("receiver");
        let signers: Vec<SigningKey> = (0..MAX_SIGNERS)
            .map(|i| ctx.account(&signer_role(i)))
            .collect();

        let ops = signers
            .iter()
            .enumerate()
            .map(|(i, k)| {
                OperationBody::SetOptions(SetOptionsOp {
                    inflation_dest: None,
                    clear_flags: None,
                    set_flags: None,
                    master_weight: None,
                    low_threshold: None,
                    // core stops matching signatures once the threshold is
                    // met and refuses the unused ones, so a payment must need
                    // all 20 to carry 20
                    med_threshold: (i == 0).then_some(MAX_SIGNATURES as u32),
                    high_threshold: None,
                    home_domain: None,
                    signer: Some(Signer {
                        key: ed25519_signer(&k.verifying_key()),
                        weight: 1,
                    }),
                })
            })
            .collect();
        let env = ctx.classic(&multisig, ops)?;
        let applied = ctx.submit_and_close(&env)?;
        let multisig_key = LedgerKey::Account(LedgerKeyAccount {
            account_id: account_id(&multisig.verifying_key()),
        });
        ctx.capture(
            "account-20-signers",
            Selector::LedgerEntry {
                ledger: applied.ledger,
                key: multisig_key.clone(),
            },
            Spec::new("an account entry holding 20 ed25519 signers, the most core allows")
                .fields(&["op.set_options.signer"])
                .flags(&["max-length-vec"]),
        )?;
        let got = ctx
            .driver()
            .node()
            .ledger_entries(std::slice::from_ref(&multisig_key))?;
        match got.entries[0].entry.as_ref().map(|e| &e.data) {
            Some(LedgerEntryData::Account(a)) => ensure!(
                a.signers.len() == MAX_SIGNERS,
                "multisig holds {} signers, expected {MAX_SIGNERS}",
                a.signers.len()
            ),
            other => bail!("multisig's account is missing: {other:?}"),
        }

        // the master key and 19 of the signers reach the medium threshold
        // of 20 exactly
        let tx = ctx
            .tx(&multisig)?
            .operation(None, pay(&sink, &Asset::Native, UNIT))
            .build()?;
        let mut keys: Vec<&SigningKey> = vec![&multisig];
        keys.extend(signers.iter().take(MAX_SIGNATURES - 1));
        let env = ctx.sign(tx, &keys)?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "signatures-20",
            Selector::ledger_of(&applied),
            Spec::new(
                "a payment signed by the source's master key and 19 of its signers, 20 signatures",
            )
            .fields(&["tx.signatures", "tx.signatures.hint"])
            .flags(&["max-length-vec"]),
        )?;

        let ops = (0..MAX_OPS)
            .map(|i| pay(&sink, &Asset::Native, UNIT + i as i64))
            .collect();
        let env = ctx.classic(&bulk, ops)?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "operations-100",
            Selector::ledger_of(&applied),
            Spec::new(
                "one transaction with 100 payments, each a different amount, so its meta and result hold 100 operations",
            )
            .fields(&["tx.operations"])
            .flags(&["max-length-vec"]),
        )?;

        let memo = Memo::Text(StringM::try_from(text(
            MAX_MEMO_TEXT,
            "shapekit memo at its bound ",
        ))?);
        let tx = ctx
            .tx(&bulk)?
            .memo(memo)
            .operation(None, pay(&sink, &Asset::Native, UNIT))
            .build()?;
        let env = ctx.sign(tx, &[&bulk])?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "memo-text-28",
            Selector::ledger_of(&applied),
            Spec::new("a payment with a 28 byte ascii text memo")
                .fields(&["tx.memo.type", "tx.memo.text"])
                .flags(&["max-length-vec"]),
        )?;

        let env = ctx.classic(
            &bulk,
            vec![OperationBody::ManageData(ManageDataOp {
                data_name: StringM::try_from(text(MAX_DATA, "shapekit-data-name-"))?.into(),
                data_value: Some(text(MAX_DATA, "shapekit data value ").try_into()?),
            })],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "data-name-value-64",
            Selector::transaction(&applied),
            Spec::new("manage data with a 64 byte name and a 64 byte value")
                .fields(&["op.manage_data.data_name", "op.manage_data.data_value"])
                .flags(&["max-length-vec"]),
        )?;

        // six assets give a send asset, five intermediate assets and a
        // destination asset: native, P1 to P5, P6
        let assets: Vec<Asset> = (1..=MAX_PATH + 1)
            .map(|i| credit(&issuer, b'P', i))
            .collect();
        for who in [&maker, &receiver] {
            let ops = assets.iter().map(trust).collect::<Result<Vec<_>>>()?;
            let env = ctx.classic(who, ops)?;
            ctx.submit_and_close(&env)?;
        }
        let env = ctx.classic(
            &issuer,
            assets.iter().map(|a| pay(&maker, a, 1000 * UNIT)).collect(),
        )?;
        ctx.submit_and_close(&env)?;
        // each offer sells the next asset for the previous one at 1, so the
        // book has exactly one route from native to P6
        let mut previous = Asset::Native;
        let mut offers = Vec::new();
        for a in &assets {
            offers.push(OperationBody::ManageSellOffer(ManageSellOfferOp {
                selling: a.clone(),
                buying: previous.clone(),
                amount: 100 * UNIT,
                price: Price { n: 1, d: 1 },
                offer_id: 0,
            }));
            previous = a.clone();
        }
        let env = ctx.classic(&maker, offers)?;
        ctx.submit_and_close(&env)?;

        // a second, credit-only route Q1 to Q4 with one maker per hop: a
        // payment along it changes only offers and trustlines, the shape of
        // core's path payment golden, and rides in the same transaction as
        // the 5 asset path
        let route: Vec<Asset> = (1..=CREDIT_HOPS + 1)
            .map(|i| credit(&issuer, b'Q', i))
            .collect();
        let hop_makers: Vec<SigningKey> = (1..=CREDIT_HOPS)
            .map(|i| ctx.account(&format!("hop-maker-{i}")))
            .collect();
        for (who, a) in [(&bulk, &route[0]), (&receiver, &route[CREDIT_HOPS])] {
            let env = ctx.classic(who, vec![trust(a)?])?;
            ctx.submit_and_close(&env)?;
        }
        for (i, m) in hop_makers.iter().enumerate() {
            let env = ctx.classic(m, vec![trust(&route[i])?, trust(&route[i + 1])?])?;
            ctx.submit_and_close(&env)?;
        }
        let mut pays = vec![pay(&bulk, &route[0], 1000 * UNIT)];
        for (i, m) in hop_makers.iter().enumerate() {
            pays.push(pay(m, &route[i + 1], 1000 * UNIT));
        }
        let env = ctx.classic(&issuer, pays)?;
        ctx.submit_and_close(&env)?;
        for (i, m) in hop_makers.iter().enumerate() {
            let env = ctx.classic(
                m,
                vec![OperationBody::ManageSellOffer(ManageSellOfferOp {
                    selling: route[i + 1].clone(),
                    buying: route[i].clone(),
                    amount: 100 * UNIT,
                    price: Price { n: 1, d: 1 },
                    offer_id: 0,
                })],
            )?;
            ctx.submit_and_close(&env)?;
        }

        let path: Vec<Asset> = assets[..MAX_PATH].to_vec();
        let dest = assets[MAX_PATH].clone();
        let env = ctx.classic(
            &bulk,
            vec![
                OperationBody::PathPaymentStrictReceive(PathPaymentStrictReceiveOp {
                    send_asset: Asset::Native,
                    send_max: 20 * UNIT,
                    destination: muxed_account(&receiver.verifying_key()),
                    dest_asset: dest.clone(),
                    dest_amount: 10 * UNIT,
                    path: path.clone().try_into()?,
                }),
                OperationBody::PathPaymentStrictReceive(PathPaymentStrictReceiveOp {
                    send_asset: route[0].clone(),
                    send_max: 20 * UNIT,
                    destination: muxed_account(&receiver.verifying_key()),
                    dest_asset: route[CREDIT_HOPS].clone(),
                    dest_amount: 10 * UNIT,
                    path: route[1..CREDIT_HOPS].to_vec().try_into()?,
                }),
            ],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "path-strict-receive-5",
            Selector::ledger_of(&applied),
            Spec::new(
                "a strict-receive path payment from native to P6 through P1 to P5, crossing one offer per hop, followed in the same transaction by a credit-only one from Q1 to Q4 through three makers",
            )
            .fields(&["op.path_payment.path", "op.path_payment.amounts"])
            .flags(&["max-length-vec"]),
        )?;

        let env = ctx.classic(
            &bulk,
            vec![OperationBody::PathPaymentStrictSend(
                PathPaymentStrictSendOp {
                    send_asset: Asset::Native,
                    send_amount: 10 * UNIT,
                    destination: muxed_account(&receiver.verifying_key()),
                    dest_asset: dest,
                    dest_min: 1,
                    path: path.try_into()?,
                },
            )],
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "path-strict-send-5",
            Selector::ledger_of(&applied),
            Spec::new(
                "a strict-send path payment from native to P6 through P1 to P5, crossing one offer per hop",
            )
            .fields(&["op.path_payment.path", "op.path_payment.amounts"])
            .flags(&["max-length-vec"]),
        )?;

        // a refused envelope matches no home domain fragment on its own;
        // multisig's 20 signatures keep it in the family via `.signatures#max`
        for (slug, bytes, why) in [
            (
                "home-domain-del-rejected",
                vec![0x7f],
                "0x7f (DEL), one past core's printable range",
            ),
            (
                "home-domain-utf8-rejected",
                vec![0xc3, 0xa9],
                "c3a9, valid UTF-8 for U+00E9 but not ascii",
            ),
        ] {
            let domain = home_domain_with(&bytes);
            ensure!(domain.len() == MAX_HOME_DOMAIN, "{slug} is not 32 bytes");
            let env = set_home_domain(ctx, &multisig, &signers, domain.clone())?;
            let r = ctx.capture_rejected(
                slug,
                &env,
                Spec::new(format!(
                    "set options with a 32 byte home domain holding {why}, which core refuses as SET_OPTIONS_INVALID_HOME_DOMAIN; hex {}",
                    hex::encode(&domain)
                ))
                .fields(&["op.set_options.home_domain", "tx.signatures"])
                .flags(&["max-length-vec"]),
            )?;
            ensure_invalid_home_domain(&r.result)?;
        }

        let domain = home_domain();
        ensure!(
            domain.len() == MAX_HOME_DOMAIN && domain.iter().all(|b| (0x20..0x7f).contains(b)),
            "the accepted home domain is not 32 printable ascii bytes"
        );
        let env = set_home_domain(ctx, &multisig, &signers, domain.clone())?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "home-domain-32",
            Selector::ledger_of(&applied),
            Spec::new(format!(
                "set options with a 32 byte home domain {:?}, printable ascii with a space, a slash and a tilde, accepted by core and stored on an account with 20 signers",
                String::from_utf8(domain.clone())?
            ))
            .fields(&["op.set_options.home_domain", "tx.signatures"])
            .flags(&["max-length-vec"]),
        )?;
        let got = ctx
            .driver()
            .node()
            .ledger_entries(std::slice::from_ref(&multisig_key))?;
        match got.entries[0].entry.as_ref().map(|e| &e.data) {
            Some(LedgerEntryData::Account(a)) => ensure!(
                a.home_domain.as_slice() == domain.as_slice(),
                "multisig's home domain is {:?}, expected the 32 byte one",
                a.home_domain
            ),
            other => bail!("multisig's account is missing: {other:?}"),
        }
        Ok(())
    }
}

/// Set options from multisig changing only the home domain, which needs the
/// medium threshold of 20, so the master key and 19 signers sign.
fn set_home_domain(
    ctx: &ChainCtx,
    multisig: &SigningKey,
    signers: &[SigningKey],
    domain: Vec<u8>,
) -> Result<TransactionEnvelope> {
    let tx = ctx
        .tx(multisig)?
        .operation(
            None,
            OperationBody::SetOptions(SetOptionsOp {
                inflation_dest: None,
                clear_flags: None,
                set_flags: None,
                master_weight: None,
                low_threshold: None,
                med_threshold: None,
                high_threshold: None,
                home_domain: Some(StringM::try_from(domain)?.into()),
                signer: None,
            }),
        )
        .build()?;
    let mut keys: Vec<&SigningKey> = vec![multisig];
    keys.extend(signers.iter().take(MAX_SIGNATURES - 1));
    ctx.sign(tx, &keys)
}

fn ensure_invalid_home_domain(r: &TransactionResultResult) -> Result<()> {
    let TransactionResultResult::TxFailed(ops) = r else {
        bail!("core answered {r:?} for a non-printable home domain, expected tx_failed");
    };
    ensure!(
        matches!(
            ops.first(),
            Some(OperationResult::OpInner(OperationResultTr::SetOptions(
                SetOptionsResult::InvalidHomeDomain
            )))
        ),
        "core answered {ops:?} for a non-printable home domain, expected set_options invalid_home_domain"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_domains_are_at_the_bound() {
        assert_eq!(home_domain().len(), MAX_HOME_DOMAIN);
        assert_eq!(home_domain_with(&[0xc3, 0xa9]).len(), MAX_HOME_DOMAIN);
    }
}
