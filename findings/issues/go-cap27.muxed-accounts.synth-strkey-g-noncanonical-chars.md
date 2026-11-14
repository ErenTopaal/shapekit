<!-- repo: stellar/go-stellar-sdk -->
# github.com/stellar/go-stellar-sdk v0.7.1, v0.7.2, v0.7.3, v0.8.0: the sdk accepted a value the network refuses (cap27.muxed-accounts.synth-strkey-g-noncanonical-chars)

## What happens

On the input below, `github.com/stellar/go-stellar-sdk` v0.7.1, v0.7.2, v0.7.3, v0.8.0 answers differently from the Stellar network in 12 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap27.muxed-accounts.synth-strkey-g-noncanonical-chars` (family `cap27.muxed-accounts`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `case 0/strkey_decode` on v0.7.1, v0.7.2, v0.7.3, v0.8.0

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF\n\n\n\n\n\n\n\n"
}
```

Expected: `"rejected"`

Actual: `{"kind":"account","payload":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}`

### `case 1/strkey_decode` on v0.7.1, v0.7.2, v0.7.3, v0.8.0

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "\r\r\r\r\r\r\r\rGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF"
}
```

Expected: `"rejected"`

Actual: `{"kind":"account","payload":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}`

### `case 2/strkey_decode` on v0.7.1, v0.7.2, v0.7.3, v0.8.0

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "GAAAAAAAAAAAAAAAAAAAAAAAAAAA\r\n\r\n\r\n\r\nAAAAAAAAAAAAAAAAAAAAAAAAAWHF"
}
```

Expected: `"rejected"`

Actual: `{"kind":"account","payload":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk go --version v0.7.1 --entry cap27.muxed-accounts.synth-strkey-g-noncanonical-chars
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/vectors/strkey/cap27.muxed-accounts.synth-strkey-g-noncanonical-chars.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/vectors/strkey/cap27.muxed-accounts.synth-strkey-g-noncanonical-chars.json) cases 0, 1, 2

How the corpus entry was built and checked: ed25519 account public keys (G, 6 << 3), each with carriage returns, newlines or 0xff bytes inserted. SEP-23 1.4.0 (Specification, step 6 and the paragraph after it) defines a strkey as RFC 4648 base32 without padding and valid only if re-encoding its binary yields the exact same string, and RFC 4648 section 3.3 says a decoder must reject characters outside the base alphabet unless the referring specification says otherwise, which SEP-23 does not. so every case must be rejected; stellar-strkey 1.0.0 rejects each one. go-stellar-sdk before v0.9.0 decoded the runs of these bytes to the payload of the strkey they were inserted into, because go's encoding/base32 skips carriage returns and newlines and its unpadded decoder reads 0xff as padding; v0.9.0 rejects any byte outside the alphabet first (go-stellar-sdk#6026). cases that are not utf-8 carry the text as text_base64.

Related: https://github.com/stellar/go-stellar-sdk/pull/6026

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap27.muxed-accounts.synth-strkey-g-noncanonical-chars`
