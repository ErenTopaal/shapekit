<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.3.0, 16.3.1, 16.4.1: the sdk accepted a value the network refuses (cap40.signed-payload-signer.synth-strkey-t-invalid)

## What happens

`@stellar/stellar-sdk` 16.3.0, 16.3.1, 16.4.1 answers 6 of the checks of corpus entry `cap40.signed-payload-signer.synth-strkey-t-invalid` differently from the Stellar network.

## Minimal vector

Entry `cap40.signed-payload-signer.synth-strkey-t-invalid` (kind Strkey, family `cap40.signed-payload-signer`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `case 2/strkey_decode` on 16.3.0, 16.3.1, 16.4.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "TAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFS2A"
}
```

Expected: `"rejected"`

Actual: `{"kind":"pre_auth_tx","payload":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="}`

### `case 3/strkey_decode` on 16.3.0, 16.3.1, 16.4.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "TAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAC7JE"
}
```

Expected: `"rejected"`

Actual: `{"kind":"pre_auth_tx","payload":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}`

## Reproduce

```sh
shapekit conformance run --sdk js --version 16.3.0 --entry cap40.signed-payload-signer.synth-strkey-t-invalid
```

## Expected value and its source

- corpus/vectors/strkey/cap40.signed-payload-signer.synth-strkey-t-invalid.json case 2
- corpus/vectors/strkey/cap40.signed-payload-signer.synth-strkey-t-invalid.json case 3

Entry notes: invalid pre-authorized transaction hashes (T, 19 << 3). expectations are stellar-strkey 1.0.0's verdicts; SEP-23 1.4.0 says a strkey is valid only if re-encoding its binary gives back the same string, which rules out wrong lengths, set trailing bits, padding and lowercase. the reject reason is stellar-strkey's error and is informational; a testee passes by rejecting.

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `cap40.signed-payload-signer.synth-strkey-t-invalid`
