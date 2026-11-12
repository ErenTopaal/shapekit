<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.3.0, 16.3.1, 16.4.1: the sdk accepted a value the network refuses (cap40.signed-payload-signer.synth-strkey-p-invalid)

## What happens

`@stellar/stellar-sdk` 16.3.0, 16.3.1, 16.4.1 answers 33 of the checks of corpus entry `cap40.signed-payload-signer.synth-strkey-p-invalid` differently from the Stellar network.

## Minimal vector

Entry `cap40.signed-payload-signer.synth-strkey-p-invalid` (kind Strkey, family `cap40.signed-payload-signer`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `case 2/strkey_decode` on 16.3.0, 16.3.1, 16.4.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "PA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAQCAAAVOZQ"
}
```

Expected: `"rejected"`

Actual: `{"kind":"signed_payload","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6JoAAAABAQAA"}`

### `case 3/strkey_decode` on 16.3.0, 16.3.1, 16.4.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "PA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAQCAAAAAAN6TY"
}
```

Expected: `"rejected"`

Actual: `{"kind":"signed_payload","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6JoAAAABAQAAAAA="}`

### `case 13/strkey_decode` on 16.3.0, 16.3.1, 16.4.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "PA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAKH4Y"
}
```

Expected: `"rejected"`

Actual: `{"kind":"signed_payload","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6JoAAAAA"}`

### `case 14/strkey_decode` on 16.3.0, 16.3.1, 16.4.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "PA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAABAQCAQDAQCQMBYIBEFAWDANBYHRAEISCMKBKFQXDAMRUGY4DUPB6IBBEIRSIJJGE4UCSKRLFQWS4LZQGEZDGNBVGY3TQOJ2HM6D2PR7IBAQAAAARVTQ"
}
```

Expected: `"rejected"`

Actual: `{"kind":"signed_payload","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6JoAAABBAQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyAhIiMkJSYnKCkqKywtLi8wMTIzNDU2Nzg5Ojs8PT4/QEEAAAA="}`

### `case 15/strkey_decode` on 16.3.0, 16.3.1, 16.4.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "PA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAACACAQDAQCQAAAABHMA"
}
```

Expected: `"rejected"`

Actual: `{"kind":"signed_payload","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6JoAAAAEAQIDBAUAAAA="}`

6 more failing checks are listed in `divergence-matrix.json`.

## Reproduce

```sh
shapekit conformance run --sdk js --version 16.3.0 --entry cap40.signed-payload-signer.synth-strkey-p-invalid
```

## Expected value and its source

- corpus/vectors/strkey/cap40.signed-payload-signer.synth-strkey-p-invalid.json case 13
- corpus/vectors/strkey/cap40.signed-payload-signer.synth-strkey-p-invalid.json case 14
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
