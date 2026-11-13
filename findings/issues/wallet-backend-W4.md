<!-- repo: stellar/wallet-backend -->
# To_muxed_id is kept only when it is a u64; a text, hash or return memo carried by the event is lost

## What happens

At 3e22515a44979fdc3a558fda3991ca2b48e38cef, [`internal/indexer/processors/token_transfer.go` line 123](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/token_transfer.go#L123): `setToMuxedID` keeps the CAP-67 `to_muxed_id` only when it is an id (`ttp.MuxedInfo_Id`). Core writes the transaction memo there when the destination is not muxed: text as `string`, hash and return as `bytes`. Those values are dropped, so the stored credit has no `to_muxed_id` although the event carries one.

Severity: data-loss. CAPs: CAP-0019, CAP-0024, CAP-0067.

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

## Suggested fix

Store `to_muxed_id` with its type (u64, string, bytes), as CAP-67 defines it, or keep the memo it carries in another column.

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
