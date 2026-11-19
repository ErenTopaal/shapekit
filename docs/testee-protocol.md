# Testee protocol

A testee is a small program that wraps one Stellar SDK at one exact version and answers
requests from the conformance runner (`shapekit conformance run`). The runner sends corpus
data through the SDK's own decoder, encoder, strkey codec, ordering functions and native
projection, and compares what comes back with the network's answer (the corpus manifest) and
with the other SDKs.

Protocol version: **2**. Version 2 adds the ops `tx_hash`, `balance_id`, `contract_id`, `coverage_snapshot`,
`coverage_new` and `address_from_scval`
and changes nothing else, so the runner also accepts a version 1 testee; that testee cannot
list the new ops, and their checks count as `unsupported`. The machine-readable form is
[schema/testee-protocol.schema.json](../schema/testee-protocol.schema.json). This page and the
schema describe the same protocol; if they disagree, report it as a bug.

## Transport

- The runner starts the testee as a child process and talks to it over stdin and stdout.
- Every message is one JSON object encoded as UTF-8 on one line, terminated by `\n`. No
  pretty printing, no blank lines, no byte order mark.
- The runner sends one request and waits for exactly one response before it sends the next.
  Requests and responses are matched by `id` as well; a response whose `id` differs from the
  outstanding request is a protocol violation.
- stdout carries responses only. Logging, warnings and stack traces go to stderr. The runner
  keeps the last 16 KiB of stderr and adds it to the report when the testee exits or breaks
  the protocol.
- A line can be large (a full `LedgerCloseMeta` in base64). Testees must accept request lines
  of at least 64 MiB.
- A response line is at most 256 MiB (268435456 bytes, not counting the `\n`). The runner
  stops reading a longer line at that size, records a protocol violation (`error`) and restarts
  the testee, so a testee that prints without end cannot grow the runner without bound. A
  response that is not UTF-8 is a protocol violation too.
- When stdin reaches end of file the testee exits with status 0.
- The runner applies a per-request timeout and a memory limit to the testee process (see
  [Limits the runner applies](#limits-the-runner-applies)). When a testee times out, exits,
  is killed or breaks the protocol, the runner records the request in flight as `error`,
  restarts the testee and sends `hello` again.

## Messages

### Request

```json
{"id": "17", "op": "decode_encode", "type": "TransactionEnvelope", "xdr": "AAAAAg...", "depth_limit": 1000, "len_limit": 4096}
```

- `id`: a string the runner chooses. No two requests to the same testee carry the same `id`.
  Echo it back. Answer a line that is not a JSON object with a string `id` with `"id": ""`
  and a `bad-request` error.
- Skip an empty line (a `\n` with nothing before it) without answering. The runner never sends
  one, and every testee and skeleton here skips it, so a stray newline does not shift the responses.
- `op`: one of the operation names below.
- The other members depend on `op`. A testee must ignore members it does not know, so that
  later protocol versions can add optional members.

### Response

Exactly one of three forms:

```json
{"id": "17", "status": "ok", "result": {"xdr": "AAAAAg..."}}
{"id": "17", "status": "error", "error": {"class": "rejected", "message": "xdr value invalid"}}
{"id": "17", "status": "unsupported", "reason": "go-stellar-sdk has no scval map sort"}
```

A response with any other top-level member, an unknown `status`, or an `error.class` outside
the table below is a protocol violation.

- `status: "ok"`: the SDK produced a value. `result` is an object holding the op's result
  members.
- `status: "error"`: the SDK, or the testee, did not produce a value. `error.class` says why,
  `error.message` is the SDK's own message (free text, never compared).
- `status: "unsupported"`: the SDK version under test has no API for this operation, or for
  this particular input of it (for example an XDR type the SDK does not know). `reason` is
  required, must not be empty, and names what is missing. The check counts as `unsupported`,
  never as a pass.

### Error classes

| class | meaning | how the runner counts it |
|---|---|---|
| `rejected` | the SDK refused the input through its normal error path: a returned error, a thrown exception, a `Result::Err` | a rejection. Correct for `must-reject` inputs |
| `panic` | the SDK crashed instead of rejecting: a Rust panic, a Go panic, a JS `RangeError: Maximum call stack size exceeded` or any error the testee can tell is not the SDK's documented rejection | always `diverges-from-network`: the correct answer to a bad input is a rejection, not a crash |
| `bad-request` | the request does not follow this protocol (missing member, bad base64, unknown op name) | `error`: a runner or testee bug, never an SDK finding |
| `internal` | the testee itself failed for a reason unrelated to the input | `error` |

A testee that cannot tell `rejected` from `panic` (because the SDK uses one exception type
for both) reports `rejected`. Catch panics around every SDK call so that one input cannot take
the process down. A process that dies anyway is recorded as `error` for that request.

## Value encodings

These are fixed for every op so that the runner compares like with like.

| value | encoding |
|---|---|
| XDR bytes | standard base64 (RFC 4648 section 4) with `=` padding, no line breaks |
| raw bytes (strkey payloads) | standard base64 with padding |
| XDR type names | the type name as the Rust `stellar-xdr` crate spells it: `TransactionEnvelope`, `LedgerCloseMeta`, `ScVal`, `ScSpecFunctionV0` |
| decoded values as JSON | SEP-51 XDR-JSON (version 2.0.1) of the value itself, not wrapped in its type name. Compared as JSON values: member order does not matter, everything else must be equal, including 64-bit integers written as strings |
| order | integer `-1`, `0` or `1`: the sign of `compare(a, b)` |
| 64-bit IDs | decimal string with no sign for unsigned values, a leading `-` only for negative values, no leading zeros, no whitespace (`"0"`, `"16"`, `"-1"`) |
| liquidity pool ID | lowercase hex of the 32-byte pool hash, 64 characters |
| strkey kind | one of the names in the strkey table below |

## hello

Sent first, after every (re)start.

Request: `{"id": "1", "op": "hello"}`

Result members:

| member | type | meaning |
|---|---|---|
| `protocol` | integer | `2` (the runner also accepts `1`) |
| `sdk` | string | the package under test: `stellar-xdr` (Rust), `github.com/stellar/go-stellar-sdk` (Go), `@stellar/stellar-sdk` (JS), `soroban-sdk` (the Rust contract SDK), `stellar-sdk` (Python), `network.lightsail:stellar-sdk` (Java), `stellar_flutter_sdk` (Dart), `soneso/stellar-php-sdk` (PHP), `stellar-dotnet-sdk` (.NET), `com.soneso.stellar:stellar-sdk-jvm` (KMP), `stellar-cli` (stellar-cli) |
| `sdk_version` | string | the exact version that is linked or loaded, read from the build or the installed package, never typed in. The runner refuses a testee whose `sdk` or `sdk_version` differs from the `package` and `version` in the versions file |
| `components` | object, string to string | other packages whose behaviour the ops depend on and their exact versions, for example `{"stellar-strkey": "1.0.0"}`. Can be empty |
| `ops` | array of strings | the ops this build implements, excluding `hello`. The runner refuses a `hello` that lists an op this page does not define. It does not send unlisted ops and records their checks as `unsupported` |
| `decode_limits` | `"honoured"` or `"ignored"` | whether `depth_limit` and `len_limit` reach the SDK's decoder (see `decode_encode`). `"honoured"` covers the XDR inputs of every op. `asset_order`, `pool_id`, `map_sort`, `map_validate`, `to_native` and `address_from_scval` requests carry no limits, so a testee that reports `"honoured"` decodes their inputs at depth 1000 with the input's own length as the length limit, or with `depth_limit` and `len_limit` when a request carries them |

## Operations

### decode_encode

Decode bytes as one value of a type and encode the value again.

Request: `type` (string), `xdr` (base64), `depth_limit` (integer), `len_limit` (integer).

- Decode must consume the whole input. Trailing bytes are a rejection.
- `depth_limit` and `len_limit` are the runner's decode limits (`Limits { depth, len }` in
  rs-stellar-xdr). A testee passes them to the SDK when the SDK's API accepts limits and
  reports `decode_limits: "honoured"`. Otherwise it decodes with the SDK's defaults and reports
  `"ignored"`. The matrix lists each testee's `decode_limits`, so you can see which testees
  decoded depth and length entries under SDK defaults.
- Re-encode with the SDK's encoder and the same limits.

Result: `xdr` (base64 of the re-encoded bytes).

### decode_json

Decode bytes as one value of a type and render it as SEP-51 XDR-JSON.

Request: same members as `decode_encode`.

Result: `json` (the SEP-51 value as a JSON value, not a string).

A testee implements this only with the SDK's own decoded objects: either an SDK-native SEP-51
renderer, or a projection the testee writes from the SDK's decoded value (js-xdr type
introspection, Go reflection). A second XDR decoder inside the testee defeats the check.

