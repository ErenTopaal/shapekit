<!-- repo: stellar/stellar-etl -->
# Question: no `signer_removed` effect for a pre-auth signer core removes when its transaction applies

Checked at [7e3fc9af](https://github.com/stellar/stellar-etl/commit/7e3fc9af95359dad6ba939fa88791f49cdf531ae) (v2.8.35). `internal/transform/effects.go` is the same in v2.8.35, the latest release.

## What we see

A pre-auth transaction signer (`T...`) is removed from its account when the transaction it authorizes applies, whether that transaction succeeds or fails, and for a fee bump also from the fee source. The removal is in the transaction's `txChangesBefore`, not in an operation's changes.

The effects table has no `signer_removed` row for it.

## Where it comes from

[`effects`](https://github.com/stellar/stellar-etl/blob/7e3fc9af95359dad6ba939fa88791f49cdf531ae/internal/transform/effects.go#L60-L176) returns no effects for a failed transaction and builds the others per operation from the operation's own changes. The only source of `signer_removed` is [`addSetOptionsEffects`](https://github.com/stellar/stellar-etl/blob/7e3fc9af95359dad6ba939fa88791f49cdf531ae/internal/transform/effects.go#L530-L645), so a signer removed by core outside a `SetOptions` operation gets none.

## Question

Is the effects table meant to describe only what operations do, or should a pre-auth signer that core consumes get `signer_removed`? If the omission is intended, a line in the effects documentation would let consumers know to read signer removals from the transaction-level ledger entry changes.

## The ledger

Testnet passphrase, standalone stellar-core 30.0.0:

- `classic.failed-tx.p30`, ledger 67, transaction `44d93dbceaf43f09b5d08608600d1c6e50d0fbde60659efd7a1ce8f8814dee4e` (failed): the meta removes signer `TBCNSPN45L2D6CNV2CDAQYANDRXFBUH33ZQGLHX5PIOOR6EBJXXE4TJL` of `GDD25X2BOFUU33TVAMOKUHP7UNWFO45G2YIB54KLIAJAJJZTRYXUIBES`.
- `cap15.fee-bump.p30`, ledger 85, transaction `6411ddc2788dc797d4775332e3199098e2a3a2213e5566ab78a160a839bf8afd` (a fee bump whose fee source is the inner source): the meta removes signer `TBSBDXOCPCG4PF6UO5JTFYYZSCMOFI5CEE7FKZVLPCQWBKBZX6FP3CEA` of `GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX`.

The effects export of these ledgers has no row for either removal.

## Reproduce

Each ledger is committed in [shapekit](https://github.com/ErenTopaal/shapekit) as a batch of a SEP-54 data lake (testnet passphrase, zstd, one ledger per batch):

- ledger 67 of `classic.failed-tx.p30`: [`corpus/chains/classic.failed-tx.p30/testnet/ledgers/FFFFFFFF--0-63999/FFFFFFBC--67.xdr.zst`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/chains/classic.failed-tx.p30/testnet/ledgers/FFFFFFFF--0-63999/FFFFFFBC--67.xdr.zst)
- ledger 85 of `cap15.fee-bump.p30`: [`corpus/chains/cap15.fee-bump.p30/testnet/ledgers/FFFFFFFF--0-63999/FFFFFFAA--85.xdr.zst`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/chains/cap15.fee-bump.p30/testnet/ledgers/FFFFFFFF--0-63999/FFFFFFAA--85.xdr.zst)

From a checkout of the shapekit repository at tag `v0.2.0` (needs Go and Docker), this builds stellar-etl at the commit above, serves the lake to it through a local fake GCS server, runs its exports, `export_effects` among them, for every ledger and compares the rows with the ledger meta:

```sh
(cd adapters && go build -o bin/etl-adapter ./stellaretl/cmd/etl-adapter)
shapekit harness run --chain classic.failed-tx.p30 --chain cap15.fee-bump.p30 --network testnet --adapter "adapters/bin/etl-adapter --commit 7e3fc9af95359dad6ba939fa88791f49cdf531ae"
```

In `harness-out/HARNESS.md`, the Entries table lists the entries `classic.failed-tx.p30-pre-auth-failed-removes-signer` and `cap15.fee-bump.p30-fee-source-is-inner-source-pre-auth-signer-meta` with the verdict `dropped` and the detail `signer-removed is missing`.
