<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1: the sdk accepted a value the network refuses (cap27.muxed-accounts.synth-textual-muxed-id-noncanonical)

## What happens

On the input below, `@stellar/stellar-sdk` 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1 answers differently from the Stellar network in 55 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap27.muxed-accounts.synth-textual-muxed-id-noncanonical` (family `cap27.muxed-accounts`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

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

### `case 5/id_parse` on 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1

the sdk accepted a value the network refuses

```json
{
  "input": " 16 ",
  "kind": "muxed",
  "op": "id_parse"
}
```

Expected: `"rejected"`

Actual: `{"id":"16"}`

6 more checks fail; the command below lists every one.

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk js --version 16.3.0 --entry cap27.muxed-accounts.synth-textual-muxed-id-noncanonical
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/vectors/textual-id/cap27.muxed-accounts.synth-textual-muxed-id-noncanonical.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/vectors/textual-id/cap27.muxed-accounts.synth-textual-muxed-id-noncanonical.json) cases 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12

How the corpus entry was built and checked: non-canonical or out-of-range muxed account ids (uint64, 0 to 2^64-1). accepted cases carry the M address and MuxedAccount XDR for key 0x3a..3a with that id. text is canonical only as the exact decimal rendering of an in-range value, and refused with a sign (including -0), whitespace, a radix prefix, an exponent, non-ASCII digits, when empty or out of range; a JSON number is canonical as an integer in range with |n| <= 2^53-1, and refused when negative, non-integral or out of range. leading-zero text, integral numbers written with .0 and in-range numbers above 2^53-1 are in a separate implementation-defined entry. negative offer ids are refused because core answers MANAGE_SELL_OFFER_MALFORMED / MANAGE_BUY_OFFER_MALFORMED for offerID < 0 from protocol 15 (stellar-core v30.0.0 src/transactions/ManageOfferOpFrameBase.cpp, ManageOfferOpFrameBase::doCheckValid). the reject reason is informational; a testee passes by refusing.

Related: https://github.com/stellar/js-stellar-sdk/issues/1785

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap27.muxed-accounts.synth-textual-muxed-id-noncanonical`
