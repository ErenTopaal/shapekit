<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1: the native projection keeps 1 of 2 map entries; the rest were lost silently (soroban.scval-shapes.synth-map-vec-and-string)

## What happens

On the input below, `@stellar/stellar-sdk` 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1 answers differently from the Stellar network in 5 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `soroban.scval-shapes.synth-map-vec-and-string` (family `soroban.scval-shapes`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `case 0/to_native` on 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1

the native projection keeps 1 of 2 map entries; the rest were lost silently

```json
{
  "op": "to_native",
  "xdr": "AAAAEQAAAAEAAAACAAAADgAAAAExAAAAAAAAAwAAABQAAAAQAAAAAQAAAAEAAAADAAAAAQAAAAMAAAAK"
}
```

Expected: `{"entries":2,"native":"a map keeping all entries"}`

Actual: `{"native":{"entries":[{"key":{"type":"string","value":"1"},"value":{"type":"integer","value":"10"}}],"type":"map"}}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk js --version 16.3.0 --entry soroban.scval-shapes.synth-map-vec-and-string
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/vectors/native-projection/soroban.scval-shapes.synth-map-vec-and-string.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/vectors/native-projection/soroban.scval-shapes.synth-map-vec-and-string.json) case 0

How the corpus entry was built and checked: vec [u32 1] and string "1": an array key stringifies to its single element in some languages. keys are sorted the way soroban-env-host 30.0.0 sorts them and the host converts the map, so a contract can emit it. rule no-entry-lost: an error passes, a structure keeping all 2 entries with their key types passes, fewer entries fails.

Related: https://github.com/stellar/js-stellar-sdk/issues/1789

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `soroban.scval-shapes.synth-map-vec-and-string`
