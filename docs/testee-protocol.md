# Testee protocol

A testee is a small program that wraps one Stellar SDK at one exact version and answers
requests from the conformance runner (`shapekit conformance run`). The runner sends corpus
data through the SDK's own decoder, encoder, strkey codec, ordering functions and native
projection, and compares what comes back with the network's answer (the corpus manifest) and
with the other SDKs.

Protocol version: **2**. Version 2 adds the ops `tx_hash`, `balance_id`, `contract_id`, `coverage_snapshot` and
`coverage_new`
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
| `sdk` | string | the package under test: `stellar-xdr` (Rust), `github.com/stellar/go-stellar-sdk` (Go), `@stellar/stellar-sdk` (JS) |
| `sdk_version` | string | the exact version that is linked or loaded, read from the build or the installed package, never typed in. The runner refuses a testee whose `sdk` or `sdk_version` differs from the `package` and `version` in the versions file |
| `components` | object, string to string | other packages whose behaviour the ops depend on and their exact versions, for example `{"stellar-strkey": "1.0.0"}`. Can be empty |
| `ops` | array of strings | the ops this build implements, excluding `hello`. The runner refuses a `hello` that lists an op this page does not define. It does not send unlisted ops and records their checks as `unsupported` |
| `decode_limits` | `"honoured"` or `"ignored"` | whether `depth_limit` and `len_limit` reach the SDK's decoder (see `decode_encode`). `"honoured"` covers the XDR inputs of every op. `asset_order`, `pool_id`, `map_sort` and `map_validate` requests carry no limits, so a testee that reports `"honoured"` decodes their inputs at depth 1000 with the input's own length as the length limit, or with `depth_limit` and `len_limit` when a request carries them |

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
comparison (`Asset.compare` in JS, `xdr.Asset.LessThan` in Go, `Ord` in Rust).

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
`ScMap::sorted_from` in Rust).

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
and `Operation.manageSellOffer`; in Rust the only textual path is the XDR-JSON reader of the
`id` and `offer_id` fields; in Go the Horizon response types, `horizon.Offer.ID` and
`horizon.Transaction.AccountMuxedID`, which read IDs from JSON strings only), in the JSON type
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

### tx_hash

Request: `xdr` (base64 XDR `TransactionEnvelope`), `network_passphrase` (string),
`depth_limit`, `len_limit`, and optionally `inner` (boolean).

The SDK builds its transaction object from the envelope and the passphrase and returns the
transaction hash: the hash of the outer transaction, for a fee bump the fee bump's own hash.
With `inner: true` the testee returns the hash of a fee bump's inner transaction; `inner` on
an envelope that is not a fee bump is a `bad-request`. An SDK whose hash function takes a
network id instead of a passphrase (Rust `stellar-xdr`) gets the SHA-256 of the passphrase,
which is the network id by definition.

Result: `hash` (lowercase hex, 64 characters).

### balance_id

Request: `xdr` (base64 XDR `TransactionEnvelope`), `network_passphrase`, `op_index` (integer),
`depth_limit`, `len_limit`.

The ID of the claimable balance the `CreateClaimableBalance` operation at `op_index` creates.
For a fee bump the operation belongs to the inner transaction. An operation of another type,
or an index past the last operation, is `rejected`. An SDK without a balance ID function (Rust
`stellar-xdr`) computes SHA-256 of the `HashIdPreimage` (`ENVELOPE_TYPE_OP_ID`) written by the
SDK's own encoder.

Result: `balance_id` (lowercase hex of the `ClaimableBalanceId` XDR, 72 characters: the 4-byte
type, then the 32-byte hash).

### contract_id

Request: `preimage` (base64 XDR `ContractIdPreimage`), `network_passphrase`, `depth_limit`,
`len_limit`.

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

Result: `tool` (what measured it), `ranges` (sorted strings, each range once), and for the
JavaScript testee `uncovered` (the ranges V8 has reported with a count of 0 that have not run
since the process started, in the same form). V8 resets its counters each time it is read, so
the JavaScript testee folds every read into what it has seen before; every snapshot covers the
whole process, as for Rust and Go. V8
lists a nested block only while its count differs from the count of the block around it, so a
block can be missing from both lists: it then ran exactly as often as its parent. To decide
whether a range ran, look it up in `ranges`, then in `uncovered`, and otherwise take the
answer of the smallest listed range that contains it. Rust regions and Go blocks are a fixed
set, so those testees send no `uncovered`. Range strings are
only compared within one SDK version and one tool: the union over runs, or the ranges one
run adds, is what is meaningful. Counters live in the process, so what a testee covered before
the runner restarted it is not in a later snapshot.

### coverage_new

Request: no members besides `op`.