### encode_json

Read a SEP-51 XDR-JSON value of a type and encode it as XDR.

Request: `type` (string), `json` (JSON value).

Result: `xdr` (base64).

This op reads JSON where every other op reads bytes. It exists because some flaws only show
on the JSON-to-XDR path. The rs-stellar-xdr advisory
[GHSA-3xhm-p452-9wjh](https://github.com/stellar/rs-stellar-xdr/security/advisories/GHSA-3xhm-p452-9wjh)
is one: 28.0.0 accepts a JSON `ScSpecFunctionV0` with two `outputs` (the XDR allows at most
one) and encodes bytes that no decoder accepts. No decode-side op can see it. SDKs without a
SEP-51 reader answer `unsupported`.

### strkey_decode

Request: `text` (string, passed to the SDK unchanged, including any whitespace or case
oddities), or instead `text_base64` (standard base64 of the input bytes) when the input is not
UTF-8 and so has no JSON string form. Exactly one of the two is present.

A testee passes the `text_base64` bytes to the SDK as they are when the SDK's decoder can take
them (a Go `string`, a Rust `&[u8]`). An SDK whose decoder takes only Unicode text cannot be
given bytes that are not UTF-8, so its testee answers `unsupported` for them rather than
picking a lossy mapping to text.

Result: `kind` (strkey kind) and `payload` (base64 of the raw payload, defined below).

If the SDK has one decoder per kind rather than a generic one, the testee picks the decoder
from the first character and reports `rejected` when that decoder refuses the text, and when
no decoder exists for that first character.

### strkey_encode

Request: `kind` (strkey kind), `payload` (base64 raw payload).

Result: `text`.

The testee calls the SDK's encoder for that kind with the payload in whatever structured form
the SDK's encoder takes, and must not repair the payload first. An SDK encoder that refuses
the payload is `rejected`.

### Strkey kinds and raw payloads

The raw payload is the byte string between the version byte and the checksum, as SEP-23
defines it. A testee whose SDK returns structured values rebuilds this layout from them.

| kind | prefix | version byte | raw payload |
|---|---|---|---|
| `account` | G | 6 << 3 | 32-byte ed25519 public key |
| `muxed_account` | M | 12 << 3 | 32-byte ed25519 key, then the 8-byte ID big-endian |
| `seed` | S | 18 << 3 | 32-byte ed25519 seed |
| `pre_auth_tx` | T | 19 << 3 | 32-byte transaction hash |
| `hash_x` | X | 23 << 3 | 32-byte hash |
| `signed_payload` | P | 15 << 3 | 32-byte ed25519 key, 4-byte big-endian payload length, the payload, zero padding to a multiple of 4 |
| `contract` | C | 2 << 3 | 32-byte contract ID |
| `liquidity_pool` | L | 11 << 3 | 32-byte pool ID |
| `claimable_balance` | B | 1 << 3 | 1 type byte (`0` for v0), then the 32-byte balance ID |
| `muxed_contract` | W | 22 << 3 | 32-byte contract ID, then the 8-byte ID big-endian (CAP-84) |

### asset_order

Request: `a`, `b` (each base64 XDR of an `Asset`).

The testee decodes both with the SDK's XDR decoder, turns them into the SDK's asset objects
where the comparison lives on those objects (JS `Asset.fromOperation`), and calls the SDK's
comparison (`Asset.compare` in JS, `xdr.Asset.LessThan` in Go, `Ord` in Rust,
`LiquidityPoolAsset.is_valid_lexicographic_order` in Python, which is "less than",
`Asset.compareTo` in Java).

Result: `order` (`-1`, `0` or `1`). An SDK that only offers "less than" answers with
`lt(a, b) ? -1 : lt(b, a) ? 1 : 0`.

### pool_id

Request: `a`, `b` (base64 XDR `Asset`), `fee` (integer, basis points; 30 is the only fee core
accepts).

The assets are passed to the SDK in the given order. An SDK that refuses an unordered or equal
pair answers `rejected`. An SDK with XDR types but no pool ID function (Rust `stellar-xdr`)
computes SHA-256 of the `LiquidityPoolParameters` XDR written by the SDK's own encoder, and
refuses pairs that the SDK's own `Asset` ordering does not put strictly in order, as core does.

Result: `pool_id` (lowercase hex).

### map_sort

Request: `entries`, an array of `{"key": <base64 ScVal>, "val": <base64 ScVal>}` in input
order.

The testee calls the SDK's sorted-map constructor (`scvSortedMap` in JS,
`ScMap::sorted_from` in Rust, `scval.to_map` in Python, `Scv.toMap` in Java). A constructor that
takes a dictionary
cannot be given two equal keys, so its testee answers `unsupported` for an input that repeats a
key.

Result: `xdr` (base64 of the resulting `ScVal`, arm `SCV_MAP`).

### map_validate

Request: `xdr` (base64 of an `ScVal` whose arm is `SCV_MAP` with a present map).

Result: `valid` (boolean): whether the SDK's own map validation accepts the map. An SDK
validation that refuses the map is `"valid": false`, not an error; `rejected` is only for
bytes the SDK cannot decode.

### id_parse

Request: `kind` (`"muxed"` or `"offer"`), `input` (a JSON string or a JSON number, written by
the runner exactly as the corpus vector has it, which includes numbers above 2^53 and
negative numbers).

