<!-- repo: Soneso/stellar-php-sdk -->
# soneso/stellar-php-sdk 1.18.0: the sdk accepted a value the network refuses (classic.max-length.synth-tx-101-operations)

## What happens

On the input below, `soneso/stellar-php-sdk` 1.18.0 answers differently from the Stellar network in 1 check.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `classic.max-length.synth-tx-101-operations` (family `classic.max-length`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `any/decode_encode` on 1.18.0

the sdk accepted a value the network refuses

```json
{
  "depth_limit": 1000,
  "len_limit": 5800,
  "op": "decode_encode",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAZQAAAAAAAAABAAAAAAICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMAAAAAAAAAAAAAAAEAAAAAAAAAAQAAAAAEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAAAAAAAAAAAAAAAAQAAAAAAAAABAAAAAAICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMAAAAAAAAAAAAAAAEAAAAAAAAAAQAAAAAEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE... (7736 chars)"
}
```

Expected: `"rejected"`

Actual: `{"xdr":"AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAZQAAAAAAAAABAAAAAAICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMAAAAAAAAAAAAAAAEAAAAAAAAAAQAAAAAEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAAAAAAAAAAAAAAAAQAAAAAAAAABAAAAAAICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMAAAAAAAAAAAAAAAEAAAAAAAAAAQAAAAAEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE... (7736 chars)"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk php --version 1.18.0 --entry classic.max-length.synth-tx-101-operations
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/classic.max-length/classic.max-length.synth-tx-101-operations/any.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/classic.max-length/classic.max-length.synth-tx-101-operations/any.xdr): decode = must-reject (validity InvalidByCore)

How the corpus entry was built and checked: a v1 transaction envelope with 101 operations, one past MAX_OPS_PER_TX = 100 (operations<MAX_OPS_PER_TX> in Stellar-transaction.x). stellar-xdr 30.0.0 rejects it (TransactionEnvelope::from_xdr) with LengthExceedsMax under Limits { depth: 1000 (xdr.envelope.depth), len: 5800 (the input length) }.

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `classic.max-length.synth-tx-101-operations`