The cheap form of `coverage_snapshot`, for a fuzzer that asks after every input whether the
input reached code nothing before it reached. A testee lists it exactly when it lists
`coverage_snapshot`. Result: `new`, how many coverage units have run for the first time since
the previous `coverage_new` request (since the process started, for the first one), and
`total`, how many have run since the process started. The units are the tool's own, read in
the process with no files and no external tool:

| testee | a unit is |
|---|---|
| Rust | an llvm counter of the testee binary, `stellar-xdr` and the crates around it (`__llvm_profile_begin_counters` to `__llvm_profile_end_counters`) |
| Go | a statement-block counter of an instrumented package, read from `runtime/coverage.WriteCounters` |
| JavaScript | a range of the SDK package, folded exactly as `coverage_snapshot` folds it (the two ops share one fold, so neither loses what the other read) |

The numbers are only compared within one process: they say whether coverage grew, not which
code ran. Ask `coverage_snapshot` for the ranges themselves.

`shapekit conformance run --coverage` builds and starts these variants (the Rust and Go builds
go to `target/testees-cov/`), runs the selection, asks each testee for one snapshot at the end
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
| `xdr`, `decode = implementation-defined` | `decode_encode`, `decode_json` | every SDK that answers gives the same outcome (same status, same bytes, same JSON) |
| `ordering`, `asset_order` cases | `asset_order` | `expected` `less`, `equal`, `greater` is the order core uses (`Asset` comparison by type, code, then issuer key bytes) |
| `ordering`, `pool_id` cases | `pool_id` | the pool ID equals `expected.pool_id`, or the answer is `rejected` when `expected.reject` is set |
| `ordering`, `map_sort` cases | `map_sort` | the result equals `expected.map` byte for byte, or is `rejected` when the case expects a rejection |
| `ordering`, `map_validate` cases | `map_validate` | `valid` is true for `expected = accept`, false for `reject` |
| `strkey` | `strkey_decode`, and `strkey_encode` for valid strings | decode gives the expected kind and raw payload, or `rejected` for invalid strings; encode of that kind and payload gives back the text |
| `textual-id` | `id_parse` | the ID equals `expected.id`, or `rejected` when the case expects a rejection. A case whose `expected` is `{"either": [{"id": ...}, {"reject": ...}]}` passes with exactly that ID or with `rejected`; any other ID diverges. This form is used where reading the exact ID and refusing are both correct: decimal text with leading zeros, integral JSON numbers written with a zero fraction, JSON numbers above 2^53-1, and negative offer IDs, which the XDR `int64` holds and stellar-core refuses only when it checks the operation. Such a case is judged against its own expectation even when the entry is `implementation-defined` |
| `native-projection` | `to_native` | `rejected`, or a native map that keeps every entry of the input map |

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

Implementation-defined checks are decided by one vote per SDK: the SDK's `current` version in
`versions.toml` votes when the run holds it, otherwise its last version in the run
(versions.toml order, or the order of `--version`). An SDK whose voting version does not answer
casts no vote. The most common vote is the majority; every testee, voting or not, whose answer
differs from it is `diverges-from-peers`, and with a tie no answer is the majority. With fewer
than two votes, as in any run of a single SDK however many versions it holds, there is no peer
to compare with and the check counts as `unsupported`, not as a pass.

An SDK version in `versions.toml` can carry a `max_protocol`. A divergence on an entry
generated at a later protocol (`protocol.generated_at`, else `protocol.valid_min`) is then
reported as `unsupported`, with the original verdict in its detail, because an SDK release
cannot know XDR from a later protocol. A `panic` or a testee process that died is never
downgraded this way: crashing on unknown XDR is a robustness defect, not a missing feature.

### The reference testee and the cross-check

