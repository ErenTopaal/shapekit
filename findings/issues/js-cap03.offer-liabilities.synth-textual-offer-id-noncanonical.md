<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1, 17.3.0, 17.4.0: the sdk accepted a value the network refuses (cap03.offer-liabilities.synth-textual-offer-id-noncanonical)

## What happens

`@stellar/stellar-sdk` 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1, 17.3.0, 17.4.0 answers 101 of the checks of corpus entry `cap03.offer-liabilities.synth-textual-offer-id-noncanonical` differently from the Stellar network.

## Minimal vector

Entry `cap03.offer-liabilities.synth-textual-offer-id-noncanonical` (kind TextualId, family `cap03.offer-liabilities`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `case 3/id_parse` on 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1

the sdk accepted a value the network refuses

```json
{
  "input": "0x10",
  "kind": "offer",
  "op": "id_parse"
}
```

Expected: `"rejected"`

Actual: `{"id":"16"}`

### `case 4/id_parse` on 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1

the sdk accepted a value the network refuses

```json
{
  "input": "0X10",
  "kind": "offer",
  "op": "id_parse"
}
```

Expected: `"rejected"`

Actual: `{"id":"16"}`

### `case 5/id_parse` on 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1

the sdk accepted a value the network refuses

```json
{
  "input": "0o20",
  "kind": "offer",
  "op": "id_parse"
}
```

Expected: `"rejected"`

Actual: `{"id":"16"}`

### `case 6/id_parse` on 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1

the sdk accepted a value the network refuses

```json
{
  "input": "0b10000",
  "kind": "offer",
  "op": "id_parse"
}
```

Expected: `"rejected"`

Actual: `{"id":"16"}`

### `case 7/id_parse` on 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1, 17.3.0, 17.4.0

the sdk accepted a value the network refuses

```json
{
  "input": "016",
  "kind": "offer",
  "op": "id_parse"
}
```

Expected: `"rejected"`

Actual: `{"id":"16"}`

14 more failing checks are listed in `divergence-matrix.json`.

## Reproduce

```sh
shapekit conformance run --sdk js --version 16.3.0 --entry cap03.offer-liabilities.synth-textual-offer-id-noncanonical
```

## Expected value and its source

- corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json case 10
- corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json case 11
- corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json case 12
- corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json case 13
- corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json case 14
- corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json case 15
- corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json case 16
- corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json case 22
- corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json case 23
- corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json case 25
- corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json case 26
- corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json case 28
- corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json case 3
- corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json case 4
- corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json case 5
- corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json case 6
- corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json case 7
- corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json case 8
- corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json case 9

Entry notes: non-canonical or out-of-range offer ids (int64 in XDR, 0 to 2^63-1 because core refuses negative ids from protocol 15). accepted cases carry a ManageSellOfferOp with that offer id. text is accepted only as the exact decimal rendering of an in-range value (ASCII digits, no sign, whitespace, leading zero, radix prefix, fraction or exponent); a JSON number is accepted as an integer in range with |n| <= 2^53-1, and refused when negative, fractional or out of range. in-range JSON numbers above 2^53-1 are in a separate implementation-defined entry. negative offer ids are refused because core answers MANAGE_SELL_OFFER_MALFORMED / MANAGE_BUY_OFFER_MALFORMED for offerID < 0 from protocol 15 (stellar-core v30.0.0 src/transactions/ManageOfferOpFrameBase.cpp, ManageOfferOpFrameBase::doCheckValid). the reject reason is informational; a testee passes by refusing.

Related: https://github.com/stellar/js-stellar-sdk/issues/1785

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `cap03.offer-liabilities.synth-textual-offer-id-noncanonical`
