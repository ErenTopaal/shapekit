<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.4.1: the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth (soroban.scval-shapes.p30-map-depth-max-meta)

## What happens

On the input below, `@stellar/stellar-sdk` 16.4.1 answers differently from the Stellar network in 4 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `soroban.scval-shapes.p30-map-depth-max-meta` (family `soroban.scval-shapes`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `testnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 9796,
  "op": "decode_encode",
  "type": "TransactionResultMetaV1",
  "xdr": "AAAAAFeT2rsvGvc8yf788AP+7mKkrPEp0+37OBA+slzhs8abAAAAAAACE7YAAAAAAAAAAQAAAAAAAAAYAAAAAFIFB0skdP1uVcmohDPnF6vsomu4lY6IAxySzHRXUhaZAAAAAAAAAAIAAAADAAAASAAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHRumwAAAEQAAAAEAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABIAAAAAGm0pQAAAAAAAAAAAQAAAEkAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0fZeyMAAABEAAAABAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (13064 chars)"
}
```

Expected: `{"xdr":"AAAAAFeT2rsvGvc8yf788AP+7mKkrPEp0+37OBA+slzhs8abAAAAAAACE7YAAAAAAAAAAQAAAAAAAAAYAAAAAFIFB0skdP1uVcmohDPnF6vsomu4lY6IAxySzHRXUhaZAAAAAAAAAAIAAAADAAAASAAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHRumwAAAEQAAAAEAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABIAAAAAGm0pQAAAAAAAAAAAQAAAEkAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0fZeyMAAABEAAAABAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (13064 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `testnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 9796,
  "op": "decode_json",
  "type": "TransactionResultMetaV1",
  "xdr": "AAAAAFeT2rsvGvc8yf788AP+7mKkrPEp0+37OBA+slzhs8abAAAAAAACE7YAAAAAAAAAAQAAAAAAAAAYAAAAAFIFB0skdP1uVcmohDPnF6vsomu4lY6IAxySzHRXUhaZAAAAAAAAAAIAAAADAAAASAAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHRumwAAAEQAAAAEAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABIAAAAAGm0pQAAAAAAAAAAAQAAAEkAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0fZeyMAAABEAAAABAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (13064 chars)"
}
```

Expected: `{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999837851","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":72,"seq_time":"1773446400"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776132","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":72}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5... (17262 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 9796,
  "op": "decode_encode",
  "type": "TransactionResultMetaV1",
  "xdr": "AAAAAMD5+B6vDX2dZp+1TLdCoO2O1RH/S/IOlXkcFbsByHIRAAAAAAACE7YAAAAAAAAAAQAAAAAAAAAYAAAAAM6f+QDXq3LEm8K+XkpZ/u/rwKmnVOwtkRrZcWU8K8yHAAAAAAAAAAIAAAADAAAASAAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHRumwAAAEQAAAAEAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABIAAAAAGm0pQAAAAAAAAAAAQAAAEkAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0fZeyMAAABEAAAABAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (13064 chars)"
}
```

Expected: `{"xdr":"AAAAAMD5+B6vDX2dZp+1TLdCoO2O1RH/S/IOlXkcFbsByHIRAAAAAAACE7YAAAAAAAAAAQAAAAAAAAAYAAAAAM6f+QDXq3LEm8K+XkpZ/u/rwKmnVOwtkRrZcWU8K8yHAAAAAAAAAAIAAAADAAAASAAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHRumwAAAEQAAAAEAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABIAAAAAGm0pQAAAAAAAAAAAQAAAEkAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0fZeyMAAABEAAAABAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (13064 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 9796,
  "op": "decode_json",
  "type": "TransactionResultMetaV1",
  "xdr": "AAAAAMD5+B6vDX2dZp+1TLdCoO2O1RH/S/IOlXkcFbsByHIRAAAAAAACE7YAAAAAAAAAAQAAAAAAAAAYAAAAAM6f+QDXq3LEm8K+XkpZ/u/rwKmnVOwtkRrZcWU8K8yHAAAAAAAAAAIAAAADAAAASAAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHRumwAAAEQAAAAEAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABIAAAAAGm0pQAAAAAAAAAAAQAAAEkAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0fZeyMAAABEAAAABAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (13064 chars)"
}
```

Expected: `{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999837851","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":72,"seq_time":"1773446400"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776132","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":72}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5... (17262 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk js --version 16.4.1 --entry soroban.scval-shapes.p30-map-depth-max-meta
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-meta/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-meta/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-meta/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-meta/mainnet.xdr): decode = must-accept, roundtrip = true
- [`corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-meta/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-meta/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-meta/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-meta/testnet.xdr): decode = must-accept, roundtrip = true

How the corpus entry was built and checked: 98 nested maps around a u32 leaf (99 ScVal levels), the deepest the host accepts, echoed into a contract data entry, an event and the return value. simulation cannot size this transaction (it encodes the envelope under xdr depth 500), so its resources are those of the deepest priced level plus a margin

Related: https://github.com/stellar/js-stellar-sdk/pull/1766, https://github.com/stellar/js-stellar-sdk/issues/1789

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `soroban.scval-shapes.p30-map-depth-max-meta`
