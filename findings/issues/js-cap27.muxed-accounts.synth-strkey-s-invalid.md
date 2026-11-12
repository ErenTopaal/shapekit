<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.3.0, 16.3.1, 16.4.1: the sdk accepted a value the network refuses (cap27.muxed-accounts.synth-strkey-s-invalid)

## What happens

`@stellar/stellar-sdk` 16.3.0, 16.3.1, 16.4.1 answers 6 of the checks of corpus entry `cap27.muxed-accounts.synth-strkey-s-invalid` differently from the Stellar network.

## Minimal vector

Entry `cap27.muxed-accounts.synth-strkey-s-invalid` (kind Strkey, family `cap27.muxed-accounts`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `case 2/strkey_decode` on 16.3.0, 16.3.1, 16.4.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "SAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAACXAQ"
}
```

Expected: `"rejected"`

Actual: `{"kind":"seed","payload":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="}`

### `case 3/strkey_decode` on 16.3.0, 16.3.1, 16.4.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "SAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHT5A"
}
```

Expected: `"rejected"`

Actual: `{"kind":"seed","payload":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}`

## Reproduce

```sh
shapekit conformance run --sdk js --version 16.3.0 --entry cap27.muxed-accounts.synth-strkey-s-invalid
```

## Expected value and its source

- corpus/vectors/strkey/cap27.muxed-accounts.synth-strkey-s-invalid.json case 2
- corpus/vectors/strkey/cap27.muxed-accounts.synth-strkey-s-invalid.json case 3

Entry notes: invalid ed25519 secret seeds (S, 18 << 3); every seed here is a made-up byte pattern. expectations are stellar-strkey 1.0.0's verdicts; SEP-23 1.4.0 says a strkey is valid only if re-encoding its binary gives back the same string, which rules out wrong lengths, set trailing bits, padding and lowercase. the reject reason is stellar-strkey's error and is informational; a testee passes by rejecting.

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `cap27.muxed-accounts.synth-strkey-s-invalid`
