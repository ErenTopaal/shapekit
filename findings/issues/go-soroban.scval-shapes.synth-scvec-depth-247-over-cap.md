<!-- repo: stellar/go-stellar-sdk -->
# github.com/stellar/go-stellar-sdk v0.7.1, v0.7.2, v0.7.3, v0.8.0, v0.9.0: the sdk accepted a value the network refuses (soroban.scval-shapes.synth-scvec-depth-247-over-cap)

## What happens

On the input below, `github.com/stellar/go-stellar-sdk` v0.7.1, v0.7.2, v0.7.3, v0.8.0, v0.9.0 answers differently from the Stellar network in 5 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `soroban.scval-shapes.synth-scvec-depth-247-over-cap` (family `soroban.scval-shapes`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `any/decode_encode` on v0.7.1, v0.7.2, v0.7.3, v0.8.0, v0.9.0

the sdk accepted a value the network refuses

```json
{
  "depth_limit": 1000,
  "len_limit": 3212,
  "op": "decode_encode",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAAB... (4284 chars)"
}
```

Expected: `"rejected"`

Actual: `{"xdr":"AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAABAAAAAQAAABAAAAAB... (4284 chars)"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk go --version v0.7.1 --entry soroban.scval-shapes.synth-scvec-depth-247-over-cap
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/soroban.scval-shapes/soroban.scval-shapes.synth-scvec-depth-247-over-cap/any.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.synth-scvec-depth-247-over-cap/any.xdr): decode = must-reject (validity InvalidByCore)

How the corpus entry was built and checked: an invoke-contract argument nested 247 ScVec levels deep, one past the deepest that decodes inside an envelope at depth 1000 (scval.depth.envelope = 246). core refuses the envelope from protocol 28 because its rust parser (TransactionEnvelope::read_xdr in can_parse_transaction) fails at depth 1000. stellar-xdr 30.0.0 rejects it (TransactionEnvelope::from_xdr) with DepthLimitExceeded under Limits { depth: 1000 (xdr.envelope.depth), len: 3212 (the input length) }.

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `soroban.scval-shapes.synth-scvec-depth-247-over-cap`
