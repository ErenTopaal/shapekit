<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 28.0.0, 28.0.1, 30.0.0: the sdk's XDR-JSON differs from the expected value (cap40.signed-payload-signer.synth-empty-payload-signer)

## What happens

`stellar-xdr` 28.0.0, 28.0.1, 30.0.0 answers 5 of the checks of corpus entry `cap40.signed-payload-signer.synth-empty-payload-signer` differently from the Stellar network.

## Minimal vector

Entry `cap40.signed-payload-signer.synth-empty-payload-signer` (kind Xdr, family `cap40.signed-payload-signer`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `any/decode_json` on 28.0.0, 28.0.1

the sdk's XDR-JSON differs from the expected value

```json
{
  "depth_limit": 1000,
  "len_limit": 232,
  "op": "decode_json",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAFAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAx4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eAAAAAAAAAAEAAAAAAAAAAQcHBwcAAABABwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBw=="
}
```

Expected: `{"json":{"tx":{"signatures":[{"hint":"07070707","signature":"07070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707"}],"tx":{"cond":"none","ext":"v0","fee":100,"memo":"none","operations":[{"body":{"set_options":{"clear_flags":null,"high_threshold":null,"home_domain":null,"inflation_dest":null,"low_threshold":null,"master_weight":null,"med_threshold":null,"set_flags":null,"signer":{"key":"<INVALID:GAPB4HQ6DYPB4HQ6DYPB4HQ6DYPB4HQ6DYPB4HQ6DYPB4HQ6DYPB5UWU:EMPTY_PAYLOAD>","weight":1}}},"source_account":null}],"seq_num":"4294... (687 chars, see the source below)`

Actual: `{"json":{"tx":{"signatures":[{"hint":"07070707","signature":"07070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707"}],"tx":{"cond":"none","ext":"v0","fee":100,"memo":"none","operations":[{"body":{"set_options":{"clear_flags":null,"high_threshold":null,"home_domain":null,"inflation_dest":null,"low_threshold":null,"master_weight":null,"med_threshold":null,"set_flags":null,"signer":{"key":"PAPB4HQ6DYPB4HQ6DYPB4HQ6DYPB4HQ6DYPB4HQ6DYPB4HQ6DYPB4AAAAAAIOTQ","weight":1}}},"source_account":null}],"seq_num":"4294967297","source_a... (670 chars, see the source below)`

### `any/encode_json` on 28.0.0, 28.0.1, 30.0.0

the sdk refused a value the network accepts: xdr value invalid at line 1 column 538

```json
{
  "json": {
    "tx": {
      "signatures": [
        {
          "hint": "07070707",
          "signature": "07070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707"
        }
      ],
      "tx": {
        "cond": "none",
        "ext": "v0",
        "fee": 100,
        "memo": "none",
        "operations": [
          {
            "body": {
              "set_options": {
                "clear_flags": null,
                "high_threshold": null,
                "home_domain": null,
                "inflation_dest": null,
                "low_threshold": null,
                "master_weight": null,
                "med_threshold": null,
                "set_flags": null,
                "signer": {
                  "key": "<INVALID:GAPB4HQ6DYPB4HQ6DYPB4HQ6DYPB4HQ6DYPB4HQ6DYPB4HQ6DYPB5UWU:EMPTY_PAYLOAD>",
                  "weight": 1
                }
              }
            },
            "source_account": null
          }
        ],
        "seq_num": "4294967297",
        "source_account": "GAAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQDZ7H"
      }
    }
  },
  "op": "encode_json",
  "type": "TransactionEnvelope"
}
```

Expected: `{"xdr":"AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAFAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAx4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eAAAAAAAAAAEAAAAAAAAAAQcHBwcAAABABwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBw=="}`

Actual: `{"rejected":"xdr value invalid at line 1 column 538"}`

## Reproduce

```sh
shapekit conformance run --sdk rust --version 28.0.0 --entry cap40.signed-payload-signer.synth-empty-payload-signer
```

## Expected value and its source

- corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.synth-empty-payload-signer/any.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.synth-empty-payload-signer/any.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.synth-empty-payload-signer/any.xdr

Entry notes: a set-options signer of type ed25519 signed payload with an empty payload. the XDR allows 0 to 64 bytes, but core answers SET_OPTIONS_BAD_SIGNER for an empty one (SetOptionsOpFrame::doCheckValid), so it never reaches a ledger.

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `cap40.signed-payload-signer.synth-empty-payload-signer`
