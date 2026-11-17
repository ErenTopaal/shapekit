# Connecting a PostgreSQL indexer with a mapping file

The replay harness talks to an indexer through an adapter ([adapter-interface.md](adapter-interface.md)).
When your indexer runs in Docker and stores what it ingests in PostgreSQL, you do not need to
write that adapter. You write one TOML file instead, and shapekit's built-in **mapped adapter**
does the rest: it builds or pulls your image, starts the services next to it, delivers the
chain's ledgers, watches progress, and reads back what your indexer stored.

```sh
shapekit check indexer --mapping my-indexer.toml --var commit=<full commit hash>
# the same run through the harness command, with every harness option
shapekit harness run --mapping my-indexer.toml --var commit=<full commit hash> --network both
```

You need Docker and git on the machine. No Go, Rust or other toolchain is involved.

`adapters/walletbackend/adapter.toml` connects wallet-backend this way. Read it next to this page:
it uses every section described here.

## How a run uses the file

For each chain the harness starts `shapekit harness adapter --mapping <file>` and sends it the
protocol's requests. The mapped adapter answers them from the file:

| request | what the mapped adapter does |
|---|---|
| `describe` | clones `[source]` (or fetches the commit), reads the `[[source.fact]]` values, answers with `[target]` and `[claims]` |
| `start` | builds `[image]`, starts each `[[service]]` and waits until it is ready, connects to `[database]` and runs its `init` statements, releases the lake up to the bootstrap ledger (lake modes), runs each `[[step]]` to completion, then starts `[ingest]` with the harness's memory limit |
| `advance` | releases the lake's batches up to the new head (lake modes) |
| `status` | reads `[status].cursor` from the database, scans the ingest container's log with `[[status.error]]` and `[status].crash`, and reports peak memory from the container's cgroup |
| `export` | runs the `[export]` queries and turns stored rows into oracle changes |
| `dump` | streams every base table of the `public` schema, rows ordered by every column |
| `restart` | restarts the ingest container on the same database |
| `stop` | returns the ingest log's tail and removes every container it started |

Every container joins the harness's internal Docker network only, so your indexer reaches the
replay server and nothing else.

## Placeholders

Strings in the file can hold `{name}` placeholders. `{{` and `}}` are literal braces. A
placeholder with no value is an error, never literal text. SQL is not expanded.

| placeholder | value |
|---|---|
| `{<var>}` | a variable from `[vars]`, set with `--var <var>=<value>` |
| `{cache}` | the user cache directory (`$XDG_CACHE_HOME` or `~/.cache`; `~/Library/Caches` on macOS) |
| `{fact.<name>}` | a `[[source.fact]]` value |
| `{image}`, `{image.id}` | the target image's tag and its image id |
| `{service.<name>.host}` | the service container's name, which resolves on the network |
| `{service.<name>.ip}` | its address, for connections from the host |
| `{service.<name>.image}` | its image |
| `{network_passphrase}`, `{network}`, `{chain}`, `{protocol}` | the chain being replayed |
| `{rpc_url}`, `{archive_url}` | the replay server's JSON-RPC endpoint and history archive |
| `{lake.url}`, `{lake.path}`, `{lake.ledgers_per_batch}`, `{lake.batches_per_partition}` | the chain's SEP-54 lake |
| `{lake.dir}` | the lake's mount point in `sep54-filesystem` mode |
| `{bootstrap}`, `{last}`, `{checkpoint_frequency}` | ledger window of the chain |
| `{memory_bytes}`, `{docker_network}`, `{workdir}`, `{delivery}` | the harness's limits, network, work directory and delivery mode |

`[target]` can use variables, facts and `{cache}` only, because `describe` runs before
anything starts.

## Sections

### `[target]` and `[vars]`

```toml
schema = 1

[target]
name = "my-indexer"
version = "{commit}"             # the exact build, kept in the report
bootstrap = "checkpoint"         # or "ledger", see adapter-interface.md "Bootstrap"
delivery_modes = ["rpc", "sep54-s3"]
default_delivery = "rpc"
protocols = { min = "1", max = "{fact.max_protocol}" }
container_prefix = "myidx"       # optional

[vars.commit]
required = true
pattern = "^[0-9a-f]{40}$"       # the whole value must match
about = "the full commit hash to build"

[vars.repo]
default = "{cache}/shapekit/my-indexer"
```

