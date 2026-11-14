<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.4.1: the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth (soroban.scval-shapes.p30-map-depth-max-envelope)

## What happens

`@stellar/stellar-sdk` 16.4.1 answers 4 of the checks of corpus entry `soroban.scval-shapes.p30-map-depth-max-envelope` differently from the Stellar network.

## Minimal vector

Entry `soroban.scval-shapes.p30-map-depth-max-envelope` (kind Xdr, family `soroban.scval-shapes`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 2740,
  "op": "decode_encode",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAADHLDKysknb+QKFaygiiDn3jZ//+EEw8KtfoWZkuRrthgCa83gAAABEAAAABQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEIkUFyfn7Zibq3+S/9YLAgqTqv/UUqeNNqCW18RoD5CAAAAARlY2hvAAAAAQAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAAB... (3656 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAADHLDKysknb+QKFaygiiDn3jZ//+EEw8KtfoWZkuRrthgCa83gAAABEAAAABQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEIkUFyfn7Zibq3+S/9YLAgqTqv/UUqeNNqCW18RoD5CAAAAARlY2hvAAAAAQAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAAB... (3656 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `testnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 2740,
  "op": "decode_json",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAADHLDKysknb+QKFaygiiDn3jZ//+EEw8KtfoWZkuRrthgCa83gAAABEAAAABQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEIkUFyfn7Zibq3+S/9YLAgqTqv/UUqeNNqCW18RoD5CAAAAARlY2hvAAAAAQAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAAB... (3656 chars)"
}
```

Expected: `{"json":{"tx":{"signatures":[{"hint":"b91aed86","signature":"ee0a81233067adfe70cd66fd8c3192fb0cfb2a122aceff0378006e9e386ca18dac2108aac55f7cf28b8c381537a954f97378387a4fcbd62436c41ae7bf1c500b"}],"tx":{"cond":"none","ext":{"v1":{"ext":"v0","resource_fee":"10154772","resources":{"disk_read_bytes":0,"footprint":{"read_only":[{"contract_data":{"contract":"CAEJCQLSPZ7NTCN2W74S77LAWAQKSOVP7VCSU6GTNIEW27CGQD4QQWPX","durability":"persistent","key":"ledger_key_contract_instance"}},{"contract_code":{"hash":"5df9b5df3832de0441dd36d086306e1c7d40a1babcfd186c944bcb5cf691ce70"}}],"read_write":[{"contract_data"... (4968 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 2740,
  "op": "decode_encode",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAADHLDKysknb+QKFaygiiDn3jZ//+EEw8KtfoWZkuRrthgCa83gAAABEAAAABQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAHwrg5vmX5ULiu4Ho7p5ZCQMoc9dvLVxmVa6oiVn8LWWgAAAARlY2hvAAAAAQAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAAB... (3656 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAADHLDKysknb+QKFaygiiDn3jZ//+EEw8KtfoWZkuRrthgCa83gAAABEAAAABQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAHwrg5vmX5ULiu4Ho7p5ZCQMoc9dvLVxmVa6oiVn8LWWgAAAARlY2hvAAAAAQAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAAB... (3656 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 2740,
  "op": "decode_json",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAADHLDKysknb+QKFaygiiDn3jZ//+EEw8KtfoWZkuRrthgCa83gAAABEAAAABQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAHwrg5vmX5ULiu4Ho7p5ZCQMoc9dvLVxmVa6oiVn8LWWgAAAARlY2hvAAAAAQAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAAB... (3656 chars)"
}
```

Expected: `{"json":{"tx":{"signatures":[{"hint":"b91aed86","signature":"6b9e0e63c52fce387e7e51960d144218258a9f4d0a0b6381f740d14513e37f2b8590fc10012d585a298942fce896c6e868fa74a9c5bc0392176cec8ef8ac2c08"}],"tx":{"cond":"none","ext":{"v1":{"ext":"v0","resource_fee":"10154772","resources":{"disk_read_bytes":0,"footprint":{"read_only":[{"contract_data":{"contract":"CDYK4DTPTF7FILRLXAPI52PFSCIDFBZ5O3ZNLRTFLLVIRFM7YLLFUUI4","durability":"persistent","key":"ledger_key_contract_instance"}},{"contract_code":{"hash":"5df9b5df3832de0441dd36d086306e1c7d40a1babcfd186c944bcb5cf691ce70"}}],"read_write":[{"contract_data"... (4968 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

## Reproduce

```sh
shapekit conformance run --sdk js --version 16.4.1 --entry soroban.scval-shapes.p30-map-depth-max-envelope
```

## Expected value and its source

- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-envelope/mainnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-envelope/mainnet.xdr: decode = must-accept, roundtrip = true
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-envelope/testnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-envelope/testnet.xdr: decode = must-accept, roundtrip = true

Entry notes: 98 nested maps around a u32 leaf (99 ScVal levels), the deepest the host accepts, echoed into a contract data entry, an event and the return value. simulation cannot size this transaction (it encodes the envelope under xdr depth 500), so its resources are those of the deepest priced level plus a margin

Related: https://github.com/stellar/js-stellar-sdk/pull/1766, https://github.com/stellar/js-stellar-sdk/issues/1789

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.scval-shapes.p30-map-depth-max-envelope`
