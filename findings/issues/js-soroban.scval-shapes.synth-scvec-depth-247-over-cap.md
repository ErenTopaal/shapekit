<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 17.2.0, 17.2.1, 17.3.0, 17.4.0: the sdk accepted a value the network refuses (soroban.scval-shapes.synth-scvec-depth-247-over-cap)

## What happens

`@stellar/stellar-sdk` 17.2.0, 17.2.1, 17.3.0, 17.4.0 answers 4 of the checks of corpus entry `soroban.scval-shapes.synth-scvec-depth-247-over-cap` differently from the Stellar network.

## Minimal vector

Entry `soroban.scval-shapes.synth-scvec-depth-247-over-cap` (kind Xdr, family `soroban.scval-shapes`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `any/decode_encode` on 17.2.0, 17.2.1, 17.3.0, 17.4.0

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

```sh
shapekit conformance run --sdk js --version 17.2.0 --entry soroban.scval-shapes.synth-scvec-depth-247-over-cap
```

## Expected value and its source

- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.synth-scvec-depth-247-over-cap/any.xdr: decode = must-reject (validity InvalidByCore)

Entry notes: an invoke-contract argument nested 247 ScVec levels deep, one past the deepest that decodes inside an envelope at depth 1000 (scval.depth.envelope = 246). core refuses the envelope from protocol 28 because its rust parser (TransactionEnvelope::read_xdr in can_parse_transaction) fails at depth 1000. stellar-xdr 30.0.0 rejects it (TransactionEnvelope::from_xdr) with DepthLimitExceeded under Limits { depth: 1000 (xdr.envelope.depth), len: 3212 (the input length) }.

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.scval-shapes.synth-scvec-depth-247-over-cap`
