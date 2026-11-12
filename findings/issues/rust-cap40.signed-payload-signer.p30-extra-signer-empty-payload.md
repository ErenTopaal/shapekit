<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 30.0.0: the sdk refused a value the network accepts: xdr value invalid at line 1 column 304 (cap40.signed-payload-signer.p30-extra-signer-empty-payload)

## What happens

`stellar-xdr` 30.0.0 answers 2 of the checks of corpus entry `cap40.signed-payload-signer.p30-extra-signer-empty-payload` differently from the Stellar network.

## Minimal vector

Entry `cap40.signed-payload-signer.p30-extra-signer-empty-payload` (kind Xdr, family `cap40.signed-payload-signer`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: xdr value invalid at line 1 column 304

```json
{
  "json": {
    "tx": {
      "signatures": [
        {
          "hint": "2722451e",
          "signature": "ee66ce00ffd14c2e53c86ed39e56407cb69041e64ef6414f33a0005b6361001e4b79429471080eb4c270983d031cb9cf4601d0bb788952cf98ccbd6a46bbc402"
        }
      ],
      "tx": {
        "cond": {
          "v2": {
            "extra_signers": [
              "<INVALID:GBJNRNVABFPR26UZT3UE2D5Y73U33YQ7O6UI2YEKD6T4JM5AZQL5HXJT:EMPTY_PAYLOAD>"
            ],
            "ledger_bounds": null,
            "min_seq_age": "0",
            "min_seq_ledger_gap": 0,
            "min_seq_num": null,
            "time_bounds": null
          }
        },
        "ext": "v0",
        "fee": 100,
        "memo": "none",
        "operations": [
          {
            "body": {
              "payment": {
                "amount": "10000000",
                "asset": "native",
                "destination": "GCCKY2DTW66FL46YTD52NMFQAXOHAIKZDISNRQMOE2DN77OMGWRV7GZ2"
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

Expected: `{"xdr":"AAAAAgAAAACEukrMC4ar0JKMmb6K/edSkB76EXAhN9uMoY7GJyJFHgAAAGQAAAADAAAABwAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAA1LYtqAJXx16mZ7oTQ+4/um94h93qI1gih+nxLOgzBfTAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAhKxoc7e8VfPYmPumsLAF3HAhWRok2MGOJobf/cw1o18AAAAAAAAAAACYloAAAAAAAAAAASciRR4AAABA7mbOAP/RTC5TyG7TnlZAfLaQQeZO9kFPM6AAW2NhAB5LeUKUcQgOtMJwmD0DHLnPRgHQu3iJUs+YzL1qRrvEAg=="}`

Actual: `{"rejected":"xdr value invalid at line 1 column 304"}`

### `mainnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: xdr value invalid at line 1 column 304

```json
{
  "json": {
    "tx": {
      "signatures": [
        {
          "hint": "2722451e",
          "signature": "cb70ad0485934c34089bd71b1272469bdfd65bf8056c68fa7389b084dce4c2a9f8d07216284a5eff20b327fa641d2a2f65f081663286875e6c65081e8c9ba703"
        }
      ],
      "tx": {
        "cond": {
          "v2": {
            "extra_signers": [
              "<INVALID:GBJNRNVABFPR26UZT3UE2D5Y73U33YQ7O6UI2YEKD6T4JM5AZQL5HXJT:EMPTY_PAYLOAD>"
            ],
            "ledger_bounds": null,
            "min_seq_age": "0",
            "min_seq_ledger_gap": 0,
            "min_seq_num": null,
            "time_bounds": null
          }
        },
        "ext": "v0",
        "fee": 100,
        "memo": "none",
        "operations": [
          {
            "body": {
              "payment": {
                "amount": "10000000",
                "asset": "native",
                "destination": "GCCKY2DTW66FL46YTD52NMFQAXOHAIKZDISNRQMOE2DN77OMGWRV7GZ2"
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

Expected: `{"xdr":"AAAAAgAAAACEukrMC4ar0JKMmb6K/edSkB76EXAhN9uMoY7GJyJFHgAAAGQAAAADAAAABwAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAA1LYtqAJXx16mZ7oTQ+4/um94h93qI1gih+nxLOgzBfTAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAhKxoc7e8VfPYmPumsLAF3HAhWRok2MGOJobf/cw1o18AAAAAAAAAAACYloAAAAAAAAAAASciRR4AAABAy3CtBIWTTDQIm9cbEnJGm9/WW/gFbGj6c4mwhNzkwqn40HIWKEpe/yCzJ/pkHSovZfCBZjKGh15sZQgejJunAw=="}`

Actual: `{"rejected":"xdr value invalid at line 1 column 304"}`

## Reproduce

```sh
shapekit conformance run --sdk rust --version 30.0.0 --entry cap40.signed-payload-signer.p30-extra-signer-empty-payload
```

## Expected value and its source

- corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p30-extra-signer-empty-payload/mainnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p30-extra-signer-empty-payload/mainnet.xdr
- corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p30-extra-signer-empty-payload/testnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p30-extra-signer-empty-payload/testnet.xdr

Entry notes: an extra signer with an empty signed payload, which core refuses as malformed before the transaction reaches a ledger. submitted with ledger 11 as the last closed ledger

Related: https://github.com/stellar/wallet-backend/pull/729, https://github.com/stellar/wallet-backend/issues/716

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `cap40.signed-payload-signer.p30-extra-signer-empty-payload`
