<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.3.0, 16.3.1, 16.4.1: the sdk accepted a value the network refuses (cap38.liquidity-pools.synth-strkey-l-invalid)

## What happens

On the input below, `@stellar/stellar-sdk` 16.3.0, 16.3.1, 16.4.1 answers differently from the Stellar network in 6 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap38.liquidity-pools.synth-strkey-l-invalid` (family `cap38.liquidity-pools`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `case 2/strkey_decode` on 16.3.0, 16.3.1, 16.4.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "LAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAMADI"
}
```

Expected: `"rejected"`

Actual: `{"kind":"liquidity_pool","payload":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="}`

### `case 3/strkey_decode` on 16.3.0, 16.3.1, 16.4.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "LAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAQV4"
}
```

Expected: `"rejected"`

Actual: `{"kind":"liquidity_pool","payload":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk js --version 16.3.0 --entry cap38.liquidity-pools.synth-strkey-l-invalid
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/vectors/strkey/cap38.liquidity-pools.synth-strkey-l-invalid.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/vectors/strkey/cap38.liquidity-pools.synth-strkey-l-invalid.json) cases 2, 3

How the corpus entry was built and checked: invalid liquidity pool ids (L, 11 << 3). expectations are stellar-strkey 1.0.0's verdicts; SEP-23 1.4.0 says a strkey is valid only if re-encoding its binary gives back the same string, which rules out wrong lengths, set trailing bits, padding and lowercase. the reject reason is stellar-strkey's error and is informational; a testee passes by rejecting.

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap38.liquidity-pools.synth-strkey-l-invalid`
