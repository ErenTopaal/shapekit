//! `kind = textual-id` vectors: muxed account ids and offer ids given as text or as a native
//! number (js-stellar-sdk#1785).
//!
//! The canonical rule follows from the XDR types. A muxed id is `uint64` (MuxedAccount
//! med25519.id), so it is any value 0 to 2^64-1. An offer id is `int64` (ManageSellOfferOp,
//! ManageBuyOfferOp, LedgerKeyOffer), and core answers MALFORMED for a negative one from
//! protocol 15 (ManageOfferOpFrameBase::doCheckValid), so it is 0 to 2^63-1, where 0 means
//! "create a new offer".
//!
//! - Text is accepted only when it is the decimal rendering of the value it parses to: ASCII
//!   digits, no sign, no whitespace, no leading zero except "0" itself, no radix prefix, no
//!   fraction or exponent, and in range. So the text and the id that gets encoded can never
//!   disagree.
//! - A native number (a JSON number in the vector) is accepted when it is an integer in range
//!   with |n| <= 2^53-1. Negative numbers, fractions and values outside the XDR range are
//!   refused like their textual forms.
//! - A JSON number above 2^53-1 that is otherwise in range is implementation-defined. That
//!   bound is not a network rule: RFC 8259 section 6 only says such numbers are not
//!   interoperable, because many JSON and language number types are IEEE-754 doubles that
//!   cannot hold them exactly. js-stellar-sdk#1785 is that failure: the number was rounded to
//!   a neighbour and a different id was encoded. A testee passes by encoding exactly the
//!   written id or by refusing; encoding any other id fails.

use anyhow::{Result, ensure};
use serde_json::{Number, Value, json};
use shapekit_manifest::{EntryKind, Validity};
use stellar_xdr as x;

use crate::synth::{LINK_JS_1785, LINK_RFC_8259, Spec, VectorItem, repr};
use crate::synth_xdr::key;

pub const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

/// The refusal reason for an in-range JSON number above 2^53-1, which is not a network rule.
pub const UNSAFE_INTEGER: &str = "unsafe-integer";

#[derive(Clone, Copy, PartialEq, Eq)]
enum IdKind {
    Muxed,
    Offer,
}

impl IdKind {
    fn max(self) -> u64 {
        match self {
            IdKind::Muxed => u64::MAX,
            IdKind::Offer => i64::MAX as u64,
        }
    }

    fn name(self) -> &'static str {
        match self {
            IdKind::Muxed => "muxed",
            IdKind::Offer => "offer",
        }
    }
}

/// The canonical id of a textual input, or why it is refused.
pub fn parse_text(text: &str, max: u64) -> std::result::Result<u64, &'static str> {
    if text.is_empty() {
        return Err("empty");
    }
    if !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err("not-decimal-digits");
    }
    if text.len() > 1 && text.starts_with('0') {
        return Err("leading-zero");
    }
    let v: u64 = text.parse().map_err(|_| "out-of-range")?;
    if v > max {
        return Err("out-of-range");
    }
    Ok(v)
}

/// The canonical id of a native number input, or why it is refused.
pub fn parse_number(n: &Number, max: u64) -> std::result::Result<u64, &'static str> {
    if let Some(v) = n.as_u64() {
        // range first: an out-of-range id is refused by the network whatever its precision
        if v > max {
            return Err("out-of-range");
        }
        if v > MAX_SAFE_INTEGER {
            return Err(UNSAFE_INTEGER);
        }
        return Ok(v);
    }
    if n.as_i64().is_some() {
        return Err("negative");
    }
    Err("not-an-integer")
}

fn expected_ok(kind: IdKind, id: u64) -> Result<Value> {
    Ok(match kind {
        IdKind::Muxed => {
            let m = stellar_strkey::ed25519::MuxedAccount {
                ed25519: key(0x3a),
                id,
            };
            let xdr = x::MuxedAccount::MuxedEd25519(x::MuxedAccountMed25519 {
                id,
                ed25519: x::Uint256(key(0x3a)),
            });
            json!({
                "id": id.to_string(),
                "muxed_address": m.to_string().to_string(),
                "muxed_account": repr(&xdr)?,
            })
        }
        IdKind::Offer => {
            let op = x::ManageSellOfferOp {
                selling: x::Asset::Native,
                buying: x::Asset::CreditAlphanum4(x::AlphaNum4 {
                    asset_code: x::AssetCode4(*b"USD\0"),
                    issuer: x::AccountId(x::PublicKey::PublicKeyTypeEd25519(x::Uint256(key(3)))),
                }),
                amount: 10,
                price: x::Price { n: 1, d: 1 },
                offer_id: i64::try_from(id)?,
            };
            json!({ "id": id.to_string(), "manage_sell_offer": repr(&op)? })
        }
    })
}

fn text_inputs(kind: IdKind) -> Vec<&'static str> {
    let mut v = vec![
        "0",
        "1",
        "16",
        "9007199254740991",
        "9007199254740992",
        "9007199254740993",
        "9223372036854775807",
        "9223372036854775808",
        "18446744073709551615",
        "18446744073709551616",
        "0x10",
        "0X10",
        "0o20",
        "0b10000",
        "016",
        "00",
        " 16 ",
        "16 ",
        "\t16",
        "16\n",
        "",
        "+16",
        "-1",
        "-0",
        "16.0",
        "1e1",
        "1_6",
        "\u{ff11}\u{ff16}",
        "99999999999999999999999",
    ];
    if kind == IdKind::Offer {
        v.push("-16");
        v.push("-9223372036854775808");
    }
    v
}

