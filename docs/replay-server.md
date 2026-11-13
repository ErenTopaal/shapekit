# Replay server

`shapekit serve` replays one recorded chain over HTTP. The harness starts one per chain and
points the target at it (see [adapter-interface.md](adapter-interface.md)); it can also be run
by hand:

```sh
shapekit serve --chain cap73.sac-trust.p30 --network testnet --listen 127.0.0.1:8000
```

Every answer comes from the committed chain. Nothing is fetched from a network, and the server
accepts no transactions. Source: `crates/shapekit-serve`.

## Followed upstream version

The JSON-RPC side follows **stellar-rpc v30.0.1** (tag `v30.0.1`, commit
`6b6ec60c8bde5a93984b6290065d3882d384b25d`, release image `stellar/stellar-rpc:30.0.1`,
`sha256:b6d41d3148087adf5d6856380a40854bd1e25b29d62a7ae484258e290e73b096`; all in `pins.toml`).
Request and response types are those of go-stellar-sdk v0.8.0 `protocols/rpc`. The envelope
follows stellar-rpc's HTTP bridge (`cmd/stellar-rpc/internal/jsonrpc/bridge.go`), its batch
limit (`jsonrpc/batchlimit.go`) and the request parser of jrpc2 v1.3.3, which the bridge uses
unchanged.

The behaviour was checked against the public testnet endpoint while it ran stellar-rpc
30.0.1 (server version `30.0.1-6b6ec60c...`). The captures and their provenance are in
`crates/shapekit-serve/tests/fixtures/stellar-rpc/` (`getledgers.json`, captured
2026-11-12, and `gettransactions.json`, captured 2026-11-13), and `crates/shapekit-serve/tests`
replays the same requests against a committed chain. Where v30.0.0 and v30.0.1 differ (a
`getLedgers` or `getTransactions` cursor at or past the tip), v30.0.1 is followed.

## Routes

| Route | Answers |
|---|---|
| `POST /`, `POST /rpc` | JSON-RPC 2.0, the methods below |
| `GET /ledgers/<key>` | the chain's SEP-54 lake: `.config.json` and every batch whose ledgers are all at or below the head |
| `GET /archive/<path>` | the history archive as it stood at the newest checkpoint at or below the head, including `.well-known/stellar-history.json` |
| `GET /control/head` | `{"head": N, "first": F, "last": L}`: the visible head and the chain's first and last ledger |
| `POST /control/head` | `{"ledger": N}` moves the head; the harness releases ledgers one at a time this way |

The head is the newest ledger a client can see. Ledgers above it do not exist for any route:
`getLatestLedger` reports the head, paging stops at it, and lake batches and archive
checkpoints past it are not served.

## Methods