Every `expected_json` in the corpus is the XDR-JSON rendering of the stellar-xdr version the
generator links (`generated_with.stellar_xdr` in the manifest). The Rust testee at that version
renders with the same library, so its `decode_json` and `encode_json` checks agreeing with
`expected_json` prove nothing beyond the library agreeing with itself. The matrix marks that
testee as the reference for `expected_json` (`reference_for` in `testees`) and counts those
checks as `reference` (listed in the cell's `reference_checks`), never as `pass`. A reference
check that disagrees is still a failure: it means the library cannot read back its own
rendering. In the entry summary, a passing cell with reference checks is counted under
`reference`.

`shapekit conformance crosscheck` supplies the independent evidence: it sends every
`decode_json` check with a canonical `expected_json` to the Go and JS testees at their
current versions and reports, per entry, where either renders something else. A disagreement
where two decoders render the same other value points at stellar-xdr's rendering; one where a
single decoder differs while another agrees is that SDK's divergence. The command fails unless
every disagreement is a single-SDK divergence already in that SDK's known-failures list.

## Known failures

`conformance/known-failures/<sdk>.txt` lists one known failure per line:

```
<entry-id> <version or *> <result> <issue link or findings/issues/ draft> <check>[, <check>...]
```

The first four fields are separated by whitespace. `<result>` is `diverges-from-network`,
`diverges-from-peers` or `error`. Everything after the issue is the list of failing checks,
separated by commas, each named exactly as the matrix names it (`testnet/decode_json`,
`case 3/strkey_decode`). Every listed check must fail with `<result>`; checks of one entry
that fail with different results go on separate lines. `*` stands for every version; use it
only when every version fails the same checks with the same result. A `#` starts a comment
only at the start of a line or after whitespace, so an issue URL keeps its `#fragment`. A
missing `<sdk>.txt` counts as an empty list.

Each check is judged on its own. A failing check (`diverges-from-network`,
`diverges-from-peers`, `error`) listed with its result does not fail the run. A failing check
that no line lists fails it, even when other checks of the same entry are listed, so a new
regression next to a known failure is caught. A listed check that now passes, is `reference`,
is unsupported, fails with another result, or is not a check of the entry fails the run too,
until the line is fixed. Every line must name an entry in the manifest and a version in
`versions.toml`. A `diverges-from-peers` result needs more than one SDK in the run, so a
single-SDK run accepts a `diverges-from-peers` line whose check passes.

## Limits the runner applies

- A per-request timeout: `timeout_ms` per SDK in `conformance/versions.toml`, 10000 (10 s)
  when absent. It covers writing the request as well as reading the response, so a testee
  that stops reading stdin cannot block the runner on a request larger than the pipe buffer.
- Response lines of at most 256 MiB (see [Transport](#transport)).
- A memory limit set with `setrlimit` in the child before `exec`: `RLIMIT_AS` (address space)
  for testees whose runtime reserves little virtual memory (Rust, Go), `RLIMIT_DATA` for
  runtimes that reserve large `PROT_NONE` regions up front (Node's V8 cage), where an
  address-space cap would kill the process before it touches the data. The resource and size
  are set per SDK in `versions.toml` (`memory = { resource = "as" | "data", mib = <n> }`).

## The version matrix

`shapekit conformance sync-versions` writes the version lists of
[conformance/versions.toml](../conformance/versions.toml) and the per-version testee files under
`testees/*/versions/` from what crates.io, the Go module proxy and npm publish. For each SDK it
picks:

- for every release line (a major version, or a minor version below 1.0) the newest stable
  release that the registry has not yanked, deprecated or retracted and whose XDR knows the
  protocol in `[sync] min_protocol` or a later one;
- every version that [conformance/reproductions.toml](../conformance/reproductions.toml) or a
  file under [conformance/known-failures](../conformance/known-failures) names.

The registry's latest stable release becomes `current`. A version's `max_protocol` comes from
the stellar/stellar-xdr commit its code was generated from: the `xdr-version` file in the
stellar-xdr crate, `xdr/xdr_commit_generated.txt` in go-stellar-sdk, and the `XDR_COMMIT` line
of js-stellar-sdk's Makefile at the commit npm records as the release's `gitHead`. The
stellar-xdr release tag at that commit names a protocol. Protocols without a tag of their own
changed no XDR, so `max_protocol` runs up to the protocol before the next tag: `v28.0` covers
protocol 29. A commit that is not a release tag stops the command.

For a newly picked version the command writes the testee files from those of the current
version and resolves them with `cargo generate-lockfile`, `go mod tidy` or
`npm install --package-lock-only`. It removes the files of versions it no longer picks and
leaves the files of the other versions alone, since their lock files are the pins. The comment
above each version in `versions.toml` says why it was picked.

```sh
shapekit conformance sync-versions            # update versions.toml and testees/*/versions/
shapekit conformance sync-versions --check    # exit 1 and list the difference when either is behind
```

The weekly `upstream-drift` workflow runs the check and opens a tracking issue with the
difference. A new version then still goes through `conformance run --all-versions`,
`conformance reproduce` and triage before it is merged.

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

1. Read requests line by line, answer each with exactly one line, flush after every line.
2. Answer `hello` with versions read from the build (Go `debug.ReadBuildInfo`, the installed
   package's `package.json`, Cargo's resolved dependency) and never from a constant.
3. Implement each op against the SDK's public API only. Where the SDK has no API, answer
   `unsupported` with a reason. Never emulate an SDK feature inside the testee.
4. Catch every exception or panic around SDK calls and classify it.
5. Add the testee's build and run command per SDK version to
   [conformance/versions.toml](../conformance/versions.toml). For the three SDKs here,
   `shapekit conformance sync-versions` does this (see "The version matrix").

Working testees for all three SDKs are in `testees/rust`, `testees/go` and `testees/js`.
