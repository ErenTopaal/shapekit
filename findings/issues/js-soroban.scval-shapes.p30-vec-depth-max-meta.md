<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.4.1: the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth (soroban.scval-shapes.p30-vec-depth-max-meta)

## What happens

`@stellar/stellar-sdk` 16.4.1 answers 4 of the checks of corpus entry `soroban.scval-shapes.p30-vec-depth-max-meta` differently from the Stellar network.

## Minimal vector

Entry `soroban.scval-shapes.p30-vec-depth-max-meta` (kind Xdr, family `soroban.scval-shapes`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 5088,
  "op": "decode_encode",
  "type": "TransactionResultMetaV1",
  "xdr": "AAAAACazU7yOVDfOspUD9qHSNiLSlEBlupMzxhg1G6VGHfRPAAAAAAACFJcAAAAAAAAAAQAAAAAAAAAYAAAAANu53zfcQUeSGDSRB1fEPAq5TLpMp2GIwealAdu5RjuFAAAAAAAAAAIAAAADAAAARgAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHabdQAAAEQAAAACAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABGAAAAAGmyAgAAAAAAAAAAAQAAAEcAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0h0J7UAAABEAAAAAgAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (6784 chars)"
}
```

Expected: `{"xdr":"AAAAACazU7yOVDfOspUD9qHSNiLSlEBlupMzxhg1G6VGHfRPAAAAAAACFJcAAAAAAAAAAQAAAAAAAAAYAAAAANu53zfcQUeSGDSRB1fEPAq5TLpMp2GIwealAdu5RjuFAAAAAAAAAAIAAAADAAAARgAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHabdQAAAEQAAAACAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABGAAAAAGmyAgAAAAAAAAAAAQAAAEcAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0h0J7UAAABEAAAAAgAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (6784 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `testnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 5088,
  "op": "decode_json",
  "type": "TransactionResultMetaV1",
  "xdr": "AAAAACazU7yOVDfOspUD9qHSNiLSlEBlupMzxhg1G6VGHfRPAAAAAAACFJcAAAAAAAAAAQAAAAAAAAAYAAAAANu53zfcQUeSGDSRB1fEPAq5TLpMp2GIwealAdu5RjuFAAAAAAAAAAIAAAADAAAARgAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHabdQAAAEQAAAACAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABGAAAAAGmyAgAAAAAAAAAAAQAAAEcAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0h0J7UAAABEAAAAAgAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (6784 chars)"
}
```

Expected: `{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999980405","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":70,"seq_time":"1773273600"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776130","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":70}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5... (7724 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 5088,
  "op": "decode_encode",
  "type": "TransactionResultMetaV1",
  "xdr": "AAAAAHn7Z3MZOO4yH7lqBG//3tovC8wOahYHTuBIHWoJbiOtAAAAAAACFJcAAAAAAAAAAQAAAAAAAAAYAAAAAEi1Dv+0WZFxCxAoHq9sayasjIaJoL+BSCZcyLpjxMb5AAAAAAAAAAIAAAADAAAARgAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHabdQAAAEQAAAACAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABGAAAAAGmyAgAAAAAAAAAAAQAAAEcAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0h0J7UAAABEAAAAAgAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (6784 chars)"
}
```

Expected: `{"xdr":"AAAAAHn7Z3MZOO4yH7lqBG//3tovC8wOahYHTuBIHWoJbiOtAAAAAAACFJcAAAAAAAAAAQAAAAAAAAAYAAAAAEi1Dv+0WZFxCxAoHq9sayasjIaJoL+BSCZcyLpjxMb5AAAAAAAAAAIAAAADAAAARgAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHabdQAAAEQAAAACAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABGAAAAAGmyAgAAAAAAAAAAAQAAAEcAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0h0J7UAAABEAAAAAgAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (6784 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 5088,
  "op": "decode_json",
  "type": "TransactionResultMetaV1",
  "xdr": "AAAAAHn7Z3MZOO4yH7lqBG//3tovC8wOahYHTuBIHWoJbiOtAAAAAAACFJcAAAAAAAAAAQAAAAAAAAAYAAAAAEi1Dv+0WZFxCxAoHq9sayasjIaJoL+BSCZcyLpjxMb5AAAAAAAAAAIAAAADAAAARgAAAAAAAAAAxywysrJJ2/kChWsoIog5942f//hBMPCrX6FmZLka7YYAAAAXSHabdQAAAEQAAAACAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABGAAAAAGmyAgAAAAAAAAAAAQAAAEcAAAAAAAAAAMcsMrKySdv5AoVrKCKIOfeNn//4QTDwq1+hZmS5Gu2GAAAAF0h0J7UAAABEAAAAAgAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAA... (6784 chars)"
}
```

Expected: `{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999980405","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":70,"seq_time":"1773273600"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776130","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":70}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5... (7724 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

## Reproduce

```sh
shapekit conformance run --sdk js --version 16.4.1 --entry soroban.scval-shapes.p30-vec-depth-max-meta
```

## Expected value and its source

- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-max-meta/mainnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-max-meta/mainnet.xdr: decode = must-accept, roundtrip = true
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-max-meta/testnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-max-meta/testnet.xdr: decode = must-accept, roundtrip = true

Entry notes: 99 nested vecs around a u32 leaf (100 ScVal levels), the deepest the host accepts, echoed into a contract data entry, an event and the return value

Related: https://github.com/stellar/js-stellar-sdk/pull/1766, https://github.com/stellar/js-stellar-sdk/issues/1789

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.scval-shapes.p30-vec-depth-max-meta`
