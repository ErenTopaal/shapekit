# Testee protocol

A testee is a small program that wraps one Stellar SDK at one exact version and answers
requests from the conformance runner (`shapekit conformance run`). The runner sends corpus
data through the SDK's own decoder, encoder, strkey codec, ordering functions and native
projection, and compares what comes back with the network's answer (the corpus manifest) and
with the other SDKs.

Protocol version: **1**. The machine-readable form is `schema/testee-protocol.schema.json`.
Anything this page and the schema disagree on is a bug in one of them.

## Transport

- The runner starts the testee as a child process and talks to it over stdin and stdout.
- Every message is one JSON object encoded as UTF-8 on one line, terminated by `\n`. No
  pretty printing, no blank lines, no byte order mark.
- The runner sends one request and waits for exactly one response before it sends the next.
  Requests and responses are matched by `id` as well; a response whose `id` differs from the
  outstanding request is a protocol violation.
- stdout carries responses only. Logging, warnings and stack traces go to stderr, which the
  runner keeps and shows next to any `error` result.
- A line can be large (a full `LedgerCloseMeta` in base64). Testees must accept request lines
  of at least 64 MiB.
- When stdin reaches end of file the testee exits with status 0.
- The runner enforces a per-request timeout and an address-space or data-segment limit on the
  testee process (see "Limits the runner applies"). A testee that times out, exits or is
  killed is restarted, said `hello` to again, and the request that was in flight is recorded
  as `error`.

## Messages

### Request

```json
{"id": "17", "op": "decode_encode", "type": "TransactionEnvelope", "xdr": "AAAAAg...", "depth_limit": 1000, "len_limit": 4096}
```

- `id`: string chosen by the runner, unique within one process lifetime. Echo it back. A
  line that is not a JSON object with a string `id` is answered with `"id": ""` and a
  `bad-request` error.
- `op`: one of the operation names below.
- The other members depend on `op`. Unknown members must be ignored, so that later
  protocol versions can add optional members.

### Response

Exactly one of three forms:

```json
{"id": "17", "status": "ok", "result": {"xdr": "AAAAAg..."}}
{"id": "17", "status": "error", "error": {"class": "rejected", "message": "xdr value invalid"}}
{"id": "17", "status": "unsupported", "reason": "go-stellar-sdk has no scval map sort"}
```

- `status: "ok"`: the SDK produced a value. `result` holds the op's result members.
- `status: "error"`: the SDK, or the testee, did not produce a value. `error.class` says why,
  `error.message` is the SDK's own message (free text, never compared).
- `status: "unsupported"`: the SDK version under test has no API for this operation, or for
  this particular input of it (for example an XDR type the SDK does not know). `reason` is
  required and should name what is missing. The matrix shows it as not applicable and it is
  never counted as a pass.

### Error classes

| class | meaning | how the runner counts it |
|---|---|---|
| `rejected` | the SDK refused the input through its normal error path: a returned error, a thrown exception, a `Result::Err` | a rejection. Correct for `must-reject` inputs |
| `panic` | the SDK crashed instead of rejecting: a Rust panic, a Go panic, a JS `RangeError: Maximum call stack size exceeded` or any error the testee can tell is not the SDK's documented rejection | always a divergence from the network: core never crashes on these inputs and an SDK should reject, not crash |
| `bad-request` | the request does not follow this protocol (missing member, bad base64, unknown op name) | `error`: a runner or testee bug, never an SDK finding |
| `internal` | the testee itself failed for a reason unrelated to the input | `error` |

A testee that cannot tell `rejected` from `panic` (because the SDK uses one exception type
for both) reports `rejected`. Catch panics around every SDK call so one input cannot take the
process down; a process that dies anyway is recorded as `error` for that request.

## Value encodings

These are fixed for every op so that the runner compares like with like.

