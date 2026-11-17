<!-- repo: Soneso/stellar_flutter_sdk -->
# stellar_flutter_sdk 3.12.0: the sdk's XDR-JSON differs from the expected value (cap03.offer-liabilities.synth-negative-offer-id)

## What happens

On the input below, `stellar_flutter_sdk` 3.12.0 answers differently from the Stellar network in 2 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap03.offer-liabilities.synth-negative-offer-id` (family `cap03.offer-liabilities`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `any/decode_json` on 3.12.0

the sdk's XDR-JSON differs from the expected value

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

the sdk refused a value the network accepts: FormatException: FormatException: XDR-JSON XdrUint64 holds -16, outside the range of a uint64

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

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk dart --version 3.12.0 --entry cap03.offer-liabilities.synth-negative-offer-id
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.synth-negative-offer-id/any.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.synth-negative-offer-id/any.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.synth-negative-offer-id/any.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.synth-negative-offer-id/any.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.synth-negative-offer-id/any.xdr

How the corpus entry was built and checked: a manage sell offer with offerID -16. the XDR type is int64, but from protocol 15 core answers MANAGE_SELL_OFFER_MALFORMED for a negative id (ManageOfferOpFrameBase::doCheckValid).

Related: https://github.com/stellar/js-stellar-sdk/issues/1785

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap03.offer-liabilities.synth-negative-offer-id`
