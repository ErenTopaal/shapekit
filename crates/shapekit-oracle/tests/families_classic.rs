//! Known answers for the classic families: the oracle's facts for the committed chains equal
//! what the recipe in `crates/shapekit-gen/src/recipe/` submitted. Every expectation is read
//! from the recipe (its constants, roles, operation order and the protocol rules it relies
//! on), never from an oracle file, and every test runs both network variants.

mod common;

use common::*;
use sha2::{Digest, Sha256};
use shapekit_oracle::*;
use stellar_xdr::{
    AccountId, AlphaNum4, AssetCode4, HashIdPreimage, HashIdPreimageOperationId, Limits,
    LiquidityPoolConstantProductParameters, LiquidityPoolParameters, PublicKey, SequenceNumber,
    Uint256, WriteXdr,
};

const BASE_FEE: i128 = shapekit_core_driver::tx::BASE_FEE as i128;

/// One unit of an issued asset or of XLM in stroops, as every recipe here defines it.
const UNIT: i128 = 10_000_000;

fn key_bytes(recipe: &str, role: &str) -> [u8; 32] {
    shapekit_core_driver::keys::derive_key(recipe, role)
        .verifying_key()
        .to_bytes()
}

fn muxed(recipe: &str, role: &str, id: u64) -> String {
    format!(
        "{}",
        stellar_strkey::ed25519::MuxedAccount {
            ed25519: key_bytes(recipe, role),
            id,
        }
    )
}

fn source(fam: &str, id: &str) -> (String, u32) {
    let e = entry(fam, id);
    (
        e.source.chain.expect("entry names a chain"),
        e.source.ledger_seq.expect("entry names a ledger"),
    )
}

/// The ledger whose operations created `account`; its sequence number starts there.
fn created_in(chain_id: &str, network: &str, account: &str) -> u32 {
    let c = chain(chain_id, network);
    c.ledgers
        .iter()
        .find(|(_, l)| {
            l.transactions.iter().any(|t| {
                op_changes(t)
                    .iter()
                    .any(|c| matches!(c, Change::AccountCreated { account: a, .. } if a == account))
            })
        })
        .map(|(seq, _)| *seq)
        .unwrap_or_else(|| panic!("{chain_id} {network}: {account} is never created"))
}

/// An account's sequence number at creation, `ledger << 32` (core's starting sequence).
fn start_seq(chain_id: &str, network: &str, account: &str) -> i128 {
    i128::from(created_in(chain_id, network, account)) << 32
}

fn ops(tx: &TxFacts) -> Vec<(u32, &Change)> {
    tx.changes
        .iter()
        .filter(|c| c.scope == ChangeScope::Operation)
        .map(|c| {
            (
                c.op_index.expect("operation changes carry an index"),
                &c.change,
            )
        })
        .collect()
}

/// Operation-scope change of what `holder` holds of `asset` in `held_in`, summed.
fn delta(tx: &TxFacts, holder: &str, asset: &str, held_in: &str) -> i128 {
    ops(tx)
        .into_iter()
        .filter_map(|(_, c)| match c {
            Change::BalanceChange {
                holder: h,
                asset: a,
                held_in: w,
                delta,
                ..
            } if h == holder && a == asset && w == held_in => Some(delta.0),
            _ => None,
        })
        .sum()
}

fn delta_at(tx: &TxFacts, op: u32, holder: &str, asset: &str, held_in: &str) -> i128 {
    ops(tx)
        .into_iter()
        .filter(|(i, _)| *i == op)
        .filter_map(|(_, c)| match c {
            Change::BalanceChange {
                holder: h,
                asset: a,
                held_in: w,
                delta,
                ..
            } if h == holder && a == asset && w == held_in => Some(delta.0),
            _ => None,
        })
        .sum()
}

fn seqs(tx: &TxFacts, scope: ChangeScope) -> Vec<(String, i128, i128)> {
    changes_in(tx, scope)
        .into_iter()
        .filter_map(|c| match c {
            Change::Sequence {
                account,
                before,
                after,
            } => Some((account.clone(), before.0, after.0)),
            _ => None,
        })
        .collect()
}

/// (action, id, seller, selling, buying, amount before, amount after, price n, price d)
type OfferRow = (
    Action,
    i128,
    String,
    String,
    String,
    Option<i128>,
    Option<i128>,
    i32,
    i32,
);

fn offers(tx: &TxFacts) -> Vec<OfferRow> {
    op_changes(tx)
        .into_iter()
        .filter_map(|c| match c {
            Change::Offer {
                action,
                offer_id,
                seller,
                selling,
                buying,
                amount_before,
                amount_after,
                price_n,
                price_d,
            } => Some((
                *action,
                offer_id.0,
                seller.clone(),
                selling.clone(),
                buying.clone(),
                amount_before.map(|a| a.0),
                amount_after.map(|a| a.0),
                *price_n,
                *price_d,
            )),
            _ => None,
        })
        .collect()
}

/// (op, counterparty, offer id, sold asset, sold amount, bought asset, bought amount)
type TradeRow = (u32, String, Option<i128>, String, i128, String, i128);

fn trades(tx: &TxFacts) -> Vec<TradeRow> {
    ops(tx)
        .into_iter()
        .filter_map(|(i, c)| match c {
            Change::Trade {
                counterparty,
                offer_id,
                sold_asset,
                sold_amount,
                bought_asset,
                bought_amount,
            } => Some((
                i,
                counterparty.clone(),
                offer_id.map(|o| o.0),
                sold_asset.clone(),
                sold_amount.0,
                bought_asset.clone(),
                bought_amount.0,
            )),
            _ => None,
        })
        .collect()
}

/// (op, account, asset, flags before, flags after)
fn trustline_flags(tx: &TxFacts) -> Vec<(u32, String, String, u32, u32)> {
    ops(tx)
        .into_iter()
        .filter_map(|(i, c)| match c {
            Change::TrustlineFlags {
                account,
                asset,
                before,
                after,
            } => Some((i, account.clone(), asset.clone(), *before, *after)),
            _ => None,
        })
        .collect()
}

/// (balance id, asset, amount, claimants)
fn balances_created(tx: &TxFacts) -> Vec<(String, String, i128, Vec<String>)> {
    op_changes(tx)
        .into_iter()
        .filter_map(|c| match c {
            Change::ClaimableBalanceCreated {
                balance_id,
                asset,
                amount,
                claimants,
            } => Some((
                balance_id.clone(),
                asset.clone(),
                amount.0,
                claimants.clone(),
            )),
            _ => None,
        })
        .collect()
}

/// (target, signer, sponsor before, sponsor after)
type SponsorRow = (String, Option<String>, Option<String>, Option<String>);

fn sponsorships(tx: &TxFacts) -> Vec<SponsorRow> {
    op_changes(tx)
        .into_iter()
        .filter_map(|c| match c {
            Change::Sponsorship {
                target,
                signer,
                sponsor_before,
                sponsor_after,
            } => Some((
                target.clone(),
                signer.clone(),
                sponsor_before.clone(),
                sponsor_after.clone(),
            )),
            _ => None,
        })
        .collect()
}

/// `to_muxed_id` of every transfer and mint event, in order.
fn credited_muxed_ids(tx: &TxFacts) -> Vec<(String, Option<ToMuxedId>)> {
    token_events(tx)
        .into_iter()
        .filter(|t| t.event == "transfer" || t.event == "mint")
        .map(|t| (t.event.clone(), t.to_muxed_id.clone()))
        .collect()
}

fn assert_no_muxed_participants(tx: &TxFacts, ctx: &str) {
    for p in &tx.participants {
        assert!(!p.address.starts_with('M'), "{ctx}: {p:?}");
        assert!(p.mux_ids.is_empty(), "{ctx}: {p:?}");
    }
}

fn account_participants(tx: &TxFacts) -> Vec<String> {
    tx.participants
        .iter()
        .filter(|p| p.kind == AddressKind::Account)
        .map(|p| p.address.clone())
        .collect()
}