`--var` sets a variable for one run; a variable the file does not define is refused.

### `[source]`, facts and `[image]`

```toml
[source]
git = "https://github.com/example/my-indexer"
dir = "{repo}"
commit = "{commit}"

[[source.fact]]
name = "max_protocol"
file = "go.mod"
pattern = '(?m)^\s*github\.com/stellar/go-stellar-sdk\s+(\S+)'
values = { "v0.7.3" = "28", "v0.8.0" = "30" }   # a capture missing here is an error

[[source.fact]]
name = "legacy_flag"
file = "cmd/ingest.go"
contains = "SomeOption"          # "true" or "false"

[image]
tag = "shapekit-my-indexer:{commit}"
dockerfile = "Dockerfile"
build_args = { GIT_COMMIT = "{commit}" }
base_label = "shapekit.my-indexer.bases"

[image.bases]
"golang:1.25.9-bookworm" = "docker.io/library/golang@sha256:..."
```

The build context is `git archive` of the commit. Every `FROM` of the Dockerfile must have an
entry in `[image.bases]`, and is built on that digest instead of its tag; a `FROM` with no pin
is refused. With `base_label` set, an existing image whose label holds the same pinned set is
reused. To run a published image instead, set `[image] name = "repo/image:tag@sha256:..."`
and leave out `tag`, `dockerfile` and `bases`.

### `[[service]]` and `[database]`

```toml
[[service]]
name = "db"
image = "postgres:17@sha256:..."
memory = "1073741824"            # bytes; no limit when absent
env = { POSTGRES_HOST_AUTH_METHOD = "trust", POSTGRES_DB = "indexer" }
command = ["postgres"]
ready = { exec = ["pg_isready", "-h", "127.0.0.1", "-U", "postgres"], seconds = 180 }

[[service]]
name = "s3"
delivery = ["sep54-s3"]          # only in these modes
image = "chrislusf/seaweedfs:4.46@sha256:..."
ready = { http = "http://{service.s3.ip}:8333/ledgers", seconds = 180 }

[database]
url = "postgres://postgres@{service.db.ip}:5432/indexer?sslmode=disable"
init = ["CREATE EXTENSION IF NOT EXISTS timescaledb"]
```

Services start in the order written. `ready` is either a command run inside the container
until it exits 0, or a URL fetched until it answers 200. `component` names the service's image
in the report.

### Delivery

`rpc` needs no section: your indexer reads `{rpc_url}`. The lake modes release the chain's
SEP-54 lake batch by batch, so your indexer never sees a ledger the harness has not made
visible:

```toml
[delivery.sep54-s3]
bucket_url = "http://{service.s3.ip}:8333/ledgers"   # each object is a PUT to <bucket_url>/<key>

[delivery.sep54-gcs]
url = "http://{service.gcs.ip}:4443"                 # a GCS JSON API, such as fake-gcs-server
bucket = "ledgers"
prefix = "{network}/"

[delivery.sep54-filesystem]
mount = "/lake"                                      # read-only in the step and ingest containers
```

Every mode in `target.delivery_modes` other than `rpc` needs its section.

### `[env]`, `[[step]]` and `[ingest]`

```toml
[env]
DATABASE_URL = "postgres://postgres@{service.db.host}:5432/indexer?sslmode=disable"
NETWORK_PASSPHRASE = "{network_passphrase}"
RPC_URL = "{rpc_url}"

[[env_when]]
delivery = "sep54-s3"
env = { LEDGER_SOURCE = "s3", S3_ENDPOINT = "http://{service.s3.host}:8333" }

[[env_when]]
fact = { legacy_flag = "true" }
env = { LEGACY = "1" }

[[step]]
name = "migrate up"
command = ["migrate", "up"]      # in the target image unless `image` is set

[ingest]
command = ["ingest"]
restart = "on-failure"           # docker's --restart
```

