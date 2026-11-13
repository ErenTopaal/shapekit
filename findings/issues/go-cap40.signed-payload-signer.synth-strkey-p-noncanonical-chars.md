<!-- repo: stellar/go-stellar-sdk -->
# github.com/stellar/go-stellar-sdk v0.7.1, v0.7.2, v0.7.3, v0.8.0: the sdk accepted a value the network refuses (cap40.signed-payload-signer.synth-strkey-p-noncanonical-chars)

## What happens

`github.com/stellar/go-stellar-sdk` v0.7.1, v0.7.2, v0.7.3, v0.8.0 answers 16 of the checks of corpus entry `cap40.signed-payload-signer.synth-strkey-p-noncanonical-chars` differently from the Stellar network.

## Minimal vector

Entry `cap40.signed-payload-signer.synth-strkey-p-noncanonical-chars` (kind Strkey, family `cap40.signed-payload-signer`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `case 0/strkey_decode` on v0.7.1, v0.7.2, v0.7.3, v0.8.0

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "PA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAQCAAAAC4DY\n\n\n"
}
```

Expected: `"rejected"`

Actual: `{"kind":"signed_payload","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6JoAAAABAQAAAA=="}`

### `case 1/strkey_decode` on v0.7.1, v0.7.2, v0.7.3, v0.8.0

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "\r\r\rPA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAQCAAAAC4DY"
}
```

Expected: `"rejected"`

Actual: `{"kind":"signed_payload","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6JoAAAABAQAAAA=="}`

### `case 2/strkey_decode` on v0.7.1, v0.7.2, v0.7.3, v0.8.0

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "PA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVW\r\n\rLTJJFC7MGXUA74P7UJUAAAAAAQCAAAAC4DY"
}
```

Expected: `"rejected"`

Actual: `{"kind":"signed_payload","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6JoAAAABAQAAAA=="}`

### `case 4/strkey_decode` on v0.7.1, v0.7.2, v0.7.3, v0.8.0

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text_base64": "UEE3UVlORjdTT1dRM0dMUjJCR01aRUhYQVZJUlpBNEtWV0xUSkpGQzdNR1hVQTc0UDdVSlVBQUFBQUFRQ0FBQUFDNERZ////"
}
```

Expected: `"rejected"`

Actual: `{"kind":"signed_payload","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6JoAAAABAQAAAA=="}`

## Reproduce

```sh
shapekit conformance run --sdk go --version v0.7.1 --entry cap40.signed-payload-signer.synth-strkey-p-noncanonical-chars
```

## Expected value and its source

- corpus/vectors/strkey/cap40.signed-payload-signer.synth-strkey-p-noncanonical-chars.json case 0
- corpus/vectors/strkey/cap40.signed-payload-signer.synth-strkey-p-noncanonical-chars.json case 1
- corpus/vectors/strkey/cap40.signed-payload-signer.synth-strkey-p-noncanonical-chars.json case 2
- corpus/vectors/strkey/cap40.signed-payload-signer.synth-strkey-p-noncanonical-chars.json case 4

Entry notes: ed25519 signed-payload signers (P, 15 << 3, CAP-40): 32 key bytes, a u32 length, 1 to 64 payload bytes and zero padding to a multiple of four, each with carriage returns, newlines or 0xff bytes inserted. SEP-23 1.4.0 (Specification, step 6 and the paragraph after it) defines a strkey as RFC 4648 base32 without padding and valid only if re-encoding its binary yields the exact same string, and RFC 4648 section 3.3 says a decoder must reject characters outside the base alphabet unless the referring specification says otherwise, which SEP-23 does not. so every case must be rejected; stellar-strkey 1.0.0 rejects each one. go-stellar-sdk before v0.9.0 decoded the runs of these bytes to the payload of the strkey they were inserted into, because go's encoding/base32 skips carriage returns and newlines and its unpadded decoder reads 0xff as padding; v0.9.0 rejects any byte outside the alphabet first (go-stellar-sdk#6026). cases that are not utf-8 carry the text as text_base64.

Related: https://github.com/stellar/go-stellar-sdk/pull/6026

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `cap40.signed-payload-signer.synth-strkey-p-noncanonical-chars`
