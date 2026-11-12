<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.4.1: the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth (soroban.scval-shapes.p30-map-depth-max-meta)

## What happens

`@stellar/stellar-sdk` 16.4.1 answers 4 of the checks of corpus entry `soroban.scval-shapes.p30-map-depth-max-meta` differently from the Stellar network.

## Minimal vector

Entry `soroban.scval-shapes.p30-map-depth-max-meta` (kind Xdr, family `soroban.scval-shapes`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 9796,
  "op": "decode_encode",
  "type": "TransactionResultMetaV1",
  "xdr": "AAAAABGVHnwWUOIbIDDAxhKlzoFjv7vA+gIr0l2W1LqsFDLpAAAAAAACE7cAAAAAAAAAAQAAAAAAAAAYAAAAAFIFB0skdP1uVcmohDPnF6vsomu4lY6IAxySzHRXUhaZAAAAAAAAAAIAAAADAAAASAAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHRPMAAAAEQAAAAEAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABIAAAAAGm0pQAAAAAAAAAAAQAAAEkAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0fZW7cAAABEAAAABAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (13064 chars)"
}
```

Expected: `{"xdr":"AAAAABGVHnwWUOIbIDDAxhKlzoFjv7vA+gIr0l2W1LqsFDLpAAAAAAACE7cAAAAAAAAAAQAAAAAAAAAYAAAAAFIFB0skdP1uVcmohDPnF6vsomu4lY6IAxySzHRXUhaZAAAAAAAAAAIAAAADAAAASAAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHRPMAAAAEQAAAAEAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABIAAAAAGm0pQAAAAAAAAAAAQAAAEkAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0fZW7cAAABEAAAABAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (13064 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `testnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 9796,
  "op": "decode_json",
  "type": "TransactionResultMetaV1",
  "xdr": "AAAAABGVHnwWUOIbIDDAxhKlzoFjv7vA+gIr0l2W1LqsFDLpAAAAAAACE7cAAAAAAAAAAQAAAAAAAAAYAAAAAFIFB0skdP1uVcmohDPnF6vsomu4lY6IAxySzHRXUhaZAAAAAAAAAAIAAAADAAAASAAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHRPMAAAAEQAAAAEAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABIAAAAAGm0pQAAAAAAAAAAAQAAAEkAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0fZW7cAAABEAAAABAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (13064 chars)"
}
```

Expected: `{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999829808","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":72,"seq_time":"1773446400"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776132","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":72}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5... (17262 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 9796,
  "op": "decode_encode",
  "type": "TransactionResultMetaV1",
  "xdr": "AAAAANyh/t/jl21GMrrklhaN1HpwfQCRTTDrPyHmTB+PqdKIAAAAAAACE7cAAAAAAAAAAQAAAAAAAAAYAAAAAM6f+QDXq3LEm8K+XkpZ/u/rwKmnVOwtkRrZcWU8K8yHAAAAAAAAAAIAAAADAAAASAAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHRPMAAAAEQAAAAEAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABIAAAAAGm0pQAAAAAAAAAAAQAAAEkAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0fZW7cAAABEAAAABAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (13064 chars)"
}
```

Expected: `{"xdr":"AAAAANyh/t/jl21GMrrklhaN1HpwfQCRTTDrPyHmTB+PqdKIAAAAAAACE7cAAAAAAAAAAQAAAAAAAAAYAAAAAM6f+QDXq3LEm8K+XkpZ/u/rwKmnVOwtkRrZcWU8K8yHAAAAAAAAAAIAAAADAAAASAAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHRPMAAAAEQAAAAEAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABIAAAAAGm0pQAAAAAAAAAAAQAAAEkAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0fZW7cAAABEAAAABAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (13064 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 9796,
  "op": "decode_json",
  "type": "TransactionResultMetaV1",
  "xdr": "AAAAANyh/t/jl21GMrrklhaN1HpwfQCRTTDrPyHmTB+PqdKIAAAAAAACE7cAAAAAAAAAAQAAAAAAAAAYAAAAAM6f+QDXq3LEm8K+XkpZ/u/rwKmnVOwtkRrZcWU8K8yHAAAAAAAAAAIAAAADAAAASAAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHRPMAAAAEQAAAAEAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABIAAAAAGm0pQAAAAAAAAAAAQAAAEkAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0fZW7cAAABEAAAABAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (13064 chars)"
}
```

Expected: `{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999829808","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":72,"seq_time":"1773446400"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776132","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":72}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5... (17262 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

## Reproduce

```sh
shapekit conformance run --sdk js --version 16.4.1 --entry soroban.scval-shapes.p30-map-depth-max-meta
```

## Expected value and its source

- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-meta/mainnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-meta/mainnet.xdr: decode = must-accept, roundtrip = true
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-meta/testnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-meta/testnet.xdr: decode = must-accept, roundtrip = true

Entry notes: 98 nested maps around a u32 leaf (99 ScVal levels), the deepest the host accepts, echoed into a contract data entry, an event and the return value. simulation cannot size this transaction (it encodes the envelope under xdr depth 500), so its resources are those of the deepest priced level plus a margin

Related: https://github.com/stellar/js-stellar-sdk/pull/1766, https://github.com/stellar/js-stellar-sdk/issues/1789

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.scval-shapes.p30-map-depth-max-meta`