/// The claimable balance id core derives for operation `op` of a transaction from `tx_source`
/// with sequence number `seq` (`ENVELOPE_TYPE_OP_ID` over the parent transaction's source).
fn balance_id(tx_source: &str, seq: i128, op: u32) -> String {
    let pk = stellar_strkey::ed25519::PublicKey::from_string(tx_source).unwrap();
    let pre = HashIdPreimage::OpId(HashIdPreimageOperationId {
        source_account: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(pk.0))),
        seq_num: SequenceNumber(i64::try_from(seq).unwrap()),
        op_num: op,
    });
    let h: [u8; 32] = Sha256::digest(pre.to_xdr(Limits::none()).unwrap()).into();
    format!("{}", stellar_strkey::ClaimableBalance::V0(h))
}

/// cap01.bump-sequence: alice bumps 2^20 ahead and then to 1, behind her number; maxed bumps
/// to i64::MAX; mover bumps past the starting sequence, so its merges fail.
#[test]
fn cap01_bump_sequence_moves_the_sequence_number() {
    let fam = "cap01.bump-sequence";
    let alice = account(fam, "alice");
    let maxed = account(fam, "maxed");
    let mover = account(fam, "mover");
    let bob = account(fam, "bob");
    for n in NETWORKS {
        let (chain_id, _) = source(fam, &format!("{fam}.p30-bump-forward"));
        let a0 = start_seq(&chain_id, n, &alice);
        let tx = only_tx(fam, &format!("{fam}.p30-bump-forward"), n);
        assert!(tx.successful, "{n}");
        assert_eq!(
            seqs(&tx, ChangeScope::TxBefore),
            vec![(alice.clone(), a0, a0 + 1)]
        );
        assert_eq!(
            seqs(&tx, ChangeScope::Operation),
            vec![(alice.clone(), a0 + 1, a0 + (1 << 20))],
            "{n}"
        );

        // behind the current number, so only the transaction's own sequence moves
        let at = a0 + (1 << 20);
        let tx = only_tx(fam, &format!("{fam}.p30-bump-behind"), n);
        assert!(tx.successful, "{n}");
        assert_eq!(
            seqs(&tx, ChangeScope::TxBefore),
            vec![(alice.clone(), at, at + 1)]
        );
        assert!(seqs(&tx, ChangeScope::Operation).is_empty(), "{n}");

        let m0 = start_seq(&chain_id, n, &maxed);
        let tx = only_tx(fam, &format!("{fam}.p30-bump-to-i64-max"), n);
        assert_eq!(
            seqs(&tx, ChangeScope::TxBefore),
            vec![(maxed.clone(), m0, m0 + 1)]
        );
        assert_eq!(
            seqs(&tx, ChangeScope::Operation),
            vec![(maxed.clone(), m0 + 1, i64::MAX.into())],
            "{n}"
        );

        // the merge fails and rolls the bump in the same transaction back
        let v0 = start_seq(&chain_id, n, &mover);
        let tx = entry_tx(
            fam,
            &format!("{fam}.p30-bump-then-merge-seqnum-too-far-meta"),
            n,
        );
        assert!(!tx.successful, "{n}");
        assert_eq!(tx.result, "tx_failed");
        assert_eq!(tx.fee.charged.0, 2 * BASE_FEE);
        assert_eq!(
            seqs(&tx, ChangeScope::TxBefore),
            vec![(mover.clone(), v0, v0 + 1)]
        );
        assert!(op_changes(&tx).is_empty(), "{n}: {:?}", op_changes(&tx));
        assert!(participant(&tx, &bob).is_none(), "{n}");

        // past_start: (last closed + 3) << 32, taken the ledger before this one closed
        let (_, ledger) = source(fam, &format!("{fam}.p30-bump-past-starting-sequence"));
        let far = (i128::from(ledger) + 2) << 32;
        let tx = only_tx(fam, &format!("{fam}.p30-bump-past-starting-sequence"), n);
        assert!(tx.successful, "{n}");
        assert_eq!(
            seqs(&tx, ChangeScope::TxBefore),
            vec![(mover.clone(), v0 + 1, v0 + 2)]
        );
        assert_eq!(
            seqs(&tx, ChangeScope::Operation),
            vec![(mover.clone(), v0 + 2, far)],
            "{n}"
        );

        let tx = only_tx(fam, &format!("{fam}.p30-merge-seqnum-too-far"), n);
        assert!(!tx.successful, "{n}");
        assert_eq!(
            seqs(&tx, ChangeScope::TxBefore),
            vec![(mover.clone(), far, far + 1)]
        );
        assert!(effect_kinds(&tx).is_empty(), "{n}: {:?}", effect_kinds(&tx));
        assert!(participant(&tx, &bob).is_none(), "{n}");
    }
}

