<!-- repo: Beans-BV/dotnet-stellar-sdk -->
# stellar-dotnet-sdk 16.0.0, 17.0.0: the sdk accepted a value the network refuses (cap23.claimable-balances.synth-strkey-b-invalid)

## What happens

On the input below, `stellar-dotnet-sdk` 16.0.0, 17.0.0 answers differently from the Stellar network in 4 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap23.claimable-balances.synth-strkey-b-invalid` (family `cap23.claimable-balances`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `case 13/strkey_decode` on 16.0.0, 17.0.0

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "BAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAADQFQ"
}
```

Expected: `"rejected"`

Actual: `{"kind":"claimable_balance","payload":"AQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}`

### `case 15/strkey_decode` on 16.0.0, 17.0.0

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "BAAT6DBUX6J22DMZOHIEZTEQ64CVCHEDRKWZONFEUL5Q26QD7R76RGXACA"
}
```

Expected: `"rejected"`

Actual: `{"kind":"claimable_balance","payload":"AT8MNL+TrQ2ZcdBMzJD3BVEcg4qtlzSkovsNegP8f+ia"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk dotnet --version 16.0.0 --entry cap23.claimable-balances.synth-strkey-b-invalid
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/vectors/strkey/cap23.claimable-balances.synth-strkey-b-invalid.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/vectors/strkey/cap23.claimable-balances.synth-strkey-b-invalid.json) cases 13, 15

How the corpus entry was built and checked: invalid claimable balance ids (B, 1 << 3): a version byte (0 for v0) and 32 hash bytes. expectations are stellar-strkey 1.0.0's verdicts; SEP-23 1.4.0 says a strkey is valid only if re-encoding its binary gives back the same string, which rules out wrong lengths, set trailing bits, padding and lowercase. the reject reason is stellar-strkey's error and is informational; a testee passes by rejecting.

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap23.claimable-balances.synth-strkey-b-invalid`
