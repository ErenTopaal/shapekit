<!-- repo: stellar/stellar-etl -->
# Question: no `account_created` effect for an account a Stellar asset contract transfer creates

Checked at [7e3fc9af](https://github.com/stellar/stellar-etl/commit/7e3fc9af95359dad6ba939fa88791f49cdf531ae) (v2.8.35). `internal/transform/effects.go` is the same in v2.8.35, the latest release.

## What we see

Since protocol 26 ([CAP-73](https://github.com/stellar/stellar-protocol/blob/ced008c3727005ca4a775f430fe6fe346c30f58f/core/cap-0073.md)) an XLM transfer through the native asset contract to an account that does not exist creates that account. The effects table records the transfer as `account_debited` and `account_credited`, but has no `account_created` row for the new account.

## Where it comes from

[`addInvokeHostFunctionEffects`](https://github.com/stellar/stellar-etl/blob/7e3fc9af95359dad6ba939fa88791f49cdf531ae/internal/transform/effects.go#L1326-L1440) builds the effects of a contract call from its Stellar asset contract events only. `account_created` is written only for a `CreateAccount` operation ([`addAccountCreatedEffects`](https://github.com/stellar/stellar-etl/blob/7e3fc9af95359dad6ba939fa88791f49cdf531ae/internal/transform/effects.go#L411-L437)).

## Question

Should an account created by a contract transfer get `account_created`, as one created by `CreateAccount` does? If not, documenting that the effects of a contract call cover token movements only would tell consumers to find new accounts in the ledger entry changes.

## The ledger

Testnet passphrase, standalone stellar-core 30.0.0, ledger 73 of the `cap73.sac-trust.p30` chain, transaction `69d6a941d953100889d67b93301a822edc56557edb67abe24be12f8ce624311a`: `GDKMA2CZT4MP2PTP7GFMTUGDJEE7QBFRNJAYDKEYSEKJ7G4VO6OT5CZQ` transfers 100 XLM through the native asset contract to `GCF4WKRRASFFTOM3KAFPB2BB5YPBY5AYUG273SCJXFLHGRLK2SCFPSFM`, which did not exist, and the meta creates it with a balance of 1000000000 stroops. The effects export of that ledger has `account_debited` and `account_credited` rows but no `account_created`.

## Reproduce

Each ledger is committed in [shapekit](https://github.com/ErenTopaal/shapekit) as a batch of a SEP-54 data lake (testnet passphrase, zstd, one ledger per batch):

- ledger 73 of `cap73.sac-trust.p30`: [`corpus/chains/cap73.sac-trust.p30/testnet/ledgers/FFFFFFFF--0-63999/FFFFFFB6--73.xdr.zst`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/chains/cap73.sac-trust.p30/testnet/ledgers/FFFFFFFF--0-63999/FFFFFFB6--73.xdr.zst)

From a checkout of the shapekit repository at tag `v0.2.0` (needs Go and Docker), this builds stellar-etl at the commit above, serves the lake to it through a local fake GCS server, runs its exports, `export_effects` among them, for every ledger and compares the rows with the ledger meta:

```sh
(cd adapters && go build -o bin/etl-adapter ./stellaretl/cmd/etl-adapter)
shapekit harness run --chain cap73.sac-trust.p30 --network testnet --adapter "adapters/bin/etl-adapter --commit 7e3fc9af95359dad6ba939fa88791f49cdf531ae"
```

In `harness-out/HARNESS.md`, the Entries table lists the entry `cap73.sac-trust.p30-xlm-transfer-creates-account-meta` with the verdict `dropped` and the detail `account-created is missing`.
