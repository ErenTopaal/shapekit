<!-- repo: stellar/wallet-backend -->
# An XLM transfer through the native asset contract that creates the destination account records no account creation

## What happens

At 3e22515a44979fdc3a558fda3991ca2b48e38cef, [`internal/indexer/processors/token_transfer.go` line 293](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/token_transfer.go#L293): `handleTransfer` adds the `ACCOUNT`/`CREATE` row only for `OperationTypeCreateAccount`. Since protocol 23 a native SAC transfer to a G address that does not exist creates the account (CAP-73); the meta shows the new account entry, but the operation is `InvokeHostFunction`, so only the balance credit is stored.

Severity: data-loss. CAPs: CAP-0046-06, CAP-0073.

## Minimal entry

Corpus entry `cap73.sac-trust.p28-xlm-transfer-creates-account-envelope` of the shapekit corpus: chain `cap73.sac-trust.p28`, ledger 69, recorded on a standalone stellar-core 30.0.0 under the testnet passphrase (the mainnet variant gives the same result). Transaction(s): `f715fe4d1651ded84dd1d5656f5e1c1b7fa17f5236dcc339b04eff051d1dd371`.

## Reproduce

```sh
(cd adapters && go build -o bin/wb-adapter ./walletbackend/cmd/wb-adapter)
shapekit harness run --chain cap73.sac-trust.p28 --network testnet --adapter "adapters/bin/wb-adapter --commit 3e22515a44979fdc3a558fda3991ca2b48e38cef"
```

The harness replays the chain through a replay stellar-rpc and history archive into wallet-backend's live ingestion (TimescaleDB, `migrate up`, `ingest`), reads back `transactions`, `transactions_accounts` and `state_changes`, and compares them with what the ledger meta shows. Output: `harness-out/HARNESS.md`.

## Expected (ledger meta) and stored

From the harness report of the run above (`harness-report.json`, entry `cap73.sac-trust.p28-xlm-transfer-creates-account-envelope`, testnet):

- dropped `account-created`: account-created is missing
  - ledger: `{"account": "GCF4WKRRASFFTOM3KAFPB2BB5YPBY5AYUG273SCJXFLHGRLK2SCFPSFM"}`

Verdict for the entry: dropped: 1 of 5 in-scope fact(s) missing.

## Suggested fix

Record ACCOUNT CREATE from the account entries the meta creates rather than from the operation type.

## Other entries with the same cause

8 problem(s) over 4 entries (mainnet, testnet):

- `cap73.sac-trust.p28-xlm-transfer-creates-account-meta`
- `cap73.sac-trust.p30-xlm-transfer-creates-account-envelope`
- `cap73.sac-trust.p30-xlm-transfer-creates-account-meta`
