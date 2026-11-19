<!-- repo: Soneso/stellar_flutter_sdk -->
# stellar_flutter_sdk 3.12.0: question: offer ids are generated as XdrUint64, so the XDR-JSON of a negative offer id is another number and the canonical form does not encode

## What happens

Question: offer ids are generated as XdrUint64, so the XDR-JSON of a negative offer id is another number and the canonical form does not encode.

## Cause

The generator override tools/xdr-generator/generator/field_overrides.rb L76-L85 ("offer IDs are always positive") makes the int64 offer id an XdrUint64 in XdrManageSellOfferOp (lib/src/xdr/xdr_manage_sell_offer_op.dart L34 at 3.12.0), XdrManageBuyOfferOp, XdrOfferEntry, XdrLedgerKeyOffer, XdrClaimOfferAtom and XdrClaimOfferAtomV0. The bytes round trip, but XDR-JSON renders offer id -16 as "18446744073709551600" and refuses the canonical "-16" ("XdrUint64 holds -16, outside the range of a uint64"). A negative offer id reaches a ledger only in operations that failed before protocol 15.

## Affected versions

`stellar_flutter_sdk` 3.12.0, every version shapekit ran.

## Entries

shapekit's finding DART-OFFERU64 takes 2 corpus entries on this SDK: `cap03.offer-liabilities.synth-negative-offer-id`, `explore.witnesses.f62d-r0004-l69`. The checks below are those of `cap03.offer-liabilities.synth-negative-offer-id`.

## Checks on `cap03.offer-liabilities.synth-negative-offer-id`

On this entry, `stellar_flutter_sdk` 3.12.0 answers differently from the Stellar network in 2 checks on 1 version.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

Corpus entry `cap03.offer-liabilities.synth-negative-offer-id` (family `cap03.offer-liabilities`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `any/decode_json` on 3.12.0

The SDK's XDR-JSON differs from the expected value

```json
{
  "depth_limit": 1000,
  "len_limit": 224,
  "op": "decode_json",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAADAAAAAAAAAAFVU0QAAAAAAAMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAAAAAAAAAAoAAAABAAAAAf/////////wAAAAAAAAAAEHBwcHAAAAQAcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc="
}
```

Expected: `{"json":{"tx":{"signatures":[{"hint":"07070707","signature":"07070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707"}],"tx":{"cond":"none","ext":"v0","fee":100,"memo":"none","operations":[{"body":{"manage_sell_offer":{"amount":"10","buying":{"credit_alphanum4":{"asset_code":"USD","issuer":"GABQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQHGPC"}},"offer_id":"-16","price":{"d":1,"n":1},"selling":"native"}},"source_account":null}],"seq_num":"4294967297","source_account":"GAAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAE... (611 chars, see the source below)`

Actual: `{"json":{"tx":{"signatures":[{"hint":"07070707","signature":"07070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707"}],"tx":{"cond":"none","ext":"v0","fee":100,"memo":"none","operations":[{"body":{"manage_sell_offer":{"amount":"10","buying":{"credit_alphanum4":{"asset_code":"USD","issuer":"GABQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQHGPC"}},"offer_id":"18446744073709551600","price":{"d":1,"n":1},"selling":"native"}},"source_account":null}],"seq_num":"4294967297","source_account":"GAAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBA... (628 chars, see the source below)`

### `any/encode_json` on 3.12.0

The SDK refused a value the network accepts: FormatException: FormatException: XDR-JSON XdrUint64 holds -16, outside the range of a uint64

```json
{
  "json": {
    "tx": {
      "signatures": [
        {
          "hint": "07070707",
          "signature": "07070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707"
        }
      ],
      "tx": {
        "cond": "none",
        "ext": "v0",
        "fee": 100,
        "memo": "none",
        "operations": [
          {
            "body": {
              "manage_sell_offer": {
                "amount": "10",
                "buying": {
                  "credit_alphanum4": {
                    "asset_code": "USD",
                    "issuer": "GABQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQHGPC"
                  }
                },
                "offer_id": "-16",
                "price": {
                  "d": 1,
                  "n": 1
                },
                "selling": "native"
              }
            },
            "source_account": null
          }
        ],
        "seq_num": "4294967297",
        "source_account": "GAAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQDZ7H"
      }
    }
  },
  "op": "encode_json",
  "type": "TransactionEnvelope"
}
```

Expected: `{"xdr":"AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAADAAAAAAAAAAFVU0QAAAAAAAMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAAAAAAAAAAoAAAABAAAAAf/////////wAAAAAAAAAAEHBwcHAAAAQAcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc="}`

Actual: `{"rejected":"FormatException: FormatException: XDR-JSON XdrUint64 holds -16, outside the range of a uint64"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`, with the Rust toolchain of its `rust-toolchain.toml` and the toolchain `stellar_flutter_sdk` builds with (`shapekit doctor sdk --sdk dart` lists what is missing):

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit conformance run --sdk dart --version 3.12.0 --entry cap03.offer-liabilities.synth-negative-offer-id
```

The run builds the SDK's testee at that version as `conformance/versions.toml` describes, then runs the entry's checks.

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.synth-negative-offer-id/any.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.synth-negative-offer-id/any.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.synth-negative-offer-id/any.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.synth-negative-offer-id/any.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.synth-negative-offer-id/any.xdr

How the corpus entry was built and checked: A manage sell offer with offerID -16. the XDR type is int64, but from protocol 15 core answers MANAGE_SELL_OFFER_MALFORMED for a negative id (ManageOfferOpFrameBase::doCheckValid).

Related: https://github.com/stellar/js-stellar-sdk/issues/1785

## Sibling issues

- `network.lightsail:stellar-sdk` (lightsail-network/java-stellar-sdk) diverges on `explore.witnesses.f62d-r0004-l69`, one of these entries: no finding takes it yet.
- `com.soneso.stellar:stellar-sdk-jvm` (Soneso/kmp-stellar-sdk) diverges on `explore.witnesses.f62d-r0004-l69`, one of these entries: shapekit's finding KMP-TXOBJECT, the transaction object refuses values the network applied: max time 2^64-1 and offer id -1 (filed as https://github.com/Soneso/kmp-stellar-sdk/issues/171).
- `soneso/stellar-php-sdk` (Soneso/stellar-php-sdk) diverges on `explore.witnesses.f62d-r0004-l69`, one of these entries: shapekit's finding PHP-OFFERID, rebuilding a transaction with a manage sell offer of offer id -1 throws (not filed).
- `stellar-sdk` (StellarCN/py-stellar-base) diverges on `explore.witnesses.f62d-r0004-l69`, one of these entries: no finding takes it yet.
- `stellar-cli` (stellar/stellar-cli) diverges on `explore.witnesses.f62d-r0004-l69`, one of these entries: shapekit's finding CLI-TXHASH-V1, stellar tx hash refuses v0 and fee bump envelopes the network applied (covered upstream by https://github.com/stellar/stellar-cli/issues/2769).

shapekit vector: `cap03.offer-liabilities.synth-negative-offer-id`