/// cap03.offer-liabilities: maker rests an XLM and a USD offer, taker's manage-buy takes the
/// first and rests, the USD offer shrinks by id, the whale bids up to 0.1 XLM short of
/// INT64_MAX, a merge into it fails, then a second book of issuer, credit and two-maker crosses.
#[test]
fn cap03_offers_cross_rest_and_update() {
    let fam = "cap03.offer-liabilities";
    let issuer = account(fam, "issuer");
    let maker = account(fam, "maker");
    let taker = account(fam, "taker");
    let whale = account(fam, "whale");
    let merger = account(fam, "merger");
    let usd = format!("USD:{issuer}");
    let eur = format!("EUR:{issuer}");
    let gbp = format!("GBP:{issuer}");
    let native = "native".to_string();
    let id = |slug: &str| format!("{fam}.p30-{slug}");
    for n in NETWORKS {
        let tx = only_tx(fam, &id("sell-native-selling-liabilities"), n);
        let o = offers(&tx);
        assert_eq!(o.len(), 1, "{n}: {o:?}");
        let xlm_offer = o[0].1;
        assert_eq!(
            o[0],
            (
                Action::Created,
                xlm_offer,
                maker.clone(),
                native.clone(),
                usd.clone(),
                None,
                Some(100 * UNIT),
                1,
                1
            ),
            "{n}"
        );

        let tx = only_tx(fam, &id("sell-credit-trustline-liabilities"), n);
        let o = offers(&tx);
        assert_eq!(o.len(), 1, "{n}: {o:?}");
        let usd_offer = o[0].1;
        assert_ne!(usd_offer, xlm_offer);
        assert_eq!(
            o[0],
            (
                Action::Created,
                usd_offer,
                maker.clone(),
                usd.clone(),
                native.clone(),
                None,
                Some(100 * UNIT),
                2,
                1
            ),
            "{n}"
        );

        // buy 150 XLM at 1: takes all 100 of the maker's XLM offer and rests with 50
        let tx = entry_tx(fam, &id("buy-offer-crosses-and-rests-meta"), n);
        assert!(tx.successful);
        assert_eq!(
            trades(&tx),
            vec![(
                0,
                maker.clone(),
                Some(xlm_offer),
                native.clone(),
                100 * UNIT,
                usd.clone(),
                100 * UNIT
            )],
            "{n}"
        );
        let mut o = offers(&tx);
        o.sort_by_key(|r| r.1);
        let rest = o
            .iter()
            .find(|r| r.2 == taker)
            .expect("taker's resting offer");
        assert_eq!(
            (rest.0, &rest.3, &rest.4, rest.6, rest.7, rest.8),
            (Action::Created, &usd, &native, Some(50 * UNIT), 1, 1),
            "{n}"
        );
        assert!(
            o.contains(&(
                Action::Removed,
                xlm_offer,
                maker.clone(),
                native.clone(),
                usd.clone(),
                Some(100 * UNIT),
                None,
                1,
                1
            )),
            "{n}: {o:?}"
        );
        assert_eq!(native_op_delta(&tx, &taker), 100 * UNIT);
        assert_eq!(delta(&tx, &taker, &usd, "trustline"), -100 * UNIT);
        assert_eq!(native_op_delta(&tx, &maker), -100 * UNIT);
        assert_eq!(delta(&tx, &maker, &usd, "trustline"), 100 * UNIT);

        let tx = only_tx(fam, &id("update-offer-by-id"), n);
        assert_eq!(
            offers(&tx),
            vec![(
                Action::Updated,
                usd_offer,
                maker.clone(),
                usd.clone(),
                native.clone(),
                Some(100 * UNIT),
                Some(60 * UNIT),
                2,
                1
            )],
            "{n}"
        );

        // the bid is i64::MAX - (balance - fee) - HEADROOM, with the balance read after the
        // whale's trust transaction paid its fee
        let headroom = 1_000_000;
        let balance = 100_000_000_000 - BASE_FEE;
        let bid = i128::from(i64::MAX) - (balance - BASE_FEE) - headroom;
        let tx = entry_tx(fam, &id("buy-offer-near-int64-max-meta"), n);
        let o = offers(&tx);
        assert_eq!(o.len(), 1, "{n}: {o:?}");
        assert_eq!(
            (o[0].0, &o[0].2, &o[0].3, &o[0].4, o[0].6, o[0].7, o[0].8),
            (Action::Created, &whale, &usd, &native, Some(bid), 1, 1),
            "{n}"
        );

        // liabilities are not a reported fact, so the failed merge shows only its fee
        let tx = only_tx(fam, &id("merge-dest-full"), n);
        assert!(!tx.successful);
        assert_eq!(tx.fee.source, merger);
        assert!(effect_kinds(&tx).is_empty(), "{n}: {:?}", effect_kinds(&tx));
        assert!(participant(&tx, &whale).is_none(), "{n}");

        // issuers hold no trustline for their own asset, so only the offer appears
        let tx = only_tx(fam, &id("issuer-sells-own-asset"), n);
        let o = offers(&tx);
        assert_eq!(o.len(), 1, "{n}: {o:?}");
        assert_eq!(
            (o[0].0, &o[0].2, &o[0].3, &o[0].4, o[0].6, o[0].7, o[0].8),
            (
                Action::Created,
                &issuer,
                &gbp,
                &native,
                Some(100 * UNIT),
                5,
                1
            ),
            "{n}"
        );
        assert!(
            !op_changes(&tx).iter().any(|c| matches!(
                c,
                Change::TrustlineCreated { .. } | Change::BalanceChange { .. }
            )),
            "{n}: {:?}",
            op_changes(&tx)
        );

        // taker2 sells 40 EUR into maker4's 100 GBP offer at 1 and nothing rests
        let maker2 = account(fam, "maker2");
        let maker3 = account(fam, "maker3");
        let maker4 = account(fam, "maker4");
        let taker2 = account(fam, "taker2");
        let tx = only_tx(fam, &id("credit-pair-cross-partial"), n);
        let t = trades(&tx);
        assert_eq!(t.len(), 1, "{n}: {t:?}");
        assert_eq!(
            (&t[0].1, &t[0].3, t[0].4, &t[0].5, t[0].6),
            (&maker4, &gbp, 40 * UNIT, &eur, 40 * UNIT),
            "{n}"
        );
        let o = offers(&tx);
        assert_eq!(o.len(), 1, "{n}: {o:?}");
        assert_eq!(
            (o[0].0, &o[0].2, o[0].5, o[0].6),
            (Action::Updated, &maker4, Some(100 * UNIT), Some(60 * UNIT)),
            "{n}"
        );
        assert_eq!(delta(&tx, &taker2, &eur, "trustline"), -40 * UNIT);
        assert_eq!(delta(&tx, &taker2, &gbp, "trustline"), 40 * UNIT);
        assert_eq!(delta(&tx, &maker4, &eur, "trustline"), 40 * UNIT);
        assert_eq!(delta(&tx, &maker4, &gbp, "trustline"), -40 * UNIT);

        // a bid for 80 XLM takes maker2's 50 whole and 30 of maker3's 100
        let tx = entry_tx(fam, &id("buy-offer-takes-two-makers-meta"), n);
        let mut t: Vec<_> = trades(&tx)
            .into_iter()
            .map(|r| (r.1, r.3, r.4, r.5, r.6))
            .collect();
        t.sort();
        let mut want = vec![
            (
                maker2.clone(),
                native.clone(),
                50 * UNIT,
                eur.clone(),
                50 * UNIT,
            ),
            (
                maker3.clone(),
                native.clone(),
                30 * UNIT,
                eur.clone(),
                30 * UNIT,
            ),
        ];
        want.sort();
        assert_eq!(t, want, "{n}");
        let o = offers(&tx);
        let of = |who: &str| o.iter().find(|r| r.2 == who).cloned().expect("offer");
        let m2 = of(&maker2);
        assert_eq!((m2.0, m2.5, m2.6), (Action::Removed, Some(50 * UNIT), None));
        let m3 = of(&maker3);
        assert_eq!(
            (m3.0, m3.5, m3.6),
            (Action::Updated, Some(100 * UNIT), Some(70 * UNIT))
        );
        assert!(!o.iter().any(|r| r.2 == taker2), "{n}: nothing rests");
        assert_eq!(native_op_delta(&tx, &taker2), 80 * UNIT);
        assert_eq!(delta(&tx, &taker2, &eur, "trustline"), -80 * UNIT);
    }
}

/// cap15.fee-bump: payer bumps alice's 1 XLM payment to bob and to an unfunded ghost, as a
/// plain fee source and muxed with ids 2^64-1 and 1; the fee is charged to the base account.
#[test]
fn cap15_fee_bump_charges_the_base_fee_source() {
    let fam = "cap15.fee-bump";
    let alice = account(fam, "alice");
    let bob = account(fam, "bob");
    let payer = account(fam, "payer");
    let ghost = account(fam, "ghost");
    let amount = 10_000_000;
    let cases = [
        ("inner-success", true, None),
        ("inner-failed", false, None),
        ("muxed-fee-source-inner-success", true, Some(u64::MAX)),
        ("muxed-fee-source-inner-failed", false, Some(1)),
    ];
    for n in NETWORKS {
        let (chain_id, _) = source(fam, &format!("{fam}.p30-inner-success-meta"));
        let a0 = start_seq(&chain_id, n, &alice);
        for (k, (slug, ok, mux)) in cases.into_iter().enumerate() {
            let ctx = format!("{slug} {n}");
            let tx = entry_tx(fam, &format!("{fam}.p30-{slug}-meta"), n);
            assert_eq!(tx.successful, ok, "{ctx}");
            let want = if ok {
                "tx_fee_bump_inner_success"
            } else {
                "tx_fee_bump_inner_failed"
            };
            assert_eq!(tx.result, want, "{ctx}");
            assert!(tx.inner_hash.is_some(), "{ctx}");
            // one inner operation plus the bump, at the base fee
            assert_eq!(tx.fee.source, payer, "{ctx}");
            assert_eq!(tx.fee.charged.0, 2 * BASE_FEE, "{ctx}");
            assert_eq!(tx.fee.balance_delta.0, -2 * BASE_FEE, "{ctx}");
            assert!(
                !changes_in(&tx, ChangeScope::FeeProcessing)
                    .iter()
                    .any(|c| matches!(c,
                    Change::BalanceChange { holder, .. } if *holder == alice)),
                "{ctx}: alice pays no fee"
            );
            // the inner transaction's sequence number is alice's, one per case so far
            let s = a0 + k as i128;
            assert_eq!(
                seqs(&tx, ChangeScope::TxBefore),
                vec![(alice.clone(), s, s + 1)],
                "{ctx}"
            );
            if ok {
                assert_eq!(native_op_delta(&tx, &alice), -amount, "{ctx}");
                assert_eq!(native_op_delta(&tx, &bob), amount, "{ctx}");
            } else {
                assert!(
                    effect_kinds(&tx).is_empty(),
                    "{ctx}: {:?}",
                    effect_kinds(&tx)
                );
                assert!(participant(&tx, &ghost).is_none(), "{ctx}");
            }
            let fee_source = participant(&tx, &payer).expect("payer");
            assert!(fee_source.roles.iter().any(|r| r == "fee-source"), "{ctx}");
            assert_no_muxed_participants(&tx, &ctx);
            let declared = match mux {
                Some(id) => muxed(fam, "payer", id),
                None => payer.clone(),
            };
            assert_eq!(
                tx.declared.fee_source.as_deref(),
                Some(declared.as_str()),
                "{ctx}"
            );
        }
    }
}

