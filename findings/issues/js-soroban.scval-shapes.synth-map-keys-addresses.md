<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.3.0, 16.3.1, 16.4.1, 17.2.0: the sdk's bytes differ from the expected bytes (soroban.scval-shapes.synth-map-keys-addresses)

## What happens

On the input below, `@stellar/stellar-sdk` 16.3.0, 16.3.1, 16.4.1, 17.2.0 answers differently from the Stellar network in 4 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `soroban.scval-shapes.synth-map-keys-addresses` (family `soroban.scval-shapes`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `case 0/map_sort` on 16.3.0, 16.3.1, 16.4.1, 17.2.0

the sdk's bytes differ from the expected bytes

```json
{
  "entries": [
    {
      "key": "AAAAEgAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==",
      "val": "AAAAAwAAAAA="
    },
    {
      "key": "AAAAEgAAAAAAAAAA//////////////////////////////////////////8=",
      "val": "AAAAAwAAAAE="
    }
  ],
  "op": "map_sort"
}
```

Expected: `{"xdr":"AAAAEQAAAAEAAAACAAAAEgAAAAAAAAAA//////////////////////////////////////////8AAAADAAAAAQAAABIAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAADAAAAAA=="}`

Actual: `{"xdr":"AAAAEQAAAAEAAAACAAAAEgAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAMAAAAAAAAAEgAAAAAAAAAA//////////////////////////////////////////8AAAADAAAAAQ=="}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk js --version 16.3.0 --entry soroban.scval-shapes.synth-map-keys-addresses
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/vectors/ordering/soroban.scval-shapes.synth-map-keys-addresses.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/vectors/ordering/soroban.scval-shapes.synth-map-keys-addresses.json) case 0

How the corpus entry was built and checked: an account address with key bytes ff..ff and a contract address with hash 00..00: ScAddressType decides first (account 0 before contract 1), so the account is lower although its bytes are higher. map_sort expects the entry indices in the order soroban-env-host 30.0.0 sorts them (Compare<ScVal> for Budget, src/host/comparison.rs), and the sorted map; map_validate expects the host's verdict on converting the map to a host object, which refuses keys that are not strictly increasing ("ScMap was not sorted by key for conversion to host object").

Related: https://github.com/stellar/js-stellar-sdk/pull/1766

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `soroban.scval-shapes.synth-map-keys-addresses`
