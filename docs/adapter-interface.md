# Adapter interface

The replay harness (`shapekit harness run`) feeds recorded chains to an indexer and checks
what the indexer stored against the meta oracle. It never talks to the indexer directly. An
**adapter** sits in between: a program that starts the indexer, points it at the harness's
ledger delivery endpoints, reports how far it got, and reads back what it stored in the
oracle's vocabulary.

Protocol version: **1**. The machine-readable form is `schema/adapter-protocol.schema.json`,
generated from `crates/shapekit-harness/src/protocol.rs` together with the vocabulary section
at the end of this page (`shapekit harness schema --write`; `cargo test -p shapekit-harness`
fails when either is stale). Anything this page and the schema disagree on is a bug in one of
them.

The reference adapter is `adapters/walletbackend` (wallet-backend, Go).

## What the harness does with an adapter

For each chain (one recorded ledger sequence per shape family, protocol and passphrase):

1. Creates a Docker network with `--internal` and starts the replay server
   (`shapekit-serve`, in process) bound to that network's gateway address on a free port.
   The visible head is set to the bootstrap ledger (see "Bootstrap").
2. Starts the adapter program, sends `describe`, then `start`.
3. Waits until `status` reports `last_ingested_ledger` at or past the bootstrap ledger.
4. For every later ledger of the chain in order: moves the server's head to it, sends
   `advance`, and polls `status` until the target has ingested it, the target died, or it
   stopped making progress.
5. Sends `export` for every ledger the target ingested and compares the records with the
   oracle files of the chain's corpus entries.
6. Sends `stop`, keeps the returned logs, removes the network and the work directory.

A new adapter process is started for every chain, so a target that halts in one family does
not hide the others.

## Transport

- The harness starts the adapter as a child process with the command line it was given and
  talks to it over stdin and stdout. The working directory is the harness's.
- Every message is one JSON object encoded as UTF-8 on one line, terminated by `\n`. No
  pretty printing, no blank lines, no byte order mark.
- The harness sends one request and reads response lines until the final line for that
  request (`status` `ok` or `error`) before it sends the next. Every response line carries the
  request's `id`. A line with another `id` is a protocol violation and ends the chain's run.
- Only `export` has more than one response line: any number of `record` lines, then the
  final line.
- stdout carries responses only. Logs go to stderr; the harness saves the adapter's stderr
  next to the report (`logs/<chain>.<network>.adapter.log`).
- Members the receiver does not know must be ignored, so later protocol versions can add
  optional members.
- When stdin reaches end of file the adapter tears the target down as for `stop` and exits.
- The harness enforces a timeout per request (defaults: `describe` 60 s, `start` 30 min,
  since it may build images, `advance` 120 s, `status` 60 s, `export` 10 min, `stop` 5 min).
  On a timeout, a protocol violation or an adapter exit, the harness kills the adapter
  process and records the chain's remaining entries as `not-evaluated` with the reason. The
  adapter must therefore not leave containers behind when it is killed: label them with the
  chain's `docker_network` name, which the harness removes along with every container still
  attached to it.

### Request

```json
{"id": "3", "cmd": "advance", "head": 65}
```

- `id`: string, unique within one adapter process.
- `cmd`: `describe`, `start`, `advance`, `status`, `export` or `stop`. The command's members
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

Request: `{"id": "0", "cmd": "describe", "protocol": 1}`

Result:

| member | type | meaning |
|---|---|---|
| `protocol` | integer | must be `1`; the harness stops otherwise |
| `adapter`, `adapter_version` | string | the adapter program and its version |
| `target` | string | the indexer, for example `wallet-backend` |
| `target_version` | string | the exact build, for example a full commit hash, never a branch name |
| `delivery_modes` | array | modes the adapter can drive: `sep54-filesystem`, `sep54-s3`, `rpc` |
| `default_delivery` | string | mode used unless the harness is given `--delivery`; one of `delivery_modes` |
| `bootstrap` | `checkpoint` or `ledger` | how the target starts, see "Bootstrap" |
| `vocab_version` | integer | the oracle vocabulary version the claims and exports use; must equal the harness's |
| `protocols` | `{"min", "max"}` | ledger protocols the target build ingests; chains outside are `out-of-scope` |
| `claims` | object | what the target records, see "Claims" |