/// cap15.fee-bump: alice wraps her own payment to bob in a fee bump she pays for, after her
/// four bumped payments. Core writes two STATE/UPDATED pairs for her in txChangesBefore, a
/// no-op one-time-signer pass and the sequence bump; the bump must survive the sort.
#[test]
fn cap15_fee_source_is_inner_source_keeps_the_sequence_bump() {
    let fam = "cap15.fee-bump";
    let alice = account(fam, "alice");
    let bob = account(fam, "bob");
    let amount = 10_000_000;
    for n in NETWORKS {
        let id = format!("{fam}.p30-fee-source-is-inner-source-meta");
        let (chain_id, _) = source(fam, &id);
        let s = start_seq(&chain_id, n, &alice) + 4;
        let tx = entry_tx(fam, &id, n);
        assert!(tx.successful, "{n}");
        assert_eq!(tx.fee.source, alice, "{n}");
        assert_eq!(tx.fee.charged.0, 2 * BASE_FEE, "{n}");
        assert_eq!(tx.fee.balance_delta.0, -2 * BASE_FEE, "{n}");
        assert_eq!(
            seqs(&tx, ChangeScope::TxBefore),
            vec![(alice.clone(), s, s + 1)],
            "{n}: {:?}",
            tx.changes
        );
        assert_eq!(native_op_delta(&tx, &alice), -amount, "{n}");
        assert_eq!(native_op_delta(&tx, &bob), amount, "{n}");
        assert_eq!(tx.declared.fee_source.as_deref(), Some(alice.as_str()));
    }
}

/// cap18.maintain-liabilities: AllowTrust moves MNT through 0, 1, 2 and back to 0 (removing
/// the holder's 400 MNT offer), MAINTAINLIAB goes 0 to 2, then one transaction sets it 2 to 1
/// with SetTrustLineFlags and MNT 0 to 2 with AllowTrust.
#[test]
fn cap18_allow_trust_flag_transitions() {
    let fam = "cap18.maintain-liabilities";
    let issuer = account(fam, "issuer");
    let holder = account(fam, "holder");
    let mnt = format!("MNT:{issuer}");
    let mlb = format!("MAINTAINLIAB:{issuer}");
    let row = |op: u32, asset: &str, before: u32, after: u32| {
        (op, holder.clone(), asset.to_string(), before, after)
    };
    let cases = [
        ("allow-trust-authorize-alphanum4", vec![row(0, &mnt, 0, 1)]),
        (
            "allow-trust-unauthorized-to-maintain-alphanum12",
            vec![row(0, &mlb, 0, 2)],
        ),
        (
            "allow-trust-authorized-to-maintain-alphanum4",
            vec![row(0, &mnt, 1, 2)],
        ),
        (
            "allow-trust-maintain-to-revoked-alphanum4",
            vec![row(0, &mnt, 2, 0)],
        ),
        (
            "set-flags-and-allow-trust-maintain",
            vec![row(0, &mlb, 2, 1), row(1, &mnt, 0, 2)],
        ),
    ];
    for n in NETWORKS {
        for (slug, want) in &cases {
            let tx = only_tx(fam, &format!("{fam}.p30-{slug}"), n);
            assert!(tx.successful, "{slug} {n}");
            assert_eq!(tx.fee.source, issuer);
            assert_eq!(&trustline_flags(&tx), want, "{slug} {n}");
        }
        // authorize 2 keeps the offer; only full revocation removes it
        let tx = only_tx(
            fam,
            &format!("{fam}.p30-allow-trust-authorized-to-maintain-alphanum4"),
            n,
        );
        assert!(offers(&tx).is_empty(), "{n}: {:?}", offers(&tx));
        let tx = only_tx(
            fam,
            &format!("{fam}.p30-allow-trust-maintain-to-revoked-alphanum4"),
            n,
        );
        let o = offers(&tx);
        assert_eq!(o.len(), 1, "{n}: {o:?}");
        assert_eq!(
            (
                &o[0].0, &o[0].2, &o[0].3, &o[0].4, o[0].5, o[0].6, o[0].7, o[0].8
            ),
            (
                &Action::Removed,
                &holder,
                &mnt,
                &"native".to_string(),
                Some(4_000_000_000),
                None,
                2,
                1
            ),
            "{n}"
        );
    }
}

/// cap21.preconditions: alice pays bob 1 XLM under ledger and time bounds, an open-ended
/// ledger bound, a min_seq_num that lets her skip 99 numbers, a minimum age and gap, two
/// extra signers, and all of them at once (skipping again).
#[test]
fn cap21_preconditions_move_the_sequence_as_submitted() {
    let fam = "cap21.preconditions";
    let alice = account(fam, "alice");
    let bob = account(fam, "bob");
    let cosigner = account(fam, "cosigner");
    let amount = 10_000_000;
    let jump = 100;
    // (slug, how far the transaction's sequence number moves alice's)
    let cases = [
        ("ledger-and-time-bounds", 1),
        ("ledger-bounds-no-maximum", 1),
        ("min-seq-num-jump", jump),
        ("min-seq-age-and-gap", 1),
        ("two-extra-signers", 1),
        ("all-preconditions", jump),
    ];
    for n in NETWORKS {
        let (chain_id, _) = source(fam, &format!("{fam}.p30-all-preconditions-meta"));
        let mut s = start_seq(&chain_id, n, &alice);
        for (slug, step) in cases {
            let tx = entry_tx(fam, &format!("{fam}.p30-{slug}-meta"), n);
            assert!(tx.successful, "{slug} {n}");
            assert_eq!(
                seqs(&tx, ChangeScope::TxBefore),
                vec![(alice.clone(), s, s + step)],
                "{slug} {n}"
            );
            s += step;
            assert_eq!(native_op_delta(&tx, &alice), -amount, "{slug} {n}");
            assert_eq!(native_op_delta(&tx, &bob), amount, "{slug} {n}");
            // an extra signer only signs the envelope and touches no entry
            assert!(participant(&tx, &cosigner).is_none(), "{slug} {n}");
        }
    }
}

/// cap23.claimable-balances: the creator puts 100 XLM in a balance for claimant-0 to
/// claimant-9, then claimant-9 claims it through its depth-four predicate.
#[test]
fn cap23_ten_claimants_create_and_claim() {
    let fam = "cap23.claimable-balances";
    let creator = account(fam, "creator");
    let claimants: Vec<String> = (0..10)
        .map(|i| account(fam, &format!("claimant-{i}")))
        .collect();
    let amount = 1_000_000_000;
    for n in NETWORKS {
        let id = format!("{fam}.p30-create-ten-claimants-depth-4-meta");
        let (chain_id, _) = source(fam, &id);
        let bid = balance_id(&creator, start_seq(&chain_id, n, &creator) + 1, 0);
        let tx = entry_tx(fam, &id, n);
        assert!(tx.successful);
        assert_eq!(
            balances_created(&tx),
            vec![(bid.clone(), "native".into(), amount, claimants.clone())],
            "{n}"
        );
        assert_eq!(native_op_delta(&tx, &creator), -amount, "{n}");
        assert_eq!(delta(&tx, &bid, "native", "claimable-balance"), amount);
        // claimants are named in the entry only, so none of them is a participant yet
        for c in &claimants {
            assert!(participant(&tx, c).is_none(), "{n} {c}");
        }

        let tx = entry_tx(fam, &format!("{fam}.p30-claim-through-depth-4-meta"), n);
        assert!(tx.successful);
        assert_eq!(tx.fee.source, claimants[9]);
        let claimed: Vec<_> = op_changes(&tx)
            .into_iter()
            .filter_map(|c| match c {
                Change::ClaimableBalanceClaimed {
                    balance_id,
                    asset,
                    amount,
                } => Some((balance_id.clone(), asset.clone(), amount.0)),
                _ => None,
            })
            .collect();
        assert_eq!(claimed, vec![(bid.clone(), "native".into(), amount)], "{n}");
        assert_eq!(native_op_delta(&tx, &claimants[9]), amount, "{n}");
        assert_eq!(delta(&tx, &bid, "native", "claimable-balance"), -amount);
        // the creator paid the entry's reserve, which the claim hands back
        assert_eq!(
            sponsorships(&tx),
            vec![(
                format!("claimable balance {bid}"),
                None,
                Some(creator.clone()),
                None
            )],
            "{n}"
        );
    }
}

