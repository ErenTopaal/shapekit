<!-- repo: stellar/go-stellar-sdk -->
# github.com/stellar/go-stellar-sdk v0.7.1: the sdk accepted a value the network refuses (cap27.muxed-accounts.synth-strkey-m-invalid)

## What happens

`github.com/stellar/go-stellar-sdk` v0.7.1 answers 4 of the checks of corpus entry `cap27.muxed-accounts.synth-strkey-m-invalid` differently from the Stellar network.

## Minimal vector

Entry `cap27.muxed-accounts.synth-strkey-m-invalid` (kind Strkey, family `cap27.muxed-accounts`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `case 2/strkey_decode` on v0.7.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "MA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAAAAAAM36A"
}
```

Expected: `"rejected"`

Actual: `{"kind":"muxed_account","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6JoAAAAAAAAA"}`

### `case 3/strkey_decode` on v0.7.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "MA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAAAAAAAAAMYWQ"
}
```

Expected: `"rejected"`

Actual: `{"kind":"muxed_account","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6JoAAAAAAAAAAAA="}`

### `case 13/strkey_decode` on v0.7.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "MA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAPG"
}
```

Expected: `"rejected"`

Actual: `{"kind":"muxed_account","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6Jo="}`

### `case 16/strkey_decode` on v0.7.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "MA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJVAAAAAAAAAAAAAAV75I"
}
```

Expected: `"rejected"`

Actual: `{"kind":"muxed_account","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6JqAAAAAAAAAAAE="}`

## Reproduce

```sh
shapekit conformance run --sdk go --version v0.7.1 --entry cap27.muxed-accounts.synth-strkey-m-invalid
```

## Expected value and its source

- corpus/vectors/strkey/cap27.muxed-accounts.synth-strkey-m-invalid.json case 13
- corpus/vectors/strkey/cap27.muxed-accounts.synth-strkey-m-invalid.json case 16
- corpus/vectors/strkey/cap27.muxed-accounts.synth-strkey-m-invalid.json case 2
- corpus/vectors/strkey/cap27.muxed-accounts.synth-strkey-m-invalid.json case 3

Entry notes: invalid muxed accounts (M, 12 << 3, CAP-27): 32 key bytes then a big-endian u64 id. expectations are stellar-strkey 1.0.0's verdicts; SEP-23 1.4.0 says a strkey is valid only if re-encoding its binary gives back the same string, which rules out wrong lengths, set trailing bits, padding and lowercase. the reject reason is stellar-strkey's error and is informational; a testee passes by rejecting.

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `cap27.muxed-accounts.synth-strkey-m-invalid`
