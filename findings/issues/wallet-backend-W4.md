<!-- repo: stellar/wallet-backend -->
# Question: to_muxed_id keeps only u64 values, so a text, hash or return memo of a payment to a G address is not stored. Intended?

## What happens

At 3e22515a44979fdc3a558fda3991ca2b48e38cef, [`internal/indexer/processors/token_transfer.go` lines 119-124](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/token_transfer.go#L119-L124): `setToMuxedID` copies the CAP-67 `to_muxed_id` of a transfer or mint event into the credit row only when its content is an id (`ttp.MuxedInfo_Id`). The comment above it says this is deliberate: "text and hash memos are not ids and are left out". The column is documented as the "CAP-67 destination memo (u64)" (`internal/indexer/types/types.go` line 917).

So this may well be intended. The reason to ask is what it costs a consumer. For a classic payment to a G address that carries a text, hash or return memo, core writes that memo into the event's `to_muxed_id` (typed as `string` or `bytes`). That is the common shape of a deposit to an exchange or custodian that identifies the customer by a text memo. At this commit no other table seems to keep it either: the `transactions` table (`internal/db/migrations/2025-06-10.2-transactions.sql`) has no memo column, and `operations.operation_xdr` holds the operation, not the transaction's memo. If that reading is right, an indexer consumer cannot recover which customer a text-memo deposit was for from wallet-backend's data, while it can for a MEMO_ID deposit or an M address.

Questions:

1. Is dropping text, hash and return memos from `to_muxed_id` the intended behaviour?
2. If so, is the memo meant to be available somewhere else (another table or the API) that I missed?

Severity as the harness classifies it against the ledger: data-loss (the event carries a value that no stored row has). CAPs: CAP-0019, CAP-0024, CAP-0067.

## Minimal entry

Corpus entry `classic.memo-bytes.p30-memo-ff` of the shapekit corpus: chain `classic.memo-bytes.p30`, ledger 65, recorded on a standalone stellar-core 30.0.0 under the testnet passphrase (the mainnet variant gives the same result). Transaction(s): `0da58ec0335794e936efbb99b0c03fe10107294a01a9f62a27937ab32ff60dfa`.

## Reproduce

```sh
(cd adapters && go build -o bin/wb-adapter ./walletbackend/cmd/wb-adapter)
shapekit harness run --chain classic.memo-bytes.p30 --network testnet --adapter "adapters/bin/wb-adapter --commit 3e22515a44979fdc3a558fda3991ca2b48e38cef"
```

The harness replays the chain through a replay stellar-rpc and history archive into wallet-backend's live ingestion (TimescaleDB, `migrate up`, `ingest`), reads back `transactions`, `transactions_accounts` and `state_changes`, and compares them with what the ledger meta shows. Output: `harness-out/HARNESS.md`.

## Expected (ledger meta) and stored

From the harness report of the run above (`harness-report.json`, entry `classic.memo-bytes.p30-memo-ff`, testnet):

- wrong-value `token-event`: token-event differs in to_muxed_id
  - ledger: `{"amount": "1", "contract": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC", "event": "transfer", "from": "GDWC5XJM77XUV2YB2J4JB6I3MY3M7IJFWFMWDLD6P25JRDF2VMQBMQAG", "to": "GAVB7A4ZGZEHRUDGWXMSBHPLNQK3QVBI5NBNYZ5YL5MYNNMP4KV4NAS4", "to_muxed_id": {"type": "string", "value": "ff"}}`
  - stored: `{"amount": "1", "contract": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC", "event": "transfer", "from": "GDWC5XJM77XUV2YB2J4JB6I3MY3M7IJFWFMWDLD6P25JRDF2VMQBMQAG", "to": "GAVB7A4ZGZEHRUDGWXMSBHPLNQK3QVBI5NBNYZ5YL5MYNNMP4KV4NAS4", "to_muxed_id": null}`

Verdict for the entry: mishandled (wrong-value): 1 wrong value(s), 0 fact(s) stored that the meta does not show.

## Possible changes, if this is not intended

Store `to_muxed_id` with its CAP-67 type (u64, string or bytes), or store the transaction memo in its own column. If it is intended, a line in the schema or API docs saying that only id memos are kept would let consumers know to read memos elsewhere.

## Other entries with the same cause

34 problem(s) over 17 entries (mainnet, testnet):

- `cap67.unified-events.p22-backfill-payment-memo-hash`
- `cap67.unified-events.p22-backfill-payment-memo-text`
- `cap67.unified-events.p30-payment-memo-hash`
- `cap67.unified-events.p30-payment-memo-text`
- `classic.max-length.p30-memo-text-28`
- `classic.memo-arms.p30-memo-hash-ff`
- `classic.memo-arms.p30-memo-hash-sha256`
- `classic.memo-arms.p30-memo-hash-zero`
- `classic.memo-arms.p30-memo-return-ff`
- `classic.memo-arms.p30-memo-return-sha256`
- `classic.memo-arms.p30-memo-return-zero`
- `classic.memo-bytes.p30-memo-above-max-code-point`
- `classic.memo-bytes.p30-memo-lone-continuation`
- `classic.memo-bytes.p30-memo-overlong-nul`
- `classic.memo-bytes.p30-memo-surrogate`
- `classic.memo-bytes.p30-memo-truncated-sequence-28`

## Related

- https://github.com/stellar/wallet-backend/issues/800