/// cap27.muxed-accounts: muxed alice pays muxed bob under ids 0, 1, 2^53 and 2^64-1, four
/// payments with muxed operation sources, path payments, an issued payment and a clawback to
/// muxed holder, and leaver merging into muxed alice; all resolve to the base accounts.
#[test]
fn cap27_muxed_accounts_resolve_to_base_accounts() {
    let fam = "cap27.muxed-accounts";
    let alice = account(fam, "alice");
    let bob = account(fam, "bob");
    let issuer = account(fam, "issuer");
    let holder = account(fam, "holder");
    let leaver = account(fam, "leaver");
    let muxd = format!("MUXD:{issuer}");
    let amount = 10_000_000;
    let ids: [(u64, &str); 4] = [(0, "0"), (1, "1"), (1 << 53, "2p53"), (u64::MAX, "2p64-1")];
    for p in [28, 30] {
        for n in NETWORKS {
            for (mux, name) in ids {
                let ctx = format!("p{p} {n} id {name}");
                let tx = only_tx(
                    fam,
                    &format!("{fam}.p{p}-tx-source-and-payment-destination-id-{name}"),
                    n,
                );
                assert!(tx.successful, "{ctx}");
                assert_eq!(tx.fee.source, alice, "{ctx}");
                assert_eq!(native_op_delta(&tx, &alice), -amount, "{ctx}");
                assert_eq!(native_op_delta(&tx, &bob), amount, "{ctx}");
                assert_eq!(
                    credited_muxed_ids(&tx),
                    vec![("transfer".into(), Some(ToMuxedId::U64(mux.into())))],
                    "{ctx}"
                );
                assert_no_muxed_participants(&tx, &ctx);
                assert_eq!(tx.declared.source, muxed(fam, "alice", mux), "{ctx}");
            }

            let ctx = format!("p{p} {n} operation sources");
            let tx = only_tx(fam, &format!("{fam}.p{p}-operation-sources-four-ids"), n);
            assert_eq!(tx.fee.charged.0, 4 * BASE_FEE, "{ctx}");
            for op in 0..4 {
                assert_eq!(delta_at(&tx, op, &alice, "native", "account"), -amount);
                assert_eq!(delta_at(&tx, op, &bob, "native", "account"), amount);
            }
            // a plain destination and no memo, so the transfers carry no to_muxed_id
            assert_eq!(
                credited_muxed_ids(&tx),
                vec![("transfer".to_string(), None); 4],
                "{ctx}"
            );
            assert_no_muxed_participants(&tx, &ctx);

            for (slug, mux) in [
                (
                    "path-payment-strict-receive-destination-id-2p64-1",
                    u64::MAX,
                ),
                ("path-payment-strict-send-destination-id-1", 1),
            ] {
                let ctx = format!("p{p} {n} {slug}");
                let tx = only_tx(fam, &format!("{fam}.p{p}-{slug}"), n);
                assert_eq!(native_op_delta(&tx, &alice), -amount, "{ctx}");
                assert_eq!(native_op_delta(&tx, &bob), amount, "{ctx}");
                assert_eq!(
                    credited_muxed_ids(&tx),
                    vec![("transfer".into(), Some(ToMuxedId::U64(mux.into())))],
                    "{ctx}"
                );
                assert_no_muxed_participants(&tx, &ctx);
            }

            let ctx = format!("p{p} {n} issued payment");
            let tx = only_tx(
                fam,
                &format!("{fam}.p{p}-issued-payment-destination-id-2p53"),
                n,
            );
            assert_eq!(delta(&tx, &holder, &muxd, "trustline"), amount, "{ctx}");
            assert_eq!(
                credited_muxed_ids(&tx),
                vec![("mint".into(), Some(ToMuxedId::U64((1u64 << 53).into())))],
                "{ctx}"
            );
            assert_no_muxed_participants(&tx, &ctx);

            let ctx = format!("p{p} {n} clawback");
            let tx = only_tx(fam, &format!("{fam}.p{p}-clawback-from-id-2p64-1"), n);
            let lines: Vec<_> = op_changes(&tx)
                .into_iter()
                .filter_map(|c| match c {
                    Change::BalanceChange {
                        holder: h,
                        asset,
                        before,
                        after,
                        ..
                    } if *h == holder && *asset == muxd => Some((before.0, after.0)),
                    _ => None,
                })
                .collect();
            assert_eq!(lines, vec![(amount, 0)], "{ctx}");
            let cb: Vec<_> = token_events(&tx)
                .into_iter()
                .filter(|t| t.event == "clawback")
                .map(|t| (t.from.clone(), t.amount))
                .collect();
            assert_eq!(cb, vec![(Some(holder.clone()), Some(Int(amount)))], "{ctx}");
            assert_no_muxed_participants(&tx, &ctx);

            // leaver had its starting balance less one transaction's fee
            let ctx = format!("p{p} {n} merge");
            let left = 100_000_000_000 - BASE_FEE;
            let tx = only_tx(
                fam,
                &format!("{fam}.p{p}-account-merge-destination-id-2p53"),
                n,
            );
            assert!(
                op_changes(&tx).iter().any(|c| matches!(c,
                    Change::AccountRemoved { account, final_balance }
                        if *account == leaver && final_balance.0 == left)),
                "{ctx}: {:?}",
                op_changes(&tx)
            );
            assert_eq!(native_op_delta(&tx, &alice), left, "{ctx}");
            assert_eq!(
                credited_muxed_ids(&tx),
                vec![("transfer".into(), Some(ToMuxedId::U64((1u64 << 53).into())))],
                "{ctx}"
            );
            assert_no_muxed_participants(&tx, &ctx);
        }
    }
}

