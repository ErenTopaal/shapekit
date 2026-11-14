<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 17.2.0: the sdk's bytes differ from the expected bytes (soroban.scval-shapes.synth-map-keys-bytes)

## What happens

On the input below, `@stellar/stellar-sdk` 17.2.0 answers differently from the Stellar network in 1 check.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `soroban.scval-shapes.synth-map-keys-bytes` (family `soroban.scval-shapes`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `case 0/map_sort` on 17.2.0

the sdk's bytes differ from the expected bytes

```json
{
  "entries": [
    {
      "key": "AAAADQAAAAEKAAAA",
      "val": "AAAAAwAAAAA="
    },
    {
      "key": "AAAADQAAAAEJAAAA",
      "val": "AAAAAwAAAAE="
    },
    {
      "key": "AAAADQAAAAIBAgAA",
      "val": "AAAAAwAAAAI="
    },
    {
      "key": "AAAADQAAAAECAAAA",
      "val": "AAAAAwAAAAM="
    }
  ],
  "op": "map_sort"
}
```

Expected: `{"xdr":"AAAAEQAAAAEAAAAEAAAADQAAAAIBAgAAAAAAAwAAAAIAAAANAAAAAQIAAAAAAAADAAAAAwAAAA0AAAABCQAAAAAAAAMAAAABAAAADQAAAAEKAAAAAAAAAwAAAAA="}`

Actual: `{"xdr":"AAAAEQAAAAEAAAAEAAAADQAAAAIBAgAAAAAAAwAAAAIAAAANAAAAAQoAAAAAAAADAAAAAAAAAA0AAAABAgAAAAAAAAMAAAADAAAADQAAAAEJAAAAAAAAAwAAAAE="}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk js --version 17.2.0 --entry soroban.scval-shapes.synth-map-keys-bytes
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/vectors/ordering/soroban.scval-shapes.synth-map-keys-bytes.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/vectors/ordering/soroban.scval-shapes.synth-map-keys-bytes.json) case 0

How the corpus entry was built and checked: the js-stellar-sdk#1766 bytes keys [10], [9], [1,2], [2]. the host compares content bytewise, so [1,2] comes before [2]; the XDR encodings start with the length, so sorting the encoded keys puts [2] first. map_sort expects the entry indices in the order soroban-env-host 30.0.0 sorts them (Compare<ScVal> for Budget, src/host/comparison.rs), and the sorted map; map_validate expects the host's verdict on converting the map to a host object, which refuses keys that are not strictly increasing ("ScMap was not sorted by key for conversion to host object").

Related: https://github.com/stellar/js-stellar-sdk/pull/1766

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `soroban.scval-shapes.synth-map-keys-bytes`