| Method | Behaviour |
|---|---|
| `getHealth` | `status: "healthy"`, latest and oldest ledger with close times, and `ledgerRetentionWindow` = head - first ledger + 1 (every released ledger is retained) |
| `getNetwork` | the chain's passphrase and the protocol of the head ledger. No `friendbotUrl`, as stellar-rpc omits it when none is configured |
| `getVersionInfo` | `version: "shapekit-replay <version>"`, empty `commitHash` and `buildTimestamp`, `captiveCoreVersion` naming the core image that recorded the chain, and the head's protocol |
| `getLatestLedger` | id (header hash), protocol, sequence, close time, `headerXdr` and `metadataXdr` of the head |
| `getLedgers` | see [getLedgers](#getledgers) |
| `getLedgerEntries` | up to 200 keys (`getLedgerEntriesMaxKeys`), entries as of the head, with `liveUntilLedgerSeq` for contract data and code. TTL keys are refused as stellar-rpc refuses them |
| `getTransaction` | by hex hash. A transaction above the head is `NOT_FOUND`, with the same fields stellar-rpc sends for one |
| `getTransactions` | see [getTransactions](#gettransactions) |
| `sendTransaction` | always -32600: the server is read only |
| anything else | -32601 `method not found`, the method name in `data`. `simulateTransaction`, `getEvents`, `getFeeStats` and the other write or derived methods are not implemented |

Every method that returns XDR accepts `xdrFormat`: `base64` (default) or `json`. In `json`
mode the `*Xdr` fields of `getLedgers` stay present and empty, as go-stellar-sdk leaves them,
and the `*Json` fields carry the SEP-51 form. Params are an object or absent, never positional:
a positional or scalar `params` is -32602 `invalid parameters`.

### getLedgers

- `startLedger` must be between the oldest ledger and the head, unless a cursor is given;
  `startLedger` and `cursor` together are an error. The messages are those of go-stellar-sdk's
  `ValidatePagination`, joined with a format error as stellar-rpc joins them (-32600).
- `limit` defaults to 50 and may not exceed 200.
- A cursor is a ledger sequence; the page starts at the next ledger. A cursor below the oldest
  ledger is -32602. A cursor at or past the head is a caught-up poller: it gets an empty page
  with its own cursor echoed (the v30.0.1 change). `cursor = 4294967295` wraps to ledger 0 and
  is refused the same way as any cursor below the oldest ledger.
- The returned cursor is the last ledger of the page, or for an empty page the request's
  cursor (or `startLedger - 1`).

### getTransactions

Paging is stellar-rpc v30.0.1's `getTransactionsByLedgerSequence` and
`processTransactionsInLedger` (`cmd/stellar-rpc/internal/methods/get_transactions.go`), step
for step:

- The cursor is a TOID, `ledger << 32 | applicationOrder << 12 | operation`, as a decimal
  int64 string (go-stellar-sdk `toid`). Every int64 parses; text that is not an int64 is -32602
  with Go's `strconv.ParseInt` message, and a cursor whose ledger part is negative is -32602.
- The walk starts with the transaction after the cursor, or at the first transaction of
  `startLedger`. The cursor's transaction offset applies only within the cursor's own ledger,
  so a page that ended on a ledger's last transaction continues at the next ledger's first.
- `limit` defaults to 50 and may not exceed 200 (same validation as `getLedgers`).
- The returned cursor is that of the last transaction returned, `toid(ledger, order, 1)`. When
  the walk passed a ledger without returning anything from it, it is `toid(ledger, 0, 1)` of
  the last ledger passed.
- A cursor that would come out lower than the request's own (a caught-up poller whose cursor
  points at or past the tip) is replaced by the request's cursor, so the returned cursor always
  fetches what comes next and never re-delivers a transaction.
- One call scans at most 10000 ledgers (`LedgerScanLimit`) and stops at the head, read once at
  the start of the call.
- A walk that starts below the oldest ledger is -32602 `database does not contain metadata for
  ledger: N`.
- Each transaction carries status (`SUCCESS` or `FAILED`), hash, application order, fee bump
  flag, envelope, result and meta, diagnostic events when present, transaction and per
  operation contract events as `GetTransactionEvents` splits them (V4 meta; for V3 meta only a
  Soroban transaction reports its events, as one operation), ledger and `createdAt`.

## Envelope

- **Batches.** A JSON array is a batch of at most **10** calls (stellar-rpc's
  `MAX_REQUEST_BATCH_SIZE` default). A larger batch gets a batch reply holding one -32600 error
  (`batch exceeds the limit of 10 calls`) and nothing runs. An empty batch gets a single -32600
  error (`empty batch`).
- **Order.** Invalid members are answered first, then valid calls in request order (bridge.go
  `rank`).
- **Notifications.** A request with no id or a null id is a notification and gets no answer;
  a body of nothing but notifications gets HTTP 204 with no body.
- **Invalid members** are answered whether or not they carry an id, with jrpc2's codes: a body
  or member that is not an object is -32700 `request is not a JSON object` with a null id
  (JSON-RPC 2.0's examples use -32600 there; jrpc2, and so stellar-rpc, answer -32700). A wrong
  or missing `jsonrpc: "2.0"` is -32600 `invalid version marker`; an id that is neither string
  nor number is -32600 `invalid request ID`; a non-string method is -32700 `invalid method
  name`; scalar params are -32600; request and reply fields mixed is -32600; unknown keys are
  -32600 `extra fields in request`, naming them in `data`. A body that is not JSON is -32700
  `invalid request value`.
- **Response budget.** All answers to one HTTP request share a budget of **64 MiB**
  (`MAX_RESPONSE_BYTES`). A call is checked as its answer grows, one item at a time; a call
  that would take the request past the budget is answered instead with -32098 `the response is
  larger than the node may hold; request fewer items` (bridge.go `errResponseTooLarge`, which
  jrpc2 reports as a system error). Answers of 4 KiB or less always fit, so cheap calls such as
  `getHealth` still answer when the budget is spent (bridge.go `smallResponseBytes`).
  stellar-rpc bounds the bytes held across all requests (`max-total-response-bytes`, 2 GiB by
  default); the replay bounds each request instead, and at most 4 requests are answered at
  once, so its response memory peaks at 4 x 64 MiB. The largest single answer the committed
  corpus gives is 3.8 MB (`getTransactions` in json format on `classic.failed-tx.p30`).
- **Request bodies** above axum's default body limit of 2 MiB are refused with HTTP 413.

## Error codes

| Code | Meaning here |
|---|---|
| -32700 | body is not JSON, or a member is not an object, or a field has the wrong JSON type |
| -32600 | invalid request (version marker, id, batch size, extra fields), `getLedgers` and `getTransactions` pagination and format errors, `sendTransaction` |
| -32601 | method not implemented |
| -32602 | bad params: positional params, undecodable keys or hashes, bad cursors, `getLedgerEntries` over 200 keys or with a TTL key, `xdrFormat` other than base64 or json on `getLedgerEntries` and `getTransaction` |
| -32603 | internal: a recorded ledger could not be read |
| -32098 | the request's answers exceed the response budget |
