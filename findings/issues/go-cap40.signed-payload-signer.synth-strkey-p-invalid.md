<!-- repo: stellar/go-stellar-sdk -->
# github.com/stellar/go-stellar-sdk v0.7.1: the sdk accepted a value the network refuses (cap40.signed-payload-signer.synth-strkey-p-invalid)

## What happens

`github.com/stellar/go-stellar-sdk` v0.7.1 answers 10 of the checks of corpus entry `cap40.signed-payload-signer.synth-strkey-p-invalid` differently from the Stellar network.

## Minimal vector

Entry `cap40.signed-payload-signer.synth-strkey-p-invalid` (kind Strkey, family `cap40.signed-payload-signer`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `case 2/strkey_decode` on v0.7.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "PA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAQCAAAVOZQ"
}
```

Expected: `"rejected"`

Actual: `{"kind":"signed_payload","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6JoAAAABAQAA"}`

### `case 3/strkey_decode` on v0.7.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "PA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAQCAAAAAAN6TY"
}
```

Expected: `"rejected"`

Actual: `{"kind":"signed_payload","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6JoAAAABAQAAAAA="}`

### `case 13/strkey_decode` on v0.7.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "PA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAKH4Y"
}
```

Expected: `"rejected"`

Actual: `{"kind":"signed_payload","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6JoAAAAA"}`

### `case 15/strkey_decode` on v0.7.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "PA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAACACAQDAQCQAAAABHMA"
}
```

Expected: `"rejected"`

Actual: `{"kind":"signed_payload","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6JoAAAAEAQIDBAUAAAA="}`

### `case 16/strkey_decode` on v0.7.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "PA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAEQCAQDAQCQAAAAHKDA"
}
```

Expected: `"rejected"`

Actual: `{"kind":"signed_payload","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6JoAAAAJAQIDBAUAAAA="}`

5 more failing checks are listed in `divergence-matrix.json`.

## Reproduce

```sh
shapekit conformance run --sdk go --version v0.7.1 --entry cap40.signed-payload-signer.synth-strkey-p-invalid
```

## Expected value and its source

- corpus/vectors/strkey/cap40.signed-payload-signer.synth-strkey-p-invalid.json case 13
- corpus/vectors/strkey/cap40.signed-payload-signer.synth-strkey-p-invalid.json case 15
- corpus/vectors/strkey/cap40.signed-payload-signer.synth-strkey-p-invalid.json case 16
- corpus/vectors/strkey/cap40.signed-payload-signer.synth-strkey-p-invalid.json case 17
- corpus/vectors/strkey/cap40.signed-payload-signer.synth-strkey-p-invalid.json case 18
- corpus/vectors/strkey/cap40.signed-payload-signer.synth-strkey-p-invalid.json case 19
- corpus/vectors/strkey/cap40.signed-payload-signer.synth-strkey-p-invalid.json case 2
- corpus/vectors/strkey/cap40.signed-payload-signer.synth-strkey-p-invalid.json case 20
- corpus/vectors/strkey/cap40.signed-payload-signer.synth-strkey-p-invalid.json case 21
- corpus/vectors/strkey/cap40.signed-payload-signer.synth-strkey-p-invalid.json case 3

Entry notes: invalid ed25519 signed-payload signers (P, 15 << 3, CAP-40): 32 key bytes, a u32 length, 1 to 64 payload bytes and zero padding to a multiple of four. expectations are stellar-strkey 1.0.0's verdicts; SEP-23 1.4.0 says a strkey is valid only if re-encoding its binary gives back the same string, which rules out wrong lengths, set trailing bits, padding and lowercase. the reject reason is stellar-strkey's error and is informational; a testee passes by rejecting.

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `cap40.signed-payload-signer.synth-strkey-p-invalid`
