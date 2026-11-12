<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1: the native projection keeps 1 of 3 map entries; the rest were lost silently (soroban.scval-shapes.synth-map-void-and-text)

## What happens

`@stellar/stellar-sdk` 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1 answers 5 of the checks of corpus entry `soroban.scval-shapes.synth-map-void-and-text` differently from the Stellar network.

## Minimal vector

Entry `soroban.scval-shapes.synth-map-void-and-text` (kind NativeProjection, family `soroban.scval-shapes`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `case 0/to_native` on 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1

the native projection keeps 1 of 3 map entries; the rest were lost silently

```json
{
  "op": "to_native",
  "xdr": "AAAAEQAAAAEAAAADAAAAAQAAAAMAAAAKAAAADgAAAARudWxsAAAAAwAAABQAAAAPAAAABG51bGwAAAADAAAAHg=="
}
```

Expected: `{"entries":3,"native":"a map keeping all entries"}`

Actual: `{"native":{"entries":[{"key":{"type":"string","value":"null"},"value":{"type":"integer","value":"30"}}],"type":"map"}}`

## Reproduce

```sh
shapekit conformance run --sdk js --version 16.3.0 --entry soroban.scval-shapes.synth-map-void-and-text
```

## Expected value and its source

- corpus/vectors/native-projection/soroban.scval-shapes.synth-map-void-and-text.json case 0

Entry notes: void, string "null" and symbol "null": void projects to null, which stringifies to null as a key. keys are sorted the way soroban-env-host 30.0.0 sorts them and the host converts the map, so a contract can emit it. rule no-entry-lost: an error passes, a structure keeping all 3 entries with their key types passes, fewer entries fails.

Related: https://github.com/stellar/js-stellar-sdk/issues/1789

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.scval-shapes.synth-map-void-and-text`
