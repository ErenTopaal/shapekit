<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.3.0, 16.3.1, 16.4.1: the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth (soroban.scval-shapes.synth-scmap-depth-197-deepest-accepted)

## What happens

`@stellar/stellar-sdk` 16.3.0, 16.3.1, 16.4.1 answers 6 of the checks of corpus entry `soroban.scval-shapes.synth-scmap-depth-197-deepest-accepted` differently from the Stellar network.

## Minimal vector

Entry `soroban.scval-shapes.synth-scmap-depth-197-deepest-accepted` (kind Xdr, family `soroban.scval-shapes`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `any/decode_encode` on 16.3.0, 16.3.1, 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 4188,
  "op": "decode_encode",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAQAAABEAAAABAAAAAQAAAAMAAAAAAAAAEQAAAAEAAAABAAAAAwAAAAAAAAARAAAAAQAAAAEAAAADAAAAAAAAABEAAAABAAAAAQAAAAMAAAAAAAAAEQAAAAEAAAABAAAAAwAAAAAAAAARAAAAAQAAAAEAAAADAAAAAAAAABEAAAABAAAAAQAAAAMAAAAAAAAAEQAAAAEAAAABAAAAAwAAAAAAAAARAAAAAQAAAAEAAAADAAAAAAAAABEAAAABAAAAAQAAAAMAAAAAAAAAEQAAAAEAAAABAAAAAwAAAAAAAAARAAAAAQAAAAEAAAADAAAAAAAAABEAAAABAAAAAQAAAAMAAAAA... (5584 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAQAAABEAAAABAAAAAQAAAAMAAAAAAAAAEQAAAAEAAAABAAAAAwAAAAAAAAARAAAAAQAAAAEAAAADAAAAAAAAABEAAAABAAAAAQAAAAMAAAAAAAAAEQAAAAEAAAABAAAAAwAAAAAAAAARAAAAAQAAAAEAAAADAAAAAAAAABEAAAABAAAAAQAAAAMAAAAAAAAAEQAAAAEAAAABAAAAAwAAAAAAAAARAAAAAQAAAAEAAAADAAAAAAAAABEAAAABAAAAAQAAAAMAAAAAAAAAEQAAAAEAAAABAAAAAwAAAAAAAAARAAAAAQAAAAEAAAADAAAAAAAAABEAAAABAAAAAQAAAAMAAAAA... (5584 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `any/decode_json` on 16.3.0, 16.3.1, 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 4188,
  "op": "decode_json",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAQAAABEAAAABAAAAAQAAAAMAAAAAAAAAEQAAAAEAAAABAAAAAwAAAAAAAAARAAAAAQAAAAEAAAADAAAAAAAAABEAAAABAAAAAQAAAAMAAAAAAAAAEQAAAAEAAAABAAAAAwAAAAAAAAARAAAAAQAAAAEAAAADAAAAAAAAABEAAAABAAAAAQAAAAMAAAAAAAAAEQAAAAEAAAABAAAAAwAAAAAAAAARAAAAAQAAAAEAAAADAAAAAAAAABEAAAABAAAAAQAAAAMAAAAAAAAAEQAAAAEAAAABAAAAAwAAAAAAAAARAAAAAQAAAAEAAAADAAAAAAAAABEAAAABAAAAAQAAAAMAAAAA... (5584 chars)"
}
```

Expected: `{"json":{"tx":{"signatures":[{"hint":"07070707","signature":"07070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707"}],"tx":{"cond":"none","ext":{"v1":{"ext":"v0","resource_fee":"100000","resources":{"disk_read_bytes":0,"footprint":{"read_only":[],"read_write":[]},"instructions":1000000,"write_bytes":0}}},"fee":100,"memo":"none","operations":[{"body":{"invoke_host_function":{"auth":[],"host_function":{"invoke_contract":{"args":[{"map":[{"key":{"u32":0},"val":{"map":[{"key":{"u32":0},"val":{"map":[{"key":{"u32":0},"val":{... (7446 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

## Reproduce

```sh
shapekit conformance run --sdk js --version 16.3.0 --entry soroban.scval-shapes.synth-scmap-depth-197-deepest-accepted
```

## Expected value and its source

- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.synth-scmap-depth-197-deepest-accepted/any.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.synth-scmap-depth-197-deepest-accepted/any.xdr: decode = must-accept, roundtrip = true

Entry notes: an invoke-contract argument nested 197 ScMap levels deep, at the deepest an envelope decodes at depth 1000. core's depth check accepts it; the host converts at most 100 levels (scval.depth) and runs at most 100 frames, so an invocation this deep fails and the transaction lands in the ledger as failed with the full tree intact. a decoder must accept it.

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.scval-shapes.synth-scmap-depth-197-deepest-accepted`