| value | encoding |
|---|---|
| XDR bytes | standard base64 (RFC 4648 section 4) with `=` padding, no line breaks |
| raw bytes (strkey payloads) | standard base64 with padding |
| XDR type names | the name in the `.x` definitions at stellar-xdr `v30.0`, as `stellar-xdr` spells it: `TransactionEnvelope`, `LedgerCloseMeta`, `ScVal`, `ScSpecFunctionV0` |
| decoded values as JSON | SEP-51 XDR-JSON (Draft, 2.0.1) of the value itself, not wrapped in its type name. Compared as JSON values: member order is irrelevant, everything else must be equal, including 64-bit integers written as strings |
| order | integer `-1`, `0` or `1`: the sign of `compare(a, b)` |
| 64-bit IDs | decimal string with no sign for unsigned values, a leading `-` only for negative values, no leading zeros, no whitespace (`"0"`, `"16"`, `"-1"`) |
| liquidity pool ID | lowercase hex of the 32-byte pool hash, 64 characters |
| strkey kind | one of the names in the strkey table below |

## hello

Sent first, after every (re)start.

Request: `{"id": "0", "op": "hello"}`

Result members:

| member | type | meaning |
|---|---|---|
| `protocol` | integer | must be `1` |
| `sdk` | string | the package under test: `stellar-xdr` (Rust), `github.com/stellar/go-stellar-sdk` (Go), `@stellar/stellar-sdk` (JS) |
| `sdk_version` | string | the exact version that is linked or loaded, read from the build or the installed package, never typed in. The runner refuses a testee whose version differs from `conformance/versions.toml` |
| `components` | object, string to string | other packages whose behaviour the ops depend on and their exact versions, for example `{"stellar-strkey": "1.0.0"}`. May be empty |
| `ops` | array of strings | the ops this build implements, excluding `hello`. The runner does not send other ops and records them as `unsupported` |
| `decode_limits` | `"honoured"` or `"ignored"` | whether `depth_limit` and `len_limit` reach the SDK's decoder (see `decode_encode`) |

## Operations

### decode_encode

Decode bytes as one value of a type and encode the value again.

Request: `type` (string), `xdr` (base64), `depth_limit` (integer), `len_limit` (integer).

- Decode must consume the whole input. Trailing bytes are a rejection.
- `depth_limit` and `len_limit` are the runner's decode limits (`Limits { depth, len }` in
  rs-stellar-xdr). A testee passes them to the SDK when the SDK's API accepts limits and
  reports `decode_limits: "honoured"`. Otherwise it decodes with the SDK's defaults and reports
  `"ignored"`; the matrix then notes that depth and length entries ran under SDK defaults.
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

This op is the one addition to the list in the plan. It is needed because the
rs-stellar-xdr advisory GHSA-3xhm-p452-9wjh is a JSON-to-XDR flaw: 28.0.0 accepts a JSON
`ScSpecFunctionV0` with two `outputs` (the XDR allows at most one) and encodes bytes that no
decoder accepts. No decode-side op can see it. SDKs without a SEP-51 reader answer
`unsupported`.

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

The testee calls the SDK's sorted-map constructor (`nativeToScVal`/`scvSortedMap` in JS,
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
`id` and `offer_id` fields), in the JSON type it arrived as. A testee must read numbers without going through a binary floating
point type unless that is what the SDK itself does with them. If the SDK has no API for the
input's JSON type, answer `unsupported`.

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

## How the runner uses the ops

Per manifest entry kind (`crates/shapekit-manifest/src/model.rs`). Vector cases
(`corpus/vectors/<kind>/<id>.json`) carry their op name in `op`, their inputs, and
`expected`; the runner turns each into the request below. A case it cannot map is an `error`
(a runner bug), never skipped.