fn number_inputs() -> Vec<Number> {
    vec![
        Number::from(0u64),
        Number::from(16u64),
        Number::from(MAX_SAFE_INTEGER),
        Number::from(MAX_SAFE_INTEGER + 1),
        Number::from(MAX_SAFE_INTEGER + 2),
        Number::from(i64::MAX as u64),
        Number::from(u64::MAX),
        Number::from(-1i64),
        Number::from(-16i64),
        Number::from_f64(16.5).expect("16.5 is finite"),
        Number::from_f64(16.0).expect("16.0 is finite"),
    ]
}

pub fn items() -> Result<Vec<VectorItem>> {
    let mut out = Vec::new();
    for kind in [IdKind::Muxed, IdKind::Offer] {
        let mut accept = Vec::new();
        let mut reject = Vec::new();
        let mut unsafe_numbers = Vec::new();
        for text in text_inputs(kind) {
            let case = |expected: Value| {
                json!({
                    "op": "id_parse",
                    "kind": kind.name(),
                    "input": { "text": text },
                    "expected": expected,
                })
            };
            match parse_text(text, kind.max()) {
                Ok(id) => {
                    ensure!(
                        id.to_string() == text,
                        "accepted text {text:?} is not canonical"
                    );
                    accept.push(case(expected_ok(kind, id)?));
                }
                Err(why) => reject.push(case(json!({ "reject": why }))),
            }
        }
        for n in number_inputs() {
            let case = |expected: Value| {
                json!({
                    "op": "id_parse",
                    "kind": kind.name(),
                    "input": { "number": n },
                    "expected": expected,
                })
            };
            match parse_number(&n, kind.max()) {
                Ok(id) => accept.push(case(expected_ok(kind, id)?)),
                Err(UNSAFE_INTEGER) => {
                    let id = n
                        .as_u64()
                        .ok_or_else(|| anyhow::anyhow!("an unsafe integer is a u64"))?;
                    unsafe_numbers.push(case(json!({
                        "either": [expected_ok(kind, id)?, { "reject": UNSAFE_INTEGER }],
                    })));
                }
                Err(why) => reject.push(case(json!({ "reject": why }))),
            }
        }
        // the round trip through the XDR integer type is what makes the decimal canonical
        for c in &accept {
            let id: u64 = c["expected"]["id"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| anyhow::anyhow!("accepted case without a decimal id"))?;
            ensure!(id <= kind.max(), "accepted id {id} is out of range");
        }
        let (family, flags, what): (&str, &[&str], &str) = match kind {
            IdKind::Muxed => (
                "cap27.muxed-accounts",
                &["muxed-account", "non-canonical-id"],
                "muxed account ids (uint64, 0 to 2^64-1). accepted cases carry the M address and MuxedAccount XDR for key 0x3a..3a with that id",
            ),
            IdKind::Offer => (
                "cap03.offer-liabilities",
                &["non-canonical-id"],
                "offer ids (int64 in XDR, 0 to 2^63-1 because core refuses negative ids from protocol 15). accepted cases carry a ManageSellOfferOp with that offer id",
            ),
        };
        let rule = "text is accepted only as the exact decimal rendering of an in-range value (ASCII digits, no sign, whitespace, leading zero, radix prefix, fraction or exponent); a JSON number is accepted as an integer in range with |n| <= 2^53-1, and refused when negative, fractional or out of range. in-range JSON numbers above 2^53-1 are in a separate implementation-defined entry. negative offer ids are refused because core answers MANAGE_SELL_OFFER_MALFORMED / MANAGE_BUY_OFFER_MALFORMED for offerID < 0 from protocol 15 (stellar-core v30.0.0 src/transactions/ManageOfferOpFrameBase.cpp, ManageOfferOpFrameBase::doCheckValid). the reject reason is informational; a testee passes by refusing.";
        let fields: &[&str] = match kind {
            IdKind::Muxed => &["tx.source_account.mux_id", "op.payment.destination.mux_id"],
            IdKind::Offer => &["op.manage_offer.offer_id"],
        };
        out.push(VectorItem {
            spec: Spec::new(
                family,
                format!("textual-{}-id-canonical", kind.name()),
                EntryKind::TextualId,
            )
            .flags(flags)
            .links(&[LINK_JS_1785])
            .fields(fields)
            .notes(format!("canonical {what}. {rule}")),
            cases: accept,
        });
        out.push(VectorItem {
            spec: Spec::new(
                family,
                format!("textual-{}-id-noncanonical", kind.name()),
                EntryKind::TextualId,
            )
            .reject(Validity::NotEmittable)
            .flags(flags)
            .links(&[LINK_JS_1785])
            .fields(fields)
            .notes(format!("non-canonical or out-of-range {what}. {rule}")),
            cases: reject,
        });
        out.push(VectorItem {
            spec: Spec::new(
                family,
                format!("textual-{}-id-unsafe-number", kind.name()),
                EntryKind::TextualId,
            )
            .implementation_defined(Validity::Valid)
            .flags(flags)
            .flags(&["unsafe-json-number"])
            .links(&[LINK_JS_1785, LINK_RFC_8259])
            .fields(fields)
            .notes(format!(
                "in-range {what}, given as JSON numbers above 2^53-1. this bound is not a network rule but a JSON interoperability limit: RFC 8259 section 6 says integers outside [-(2^53)+1, 2^53-1] are not interoperable because implementations commonly hold numbers as IEEE-754 doubles. js-stellar-sdk#1785 is that failure: a number above 2^53-1 lost precision and the operation encoded a different id. each case expects either the exact written id or a refusal ({UNSAFE_INTEGER}); encoding any other id fails."
            )),
            cases: unsafe_numbers,
        });
    }
    Ok(out)
}
