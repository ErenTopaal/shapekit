<!-- repo: stellar/go-stellar-sdk -->
# github.com/stellar/go-stellar-sdk v0.7.1: the sdk accepted a value the network refuses (cap40.signed-payload-signer.synth-strkey-x-invalid)

## What happens

`github.com/stellar/go-stellar-sdk` v0.7.1 answers 2 of the checks of corpus entry `cap40.signed-payload-signer.synth-strkey-x-invalid` differently from the Stellar network.

## Minimal vector

Entry `cap40.signed-payload-signer.synth-strkey-x-invalid` (kind Strkey, family `cap40.signed-payload-signer`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `case 2/strkey_decode` on v0.7.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "XAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAPF4I"
}
```

Expected: `"rejected"`

Actual: `{"kind":"hash_x","payload":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="}`

### `case 3/strkey_decode` on v0.7.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "XAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFN54"
}
```

Expected: `"rejected"`

Actual: `{"kind":"hash_x","payload":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}`

## Reproduce

```sh
shapekit conformance run --sdk go --version v0.7.1 --entry cap40.signed-payload-signer.synth-strkey-x-invalid
```

## Expected value and its source

- corpus/vectors/strkey/cap40.signed-payload-signer.synth-strkey-x-invalid.json case 2
- corpus/vectors/strkey/cap40.signed-payload-signer.synth-strkey-x-invalid.json case 3

Entry notes: invalid hash-x signers (X, 23 << 3). expectations are stellar-strkey 1.0.0's verdicts; SEP-23 1.4.0 says a strkey is valid only if re-encoding its binary gives back the same string, which rules out wrong lengths, set trailing bits, padding and lowercase. the reject reason is stellar-strkey's error and is informational; a testee passes by rejecting.

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `cap40.signed-payload-signer.synth-strkey-x-invalid`