| entry kind | ops sent | pass when |
|---|---|---|
| `xdr`, `decode = must-accept` | `decode_encode`, `decode_json`, `encode_json` (the last two only with an expected JSON file, `encode_json` only with `roundtrip = true`) | `decode_encode` is ok and, with `roundtrip`, returns the input bytes; `decode_json` equals the variant's expected JSON; `encode_json` of the expected JSON returns the input bytes |
| `xdr`, `decode = must-reject` | `decode_encode` | the answer is `rejected` |
| `xdr`, `decode = implementation-defined` | `decode_encode`, `decode_json` | every SDK that answers gives the same outcome (same status, same bytes, same JSON) |
| `ordering`, `asset_order` cases | `asset_order` | `expected` `less`, `equal`, `greater` is the order core uses (plan R5) |
| `ordering`, `pool_id` cases | `pool_id` | the pool ID equals `expected.pool_id`, or the answer is `rejected` when `expected.reject` is set |
| `ordering`, `map_sort` cases | `map_sort` | the result equals `expected.map` byte for byte, or is `rejected` when the case expects a rejection |
| `ordering`, `map_validate` cases | `map_validate` | `valid` is true for `expected = accept`, false for `reject` |
| `strkey` | `strkey_decode`, and `strkey_encode` for valid strings | decode gives the expected kind and raw payload, or `rejected` for invalid strings; encode of that kind and payload gives back the text |
| `textual-id` | `id_parse` | the ID equals `expected.id`, or `rejected` when the case expects a rejection. A case whose `expected` is `{"either": [{"id": ...}, {"reject": ...}]}` passes with exactly that ID or with `rejected`; any other ID diverges. This covers JSON numbers above 2^53-1 that an SDK may read exactly or refuse, so the case is decided against its own expectation even when the entry is `implementation-defined` |
| `native-projection` | `to_native` | `rejected`, or a native map that keeps every entry of the input map |

Where the expected XDR-JSON contains stellar-xdr's `<INVALID:` marker (it writes one for a
value with no strkey form, such as a signed-payload signer with an empty payload), SEP-51 has
no canonical rendering of the value. `decode_json` then passes with any JSON the SDK renders
(it still fails if the SDK refuses), and `encode_json` is not sent, since there is no
canonical JSON to encode from. `decode_encode` checks the byte round trip as usual.

Decode requests carry `depth_limit` 1000 (`DECODE_DEPTH`, the depth core's Rust parser
applies to envelopes from protocol 28) and `len_limit` equal to the input length, the limits
the corpus expectations were computed under.

Every `xdr` variant (`testnet`, `mainnet`, `any`) is checked separately. An entry's result is
the worst of its checks, in this order: `error`, `diverges-from-network`,
`diverges-from-peers`, `unsupported`, `pass`. A `panic` always counts as
`diverges-from-network`. An implementation-defined check that only one SDK answered has no
peer to compare with and counts as `unsupported`, not as a pass.

`versions.toml` may give an SDK version a `max_protocol`. A divergence on an entry generated
at a later protocol (`protocol.generated_at`, else `protocol.valid_min`) is then reported as
`unsupported` with the original verdict in its detail: an SDK that predates a protocol is not
expected to read it.

## Known failures

`conformance/known-failures/<sdk>.txt` lists one known failure per line:

```
<entry-id> <version or *> <result> <issue link or findings/issues/ draft>
```

A failing entry (`diverges-from-network`, `diverges-from-peers`, `error`) with a matching line
does not fail the run. A failing entry without one fails it. A line whose entry now passes,
is unsupported, or fails with another result fails the run too, until the line is fixed or
removed. Every line must name an entry in the manifest and a version in `versions.toml`.

## Limits the runner applies

- Per-request timeout, default 10 s, set per SDK in `conformance/versions.toml`.
- A memory limit set with `setrlimit` in the child before `exec`: `RLIMIT_AS` (address space)
  for testees whose runtime reserves little virtual memory (Rust, Go), `RLIMIT_DATA` for
  runtimes that reserve large `PROT_NONE` regions up front (Node's V8 cage), where an
  address-space cap would kill the process before it touches the data. The resource and size
  are set per SDK in `versions.toml`.

## Writing a testee

1. Read requests line by line, answer each with exactly one line, flush after every line.
2. Answer `hello` with versions read from the build (Go `debug.ReadBuildInfo`, the installed
   package's `package.json`, Cargo's resolved dependency) and never from a constant.
3. Implement each op against the SDK's public API only. Where the SDK has no API, answer
   `unsupported` with a reason. Never emulate an SDK feature inside the testee.
4. Catch every exception or panic around SDK calls and classify it.
5. Add the testee's build and run command per SDK version to `conformance/versions.toml`.

The reference testees are `testees/rust`, `testees/go` and `testees/js`.
