<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 28.0.0, 28.0.1, 30.0.0: the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 519 (soroban.scval-shapes.synth-event-vec-depth-101)

## What happens

On the input below, `stellar-xdr` 28.0.0, 28.0.1, 30.0.0 answers differently from the Stellar network in 3 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `soroban.scval-shapes.synth-event-vec-depth-101` (family `soroban.scval-shapes`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `any/encode_json` on 28.0.0, 28.0.1, 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 519

```json
{"json":{"body":{"v0":{"data":{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"u32":0}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]}]},"topics":[{"symbol":"shape"}]}},"contract_id":"CAZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDFJRF","ext":"v0","type":"contract"},"op":"encode_json","type":"ContractEvent"}
```

Expected: `{"xdr":"AAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAAEAAAAAAAAAAQAAAA8AAAAFc2hhcGUAAAAAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQAAAAAQAAAAEAAAAQ... (1720 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 519"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk rust --version 28.0.0 --entry soroban.scval-shapes.synth-event-vec-depth-101
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/soroban.scval-shapes/soroban.scval-shapes.synth-event-vec-depth-101/any.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.synth-event-vec-depth-101/any.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.scval-shapes/soroban.scval-shapes.synth-event-vec-depth-101/any.xdr

How the corpus entry was built and checked: a contract event whose data nests 101 ScVec levels. every ScVal the host writes passes from_host_val, which fails past DEFAULT_HOST_DEPTH_LIMIT = 100 (scval.depth), so meta never carries this. the bytes decode well inside xdr.meta.depth.

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `soroban.scval-shapes.synth-event-vec-depth-101`
