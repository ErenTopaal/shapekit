<!-- repo: stellar/wallet-backend -->
# CreateAccount writes a SIGNER ADD for the new account's master key, which the ledger does not show

## What happens

At e967ce8b7362b79c277e700e789181d76fbb5967, [`internal/indexer/processors/effects_horizon.go` line 122](https://github.com/stellar/wallet-backend/blob/e967ce8b7362b79c277e700e789181d76fbb5967/internal/indexer/processors/effects_horizon.go#L122): `addAccountCreatedEffects` emits Horizon's `signer_created` effect for the new account's master key, which becomes a `SIGNER`/`ADD` row. The ledger shows no signer: the master key's weight is threshold 0 of the new account entry, and no signer entry is added.

Severity: wrong-data. CAPs: CAP-0033.

## Minimal entry

Corpus entry `cap33.sponsorship.p30-sponsored-account-envelope` of the shapekit corpus: chain `cap33.sponsorship.p30`, ledger 65, recorded on a standalone stellar-core 30.0.0 under the testnet passphrase (the mainnet variant gives the same result). Transaction(s): `deaf08ab7a2f5b5baee797b4567f4eb2edc56d2397c31576d40ffa9a7b68dfed`.

## Reproduce

```sh
(cd adapters && go build -o bin/wb-adapter ./walletbackend/cmd/wb-adapter)
shapekit harness run --chain cap33.sponsorship.p30 --network testnet --adapter "adapters/bin/wb-adapter --commit e967ce8b7362b79c277e700e789181d76fbb5967"
```

The harness replays the chain through a replay stellar-rpc and history archive into wallet-backend's live ingestion (TimescaleDB, `migrate up`, `ingest`), reads back `transactions`, `transactions_accounts` and `state_changes`, and compares them with what the ledger meta shows. Output: `harness-out/HARNESS.md`.

## Expected (ledger meta) and stored

From the harness report of the run above (`harness-report.json`, entry `cap33.sponsorship.p30-sponsored-account-envelope`, testnet):

- extra `signer-added`: signer-added is stored but the meta does not show it
  - stored: `{"account": "GD7H63KWIVUWQD73AP53NQMZBJYEHNYXCSPL22QNQE37MVS7PVQA3NE6", "signer": "GD7H63KWIVUWQD73AP53NQMZBJYEHNYXCSPL22QNQE37MVS7PVQA3NE6", "weight": 1}`

Verdict for the entry: mishandled (wrong-value): 0 wrong value(s), 1 fact(s) stored that the meta does not show.

## Suggested fix

Do not emit a signer row for account creation, or mark it as derived; the master key is not a signer entry.

## Other entries with the same cause

4 problem(s) over 2 entries (mainnet, testnet):

- `cap33.sponsorship.p30-sponsored-account-meta`
