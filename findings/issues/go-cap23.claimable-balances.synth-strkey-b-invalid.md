<!-- repo: stellar/go-stellar-sdk -->
# github.com/stellar/go-stellar-sdk v0.7.1, v0.7.2, v0.7.3, v0.8.0, v0.9.0: the sdk accepted a value the network refuses (cap23.claimable-balances.synth-strkey-b-invalid)

## What happens

`github.com/stellar/go-stellar-sdk` v0.7.1, v0.7.2, v0.7.3, v0.8.0, v0.9.0 answers 12 of the checks of corpus entry `cap23.claimable-balances.synth-strkey-b-invalid` differently from the Stellar network.

## Minimal vector

Entry `cap23.claimable-balances.synth-strkey-b-invalid` (kind Strkey, family `cap23.claimable-balances`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `case 2/strkey_decode` on v0.7.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "BAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAZBO"
}
```

Expected: `"rejected"`

Actual: `{"kind":"claimable_balance","payload":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}`

### `case 3/strkey_decode` on v0.7.1

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "BAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAZMEQ"
}
```

Expected: `"rejected"`

Actual: `{"kind":"claimable_balance","payload":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="}`

### `case 13/strkey_decode` on v0.7.1, v0.7.2, v0.7.3, v0.8.0, v0.9.0

the sdk accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "BAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAADQFQ"
}
```

Expected: `"rejected"`

Actual: `{"kind":"claimable_balance","payload":"AQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}`

### `case 15/strkey_decode` on v0.7.1, v0.7.2, v0.7.3, v0.8.0, v0.9.0

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

```sh
shapekit conformance run --sdk go --version v0.7.1 --entry cap23.claimable-balances.synth-strkey-b-invalid
```

## Expected value and its source

- corpus/vectors/strkey/cap23.claimable-balances.synth-strkey-b-invalid.json case 13
- corpus/vectors/strkey/cap23.claimable-balances.synth-strkey-b-invalid.json case 15
- corpus/vectors/strkey/cap23.claimable-balances.synth-strkey-b-invalid.json case 2
- corpus/vectors/strkey/cap23.claimable-balances.synth-strkey-b-invalid.json case 3

Entry notes: invalid claimable balance ids (B, 1 << 3): a version byte (0 for v0) and 32 hash bytes. expectations are stellar-strkey 1.0.0's verdicts; SEP-23 1.4.0 says a strkey is valid only if re-encoding its binary gives back the same string, which rules out wrong lengths, set trailing bits, padding and lowercase. the reject reason is stellar-strkey's error and is informational; a testee passes by rejecting.

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `cap23.claimable-balances.synth-strkey-b-invalid`
