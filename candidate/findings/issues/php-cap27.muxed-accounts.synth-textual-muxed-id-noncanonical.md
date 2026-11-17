<!-- repo: Soneso/stellar-php-sdk -->
# soneso/stellar-php-sdk 1.18.0: the sdk accepted a value the network refuses (cap27.muxed-accounts.synth-textual-muxed-id-noncanonical)

## What happens

On the input below, `soneso/stellar-php-sdk` 1.18.0 answers differently from the Stellar network in 2 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap27.muxed-accounts.synth-textual-muxed-id-noncanonical` (family `cap27.muxed-accounts`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `case 18/id_parse` on 1.18.0

the sdk accepted a value the network refuses

```json
{
  "input": -1,
  "kind": "muxed",
  "op": "id_parse"
}
```

Expected: `"rejected"`

Actual: `{"id":"18446744073709551615"}`

### `case 19/id_parse` on 1.18.0

the sdk accepted a value the network refuses

```json
{
  "input": -16,
  "kind": "muxed",
  "op": "id_parse"
}
```

Expected: `"rejected"`

Actual: `{"id":"18446744073709551600"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk php --version 1.18.0 --entry cap27.muxed-accounts.synth-textual-muxed-id-noncanonical
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/vectors/textual-id/cap27.muxed-accounts.synth-textual-muxed-id-noncanonical.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/vectors/textual-id/cap27.muxed-accounts.synth-textual-muxed-id-noncanonical.json) cases 18, 19

How the corpus entry was built and checked: non-canonical or out-of-range muxed account ids (uint64, 0 to 2^64-1). accepted cases carry the M address and MuxedAccount XDR for key 0x3a..3a with that id. text is canonical only as the exact decimal rendering of an in-range value, and refused with a sign (including -0), whitespace, a radix prefix, an exponent, non-ASCII digits, when empty or out of range; a JSON number is canonical as an integer in range with |n| <= 2^53-1, and refused when negative, non-integral or out of range. leading-zero text, integral numbers written with .0 and in-range numbers above 2^53-1 are in a separate implementation-defined entry. the reject reason is informational; a testee passes by refusing.

Related: https://github.com/stellar/js-stellar-sdk/issues/1785

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap27.muxed-accounts.synth-textual-muxed-id-noncanonical`
