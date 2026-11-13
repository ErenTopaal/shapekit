<!-- repo: stellar/wallet-backend -->
# Transaction participants from meta are never collected (participantsForMeta reads the TransactionMeta V0 arm)

> Already reported upstream as https://github.com/stellar/wallet-backend/issues/677. This draft records the reproduction and is not to be filed as a new issue; it can go into a comment there.

## What happens

At 3e22515a44979fdc3a558fda3991ca2b48e38cef, [`internal/indexer/processors/participants.go` line 66](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/participants.go#L66): `participantsForMeta` returns at once when `meta.Operations` (the V0 arm of the `TransactionMeta` union) is nil, which it is for every v1 to v4 meta, so the meta-derived participant loop never runs. Participants then come only from the envelope, operation participants and fee changes.

Severity: data-loss. CAPs: CAP-0015, CAP-0023, CAP-0035, CAP-0067.

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

- dropped `participant`: participant GCNPGADCOMTNPTM2XR6V7IOCWQ2MLJNGTCGD3U7SVPHHS7PKN6HMDEAG is missing
  - ledger: `"GCNPGADCOMTNPTM2XR6V7IOCWQ2MLJNGTCGD3U7SVPHHS7PKN6HMDEAG"`

Verdict for the entry: mishandled (wrong-value): 0 wrong value(s), 1 fact(s) stored that the meta does not show.

## Suggested fix

Read operation changes through `ingest.LedgerTransaction.GetOperationChanges` (or `GetChanges`), which handle v1 to v4 meta; #677 has the details.

## Other entries with the same cause

76 problem(s) over 20 entries (mainnet, testnet):

- `cap15.fee-bump.p30-inner-bad-seq`
- `cap23.claimable-balances.p30-claim-through-depth-4-meta`
- `cap23.claimable-balances.p30-create-ten-claimants-depth-4-envelope`
- `cap23.claimable-balances.p30-create-ten-claimants-depth-4-meta`
- `cap35.clawback.p30-clawback-claimable-balance-envelope`
- `cap35.clawback.p30-clawback-claimable-balance-meta`
- `cap35.clawback.p30-create-clawback-enabled-balance`
- `cap67.unified-events.p22-backfill-claim-claimable-balance`
- `cap67.unified-events.p22-backfill-clawback-claimable-balance`
- `cap67.unified-events.p22-backfill-create-claimable-balance`
- `cap67.unified-events.p22-backfill-payment-burn`
- `cap67.unified-events.p22-claim-claimable-balance`
- `cap67.unified-events.p22-clawback-claimable-balance`
- `cap67.unified-events.p22-create-claimable-balance`
- `cap67.unified-events.p22-payment-burn`
- `cap67.unified-events.p30-claim-claimable-balance`
- `cap67.unified-events.p30-clawback-claimable-balance`
- `cap67.unified-events.p30-create-claimable-balance`
- `cap67.unified-events.p30-payment-burn`
