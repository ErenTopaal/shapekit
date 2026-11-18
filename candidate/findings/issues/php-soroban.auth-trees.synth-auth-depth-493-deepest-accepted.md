<!-- repo: Soneso/stellar-php-sdk -->
# soneso/stellar-php-sdk 1.18.0: the sdk refused a value the network accepts: JsonException: Maximum stack depth exceeded (soroban.auth-trees.synth-auth-depth-493-deepest-accepted)

## What happens

On the input below, `soneso/stellar-php-sdk` 1.18.0 answers differently from the Stellar network in 2 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `soroban.auth-trees.synth-auth-depth-493-deepest-accepted` (family `soroban.auth-trees`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `any/decode_json` on 1.18.0

the sdk refused a value the network accepts: JsonException: Maximum stack depth exceeded

```json
{
  "depth_limit": 1000,
  "len_limit": 27852,
  "op": "decode_json",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAATIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyAAAABGRlZXAAAAAAAAAAAQAAAAAAAAABMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIAAAAEZGVlcAAAAAAAAAABAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAATIyMjIyMjIyMjIyMjIyMjIyMjIy... (37136 chars)"
}
```

Expected: `{"depth":997,"json_bytes":80656,"preview":"{\"json\":{\"tx\":{\"signatures\":[{\"hint\":\"07070707\",\"signature\":\"07070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707\"}],\"tx\":{\"cond\":\"none\",\"ext\":{\"v1\":{\"ext\":\"v0\",\"resource_fee\":\"100000\",\"resources\":{\"disk_read_bytes\":0,\"footprint\":{\"read_only\":[],\"read_write\":[]},\"instructions\":1000000,\"write_bytes\":0}}},\"fee\":100,\"memo\":\"none\",\"operations\":[{\"body\":{\"invoke_host_function\":{\"auth\":[{\"credentials\":\"source_account\",\... (700 chars, see the source below)`

Actual: `{"rejected":"JsonException: Maximum stack depth exceeded"}`

### `any/encode_json` on 1.18.0

the sdk refused a value the network accepts: JsonException: Maximum stack depth exceeded

```json
{
  "depth": 997,
  "json_bytes": 80704,
  "preview": "{\"json\":{\"tx\":{\"signatures\":[{\"hint\":\"07070707\",\"signature\":\"07070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707\"}],\"tx\":{\"cond\":\"none\",\"ext\":{\"v1\":{\"ext\":\"v0\",\"resource_fee\":\"100000\",\"resources\":{\"disk_read_bytes\":0,\"footprint\":{\"read_only\":[],\"read_write\":[]},\"instructions\":1000000,\"write_bytes\":0}}},\"fee\":100,\"memo\":\"none\",\"operations\":[{\"body\":{\"invoke_host_function\":{\"auth\":[{\"credentials\":\"source_account\",\"root_invocation\":{\"",
  "sha256": "cc8f7920ec49718124482c941e189230a894069c14b86602b6862e316b7c88bf"
}
```

Expected: `{"xdr":"AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAATIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyAAAABGRlZXAAAAAAAAAAAQAAAAAAAAABMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIAAAAEZGVlcAAAAAAAAAABAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAATIyMjIyMjIyMjIyMjIyMjIyMjIy... (37136 chars)"}`

Actual: `{"rejected":"JsonException: Maximum stack depth exceeded"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`:

```sh
shapekit conformance run --sdk php --version 1.18.0 --entry soroban.auth-trees.synth-auth-depth-493-deepest-accepted
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.synth-auth-depth-493-deepest-accepted/any.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.synth-auth-depth-493-deepest-accepted/any.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.synth-auth-depth-493-deepest-accepted/any.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.synth-auth-depth-493-deepest-accepted/any.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.auth-trees/soroban.auth-trees.synth-auth-depth-493-deepest-accepted/any.xdr

How the corpus entry was built and checked: a source-account authorization entry whose invocation tree is a single chain 493 invocations deep, at the deepest an envelope decodes at depth 1000. core's depth check accepts it; the host converts at most 100 levels (scval.depth) and runs at most 100 frames, so an invocation this deep fails and the transaction lands in the ledger as failed with the full tree intact. a decoder must accept it.

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `soroban.auth-trees.synth-auth-depth-493-deepest-accepted`
