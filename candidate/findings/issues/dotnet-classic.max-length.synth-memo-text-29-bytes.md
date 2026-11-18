<!-- repo: Beans-BV/dotnet-stellar-sdk -->
# stellar-dotnet-sdk 16.0.0, 17.0.0: the sdk accepted a value the network refuses (classic.max-length.synth-memo-text-29-bytes)

## What happens

On the input below, `stellar-dotnet-sdk` 16.0.0, 17.0.0 answers differently from the Stellar network in 2 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `classic.max-length.synth-memo-text-29-bytes` (family `classic.max-length`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `any/decode_encode` on 16.0.0, 17.0.0

the sdk accepted a value the network refuses

```json
{
  "depth_limit": 1000,
  "len_limit": 236,
  "op": "decode_encode",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAABAAAAHW1tbW1tbW1tbW1tbW1tbW1tbW1tbW1tbW1tbW1tAAAAAAAAAQAAAAAAAAABAAAAAAICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAAAAAAAAAAAAAAABAAAAAAAAAAEHBwcHAAAAQAcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc="
}
```

Expected: `"rejected"`

Actual: `{"xdr":"AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAABAAAAHW1tbW1tbW1tbW1tbW1tbW1tbW1tbW1tbW1tbW1tAAAAAAAAAQAAAAAAAAABAAAAAAICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAAAAAAAAAAAAAAABAAAAAAAAAAEHBwcHAAAAQAcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc="}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`:

```sh
shapekit conformance run --sdk dotnet --version 16.0.0 --entry classic.max-length.synth-memo-text-29-bytes
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/classic.max-length/classic.max-length.synth-memo-text-29-bytes/any.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/classic.max-length/classic.max-length.synth-memo-text-29-bytes/any.xdr): decode = must-reject (validity InvalidByCore)

How the corpus entry was built and checked: a memo text of 29 bytes, one past string text<28>. stellar-xdr 30.0.0 rejects it (TransactionEnvelope::from_xdr) with LengthExceedsMax under Limits { depth: 1000 (xdr.envelope.depth), len: 236 (the input length) }.

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `classic.max-length.synth-memo-text-29-bytes`
