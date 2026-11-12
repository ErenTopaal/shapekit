<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.3.0, 16.3.1, 16.4.1, 17.2.0: the sdk's bytes differ from the expected bytes (soroban.scval-shapes.synth-map-keys-mixed-types)

## What happens

`@stellar/stellar-sdk` 16.3.0, 16.3.1, 16.4.1, 17.2.0 answers 4 of the checks of corpus entry `soroban.scval-shapes.synth-map-keys-mixed-types` differently from the Stellar network.

## Minimal vector

Entry `soroban.scval-shapes.synth-map-keys-mixed-types` (kind Ordering, family `soroban.scval-shapes`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `case 0/map_sort` on 16.3.0, 16.3.1, 16.4.1, 17.2.0

the sdk's bytes differ from the expected bytes

```json
{
  "entries": [
    {
      "key": "AAAADwAAAAFhAAAA",
      "val": "AAAAAwAAAAA="
    },
    {
      "key": "AAAAAwAAAAE=",
      "val": "AAAAAwAAAAE="
    },
    {
      "key": "AAAAAAAAAAE=",
      "val": "AAAAAwAAAAI="
    }
  ],
  "op": "map_sort"
}
```

Expected: `{"xdr":"AAAAEQAAAAEAAAADAAAAAAAAAAEAAAADAAAAAgAAAAMAAAABAAAAAwAAAAEAAAAPAAAAAWEAAAAAAAADAAAAAA=="}`

Actual: `{"xdr":"AAAAEQAAAAEAAAADAAAADwAAAAFhAAAAAAAAAwAAAAAAAAADAAAAAQAAAAMAAAABAAAAAAAAAAEAAAADAAAAAg=="}`

## Reproduce

```sh
shapekit conformance run --sdk js --version 16.3.0 --entry soroban.scval-shapes.synth-map-keys-mixed-types
```

## Expected value and its source

- corpus/vectors/ordering/soroban.scval-shapes.synth-map-keys-mixed-types.json case 0

Entry notes: the js-stellar-sdk#1766 mixed keys: different types order by ScValType discriminant, so bool (0) before u32 (3) before symbol (15). map_sort expects the entry indices in the order soroban-env-host 30.0.0 sorts them (Compare<ScVal> for Budget, src/host/comparison.rs), and the sorted map; map_validate expects the host's verdict on converting the map to a host object, which refuses keys that are not strictly increasing ("ScMap was not sorted by key for conversion to host object").

Related: https://github.com/stellar/js-stellar-sdk/pull/1766

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.scval-shapes.synth-map-keys-mixed-types`
