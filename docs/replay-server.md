# Replay server

`shapekit serve` replays one recorded chain over HTTP. The replay harness starts one in
process for each chain and points the target at it (see
[adapter-interface.md](adapter-interface.md)). You can also run it by hand:

```sh
shapekit serve --chain cap73.sac-trust.p30 --network testnet --listen 127.0.0.1:8000
```

`--chain` takes a chain id under `corpus/chains` together with `--network` (`testnet` or
`mainnet`), or the path of a chain's network directory. `--listen` defaults to
`127.0.0.1:8000`. `--head` sets the visible head at start; without it the head is the
chain's last ledger.

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

The tests check this behaviour against captures from the public testnet endpoint
`https://soroban-testnet.stellar.org`, taken while it ran stellar-rpc 30.0.1 (server version
`30.0.1-6b6ec60c...`). The captures and their provenance are in
`crates/shapekit-serve/tests/fixtures/stellar-rpc/` (`getledgers.json`, captured
2026-11-12, and `gettransactions.json`, captured 2026-11-13). The tests in
`crates/shapekit-serve/tests` send the same requests to a server replaying a committed chain.
Where v30.0.0 and v30.0.1 differ (a `getLedgers` or `getTransactions` cursor at or past the
tip), the server follows v30.0.1.

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
| any other method | -32601 `method not found`, with the method name in `data`. This includes `simulateTransaction`, `getEvents` and `getFeeStats` |

