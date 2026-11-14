<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.3.0, 16.3.1, 16.4.1: the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth (soroban.auth-trees.synth-auth-depth-493-deepest-accepted)

## What happens

On the input below, `@stellar/stellar-sdk` 16.3.0, 16.3.1, 16.4.1 answers differently from the Stellar network in 6 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `soroban.auth-trees.synth-auth-depth-493-deepest-accepted` (family `soroban.auth-trees`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `any/decode_encode` on 16.3.0, 16.3.1, 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 27852,
  "op": "decode_encode",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAATIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyAAAABGRlZXAAAAAAAAAAAQAAAAAAAAABMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIAAAAEZGVlcAAAAAAAAAABAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAATIyMjIyMjIyMjIyMjIyMjIyMjIy... (37136 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAATIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyAAAABGRlZXAAAAAAAAAAAQAAAAAAAAABMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIAAAAEZGVlcAAAAAAAAAABAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAATIyMjIyMjIyMjIyMjIyMjIyMjIy... (37136 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `any/decode_json` on 16.3.0, 16.3.1, 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1000,
  "len_limit": 27852,
  "op": "decode_json",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAATIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyAAAABGRlZXAAAAAAAAAAAQAAAAAAAAABMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIAAAAEZGVlcAAAAAAAAAABAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAATIyMjIyMjIyMjIyMjIyMjIyMjIy... (37136 chars)"
}
```

Expected: `{"json":{"tx":{"signatures":[{"hint":"07070707","signature":"07070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707"}],"tx":{"cond":"none","ext":{"v1":{"ext":"v0","resource_fee":"100000","resources":{"disk_read_bytes":0,"footprint":{"read_only":[],"read_write":[]},"instructions":1000000,"write_bytes":0}}},"fee":100,"memo":"none","operations":[{"body":{"invoke_host_function":{"auth":[{"credentials":"source_account","root_invocation":{"function":{"contract_fn":{"args":[],"contract_address":"CAZDEMRSGIZDEMRSGIZDEMRSGIZDEMRS... (80656 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk js --version 16.3.0 --entry soroban.auth-trees.synth-auth-depth-493-deepest-accepted
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.synth-auth-depth-493-deepest-accepted/any.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.synth-auth-depth-493-deepest-accepted/any.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.synth-auth-depth-493-deepest-accepted/any.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.synth-auth-depth-493-deepest-accepted/any.xdr): decode = must-accept, roundtrip = true

How the corpus entry was built and checked: a source-account authorization entry whose invocation tree is a single chain 493 invocations deep, at the deepest an envelope decodes at depth 1000. core's depth check accepts it; the host converts at most 100 levels (scval.depth) and runs at most 100 frames, so an invocation this deep fails and the transaction lands in the ledger as failed with the full tree intact. a decoder must accept it.

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `soroban.auth-trees.synth-auth-depth-493-deepest-accepted`
