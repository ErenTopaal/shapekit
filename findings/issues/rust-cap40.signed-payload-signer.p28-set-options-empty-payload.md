<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 28.0.0, 28.0.1, 30.0.0: the sdk's XDR-JSON differs from the expected value (cap40.signed-payload-signer.p28-set-options-empty-payload)

## What happens

`stellar-xdr` 28.0.0, 28.0.1, 30.0.0 answers 10 of the checks of corpus entry `cap40.signed-payload-signer.p28-set-options-empty-payload` differently from the Stellar network.

## Minimal vector

Entry `cap40.signed-payload-signer.p28-set-options-empty-payload` (kind Xdr, family `cap40.signed-payload-signer`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/decode_json` on 28.0.0, 28.0.1

the sdk's XDR-JSON differs from the expected value

```json
{
  "depth_limit": 1000,
  "len_limit": 232,
  "op": "decode_json",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAACEukrMC4ar0JKMmb6K/edSkB76EXAhN9uMoY7GJyJFHgAAAGQAAAADAAAABwAAAAAAAAAAAAAAAQAAAAAAAAAFAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAA1LYtqAJXx16mZ7oTQ+4/um94h93qI1gih+nxLOgzBfTAAAAAAAAAAEAAAAAAAAAASciRR4AAABAGfwoNKmag8svMMbMMGDlYJiyNZDnvCJEwbbdyP3Qc1eR2j0oLeicYcvrq5Us48DuLW2eQwYGOTafRqsVxpHxAA=="
}
```

Expected: `{"json":{"tx":{"signatures":[{"hint":"2722451e","signature":"19fc2834a99a83cb2f30c6cc3060e56098b23590e7bc2244c1b6ddc8fdd0735791da3d282de89c61cbebab952ce3c0ee2d6d9e43060639369f46ab15c691f100"}],"tx":{"cond":"none","ext":"v0","fee":100,"memo":"none","operations":[{"body":{"set_options":{"clear_flags":null,"high_threshold":null,"home_domain":null,"inflation_dest":null,"low_threshold":null,"master_weight":null,"med_threshold":null,"set_flags":null,"signer":{"key":"<INVALID:GBJNRNVABFPR26UZT3UE2D5Y73U33YQ7O6UI2YEKD6T4JM5AZQL5HXJT:EMPTY_PAYLOAD>","weight":1}}},"source_account":null}],"seq_num":"1288... (688 chars, see the source below)`

Actual: `{"json":{"tx":{"signatures":[{"hint":"2722451e","signature":"19fc2834a99a83cb2f30c6cc3060e56098b23590e7bc2244c1b6ddc8fdd0735791da3d282de89c61cbebab952ce3c0ee2d6d9e43060639369f46ab15c691f100"}],"tx":{"cond":"none","ext":"v0","fee":100,"memo":"none","operations":[{"body":{"set_options":{"clear_flags":null,"high_threshold":null,"home_domain":null,"inflation_dest":null,"low_threshold":null,"master_weight":null,"med_threshold":null,"set_flags":null,"signer":{"key":"PBJNRNVABFPR26UZT3UE2D5Y73U33YQ7O6UI2YEKD6T4JM5AZQL5GAAAAAAHDNY","weight":1}}},"source_account":null}],"seq_num":"12884901895","source_... (671 chars, see the source below)`

### `testnet/encode_json` on 28.0.0, 28.0.1, 30.0.0

the sdk refused a value the network accepts: xdr value invalid at line 1 column 538

```json
{
  "json": {
    "tx": {
      "signatures": [
        {
          "hint": "2722451e",
          "signature": "19fc2834a99a83cb2f30c6cc3060e56098b23590e7bc2244c1b6ddc8fdd0735791da3d282de89c61cbebab952ce3c0ee2d6d9e43060639369f46ab15c691f100"
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
                  "key": "<INVALID:GBJNRNVABFPR26UZT3UE2D5Y73U33YQ7O6UI2YEKD6T4JM5AZQL5HXJT:EMPTY_PAYLOAD>",
                  "weight": 1
                }
              }
            },
            "source_account": null
          }
        ],
        "seq_num": "12884901895",
        "source_account": "GCCLUSWMBODKXUESRSM35CX545JJAHX2CFYCCN63RSQY5RRHEJCR4DC2"
      }
    }
  },
  "op": "encode_json",
  "type": "TransactionEnvelope"
}
```

Expected: `{"xdr":"AAAAAgAAAACEukrMC4ar0JKMmb6K/edSkB76EXAhN9uMoY7GJyJFHgAAAGQAAAADAAAABwAAAAAAAAAAAAAAAQAAAAAAAAAFAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAA1LYtqAJXx16mZ7oTQ+4/um94h93qI1gih+nxLOgzBfTAAAAAAAAAAEAAAAAAAAAASciRR4AAABAGfwoNKmag8svMMbMMGDlYJiyNZDnvCJEwbbdyP3Qc1eR2j0oLeicYcvrq5Us48DuLW2eQwYGOTafRqsVxpHxAA=="}`

Actual: `{"rejected":"xdr value invalid at line 1 column 538"}`

### `mainnet/decode_json` on 28.0.0, 28.0.1

the sdk's XDR-JSON differs from the expected value

```json
{
  "depth_limit": 1000,
  "len_limit": 232,
  "op": "decode_json",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAACEukrMC4ar0JKMmb6K/edSkB76EXAhN9uMoY7GJyJFHgAAAGQAAAADAAAABwAAAAAAAAAAAAAAAQAAAAAAAAAFAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAA1LYtqAJXx16mZ7oTQ+4/um94h93qI1gih+nxLOgzBfTAAAAAAAAAAEAAAAAAAAAASciRR4AAABABl4mHN9TZSQi5M4HPLT8JTJdgHHFuCRilMgzb4mM8leZRmOGsenQRiZSwXagjGD0tou5CWZ7wgwanq5MHCIsDg=="
}
```

Expected: `{"json":{"tx":{"signatures":[{"hint":"2722451e","signature":"065e261cdf53652422e4ce073cb4fc25325d8071c5b8246294c8336f898cf25799466386b1e9d0462652c176a08c60f4b68bb909667bc20c1a9eae4c1c222c0e"}],"tx":{"cond":"none","ext":"v0","fee":100,"memo":"none","operations":[{"body":{"set_options":{"clear_flags":null,"high_threshold":null,"home_domain":null,"inflation_dest":null,"low_threshold":null,"master_weight":null,"med_threshold":null,"set_flags":null,"signer":{"key":"<INVALID:GBJNRNVABFPR26UZT3UE2D5Y73U33YQ7O6UI2YEKD6T4JM5AZQL5HXJT:EMPTY_PAYLOAD>","weight":1}}},"source_account":null}],"seq_num":"1288... (688 chars, see the source below)`

Actual: `{"json":{"tx":{"signatures":[{"hint":"2722451e","signature":"065e261cdf53652422e4ce073cb4fc25325d8071c5b8246294c8336f898cf25799466386b1e9d0462652c176a08c60f4b68bb909667bc20c1a9eae4c1c222c0e"}],"tx":{"cond":"none","ext":"v0","fee":100,"memo":"none","operations":[{"body":{"set_options":{"clear_flags":null,"high_threshold":null,"home_domain":null,"inflation_dest":null,"low_threshold":null,"master_weight":null,"med_threshold":null,"set_flags":null,"signer":{"key":"PBJNRNVABFPR26UZT3UE2D5Y73U33YQ7O6UI2YEKD6T4JM5AZQL5GAAAAAAHDNY","weight":1}}},"source_account":null}],"seq_num":"12884901895","source_... (671 chars, see the source below)`

### `mainnet/encode_json` on 28.0.0, 28.0.1, 30.0.0

the sdk refused a value the network accepts: xdr value invalid at line 1 column 538

```json
{
  "json": {
    "tx": {
      "signatures": [
        {
          "hint": "2722451e",
          "signature": "065e261cdf53652422e4ce073cb4fc25325d8071c5b8246294c8336f898cf25799466386b1e9d0462652c176a08c60f4b68bb909667bc20c1a9eae4c1c222c0e"
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
                  "key": "<INVALID:GBJNRNVABFPR26UZT3UE2D5Y73U33YQ7O6UI2YEKD6T4JM5AZQL5HXJT:EMPTY_PAYLOAD>",
                  "weight": 1
                }
              }
            },
            "source_account": null
          }
        ],
        "seq_num": "12884901895",
        "source_account": "GCCLUSWMBODKXUESRSM35CX545JJAHX2CFYCCN63RSQY5RRHEJCR4DC2"
      }
    }
  },
  "op": "encode_json",
  "type": "TransactionEnvelope"
}
```

Expected: `{"xdr":"AAAAAgAAAACEukrMC4ar0JKMmb6K/edSkB76EXAhN9uMoY7GJyJFHgAAAGQAAAADAAAABwAAAAAAAAAAAAAAAQAAAAAAAAAFAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAA1LYtqAJXx16mZ7oTQ+4/um94h93qI1gih+nxLOgzBfTAAAAAAAAAAEAAAAAAAAAASciRR4AAABABl4mHN9TZSQi5M4HPLT8JTJdgHHFuCRilMgzb4mM8leZRmOGsenQRiZSwXagjGD0tou5CWZ7wgwanq5MHCIsDg=="}`

Actual: `{"rejected":"xdr value invalid at line 1 column 538"}`

## Reproduce

```sh
shapekit conformance run --sdk rust --version 28.0.0 --entry cap40.signed-payload-signer.p28-set-options-empty-payload
```

## Expected value and its source

- corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p28-set-options-empty-payload/mainnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p28-set-options-empty-payload/mainnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p28-set-options-empty-payload/mainnet.xdr
- corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p28-set-options-empty-payload/testnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p28-set-options-empty-payload/testnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p28-set-options-empty-payload/testnet.xdr

Entry notes: SetOptions adding a signed-payload signer with an empty payload, which core refuses with SET_OPTIONS_BAD_SIGNER before the transaction reaches a ledger. submitted with ledger 11 as the last closed ledger

Related: https://github.com/stellar/wallet-backend/pull/729, https://github.com/stellar/wallet-backend/issues/716

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `cap40.signed-payload-signer.p28-set-options-empty-payload`