The testee hands `input` to the SDK API that takes a textual or numeric ID (JS `MuxedAccount`
and `Operation.manageSellOffer`; in Rust the only textual path is the XDR-JSON reader of
`ManageSellOfferOp.offer_id`, and muxed IDs have none, so the Rust testee answers
`unsupported` for them; in Go the Horizon response types, `horizon.Offer.ID` and
`horizon.Transaction.AccountMuxedID`, which read IDs from JSON strings only; in Python
`MuxedAccount` and `ManageSellOffer`, which take integers only; in Java the Horizon response
types `OfferResponse.id` and `TransactionResponse.accountMuxedId`, read with the SDK's Gson), in
the JSON type
it arrived as. A testee must read numbers
without going through a binary floating point type, unless that is what the SDK itself does
with them. If the SDK has no API for the input's JSON type, answer `unsupported`.

Result: `id`, the 64-bit value the SDK would encode, as a decimal string (unsigned for muxed,
signed for offer).

### to_native

Request: `xdr` (base64 of an `ScVal`).

The testee calls the SDK's native projection (JS `scValToNative`) and describes the result
losslessly as a native value:

| native value | form |
|---|---|
| map or dictionary | `{"type": "map", "entries": [{"key": <native>, "value": <native>}, ...]}` in the order the native structure iterates |
| array | `{"type": "array", "items": [<native>, ...]}` |
| string | `{"type": "string", "value": "..."}` |
| byte buffer | `{"type": "bytes", "value": "<base64>"}` |
| integer of any width | `{"type": "integer", "value": "<decimal>"}` |
| boolean | `{"type": "bool", "value": true}` |
| null or undefined | `{"type": "null"}` |
| anything else | `{"type": "object", "class": "<language type name>", "text": "<its string form>"}` |

Result: `native`.

### address_from_scval

Request: `xdr` (base64 of an `ScVal`).

The testee hands the `ScVal` to the SDK's conversion into its address type, the type that
holds the most kinds of address (soroban-sdk `MuxedAddress::try_from_val`), and writes the
address it gets as a strkey with the SDK's own encoder. An `ScVal` that is not an address, or an
address the type cannot hold, must come back as `rejected`; a crash is a `panic`.

Result: `text` (the strkey).

### tx_hash

Request: `xdr` (base64 XDR `TransactionEnvelope`), `network_passphrase` (string),
`depth_limit` and `len_limit` (optional in the schema; the runner always sends them), and
optionally `inner` (boolean).

The SDK builds its transaction object from the envelope and the passphrase and returns the
transaction hash: the hash of the outer transaction, for a fee bump the fee bump's own hash.
With `inner: true` the testee returns the hash of a fee bump's inner transaction; `inner` on
an envelope that is not a fee bump is a `bad-request`. An SDK whose hash function takes a
network id instead of a passphrase (Rust `stellar-xdr`) gets the SHA-256 of the passphrase,
which is the network id by definition.

Result: `hash` (lowercase hex, 64 characters).

### balance_id

Request: `xdr` (base64 XDR `TransactionEnvelope`), `network_passphrase`, `op_index` (integer),
`depth_limit` and `len_limit` (optional in the schema; the runner always sends them).

The ID of the claimable balance the `CreateClaimableBalance` operation at `op_index` creates.
For a fee bump the operation belongs to the inner transaction. An operation of another type,
or an index past the last operation, is `rejected`. An SDK without a balance ID function (Rust
`stellar-xdr`) computes SHA-256 of the `HashIdPreimage` (`ENVELOPE_TYPE_OP_ID`) written by the
SDK's own encoder.

Result: `balance_id` (lowercase hex of the `ClaimableBalanceId` XDR, 72 characters: the 4-byte
type, then the 32-byte hash).

### contract_id

Request: `preimage` (base64 XDR `ContractIdPreimage`), `network_passphrase`, `depth_limit` and
`len_limit` (optional in the schema; the runner always sends them).

The ID of the contract the preimage deploys on that network. An SDK with a function for asset
preimages (`Asset.contractId`, `xdr.Asset.ContractID`) uses it. For a preimage without such a
function, the testee computes the SDK's SHA-256 of the `HashIdPreimage`
(`ENVELOPE_TYPE_CONTRACT_ID`) written by the SDK's own encoder, the way the SDK's asset
function does.

Result: `contract_id` (lowercase hex, 64 characters).

### coverage_snapshot

Request: no members besides `op`.

The code of the SDK package that ran at least once since the testee process started. A testee
lists the op in `hello` only when it collects coverage, which costs time, so only a coverage
build or mode does:

| testee | how | a range is |
|---|---|---|
| Rust | built with `RUSTFLAGS="-C instrument-coverage --cfg shapekit_coverage"`; the profiler runtime writes the counters on request and the toolchain's `llvm-profdata` and `llvm-cov export` (rustup's `llvm-tools` component) read them | a code region of a `stellar-xdr` source file, `src/<file>:<line>:<col>-<line>:<col>` |
| Go | built with `-cover -covermode=atomic -coverpkg=github.com/stellar/go-stellar-sdk/...,.`; `runtime/coverage` writes the counters on request and `go tool covdata textfmt` reads them | a statement block of a go-stellar-sdk file, `<package path>/<file>:<line>.<col>,<line>.<col>` |
| JavaScript | started with `SHAPEKIT_COVERAGE=1`; V8 precise block coverage through the inspector, started before the SDK is imported | a function or block range of a file of the installed package, `<path>:<start offset>-<end offset>` |
| Python | started with `SHAPEKIT_COVERAGE=1`; coverage.py line coverage, started before the SDK is imported, read in the process with no data file, of the `stellar_sdk` package, and on the SDK's default branch also of `stellar_xdr` and `stellar_strkey`, which hold its XDR and strkey code there | a line of a measured package's source file that has run, `<package>/<path>.py:<line>`: `stellar_sdk/...` up to 17.x, and `stellar_sdk/...`, `stellar_xdr/...` or `stellar_strkey/...` on the default branch |
| Java | started with `SHAPEKIT_COVERAGE=1` and the JaCoCo agent of its build (`-javaagent:.../jacocoagent.jar=output=none,includes=org.stellar.sdk.*`); the agent's execution data, analysed in the process with `org.jacoco.core` | a line of an SDK class where an instruction has run, `org/stellar/sdk/<path>.java:<line>` |
| KMP | the same as Java, with `includes=com.soneso.stellar.sdk.*` | a line of an SDK class where an instruction has run, `com/soneso/stellar/sdk/<path>.kt:<line>` |
| stellar-cli | started with `SHAPEKIT_COVERAGE=1` and a `stellar` binary built with `-C instrument-coverage` from the crate source of the version (`cargo install --locked --no-default-features`: the default feature only adds wasm-opt, the OS keyring and Ledger support, which the measured commands do not use, and the keyring needs the system's libdbus); every run of the binary adds its counters to the testee's profile files, which llvm-profdata merges and llvm-cov export reads | a code region of a `stellar-xdr`, `stellar-strkey` or `soroban-cli` source file, `<crate>-<version>/<path>:<line>:<col>-<line>:<col>` |

Result: `tool` (what measured it), `ranges` (sorted strings, each range once), and for the
JavaScript testee `uncovered` (the ranges V8 has reported with a count of 0 that have not run
since the process started, in the same form). V8 resets its counters each time it is read, so
the JavaScript testee folds every read into what it has seen before; every snapshot covers the
whole process, as for every other testee. V8 lists a nested block only while its count differs from the count of the block around it, so a
block can be missing from both lists: it then ran exactly as often as its parent. To decide
whether a range ran, look it up in `ranges`, then in `uncovered`, and otherwise take the
answer of the smallest listed range that contains it. Rust regions and Go blocks are a fixed
set, and coverage.py reports only lines that ran, so those testees send no `uncovered`. Range strings are
only compared within one SDK version and one tool: the union over runs, or the ranges one
run adds, is what is meaningful. Counters live in the process, so what a testee covered before
the runner restarted it is not in a later snapshot.

### coverage_new

Request: no members besides `op`.

The cheap form of `coverage_snapshot`, for a fuzzer that asks after every input whether the
input reached code nothing before it reached. A testee lists it exactly when it lists
`coverage_snapshot`, except the stellar-cli testee: its counters are only in the profile files
each run of the binary writes when it exits, which take an llvm-profdata merge to read, so it
answers `unsupported`. Result: `new`, how many coverage units have run for the first time since
the previous `coverage_new` request (since the process started, for the first one), and
`total`, how many have run since the process started. The units are the tool's own, read in
the process with no files and no external tool:

| testee | a unit is |
|---|---|
| Rust | an llvm counter of the testee binary, `stellar-xdr` and the crates around it (`__llvm_profile_begin_counters` to `__llvm_profile_end_counters`) |
| Go | a statement-block counter of an instrumented package, read from `runtime/coverage.WriteCounters` |
| JavaScript | a range of the SDK package, folded exactly as `coverage_snapshot` folds it (the two ops share one fold, so neither loses what the other read) |
| Python | a line of a `stellar_sdk` source file that has run, as `coverage_snapshot` lists it |
| Java, KMP | a JaCoCo probe of an SDK class |

The numbers are only compared within one process: they say whether coverage grew, not which
code ran. Ask `coverage_snapshot` for the ranges themselves.

`shapekit conformance run --coverage` builds and starts these variants (the Rust, Go and
stellar-cli builds go to `target/testees-cov/`), runs the selection, asks each testee for one snapshot at the end
and writes `coverage.json` next to the matrix, with the number of processes each testee needed.

## How the runner uses the ops

The table lists, per manifest entry kind
([crates/shapekit-manifest/src/model.rs](../crates/shapekit-manifest/src/model.rs)), which ops
the runner sends and when a check passes. Each case of a vector file carries its op name in
`op`, its inputs, and `expected`, and the runner turns it into one request (two for a valid
strkey). A case the runner cannot map is an `error` (a runner bug), never skipped.

| entry kind | ops sent | pass when |
|---|---|---|
| `xdr`, `decode = must-accept` | `decode_encode`, `decode_json`, `encode_json` (the last two only with an expected JSON file, `encode_json` only with `roundtrip = true`) | `decode_encode` is ok and, with `roundtrip`, returns the input bytes; `decode_json` equals the variant's expected JSON; `encode_json` of the expected JSON returns the input bytes |
| `xdr`, `decode = must-reject` | `decode_encode` | the answer is `rejected` |
| `xdr` `LedgerCloseMeta` that the network wrote (`validity = valid`, not synthetic) | also `tx_hash` for every envelope of the transaction set (and `inner: true` for each applied fee bump), `balance_id` for every successful `CreateClaimableBalance`, `contract_id` for every contract a successful `InvokeHostFunction` created | the hash or ID equals the one the ledger holds: the `transactionHash` of the envelope's result pair, the balance ID the operation returned, the ID of the contract instance the operation created |
| `xdr` `TransactionEnvelope` applied in a committed chain (`source.chain`, `source.ledger_seq`) | also `tx_hash` | the hash equals the `transactionHash` of the envelope's result pair in that ledger of `corpus/chains/<chain>/<variant>/` |
| `xdr`, `decode = implementation-defined` | `decode_encode`, `decode_json` | the SDK answers without crashing; the answer is recorded, not judged (see below) |
| `ordering`, `asset_order` cases | `asset_order` | `expected` `less`, `equal`, `greater` is the order core uses (`Asset` comparison by type, code, then issuer key bytes) |
| `ordering`, `pool_id` cases | `pool_id` | the pool ID equals `expected.pool_id`, or the answer is `rejected` when `expected.reject` is set |
| `ordering`, `map_sort` cases | `map_sort` | the result equals `expected.map` byte for byte, or is `rejected` when the case expects a rejection |
| `ordering`, `map_validate` cases | `map_validate` | `valid` is true for `expected = accept`, false for `reject` |
| `strkey` | `strkey_decode`, and `strkey_encode` for valid strings | decode gives the expected kind and raw payload, or `rejected` for invalid strings; encode of that kind and payload gives back the text |
| `textual-id` | `id_parse` | the ID equals `expected.id`, or `rejected` when the case expects a rejection. A case whose `expected` is `{"either": [{"id": ...}, {"reject": ...}]}` passes with exactly that ID or with `rejected`; any other ID diverges. This form is used where reading the exact ID and refusing are both correct: decimal text with leading zeros, integral JSON numbers written with a zero fraction, JSON numbers above 2^53-1, and negative offer IDs, which the XDR `int64` holds and stellar-core refuses only when it checks the operation. Such a case is judged against its own expectation even when the entry is `implementation-defined` |
| `native-projection` | `to_native` | `rejected`, or a native map that keeps every entry of the input map |
| `native-projection`, `address_from_scval` cases | `address_from_scval` | the strkey equals `expected.text`, or the answer is `rejected` when the case expects a rejection. `{"either": [{"text": ...}, {"reject": ...}]}` passes with that strkey or with `rejected`: SDK address types differ on whether they hold claimable balance and liquidity pool addresses |

The runner derives each expected hash and ID with the law library (`crates/shapekit-laws`) and
uses it only where the ledger confirms it: a derived transaction hash must be the hash core
wrote in a result pair of that ledger, a derived contract ID must be the ID of an instance the
operation created. Where the ledger does not confirm a derivation, the check is an `error`,
never a finding against an SDK. Entries that core did not write (synthetic entries, envelopes
core refused, history envelopes with no committed ledger) get no such checks.

Where the expected XDR-JSON contains stellar-xdr's `<INVALID:` marker (it writes one for a
value with no strkey form, such as a signed-payload signer with an empty payload), SEP-51 has
no canonical rendering of the value. `decode_json` then passes with any JSON the SDK renders
(it still fails if the SDK refuses), and `encode_json` is not sent, since there is no
canonical JSON to encode from. `decode_encode` checks the byte round trip as usual.

