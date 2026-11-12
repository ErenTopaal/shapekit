<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 17.2.0: the sdk's bytes differ from the expected bytes (soroban.scval-shapes.synth-map-keys-bytes)

## What happens

`@stellar/stellar-sdk` 17.2.0 answers 1 of the checks of corpus entry `soroban.scval-shapes.synth-map-keys-bytes` differently from the Stellar network.

## Minimal vector

Entry `soroban.scval-shapes.synth-map-keys-bytes` (kind Ordering, family `soroban.scval-shapes`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

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

```sh
shapekit conformance run --sdk js --version 17.2.0 --entry soroban.scval-shapes.synth-map-keys-bytes
```

## Expected value and its source

- corpus/vectors/ordering/soroban.scval-shapes.synth-map-keys-bytes.json case 0

Entry notes: the js-stellar-sdk#1766 bytes keys [10], [9], [1,2], [2]. the host compares content bytewise, so [1,2] comes before [2]; the XDR encodings start with the length, so sorting the encoded keys puts [2] first. map_sort expects the entry indices in the order soroban-env-host 30.0.0 sorts them (Compare<ScVal> for Budget, src/host/comparison.rs), and the sorted map; map_validate expects the host's verdict on converting the map to a host object, which refuses keys that are not strictly increasing ("ScMap was not sorted by key for conversion to host object").

Related: https://github.com/stellar/js-stellar-sdk/pull/1766

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.scval-shapes.synth-map-keys-bytes`
