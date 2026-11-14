<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.4.1: the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth (soroban.scval-shapes.p30-map-mixed-keys-sorted-meta)

## What happens

On the input below, `@stellar/stellar-sdk` 16.4.1 answers differently from the Stellar network in 4 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `soroban.scval-shapes.p30-map-mixed-keys-sorted-meta` (family `soroban.scval-shapes`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `testnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 5704,
  "op": "decode_encode",
  "type": "TransactionResultMetaV1",
  "xdr": "AAAAAC8fjwRRFKXBV1uYT+OIj07VzZHZNXSQc3vb8kSJ3SeHAAAAAAAAHiYAAAAAAAAAAQAAAAAAAAAYAAAAABo8HAfOKD6/DVYBcO72VVcYPuYuWkg+EwIS3THWH26cAAAAAAAAAAIAAAADAAAASgAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHIuqAAAAEQAAAAGAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABKAAAAAGm3SAAAAAAAAAAAAQAAAEsAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0hx+8gAAABEAAAABgAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (7608 chars)"
}
```

Expected: `{"xdr":"AAAAAC8fjwRRFKXBV1uYT+OIj07VzZHZNXSQc3vb8kSJ3SeHAAAAAAAAHiYAAAAAAAAAAQAAAAAAAAAYAAAAABo8HAfOKD6/DVYBcO72VVcYPuYuWkg+EwIS3THWH26cAAAAAAAAAAIAAAADAAAASgAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHIuqAAAAEQAAAAGAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABKAAAAAGm3SAAAAAAAAAAAAQAAAEsAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0hx+8gAAABEAAAABgAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (7608 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `testnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 5704,
  "op": "decode_json",
  "type": "TransactionResultMetaV1",
  "xdr": "AAAAAC8fjwRRFKXBV1uYT+OIj07VzZHZNXSQc3vb8kSJ3SeHAAAAAAAAHiYAAAAAAAAAAQAAAAAAAAAYAAAAABo8HAfOKD6/DVYBcO72VVcYPuYuWkg+EwIS3THWH26cAAAAAAAAAAIAAAADAAAASgAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHIuqAAAAEQAAAAGAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABKAAAAAGm3SAAAAAAAAAAAAQAAAEsAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0hx+8gAAABEAAAABgAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (7608 chars)"
}
```

Expected: `{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999690408","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":74,"seq_time":"1773619200"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776134","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":74}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5... (11583 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 5704,
  "op": "decode_encode",
  "type": "TransactionResultMetaV1",
  "xdr": "AAAAAOK0a/UEWrKZAGOyMHoPVNHhgoLSPpvlG4uS176w8+XwAAAAAAAAHiYAAAAAAAAAAQAAAAAAAAAYAAAAAObcznpqLdlLejj8Lhh9HpDVQjrWLYPBfI6g89B6kl5sAAAAAAAAAAIAAAADAAAASgAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHIuqAAAAEQAAAAGAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABKAAAAAGm3SAAAAAAAAAAAAQAAAEsAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0hx+8gAAABEAAAABgAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (7608 chars)"
}
```

Expected: `{"xdr":"AAAAAOK0a/UEWrKZAGOyMHoPVNHhgoLSPpvlG4uS176w8+XwAAAAAAAAHiYAAAAAAAAAAQAAAAAAAAAYAAAAAObcznpqLdlLejj8Lhh9HpDVQjrWLYPBfI6g89B6kl5sAAAAAAAAAAIAAAADAAAASgAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHIuqAAAAEQAAAAGAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABKAAAAAGm3SAAAAAAAAAAAAQAAAEsAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0hx+8gAAABEAAAABgAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (7608 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 5704,
  "op": "decode_json",
  "type": "TransactionResultMetaV1",
  "xdr": "AAAAAOK0a/UEWrKZAGOyMHoPVNHhgoLSPpvlG4uS176w8+XwAAAAAAAAHiYAAAAAAAAAAQAAAAAAAAAYAAAAAObcznpqLdlLejj8Lhh9HpDVQjrWLYPBfI6g89B6kl5sAAAAAAAAAAIAAAADAAAASgAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHIuqAAAAEQAAAAGAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABKAAAAAGm3SAAAAAAAAAAAAQAAAEsAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0hx+8gAAABEAAAABgAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (7608 chars)"
}
```

Expected: `{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999690408","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":74,"seq_time":"1773619200"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776134","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":74}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5... (11583 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk js --version 16.4.1 --entry soroban.scval-shapes.p30-map-mixed-keys-sorted-meta
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-mixed-keys-sorted-meta/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-mixed-keys-sorted-meta/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-mixed-keys-sorted-meta/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-mixed-keys-sorted-meta/mainnet.xdr): decode = must-accept, roundtrip = true
- [`corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-mixed-keys-sorted-meta/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-mixed-keys-sorted-meta/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-mixed-keys-sorted-meta/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-mixed-keys-sorted-meta/testnet.xdr): decode = must-accept, roundtrip = true

How the corpus entry was built and checked: a map with one key of each of the 19 ScVal types the host converts, in host order (type discriminant first), returned unchanged

Related: https://github.com/stellar/js-stellar-sdk/pull/1766

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `soroban.scval-shapes.p30-map-mixed-keys-sorted-meta`