Steps and the ingest container get the same environment. The ingest container's `--memory` and
`--memory-swap` are the harness's memory limit.

### `[status]`

```toml
[status]
cursor = "SELECT value FROM ingest_state WHERE key = 'last_ledger'"
log_tail = 5000
crash = '(?m)^(panic: runtime error|fatal error: )'

[[status.error]]
pattern = '\blevel=(\w+)'
values = ["error", "fatal"]      # the first match's first group must be one of these
also = 'ingest'                  # optional: the line must match this too
```

`cursor` returns one row with the last ledger your indexer durably stored. Read it from the
same transaction that commits the ledger's data, not from a log line or a metric. No row, NULL
and 0 mean none yet. The newest log line any `[[status.error]]` matches is `last_error`. A
`crash` match, or a container that stays down, reports `crashed`; Docker's OOM kill reports
`oom`. A container that is running is `starting` until the cursor has a value, then `running`.

### `[claims]`

The claims of the protocol's `describe`, written in TOML. See adapter-interface.md "Claims".

```toml
[claims.participants]
address_kinds = ["account"]
mux_ids = false

[[claims.changes]]
kind = "token-event"
where = { event = ["transfer", "mint", "burn"] }
fields = ["event", "contract", "from", "to", "amount"]
```

### `[export]`: from rows to oracle changes

```toml
[export]
transactions = "SELECT id, hash, ledger FROM txs WHERE ledger BETWEEN $1 AND $2 ORDER BY id"
transaction = { id = "id", hash = "hash", ledger = "ledger" }
participants = "SELECT address FROM tx_participants WHERE tx_id = $1"
participant = { address = "address", forms = "account" }
rows = "SELECT * FROM changes WHERE tx_id = $1 ORDER BY op, seq"
table = "changes"
category = "category"            # optional: kept with each change and unmapped row
reason = "reason"
order = ["op", "seq"]
operation = { column = "op", encoding = "index" }   # or "toid"

[export.forms]
account = ["G", "C"]
signer = ["G", "T", "X", "P"]
```

- `transactions` returns one row per stored transaction of ledgers `$1` to `$2`. `hash` is bytes
  or hex text. The `id` value is the `$1` of `participants` and `rows`.
- Addresses are stored bytes: a strkey version byte, then the payload. They are decoded strictly,
  never padded or cut, and must be one of the listed strkey letters. Bytes that do not decode
  become `null` with the stored hex and the reason in the change's `raw`.
- `operation` splits a transaction's rows into operations. `index` is the 0-based operation
  index with NULL for the transaction's own rows (fees); `toid` is a TOID, whose low 12 bits
  minus one are the index and where 0 is the transaction.
- Parameters are converted to the type the statement asks for, and columns of types other than
  integers, text, bytes, JSON and booleans are read in PostgreSQL's text form.

**Lookups** name asset contracts. Each row of `query` is a `code` and an `issuer`; the key is the
Stellar Asset Contract id under the run's passphrase:

```toml
[[export.lookup]]
name = "assets"
type = "asset-contract"
native = true
query = "SELECT code, issuer FROM assets"
```

**Unattributed rows**, stored for no stored transaction, are reported, never dropped. Each
query reads a window of `unattributed_window` ledgers (`$1` to `$2`, default 64):

```toml
[[export.unattributed]]
query = "SELECT * FROM changes c WHERE c.ledger BETWEEN $1 AND $2 AND NOT EXISTS (SELECT 1 FROM txs t WHERE t.id = c.tx_id)"
ledger = { column = "ledger" }   # encoding = "toid" reads the ledger from a TOID's upper half
table = "changes"
note = "change of a transaction the indexer did not store"
```

**Passes and cases** map the rows of one operation. Passes run in order. A pass takes the rows
no earlier pass took that match its `select`, in order, and gives each to the first case that
matches it. Rows that no pass takes are reported as unmapped, so a gap in the file shows up in
the report next to the entry instead of disappearing.

