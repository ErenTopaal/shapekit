<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1: the sdk reads a different id; it must read this one exactly or refuse (cap03.offer-liabilities.synth-textual-offer-id-unsafe-number)

## What happens

`@stellar/stellar-sdk` 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1 answers 5 of the checks of corpus entry `cap03.offer-liabilities.synth-textual-offer-id-unsafe-number` differently from the Stellar network.

## Minimal vector

Entry `cap03.offer-liabilities.synth-textual-offer-id-unsafe-number` (kind TextualId, family `cap03.offer-liabilities`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `case 1/id_parse` on 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1

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

```sh
shapekit conformance run --sdk js --version 16.3.0 --entry cap03.offer-liabilities.synth-textual-offer-id-unsafe-number
```

## Expected value and its source

- corpus/vectors/textual-id/cap03.offer-liabilities.synth-textual-offer-id-unsafe-number.json case 1

Entry notes: in-range offer ids (int64 in XDR, 0 to 2^63-1 because core refuses negative ids from protocol 15). accepted cases carry a ManageSellOfferOp with that offer id, given as JSON numbers above 2^53-1. this bound is not a network rule but a JSON interoperability limit: RFC 8259 section 6 says integers outside [-(2^53)+1, 2^53-1] are not interoperable because implementations commonly hold numbers as IEEE-754 doubles. js-stellar-sdk#1785 is that failure: a number above 2^53-1 lost precision and the operation encoded a different id. each case expects either the exact written id or a refusal (unsafe-integer); encoding any other id fails.

Related: https://github.com/stellar/js-stellar-sdk/issues/1785, https://www.rfc-editor.org/rfc/rfc8259#section-6

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `cap03.offer-liabilities.synth-textual-offer-id-unsafe-number`
