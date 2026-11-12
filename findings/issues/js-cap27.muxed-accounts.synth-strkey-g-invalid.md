<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.3.0, 16.3.1, 16.4.1: the sdk accepted a value the network refuses (cap27.muxed-accounts.synth-strkey-g-invalid)

## What happens

`@stellar/stellar-sdk` 16.3.0, 16.3.1, 16.4.1 answers 15 of the checks of corpus entry `cap27.muxed-accounts.synth-strkey-g-invalid` differently from the Stellar network.

## Minimal vector

Entry `cap27.muxed-accounts.synth-strkey-g-invalid` (kind Strkey, family `cap27.muxed-accounts`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `case 2/strkey_decode` on 16.3.0, 16.3.1, 16.4.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAG4XI"
}
```

Expected: `"rejected"`

Actual: `{"kind":"account","payload":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="}`

### `case 3/strkey_decode` on 16.3.0, 16.3.1, 16.4.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEL6U"
}
```

Expected: `"rejected"`

Actual: `{"kind":"account","payload":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}`

### `case 12/strkey_decode` on 16.3.0, 16.3.1, 16.4.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "MA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAPG"
}
```

Expected: `"rejected"`

Actual: `{"kind":"muxed_account","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6Jo="}`

### `case 13/strkey_decode` on 16.3.0, 16.3.1, 16.4.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "GAAAAAAAACGC6"
}
```

Expected: `"rejected"`

Actual: `{"kind":"account","payload":"AAAAAAA="}`

### `case 15/strkey_decode` on 16.3.0, 16.3.1, 16.4.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "GA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUACUSI"
}
```

Expected: `"rejected"`

Actual: `{"kind":"account","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6JoA"}`

## Reproduce

```sh
shapekit conformance run --sdk js --version 16.3.0 --entry cap27.muxed-accounts.synth-strkey-g-invalid
```

## Expected value and its source

- corpus/vectors/strkey/cap27.muxed-accounts.synth-strkey-g-invalid.json case 12
- corpus/vectors/strkey/cap27.muxed-accounts.synth-strkey-g-invalid.json case 13
- corpus/vectors/strkey/cap27.muxed-accounts.synth-strkey-g-invalid.json case 15
- corpus/vectors/strkey/cap27.muxed-accounts.synth-strkey-g-invalid.json case 2
- corpus/vectors/strkey/cap27.muxed-accounts.synth-strkey-g-invalid.json case 3

Entry notes: invalid ed25519 account public keys (G, 6 << 3). expectations are stellar-strkey 1.0.0's verdicts; SEP-23 1.4.0 says a strkey is valid only if re-encoding its binary gives back the same string, which rules out wrong lengths, set trailing bits, padding and lowercase. the reject reason is stellar-strkey's error and is informational; a testee passes by rejecting.

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `cap27.muxed-accounts.synth-strkey-g-invalid`