Decode requests carry `len_limit` equal to the input length and a `depth_limit` that depends
on the type: 1000 for every type except `LedgerCloseMeta`, which gets 1012. 1000 is the depth
at which core's Rust parser reads envelopes from protocol 28 (`xdr.envelope.depth` in
[registry/caps-limits.toml](../registry/caps-limits.toml)). A `LedgerCloseMeta` holds its
envelopes up to 12 levels further down (`xdr.meta.depth`). The corpus expectations were
computed under the same limits. The matrix records both values as `decode_depth` and
`ledger_decode_depth`.

Every `xdr` variant (`testnet`, `mainnet`, `any`) is checked separately. An entry's result is
the worst of its checks, in this order: `error`, `diverges-from-network`,
`diverges-from-peers`, `unsupported`, `reference`, `pass` (a `reference` check counts as a pass
for the entry's result, see below). A `panic` always counts as `diverges-from-network`.

Because an entry is as bad as its worst check, one op an SDK lacks (Go has no `encode_json`)
makes the whole entry `unsupported`. The matrix therefore also counts every check on its own,
per testee (`check_summary`) and per testee and op (`op_summary`).

An implementation-defined entry has no required answer: the references disagree on it (for the
bool word 2, stellar-core's xdrpp reads true and stellar-xdr reads false), so no answer is
wrong. Its checks pass whenever the SDK answers without crashing, and the matrix keeps each
answer in the check's `actual` and the spread of answers in its `detail`: how many SDKs
answered and how many different answers they gave, counting each SDK once (its `current`
version when the run holds it, otherwise its last version in the run). A crash is still
`diverges-from-network`. Nothing is decided by a vote across SDKs, so a check's result does not
change when an SDK is added to or left out of a run. A matrix or known-failures file from before
this rule can hold `diverges-from-peers`; no run writes it any more.

An SDK version in `versions.toml` can carry a `max_protocol`. A divergence on an entry
generated at a later protocol (`protocol.generated_at`, else `protocol.valid_min`) is then
reported as `unsupported`, with the original verdict in its detail, because an SDK release
cannot know XDR from a later protocol. A `panic` or a testee process that died is never
downgraded this way: crashing on unknown XDR is a robustness defect, not a missing feature.

### The reference testee and the cross-check

Every `expected_json` in the corpus is the XDR-JSON rendering of the stellar-xdr version the
generator links (`generated_with.stellar_xdr` in the manifest), put right where it departs from
SEP-51 v2.0.1 (`crates/shapekit-manifest/src/sep51.rs` cites the lines): a fixed-length opaque
field that is not a typedef becomes a hexadecimal string instead of an array of byte numbers, a
bare `Int64`, `Uint64`, `TimePoint` or `Duration` becomes a decimal string instead of a JSON
number, and the `BinaryFuseFilterType` names lose the letter stellar-xdr keeps in front of a
digit (`8_bit`, not `b8_bit`). `shapekit manifest rerender --corpus <dir>` rewrites the expected
files after a change to these rules. The Rust testee at that version renders with the same
library, so on every other value its `decode_json` and `encode_json` checks agreeing with
`expected_json` prove nothing beyond the library agreeing with itself. The matrix marks that
testee as the reference for `expected_json` (`reference_for` in `testees`) and counts those
checks as `reference` (listed in the cell's `reference_checks`), never as `pass`. A reference
check that disagrees is still a failure: either the library cannot read back its own
rendering, or the value is one of the three above, where the library departs from SEP-51. In the entry summary, a passing cell with reference checks is counted under
`reference`.

`shapekit conformance crosscheck` supplies the independent evidence: it sends every
`decode_json` check with a canonical `expected_json` to the Go and JS testees (or the
`--sdk` ones) at their current versions and reports, per entry, where either renders something else. A disagreement
where two decoders render the same other value points at stellar-xdr's rendering; one where a
single decoder differs while another agrees is that SDK's divergence. The command fails unless
every disagreement is a single-SDK divergence already in that SDK's known-failures list.

## Known failures

`conformance/known-failures/<sdk>.txt` lists one known failure per line:

```
<entry-id> <version or *> <result> <issue link or findings/issues/ draft> <check>[, <check>...]
```

The first four fields are separated by whitespace. `<result>` is the check's result with the
kind of failure where it has one, as the matrix records it in a check's `failure`:

| `<result>` | the check |
|---|---|
| `diverges-from-network` | the SDK answered, and the answer is wrong |
| `diverges-from-network:panic` | the SDK crashed instead of answering (`panic`) |
| `error:bad-request`, `error:internal` | the testee answered with that error class |
| `error:timeout` | no answer within the timeout |
| `error:exited` | the testee process died |
| `error:protocol` | the answer broke this protocol |
| `error:unmappable` | the runner could not turn the corpus entry into a request |

A line matches only its own kind, so a known wrong answer does not hide a crash on the same
check, nor a known bad request a timeout; an `error` always names its kind. No check is ever
`diverges-from-peers` (implementation-defined checks are recorded as passes), so a line cannot
list it. Everything after the issue is the list of failing checks,
separated by commas, each named exactly as the matrix names it (`testnet/decode_json`,
`case 3/strkey_decode`). Every listed check must fail with `<result>`; checks of one entry
that fail with different results go on separate lines. `*` stands for every version; use it
only when every version fails the same checks with the same result. A `#` starts a comment
only at the start of a line or after whitespace, so an issue URL keeps its `#fragment`. A
missing `<sdk>.txt` counts as an empty list.

Each check is judged on its own. A failing check (`diverges-from-network` or `error`) listed
with its result and kind does not fail the run. A failing check
that no line lists fails it, even when other checks of the same entry are listed, so a new
regression next to a known failure is caught. A listed check that now passes, is `reference`,
is unsupported, fails with another result, or is not a check of the entry fails the run too,
until the line is fixed. Every line must name an entry in the manifest and a version in
`versions.toml`.

## Limits the runner applies

- A per-request timeout: `timeout_ms` per SDK in `conformance/versions.toml`, 10000 (10 s)
  when absent; a `--coverage` run raises it to at least 120000. It covers writing the request as well as reading the response, so a testee
  that stops reading stdin cannot block the runner on a request larger than the pipe buffer.
- Response lines of at most 256 MiB (see [Transport](#transport)).
- A memory limit set with `setrlimit` in the child before `exec`: `RLIMIT_AS` (address space)
  for testees whose runtime reserves little virtual memory (Rust, Go, Python, PHP,
  stellar-cli), `RLIMIT_DATA` for runtimes that reserve large `PROT_NONE` regions up front
  (Node's V8 cage, the JVM, the Dart VM, .NET), where an
  address-space cap would kill the process before it touches the data. The resource and size
  are set per SDK in `versions.toml` (`memory = { resource = "as" | "data", mib = <n> }`).
  On macOS neither rlimit can be set: the kernel refuses a limit below the process's current
  virtual size, and an arm64 process maps hundreds of GiB before it starts. There the runner
  reads the physical footprint of the testee and of every process it started every 10 ms, and
  kills a process with `SIGKILL` once its own footprint passes `mib`, for both resources. The
  stderr tail of the failure says which process was killed and at what footprint. A process
  that allocates faster than the sampling can pass `mib` by what it allocates in 10 ms before
  it is killed.

## The KMP testee

`testees/kmp` runs the JVM target of Soneso's Kotlin Multiplatform SDK
(`com.soneso.stellar:stellar-sdk-jvm` on Maven Central) on the same pinned JDK and Maven as the
Java testee. One Kotlin source is built per version by Maven from
`testees/kmp/versions/<version>/pom.xml` into `target/testees/kmp/<version>/`, and the jars Maven
resolved are checked against `testees/kmp/versions/<version>/lib.sha256` (written by
`testees/kmp/lock.sh`) by `scripts/check-jars.sh`: a changed, missing or unlisted jar in `lib/`
fails the build.

The type table `TypesGen.kt` is generated by `testees/kmp/gentypes.py` from the schema, the
manifest and each version's jar: a type maps to the class `<name>Xdr` whose companion has
`decode(XdrReader)` and `fromXdrJsonElement`. Typedefs are Kotlin value classes, whose JVM
methods carry mangled names, so the testee calls them through Kotlin reflection. The 44 schema
types with no class (overlay, survey and SCP-persistence messages, `LedgerCloseMetaBatch`)
answer `unsupported`.

| op | what it calls |
|---|---|
| `decode_encode` | the companion's `decode` and the value's `encode`; bytes left after the value are a rejection |
| `decode_json`, `encode_json` | `toXdrJsonElement` and the companion's `fromXdrJsonElement` |
| `strkey_decode`, `strkey_encode` | `StrKey`'s decoder and encoder for the kind |
| `asset_order` | `Asset.compareTo` |
| `pool_id` | `LiquidityPool(..).getLiquidityPoolId()`, which refuses pairs out of order |
| `map_sort` | `Scv.toMap` |
| `id_parse` | JSON numbers only: `ManageSellOfferOperation` takes a `Long`, `MuxedAccount` a `ULong` |
| `to_native` | `SCValXdr.toNative()`; the value it returns unchanged when it has no native form is a rejection |
| `address_from_scval` | `Address.fromSCVal` |
| `tx_hash` | `AbstractTransaction.fromEnvelopeXdr(..).hash()` |
| `balance_id` | SHA-256 of the `HashIDPreimage` the SDK encodes from its transaction object |
| `contract_id` | `Address.deriveContractId` and `Asset.getContractId` |

`map_validate` is `unsupported`: the SDK orders map keys but has no check of a map it is given.
A coverage build starts the testee with the JaCoCo agent its Maven build copies next to the jar,
as for Java. `decode_limits` is `"ignored"`: `XdrReader` takes no limits. `XdrReader` keeps its position in
a private field, which the testee reads to refuse bytes left after a value; the decode itself is
the SDK's. A `StackOverflowError`, `OutOfMemoryError`, `NullPointerException`,
`ClassCastException` or `IndexOutOfBoundsException` thrown by an SDK call is reported as
`panic` with the SDK frame it left; every other exception an SDK call throws is a rejection,
and an exception from the testee's own code is `internal`.

## The stellar-cli testee

`testees/stellar-cli` checks the `stellar` command of stellar-cli releases. One small Rust
program (`testees/stellar-cli/src/main.rs`) speaks the protocol and runs the release binary
given as its first argument once per request, with the request's bytes on stdin. Each version's
build installs the release's linux x86-64 asset with `testees/stellar-cli/fetch.sh`, which
checks it against `testees/stellar-cli/versions/<version>/asset.sha256` (written by
`fetch.sh --lock` from the digest GitHub records for the asset) and against the version the
binary prints.

| op | what it runs |
|---|---|
| `decode_encode` | `stellar xdr decode --output json`, then `stellar xdr encode --input json`: the command has no bytes-to-bytes form |
| `decode_json`, `encode_json` | `stellar xdr decode --output json` and `stellar xdr encode --input json` |
| `strkey_decode`, `strkey_encode` | `stellar strkey decode` and `stellar strkey encode` |
| `tx_hash` | `stellar tx hash --network-passphrase ..`; the inner transaction of a fee bump is `unsupported`, the command hashes only what it is given |

The other ops answer `unsupported`: the command line has nothing that orders assets, derives
pool, balance or contract ids or converts contract values. A type the binary does not know is
`unsupported`, a non-zero exit with the command's error line is a rejection, and a Rust panic
or a death by signal is a `panic`. `decode_limits` is `"ignored"`: the xdr commands take no
limits. `hello` reports the version and the stellar-xdr crate that `stellar version` prints.
Every run of the binary gets a config directory of the testee's own, removed when the testee
exits, and 25 seconds: a run still going then is killed and answered as `internal`, before the
runner's 30-second request timeout would kill the testee and leave the binary running. A
`signed_payload` raw payload with padding that is not zero, or bytes after it, is
`unsupported`: the command takes the key and the payload, and dropping those bytes would repair
the input. A coverage build runs a `stellar` binary built with `-C instrument-coverage` from the crate
source of the version instead of the release binary, and answers `coverage_snapshot`.

## The version matrix

`shapekit conformance sync-versions` writes the version lists of
[conformance/versions.toml](../conformance/versions.toml) and the per-version testee files under
`testees/*/versions/` from what crates.io, the Go module proxy, npm, PyPI, Maven Central, pub.dev,
Packagist and NuGet publish. For each SDK it
picks:

- for every release line (a major version, or a minor version below 1.0) the newest stable
  release that the registry has not yanked, deprecated or retracted and whose XDR knows the
  protocol in `[sync] min_protocol` or a later one;
- every version that [conformance/reproductions.toml](../conformance/reproductions.toml) or a
  file under [conformance/known-failures](../conformance/known-failures) names.

The registry's latest stable release becomes `current`. A version's `max_protocol` comes from
the stellar/stellar-xdr commit its code was generated from: the `xdr-version` file in the
stellar-xdr crate, `xdr/xdr_commit_generated.txt` in go-stellar-sdk, and the `XDR_COMMIT` line
of js-stellar-sdk's Makefile at the commit npm records as the release's `gitHead`. For
py-stellar-base it is the `XDR_COMMIT` line of the Makefile at the repository tag named like
the release (PyPI records no commit, so the tag is used only when its
`stellar_sdk/__version__.py` declares that version), and the same for java-stellar-sdk, whose
pom names the repository and whose `build.gradle.kts` must declare the version. A
soroban-sdk release pins `soroban-env-host` with `=`, which pins `soroban-env-common`, which
pins `stellar-xdr`; the `xdr-version` file of that `stellar-xdr` release is the commit. For
stellar_flutter_sdk it is the `XDR_COMMIT` line of the Makefile in the archive pub.dev serves,
and for soneso/stellar-php-sdk the same line at the commit Packagist records as the release's
source. stellar-dotnet-sdk names no commit: its XDR is the stellar-xdr commit whose `.x` files
have the same git blob ids as the ones the SDK ships in `StellarDotnetSdk.Xdr/schemes/` at the
commit its NuGet package records. For kmp-stellar-sdk it is the `XDR_COMMIT` line of
`tools/xdrgen-kt/xdr-source.cfg` at the repository tag `v<version>`, taken when the tag's
`gradle.properties` declares that version. For stellar-cli it is the `xdr-version` file of
the stellar-xdr release in the `Cargo.lock` that the crates.io package ships, which the release
binaries are built from.
A commit on stellar-xdr's `curr` branch whose message says it was generated from a release
tag's commit (`Generate from main (4f524bb)`) counts as that tag. The
stellar-xdr release tag at that commit names a protocol. Protocols without a tag of their own
changed no XDR, so `max_protocol` runs up to the protocol before the next tag: `v28.0` covers
protocol 29. A commit that is not a release tag stops the command.

For a newly picked version the command writes the testee files from those of the current
version and resolves them with `cargo generate-lockfile` (rust, soroban), `go mod tidy`,
`npm install --package-lock-only`, `uv pip compile --generate-hashes`,
`testees/java/lock.sh`, `flutter pub get`, `composer update --no-install`,
`dotnet restore --use-lock-file`, `testees/kmp/lock.sh` or
`testees/stellar-cli/fetch.sh --lock`. For Python, run
`python3 -I testees/python/gentypes.py` after the new version's venv is built, so the type
table covers it. It removes the files of versions it no longer picks and
leaves the files of the other versions alone, since their lock files are the pins. The comment
above each version in `versions.toml` says why it was picked.

```sh
shapekit conformance sync-versions            # update versions.toml and testees/*/versions/
shapekit conformance sync-versions --check    # exit 1 and list the difference when either is behind
```

The weekly `upstream-drift` workflow runs the check and opens a tracking issue with the
difference. A new version then still goes through `conformance run --all-versions`,
`conformance reproduce` and triage before it is merged.

Each version in `versions.toml` is an `sdk-version` obligation in the ledger
(`corpus/coverage/obligations.json`). `conformance run --sdk <sdk> --all-versions --obligations`
writes `conformance/sdk-versions/<sdk>.obligations.json`: a version is met when the whole
corpus ran against it and its known-failures list held, with the entries every check of which
passed as witnesses; otherwise it stays open and the evidence says why. A test fails while a
fragment names other versions than `versions.toml`, so after `sync-versions` rerun it for the
sdk and then `shapekit coverage`.

## Datasets outside the corpus

`shapekit conformance run --dataset <dir>` runs a set of `TransactionMeta` that is too large
for the manifest: the converted dump of core's own transaction tests, as
`shapekit coretest convert` writes it (`index.jsonl` with one line per meta core wrote, and
`meta/<two hex digits>/<sha256>.xdr` with each distinct meta once). Every distinct meta goes
through `decode_encode` and passes when the SDK writes the same bytes back. A failure on a meta
whose test sections all name a protocol newer than the version's `max_protocol` is counted
apart, as in a corpus run.

The command writes `dataset-report.json`, `DATASET.md` and
`core-test-sections.obligations.json` (one `core-test-section` obligation per test section)
into `--out`. A meta an SDK drops is shrunk while the SDK still drops it (operations, changes
and events removed one at a time) and written to the candidate lane under a family that takes
`core-test-dump` entries. `conformance/dataset/` holds the result for the current version of
each SDK.

## Writing a testee

`shapekit init testee --lang go|ts|python` writes a skeleton into `testee-<name>/` (or
`--out <dir>`; `<name>` is `--name`, else the language) for the Stellar SDK of that language: `@stellar/stellar-sdk` (ts),
`github.com/stellar/go-stellar-sdk` (go) or `stellar-sdk` (python). For another package, give
`--package`, `--sdk-version` and `--issues-repo`; `--name` sets its key in the versions file.
The skeleton answers `hello` with the SDK version read from the installed package or the build
info, lists no ops yet, answers every op this page defines with `unsupported` until you add it
to the skeleton's ops table, skips empty lines, and answers malformed lines with `bad-request`. Its
`versions.toml` holds the build steps that install the SDK, so a first run is:

```sh
shapekit init testee --lang python
shapekit check sdk --sdk python --versions-file testee-python/versions.toml \
  --known-failures testee-python/known-failures
```

Every check of that run is `unsupported` until ops are added. Every line each skeleton answers
follows `schema/testee-protocol.schema.json` (`crates/shapekit-cli/tests/init_skeletons.rs`).

1. Read requests line by line, answer each with exactly one line, flush after every line.
2. Answer `hello` with versions read from the build (Go `debug.ReadBuildInfo`, the installed
   package's `package.json`, Cargo's resolved dependency, Python's `importlib.metadata`, the file name of the jar Java loaded the SDK from) and
   never from a constant.
3. Implement each op against the SDK's public API only. Where the SDK has no API, answer
   `unsupported` with a reason. Never emulate an SDK feature inside the testee.
4. Catch every exception or panic around SDK calls and classify it.
5. Add the testee's build and run command per SDK version to
   [conformance/versions.toml](../conformance/versions.toml). For the SDKs here,
   `shapekit conformance sync-versions` does this (see "The version matrix").

Working testees are in `testees/rust`, `testees/go`, `testees/js`, `testees/soroban`,
`testees/python`, `testees/java`, `testees/kmp`, `testees/dart`, `testees/php`,
`testees/dotnet` and `testees/stellar-cli`.

## The soroban-sdk testee

`testees/soroban` wraps `soroban-sdk`, the SDK contracts are written with, built with its
`testutils` feature so that the Soroban host runs inside the testee process. A contract reads
and writes XDR only as contract values: `FromXdr` and `ToXdr` hand `ScVal` bytes to the host,
and every contract type converts to and from `ScVal` through the host. The testee runs that
path and nothing else:

| op | what it calls |
|---|---|
| `decode_encode` | `Val::from_xdr`, then `ToXdr::to_xdr`, for type `ScVal`. Other types are `unsupported`: the crate reads them only through its re-export of `stellar-xdr`, which the Rust testee tests |
| `map_sort` | `Map::set` for each entry on an empty `Map<Val, Val>` |
| `map_validate` | whether the host converts the `ScVal` map into a `Map` (`Val::try_from_val`) |
| `to_native` | `Val::from_xdr`, then the first contract type that converts the value: integers, `Bytes`, `String`, `Symbol`, `Address`, `MuxedAddress`, `Vec<Val>`, `Map<Val, Val>` |
| `address_from_scval` | `MuxedAddress::try_from_val` on the `ScVal`, then `MuxedAddress::to_strkey` |
| `strkey_decode`, `strkey_encode` | `Address::from_string_bytes` and `Address::to_string` for G and C, `MuxedAddress::from_string_bytes` and `MuxedAddress::to_strkey` for M. Other kinds are `unsupported` |
| `contract_id` | `Deployer::with_address` and `Deployer::with_stellar_asset`, then `deployed_address`, on a ledger whose network ID is set to the request's |

Each request runs in a new `Env` with the budget lifted (`reset_unlimited`), so a value is
never refused only for lack of budget. `decode_limits` is `"ignored"`: the host reads under its
own XDR limits (depth 500) and converts under its own depth limit (100). A host function that
fails escalates its `HostError` into a panic whose message starts with `HostError:`; that is the
SDK's documented error path in native tests, so the testee reports it as `rejected`. Every other
panic is reported as `panic`.

## The Python testee

`testees/python` runs py-stellar-base (`stellar-sdk` on PyPI) under CPython 3.15. Each version
is installed into its own venv under `target/testees/python/<version>/` from the hash-locked
`testees/python/versions/<version>/requirements.txt`, so the SDK's own dependencies (the
`xdrlib3` codec above all) are pinned too. The build needs `uv` and CPython 3.15.2 on `PATH`
(both pinned in [pins.toml](../pins.toml)).

The released versions encode strkeys with `StrKey`, one encoder and decoder per kind, and XDR
with classes generated on `xdrlib3`. The SDK's default branch replaces `StrKey` with the classes
of the `stellar-strkey` package, which `stellar_sdk.strkey` re-exports, and the generated
classes with the `stellar-xdr` package, which `stellar_sdk.xdr` re-exports. The testee picks
the API by what the installed SDK has (`shapekit_testee/sdkapi.py`), not by its version, since
a build of the default branch reports the version of the last release. `hello` lists the
packages the SDK uses for both in `components`.

- `decode_limits` is `"ignored"`: the SDK has no length limit at all, so values decode under
  the SDK's default depth. Up to 17.x `from_xdr_bytes` takes no limits and the depth is 512;
  `stellar-xdr`'s `from_xdr_bytes` takes `max_depth` only, and the depth is 500.
- `decode_json` and `encode_json` use `to_json` and `from_json` (JSON text) up to 17.x, and
  `stellar-xdr`'s `to_json_value` and `from_json_value` (JSON values) on the default branch.
- On the default branch `strkey_decode` calls `stellar-strkey`'s generic decoder `parse`, and
  `Ed25519PrivateKey.from_string` for a seed, which `parse` refuses by design; the payload is
  the decoded key's `to_bytes()`. `strkey_encode` builds the kind's class with `from_bytes` and
  writes it with `str`, or `reveal` for a seed.
- `id_parse` answers `unsupported` for IDs given as JSON strings: `MuxedAccount` and
  `ManageSellOffer` take integers, and the SDK reads no textual ID. A JSON number reaches the
  SDK as Python's `json` module gives it to a caller, an `int` for an integer literal and a
  `float` otherwise. On the default branch the `TypeError` that `stellar-xdr` and
  `stellar-strkey` raise for a `float` is their documented refusal of an argument of the wrong
  type, and counts as `rejected`.
- `map_validate` answers `unsupported`: the SDK has no ScMap validation, and the key
  comparison `scval.to_map` sorts with is private.
- `address_from_scval` calls `scval.from_address`, which refuses an `ScVal` of another arm with
  `ValueError`, and answers the address's strkey.
- `balance_id` counts the `IndexError` and `TypeError` that
  `Transaction.get_claimable_balance_id` documents as its refusals as `rejected`. Elsewhere
  `ValueError` (which `stellar-xdr`'s `XdrError` and `stellar-strkey`'s `StrkeyError` extend),
  `EOFError`, `OverflowError`, `struct.error` and, up to 17.x, the `xdrlib3` errors are
  rejections, and any other exception escaping the SDK (`RecursionError`, `TypeError`,
  `AttributeError` and the rest) is a `panic`.
- The type table `shapekit_testee/types_gen.py` is generated by `testees/python/gentypes.py` from
  the schema, the manifest and each version's `stellar_sdk/xdr` source, or the `stellar_xdr`
  package when `stellar_sdk/xdr.py` re-exports it, and keyed by the stellar-xdr crate's type
  names; a test of the conformance crate checks the keys against the crate. `stellar-xdr` has no
  class for the typedefs of integers (`Int32`, `Int64`, `Uint32`, `Uint64`) and for
  `SponsorshipDescriptor`, an optional: they are type aliases. The testee answers `unsupported`
  for them on the default branch.
- Coverage counts lines of `stellar_sdk` up to 17.x. On the default branch it also counts
  `stellar_xdr` and `stellar_strkey`, where the SDK's XDR and strkey code lives. Coverage starts
  before the SDK is imported, so the testee picks the packages from the dependencies the
  installed `stellar-sdk` declares, and refuses to start when they disagree with the API the
  import shows.

## The Java testee

`testees/java` runs java-stellar-sdk (`network.lightsail:stellar-sdk` on Maven Central) on JDK
27. Each version is built by Maven from `testees/java/versions/<version>/pom.xml` into
`target/testees/java/<version>/`, and the jars Maven resolved are checked against
`testees/java/versions/<version>/lib.sha256` by `scripts/check-jars.sh`, so the SDK's own
dependencies (Gson above all) are pinned too. A changed, missing or unlisted jar in `lib/` fails
the build. The build runs Maven through `scripts/with-java.sh`, which fetches the pinned JDK and
Maven with `scripts/java-toolchain.sh` and sets `JAVA_HOME` to that JDK, so a `JAVA_HOME` naming
another JDK does not change the build. The testee itself runs on the `java` on `PATH`; put the
output of `scripts/java-toolchain.sh` in front of `PATH` for it.

- `decode_limits` is `"honoured"`: the testee decodes with the generated `decode(stream,
  maxDepth)` on an `XdrDataInputStream` whose `setMaxInputLen` is the request's `len_limit`, and
  refuses unread bytes itself, since `fromXdrByteArray` does not.
- `to_native` and `map_validate` answer `unsupported`: the SDK has only typed per-arm
  converters (`Scv.fromX`), and its ScVal key comparator is not public.
- `address_from_scval` calls `Address.fromSCVal` and answers the address's `toString`, its
  strkey.
- `id_parse` reads the input as Horizon's JSON through the SDK's own Gson, into
  `OfferResponse.id` and `TransactionResponse.accountMuxedId`, and reports the muxed ID that
  `MuxedAccount` encodes.
- `IOException`, `IllegalArgumentException`, `ArithmeticException`, the SDK's `SdkException`
  and Gson's `JsonParseException` are rejections; any other throwable from the SDK
  (`StackOverflowError`, `NullPointerException`, `ClassCastException` and the rest) is a
  `panic`.
- The type table `TypesGen.java` is generated by `testees/java/gentypes.py` from the schema, the
  manifest and each version's jar, and keyed by the stellar-xdr crate's type names; a test of
  the conformance crate checks the keys against the crate.

## The Dart testee

`testees/dart` wraps `stellar_flutter_sdk`. The package declares a dependency on Flutter, so
its dependencies resolve only with the Flutter SDK (`flutter pub get`); the testee itself is a
command-line program compiled with `dart compile exe` and needs no Flutter at run time. The
package's public entry point also exports two smart account files that import Flutter's platform
channels, which the Dart VM cannot load, so the testee imports the libraries that entry point
exports one by one, without those two. Every class it calls is part of the exported API.

The build writes the type table, `lib/tables_gen.dart`, with `tool/gentypes.dart` from
`schema/xdr/v30.0/xdr.json` and the installed SDK: a type name matches the class `Xdr<name>`
without regard to case, and the generator lists the classes the SDK names another way. Types
without a class answer `unsupported` with the reason.

| op | what it calls |
|---|---|
| `decode_encode` | the class's static `decode` and `encode`; bytes left after the value are a rejection |
| `decode_json`, `encode_json` | `toXdrJsonValue` and the static `fromXdrJsonValue` |
| `strkey_decode`, `strkey_encode` | `StrKey`'s decoder and encoder for the kind the first character names |
| `asset_order` | the order check in `AssetTypePoolShare`'s constructor, tried both ways |
| `pool_id` | SHA-256 of the `LiquidityPoolParameters` the SDK encodes, for pairs that check accepts in one order only |
| `map_sort` | `sortedScMap` |
| `id_parse` | offer ids: `ManageSellOfferOperationBuilder.setOfferId`, text only. Muxed ids: `unsupported`, `MuxedAccount` takes a `BigInt` |
| `to_native` | `XdrSCVal.toNative`; the value it returns unchanged when it has no native form is a rejection |
| `address_from_scval` | `Address.fromXdrSCVal`, written as a strkey with `StrKey` |
| `tx_hash` | `AbstractTransaction.fromEnvelopeXdr(..).hash(Network(..))` |
| `balance_id`, `contract_id` | SHA-256 of the `HashIDPreimage` the SDK encodes; `Address.deriveContractId` for address preimages |

`map_validate` is `unsupported`: the SDK orders map keys but has no check of a map it is
given. `decode_limits` is `"ignored"`. A stack overflow, an `OutOfMemoryError`, a `TypeError` or
a `NoSuchMethodError` thrown by an SDK call is reported as `panic`; every other exception an SDK
call throws is a rejection, and an exception from the testee's own code is `internal`.

## The PHP testee

`testees/php` wraps `soneso/stellar-php-sdk`, installed with Composer from the lock file under
`testees/php/versions/<version>/`. It needs PHP with the `bcmath`, `gmp`, `mbstring` and
`sodium` extensions the package requires. The build writes the type table,
`src/TablesGen.php`, with `tools/gentypes.php` from `schema/xdr/v30.0/xdr.json` and the
installed SDK, the same way the Dart testee does; the generator also maps the SDK's
`<Name>Operation` classes to the schema's `<Name>Op` types.

The ops call the same parts of the SDK as the Dart testee, with these differences:

- `decode_json` and `encode_json` call `toJson` and the static `fromJson`. The testee reads and
  writes the JSON as objects, so an empty JSON object stays an object.
- `strkey_decode` passes `text_base64` bytes to the decoders as they are: PHP strings are bytes.
- `id_parse` takes JSON numbers only, as PHP integers: `ManageSellOfferOperationBuilder::setOfferId`
  and `MuxedAccount` take an `int`. Text and numbers outside the integer range are `unsupported`.
- `hello` reports the version Composer installed (`Composer\InstalledVersions`).

PHP writes warnings to stdout by default; the testee sends them to stderr, so they reach the
runner's report without breaking the protocol. Every engine `\Error` from the SDK (a
`TypeError`, a `ValueError`, a typed property read before it was set, an undefined method) is
reported as `panic`; every `\Exception` is a rejection. PHP keeps the frames of PHP calls on
the heap, so a deep recursion grows memory rather than the stack: the testee sets PHP's
`memory_limit` to 3072 MiB, below the runner's 4096 MiB limit, and answers a request that runs
out of it as `panic` with PHP's fatal error message before the process exits.

## The .NET testee

`testees/dotnet` wraps `stellar-dotnet-sdk`, one source (`Program.cs`) built per version from
the project file and NuGet lock file under `testees/dotnet/versions/<version>/`. The SDK names
its XDR classes as the `.x` files name the types (`SCVal`, `AccountID`), so the testee finds a
request's type by reflection over `StellarDotnetSdk.Xdr`, without regard to case, and calls the
class's static `Decode` and `Encode`. The classes of nested unions and structs sit inside the
class of their outer type (`TransactionResult.TransactionResultResult`) under the name
stellar-xdr gives them, and are found the same way; a name two classes share answers
`unsupported`.

| op | what it calls |
|---|---|
| `decode_encode` | the class's `Decode` and `Encode`; bytes left after the value are a rejection |
| `strkey_decode`, `strkey_encode` | `StrKey`'s decoder and encoder for the kind; `MuxedAccountMed25519.FromMuxedAccountId` for M |
| `asset_order` | `Asset.CompareTo` |
| `pool_id` | `new LiquidityPoolId(..)` |
| `id_parse` | JSON numbers only: `ManageSellOfferOperation` takes a `long`, `MuxedAccountMed25519` a `ulong` |
| `to_native` | `SCVal.FromXdr`, the SDK's class tree of contract values |
| `address_from_scval` | `ScAddress.FromSCValXdr` |
| `tx_hash` | `Transaction.FromEnvelopeXdr` or `FeeBumpTransaction.FromEnvelopeXdr`, then `Hash(network)` |
| `balance_id`, `contract_id` | SHA-256 of the `HashIDPreimage` the SDK encodes |

The SDK has no XDR-JSON and no map ordering, so `decode_json`, `encode_json`, `map_sort` and
`map_validate` are `unsupported`, and an entry with an expected JSON counts as `unsupported`
even when its bytes pass. A `NullReferenceException`, `InvalidCastException`,
`IndexOutOfRangeException` or `InsufficientExecutionStackException` thrown by an SDK call is
reported as `panic`, with the SDK frame it left; every other exception an SDK call throws is a
rejection, and an exception from the testee's own code is `internal`. Version 16.0.0 has no W
strkey decoder, so a W string is a rejection there, and its encoder is `unsupported`.

