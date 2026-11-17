<!-- repo: Soneso/kmp-stellar-sdk -->
# com.soneso.stellar:stellar-sdk-jvm 1.18.0: the sdk accepted a value the network refuses (cap21.preconditions.synth-extra-signers-3)

## What happens

On the input below, `com.soneso.stellar:stellar-sdk-jvm` 1.18.0 answers differently from the Stellar network in 1 check.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap21.preconditions.synth-extra-signers-3` (family `cap21.preconditions`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `any/decode_encode` on 1.18.0

the sdk accepted a value the network refuses

```json
{
  "depth_limit": 1000,
  "len_limit": 336,
  "op": "decode_encode",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAADAAAAAhQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUAAAAAhUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVAAAAAhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWAAAAAAAAAAEAAAAAAAAAAQAAAAACAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgAAAAAAAAAAAAAAAQAAAAAAAAABBwcHBwAAAEAHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcH"
}
```

Expected: `"rejected"`

Actual: `{"xdr":"AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAADAAAAAhQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUAAAAAhUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVAAAAAhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWAAAAAAAAAAEAAAAAAAAAAQAAAAACAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgAAAAAAAAAAAAAAAQAAAAAAAAABBwcHBwAAAEAHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcH"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk kmp --version 1.18.0 --entry cap21.preconditions.synth-extra-signers-3
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap21.preconditions/cap21.preconditions.synth-extra-signers-3/any.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap21.preconditions/cap21.preconditions.synth-extra-signers-3/any.xdr): decode = must-reject (validity InvalidByCore)

How the corpus entry was built and checked: preconditions v2 with 3 extra signers, one past extraSigners<2>. stellar-xdr 30.0.0 rejects it (TransactionEnvelope::from_xdr) with LengthExceedsMax under Limits { depth: 1000 (xdr.envelope.depth), len: 336 (the input length) }.

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap21.preconditions.synth-extra-signers-3`