### start

Request members (besides `id` and `cmd`):

| member | type | meaning |
|---|---|---|
| `chain` | string | chain id, for example `cap40.signed-payload-signer.p28` |
| `network` | string | `testnet` or `mainnet`, the passphrase variant replayed |
| `network_passphrase` | string | the passphrase to configure the target with |
| `protocol` | integer | ledger protocol of the chain |
| `delivery.mode` | string | the mode the harness picked |
| `delivery.rpc_url` | string | JSON-RPC endpoint of the replay server; present in every mode |
| `delivery.archive_url` | string | history archive root; the archive as it stood at the newest checkpoint at or below the head |
| `delivery.lake.url` | string | SEP-54 lake root over HTTP: `.config.json` and the batches whose ledgers are all visible |
| `delivery.lake.path` | string | the same lake as a directory on the harness host (every batch, including unreleased ones) |
| `delivery.lake.ledgers_per_batch`, `batches_per_partition` | integer | the lake's layout |
| `ledgers.bootstrap` | integer | the visible head while `start` runs |
| `ledgers.last` | integer | the chain's last ledger |
| `ledgers.checkpoint_frequency` | integer | `64` |
| `limits.memory_bytes` | integer | memory limit to apply to the target's ingesting process or containers |
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
`CODE:ISSUER` (SEP-11), pool-share trustline assets as `pool-share:<hex pool id>`, hashes and
XDR as lowercase hex or base64 as the field's description says, signers as full strkeys
including `P...` signed-payload signers and their payload.

`unmapped` rows: `{"table", "category", "reason", "row", "note"}`. `row` holds every column
(byte columns as lowercase hex); `note` says why there is no mapping. Rows are never dropped
silently: an adapter that cannot map a row reports it here, and the report lists it next to
the entry so a reviewer can tell an indexer drop from an adapter gap.

### stop

Result: `logs`, the tail of the target's logs (at most 1 MiB). The adapter removes every
container, volume and file it created, then exits with status 0.

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
  wallet-backend live mode on an empty database does this at every commit pinned by this
  project (`internal/services/ingest_live.go`: `archive.GetLatestLedgerSequence()`, then
  `PopulateFromCheckpoint`, then ingestion from that ledger). It refuses an archive whose
  latest ledger is 0. The harness therefore picks the newest checkpoint before the chain's
  first entry ledger as `ledgers.bootstrap` and serves the archive as of that checkpoint.
- `ledger`: the target can be told a start ledger. `ledgers.bootstrap` is the chain's first
  ledger.

