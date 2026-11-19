<!-- repo: Soneso/stellar_flutter_sdk -->
# stellar_flutter_sdk 3.12.0: ManageSellOfferOperationBuilder.setOfferId reads hex, signed, padded and out-of-range text as an offer id

## What happens

ManageSellOfferOperationBuilder.setOfferId reads hex, signed, padded and out-of-range text as an offer id.

## Cause

ToOperationBody passes the offer id text to BigInt.parse (lib/src/manage_sell_offer_operation.dart L111), which takes a sign, surrounding whitespace and a 0x prefix, and keeps any size, so "0x10", " 16 ", "+16" and "-0" read as 16 or 0, and 9223372036854775808 and 18446744073709551615 are written as the int64 -9223372036854775808 and -1. The text forms are the ones RS-1 (rs-stellar-xdr#627), GO-TEXTID and js-stellar-sdk#1907 accept; the wrap to another id is js-stellar-sdk#1785's. Separate code in each sdk.

## Affected versions

`stellar_flutter_sdk` 3.12.0, every version shapekit ran.

## Entries

shapekit's finding DART-OFFERTEXT takes 1 corpus entry on this SDK: `cap03.offer-liabilities.synth-textual-offer-id-noncanonical`. The checks below are those of `cap03.offer-liabilities.synth-textual-offer-id-noncanonical`.

## Checks on `cap03.offer-liabilities.synth-textual-offer-id-noncanonical`

On this entry, `stellar_flutter_sdk` 3.12.0 answers differently from the Stellar network in 10 checks on 1 version.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

Corpus entry `cap03.offer-liabilities.synth-textual-offer-id-noncanonical` (family `cap03.offer-liabilities`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `case 0/id_parse` on 3.12.0

The SDK accepted a value the network refuses

```json
{
  "input": "9223372036854775808",
  "kind": "offer",
  "op": "id_parse"
}
```

Expected: `"rejected"`

Actual: `{"id":"-9223372036854775808"}`

### `case 1/id_parse` on 3.12.0

The SDK accepted a value the network refuses

```json
{
  "input": "18446744073709551615",
  "kind": "offer",
  "op": "id_parse"
}
```

Expected: `"rejected"`

Actual: `{"id":"-1"}`

### `case 3/id_parse` on 3.12.0

The SDK accepted a value the network refuses

```json
{
  "input": "0x10",
  "kind": "offer",
  "op": "id_parse"
}
```

Expected: `"rejected"`

Actual: `{"id":"16"}`

### `case 4/id_parse` on 3.12.0

The SDK accepted a value the network refuses

```json
{
  "input": "0X10",
  "kind": "offer",
  "op": "id_parse"
}
```

Expected: `"rejected"`

Actual: `{"id":"16"}`

### `case 7/id_parse` on 3.12.0

The SDK accepted a value the network refuses

```json
{
  "input": " 16 ",
  "kind": "offer",
  "op": "id_parse"
}
```

Expected: `"rejected"`

Actual: `{"id":"16"}`

5 more checks fail; the command below lists every one.

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`, with the Rust toolchain of its `rust-toolchain.toml` and the toolchain `stellar_flutter_sdk` builds with (`shapekit doctor sdk --sdk dart` lists what is missing):

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit conformance run --sdk dart --version 3.12.0 --entry cap03.offer-liabilities.synth-textual-offer-id-noncanonical
```

The run builds the SDK's testee at that version as `conformance/versions.toml` describes, then runs the entry's checks.

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json) cases 0, 1, 3, 4, 7, 8, 9, 10, 12, 13

How the corpus entry was built and checked: Non-canonical or out-of-range offer ids (int64 in XDR; a valid operation carries 0 to 2^63-1 because core refuses negative ids from protocol 15). accepted cases carry a ManageSellOfferOp with that offer id. text is canonical only as the exact decimal rendering of an in-range value, and refused with a plus sign, a minus sign on zero (-0), whitespace, a radix prefix, an exponent, non-ASCII digits, when empty or out of range; a JSON number is canonical as an integer in range with |n| <= 2^53-1, and refused when non-integral or out of range. leading-zero text, integral numbers written with .0 and in-range numbers above 2^53-1 are in a separate implementation-defined entry. negative offer ids in their canonical int64 spelling (-16 as text or as a JSON number) are in a separate entry, because XDR and SEP-51 XDR-JSON allow them and core refuses them only when it checks the operation. the reject reason is informational; a testee passes by refusing.

Related: https://github.com/stellar/js-stellar-sdk/issues/1785

## Sibling issues

- `github.com/stellar/go-stellar-sdk` (stellar/go-stellar-sdk) diverges on the same entry: shapekit's finding GO-TEXTID, question: horizon.Offer reads an offer id given as "+16" or "-0" (not filed).
- `network.lightsail:stellar-sdk` (lightsail-network/java-stellar-sdk) diverges on the same entry: no finding takes it yet.
- `@stellar/stellar-sdk` (stellar/js-stellar-sdk) diverges on the same entry: shapekit's finding JS-16X, 16.x lacks six fixes that 17.x has: map key order and collisions, textual ids, W addresses, decode depth, StrKey checks (filed as https://github.com/stellar/js-stellar-sdk/issues/1907); JS-1785, textual muxed and offer ids accept non-canonical forms that encode another id (covered upstream by https://github.com/stellar/js-stellar-sdk/issues/1785).
- `stellar-sdk` (StellarCN/py-stellar-base) diverges on the same entry: no finding takes it yet.
- `stellar-xdr` (stellar/rs-stellar-xdr) diverges on the same entry: shapekit's finding RS-1, question: the XDR-JSON reader accepts 64-bit integer strings with a plus sign, "-0" or leading zeros (not filed).

shapekit vector: `cap03.offer-liabilities.synth-textual-offer-id-noncanonical`