/// cap33.sponsorship: inside begin and end sponsoring, the sponsor creates an account with
/// 100 XLM and the sponsored account adds a SPON trustline, an ed25519 signer and a 10 XLM
/// claimable balance for the sponsor; then the sponsor revokes the signer and the trustline.
#[test]
fn cap33_sponsorship_moves_reserves_to_the_sponsor() {
    let fam = "cap33.sponsorship";
    let sponsor = account(fam, "sponsor");
    let sponsored = account(fam, "sponsored");
    let issuer = account(fam, "issuer");
    let signer = account(fam, "signer");
    let spon = format!("SPON:{issuer}");
    let line_target = format!("trustline {sponsored} {spon}");
    let account_target = format!("account {sponsored}");
    let id = |slug: &str| format!("{fam}.p30-{slug}");
    for n in NETWORKS {
        let tx = entry_tx(fam, &id("sponsored-account-meta"), n);
        assert!(tx.successful);
        assert_eq!(tx.fee.charged.0, 3 * BASE_FEE, "{n}");
        assert!(
            op_changes(&tx).iter().any(|c| matches!(c,
                Change::AccountCreated { account, starting_balance }
                    if *account == sponsored && starting_balance.0 == 1_000_000_000)),
            "{n}"
        );
        assert_eq!(native_op_delta(&tx, &sponsor), -1_000_000_000, "{n}");
        assert_eq!(
            sponsorships(&tx),
            vec![(account_target.clone(), None, None, Some(sponsor.clone()))],
            "{n}"
        );

        let tx = entry_tx(fam, &id("sponsored-trustline-meta"), n);
        let lines: Vec<_> = op_changes(&tx)
            .into_iter()
            .filter_map(|c| match c {
                Change::TrustlineCreated {
                    account,
                    asset,
                    limit,
                    flags,
                } => Some((account.clone(), asset.clone(), limit.0, *flags)),
                _ => None,
            })
            .collect();
        // the issuer sets no auth flags, so the new trustline starts authorized
        assert_eq!(
            lines,
            vec![(sponsored.clone(), spon.clone(), i64::MAX.into(), 1)],
            "{n}"
        );
        assert_eq!(
            sponsorships(&tx),
            vec![(line_target.clone(), None, None, Some(sponsor.clone()))],
            "{n}"
        );

        let tx = entry_tx(fam, &id("sponsored-signer-meta"), n);
        assert!(
            op_changes(&tx).iter().any(|c| matches!(c,
                Change::SignerAdded { account, signer: s, weight: 1 }
                    if *account == sponsored && *s == signer)),
            "{n}"
        );
        assert_eq!(
            sponsorships(&tx),
            vec![(
                account_target.clone(),
                Some(signer.clone()),
                None,
                Some(sponsor.clone())
            )],
            "{n}"
        );
        // the signer key is no account and signs nothing on ledger
        assert!(participant(&tx, &signer).is_none(), "{n}");

        // the balance id hashes the parent transaction's source and sequence, the sponsor's
        // fifth transaction, and the operation index 1
        let cb = id("sponsored-claimable-balance-meta");
        let (chain_id, _) = source(fam, &cb);
        let bid = balance_id(&sponsor, start_seq(&chain_id, n, &sponsor) + 4, 1);
        let tx = entry_tx(fam, &cb, n);
        assert_eq!(
            balances_created(&tx),
            vec![(
                bid.clone(),
                "native".into(),
                100_000_000,
                vec![sponsor.clone()]
            )],
            "{n}"
        );
        assert_eq!(native_op_delta(&tx, &sponsored), -100_000_000, "{n}");
        assert_eq!(
            sponsorships(&tx),
            vec![(
                format!("claimable balance {bid}"),
                None,
                None,
                Some(sponsor.clone())
            )],
            "{n}"
        );

        let tx = entry_tx(fam, &id("revoke-signer-sponsorship-meta"), n);
        assert_eq!(
            sponsorships(&tx),
            vec![(
                account_target.clone(),
                Some(signer.clone()),
                Some(sponsor.clone()),
                None
            )],
            "{n}"
        );
        let tx = entry_tx(fam, &id("revoke-trustline-sponsorship-meta"), n);
        assert_eq!(
            sponsorships(&tx),
            vec![(line_target.clone(), None, Some(sponsor.clone()), None)],
            "{n}"
        );
    }
}

/// cap35.clawback: holder gets 1000 CLAW, puts 100 in a claimable balance for the claimant,
/// the issuer claws that balance back and 250 from the trustline, moves the trustline to
/// maintain liabilities and back to authorized without clawback, so the next clawback fails.
#[test]
fn cap35_clawback_of_trustline_and_claimable_balance() {
    let fam = "cap35.clawback";
    let issuer = account(fam, "issuer");
    let holder = account(fam, "holder");
    let claimant = account(fam, "claimant");
    let claw = format!("CLAW:{issuer}");
    let (paid, cb_amount, clawed) = (10_000_000_000, 1_000_000_000, 2_500_000_000);
    let (authorized, maintain, clawback) = (1, 2, 4);
    let id = |slug: &str| format!("{fam}.p30-{slug}");
    for n in NETWORKS {
        let (chain_id, _) = source(fam, &id("create-clawback-enabled-balance"));
        // the holder's second transaction, after its trust
        let bid = balance_id(&holder, start_seq(&chain_id, n, &holder) + 2, 0);
        let tx = only_tx(fam, &id("create-clawback-enabled-balance"), n);
        assert_eq!(
            balances_created(&tx),
            vec![(bid.clone(), claw.clone(), cb_amount, vec![claimant.clone()])],
            "{n}"
        );
        assert_eq!(delta(&tx, &holder, &claw, "trustline"), -cb_amount, "{n}");

        let tx = entry_tx(fam, &id("clawback-claimable-balance-meta"), n);
        assert_eq!(tx.fee.source, issuer);
        let back: Vec<_> = op_changes(&tx)
            .into_iter()
            .filter_map(|c| match c {
                Change::ClaimableBalanceClawedBack {
                    balance_id,
                    asset,
                    amount,
                } => Some((balance_id.clone(), asset.clone(), amount.0)),
                _ => None,
            })
            .collect();
        assert_eq!(back, vec![(bid.clone(), claw.clone(), cb_amount)], "{n}");
        assert!(
            !op_changes(&tx)
                .iter()
                .any(|c| matches!(c, Change::ClaimableBalanceClaimed { .. })),
            "{n}"
        );
        assert!(participant(&tx, &claimant).is_none(), "{n}");

        let tx = only_tx(fam, &id("clawback"), n);
        let lines: Vec<_> = op_changes(&tx)
            .into_iter()
            .filter_map(|c| match c {
                Change::BalanceChange {
                    holder: h,
                    asset,
                    before,
                    after,
                    ..
                } if *h == holder && *asset == claw => Some((before.0, after.0)),
                _ => None,
            })
            .collect();
        assert_eq!(
            lines,
            vec![(paid - cb_amount, paid - cb_amount - clawed)],
            "{n}"
        );

        let tx = only_tx(fam, &id("set-flags-authorized-to-maintain"), n);
        assert_eq!(
            trustline_flags(&tx),
            vec![(
                0,
                holder.clone(),
                claw.clone(),
                authorized | clawback,
                maintain | clawback
            )],
            "{n}"
        );
        let tx = only_tx(fam, &id("set-flags-reauthorize-clear-clawback"), n);
        assert_eq!(
            trustline_flags(&tx),
            vec![(
                0,
                holder.clone(),
                claw.clone(),
                maintain | clawback,
                authorized
            )],
            "{n}"
        );

        let tx = only_tx(fam, &id("clawback-not-enabled"), n);
        assert!(!tx.successful, "{n}");
        assert!(effect_kinds(&tx).is_empty(), "{n}: {:?}", effect_kinds(&tx));
        assert!(participant(&tx, &holder).is_none(), "{n}");
    }
}

/// The issuer roles of `cap38.liquidity-pools`, lower key bytes first, found the way the
/// recipe's `issuer_pair` finds them: the first pair whose strkey and byte orders disagree.
fn cap38_issuers(fam: &str) -> (String, String) {
    for j in 1..64usize {
        for i in 0..j {
            let (ri, rj) = (format!("issuer-{i}"), format!("issuer-{j}"));
            let by_bytes = key_bytes(fam, &ri) < key_bytes(fam, &rj);
            let by_strkey = account(fam, &ri) < account(fam, &rj);
            if by_bytes != by_strkey {
                return if by_bytes { (ri, rj) } else { (rj, ri) };
            }
        }
    }
    panic!("no issuer pair");
}

/// (pool, action, reserve a before/after, reserve b before/after, shares before/after)
type PoolRow = (String, Action, [i128; 6]);

fn pools(tx: &TxFacts) -> Vec<PoolRow> {
    op_changes(tx)
        .into_iter()
        .filter_map(|c| match c {
            Change::LiquidityPool {
                pool,
                action,
                reserve_a_before,
                reserve_a_after,
                reserve_b_before,
                reserve_b_after,
                shares_before,
                shares_after,
                ..
            } => Some((
                pool.clone(),
                *action,
                [
                    reserve_a_before.0,
                    reserve_a_after.0,
                    reserve_b_before.0,
                    reserve_b_after.0,
                    shares_before.0,
                    shares_after.0,
                ],
            )),
            _ => None,
        })
        .collect()
}

fn pool_shares(tx: &TxFacts) -> Vec<(String, String, i128, i128)> {
    op_changes(tx)
        .into_iter()
        .filter_map(|c| match c {
            Change::PoolShares {
                account,
                pool,
                before,
                after,
                ..
            } => Some((account.clone(), pool.clone(), before.0, after.0)),
            _ => None,
        })
        .collect()
}

