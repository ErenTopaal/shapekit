# Adapter interface

The replay harness (`shapekit harness run`) feeds recorded chains to an indexer and checks
what the indexer stored against the meta oracle. It never talks to the indexer directly. An
**adapter** sits in between: a program that starts the indexer, points it at the harness's
ledger delivery endpoints, reports how far it got, and reads back what it stored in the
oracle's vocabulary.

Protocol version: **2**. The harness still drives version 1 adapters (see "Versions"). The
machine-readable form is `schema/adapter-protocol.schema.json`,
generated from `crates/shapekit-harness/src/protocol.rs` together with the vocabulary section
at the end of this page (`shapekit harness schema --write`; `cargo test -p shapekit-harness`
fails when either is stale).

The reference adapter is `adapters/walletbackend` (wallet-backend, Go).

## What the harness does with an adapter

For each chain (one recorded ledger sequence per shape family, protocol and passphrase):

1. Starts the adapter program and sends `describe`. When the chain's protocol is outside the
   adapter's `protocols`, the harness sends `stop` right away and the chain's entries are
   `out-of-scope`.
2. Creates a Docker network with `--internal` and starts the replay server
   (`shapekit-serve`, in process) bound to that network's gateway address on a free port.
   The visible head is set to the bootstrap ledger (see "Bootstrap").
3. Sends `start`, then waits until `status` reports `last_ingested_ledger` at or past the
   bootstrap ledger.
4. For every later ledger of the chain in order: moves the server's head to it, checks that
   the server answers `getLedgers` for it (a ledger the server cannot serve is a harness
   error, never a target verdict), sends `advance`, and polls `status` until the target has
   ingested it, the target died, or it stopped making progress.
5. Sends `export` for the ledgers from the one after the bootstrap ledger to the last ledger
   the target ingested, and compares the records with the oracle files of the chain's corpus
   entries. When the adapter offers `dump`, it then asks for every stored table and keeps
   the answer (see "dump").
6. Sends `stop`, keeps the returned logs, stops the replay server, and removes the network
   and the work directory.

The harness starts a new adapter process for every chain, so a target that halts in one
family does not hide the others.

## Transport

- The harness starts the adapter as a child process with the command line it was given and
  talks to it over stdin and stdout. The working directory is the harness's.
- Every message is one JSON object encoded as UTF-8 on one line, terminated by `\n`. No
  pretty printing, no blank lines, no byte order mark.
- The harness sends one request and reads response lines until the final line for that
  request (`status` `ok` or `error`) before it sends the next. Every response line carries the
  request's `id`. A line with another `id` is a protocol violation and ends the chain's run.
- Only `export` and `dump` have more than one response line: any number of `record` lines
  (`export`) or `table` and `row` lines (`dump`), then the
  final line.
- stdout carries responses only. Logs go to stderr; the harness saves the adapter's stderr
  next to the report (`logs/<chain>.<network>.adapter.log`).
- Members the receiver does not know must be ignored, so later protocol versions can add
  optional members.
- When stdin reaches end of file the adapter tears the target down as for `stop` and exits.
- The harness enforces a timeout per request. Defaults: `describe` 60 s, `start` 30 min
  (the adapter can build images during `start`), `advance` 120 s, `status` 300 s, `export`
  10 min, `stop` 5 min. `--start-timeout` and `--status-timeout` change the two that depend
  on the host. A status timeout ends the chain, so the status default leaves room for a
  heavily loaded host: the wallet-backend adapter's status answer took more than 60 s at a
  load average near 200.
- Two more limits apply to the target's progress, not to single requests: how long the
  target can go without ingesting a ledger while it bootstraps (`--bootstrap-timeout`,
  default 600 s) and after each released ledger (`--ledger-timeout`, default 120 s). When
  one passes, the target has stopped at that ledger (see "Verdicts").
- On a timeout, a protocol violation, an error response or an adapter exit, the harness sends
  `stop` where it still can and records every entry of the chain as `not-evaluated` with the
  reason. After `stop`, and on every other path, it closes the adapter's stdin, waits up to
  30 s for the adapter to exit, and then kills it.
- When a request times out, the harness stops waiting for it but remembers its id. If that
  request's lines arrive later, while the harness waits for the reply to a later request
  (usually `stop`), the harness skips them, up to and including their final line, and lists
  them in the chain's `notes`. This is the one exception to the id rule above. A line with an
  id the harness never sent, or with the id of a request that was already answered, is still
  a protocol violation.
- Attach every container the adapter starts to `docker_network`. Before the harness removes
  the network, it removes every container still attached to it, so a killed adapter leaves
  nothing behind.

### Request

```json
{"id": "3", "cmd": "advance", "head": 65}
```

- `id`: string, unique within one adapter process.
- `cmd`: `describe`, `start`, `advance`, `status`, `export`, `dump`, `restart` or `stop`. The command's members
  sit next to `cmd`.

### Response

```json
{"id": "3", "status": "ok", "result": {"head": 65}}
{"id": "7", "status": "record", "record": {"type": "transaction", "ledger": 65, "hash": "...", "participants": [], "changes": []}}
{"id": "4", "status": "error", "error": {"class": "target", "message": "timescaledb did not accept connections within 120s"}}
```

### Error classes

| class | meaning | what the harness does |
|---|---|---|
| `bad-request` | the request does not follow this protocol | stops the chain, a harness bug |
| `unsupported` | valid, but the adapter cannot do it (a delivery mode it lacks) | stops the chain |
| `target` | the target could not be built, started or queried | stops the chain; its entries are `not-evaluated` with the message |
| `internal` | the adapter itself failed | as `target` |

An error never stands for a verdict. A target that crashes or halts while ingesting is
reported through `status`, not through an error response.

## Commands

### describe

Request: `{"id": "0", "cmd": "describe", "protocol": 2}`

Result:

| member | type | meaning |
|---|---|---|
| `protocol` | integer | the version the adapter speaks: the requested one, or an older one the harness drives (`1`) |
| `adapter`, `adapter_version` | string | the adapter program and its version |
| `target` | string | the indexer, for example `wallet-backend` |
| `target_version` | string | the exact build, for example a full commit hash, never a branch name |
| `delivery_modes` | array | modes the adapter can drive: `sep54-filesystem`, `sep54-s3`, `rpc` |
| `default_delivery` | string | mode used unless the harness is given `--delivery`; one of `delivery_modes` |
| `bootstrap` | `checkpoint` or `ledger` | how the target starts, see "Bootstrap" |
| `vocab_version` | integer | the oracle vocabulary version the claims and exports use; must equal the harness's |
| `protocols` | `{"min", "max"}` | ledger protocols the target build ingests; chains outside are `out-of-scope` |
| `claims` | object | what the target records, see "Claims" |
| `capabilities` | array | optional commands the adapter answers: `dump`, `restart`. Version 2; absent means none |

