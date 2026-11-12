<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 30.0.0: the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 1360 (soroban.scval-shapes.p30-vec-depth-max-envelope)

## What happens

`stellar-xdr` 30.0.0 answers 2 of the checks of corpus entry `soroban.scval-shapes.p30-vec-depth-max-envelope` differently from the Stellar network.

## Minimal vector

Entry `soroban.scval-shapes.p30-vec-depth-max-envelope` (kind Xdr, family `soroban.scval-shapes`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 1360

```json
{"json":{"tx":{"signatures":[{"hint":"b91aed86","signature":"29050ba690c81dc8f73d2230c22780551d9c0901dd726a287f0e8a2338f616c1868eed4f65a5c1f1af18bd9e5b42c919892a5e6f63e27f3093b64807a1906b04"}],"tx":{"cond":"none","ext":{"v1":{"ext":"v0","resource_fee":"160604","resources":{"disk_read_bytes":0,"footprint":{"read_only":[{"contract_data":{"contract":"CAEJCQLSPZ7NTCN2W74S77LAWAQKSOVP7VCSU6GTNIEW27CGQD4QQWPX","durability":"persistent","key":"ledger_key_contract_instance"}},{"contract_code":{"hash":"f9b16f83ec2b943d5ac11524c410c5e8b6cd13955e15935e99b3923ffa9c8c06"}}],"read_write":[{"contract_data":{"contract":"CAEJCQLSPZ7NTCN2W74S77LAWAQKSOVP7VCSU6GTNIEW27CGQD4QQWPX","durability":"persistent","key":{"symbol":"last"}}}]},"instructions":882426,"write_bytes":1264}}},"fee":160704,"memo":"none","operations":[{"body":{"invoke_host_function":{"auth":[],"host_function":{"invoke_contract":{"args":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"u32":99}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}],"contract_address":"CAEJCQLSPZ7NTCN2W74S77LAWAQKSOVP7VCSU6GTNIEW27CGQD4QQWPX","function_name":"echo"}}... (2179 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAADHLDKysknb+QKFaygiiDn3jZ//+EEw8KtfoWZkuRrthgACc8AAAABEAAAAAwAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEIkUFyfn7Zibq3+S/9YLAgqTqv/UUqeNNqCW18RoD5CAAAAARlY2hvAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAAB... (2104 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 1360"}`

### `mainnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 1360

```json
{"json":{"tx":{"signatures":[{"hint":"b91aed86","signature":"2da284ed4b5e5dba629c7146c86445a3d1105720d3c35ec71d59f2f2c2b4e204eaf2c1f23ea88bade8663eaefccf54f47d8f0afadb9b837cf07339a6cbba3108"}],"tx":{"cond":"none","ext":{"v1":{"ext":"v0","resource_fee":"160604","resources":{"disk_read_bytes":0,"footprint":{"read_only":[{"contract_data":{"contract":"CDYK4DTPTF7FILRLXAPI52PFSCIDFBZ5O3ZNLRTFLLVIRFM7YLLFUUI4","durability":"persistent","key":"ledger_key_contract_instance"}},{"contract_code":{"hash":"f9b16f83ec2b943d5ac11524c410c5e8b6cd13955e15935e99b3923ffa9c8c06"}}],"read_write":[{"contract_data":{"contract":"CDYK4DTPTF7FILRLXAPI52PFSCIDFBZ5O3ZNLRTFLLVIRFM7YLLFUUI4","durability":"persistent","key":{"symbol":"last"}}}]},"instructions":882426,"write_bytes":1264}}},"fee":160704,"memo":"none","operations":[{"body":{"invoke_host_function":{"auth":[],"host_function":{"invoke_contract":{"args":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"u32":99}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}],"contract_address":"CDYK4DTPTF7FILRLXAPI52PFSCIDFBZ5O3ZNLRTFLLVIRFM7YLLFUUI4","function_name":"echo"}}... (2179 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAADHLDKysknb+QKFaygiiDn3jZ//+EEw8KtfoWZkuRrthgACc8AAAABEAAAAAwAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAHwrg5vmX5ULiu4Ho7p5ZCQMoc9dvLVxmVa6oiVn8LWWgAAAARlY2hvAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAAB... (2104 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 1360"}`

## Reproduce

```sh
shapekit conformance run --sdk rust --version 30.0.0 --entry soroban.scval-shapes.p30-vec-depth-max-envelope
```

## Expected value and its source

- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-max-envelope/mainnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-max-envelope/mainnet.xdr
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-max-envelope/testnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-max-envelope/testnet.xdr

Entry notes: 99 nested vecs around a u32 leaf (100 ScVal levels), the deepest the host accepts, echoed into a contract data entry, an event and the return value

Related: https://github.com/stellar/js-stellar-sdk/pull/1766, https://github.com/stellar/js-stellar-sdk/issues/1789

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.scval-shapes.p30-vec-depth-max-envelope`
