<!-- repo: stellar/wallet-backend -->
# Transfers into or out of a claimable balance or a liquidity pool keep one side only

## What happens

At 3e22515a44979fdc3a558fda3991ca2b48e38cef, [`internal/indexer/processors/token_transfer.go` line 263](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/token_transfer.go#L263): `handleTransfer` writes a single DEBIT for `CreateClaimableBalance` and `LiquidityPoolDeposit` and a single CREDIT for `ClaimClaimableBalance` and `LiquidityPoolWithdraw`, skips any other transfer touching a claimable balance, and reduces a transfer with a liquidity pool to the account side (`handleTransfersWithLiquidityPool`). The B or L end of the transfer, which the ledger's CAP-67 event names, is never stored, so the stored movement has no counterparty.

Severity: data-loss. CAPs: CAP-0023, CAP-0033, CAP-0035, CAP-0038, CAP-0067.

## Minimal entry

Corpus entry `cap23.claimable-balances.p30-claim-through-depth-4-envelope` of the shapekit corpus: chain `cap23.claimable-balances.p30`, ledger 66, recorded on a standalone stellar-core 30.0.0 under the testnet passphrase (the mainnet variant gives the same result). Transaction(s): `e21911e104f314d546bf3486b8c67dba96727af3153de08e99d311ae34c0d105`.

## Reproduce

```sh
(cd adapters && go build -o bin/wb-adapter ./walletbackend/cmd/wb-adapter)
shapekit harness run --chain cap23.claimable-balances.p30 --network testnet --adapter "adapters/bin/wb-adapter --commit 3e22515a44979fdc3a558fda3991ca2b48e38cef"
```

The harness replays the chain through a replay stellar-rpc and history archive into wallet-backend's live ingestion (TimescaleDB, `migrate up`, `ingest`), reads back `transactions`, `transactions_accounts` and `state_changes`, and compares them with what the ledger meta shows. Output: `harness-out/HARNESS.md`.

## Expected (ledger meta) and stored

From the harness report of the run above (`harness-report.json`, entry `cap23.claimable-balances.p30-claim-through-depth-4-envelope`, testnet):

- dropped `token-event`: token-event is missing
  - ledger: `{"amount": "1000000000", "contract": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC", "event": "transfer", "from": "BAADAKZHNGMRSGZO3FLGWM64I2NYZP4GFLIDBWXWKGL2TEYRSRVZC5Z7CE", "to": "GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU", "to_muxed_id": null}`
- extra `token-event`: token-event is stored but the meta does not show it
  - stored: `{"amount": "1000000000", "contract": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC", "event": "transfer", "from": null, "to": "GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU", "to_muxed_id": null}`

Verdict for the entry: mishandled (wrong-value): 0 wrong value(s), 1 fact(s) stored that the meta does not show.

## Suggested fix

Store the B or L address as the counterparty (an account column cannot hold it today) or keep the transfer as one row with both ends, as the event has them.

## Other entries with the same cause

104 problem(s) over 19 entries (mainnet, testnet):

- `cap23.claimable-balances.p30-claim-through-depth-4-meta`
- `cap23.claimable-balances.p30-create-ten-claimants-depth-4-envelope`
- `cap23.claimable-balances.p30-create-ten-claimants-depth-4-meta`
- `cap33.sponsorship.p30-sponsored-claimable-balance-envelope`
- `cap33.sponsorship.p30-sponsored-claimable-balance-meta`
- `cap35.clawback.p30-create-clawback-enabled-balance`
- `cap38.liquidity-pools.p30-deposit-envelope`
- `cap38.liquidity-pools.p30-deposit-meta`
- `cap38.liquidity-pools.p30-path-payment-through-pool`
- `cap38.liquidity-pools.p30-revoke-redeems-pool-shares`
- `cap38.liquidity-pools.p30-withdraw-envelope`
- `cap38.liquidity-pools.p30-withdraw-meta`
- `cap67.unified-events.p22-backfill-claim-claimable-balance`
- `cap67.unified-events.p22-backfill-create-claimable-balance`
- `cap67.unified-events.p22-backfill-liquidity-pool-deposit`
- `cap67.unified-events.p30-claim-claimable-balance`
- `cap67.unified-events.p30-create-claimable-balance`
- `cap67.unified-events.p30-liquidity-pool-deposit`