Recipe chains are recorded so that the first checkpoint exists before anything else
happens: after the protocol upgrade in ledger 2, `shapekit gen chains` closes empty ledgers
through ledger 63 (core's first checkpoint at the default frequency of 64) and waits for it to
be published, then applies network settings and runs the recipe from ledger 64. The archive in
each chain's `archive.tar` holds that checkpoint's real buckets. A chain recorded before this
change has no checkpoint; for `checkpoint` targets its entries are reported `not-evaluated`
with that reason until the chain is regenerated.

## Network access

Containers reach the replay server through the internal network's gateway: Docker assigns
the gateway address of a user-defined bridge network to the host side of the bridge, and a
container on an `--internal` network can reach that address but has no default route, so
nothing beyond the host is reachable. The harness binds the replay server to exactly that
address, so the server is not reachable from the LAN either. `host.docker.internal` with
`host-gateway` is not used: it resolves to the default bridge's address (`172.17.0.1`),
which a container on an internal network cannot route to.

With `--no-docker` the harness creates no network and binds the server to `127.0.0.1`, for
adapters whose target runs as a host process.

## Verdicts

The harness gives every corpus entry of the chain that has an oracle file one verdict.
Entries without an oracle file (ledger entries, synthetic entries, vectors) are not replayed
and are counted separately.

| verdict | reason | when |
|---|---|---|
| `pass` | | every in-scope fact of the entry's transactions was exported with equal values, and nothing undecodable or extra was stored |
| `dropped` | | an in-scope participant or change has no counterpart in the export (or the target stored nothing for the transaction) |
| `mishandled` | `wrong-value` | an in-scope fact was stored with a different value (paired by the kind's key fields), or the target stored a participant or change the meta does not show |
| `mishandled` | `undecodable-address` | a stored address in the entry's transactions does not decode (`raw.error`) |
| `mishandled` | `halted` | the target stopped advancing at the entry's ledger with `last_error` set |
| `mishandled` | `crashed` | the ingest process died at the entry's ledger (`health: crashed`) |
| `mishandled` | `oom` | it was killed for its memory limit at the entry's ledger (`health: oom`) |
| `mishandled` | `timeout` | it stopped advancing at the entry's ledger without reporting an error |
| `not-evaluated` | | the target stopped at an earlier ledger (the result names the entry), or the chain could not be run (the result says why) |
| `out-of-scope` | | the entry has no fact the target claims, or the chain's protocol is outside `protocols` |

When an entry has several problems the verdict is the first in the order halted, crashed,
oom, timeout, undecodable-address, wrong-value, dropped, and every problem is listed in the
report. A target stops "at" ledger `L` when `L` is the first ledger it did not ingest; every
entry in `L` gets the stop verdict, entries in later ledgers are `not-evaluated`.

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

## Output

`shapekit harness run` writes into `--out` (default `harness-out/`):

- `harness-report.json`: run metadata (adapter, target and version, delivery mode, corpus
  manifest hash, harness peak resident memory per round), per-chain results (bootstrap
  ledger, last ingested ledger, target peak memory, stop reason) and one row per entry with
  `verdict`, `reason`, the problems found and any unmapped rows.
- `HARNESS.md`: the same as tables, one row per entry.
- `logs/`: adapter stderr and target logs per chain.

`--repeat N` runs the whole selection N times in one process and fails when a later round's
peak resident memory exceeds the first round's by more than `--memory-slack` (default 10% or
16 MiB, whichever is larger); `--memory-budget` fails a round over a fixed bound. Peak memory
is read from `/proc/self/status` (`VmHWM`), reset between rounds through
`/proc/self/clear_refs`.

## Reproductions

`shapekit harness reproduce --table <toml>` runs pairs of target builds over named chains and
checks the verdicts of named entries:

```toml
[adapter]
# the command line; {commit} is replaced by each case's commit
command = ["adapters/bin/walletbackend-adapter", "--commit", "{commit}"]

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

A case holds when every listed entry has the listed verdict (and reason, when given). The
command exits non-zero when any case does not hold, and writes `reproductions.json` and
`REPRODUCTIONS.md` next to each case's full report.

## Writing an adapter

1. Read `describe` and `start` above, then the vocabulary below, and decide the claims: one
   item per vocabulary kind the target stores, with exactly the fields it keeps.
2. Map the target's rows to vocabulary changes. When the target stores something the
   vocabulary has no kind for, report the rows as `unmapped`.
3. Decode stored addresses with a strict strkey decoder and report the bytes in `raw`.
4. Apply the memory limit and join only `docker_network`.
5. Run `shapekit harness run --adapter "<your command>" --chain <id> --network testnet` on a
   small chain and read `HARNESS.md`.

The schema check `cargo test -p shapekit-harness` validates recorded adapter traffic against
the schema; add a recording of your adapter's answers there when you add an adapter.

## The wallet-backend adapter

`adapters/walletbackend` (Go), binary `adapters/walletbackend/cmd/wb-adapter`:

```sh
(cd adapters && go build -o bin/wb-adapter ./walletbackend/cmd/wb-adapter)
shapekit harness run --adapter "adapters/bin/wb-adapter --commit <full commit hash>"
```

`--repo` names a wallet-backend clone (default: `<user cache dir>/shapekit/wallet-backend`,
cloned from github.com/stellar/wallet-backend on first use); `--db-memory` caps the database
container (default 1 GiB).

- **Build.** `git archive <commit> | docker build -` with the commit's own `Dockerfile`, image
  `shapekit-wallet-backend:<commit>`, reused when it exists. Building needs network access;
  running does not.
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
  `start`; `LEDGER_BACKEND_TYPE` always set (commits before a9bac8d2 default to the datastore);
  `STELLAR_ENVIRONMENT` for commits whose ingest command still requires it; `LOG_LEVEL=INFO`.
  `sep54-s3` adds `chrislusf/seaweedfs:4.46` run as in wallet-backend's integration tests
  (`weed server -s3`, no auth, bucket `ledgers`) and the `DATASTORE_*` variables; `advance`
  uploads `.config.json` and every batch at or below the head from `lake.path`.
- **Bootstrap.** `checkpoint`: an empty database makes live mode load the archive's latest
  checkpoint and start there.
- **Status.** `last_ingested_ledger` is `ingest_store.latest_ingest_ledger`. `last_error` is the
  newest log line at level error, fatal or panic, a retry warning whose message starts with
  `Error ` (`Error ingesting data ...`, `Error fetching ledger ...`), or the `Error:` / `panic:`
  line printed on exit. Peak memory is the ingest container's cgroup `memory.peak`, sampled every
  second.
- **Protocols.** Up to 28 for commits on go-stellar-sdk v0.7.3, up to 30 on v0.8.0; another sdk
  version is an error until it is added.

Read-back joins `transactions` (hash, ledger), `transactions_accounts` (participants) and
`state_changes` on `to_id`, selecting every column so commits with other columns read as they
are. Token ids (asset contract ids) on trustline rows are named by recomputing the asset
contract id of every asset in `trustline_assets` and the SAC rows of `contract_tokens`. Address
bytes (`AddressBytea`, `SignerKeyBytea`) are decoded as stored, version byte then payload, with
go-stellar-sdk's strict strkey decoder; account columns accept `G` and `C`, signer columns `G`,
`T`, `X` and `P`. Transaction ids are TOIDs: the operation index is the low 12 bits minus one,
`operation_id` 0 is the transaction's fee row.

| `state_change_category` / `reason` | vocabulary change | fields exported |
|---|---|---|
| BALANCE DEBIT/CREDIT, `operation_id` 0 | `token-event` `fee` | `contract` (token_id), `from` (account_id), `amount` (net: DEBIT as stored, CREDIT negated) |
| BALANCE DEBIT + CREDIT, same token and amount | `token-event` `transfer` | `contract`, `from`, `to`, `amount`, `to_muxed_id` `{"type":"u64"}` from the CREDIT row |
| BALANCE MINT (+ CREDIT) | `token-event` `mint` | `contract`, `to` (the CREDIT row, else the MINT row's account), `amount`, `to_muxed_id` |
| BALANCE BURN (+ DEBIT) | `token-event` `burn` | `contract`, `from` (the DEBIT row, else the BURN row's account), `amount` |
| BALANCE DEBIT or CREDIT without a partner | `token-event` `transfer` | the side wallet-backend stored only (`from` or `to`) |
| ACCOUNT CREATE (G) | `account-created` | `account` |
| ACCOUNT CREATE (C) | `contract-deployed` | `contract` |
| ACCOUNT MERGE | `account-removed` | `account` |
| SIGNER ADD / REMOVE / UPDATE | `signer-added` / `signer-removed` / `signer-weight` | `account`, `signer` (signer_account_id), `weight` or `before`, `after` |
| SIGNATURE_THRESHOLD UPDATE (one row per threshold) | `thresholds`, one per account and operation | `account` |
| FLAGS SET / CLEAR | `account-flags`, one per account and operation | `account` |
| HOME_DOMAIN SET / UPDATE / CLEAR | `home-domain` | `account`, `before`, `after` from `key_value` old/new (absent as `""`) |
| DATA_ENTRY ADD / UPDATE / REMOVE | `data-entry` | `account`, `action`, `name_hex`, `value_before_hex`, `value_after_hex` (base64 `key_value` as hex) |
| TRUSTLINE ADD | `trustline-created` | `account`, `asset`, `limit`; `flags` from the BALANCE_AUTHORIZATION SET row written with it |
| TRUSTLINE UPDATE / REMOVE | `trustline-limit` / `trustline-removed` | `account`, `asset`, `before`, `after` |
| BALANCE_AUTHORIZATION SET / CLEAR, G holder | `trustline-flags`, one per holder, asset and operation | `account`, `asset` |
| BALANCE_AUTHORIZATION SET / CLEAR, C holder | `token-event` `set_authorized` (not claimed) | `contract`, `to`, `authorized` |
| ALLOWANCE UPDATE | `token-event` `approve` | `contract`, `from`, `spender`, `amount`, `live_until_ledger` |
| BLEND_*, any other category, and rows the rules above cannot name (a token id of no known asset) | `unmapped` | the whole row, with the reason |

wallet-backend's trustline flag bits (bit 0 authorized, bit 5 clawback enabled, bit 6 authorized
to maintain liabilities) become core's `TrustLineFlags` values (1, 4, 2). Debit and credit rows
pair in `state_change_id` order on equal token and amount. State change and participant rows
whose transaction has no `transactions` row come out as `unattributed` records.

Claims follow from the table: participants of kind `account` only (wallet-backend does not record
event-emitting contracts as participants, and keeps no mux ids), every kind above on the
fields it stores, `token-event` `transfer`/`mint`/`burn`/`clawback` on `event`, `contract`,
`from`, `to`, `amount`, `to_muxed_id`, `fee` on `event`, `contract`, `from` (the stored fee is net
of the refund), and `approve`. Values wallet-backend does not keep (starting balances, threshold
values, account flag values, trustline flag values after authorization changes, executables)
are not claimed.

## Oracle vocabulary

What the oracle says each transaction changed, read only from `LedgerCloseMeta`, in
`<variant>.oracle.json` next to each eligible corpus entry. Conventions for every kind:

- Addresses are canonical strkeys: `G` accounts, `C` contracts, `L` liquidity pools, `B`
  claimable balances. A mux id never replaces the base address.
- A participant's `mux_ids` come only from addresses that are muxed in the meta itself
  (`SC_ADDRESS_TYPE_MUXED_ACCOUNT` or muxed contract addresses in event topics). A token
  event's `to_muxed_id` is kept on the event as typed and never becomes a participant mux id:
  core writes the transaction memo there too, so a u64 may be a memo id (since version 2).
- Signers keep their own strkey form: `G`, `T` (pre-auth), `X` (hash-x), `P` (signed payload,
  with its payload).
- Amounts, balances, limits, ids and sequence numbers are decimal strings; flags, weights,
  thresholds and ledger numbers are JSON integers.
- Assets are `native` or `CODE:ISSUER`; trustline assets may also be `pool-share:<hex>`.
- `action` is `created`, `updated`, `removed` or `restored`.

The oracle file of an entry is `{"oracle_vocab_version", "network_passphrase", "ledgers":
[{"ledger_seq", "ledger_hash", "protocol", "close_time", "transactions", "upgrades",
"evicted"}]}`; each transaction has `hash`, `apply_index`, `successful`, `result`,
`inner_hash`, `fee {source, charged, balance_delta}`, `participants` (actual, from meta and
events), `changes`, and `declared` (what the envelope names, kept apart and never used as the
expected participants).

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
| `asset` | string | yes |  |
| `flags` | integer (uint32) | yes |  |
| `limit` | decimal string | yes |  |

### `trustline-removed`

| field | type | always present | meaning |
|---|---|---|---|
| `account` | string | yes |  |
| `asset` | string | yes |  |

### `trustline-flags`

| field | type | always present | meaning |
|---|---|---|---|
| `account` | string | yes |  |
| `after` | integer (uint32) | yes |  |
| `asset` | string | yes |  |
| `before` | integer (uint32) | yes |  |

### `trustline-limit`

| field | type | always present | meaning |
|---|---|---|---|
| `account` | string | yes |  |
| `after` | decimal string | yes |  |
| `asset` | string | yes |  |
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
| `pool` | string | yes |  |

### `liquidity-pool`

| field | type | always present | meaning |
|---|---|---|---|
| `action` | `"created"` / `"updated"` / `"removed"` / `"restored"` | yes |  |
| `asset_a` | string | yes |  |
| `asset_b` | string | yes |  |
| `pool` | string | yes |  |
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
| `executable` | string | yes | `wasm:<hex hash>` or `stellar-asset`. |

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