#### Versions

The harness asks for version 2. An adapter that does not speak it answers with an
`unsupported` error, and the harness sends `describe` again with `"protocol": 1` in the same
process. A version 1 adapter runs exactly as before: no `capabilities`, no `dump`, and so no carry check. Any other error, or an answer
naming a version above the one asked for, ends the chain.

### start

Request members (besides `id` and `cmd`):

| member | type | meaning |
|---|---|---|
| `chain` | string | chain id, for example `cap40.signed-payload-signer.p28` |
| `network` | string | `testnet` or `mainnet`, the passphrase variant replayed |
| `network_passphrase` | string | the passphrase to configure the target with |
| `protocol` | integer | ledger protocol of the chain |
| `delivery.mode` | string | the mode the harness picked |
| `delivery.rpc_url` | string | JSON-RPC endpoint of the replay server ([replay-server.md](replay-server.md): methods, batch and response limits, paging, error codes); present in every mode |
| `delivery.archive_url` | string | history archive root; the archive as it stood at the newest checkpoint at or below the head |
| `delivery.lake.url` | string | SEP-54 lake root over HTTP: `.config.json` and the batches whose ledgers are all visible |
| `delivery.lake.path` | string | the same lake as a directory on the harness host (every batch, including unreleased ones) |
| `delivery.lake.ledgers_per_batch`, `batches_per_partition` | integer | the lake's layout |
| `ledgers.bootstrap` | integer | the visible head while `start` runs |
| `ledgers.last` | integer | the chain's last ledger |
| `ledgers.checkpoint_frequency` | integer | `64` |
| `limits.memory_bytes` | integer | memory limit to apply to the target's ingesting process or containers (`--memory-limit`, default 4 GiB) |
| `docker_network` | string | the internal network the target's containers must join; absent with `--no-docker` |
| `workdir` | string | an empty directory for this chain; the harness removes it afterwards |

The adapter brings the target up and answers once the ingesting process runs. It configures
the target with the passphrase and the endpoints of its delivery mode only. The target gets
no other network access: containers join `docker_network` only, which has no route off the
host. The replay server URLs point at the gateway address of that network.

Result: `components`, an object of free-form strings (image ids, versions of services the
target needs) that the report keeps.

### advance

Request: `{"cmd": "advance", "head": N}`. Ledgers up to `N` are now visible on the replay
server. In `rpc` mode the target sees them through `getLedgers` on its own and the adapter
just answers. In `sep54-filesystem` and `sep54-s3` modes the adapter copies the batches whose
last ledger is at or below `N` (from `lake.url`, or from `lake.path` skipping later batches)
into the directory or bucket the target reads, then answers.

Result: `head`, equal to `N`.

### status

Result:

| member | type | meaning |
|---|---|---|
| `last_ingested_ledger` | integer or null | highest ledger durably ingested, read from the target's own store (wallet-backend: `ingest_store` cursor). Not a log line, not a metric that runs ahead of the commit |
| `health` | string | `starting` (bootstrapping), `running` (ingest process alive), `crashed` (exited or killed, not by the memory limit), `oom` (killed for exceeding `limits.memory_bytes`) |
| `last_error` | string or null | the most recent error the target reported since `start`, verbatim |
| `peak_memory_bytes` | integer or null | peak memory of the target since `start` |

`health: running` does not mean healthy. A target that retries the same ledger forever is
alive and running; the harness calls it halted when it stops advancing while `last_error`
is set (see "Verdicts").

### export

Request: `{"cmd": "export", "from": A, "to": B}`. Stream one `record` line per stored
transaction in ledgers `A` to `B` inclusive, in ledger and apply order, then
`{"status": "ok", "result": {"records": N}}` where `N` counts the record lines. Stream from
the database cursor; do not build the whole answer in memory.

Records are of two types:

```json
{"type": "transaction", "ledger": 65, "hash": "6f9d...", "participants": [...], "changes": [...], "unmapped": [...]}
{"type": "unattributed", "ledger": 65, "unmapped": [...]}
```

`transaction`:

| member | meaning |
|---|---|
| `ledger` | ledger sequence |
| `hash` | lowercase hex transaction hash. For a fee bump, the outer hash (the oracle's `hash`) |
| `participants` | every participant stored for the transaction: `{"address", "mux_ids", "raw"}` |
| `changes` | every stored state change the adapter maps to a vocabulary kind |
| `unmapped` | stored rows of this transaction the adapter has no mapping for |

A participant's `address` is the canonical strkey (`G`, `C`, `L` or `B`; never `M`, the mux
id goes in `mux_ids` as decimal strings). When the stored bytes do not decode, `address` is
`null` and `raw` says why.

A change is a vocabulary change (see the generated section below) with only the fields the
target stores: `kind`, the claimed fields, optionally `scope` and `op_index`, and

- `raw`: for every address field read from stored bytes, `{"column", "hex", "strkey"}` when
  the bytes decode or `{"column", "hex", "error"}` when they do not. In the second case the
  field itself is `null`. `hex` is the stored value exactly, for wallet-backend's
  `AddressBytea` the version byte followed by the key bytes. Adapters must not repair, pad or
  truncate the bytes before decoding: a 33-byte value that cannot hold a 40-byte muxed key is
  exactly what the harness is looking for.
- `source`: `{"table", "category", "reason"}`, the row the change came from.

Values use the vocabulary's encodings exactly: amounts, balances, limits, ids and sequence
numbers as decimal strings (`"-100"`), 32-bit values as JSON integers, assets as `native` or
`CODE:ISSUER` (SEP-11), pool-share trustline assets as `pool-share:<L strkey of the pool id>`, hashes and
XDR as lowercase hex or base64 as the field's description says, signers as full strkeys
including `P...` signed-payload signers and their payload.

`unmapped` rows: `{"table", "category", "reason", "row", "note"}`. `row` holds every column
(byte columns as lowercase hex); `note` says why there is no mapping. Rows are never dropped
silently: an adapter that cannot map a row reports it here, and the report lists it next to
the entry so a reviewer can tell an indexer drop from an adapter gap.

### stop

Result: `logs`, the tail of the target's logs (at most 1 MiB). The adapter removes every
container, volume and file it created, then exits with status 0.

### dump

Version 2, sent only to an adapter whose `describe` lists the `dump` capability. Request:
`{"cmd": "dump"}`, or `{"cmd": "dump", "tables": ["state_changes"]}` for named tables only.
Stream every table the target stored, each as one `table` line followed by its `row` lines,
then `{"status": "ok", "result": {"tables": T, "rows": R}}` where `T` and `R` count the lines
sent:

