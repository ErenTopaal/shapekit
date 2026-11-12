<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.3.0, 16.3.1, 16.4.1: the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth (soroban.scval-shapes.synth-event-vec-depth-101)

## What happens

`@stellar/stellar-sdk` 16.3.0, 16.3.1, 16.4.1 answers 6 of the checks of corpus entry `soroban.scval-shapes.synth-event-vec-depth-101` differently from the Stellar network.

## Minimal vector

Entry `soroban.scval-shapes.synth-event-vec-depth-101` (kind Xdr, family `soroban.scval-shapes`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `any/decode_encode` on 16.3.0, 16.3.1, 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 1288,
  "op": "decode_encode",
  "type": "ContractEvent",
  "xdr": "AAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAAEAAAAAAAAAAQAAAA8AAAAFc2hhcGUAAAAAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQ... (1720 chars)"
}
```

Expected: `{"xdr":"AAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAAEAAAAAAAAAAQAAAA8AAAAFc2hhcGUAAAAAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQ... (1720 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `any/decode_json` on 16.3.0, 16.3.1, 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 1288,
  "op": "decode_json",
  "type": "ContractEvent",
  "xdr": "AAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAAEAAAAAAAAAAQAAAA8AAAAFc2hhcGUAAAAAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQ... (1720 chars)"
}
```

Expected: `{"json":{"body":{"v0":{"data":{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"... (1185 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

## Reproduce

```sh
shapekit conformance run --sdk js --version 16.3.0 --entry soroban.scval-shapes.synth-event-vec-depth-101
```

## Expected value and its source

- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.synth-event-vec-depth-101/any.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.synth-event-vec-depth-101/any.xdr: decode = must-accept, roundtrip = true

Entry notes: a contract event whose data nests 101 ScVec levels. every ScVal the host writes passes from_host_val, which fails past DEFAULT_HOST_DEPTH_LIMIT = 100 (scval.depth), so meta never carries this. the bytes decode well inside xdr.meta.depth.

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.scval-shapes.synth-event-vec-depth-101`