`getLedgers`, `getLedgerEntries`, `getTransaction` and `getTransactions` accept `xdrFormat`:
`base64` (default) or `json`. `getLatestLedger` always answers in base64. In `json`
mode the `*Xdr` fields of `getLedgers` stay present and empty, as go-stellar-sdk leaves them,
and the `*Json` fields carry the SEP-51 form. `params` is an object or absent. Positional
(array) `params` is -32602 `invalid parameters`; a scalar `params` is -32600 (see
[Envelope](#envelope)).

### getLedgers

- `startLedger` must be between the oldest ledger and the head, unless a cursor is given;
  `startLedger` and `cursor` together are an error. The messages are those of go-stellar-sdk's
  `ValidatePagination`, joined with a format error as stellar-rpc joins them (-32600).
- `limit` defaults to 50 and is at most 200.
- A cursor is a ledger sequence; the page starts at the next ledger. A cursor below the oldest
  ledger is -32602. A cursor at or past the head is a caught-up poller: it gets an empty page
  with its own cursor echoed (the v30.0.1 behaviour). `cursor = 4294967295` wraps to ledger 0
  and is refused like any other cursor below the oldest ledger.
- The returned cursor is the last ledger of the page. For an empty page it is the request's
  cursor, or `startLedger - 1` when the request had no cursor.

### getTransactions

Paging is stellar-rpc v30.0.1's `getTransactionsByLedgerSequence` and
`processTransactionsInLedger` (`cmd/stellar-rpc/internal/methods/get_transactions.go`), step
for step:

- The cursor is a TOID, `ledger << 32 | applicationOrder << 12 | operation`, as a decimal
  int64 string (go-stellar-sdk `toid`). Every int64 parses. Text that is not an int64 is -32602
  with Go's `strconv.ParseInt` message, and a cursor whose ledger part is negative is -32602.
- The walk starts with the transaction after the cursor, or at the first transaction of
  `startLedger`. The cursor's transaction offset applies only within the cursor's own ledger,
  so a page that ended on a ledger's last transaction continues at the next ledger's first.
- `limit` defaults to 50 and is at most 200 (the same validation as `getLedgers`).
- The returned cursor is that of the last transaction returned, `toid(ledger, order, 1)`. When
  the walk passed a ledger without returning anything from it, the cursor is
  `toid(ledger, 0, 1)` of the last ledger passed.
- If that cursor would be lower than the request's own cursor (a caught-up poller whose cursor
  points at or past the tip), the server returns the request's cursor instead. The returned
  cursor therefore always fetches what comes next and never re-delivers a transaction.
- One call scans at most 10000 ledgers (`LedgerScanLimit`) and stops at the head, read once at
  the start of the call.
- A walk that starts below the oldest ledger is -32602 `database does not contain metadata for
  ledger: N`.
- Each transaction carries:
  - status (`SUCCESS` or `FAILED`), hash, application order and the fee bump flag;
  - envelope, result and meta;
  - diagnostic events, when present;
  - transaction events and per-operation contract events, as go-stellar-sdk's
    `GetTransactionEvents` splits them. With V4 meta every operation reports its events;
    with V3 meta only a Soroban transaction reports its events, as one operation;
  - ledger and `createdAt`.

## Envelope

- **Batches.** A JSON array is a batch of at most **10** calls (stellar-rpc's
  `MAX_REQUEST_BATCH_SIZE` default). A larger batch gets a batch reply holding one -32600 error
  (`batch exceeds the limit of 10 calls`) and nothing runs. An empty batch gets a single -32600
  error (`empty batch`).
- **Order.** Invalid members are answered first, then valid calls in request order (bridge.go
  `rank`).
- **Notifications.** A request with no id or a null id is a notification and gets no answer;
  a body of nothing but notifications gets HTTP 204 with no body.
- **Invalid members** are answered whether or not they carry an id. The codes and messages
  are jrpc2's:
  - a body that is not JSON: -32700 `invalid request value`;
  - a body or batch member that is not an object: -32700 `request is not a JSON object`, with
    a null id. JSON-RPC 2.0's examples use -32600 here; jrpc2, and so stellar-rpc, answer
    -32700. A `null` member is the exception: -32600 `invalid version marker`;
  - `jsonrpc` that is not a string: -32700 `invalid version key`; a string other than `"2.0"`,
    or no `jsonrpc` at all: -32600 `invalid version marker`;
  - an id that is neither string nor number: -32600 `invalid request ID`;
  - a method that is not a string: -32700 `invalid method name`;
  - scalar `params`: -32600 `parameters must be array or object`;
  - request and reply fields mixed: -32600 `mixed request and reply fields`;
  - unknown keys: -32600 `extra fields in request`, naming them in `data`.

  A call with an id and an empty or missing method passes these checks; it is answered in
  request order with -32600 `empty method name`.
- **Response budget.** All answers to one HTTP request share a budget of **64 MiB**
  (`MAX_RESPONSE_BYTES`). The server checks each call as its answer grows, one item at a
  time. A call that would take the request past the budget is answered instead with -32098
  `the response is larger than the node may hold; request fewer items` (bridge.go
  `errResponseTooLarge`, which jrpc2 reports as a system error). An answer of 4 KiB or less
  always fits, so cheap calls such as `getHealth` still answer when the budget is spent
  (bridge.go `smallResponseBytes`).
- **Memory bound.** stellar-rpc bounds the response bytes held across all requests
  (`max-total-response-bytes`, 2 GiB by default). The replay server bounds each request
  instead and answers at most 4 requests at once, so its response memory peaks at
  4 x 64 MiB. The largest single answer the committed corpus gives is 3.8 MB
  (`getTransactions` in json format on `classic.failed-tx.p30`).
- **Request bodies** above axum's default body limit of 2 MiB are refused with HTTP 413.

## Error codes

| Code | Meaning here |
|---|---|
| -32700 | body is not JSON, a member is not an object, or `jsonrpc` or `method` has the wrong JSON type |
| -32600 | invalid request (version marker, id, empty method, scalar params, mixed fields, extra fields, batch size), `getLedgers` and `getTransactions` pagination and format errors, `sendTransaction` |
| -32601 | method not implemented |
| -32602 | bad params: positional params, undecodable keys or hashes, bad cursors, `getLedgerEntries` over 200 keys or with a TTL key, `xdrFormat` other than base64 or json on `getLedgerEntries` and `getTransaction` |
| -32603 | internal: a recorded ledger could not be read |
| -32098 | the request's answers exceed the response budget |
