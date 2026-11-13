<!-- repo: stellar/wallet-backend -->
# A failed transaction that consumes a pre-auth signer records no signer removal

## What happens

At 3e22515a44979fdc3a558fda3991ca2b48e38cef, [`internal/indexer/processors/effects_horizon.go` line 65](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/effects_horizon.go#L65): `Effects` returns nothing for a failed transaction. Core still removes a pre-authorized transaction signer (`T...`) consumed by the transaction, in `txChangesBefore`, whether the transaction then succeeds or fails, so the signer removal of a failed transaction is never stored and the account appears to keep the signer.

Severity: data-loss. CAPs: CAP-0002, CAP-0028.

## Minimal entry

Corpus entry `classic.failed-tx.p30-pre-auth-failed-removes-signer` of the shapekit corpus: chain `classic.failed-tx.p30`, ledger 67, recorded on a standalone stellar-core 30.0.0 under the testnet passphrase (the mainnet variant gives the same result). Transaction(s): `44d93dbceaf43f09b5d08608600d1c6e50d0fbde60659efd7a1ce8f8814dee4e`.

## Reproduce

```sh
(cd adapters && go build -o bin/wb-adapter ./walletbackend/cmd/wb-adapter)
shapekit harness run --chain classic.failed-tx.p30 --network testnet --adapter "adapters/bin/wb-adapter --commit 3e22515a44979fdc3a558fda3991ca2b48e38cef"
```

The harness replays the chain through a replay stellar-rpc and history archive into wallet-backend's live ingestion (TimescaleDB, `migrate up`, `ingest`), reads back `transactions`, `transactions_accounts` and `state_changes`, and compares them with what the ledger meta shows. Output: `harness-out/HARNESS.md`.

## Expected (ledger meta) and stored

From the harness report of the run above (`harness-report.json`, entry `classic.failed-tx.p30-pre-auth-failed-removes-signer`, testnet):

- dropped `signer-removed`: signer-removed is missing
  - ledger: `{"account": "GDD25X2BOFUU33TVAMOKUHP7UNWFO45G2YIB54KLIAJAJJZTRYXUIBES", "signer": "TBCNSPN45L2D6CNV2CDAQYANDRXFBUH33ZQGLHX5PIOOR6EBJXXE4TJL", "weight": 1}`

Verdict for the entry: dropped: 1 of 3 in-scope fact(s) missing.

## Suggested fix

Read signer changes from `txChangesBefore` as well as from operation effects, for failed transactions too.

## Other entries with the same cause

2 problem(s) over 1 entry (mainnet, testnet):

- none
