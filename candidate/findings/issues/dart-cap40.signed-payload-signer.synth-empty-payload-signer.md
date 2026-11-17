<!-- repo: Soneso/stellar_flutter_sdk -->
# stellar_flutter_sdk 3.12.0: the sdk refused a value the network accepts: FormatException: FormatException: XDR-JSON XdrSignedPayload carries an empty payload, which has no strkey rendering (cap40.signed-payload-signer.synth-empty-payload-signer)

## What happens

On the input below, `stellar_flutter_sdk` 3.12.0 answers differently from the Stellar network in 1 check.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap40.signed-payload-signer.synth-empty-payload-signer` (family `cap40.signed-payload-signer`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `any/decode_json` on 3.12.0

the sdk refused a value the network accepts: FormatException: FormatException: XDR-JSON XdrSignedPayload carries an empty payload, which has no strkey rendering

```json
{
  "depth_limit": 1000,
  "len_limit": 232,
  "op": "decode_json",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAFAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAx4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eAAAAAAAAAAEAAAAAAAAAAQcHBwcAAABABwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBw=="
}
```

Expected: `{"any json, reference":{"tx":{"signatures":[{"hint":"07070707","signature":"07070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707"}],"tx":{"cond":"none","ext":"v0","fee":100,"memo":"none","operations":[{"body":{"set_options":{"clear_flags":null,"high_threshold":null,"home_domain":null,"inflation_dest":null,"low_threshold":null,"master_weight":null,"med_threshold":null,"set_flags":null,"signer":{"key":"<INVALID:GAPB4HQ6DYPB4HQ6DYPB4HQ6DYPB4HQ6DYPB4HQ6DYPB4HQ6DYPB5UWU:EMPTY_PAYLOAD>","weight":1}}},"source_account":null}],... (702 chars, see the source below)`

Actual: `{"rejected":"FormatException: FormatException: XDR-JSON XdrSignedPayload carries an empty payload, which has no strkey rendering"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk dart --version 3.12.0 --entry cap40.signed-payload-signer.synth-empty-payload-signer
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.synth-empty-payload-signer/any.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.synth-empty-payload-signer/any.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; holds <INVALID:, which has no canonical SEP-51 form, so any rendering is accepted

How the corpus entry was built and checked: a set-options signer of type ed25519 signed payload with an empty payload. the XDR allows 0 to 64 bytes, but core answers SET_OPTIONS_BAD_SIGNER for an empty one (SetOptionsOpFrame::doCheckValid), so it never reaches a ledger.

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap40.signed-payload-signer.synth-empty-payload-signer`
