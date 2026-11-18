<!-- repo: stellar/stellar-etl -->
# Question: no `trustline_removed` effect for the pool share trustline a revocation removes

Checked at [2b24ec89](https://github.com/stellar/stellar-etl/commit/2b24ec89ddce7fc0beb7795de982c93e6895eaf3) (v2.8.34). `internal/transform/effects.go` is the same in v2.8.35, the latest release.

## What we see

When an issuer revokes authorization of a trustline whose owner holds pool shares that use the asset (`SetTrustLineFlags` or `AllowTrust`), core redeems the shares into claimable balances and removes the owner's pool share trustline.

The effects table has `trustline_flags_updated`, `claimable_balance_created` and `liquidity_pool_revoked` rows, but no `trustline_removed` row for the pool share trustline.

## Where it comes from

[`addLiquidityPoolRevokedEffect`](https://github.com/stellar/stellar-etl/blob/2b24ec89ddce7fc0beb7795de982c93e6895eaf3/internal/transform/effects.go#L1152-L1219), which both [`addSetTrustLineFlagsEffects`](https://github.com/stellar/stellar-etl/blob/2b24ec89ddce7fc0beb7795de982c93e6895eaf3/internal/transform/effects.go#L1101-L1106) and `addAllowTrustEffects` call, writes the claimable balances and one `liquidity_pool_revoked` effect from the operation's changes, and does not look at the removed trustline entry.

## Question

Should the removed pool share trustline get `trustline_removed`, as a `ChangeTrust` with limit 0 gets one? If the `liquidity_pool_revoked` effect is meant to imply the removal, a line in the effects documentation would say so.

## The ledger

Testnet passphrase, standalone stellar-core 30.0.0, ledger 74 of the `cap38.liquidity-pools.p30` chain, transaction `17dc53f99a4ca34a64291886b4e2f7b9f8183bd7814412db2930217bf6ec510e`: the meta removes the trustline of `GCTWYC6XEKAC5HBLNUH6NS7VA7CZNZGBU2R7FUSM2TWLNGBR367THITH` to the pool share `LB45SWYS5MXFZG6GEWVF4BMTMDWDLIN6RJ5EU763SCQEZDS7QFF2IPOF`. The effects export of that ledger has no row for it.

## Reproduce

Each ledger is committed in [shapekit](https://github.com/ErenTopaal/shapekit) as a batch of a SEP-54 data lake (testnet passphrase, zstd, one ledger per batch):

- ledger 74 of `cap38.liquidity-pools.p30`: [`corpus/chains/cap38.liquidity-pools.p30/testnet/ledgers/FFFFFFFF--0-63999/FFFFFFB5--74.xdr.zst`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/chains/cap38.liquidity-pools.p30/testnet/ledgers/FFFFFFFF--0-63999/FFFFFFB5--74.xdr.zst)

From a checkout of the shapekit repository at tag `v0.2.0` (needs Go and Docker), this builds stellar-etl at the commit above, serves the lake to it through a local fake GCS server, runs its exports, `export_effects` among them, for every ledger and compares the rows with the ledger meta:

```sh
(cd adapters && go build -o bin/etl-adapter ./stellaretl/cmd/etl-adapter)
shapekit harness run --chain cap38.liquidity-pools.p30 --network testnet --adapter "adapters/bin/etl-adapter --commit 2b24ec89ddce7fc0beb7795de982c93e6895eaf3"
```

In `harness-out/HARNESS.md`, the Entries table lists the entry `cap38.liquidity-pools.p30-revoke-redeems-pool-shares` with the verdict `dropped` and the detail `trustline-removed is missing`.
