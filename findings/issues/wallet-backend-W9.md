<!-- repo: stellar/wallet-backend -->
# Question: CreateAccount writes a SIGNER ADD row for the new account's master key (Horizon parity). Intended for state changes?

## What happens

At 3e22515a44979fdc3a558fda3991ca2b48e38cef, [`internal/indexer/processors/effects_horizon.go` lines 122-133](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/effects_horizon.go#L122-L133): `addAccountCreatedEffects` emits a `signer_created` effect for the new account's master key with weight `keypair.DefaultSignerWeight`, which becomes a `SIGNER`/`ADD` state change. The file says it was ported from Horizon's effects processing, and Horizon does the same ([stellar-horizon `effects_processor.go`, `addAccountCreatedEffects`](https://github.com/stellar/stellar-horizon/blob/01ba44a988b3ebc2d68d3cb6747fa2c2733773cd/internal/ingest/processors/effects_processor.go#L460-L490)). It is also consistent with how set options is handled here: `SignerSummary()` counts the master key as a signer while its weight is above 0, so a later master weight change becomes a signer update or removal.

So this looks like intended Horizon parity, not a mistake. The reason to ask is what a consumer of state changes sees. In the ledger the new account entry has an empty signer list; the master key's weight is byte 0 of its thresholds. A `SIGNER`/`ADD` row for the master key has the same shape as one for a signer added by set options, so a consumer that rebuilds an account's signer entries from these rows, or counts signer additions, has to know the convention and compare the key with the account to tell them apart. Nothing in the row marks it as the master key.

Questions:

1. Is the master key meant to appear as a signer in state changes, as in Horizon's effects?
2. If so, would a marker on these rows (or a note in the state change docs) be welcome, so consumers do not read them as signer entries?

Severity as the harness classifies it against the ledger, which has no signer entry here: wrong-data (a stored fact the meta does not show). CAPs: CAP-0033 (the corpus entry is a sponsored account creation; the row appears for any CreateAccount).

## Minimal entry

Corpus entry `cap33.sponsorship.p30-sponsored-account-envelope` of the shapekit corpus: chain `cap33.sponsorship.p30`, ledger 65, recorded on a standalone stellar-core 30.0.0 under the testnet passphrase (the mainnet variant gives the same result). Transaction(s): `deaf08ab7a2f5b5baee797b4567f4eb2edc56d2397c31576d40ffa9a7b68dfed`.

## Reproduce

```sh
(cd adapters && go build -o bin/wb-adapter ./walletbackend/cmd/wb-adapter)
shapekit harness run --chain cap33.sponsorship.p30 --network testnet --adapter "adapters/bin/wb-adapter --commit 3e22515a44979fdc3a558fda3991ca2b48e38cef"
```

The harness replays the chain through a replay stellar-rpc and history archive into wallet-backend's live ingestion (TimescaleDB, `migrate up`, `ingest`), reads back `transactions`, `transactions_accounts` and `state_changes`, and compares them with what the ledger meta shows. Output: `harness-out/HARNESS.md`.

## Expected (ledger meta) and stored

From the harness report of the run above (`harness-report.json`, entry `cap33.sponsorship.p30-sponsored-account-envelope`, testnet):

- extra `signer-added`: signer-added is stored but the meta does not show it
  - stored: `{"account": "GD7H63KWIVUWQD73AP53NQMZBJYEHNYXCSPL22QNQE37MVS7PVQA3NE6", "signer": "GD7H63KWIVUWQD73AP53NQMZBJYEHNYXCSPL22QNQE37MVS7PVQA3NE6", "weight": 1}`

Verdict for the entry: mishandled (wrong-value): 0 wrong value(s), 1 fact(s) stored that the meta does not show.

## Possible changes, if a change is wanted

Mark the row as the master key (for example a reason or a flag), or leave master key weight to the account's thresholds and emit no signer row for it. If the Horizon convention is meant to stay, documenting it is enough.

## Other entries with the same cause

4 problem(s) over 2 entries (mainnet, testnet):

- `cap33.sponsorship.p30-sponsored-account-meta`
