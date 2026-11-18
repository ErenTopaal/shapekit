<!-- repo: stellar/stellar-etl -->
# Question: no `trustline_removed` effect for the pool share trustline a revocation removes

Checked at [2b24ec89](https://github.com/stellar/stellar-etl/commit/2b24ec89ddce7fc0beb7795de982c93e6895eaf3) (v2.8.34).

## What we see

When an issuer revokes authorization of a trustline whose owner holds pool shares that use the asset (`SetTrustLineFlags` or `AllowTrust`), core redeems the shares into claimable balances and removes the owner's pool share trustline.

The effects table has `trustline_flags_updated`, `claimable_balance_created` and `liquidity_pool_revoked` rows, but no `trustline_removed` row for the pool share trustline.

## Where it comes from

[`addLiquidityPoolRevokedEffect`](https://github.com/stellar/stellar-etl/blob/2b24ec89ddce7fc0beb7795de982c93e6895eaf3/internal/transform/effects.go#L1152-L1219), which both [`addSetTrustLineFlagsEffects`](https://github.com/stellar/stellar-etl/blob/2b24ec89ddce7fc0beb7795de982c93e6895eaf3/internal/transform/effects.go#L1101-L1106) and `addAllowTrustEffects` call, writes the claimable balances and one `liquidity_pool_revoked` effect from the operation's changes, and does not look at the removed trustline entry.

## Question

Should the removed pool share trustline get `trustline_removed`, as a `ChangeTrust` with limit 0 gets one? If the `liquidity_pool_revoked` effect is meant to imply the removal, a line in the effects documentation would say so.

## Reproduction

Testnet passphrase, standalone stellar-core 30.0.0, ledger 74 of the `cap38.liquidity-pools.p30` chain, transaction `17dc53f99a4ca34a64291886b4e2f7b9f8183bd7814412db2930217bf6ec510e`: the meta removes the trustline of `GCTWYC6XEKAC5HBLNUH6NS7VA7CZNZGBU2R7FUSM2TWLNGBR367THITH` to the pool share `LB45SWYS5MXFZG6GEWVF4BMTMDWDLIN6RJ5EU763SCQEZDS7QFF2IPOF`. The effects export of that ledger has no row for it.
