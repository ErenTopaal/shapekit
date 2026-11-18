<!-- repo: stellar/stellar-etl -->
# Question: no `signer_removed` effect for a pre-auth signer core removes when its transaction applies

Checked at [2b24ec89](https://github.com/stellar/stellar-etl/commit/2b24ec89ddce7fc0beb7795de982c93e6895eaf3) (v2.8.34).

## What we see

A pre-auth transaction signer (`T...`) is removed from its account when the transaction it authorizes applies, whether that transaction succeeds or fails, and for a fee bump also from the fee source. The removal is in the transaction's `txChangesBefore`, not in an operation's changes.

The effects table has no `signer_removed` row for it.

## Where it comes from

[`effects`](https://github.com/stellar/stellar-etl/blob/2b24ec89ddce7fc0beb7795de982c93e6895eaf3/internal/transform/effects.go#L60-L176) returns no effects for a failed transaction and builds the others per operation from the operation's own changes. The only source of `signer_removed` is [`addSetOptionsEffects`](https://github.com/stellar/stellar-etl/blob/2b24ec89ddce7fc0beb7795de982c93e6895eaf3/internal/transform/effects.go#L530-L645), so a signer removed by core outside a `SetOptions` operation gets none.

## Question

Is the effects table meant to describe only what operations do, or should a pre-auth signer that core consumes get `signer_removed`? If the omission is intended, a line in the effects documentation would let consumers know to read signer removals from the transaction-level ledger entry changes.

## Reproduction

Testnet passphrase, standalone stellar-core 30.0.0:

- `classic.failed-tx.p30`, ledger 67, transaction `44d93dbceaf43f09b5d08608600d1c6e50d0fbde60659efd7a1ce8f8814dee4e` (failed): the meta removes signer `TBCNSPN45L2D6CNV2CDAQYANDRXFBUH33ZQGLHX5PIOOR6EBJXXE4TJL` of `GDD25X2BOFUU33TVAMOKUHP7UNWFO45G2YIB54KLIAJAJJZTRYXUIBES`.
- `cap15.fee-bump.p30`, ledger 85, transaction `6411ddc2788dc797d4775332e3199098e2a3a2213e5566ab78a160a839bf8afd` (a fee bump whose fee source is the inner source): the meta removes signer `TBSBDXOCPCG4PF6UO5JTFYYZSCMOFI5CEE7FKZVLPCQWBKBZX6FP3CEA` of `GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX`.

The effects export of these ledgers has no row for either removal.
