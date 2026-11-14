<!-- repo: stellar/go-stellar-sdk -->
# github.com/stellar/go-stellar-sdk v0.7.1: the sdk accepted a value the network refuses (cap27.muxed-accounts.synth-strkey-m-invalid)

## What happens

On the input below, `github.com/stellar/go-stellar-sdk` v0.7.1 answers differently from the Stellar network in 4 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap27.muxed-accounts.synth-strkey-m-invalid` (family `cap27.muxed-accounts`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

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

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk go --version v0.7.1 --entry cap27.muxed-accounts.synth-strkey-m-invalid
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/vectors/strkey/cap27.muxed-accounts.synth-strkey-m-invalid.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/vectors/strkey/cap27.muxed-accounts.synth-strkey-m-invalid.json) cases 2, 3, 13, 16

How the corpus entry was built and checked: invalid muxed accounts (M, 12 << 3, CAP-27): 32 key bytes then a big-endian u64 id. expectations are stellar-strkey 1.0.0's verdicts; SEP-23 1.4.0 says a strkey is valid only if re-encoding its binary gives back the same string, which rules out wrong lengths, set trailing bits, padding and lowercase. the reject reason is stellar-strkey's error and is informational; a testee passes by rejecting.

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap27.muxed-accounts.synth-strkey-m-invalid`
