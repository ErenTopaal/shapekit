<!-- repo: stellar/wallet-backend -->
# A pool-share trustline removed by an authorization revocation records no trustline removal

## What happens

At e967ce8b7362b79c277e700e789181d76fbb5967, [`internal/indexer/processors/effects_horizon.go` line 307](https://github.com/stellar/wallet-backend/blob/e967ce8b7362b79c277e700e789181d76fbb5967/internal/indexer/processors/effects_horizon.go#L307): Trustline removals are derived only in `addChangeTrustEffects`. Revoking authorization through `AllowTrust` or `SetTrustLineFlags` redeems the holder's pool shares and removes the pool-share trustline (CAP-38), but `addAllowTrustEffects` and `addSetTrustLineFlagsEffects` emit flag effects only, so the removal in the meta is not stored.

Severity: data-loss. CAPs: CAP-0038.

## Minimal entry

Corpus entry `cap38.liquidity-pools.p30-revoke-redeems-pool-shares` of the shapekit corpus: chain `cap38.liquidity-pools.p30`, ledger 74, recorded on a standalone stellar-core 30.0.0 under the testnet passphrase (the mainnet variant gives the same result). Transaction(s): `17dc53f99a4ca34a64291886b4e2f7b9f8183bd7814412db2930217bf6ec510e`.

## Reproduce

```sh
(cd adapters && go build -o bin/wb-adapter ./walletbackend/cmd/wb-adapter)
shapekit harness run --chain cap38.liquidity-pools.p30 --network testnet --adapter "adapters/bin/wb-adapter --commit e967ce8b7362b79c277e700e789181d76fbb5967"
```

The harness replays the chain through a replay stellar-rpc and history archive into wallet-backend's live ingestion (TimescaleDB, `migrate up`, `ingest`), reads back `transactions`, `transactions_accounts` and `state_changes`, and compares them with what the ledger meta shows. Output: `harness-out/HARNESS.md`.

## Expected (ledger meta) and stored

From the harness report of the run above (`harness-report.json`, entry `cap38.liquidity-pools.p30-revoke-redeems-pool-shares`, testnet):

- dropped `trustline-removed`: trustline-removed is missing
  - ledger: `{"account": "GCTWYC6XEKAC5HBLNUH6NS7VA7CZNZGBU2R7FUSM2TWLNGBR367THITH", "asset": "pool-share:LB45SWYS5MXFZG6GEWVF4BMTMDWDLIN6RJ5EU763SCQEZDS7QFF2IPOF"}`

Verdict for the entry: dropped: 3 of 7 in-scope fact(s) missing.

## Suggested fix

Derive trustline removals from the trustline entries the meta removes, whatever operation removed them.

## Other entries with the same cause

2 problem(s) over 1 entry (mainnet, testnet):

- none
