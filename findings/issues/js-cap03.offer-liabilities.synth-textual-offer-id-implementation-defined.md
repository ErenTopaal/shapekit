<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1: the sdk reads a different id; it must read this one exactly or refuse (cap03.offer-liabilities.synth-textual-offer-id-implementation-defined)

## What happens

On the input below, `@stellar/stellar-sdk` 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1 answers differently from the Stellar network in 5 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap03.offer-liabilities.synth-textual-offer-id-implementation-defined` (family `cap03.offer-liabilities`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `case 3/id_parse` on 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1

the sdk reads a different id; it must read this one exactly or refuse

```json
{
  "input": 9007199254740993,
  "kind": "offer",
  "op": "id_parse"
}
```

Expected: `{"either":[{"id":"9007199254740993"},"rejected"]}`

Actual: `{"id":"9007199254740992"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk js --version 16.3.0 --entry cap03.offer-liabilities.synth-textual-offer-id-implementation-defined
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-implementation-defined.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-implementation-defined.json) case 3

How the corpus entry was built and checked: in-range offer ids (int64 in XDR, 0 to 2^63-1 because core refuses negative ids from protocol 15). accepted cases carry a ManageSellOfferOp with that offer id, in forms that name exactly one id but are not its canonical spelling, so a decoder may accept them or refuse them. (1) decimal text with leading zeros (016, 00): js-stellar-sdk 17.3.0, the release that fixed js-stellar-sdk#1785, deliberately accepts these and its JSDoc for the muxed and offer id parameters says so, because they encode the id the caller wrote. (2) integral JSON numbers written with a zero fraction (16.0), which RFC 8259 treats as the same number. (3) JSON numbers above 2^53-1: not a network rule but a JSON interoperability limit, since RFC 8259 section 6 says integers outside [-(2^53)+1, 2^53-1] are not interoperable when implementations hold numbers as IEEE-754 doubles; js-stellar-sdk#1785 is that failure, where such a number lost precision and a different id was encoded. each case expects either the exact id or a refusal (leading-zero, integral-fraction or unsafe-integer); encoding any other id fails.

Related: https://github.com/stellar/js-stellar-sdk/issues/1785, https://www.rfc-editor.org/rfc/rfc8259#section-6

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap03.offer-liabilities.synth-textual-offer-id-implementation-defined`
