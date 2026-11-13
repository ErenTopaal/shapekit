<!-- repo: stellar/wallet-backend -->
# Clawbacks are stored as BURN, so a clawback is indistinguishable from a payment to the issuer

## What happens

At 3e22515a44979fdc3a558fda3991ca2b48e38cef, [`internal/indexer/processors/token_transfer.go` line 363](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/token_transfer.go#L363): `handleClawback` writes `BALANCE`/`BURN` (and a `DEBIT`) for a clawback, the same rows a payment to the issuer produces. The state change reasons have no CLAWBACK, so the ledger's `clawback` event is stored as a `burn`, and a reader cannot tell an issuer seizing funds from a holder returning them.

Severity: wrong-data. CAPs: CAP-0027, CAP-0035, CAP-0067.

## Minimal entry

Corpus entry `cap35.clawback.p30-clawback-claimable-balance-envelope` of the shapekit corpus: chain `cap35.clawback.p30`, ledger 69, recorded on a standalone stellar-core 30.0.0 under the testnet passphrase (the mainnet variant gives the same result). Transaction(s): `45260cb10422d6d18fcf808e50f095d4007baaea2296b7b828dab5c9521b662e`.

## Reproduce

```sh
(cd adapters && go build -o bin/wb-adapter ./walletbackend/cmd/wb-adapter)
shapekit harness run --chain cap35.clawback.p30 --network testnet --adapter "adapters/bin/wb-adapter --commit 3e22515a44979fdc3a558fda3991ca2b48e38cef"
```

The harness replays the chain through a replay stellar-rpc and history archive into wallet-backend's live ingestion (TimescaleDB, `migrate up`, `ingest`), reads back `transactions`, `transactions_accounts` and `state_changes`, and compares them with what the ledger meta shows. Output: `harness-out/HARNESS.md`.

## Expected (ledger meta) and stored

From the harness report of the run above (`harness-report.json`, entry `cap35.clawback.p30-clawback-claimable-balance-envelope`, testnet):

- dropped `token-event`: token-event is missing
  - ledger: `{"amount": "1000000000", "contract": "CBSQ7FZXGMQC2Z4MTCO3GRPRKEFO5ZS4SH5IVZSULCKNCEMMNXUBVK7Y", "event": "clawback", "from": "BAALGTX44DBPNG4VLVQ7CWHQ7TGKJJN57UY6IUPJHTDGDGAJ7OZL7ZGS2Q", "to": null, "to_muxed_id": null}`
- extra `token-event`: token-event is stored but the meta does not show it
  - stored: `{"amount": "1000000000", "contract": "CBSQ7FZXGMQC2Z4MTCO3GRPRKEFO5ZS4SH5IVZSULCKNCEMMNXUBVK7Y", "event": "burn", "from": "GCTO7VGZHLU2X27EXI3QOM2PRBK2MLKWR74Y7KQ7WJPFJWVFCFB27K6X", "to": null, "to_muxed_id": null}`

Verdict for the entry: mishandled (wrong-value): 0 wrong value(s), 1 fact(s) stored that the meta does not show.

## Suggested fix

Add a CLAWBACK reason, or store the event type, so clawbacks stay distinguishable from burns.

## Other entries with the same cause

36 problem(s) over 9 entries (mainnet, testnet):

- `cap27.muxed-accounts.p28-clawback-from-id-2p64-1`
- `cap27.muxed-accounts.p30-clawback-from-id-2p64-1`
- `cap35.clawback.p30-clawback`
- `cap35.clawback.p30-clawback-claimable-balance-meta`
- `cap67.unified-events.p22-backfill-clawback`
- `cap67.unified-events.p22-backfill-clawback-claimable-balance`
- `cap67.unified-events.p30-clawback`
- `cap67.unified-events.p30-clawback-claimable-balance`
