<!-- repo: stellar/go-stellar-sdk -->
# github.com/stellar/go-stellar-sdk v0.7.1: the sdk accepted a value the network refuses (soroban.invoke-basics.synth-strkey-c-invalid)

## What happens

On the input below, `github.com/stellar/go-stellar-sdk` v0.7.1 answers differently from the Stellar network in 2 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `soroban.invoke-basics.synth-strkey-c-invalid` (family `soroban.invoke-basics`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

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

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk go --version v0.7.1 --entry soroban.invoke-basics.synth-strkey-c-invalid
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/vectors/strkey/soroban.invoke-basics.synth-strkey-c-invalid.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/vectors/strkey/soroban.invoke-basics.synth-strkey-c-invalid.json) cases 2, 3

How the corpus entry was built and checked: invalid contract ids (C, 2 << 3). expectations are stellar-strkey 1.0.0's verdicts; SEP-23 1.4.0 says a strkey is valid only if re-encoding its binary gives back the same string, which rules out wrong lengths, set trailing bits, padding and lowercase. the reject reason is stellar-strkey's error and is informational; a testee passes by rejecting.

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `soroban.invoke-basics.synth-strkey-c-invalid`
