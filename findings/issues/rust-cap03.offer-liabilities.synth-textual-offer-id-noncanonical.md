<!-- repo: stellar/rs-stellar-xdr -->
# Question: the XDR-JSON reader accepts 64-bit integer strings with a plus sign, "-0" or leading zeros; is that intended?

## What happens

XDR-JSON writes a 64-bit integer as a decimal string with no sign for positive values and no leading zeros (`"16"`). When reading, the crate also accepts `"+16"`, `"-0"`, `"016"` and `"+016"`, each of which reads as the same value and writes back in the canonical form. So one XDR value has several JSON texts that read to it.

The reader for these fields is `NumberOrString`, which passes a JSON string to `str::parse` ([src/generated.rs, lines 2674-2715](https://github.com/stellar/rs-stellar-xdr/blob/3c2aa43d2ab54d429ae44aa5143d805bf5dba7e2/src/generated.rs#L2674-L2715) at v30.0.0, `3c2aa43`; `src/generated.rs` is unchanged on `main` at `ebec5fb`). Rust's integer `from_str` accepts a leading `+` and leading zeros.

The crate's own tests assert this behaviour, for example [`deserialize_i64_from_json_string_with_plus_prefix`](https://github.com/stellar/rs-stellar-xdr/blob/3c2aa43d2ab54d429ae44aa5143d805bf5dba7e2/src/generated.rs#L3562-L3566), [`deserialize_i64_from_json_string_with_minus_zero`](https://github.com/stellar/rs-stellar-xdr/blob/3c2aa43d2ab54d429ae44aa5143d805bf5dba7e2/src/generated.rs#L3576-L3580) and [`deserialize_i64_from_string_with_leading_zeros`](https://github.com/stellar/rs-stellar-xdr/blob/3c2aa43d2ab54d429ae44aa5143d805bf5dba7e2/src/generated.rs#L3731-L3740), so it looks deliberate. This issue asks whether it is meant to stay.

## Reproduce

```rust
// Cargo.toml: stellar-xdr = { version = "=30.0.0", features = ["std", "serde"] }, serde_json = "1"
use stellar_xdr::ManageSellOfferOp;

fn main() {
    for id in [r#""16""#, r#""+16""#, r#""-0""#, r#""016""#, r#""+016""#] {
        let json = format!(
            r#"{{"selling":"native","buying":"native","amount":"0","price":{{"n":1,"d":1}},"offer_id":{id}}}"#
        );
        let op: ManageSellOfferOp = serde_json::from_str(&json).unwrap();
        let back = serde_json::to_value(&op).unwrap();
        println!("offer_id {id} reads as {}, writes back as {}", op.offer_id, back["offer_id"]);
    }
}
```

Output with stellar-xdr 30.0.0:

```
offer_id "16" reads as 16, writes back as "16"
offer_id "+16" reads as 16, writes back as "16"
offer_id "-0" reads as 0, writes back as "0"
offer_id "016" reads as 16, writes back as "16"
offer_id "+016" reads as 16, writes back as "16"
```

## Reference

SEP-51 maps hyper and unsigned hyper to "JSON strings, with the number base10 encoded as a simple integer" ([sep-0051.md, lines 106-109 and 140-143](https://github.com/stellar/stellar-protocol/blob/ced008c3727005ca4a775f430fe6fe346c30f58f/ecosystem/sep-0051.md#L106-L143)). It does not say whether a reader refuses other spellings of the same number. js-stellar-sdk 17.4.0's XDR-JSON reader (`xdr.ManageSellOfferOp.fromJson`) accepts the same five strings with the same results, so the two readers agree with each other today.

## Why ask

Accepting several spellings means a JSON document that was not produced by an XDR-JSON writer can still read back as a valid value, and two different JSON texts can stand for the same transaction. Unlike the textual id problem fixed in js-stellar-sdk ([#1785](https://github.com/stellar/js-stellar-sdk/issues/1785)), no input here reads as a different number than it spells: every accepted string has the value its digits say. If the leniency is intended, a sentence in the crate's or SEP-51's documentation would settle it; if it is not, refusing a leading `+`, `-0` and leading zeros in `NumberOrString` would make the text canonical.

## Additional reproduction

From a checkout of https://github.com/ErenTopaal/shapekit at tag v0.1.0:

```sh
shapekit conformance run --sdk rust --version 30.0.0 --entry cap03.offer-liabilities.synth-textual-offer-id-noncanonical
```

Cases 12 (`"+16"`) and 14 (`"-0"`) of [corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json) are the inputs above. The run also lists the negative ids of that file (cases 13, 20, 21, 23 and 24). Those are not part of this question: `offerID` is an `int64`, so a negative value is a valid XDR value that the reader has to accept; stellar-core refuses it later, when it checks the operation ([`ManageOfferOpFrameBase::doCheckValid`](https://github.com/stellar/stellar-core/blob/0e49751aa9ddf892302cca02cea7f3f5d97df9f1/src/transactions/ManageOfferOpFrameBase.cpp#L636-L641), `MANAGE_SELL_OFFER_MALFORMED` from protocol 15).

shapekit vector: `cap03.offer-liabilities.synth-textual-offer-id-noncanonical`
