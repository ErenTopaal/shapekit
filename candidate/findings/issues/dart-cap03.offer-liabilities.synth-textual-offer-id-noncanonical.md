<!-- repo: Soneso/stellar_flutter_sdk -->
# stellar_flutter_sdk 3.12.0: the sdk accepted a value the network refuses (cap03.offer-liabilities.synth-textual-offer-id-noncanonical)

## What happens

On the input below, `stellar_flutter_sdk` 3.12.0 answers differently from the Stellar network in 10 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap03.offer-liabilities.synth-textual-offer-id-noncanonical` (family `cap03.offer-liabilities`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `case 0/id_parse` on 3.12.0

the sdk accepted a value the network refuses

```json
{
  "input": "9223372036854775808",
  "kind": "offer",
  "op": "id_parse"
}
```

Expected: `"rejected"`

Actual: `{"id":"-9223372036854775808"}`

### `case 1/id_parse` on 3.12.0

the sdk accepted a value the network refuses

```json
{
  "input": "18446744073709551615",
  "kind": "offer",
  "op": "id_parse"
}
```

Expected: `"rejected"`

Actual: `{"id":"-1"}`

### `case 3/id_parse` on 3.12.0

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

### `case 4/id_parse` on 3.12.0

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

### `case 7/id_parse` on 3.12.0

the sdk accepted a value the network refuses

```json
{
  "input": " 16 ",
  "kind": "offer",
  "op": "id_parse"
}
```

Expected: `"rejected"`

Actual: `{"id":"16"}`

5 more checks fail; the command below lists every one.

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk dart --version 3.12.0 --entry cap03.offer-liabilities.synth-textual-offer-id-noncanonical
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-noncanonical.json) cases 0, 1, 3, 4, 7, 8, 9, 10, 12, 13

How the corpus entry was built and checked: non-canonical or out-of-range offer ids (int64 in XDR; a valid operation carries 0 to 2^63-1 because core refuses negative ids from protocol 15). accepted cases carry a ManageSellOfferOp with that offer id. text is canonical only as the exact decimal rendering of an in-range value, and refused with a plus sign, a minus sign on zero (-0), whitespace, a radix prefix, an exponent, non-ASCII digits, when empty or out of range; a JSON number is canonical as an integer in range with |n| <= 2^53-1, and refused when non-integral or out of range. leading-zero text, integral numbers written with .0 and in-range numbers above 2^53-1 are in a separate implementation-defined entry. negative offer ids in their canonical int64 spelling (-16 as text or as a JSON number) are in a separate entry, because XDR and SEP-51 XDR-JSON allow them and core refuses them only when it checks the operation. the reject reason is informational; a testee passes by refusing.

Related: https://github.com/stellar/js-stellar-sdk/issues/1785

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap03.offer-liabilities.synth-textual-offer-id-noncanonical`