/// cap38.liquidity-pools: the holder trusts the USD/USD pool in key byte order, deposits 100
/// of each, the trader sends 10 USD of a through it, the holder withdraws a quarter of its
/// shares, and issuer a's revocation redeems the rest into claimable balances.
#[test]
fn cap38_pool_lifecycle_follows_the_constant_product() {
    let fam = "cap38.liquidity-pools";
    let (lo, hi) = cap38_issuers(fam);
    let (ia, ib) = (account(fam, &lo), account(fam, &hi));
    assert!(ib < ia, "the pair sorts the other way by strkey");
    let holder = account(fam, "holder");
    let trader = account(fam, "trader");
    let (a, b) = (format!("USD:{ia}"), format!("USD:{ib}"));
    let usd = |issuer: &str| {
        stellar_xdr::Asset::CreditAlphanum4(AlphaNum4 {
            asset_code: AssetCode4(*b"USD\0"),
            issuer: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(key_bytes(
                fam, issuer,
            )))),
        })
    };
    let params = LiquidityPoolParameters::LiquidityPoolConstantProduct(
        LiquidityPoolConstantProductParameters {
            asset_a: usd(&lo),
            asset_b: usd(&hi),
            fee: 30,
        },
    );
    let pool_hash: [u8; 32] = Sha256::digest(params.to_xdr(Limits::none()).unwrap()).into();
    let pool = format!("{}", stellar_strkey::LiquidityPool(pool_hash));
    let id = |slug: &str| format!("{fam}.p30-{slug}");

    // the first deposit takes all of both maxima and mints sqrt(a * b) shares
    let deposit = 100 * UNIT;
    // strict send of 10 a: out = floor(rb * in * 9970 / (ra * 10000 + in * 9970))
    let sent = 10 * UNIT;
    let out = deposit * sent * 9970 / (deposit * 10_000 + sent * 9970);
    let (ra, rb) = (deposit + sent, deposit - out);
    // withdrawing 25 of 100 shares returns floor(reserve / 4) of each asset
    let (wa, wb) = (ra * 25 / 100, rb * 25 / 100);
    let (ra2, rb2, shares2) = (ra - wa, rb - wb, deposit - 25 * UNIT);
    for n in NETWORKS {
        let tx = entry_tx(fam, &id("change-trust-pool-byte-order-meta"), n);
        assert!(tx.successful);
        let created: Vec<_> = op_changes(&tx)
            .into_iter()
            .filter_map(|c| match c {
                Change::LiquidityPool {
                    pool,
                    action: Action::Created,
                    asset_a,
                    asset_b,
                    ..
                } => Some((pool.clone(), asset_a.clone(), asset_b.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(created, vec![(pool.clone(), a.clone(), b.clone())], "{n}");
        assert!(
            op_changes(&tx).iter().any(|c| matches!(c,
                Change::TrustlineCreated { account, asset, limit, .. }
                    if *account == holder && *asset == format!("pool-share:{pool}")
                        && limit.0 == i128::from(i64::MAX))),
            "{n}"
        );

        let tx = entry_tx(fam, &id("deposit-meta"), n);
        assert_eq!(
            pools(&tx),
            vec![(
                pool.clone(),
                Action::Updated,
                [0, deposit, 0, deposit, 0, deposit]
            )],
            "{n}"
        );
        assert_eq!(
            pool_shares(&tx),
            vec![(holder.clone(), pool.clone(), 0, deposit)]
        );
        assert_eq!(delta(&tx, &holder, &a, "trustline"), -deposit, "{n}");
        assert_eq!(delta(&tx, &holder, &b, "trustline"), -deposit, "{n}");

        let tx = only_tx(fam, &id("path-payment-through-pool"), n);
        assert_eq!(
            pools(&tx),
            vec![(
                pool.clone(),
                Action::Updated,
                [deposit, ra, deposit, rb, deposit, deposit]
            )],
            "{n}"
        );
        assert_eq!(
            trades(&tx),
            vec![(0, pool.clone(), None, b.clone(), out, a.clone(), sent)],
            "{n}"
        );
        assert_eq!(delta(&tx, &trader, &a, "trustline"), -sent, "{n}");
        assert_eq!(delta(&tx, &trader, &b, "trustline"), out, "{n}");

        let tx = entry_tx(fam, &id("withdraw-meta"), n);
        assert_eq!(
            pools(&tx),
            vec![(
                pool.clone(),
                Action::Updated,
                [ra, ra2, rb, rb2, deposit, shares2]
            )],
            "{n}"
        );
        assert_eq!(
            pool_shares(&tx),
            vec![(holder.clone(), pool.clone(), deposit, shares2)]
        );
        assert_eq!(delta(&tx, &holder, &a, "trustline"), wa, "{n}");
        assert_eq!(delta(&tx, &holder, &b, "trustline"), wb, "{n}");

        let tx = only_tx(fam, &id("revoke-redeems-pool-shares"), n);
        assert_eq!(tx.fee.source, ia);
        assert_eq!(
            pools(&tx),
            vec![(pool.clone(), Action::Removed, [ra2, 0, rb2, 0, shares2, 0])],
            "{n}"
        );
        assert_eq!(
            pool_shares(&tx),
            vec![(holder.clone(), pool.clone(), shares2, 0)]
        );
        let mut cbs: Vec<_> = balances_created(&tx)
            .into_iter()
            .map(|(_, asset, amount, claimants)| (asset, amount, claimants))
            .collect();
        cbs.sort();
        let mut want = vec![
            (a.clone(), ra2, vec![holder.clone()]),
            (b.clone(), rb2, vec![holder.clone()]),
        ];
        want.sort();
        assert_eq!(cbs, want, "{n}");
        assert_eq!(
            trustline_flags(&tx),
            vec![(0, holder.clone(), a.clone(), 1, 0)],
            "{n}"
        );
        assert!(
            op_changes(&tx).iter().any(|c| matches!(c,
                Change::TrustlineRemoved { account, asset }
                    if *account == holder && *asset == format!("pool-share:{pool}"))),
            "{n}"
        );
    }
}

/// `n` bytes of `seed` repeated, the recipe's `text`.
fn text(n: usize, seed: &str) -> Vec<u8> {
    seed.bytes().cycle().take(n).collect()
}

/// classic.max-length: multisig adds 20 signers with a medium threshold of 20 and pays with 20
/// signatures; bulk sends 100 payments, a 28 byte memo, a 64 byte data entry and 5 asset path
/// payments; multisig sets a 32 byte home domain.
#[test]
fn classic_max_length_vectors_reach_the_meta() {
    let fam = "classic.max-length";
    let multisig = account(fam, "multisig");
    let bulk = account(fam, "bulk");
    let sink = account(fam, "sink");
    let issuer = account(fam, "issuer");
    let maker = account(fam, "maker");
    let receiver = account(fam, "receiver");
    let signers: Vec<String> = (0..20)
        .map(|i| account(fam, &format!("signer-{i:02}")))
        .collect();
    let id = |slug: &str| format!("{fam}.p30-{slug}");
    for n in NETWORKS {
        // a ledger entry capture, read through the ledger it was taken at
        let tx = only_tx(fam, &id("account-20-signers"), n);
        let mut added: Vec<_> = op_changes(&tx)
            .into_iter()
            .filter_map(|c| match c {
                Change::SignerAdded {
                    account,
                    signer,
                    weight,
                } if *account == multisig => Some((signer.clone(), *weight)),
                _ => None,
            })
            .collect();
        added.sort();
        let mut want: Vec<_> = signers.iter().map(|s| (s.clone(), 1)).collect();
        want.sort();
        assert_eq!(added, want, "{n}");
        assert!(
            op_changes(&tx).iter().any(|c| matches!(c,
                Change::Thresholds { account, before, after }
                    if *account == multisig && before.medium == 0 && after.medium == 20)),
            "{n}"
        );

        // signatures only authorize, so none of the signers is a participant
        let tx = only_tx(fam, &id("signatures-20"), n);
        assert!(tx.successful);
        assert_eq!(native_op_delta(&tx, &multisig), -UNIT, "{n}");
        assert_eq!(native_op_delta(&tx, &sink), UNIT, "{n}");
        for s in &signers {
            assert!(participant(&tx, s).is_none(), "{n} {s}");
        }

        let tx = only_tx(fam, &id("operations-100"), n);
        assert_eq!(tx.fee.charged.0, 100 * BASE_FEE, "{n}");
        for i in 0..100u32 {
            let amount = UNIT + i128::from(i);
            assert_eq!(
                delta_at(&tx, i, &sink, "native", "account"),
                amount,
                "{n} {i}"
            );
            assert_eq!(
                delta_at(&tx, i, &bulk, "native", "account"),
                -amount,
                "{n} {i}"
            );
        }

        // core copies a text memo into the transfer's to_muxed_id; it creates no participant
        let memo = text(28, "shapekit memo at its bound ");
        let tx = only_tx(fam, &id("memo-text-28"), n);
        assert_eq!(native_op_delta(&tx, &sink), UNIT, "{n}");
        assert_eq!(
            credited_muxed_ids(&tx),
            vec![(
                "transfer".into(),
                Some(ToMuxedId::String(hex::encode(&memo)))
            )],
            "{n}"
        );
        let mut accts = account_participants(&tx);
        accts.sort();
        let mut want = vec![bulk.clone(), sink.clone()];
        want.sort();
        assert_eq!(accts, want, "{n}");

        let tx = entry_tx(fam, &id("data-name-value-64-meta"), n);
        let data: Vec<_> = op_changes(&tx)
            .into_iter()
            .filter_map(|c| match c {
                Change::DataEntry {
                    account,
                    action,
                    name_hex,
                    value_before_hex,
                    value_after_hex,
                } => Some((
                    account.clone(),
                    *action,
                    name_hex.clone(),
                    value_before_hex.clone(),
                    value_after_hex.clone(),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(
            data,
            vec![(
                bulk.clone(),
                Action::Created,
                hex::encode(text(64, "shapekit-data-name-")),
                None,
                Some(hex::encode(text(64, "shapekit data value ")))
            )],
            "{n}"
        );

        // each maker offer sells P(i) for P(i-1) at 1, so 10 units cross six offers
        let p = |i: usize| format!("P{i}:{issuer}");
        let q = |i: usize| format!("Q{i}:{issuer}");
        let hop = |i: usize| account(fam, &format!("hop-maker-{i}"));
        let route: Vec<_> = (1..=6)
            .map(|i| {
                let bought = if i == 1 {
                    "native".to_string()
                } else {
                    p(i - 1)
                };
                (maker.clone(), p(i), 10 * UNIT, bought, 10 * UNIT)
            })
            .collect();
        let rows = |tx: &TxFacts, op: u32| -> Vec<_> {
            trades(tx)
                .into_iter()
                .filter(|t| t.0 == op)
                .map(|t| (t.1, t.3, t.4, t.5, t.6))
                .collect()
        };
        for slug in ["path-strict-receive-5", "path-strict-send-5"] {
            let tx = only_tx(fam, &id(slug), n);
            assert!(tx.successful, "{slug} {n}");
            assert_eq!(rows(&tx, 0), route, "{slug} {n}");
            assert_eq!(delta_at(&tx, 0, &receiver, &p(6), "trustline"), 10 * UNIT);
            assert_eq!(delta_at(&tx, 0, &bulk, "native", "account"), -10 * UNIT);
        }
        let tx = only_tx(fam, &id("path-strict-receive-5"), n);
        let credit: Vec<_> = (1..=3)
            .map(|i| (hop(i), q(i + 1), 10 * UNIT, q(i), 10 * UNIT))
            .collect();
        assert_eq!(rows(&tx, 1), credit, "{n}");
        assert_eq!(delta_at(&tx, 1, &receiver, &q(4), "trustline"), 10 * UNIT);
        assert_eq!(delta_at(&tx, 1, &bulk, &q(1), "trustline"), -10 * UNIT);

        let tx = only_tx(fam, &id("home-domain-32"), n);
        let domains: Vec<_> = op_changes(&tx)
            .into_iter()
            .filter_map(|c| match c {
                Change::HomeDomain {
                    account,
                    before,
                    after,
                } => Some((account.clone(), before.clone(), after.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(
            domains,
            vec![(
                multisig.clone(),
                String::new(),
                "~shapekit home/domain at bound ~".to_string()
            )],
            "{n}"
        );
    }
}

/// The checks every memo payment shares: sender pays sink 1 stroop, the transfer carries
/// `want` as to_muxed_id, and the memo adds no participant or mux id.
fn assert_memo_payment(fam: &str, tx: &TxFacts, want: ToMuxedId, ctx: &str) {
    let sender = account(fam, "sender");
    let sink = account(fam, "sink");
    assert!(tx.successful, "{ctx}");
    assert_eq!(native_op_delta(tx, &sender), -1, "{ctx}");
    assert_eq!(native_op_delta(tx, &sink), 1, "{ctx}");
    assert_eq!(
        credited_muxed_ids(tx),
        vec![("transfer".into(), Some(want))],
        "{ctx}"
    );
    let mut accts = account_participants(tx);
    accts.sort();
    let mut expect = vec![sender, sink];
    expect.sort();
    assert_eq!(accts, expect, "{ctx}");
    assert_no_muxed_participants(tx, ctx);
}

/// classic.memo-arms: sender pays sink 1 stroop with MEMO_ID 0, 2^63, 2^64-1, MEMO_HASH and
/// MEMO_RETURN of zeros, 0xff and a sha256; then MEMO_ID 2^64-1 to sink muxed with id 7.
#[test]
fn classic_memo_arms_reach_to_muxed_id_only() {
    let fam = "classic.memo-arms";
    let digest = |label: &str| -> [u8; 32] { Sha256::digest(label.as_bytes()).into() };
    let bytes = |b: [u8; 32]| ToMuxedId::Bytes(hex::encode(b));
    let cases = [
        ("memo-id-zero", ToMuxedId::U64(0u64.into())),
        ("memo-id-2-63", ToMuxedId::U64((1u64 << 63).into())),
        ("memo-id-max", ToMuxedId::U64(u64::MAX.into())),
        ("memo-hash-zero", bytes([0; 32])),
        ("memo-hash-ff", bytes([0xff; 32])),
        ("memo-hash-sha256", bytes(digest("shapekit/memo-arms/hash"))),
        ("memo-return-zero", bytes([0; 32])),
        ("memo-return-ff", bytes([0xff; 32])),
        (
            "memo-return-sha256",
            bytes(digest("shapekit/memo-arms/return")),
        ),
        // the mux id wins over the memo
        ("memo-id-muxed-destination", ToMuxedId::U64(7u64.into())),
    ];
    for n in NETWORKS {
        for (slug, want) in &cases {
            let tx = only_tx(fam, &format!("{fam}.p30-{slug}"), n);
            assert_memo_payment(fam, &tx, want.clone(), &format!("{slug} {n}"));
        }
    }
}

/// classic.memo-bytes: sender pays sink 1 stroop with text memos that are not UTF-8: ff, 80,
/// c080, eda080, f4908080 and 26 ascii bytes cut by e282.
#[test]
fn classic_memo_bytes_keep_their_raw_bytes() {
    let fam = "classic.memo-bytes";
    let mut cut = b"shapekit memo cut at byte ".to_vec();
    cut.extend_from_slice(&[0xe2, 0x82]);
    assert_eq!(cut.len(), 28);
    let cases: [(&str, Vec<u8>); 6] = [
        ("memo-ff", vec![0xff]),
        ("memo-lone-continuation", vec![0x80]),
        ("memo-overlong-nul", vec![0xc0, 0x80]),
        ("memo-surrogate", vec![0xed, 0xa0, 0x80]),
        ("memo-above-max-code-point", vec![0xf4, 0x90, 0x80, 0x80]),
        ("memo-truncated-sequence-28", cut),
    ];
    for n in NETWORKS {
        for (slug, memo) in &cases {
            let tx = only_tx(fam, &format!("{fam}.p30-{slug}"), n);
            assert_memo_payment(
                fam,
                &tx,
                ToMuxedId::String(hex::encode(memo)),
                &format!("{slug} {n}"),
            );
        }
    }
}
