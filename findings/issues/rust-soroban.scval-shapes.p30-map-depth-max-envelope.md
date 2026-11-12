<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 30.0.0: the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 2277 (soroban.scval-shapes.p30-map-depth-max-envelope)

## What happens

`stellar-xdr` 30.0.0 answers 2 of the checks of corpus entry `soroban.scval-shapes.p30-map-depth-max-envelope` differently from the Stellar network.

## Minimal vector

Entry `soroban.scval-shapes.p30-map-depth-max-envelope` (kind Xdr, family `soroban.scval-shapes`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 2277

```json
{"json":{"tx":{"signatures":[{"hint":"b91aed86","signature":"c65912ace7cbcdf56be16157ca752f964cc26730dabf246dc914c57d8c4f21d25446793d6e925182c71c557bf77aff51f75bc50bf1c3ece7f13c43b31d367a03"}],"tx":{"cond":"none","ext":{"v1":{"ext":"v0","resource_fee":"10154773","resources":{"disk_read_bytes":0,"footprint":{"read_only":[{"contract_data":{"contract":"CAEJCQLSPZ7NTCN2W74S77LAWAQKSOVP7VCSU6GTNIEW27CGQD4QQWPX","durability":"persistent","key":"ledger_key_contract_instance"}},{"contract_code":{"hash":"f9b16f83ec2b943d5ac11524c410c5e8b6cd13955e15935e99b3923ffa9c8c06"}}],"read_write":[{"contract_data":{"contract":"CAEJCQLSPZ7NTCN2W74S77LAWAQKSOVP7VCSU6GTNIEW27CGQD4QQWPX","durability":"persistent","key":{"symbol":"last"}}}]},"instructions":1936810,"write_bytes":6500}}},"fee":10154873,"memo":"none","operations":[{"body":{"invoke_host_function":{"auth":[],"host_function":{"invoke_contract":{"args":[{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"sy... (5016 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAADHLDKysknb+QKFaygiiDn3jZ//+EEw8KtfoWZkuRrthgCa83kAAABEAAAABQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEIkUFyfn7Zibq3+S/9YLAgqTqv/UUqeNNqCW18RoD5CAAAAARlY2hvAAAAAQAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAAB... (3656 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 2277"}`

### `mainnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 2277

```json
{"json":{"tx":{"signatures":[{"hint":"b91aed86","signature":"f9a86e5cd8ab816c5ed8e63b58d5469de92b51f11e3164914872c44e56f620dc9b92a3eebf4037b40d4a4751177d09383216fa762d6ff5a05babf42bf464a30d"}],"tx":{"cond":"none","ext":{"v1":{"ext":"v0","resource_fee":"10154773","resources":{"disk_read_bytes":0,"footprint":{"read_only":[{"contract_data":{"contract":"CDYK4DTPTF7FILRLXAPI52PFSCIDFBZ5O3ZNLRTFLLVIRFM7YLLFUUI4","durability":"persistent","key":"ledger_key_contract_instance"}},{"contract_code":{"hash":"f9b16f83ec2b943d5ac11524c410c5e8b6cd13955e15935e99b3923ffa9c8c06"}}],"read_write":[{"contract_data":{"contract":"CDYK4DTPTF7FILRLXAPI52PFSCIDFBZ5O3ZNLRTFLLVIRFM7YLLFUUI4","durability":"persistent","key":{"symbol":"last"}}}]},"instructions":1936810,"write_bytes":6500}}},"fee":10154873,"memo":"none","operations":[{"body":{"invoke_host_function":{"auth":[],"host_function":{"invoke_contract":{"args":[{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"symbol":"v"},"val":{"map":[{"key":{"sy... (5016 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAADHLDKysknb+QKFaygiiDn3jZ//+EEw8KtfoWZkuRrthgCa83kAAABEAAAABQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAHwrg5vmX5ULiu4Ho7p5ZCQMoc9dvLVxmVa6oiVn8LWWgAAAARlY2hvAAAAAQAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAABdgAAAAAAABEAAAABAAAAAQAAAA8AAAAB... (3656 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 2277"}`

## Reproduce

```sh
shapekit conformance run --sdk rust --version 30.0.0 --entry soroban.scval-shapes.p30-map-depth-max-envelope
```

## Expected value and its source

- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-envelope/mainnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-envelope/mainnet.xdr
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-envelope/testnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-max-envelope/testnet.xdr

Entry notes: 98 nested maps around a u32 leaf (99 ScVal levels), the deepest the host accepts, echoed into a contract data entry, an event and the return value. simulation cannot size this transaction (it encodes the envelope under xdr depth 500), so its resources are those of the deepest priced level plus a margin

Related: https://github.com/stellar/js-stellar-sdk/pull/1766, https://github.com/stellar/js-stellar-sdk/issues/1789

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.scval-shapes.p30-map-depth-max-envelope`
