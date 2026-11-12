<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 28.0.0, 28.0.1, 30.0.0: the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 519 (soroban.scval-shapes.synth-event-vec-depth-101)

## What happens

`stellar-xdr` 28.0.0, 28.0.1, 30.0.0 answers 3 of the checks of corpus entry `soroban.scval-shapes.synth-event-vec-depth-101` differently from the Stellar network.

## Minimal vector

Entry `soroban.scval-shapes.synth-event-vec-depth-101` (kind Xdr, family `soroban.scval-shapes`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `any/encode_json` on 28.0.0, 28.0.1, 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 519

```json
{"json":{"body":{"v0":{"data":{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"u32":0}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]},"topics":[{"symbol":"shape"}]}},"contract_id":"CAZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDFJRF","ext":"v0","type":"contract"},"op":"encode_json","type":"ContractEvent"}
```

Expected: `{"xdr":"AAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAAEAAAAAAAAAAQAAAA8AAAAFc2hhcGUAAAAAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQ... (1720 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 519"}`

## Reproduce

```sh
shapekit conformance run --sdk rust --version 28.0.0 --entry soroban.scval-shapes.synth-event-vec-depth-101
```

## Expected value and its source

- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.synth-event-vec-depth-101/any.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.scval-shapes/soroban.scval-shapes.synth-event-vec-depth-101/any.xdr

Entry notes: a contract event whose data nests 101 ScVec levels. every ScVal the host writes passes from_host_val, which fails past DEFAULT_HOST_DEPTH_LIMIT = 100 (scval.depth), so meta never carries this. the bytes decode well inside xdr.meta.depth.

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.scval-shapes.synth-event-vec-depth-101`
