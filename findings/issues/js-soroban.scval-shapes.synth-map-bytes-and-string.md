<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.3.0, 16.3.1, 16.4.1: the native projection keeps 1 of 2 map entries; the rest were lost silently (soroban.scval-shapes.synth-map-bytes-and-string)

## What happens

`@stellar/stellar-sdk` 16.3.0, 16.3.1, 16.4.1 answers 3 of the checks of corpus entry `soroban.scval-shapes.synth-map-bytes-and-string` differently from the Stellar network.

## Minimal vector

Entry `soroban.scval-shapes.synth-map-bytes-and-string` (kind NativeProjection, family `soroban.scval-shapes`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `case 0/to_native` on 16.3.0, 16.3.1, 16.4.1

the native projection keeps 1 of 2 map entries; the rest were lost silently

```json
{
  "op": "to_native",
  "xdr": "AAAAEQAAAAEAAAACAAAADQAAAAExAAAAAAAAAwAAAAoAAAAOAAAAATEAAAAAAAADAAAAFA=="
}
```

Expected: `{"entries":2,"native":"a map keeping all entries"}`

Actual: `{"native":{"entries":[{"key":{"type":"string","value":"1"},"value":{"type":"integer","value":"20"}}],"type":"map"}}`

## Reproduce

```sh
shapekit conformance run --sdk js --version 16.3.0 --entry soroban.scval-shapes.synth-map-bytes-and-string
```

## Expected value and its source

- corpus/vectors/native-projection/soroban.scval-shapes.synth-map-bytes-and-string.json case 0

Entry notes: bytes [0x31] and string "1": a projection that turns bytes into a buffer and then into text keys them both as 1. keys are sorted the way soroban-env-host 30.0.0 sorts them and the host converts the map, so a contract can emit it. rule no-entry-lost: an error passes, a structure keeping all 2 entries with their key types passes, fewer entries fails.

Related: https://github.com/stellar/js-stellar-sdk/issues/1789

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.scval-shapes.synth-map-bytes-and-string`
