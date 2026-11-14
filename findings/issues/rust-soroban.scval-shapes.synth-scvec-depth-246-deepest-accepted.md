<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 28.0.0, 28.0.1, 30.0.0: the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 970 (soroban.scval-shapes.synth-scvec-depth-246-deepest-accepted)

## What happens

On the input below, `stellar-xdr` 28.0.0, 28.0.1, 30.0.0 answers differently from the Stellar network in 3 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `soroban.scval-shapes.synth-scvec-depth-246-deepest-accepted` (family `soroban.scval-shapes`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `any/encode_json` on 28.0.0, 28.0.1, 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 970

```json
{"json":{"tx":{"signatures":[{"hint":"07070707","signature":"07070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707"}],"tx":{"cond":"none","ext":{"v1":{"ext":"v0","resource_fee":"100000","resources":{"disk_read_bytes":0,"footprint":{"read_only":[],"read_write":[]},"instructions":1000000,"write_bytes":0}}},"fee":100,"memo":"none","operations":[{"body":{"invoke_host_function":{"auth":[],"host_function":{"invoke_contract":{"args":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec":[{"vec"... (3256 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAAB... (4268 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 970"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk rust --version 28.0.0 --entry soroban.scval-shapes.synth-scvec-depth-246-deepest-accepted
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/soroban.scval-shapes/soroban.scval-shapes.synth-scvec-depth-246-deepest-accepted/any.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.synth-scvec-depth-246-deepest-accepted/any.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.scval-shapes/soroban.scval-shapes.synth-scvec-depth-246-deepest-accepted/any.xdr

How the corpus entry was built and checked: an invoke-contract argument nested 246 ScVec levels deep, at the deepest an envelope decodes at depth 1000. core's depth check accepts it; the host converts at most 100 levels (scval.depth) and runs at most 100 frames, so an invocation this deep fails and the transaction lands in the ledger as failed with the full tree intact. a decoder must accept it.

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `soroban.scval-shapes.synth-scvec-depth-246-deepest-accepted`
