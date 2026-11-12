<!-- repo: stellar/go-stellar-sdk -->
# github.com/stellar/go-stellar-sdk v0.7.1: the sdk accepted a value the network refuses (soroban.invoke-basics.synth-strkey-c-invalid)

## What happens

`github.com/stellar/go-stellar-sdk` v0.7.1 answers 2 of the checks of corpus entry `soroban.invoke-basics.synth-strkey-c-invalid` differently from the Stellar network.

## Minimal vector

Entry `soroban.invoke-basics.synth-strkey-c-invalid` (kind Strkey, family `soroban.invoke-basics`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `case 2/strkey_decode` on v0.7.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAMLRA"
}
```

Expected: `"rejected"`

Actual: `{"kind":"contract","payload":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="}`

### `case 3/strkey_decode` on v0.7.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAADZKM"
}
```

Expected: `"rejected"`

Actual: `{"kind":"contract","payload":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}`

## Reproduce

```sh
shapekit conformance run --sdk go --version v0.7.1 --entry soroban.invoke-basics.synth-strkey-c-invalid
```

## Expected value and its source

- corpus/vectors/strkey/soroban.invoke-basics.synth-strkey-c-invalid.json case 2
- corpus/vectors/strkey/soroban.invoke-basics.synth-strkey-c-invalid.json case 3

Entry notes: invalid contract ids (C, 2 << 3). expectations are stellar-strkey 1.0.0's verdicts; SEP-23 1.4.0 says a strkey is valid only if re-encoding its binary gives back the same string, which rules out wrong lengths, set trailing bits, padding and lowercase. the reject reason is stellar-strkey's error and is informational; a testee passes by rejecting.

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.invoke-basics.synth-strkey-c-invalid`
