<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.4.1: the sdk refused a value the network accepts: Error: StrKey.decodeMuxedContract does not exist in this sdk version (cap84.muxed-contract.synth-strkey-w-valid)

## What happens

On the input below, `@stellar/stellar-sdk` 16.4.1 answers differently from the Stellar network in 7 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap84.muxed-contract.synth-strkey-w-valid` (family `cap84.muxed-contract`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `case 0/strkey_decode` on 16.4.1

the sdk refused a value the network accepts: Error: StrKey.decodeMuxedContract does not exist in this sdk version

```json
{
  "op": "strkey_decode",
  "text": "WA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAAAAAAAAWWC"
}
```

Expected: `{"kind":"muxed_contract","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6JoAAAAAAAAAAA=="}`

Actual: `{"rejected":"Error: StrKey.decodeMuxedContract does not exist in this sdk version"}`

### `case 1/strkey_decode` on 16.4.1

the sdk refused a value the network accepts: Error: StrKey.decodeMuxedContract does not exist in this sdk version

```json
{
  "op": "strkey_decode",
  "text": "WA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAAAAAAAEGHC"
}
```

Expected: `{"kind":"muxed_contract","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6JoAAAAAAAAAAQ=="}`

Actual: `{"rejected":"Error: StrKey.decodeMuxedContract does not exist in this sdk version"}`

### `case 2/strkey_decode` on 16.4.1

the sdk refused a value the network accepts: Error: StrKey.decodeMuxedContract does not exist in this sdk version

```json
{
  "op": "strkey_decode",
  "text": "WA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUABAAAAAAAAAADNQ6"
}
```

Expected: `{"kind":"muxed_contract","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6JoAIAAAAAAAAA=="}`

Actual: `{"rejected":"Error: StrKey.decodeMuxedContract does not exist in this sdk version"}`

### `case 3/strkey_decode` on 16.4.1

the sdk refused a value the network accepts: Error: StrKey.decodeMuxedContract does not exist in this sdk version

```json
{
  "op": "strkey_decode",
  "text": "WA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJVAAAAAAAAAAAACWJY"
}
```

Expected: `{"kind":"muxed_contract","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6JqAAAAAAAAAAA=="}`

Actual: `{"rejected":"Error: StrKey.decodeMuxedContract does not exist in this sdk version"}`

### `case 4/strkey_decode` on 16.4.1

the sdk refused a value the network accepts: Error: StrKey.decodeMuxedContract does not exist in this sdk version

```json
{
  "op": "strkey_decode",
  "text": "WA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJV7777777777777GMO"
}
```

Expected: `{"kind":"muxed_contract","payload":"Pww0v5OtDZlx0EzMkPcFURyDiq2XNKSi+w16A/x/6Jr//////////w=="}`

Actual: `{"rejected":"Error: StrKey.decodeMuxedContract does not exist in this sdk version"}`

2 more checks fail; the command below lists every one.

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk js --version 16.4.1 --entry cap84.muxed-contract.synth-strkey-w-valid
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/vectors/strkey/cap84.muxed-contract.synth-strkey-w-valid.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/vectors/strkey/cap84.muxed-contract.synth-strkey-w-valid.json) cases 0, 1, 2, 3, 4, 5, 6

How the corpus entry was built and checked: valid muxed contracts (W, 22 << 3, CAP-84): 32 contract id bytes then a big-endian u64 id. expectations are stellar-strkey 1.0.0's verdicts; SEP-23 1.4.0 says a strkey is valid only if re-encoding its binary gives back the same string, which rules out wrong lengths, set trailing bits, padding and lowercase. the reject reason is stellar-strkey's error and is informational; a testee passes by rejecting.

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap84.muxed-contract.synth-strkey-w-valid`
