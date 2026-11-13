<!-- repo: stellar/wallet-backend -->
# A pre-auth signer core removes in txChangesBefore is never recorded as removed

## What happens

At 3e22515a44979fdc3a558fda3991ca2b48e38cef, signer changes are written only from operation effects: [`internal/indexer/processors/effects_horizon.go` line 135](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/effects_horizon.go#L135) (`addSetOptionsEffects`) diffs the account in the SetOptions operation's own changes ([line 177](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/effects_horizon.go#L177)), and [line 65](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/effects_horizon.go#L65) returns no effects at all for a failed transaction. Core removes a pre-authorized transaction signer (`T...`) in `txChangesBefore`, outside any operation, in two cases:

- the transaction's own hash signer, when the transaction is applied, whether it then succeeds or fails;
- a fee bump's hash signer, which `FeeBumpTransactionFrame::apply` removes from the fee source (`removeOneTimeSignerKeyFromFeeSource`, stellar-core v30.0.0) in its own `LedgerTxn` before the inner transaction runs, also when the inner transaction succeeds.

Neither removal is stored, so the account appears to keep a signer it no longer has.

Severity: data-loss. CAPs: CAP-0002, CAP-0015, CAP-0028.

## Minimal entries

- Corpus entry `classic.failed-tx.p30-pre-auth-failed-removes-signer` of the shapekit corpus: chain `classic.failed-tx.p30`, ledger 67, recorded on a standalone stellar-core 30.0.0 under the testnet passphrase (the mainnet variant gives the same result). Transaction(s): `44d93dbceaf43f09b5d08608600d1c6e50d0fbde60659efd7a1ce8f8814dee4e`.
- Corpus entry `cap15.fee-bump.p30-fee-source-is-inner-source-pre-auth-signer-meta`: chain `cap15.fee-bump.p30`, ledger 85, testnet passphrase (the mainnet variant gives the same result). A successful fee bump whose fee source is the inner source and holds a pre-auth signer for the fee-bump hash. Transaction(s): `6411ddc2788dc797d4775332e3199098e2a3a2213e5566ab78a160a839bf8afd`. Its `txChangesBefore` holds two groups for the account: the signer removal, then the inner transaction's sequence bump.

## Reproduce

```sh
(cd adapters && go build -o bin/wb-adapter ./walletbackend/cmd/wb-adapter)
shapekit harness run --chain classic.failed-tx.p30 --chain cap15.fee-bump.p30 --network testnet --adapter "adapters/bin/wb-adapter --commit 3e22515a44979fdc3a558fda3991ca2b48e38cef"
```

The harness replays the chains through a replay stellar-rpc and history archive into wallet-backend's live ingestion (TimescaleDB, `migrate up`, `ingest`), reads back `transactions`, `transactions_accounts` and `state_changes`, and compares them with what the ledger meta shows. Output: `harness-out/HARNESS.md`.

## Expected (ledger meta) and stored

From the harness report of the run above (`harness-report.json`, testnet):

- `classic.failed-tx.p30-pre-auth-failed-removes-signer`: dropped `signer-removed`: signer-removed is missing
  - ledger: `{"account": "GDD25X2BOFUU33TVAMOKUHP7UNWFO45G2YIB54KLIAJAJJZTRYXUIBES", "signer": "TBCNSPN45L2D6CNV2CDAQYANDRXFBUH33ZQGLHX5PIOOR6EBJXXE4TJL", "weight": 1}`
- `cap15.fee-bump.p30-fee-source-is-inner-source-pre-auth-signer-meta`: dropped `signer-removed`: signer-removed is missing
  - ledger: `{"account": "GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX", "signer": "TBSBDXOCPCG4PF6UO5JTFYYZSCMOFI5CEE7FKZVLPCQWBKBZX6FP3CEA", "weight": 1}`
  - verdict for the entry: dropped, 1 of 5 in-scope facts missing; the other four are stored.

## Suggested fix

Read signer changes from `txChangesBefore` as well as from operation effects, for failed transactions and for fee bumps too. A key can have two groups in `txChangesBefore` (the fee bump case), so compare the value before its first group with the value after its last.

## Other entries with the same cause

The fee bump's envelope entry `cap15.fee-bump.p30-fee-source-is-inner-source-pre-auth-signer-envelope` names the same transaction. Each entry fails on both the mainnet and testnet variants.
