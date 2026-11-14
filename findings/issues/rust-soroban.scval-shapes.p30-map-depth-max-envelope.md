<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 30.0.0: the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 2277 (soroban.scval-shapes.p30-map-depth-max-envelope)

## What happens

On the input below, `stellar-xdr` 30.0.0 answers differently from the Stellar network in 2 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `soroban.scval-shapes.p30-map-depth-max-envelope` (family `soroban.scval-shapes`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `testnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 2277

```json
{"json":{"tx":{"signatures":[{"hint":"b91aed86","signature":"ee0a81233067adfe70cd66fd8c3192fb0cfb2a122aceff0378006e9e386ca18dac2108aac55f7cf28b8c381537a954f97378387a4fcbd62436c41ae7bf1c500b"}],"tx":{"cond":"none","ext":{"v1":{"ext":"v0","resource_fee":"10154772","resources":{"disk_read_bytes":0,"footprint":{"read_only":[{"contract_data":{"contract":"CAEJCQLSPZ7NTCN2W74S77LAWAQKSOVP7VCSU6GTNIEW27CGQD4QQWPX","durability":"persistent","key":"ledger_key_contract_instance"}},{"contract_code":{"hash":"5df9b5df3832de0441dd36d086306e1c7d40a1babcfd186c944bcb5cf691ce70"}}],"read_write":[{"contract_data":{"contract":"CAEJCQLSPZ7NTCN2W74S77LAWAQKSOVP7VCSU6GTNIEW27CGQD4QQWPX","durability":"persistent","key":{"symbol":"last"}}}]},"instructions":1933830,"write_bytes":6500}}},"fee":10154872,"memo":"none","operations":[{"body":{"invoke_host_function":{"auth":[],"host_function":{"invoke_contract":{"args":[{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"sy... (5016 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAADHLDKysknb+QKFaygiiDn3jZ//+EEw8KtfoWZkuRrthgCa83gAAABEAAAABQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEIkUFyfn7Zibq3+S/9YLAgqTqv/UUqeNNqCW18RoD5CAAAAARlY2hvAAAAAQAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAAB... (3656 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 2277"}`

### `mainnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 2277

```json
{"json":{"tx":{"signatures":[{"hint":"b91aed86","signature":"6b9e0e63c52fce387e7e51960d144218258a9f4d0a0b6381f740d14513e37f2b8590fc10012d585a298942fce896c6e868fa74a9c5bc0392176cec8ef8ac2c08"}],"tx":{"cond":"none","ext":{"v1":{"ext":"v0","resource_fee":"10154772","resources":{"disk_read_bytes":0,"footprint":{"read_only":[{"contract_data":{"contract":"CDYK4DTPTF7FILRLXAPI52PFSCIDFBZ5O3ZNLRTFLLVIRFM7YLLFUUI4","durability":"persistent","key":"ledger_key_contract_instance"}},{"contract_code":{"hash":"5df9b5df3832de0441dd36d086306e1c7d40a1babcfd186c944bcb5cf691ce70"}}],"read_write":[{"contract_data":{"contract":"CDYK4DTPTF7FILRLXAPI52PFSCIDFBZ5O3ZNLRTFLLVIRFM7YLLFUUI4","durability":"persistent","key":{"symbol":"last"}}}]},"instructions":1933830,"write_bytes":6500}}},"fee":10154872,"memo":"none","operations":[{"body":{"invoke_host_function":{"auth":[],"host_function":{"invoke_contract":{"args":[{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"sy... (5016 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAADHLDKysknb+QKFaygiiDn3jZ//+EEw8KtfoWZkuRrthgCa83gAAABEAAAABQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAHwrg5vmX5ULiu4Ho7p5ZCQMoc9dvLVxmVa6oiVn8LWWgAAAARlY2hvAAAAAQAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAAB... (3656 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 2277"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk rust --version 30.0.0 --entry soroban.scval-shapes.p30-map-depth-max-envelope
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-envelope/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-envelope/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-envelope/mainnet.xdr
- [`corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-envelope/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-envelope/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-envelope/testnet.xdr

How the corpus entry was built and checked: 98 nested maps around a u32 leaf (99 ScVal levels), the deepest the host accepts, echoed into a contract data entry, an event and the return value. simulation cannot size this transaction (it encodes the envelope under xdr depth 500), so its resources are those of the deepest priced level plus a margin

Related: https://github.com/stellar/js-stellar-sdk/pull/1766, https://github.com/stellar/js-stellar-sdk/issues/1789

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `soroban.scval-shapes.p30-map-depth-max-envelope`