```toml
[[export.pass]]
select = { category = "BALANCE" }

[[export.pass.case]]
level = "transaction"            # only the transaction's own rows; "operation" for the others
when = { reason = "DEBIT" }      # column = value, or a list of values
kind = "token-event"
fields.event = { value = "fee" }
fields.from = { address = "account", forms = "account" }
fields.amount = { text = "amount" }

[[export.pass.case]]
when = { reason = "DEBIT" }
kind = "token-event"
fields.event = { value = "transfer" }
fields.from = { address = "account", forms = "account" }
fields.amount = { text = "amount" }
pair = { select = { reason = "CREDIT" }, next = "seq", same = ["token", "amount"] }
paired.to = { address = "account", forms = "account" }

[[export.pass.case]]
unmapped = "BALANCE row with an unknown reason"
```

A case matches on `level`, `when`, `when_version = { column = ["C"] }` (the stored bytes'
version byte is one of the letters) and `when_version_not` (it is none of them). It then
either reports the row as `unmapped` with a note (`{<column>:version}` in the note prints the
version byte) or builds one change of `kind`:

- `merge = [columns]`: one change per operation and value of those columns; later rows with
  the same values are taken without a change of their own.
- `pair`: the next row of the pass is the partner when it matches `select`, its `next` column is
  one higher, and it agrees on `same`. `paired` fields are read from the partner, `unpaired`
  fields from the row itself when there is no partner.
- `absorb`: the first later row of the operation that matches `select` and agrees on `same` is
  taken with the row, and its `fields` are read from it.

Fields are read in name order. Each field has one source:

| source | value |
|---|---|
| `address = "<col>", forms = ...` | a strkey from stored bytes (see above); absent when NULL |
| `int = "<col>"` | a JSON integer |
| `text = "<col>"` | a string; `default` when NULL, `typed = "u64"` wraps it as `{"type", "value"}`, `skip_empty` drops `""` |
| `value = <constant>` | the constant |
| `equals = "<col>", value = "<text>"` | `true` when the column equals the text |
| `stroops = "<col>"` | a decimal amount with at most 7 fraction digits, as stroops |
| `negate = "<col>"` | a decimal integer, negated |
| `text_hex = "<col>"` | the text's bytes as lowercase hex |
| `json = "<col>", path = "a.b", as = "string" \| "base64-hex" \| "uint32"` | a value inside a JSON column |
| `bits = "<col>", map = { "<bit>" = <value> }` | flag bits translated bit by bit |
| `asset = "<col>", pool = "<col>", lookup = "<name>"` | `pool-share:<L strkey>` from a hex pool id, else the asset the lookup names for a contract address |

A field that is NULL is left out of the change, unless the source says otherwise. When a value
is present but cannot be read (an amount with 8 decimals, a token no lookup knows), the row is
reported as unmapped with the field's `note`, or a note naming the column.

Claim only what the export can show: anything a claim covers that the export lacks is a
finding (see adapter-interface.md "Claims").

## Checking a mapping

Start with one small chain and read `HARNESS.md`:

```sh
shapekit harness run --mapping my-indexer.toml --var commit=<hash> --chain cap01.bump-sequence.p30
```

Entries with `unmapped` rows show where the file has no case yet. `shapekit harness run`
refuses a file whose sections do not fit together (an unknown lookup, a delivery mode without
its section, a field with two sources) before any chain runs.

A reproductions table can run through a mapping too: `shapekit harness reproduce --table
adapters/walletbackend/reproductions.toml --mapping adapters/walletbackend/adapter.toml`
passes each case's commit as `--var commit=<commit>`.

## wallet-backend through its mapping

`adapters/walletbackend/adapter.toml` describes the same target as the Go adapter in that
directory: the same build on pinned bases (and so the same cached image per commit), services,
environment, status rules, claims and row mapping. Over every chain on both networks at
wallet-backend `3e22515a`, it gives every entry the same verdict, reason and problems as the Go
adapter, and the reproductions table holds through it.

The two differ only where no run reaches: the notes of unmapped rows whose asset or amount
cannot be read name the column and the lookup instead of wallet-backend's table names, an
unmapped row renders timestamps in PostgreSQL's text form, and the error text of an
undecodable address comes from the Rust strkey decoder rather than go-stellar-sdk.