```json
{"id": "9", "status": "table", "table": {"name": "transactions_accounts", "columns": [{"name": "tx_to_id", "type": "bigint"}, {"name": "account_id", "type": "bytea"}]}}
{"id": "9", "status": "row", "row": {"table": "transactions_accounts", "cells": ["279172878336", "30aa..."]}}
{"id": "9", "status": "ok", "result": {"tables": 1, "rows": 1}}
```

- `columns[].type` is the store's own type name (PostgreSQL: `format_type`, for example
  `bytea`, `bigint`, `character varying(255)`).
- `cells` follow `columns`. A byte column's cell is the stored bytes as lowercase hex with
  no prefix. Every other cell is the store's own text form of the value (PostgreSQL: the
  value cast to `text`). SQL NULL is `null`.
- Dump what is stored, unchanged: no decoding, no mapping, no repair. The dump is the
  evidence the relations search; anything the adapter would interpret belongs in `export`.
- A row of a table whose `table` line has not been sent, a table sent twice, and a row with
  a different number of cells than its table has columns are protocol violations.
- Unknown table names in `tables` are a `bad-request` error.

The harness sends `dump` after `export` and before `stop`, only when the target ingested a
ledger after the bootstrap ledger, and writes the answer to
`logs/<chain>.<network>.dump.json`. `--no-dump` skips it. A failed dump does not change any
verdict; the chain lists the error and the run exits non-zero.

### restart

Version 2, sent only to an adapter whose `describe` lists the `restart` capability, and only
with `harness run --restart`. Request: `{"cmd": "restart"}`. Stop the target's ingesting
process and start it again on the same store, the way a deployment restarts it, then answer
`{}`. The harness then releases the next ledger as usual; a target that does not resume
stops there like any other.

## Claims

`describe.claims` says which oracle facts the target records:

```json
{
  "participants": {"address_kinds": ["account", "contract"], "mux_ids": true},
  "changes": [
    {"kind": "signer-added", "fields": ["account", "signer", "weight"]},
    {"kind": "trustline-created", "fields": ["account", "asset", "limit"]},
    {"kind": "token-event", "where": {"event": ["transfer", "mint", "burn", "clawback"]},
     "fields": ["event", "contract", "from", "to", "amount"]}
  ]
}
```

- `participants`: the oracle's actual participants (never the envelope's declared ones) whose
  address kind is listed are expected among the exported participants. With `mux_ids`, their
  mux ids are compared too.
- each `changes` item claims the oracle changes of `kind` that satisfy `where` (every listed
  field equals one of its values; an absent field equals `null`), compared on `fields` only.

A claim is a promise. Anything inside it that the export lacks is a finding; anything
outside it is `out-of-scope`. Claim what the target is meant to record, not what it happens
to get right.

## Bootstrap

`describe.bootstrap` says how the target starts:

- `checkpoint`: the target reads the archive's `.well-known/stellar-history.json`, loads the
  ledger state of that checkpoint, and starts ingesting at the checkpoint ledger.
  wallet-backend live mode on an empty database does this at every wallet-backend commit
  this repository runs (`internal/services/ingest_live.go`:
  `archive.GetLatestLedgerSequence()`, then `PopulateFromCheckpoint`, then ingestion from
  that ledger). The harness therefore picks the newest checkpoint before the chain's first
  entry ledger as `ledgers.bootstrap` and serves the archive as of that checkpoint.
- `ledger`: the target can be told a start ledger. `ledgers.bootstrap` is the chain's first
  ledger.

Every recipe chain has a checkpoint before its first shape: after the protocol upgrade in
ledger 2, `shapekit gen chains` closes empty ledgers through ledger 63 (core's first
checkpoint at the default frequency of 64) before it applies network settings and runs the
recipe. The archive in each chain's `archive.tar` holds that checkpoint. For a `checkpoint`
target, a chain with no checkpoint before its first entry ledger does not run: its entries are
`not-evaluated`, and the reason says to regenerate the chain with `shapekit gen chains`.
Entries at or before the bootstrap ledger are always `not-evaluated`: the harness compares
only the ledgers it releases after the bootstrap ledger.

## Network access

Containers reach the replay server at the internal network's gateway address, which is the
host side of the network's bridge. A container on an `--internal` network has no route off
the host. The harness binds the replay server to the gateway address only, not to every
interface of the host.

With `--no-docker` the harness creates no network and binds the server to `127.0.0.1`, for
adapters whose target runs as a host process.

## Verdicts

The harness gives every corpus entry of the chain that has an oracle file one verdict. The
report counts the selected entries it cannot replay separately: entries with no chain
(history captures, goldens, synthetic entries, vectors) and entries with no oracle file for
the network (ledger entries and other values that are not a transaction or a ledger).

