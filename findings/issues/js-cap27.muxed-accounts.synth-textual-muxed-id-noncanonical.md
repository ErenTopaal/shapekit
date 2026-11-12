<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1, 17.3.0, 17.4.0: the sdk accepted a value the network refuses (cap27.muxed-accounts.synth-textual-muxed-id-noncanonical)

## What happens

`@stellar/stellar-sdk` 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1, 17.3.0, 17.4.0 answers 69 of the checks of corpus entry `cap27.muxed-accounts.synth-textual-muxed-id-noncanonical` differently from the Stellar network.

## Minimal vector

Entry `cap27.muxed-accounts.synth-textual-muxed-id-noncanonical` (kind TextualId, family `cap27.muxed-accounts`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `case 1/id_parse` on 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1

the sdk accepted a value the network refuses

```json
{
  "input": "0x10",
  "kind": "muxed",
  "op": "id_parse"
}
```

Expected: `"rejected"`

Actual: `{"id":"16"}`

### `case 2/id_parse` on 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1

the sdk accepted a value the network refuses

```json
{
  "input": "0X10",
  "kind": "muxed",
  "op": "id_parse"
}
```

Expected: `"rejected"`

Actual: `{"id":"16"}`

### `case 3/id_parse` on 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1

the sdk accepted a value the network refuses

```json
{
  "input": "0o20",
  "kind": "muxed",
  "op": "id_parse"
}
```

Expected: `"rejected"`

Actual: `{"id":"16"}`

### `case 4/id_parse` on 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1

the sdk accepted a value the network refuses

```json
{
  "input": "0b10000",
  "kind": "muxed",
  "op": "id_parse"
}
```

Expected: `"rejected"`

Actual: `{"id":"16"}`

### `case 5/id_parse` on 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1, 17.3.0, 17.4.0

the sdk accepted a value the network refuses

```json
{
  "input": "016",
  "kind": "muxed",
  "op": "id_parse"
}
```

Expected: `"rejected"`

Actual: `{"id":"16"}`

8 more failing checks are listed in `divergence-matrix.json`.

## Reproduce

```sh
shapekit conformance run --sdk js --version 16.3.0 --entry cap27.muxed-accounts.synth-textual-muxed-id-noncanonical
```

## Expected value and its source

- corpus/vectors/textual-id/cap27.muxed-accounts.synth-textual-muxed-id-noncanonical.json case 1
- corpus/vectors/textual-id/cap27.muxed-accounts.synth-textual-muxed-id-noncanonical.json case 10
- corpus/vectors/textual-id/cap27.muxed-accounts.synth-textual-muxed-id-noncanonical.json case 11
- corpus/vectors/textual-id/cap27.muxed-accounts.synth-textual-muxed-id-noncanonical.json case 12
- corpus/vectors/textual-id/cap27.muxed-accounts.synth-textual-muxed-id-noncanonical.json case 14
- corpus/vectors/textual-id/cap27.muxed-accounts.synth-textual-muxed-id-noncanonical.json case 2
- corpus/vectors/textual-id/cap27.muxed-accounts.synth-textual-muxed-id-noncanonical.json case 3
- corpus/vectors/textual-id/cap27.muxed-accounts.synth-textual-muxed-id-noncanonical.json case 4
- corpus/vectors/textual-id/cap27.muxed-accounts.synth-textual-muxed-id-noncanonical.json case 5
- corpus/vectors/textual-id/cap27.muxed-accounts.synth-textual-muxed-id-noncanonical.json case 6
- corpus/vectors/textual-id/cap27.muxed-accounts.synth-textual-muxed-id-noncanonical.json case 7
- corpus/vectors/textual-id/cap27.muxed-accounts.synth-textual-muxed-id-noncanonical.json case 8
- corpus/vectors/textual-id/cap27.muxed-accounts.synth-textual-muxed-id-noncanonical.json case 9

Entry notes: non-canonical or out-of-range muxed account ids (uint64, 0 to 2^64-1). accepted cases carry the M address and MuxedAccount XDR for key 0x3a..3a with that id. text is accepted only as the exact decimal rendering of an in-range value (ASCII digits, no sign, whitespace, leading zero, radix prefix, fraction or exponent); a JSON number is accepted as an integer in range with |n| <= 2^53-1, and refused when negative, fractional or out of range. in-range JSON numbers above 2^53-1 are in a separate implementation-defined entry. negative offer ids are refused because core answers MANAGE_SELL_OFFER_MALFORMED / MANAGE_BUY_OFFER_MALFORMED for offerID < 0 from protocol 15 (stellar-core v30.0.0 src/transactions/ManageOfferOpFrameBase.cpp, ManageOfferOpFrameBase::doCheckValid). the reject reason is informational; a testee passes by refusing.

Related: https://github.com/stellar/js-stellar-sdk/issues/1785

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `cap27.muxed-accounts.synth-textual-muxed-id-noncanonical`