| verdict | reason | when |
|---|---|---|
| `pass` | | every in-scope fact of the entry's transactions was exported with equal values, and nothing undecodable or extra was stored |
| `dropped` | | an in-scope participant or change has no counterpart in the export (or the target stored nothing for the transaction) |
| `mishandled` | `wrong-value` | an in-scope fact was stored with a different value (paired by the kind's key fields), or the target stored a participant or change the meta does not show |
| `mishandled` | `undecodable-address` | a stored address in the entry's transactions does not decode (`raw.error`) |
| `mishandled` | `halted` | the target stopped advancing at the entry's ledger with `last_error` set |
| `mishandled` | `crashed` | the ingest process died at the entry's ledger (`health: crashed`) |
| `mishandled` | `oom` | it was killed for its memory limit at the entry's ledger (`health: oom`), or `status` reported a `peak_memory_bytes` above `limits.memory_bytes` |
| `mishandled` | `timeout` | it stopped advancing at the entry's ledger without reporting an error |
| `not-evaluated` | | the target stopped at an earlier ledger (the result names the entry), the entry is at or before the bootstrap ledger, or the chain could not be run (the result says why) |
| `out-of-scope` | | the entry has no fact the target claims, the chain's protocol is outside `protocols`, or every comparable fact matched but a claimed event fact cannot be checked because the meta format has no place for it (see "Facts the meta cannot show") |

A target stops "at" ledger `L` when `L` is the first ledger it did not ingest. Every entry
in `L` gets the stop verdict (`halted`, `crashed`, `oom` or `timeout`) without a comparison,
and entries in later ledgers are `not-evaluated`. The harness compares every other entry,
lists every problem it finds in the report, and picks the verdict from the first problem in
the order undecodable-address, wrong-value, dropped.

### Facts the meta cannot show

`token-event` and `contract-event` facts come only from events, and older meta formats have
no place for most events. The oracle records, per transaction, `events`:

| `events` | meta | events present |
|---|---|---|
| `unified` | `TransactionMeta` v4 (protocol 23 and later, and protocol 22 ledgers recorded with CAP-67 backfill) | every operation's events, classic operations included, and the fee events |
| `soroban-operation` | v3 with Soroban meta | the invoked contract's own events, at operation scope |
| `none` | v0 to v3 without Soroban meta | none |

A claim on an event kind can match events at certain scopes. With a `where.stage` filter,
those are the scopes of the listed stages: `before-all-txs` (fee processing), `operation`,
`after-tx` and `after-all-txs` (post-apply fee). Without one, the claim can match every event
scope. When the transaction's `events` coverage lacks any of those scopes, the claim is not
comparable for that transaction, and the harness compares neither side of it: an oracle fact
is not expected, an exported fact is not extra, and the entry lists the claim in its detail.

The verdict then follows the comparable facts. A problem there gives `dropped` or
`mishandled` as usual. Otherwise the entry is `out-of-scope`, never `pass`, because a pass
would say the event claims were checked. Example: on
`cap67.unified-events.p22-payment-transfer` (protocol 22, classic events off, meta v3)
wallet-backend derives a transfer and a fee from the operation. The meta shows neither, so
the export can be neither confirmed nor contradicted there.

The harness refuses an oracle file whose transactions have no `events` member; `shapekit
oracle run` rewrites it.

Pairing for `wrong-value`: an expected change and an exported change of the same kind are a
pair when they agree on the kind's key fields that the claim compares:

| kind | key fields |
|---|---|
| `account-created`, `account-removed`, `thresholds`, `account-flags`, `home-domain`, `inflation-destination`, `sequence` | `account` |
| `balance-change` | `holder`, `asset`, `held_in` |
| `trustline-created`, `trustline-removed`, `trustline-flags`, `trustline-limit` | `account`, `asset` |
| `signer-added`, `signer-removed`, `signer-weight` | `account`, `signer` |
| `data-entry` | `account`, `name_hex` |
| `sponsorship` | `target`, `signer` |
| `claimable-balance-created`, `claimable-balance-claimed`, `claimable-balance-clawed-back` | `balance_id` |
| `pool-shares` | `account`, `pool` |
| `liquidity-pool` | `pool` |
| `offer` | `offer_id` |
| `contract-deployed` | `contract` |
| `contract-code` | `hash` |
| `contract-data` | `contract`, `key_xdr` |
| `ttl` | `key_hash` |
| `config-setting` | `setting` |
| `trade` | `counterparty`, `offer_id` |
| `contract-event` | `contract`, `stage` |
| `token-event` | `event`, `contract`, `from`, `to` |

Changes are compared as multisets: two equal transfers in one transaction must both be
exported.

## Carry check

When the adapter offers `dump`, the harness also checks what the target stored without any
mapping from its columns to the vocabulary. It collects every identifier and amount the
chain's ledgers carry, up to the last ledger the target ingested, from the ledgers' own
values: account, muxed, signed-payload, contract, pool and claimable balance strkeys, and
the integer amounts, balances, limits and fees. It adds what follows from those and the
passphrase alone: the genesis root account, the base `G` account of every `M` address and
`P` signer, and the Stellar Asset Contract id of every asset. Each value is known in these
encodings: the strkey text, the hex of the strkey's version byte and payload (the form of
wallet-backend's `AddressBytea`), the hex of the payload alone, and decimal for amounts.

Every non-null cell of the dump is then looked up:

| outcome | when |
|---|---|
| carried | the cell equals a known value in one of its encodings. Counted per column and encoding |
| truncated | the cell is a strict prefix of a known address's encoding (at least 17 bytes of hex or 20 strkey characters) and equals no known value: the target kept part of what it was given |
| fabricated | the cell decodes as a well-formed strkey (as text, or as version byte and payload in a byte column) that is not known; or the cell is in a column that holds carried addresses in other rows, starts with a strkey version byte and explains as nothing known: the target wrote an identifier the chain cannot account for |

Cells that match nothing and look like no address are not judged: they are the target's own
values (ids, timestamps, hashes of its own). A truncated or fabricated value is an error
whatever the column means, so no claim is needed for it. The carry check does not change any
verdict. The report keeps, per chain, the counts, the carried columns and the first 200
problems; `HARNESS.md` has a "Carry check" table. The carried columns are the evidence the
state comparison builds its column mapping from.

`shapekit harness carry --dump <logs>/<chain>.<network>.dump.json --chain-dir
corpus/chains/<chain>/<network> [--through <ledger>]` runs the same check on a kept dump and
exits non-zero when it finds a problem.

## Replay invariance

How ledgers reach the target, a restart in the middle, and which network passphrase the
chain was recorded under must not change what the target stores. `shapekit harness relate`
compares the dumps of two runs chain by chain:

```sh
shapekit harness run --adapter "$A" --chain <id> --network testnet --repeat 2 --out base
shapekit harness run --adapter "$A" --chain <id> --network testnet --delivery sep54-s3 --out s3
shapekit harness relate --base base/logs/round-1 --control base/logs/round-2 --other s3/logs
```

- Tables are compared as multisets of rows; row order does not matter.
- `--control` names a second run of the base configuration. Columns that differ between the
  base and the control (ingestion timestamps, for example) are noise: they are blanked on
  both sides and listed, never assumed.
- When the other run holds the chain's other network variant (`--network mainnet`), the
  harness walks the two chains' ledgers in step and translates every address and hash core
  wrote differently (transaction hashes, the genesis root, asset contract ids, in each
  encoding the carry check knows) before comparing. Numbers are not translated. Places where the two chains differ in shape (a network
  setting written as another arm) are listed and nothing under them is translated. A value
  the target derives from a translated one in another form stays a difference.
- `--restart` on `harness run` restarts the target's ingesting process once, after the
  ledger halfway between the bootstrap ledger and the chain's last, through the version 2
  `restart` command (capability `restart`): the adapter stops the process and starts it on
  the same store, as a deployment restart does. The chain's `restarted_after` says where.

Each chain is reported `same` or `DIFFERENT` with, per table, the rows found on one side
only and the columns whose values differ. The command exits non-zero when any chain
differs; `--out` writes the reports as JSON.

## Output

`shapekit harness run` writes into `--out` (default `harness-out/`):

- `harness-report.json`: run metadata (adapter, target and version, delivery mode, corpus
  manifest hash, harness peak resident memory per round), per-chain results (bootstrap
  ledger, last ingested ledger, target peak memory, stop reason) and one row per entry with
  `verdict`, `reason`, the problems found and any unmapped rows.
- `HARNESS.md`: the same as tables, one row per entry.
- `logs/`: adapter stderr (`<chain>.<network>.adapter.log`) and the target logs `stop`
  returned (`<chain>.<network>.target.log`) per chain, and the dump
  (`<chain>.<network>.dump.json`: `{"schema": 1, "tables": {<name>: {"columns", "rows"}}}`)
  when the adapter offers one; with `--repeat`, one `logs/round-<n>/` per round.

`--repeat N` runs the whole selection N times in one process. The run fails when a later
round's verdicts differ from the first round's, or when its peak resident memory exceeds the
first round's by more than `--memory-slack` (default 10% or 16 MiB, whichever is larger).
`--memory-budget` fails a round over a fixed bound. The harness reads its peak memory from
`/proc/self/status` (`VmHWM`) and resets it between rounds through `/proc/self/clear_refs`.

## Reproductions

`shapekit harness reproduce --table <toml>` runs pairs of target builds over named chains and
checks the verdicts of named entries:

```toml
[adapter]
# the command line; {commit} is replaced by each case's commit
command = ["go", "-C", "adapters", "run", "./walletbackend/cmd/wb-adapter", "--commit", "{commit}", "--cpus", "2"]

[[reproduction]]
id = "wallet-backend-729"
issue = "https://github.com/stellar/wallet-backend/pull/729"
chain = "cap40.signed-payload-signer.p28"
network = "testnet"

[[reproduction.case]]
label = "pre-fix"
commit = "2a0a84ea12523b6c92e278c0c84b443041240095"
expect = [
  { entry = "cap40.signed-payload-signer.p28-set-options-payload-1-meta", verdict = "mishandled", reason = "halted" },
]

[[reproduction.case]]
label = "fixed"
commit = "2c873353c1f0f3c9b145e25a7d009023ad104a1c"
expect = [
  { entry = "cap40.signed-payload-signer.p28-set-options-payload-1-meta", verdict = "pass" },
]
```

A case can also name the carry check's outcome on the chain:

```toml
carry = { truncated = ["state_changes.signer_account_id"], fabricated = [] }
```

`truncated` and `fabricated` are the exact sets of `table.column` where the check finds such
values; the case holds only when both sets match, which needs an adapter that offers `dump`.

This excerpt is from `adapters/walletbackend/reproductions.toml`. The adapter command runs
with the harness's environment, so `go` must be on `PATH` for that table, and the command
must contain `{commit}`. A case holds when every listed entry has the listed verdict (and
reason, when given), and its chain ran without a harness error. `shapekit harness
reproduce` exits non-zero when any case does not hold. It writes `reproductions.json` and
`REPRODUCTIONS.md` into `--out` (default `harness-reproduce/`), and each case's full report
into `<out>/<reproduction id>/<case label>/`.

## Writing an adapter

1. Read `describe` and `start` above, then the vocabulary below, and decide the claims: one
   item per vocabulary kind the target stores, with exactly the fields it keeps.
2. Map the target's rows to vocabulary changes. When the target stores something the
   vocabulary has no kind for, report the rows as `unmapped`.
3. Decode stored addresses with a strict strkey decoder and report the bytes in `raw`.
4. Apply the memory limit and join only `docker_network`.
5. Run `shapekit harness run --adapter "<your command>" --chain <id> --network testnet` on a
   small chain and read `HARNESS.md`.

`cargo test -p shapekit-harness` checks the requests the harness builds and example response
lines against the schema (`crates/shapekit-harness/tests/protocol_schema.rs`). To check your
adapter's lines the same way, validate them against `schema/adapter-protocol.schema.json`.

## The wallet-backend adapter

`adapters/walletbackend` (Go), command `adapters/walletbackend/cmd/wb-adapter`:

```sh
(cd adapters && go build -o bin/wb-adapter ./walletbackend/cmd/wb-adapter)
shapekit harness run --adapter "adapters/bin/wb-adapter --commit <full commit hash>"
```

`--commit` is required and takes a full 40-character lowercase commit hash. `--repo` names a
wallet-backend clone (default: `<user cache dir>/shapekit/wallet-backend`, cloned from
github.com/stellar/wallet-backend on first use); `--db-memory` caps the database container
(default 1 GiB); `--cpus` passes docker's `--cpus` to every container it starts (default: no
limit). The adapter offers the `rpc` (default) and `sep54-s3` delivery modes, `dump` (every
base table of the database's `public` schema, rows ordered by every column) and `restart`
(`docker restart` of the ingest container).

- **Build.** `git archive <commit> | docker buildx build --load -` with the commit's own
  `Dockerfile`, image `shapekit-wallet-backend:<commit>`. Each `FROM` in that Dockerfile is
  replaced by its base image pinned by digest (`--build-context <tag>=docker-image://<image>@<digest>`,
  the table `BaseImages` in `docker.go`, the same digests as `pins.toml`); a Dockerfile naming a
  base with no pin is refused. The image carries a label with the set of pinned bases it was
  built on and is reused only when that label matches. Building needs network access; running
  does not.
- **Services.** `timescale/timescaledb:2.28.2-pg17` (by digest, as in `pins.toml`) with the
  settings of wallet-backend's `docker-compose.yaml`, the `timescaledb` extension created before
  `wallet-backend migrate up`, then `wallet-backend ingest` (live mode) with `--memory` and
  `--memory-swap` set to `limits.memory_bytes` and `--restart on-failure`. All containers join
  `docker_network` only. The restart policy stands in for the deployment's restart loop:
  wallet-backend gives up on a ledger after five persist attempts and exits, and when restarted it
  retries the same ledger, so a give-up is reported as a halt (`running`, cursor stuck,
  `last_error` set). `crashed` means a Go runtime failure in the log or a container that stayed
  down; `oom` means docker reports an OOM kill.
- **Configuration.** `NETWORK_PASSPHRASE`, `RPC_URL`, `ARCHIVE_URL`, `CHECKPOINT_FREQUENCY` from
  `start`; `DATABASE_URL` and `INGEST_SERVER_PORT` for the containers it runs;
  `LEDGER_BACKEND_TYPE` always set, to `rpc` or `datastore` (commits before a9bac8d2 default to
  the datastore); `STELLAR_ENVIRONMENT` for commits whose ingest command still requires it;
  `LOG_LEVEL=INFO`.
  `sep54-s3` adds `chrislusf/seaweedfs:4.46` run as in wallet-backend's integration tests
  (`weed server -s3`, no auth, bucket `ledgers`) and the `DATASTORE_*` variables. `start`
  uploads `.config.json` and every batch whose last ledger is at or below the bootstrap
  ledger from `lake.path`; each `advance` uploads the batches up to the new head.
- **Bootstrap.** `checkpoint`: an empty database makes live mode load the archive's latest
  checkpoint and start there.
- **Status.** `last_ingested_ledger` is `ingest_store.latest_ingest_ledger`. `last_error` is the
  newest log line at level error, fatal or panic, a retry warning whose message starts with
  `Error ` (`Error ingesting data ...`, `Error fetching ledger ...`), or the `Error:` / `panic:`
  line printed on exit. Peak memory is the ingest container's cgroup `memory.peak`, sampled every
  second.
- **Protocols.** From 1 up to 28 for commits on go-stellar-sdk v0.7.3, and up to 30 on v0.8.0.
  A commit on another go-stellar-sdk version is refused with an error that names the version
  (`sdkProtocols` in `source.go`).

Read-back takes each transaction's hash and ledger from `transactions`, then reads its
participants from `transactions_accounts` and its rows from `state_changes`, both by the
transaction's `to_id`. It selects every column of `state_changes`, so commits with other
columns read as they are. Token ids (asset contract ids) on trustline rows are named by recomputing the asset
contract id of every asset in `trustline_assets` and the SAC rows of `contract_tokens`. Address
bytes (`AddressBytea`, `SignerKeyBytea`) are decoded as stored, version byte then payload, with
go-stellar-sdk's strict strkey decoder; account columns accept `G` and `C`, signer columns `G`,
`T`, `X` and `P`. Transaction ids are TOIDs: the operation index is the low 12 bits minus one,
`operation_id` 0 is the transaction's fee row.

| `state_change_category` / `reason` | vocabulary change | fields exported |
|---|---|---|
| BALANCE DEBIT/CREDIT, `operation_id` 0 | `token-event` `fee` | `contract` (token_id), `from` (account_id), `amount` (net: DEBIT as stored, CREDIT negated) |
| BALANCE DEBIT + CREDIT, consecutive `state_change_id`, same token and amount | `token-event` `transfer` | `contract`, `from`, `to`, `amount`, `to_muxed_id` `{"type":"u64"}` from the CREDIT row |
| BALANCE MINT (+ CREDIT at the next `state_change_id`) | `token-event` `mint` | `contract`, `to` (the CREDIT row, else the MINT row's account), `amount`, `to_muxed_id` |
| BALANCE BURN (+ DEBIT at the next `state_change_id`) | `token-event` `burn` | `contract`, `from` (the DEBIT row, else the BURN row's account), `amount` |
| BALANCE DEBIT or CREDIT without a partner | `token-event` `transfer` | the side wallet-backend stored only (`from` or `to`) |
| ACCOUNT CREATE (G) | `account-created` | `account` |
| ACCOUNT CREATE (C) | `contract-deployed` | `contract` |
| ACCOUNT MERGE | `account-removed` | `account` |
| SIGNER ADD / REMOVE / UPDATE | `signer-added` / `signer-removed` / `signer-weight` | `account`, `signer` (signer_account_id), `weight` or `before`, `after` |
| SIGNATURE_THRESHOLD UPDATE (one row per threshold) | `thresholds`, one per account and operation | `account` |
| FLAGS SET / CLEAR | `account-flags`, one per account and operation | `account` |
| HOME_DOMAIN SET / UPDATE / CLEAR | `home-domain` | `account`, `before`, `after` from `key_value` old/new (absent as `""`) |
| DATA_ENTRY ADD / UPDATE / REMOVE | `data-entry` | `account`, `action`, `name_hex`, `value_before_hex`, `value_after_hex` (base64 `key_value` as hex) |
| TRUSTLINE ADD | `trustline-created` | `account`, `asset`, `limit` (stored in horizon's 7-decimal amount form, converted to stroops); `flags` from the BALANCE_AUTHORIZATION SET row written with it |
| (asset of a pool-share trustline) | | `pool-share:` plus the pool's `L` strkey, from the hex `liquidity_pool_id`, which is how the oracle names it |
| TRUSTLINE UPDATE / REMOVE | `trustline-limit` / `trustline-removed` | `account`, `asset`, `before`, `after` |
| BALANCE_AUTHORIZATION SET / CLEAR, G holder | `trustline-flags`, one per holder, asset and operation | `account`, `asset` |
| BALANCE_AUTHORIZATION SET / CLEAR, C holder | `token-event` `set_authorized` (not claimed) | `contract`, `to`, `authorized` |
| BALANCE_AUTHORIZATION, holder byte neither `G` nor `C` | `unmapped` | the whole row; the version byte is compared with go-stellar-sdk's `strkey.VersionByteAccountID` and `VersionByteContract` |
| ALLOWANCE UPDATE | `token-event` `approve` | `contract`, `from`, `spender`, `amount`, `live_until_ledger` |
| BLEND_*, any other category, and rows the rules above cannot name (a token id of no known asset) | `unmapped` | the whole row, with the reason |

wallet-backend's trustline flag bits (bit 0 authorized, bit 5 clawback enabled, bit 6 authorized
to maintain liabilities) become core's `TrustLineFlags` values (1, 4, 2). Two BALANCE rows pair
only when the second has the next `state_change_id` after the first, the reason the pair needs,
and the same token and amount. wallet-backend writes the two rows of one token event next to
each other (`createDebitCreditPair`, `handleMint`, `handleDefaultBurnOrClawback` in
`internal/indexer/processors/token_transfer.go`) and numbers each stream's rows 1..N per
operation in that order (`AssignStateChangeOrdinals`), so this pairs rows of one event and never
joins rows of two events that happen to agree on token and amount, such as a one-sided
liquidity-pool leg of a path payment followed by a transfer. State change and participant rows
whose transaction has no `transactions` row come out as `unattributed` records.

Claims follow from the table: participants of kind `account` only (wallet-backend does not record
event-emitting contracts as participants, and keeps no mux ids), every kind above on the
fields it stores, `token-event` `transfer`/`mint`/`burn`/`clawback` on `event`, `contract`,
`from`, `to`, `amount`, `to_muxed_id`, the `fee` event of stage `before-all-txs` on `event`,
`contract`, `from` (the stored fee row is net of any refund, so its amount and the refund event
are not claimed), and `approve`. Values wallet-backend does not keep (starting balances, threshold
values, account flag values, trustline flag values after authorization changes, executables)
are not claimed.

Two claims cover facts that wallet-backend at the current pin does not store in the oracle's
form, and they are kept on purpose:

- **`clawback`.** wallet-backend's `TokenTransferProcessor` receives the token transfer
  processor's `Clawback` event (`handleClawback`) and writes it as a BURN row for the issuer
  plus a DEBIT row for the holder, the same rows a payment to the issuer produces. The oracle,
  following CAP-67, records a `clawback` event. The information reaches wallet-backend and its
  `state_change_reason` column is an open vocabulary, so the loss happens in wallet-backend's
  mapping, not in what the ledger or its schema can carry. A wallet that shows this history
  would show a clawback as a voluntary payment. Each clawback therefore counts as one dropped
  `clawback` and one extra `burn`, which is finding WB-W3.
- **`to_muxed_id` on every arm.** CAP-67 puts the destination's mux id or, without one, the
  transaction memo into a transfer's `to_muxed_id`, typed as u64, string or bytes.
  wallet-backend's `setToMuxedID` keeps only the u64 arm, although its `to_muxed_id` column is
  `TEXT`. A text, hash or return memo is lost, which is finding WB-W4.

The rule for both: a claim says what wallet-backend is expected to record of a fact it ingests
and has a place for, judged against the oracle. Claiming only what it stores today would turn
these losses into `out-of-scope` and hide them. The verdict detail and the finding's text
name the cause, so a reader can see that the difference is a design choice in wallet-backend
and decide whether it matters for their use.

## Oracle vocabulary

What the oracle says each transaction changed, read only from `LedgerCloseMeta`, in
`<variant>.oracle.json` next to each eligible corpus entry. Conventions for every kind:

- Addresses are canonical strkeys: `G` accounts, `C` contracts, `L` liquidity pools, `B`
  claimable balances. A mux id never replaces the base address.
- A participant's `mux_ids` come only from addresses that are muxed in the meta itself
  (`SC_ADDRESS_TYPE_MUXED_ACCOUNT` or muxed contract addresses in event topics). A token
  event's `to_muxed_id` is kept on the event as typed and never becomes a participant mux id:
  core writes the transaction memo there too, so a u64 there can be a memo id.
- Signers keep their own strkey form: `G`, `T` (pre-auth), `X` (hash-x), `P` (signed payload,
  with its payload).
- Amounts, balances, limits, ids and sequence numbers are decimal strings; flags, weights,
  thresholds and ledger numbers are JSON integers.
- Assets are `native` or `CODE:ISSUER`; a trustline asset can also be `pool-share:<L strkey>`,
  the pool id in its CAP-38 `L` strkey form. Pools are named by the same `L` strkey everywhere.
- `action` is `created`, `updated`, `removed` or `restored`.

The oracle file of an entry is `{"oracle_vocab_version", "network_passphrase", "ledgers":
[{"ledger_seq", "ledger_hash", "protocol", "close_time", "transactions", "upgrades",
"evicted"}]}`; each transaction has `hash`, `apply_index`, `successful`, `result`,
`inner_hash`, `fee {source, charged, balance_delta}`, `participants` (actual, from meta and
events), `changes`, `declared` (what the envelope names, kept apart and never used as the
expected participants) and `events` (which events the meta format can carry, see "Facts the
meta cannot show").

<!-- vocabulary: generated by `shapekit harness schema --write`, do not edit -->

Oracle vocabulary version **2** (`ORACLE_VOCAB_VERSION` in `crates/shapekit-oracle/src/vocab.rs`).
Every change also has `scope` (`fee-processing`, `tx-before`, `operation`, `tx-after`, `post-apply-fee`, `upgrade`) and, for `operation` scope, `op_index` (integer, from 0).

### `account-created`

| field | type | always present | meaning |
|---|---|---|---|
| `account` | string | yes |  |
| `starting_balance` | decimal string | yes |  |

### `account-removed`

| field | type | always present | meaning |
|---|---|---|---|
| `account` | string | yes |  |
| `final_balance` | decimal string | yes |  |

### `balance-change`

A change in what an address holds of an asset: account native balance, trustline balance, claimable balance amount, pool reserve, or a Stellar Asset Contract balance entry. `holder` is the address whose holding changed.

| field | type | always present | meaning |
|---|---|---|---|
| `after` | decimal string | yes |  |
| `asset` | string | yes |  |
| `before` | decimal string | yes |  |
| `delta` | decimal string | yes |  |
| `held_in` | string | yes | `account`, `trustline`, `claimable-balance`, `liquidity-pool` or `contract`. |
| `holder` | string | yes |  |

### `trustline-created`

| field | type | always present | meaning |
|---|---|---|---|
| `account` | string | yes |  |
| `asset` | string | yes | `native`, `CODE:ISSUER`, or `pool-share:` followed by the pool id as an `L` strkey (CAP-38 form, not hex) for a pool-share trustline. |
| `flags` | integer (uint32) | yes |  |
| `limit` | decimal string | yes |  |

### `trustline-removed`

| field | type | always present | meaning |
|---|---|---|---|
| `account` | string | yes |  |
| `asset` | string | yes | `native`, `CODE:ISSUER`, or `pool-share:` followed by the pool id as an `L` strkey (CAP-38 form, not hex) for a pool-share trustline. |

### `trustline-flags`

| field | type | always present | meaning |
|---|---|---|---|
| `account` | string | yes |  |
| `after` | integer (uint32) | yes |  |
| `asset` | string | yes | `native`, `CODE:ISSUER`, or `pool-share:` followed by the pool id as an `L` strkey (CAP-38 form, not hex) for a pool-share trustline. |
| `before` | integer (uint32) | yes |  |

### `trustline-limit`

| field | type | always present | meaning |
|---|---|---|---|
| `account` | string | yes |  |
| `after` | decimal string | yes |  |
| `asset` | string | yes | `native`, `CODE:ISSUER`, or `pool-share:` followed by the pool id as an `L` strkey (CAP-38 form, not hex) for a pool-share trustline. |
| `before` | decimal string | yes |  |

### `signer-added`

| field | type | always present | meaning |
|---|---|---|---|
| `account` | string | yes |  |
| `signer` | string | yes |  |
| `weight` | integer (uint32) | yes |  |

### `signer-removed`

| field | type | always present | meaning |
|---|---|---|---|
| `account` | string | yes |  |
| `signer` | string | yes |  |
| `weight` | integer (uint32) | yes |  |

### `signer-weight`

Includes the master key, whose weight is threshold 0 of the account.

| field | type | always present | meaning |
|---|---|---|---|
| `account` | string | yes |  |
| `after` | integer (uint32) | yes |  |
| `before` | integer (uint32) | yes |  |
| `signer` | string | yes |  |

### `thresholds`

| field | type | always present | meaning |
|---|---|---|---|
| `account` | string | yes |  |
| `after` | Thresholds | yes |  |
| `before` | Thresholds | yes |  |

### `account-flags`

| field | type | always present | meaning |
|---|---|---|---|
| `account` | string | yes |  |
| `after` | integer (uint32) | yes |  |
| `before` | integer (uint32) | yes |  |

### `home-domain`

| field | type | always present | meaning |
|---|---|---|---|
| `account` | string | yes |  |
| `after` | string | yes |  |
| `before` | string | yes |  |

### `inflation-destination`

| field | type | always present | meaning |
|---|---|---|---|
| `account` | string | yes |  |
| `after` | string | no |  |
| `before` | string | no |  |

### `sequence`

| field | type | always present | meaning |
|---|---|---|---|
| `account` | string | yes |  |
| `after` | decimal string | yes |  |
| `before` | decimal string | yes |  |

### `data-entry`

Data entry name and values as hex, since neither has to be UTF-8.

| field | type | always present | meaning |
|---|---|---|---|
| `account` | string | yes |  |
| `action` | `"created"` / `"updated"` / `"removed"` / `"restored"` | yes |  |
| `name_hex` | string | yes |  |
| `value_after_hex` | string | no |  |
| `value_before_hex` | string | no |  |

### `sponsorship`

The sponsor of a ledger entry or of one signer changed (created, revoked, transferred, or the sponsored thing went away). `target` names the entry or signer.

| field | type | always present | meaning |
|---|---|---|---|
| `signer` | string | no |  |
| `sponsor_after` | string | no |  |
| `sponsor_before` | string | no |  |
| `target` | string | yes |  |

### `claimable-balance-created`

| field | type | always present | meaning |
|---|---|---|---|
| `amount` | decimal string | yes |  |
| `asset` | string | yes |  |
| `balance_id` | string | yes |  |
| `claimants` | array of string | yes |  |

### `claimable-balance-claimed`

Removed by a claim. The operation type tells a claim from a clawback, since the entry change itself is the same.

| field | type | always present | meaning |
|---|---|---|---|
| `amount` | decimal string | yes |  |
| `asset` | string | yes |  |
| `balance_id` | string | yes |  |

### `claimable-balance-clawed-back`

| field | type | always present | meaning |
|---|---|---|---|
| `amount` | decimal string | yes |  |
| `asset` | string | yes |  |
| `balance_id` | string | yes |  |

### `pool-shares`

A pool-share trustline balance.

| field | type | always present | meaning |
|---|---|---|---|
| `account` | string | yes |  |
| `after` | decimal string | yes |  |
| `before` | decimal string | yes |  |
| `delta` | decimal string | yes |  |
| `pool` | string | yes | The pool id as an `L` strkey. |

### `liquidity-pool`

| field | type | always present | meaning |
|---|---|---|---|
| `action` | `"created"` / `"updated"` / `"removed"` / `"restored"` | yes |  |
| `asset_a` | string | yes |  |
| `asset_b` | string | yes |  |
| `pool` | string | yes | The pool id as an `L` strkey. |
| `reserve_a_after` | decimal string | yes |  |
| `reserve_a_before` | decimal string | yes |  |
| `reserve_b_after` | decimal string | yes |  |
| `reserve_b_before` | decimal string | yes |  |
| `shares_after` | decimal string | yes |  |
| `shares_before` | decimal string | yes |  |

### `offer`

| field | type | always present | meaning |
|---|---|---|---|
| `action` | `"created"` / `"updated"` / `"removed"` / `"restored"` | yes |  |
| `amount_after` | decimal string | no |  |
| `amount_before` | decimal string | no |  |
| `buying` | string | yes |  |
| `offer_id` | decimal string | yes |  |
| `price_d` | integer (int32) | yes |  |
| `price_n` | integer (int32) | yes |  |
| `seller` | string | yes |  |
| `selling` | string | yes |  |

### `contract-deployed`

A contract instance entry appeared. Read from meta, never from auth entries.

| field | type | always present | meaning |
|---|---|---|---|
| `contract` | string | yes |  |
| `executable` | string | yes | `wasm:<hex hash>`, `stellar-asset`, or `external-ref:<owner>:<hex tag>` (CAP-85). |

### `contract-code`

| field | type | always present | meaning |
|---|---|---|---|
| `action` | `"created"` / `"updated"` / `"removed"` / `"restored"` | yes |  |
| `hash` | string | yes |  |

### `contract-data`

| field | type | always present | meaning |
|---|---|---|---|
| `action` | `"created"` / `"updated"` / `"removed"` / `"restored"` | yes |  |
| `contract` | string | yes |  |
| `durability` | string | yes |  |
| `key_xdr` | string | yes | The key `ScVal` as base64 XDR. |

### `ttl`

| field | type | always present | meaning |
|---|---|---|---|
| `action` | `"created"` / `"updated"` / `"removed"` / `"restored"` | yes |  |
| `after` | integer | no |  |
| `before` | integer | no |  |
| `key_hash` | string | yes |  |
| `target` | string | no | The entry the TTL belongs to, when its key is visible in this ledger. |

### `config-setting`

| field | type | always present | meaning |
|---|---|---|---|
| `action` | `"created"` / `"updated"` / `"removed"` / `"restored"` | yes |  |
| `setting` | string | yes |  |

### `trade`

One offer or pool crossed by a manage-offer or path-payment operation, from the operation result's claim atoms. `counterparty` is the offer's seller or the pool.

| field | type | always present | meaning |
|---|---|---|---|
| `bought_amount` | decimal string | yes |  |
| `bought_asset` | string | yes | What the counterparty received. |
| `counterparty` | string | yes |  |
| `offer_id` | decimal string | no |  |
| `sold_amount` | decimal string | yes |  |
| `sold_asset` | string | yes | What the counterparty gave up. |

### `contract-event`

A consensus contract event that is not a token event (for example a protocol's own events, such as Blend's supply or borrow), with topics and data as base64 XDR.

| field | type | always present | meaning |
|---|---|---|---|
| `contract` | string | no |  |
| `data_xdr` | string | yes |  |
| `stage` | string | yes | `before-all-txs`, `operation`, `after-tx` or `after-all-txs`. |
| `topics_xdr` | array of string | yes |  |

### `token-event`

A token event (CAP-67 and SEP-41 shapes): transfer, mint, burn, clawback, set_authorized, fee, approve.

| field | type | always present | meaning |
|---|---|---|---|
| `amount` | decimal string | no |  |
| `asset` | string | no | Set when the contract is the Stellar Asset Contract of this asset, which the oracle proves by recomputing the contract id. |
| `asset_topic` | string | no | The SEP-11 asset string a SAC event carries as its last topic, verbatim. |
| `authorized` | boolean | no |  |
| `contract` | string | yes |  |
| `event` | string | yes |  |
| `from` | string | no |  |
| `live_until_ledger` | integer | no |  |
| `spender` | string | no |  |
| `stage` | string | yes | `before-all-txs`, `operation`, `after-tx` or `after-all-txs`. |
| `to` | string | no |  |
| `to_muxed_id` | ToMuxedId | no | The data map's `to_muxed_id` as the event typed it: `{"type": "u64", "value": "<decimal>"}`, `{"type": "string", "value": "<hex>"}` or `{"type": "bytes", "value": "<hex>"}`. Core writes the destination's mux id here when the destination is muxed and otherwise the transaction memo (id as u64, text as string, hash and return as bytes), so a u64 may be either a mux id or a memo id. It is not a participant mux id. |

<!-- end of generated vocabulary -->
