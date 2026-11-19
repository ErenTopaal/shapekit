<!-- repo: Soneso/stellar-php-sdk -->
# soneso/stellar-php-sdk 1.18.0: XDR-JSON stops at 512 nested arrays, below stellar-core's limits

## What happens

XDR-JSON stops at 512 nested arrays, below stellar-core's limits.

## Cause

ToJson calls json_encode without a depth (Soneso/StellarSDK/Xdr/XdrTransactionEnvelopeBase.php L123-L128 at 1.18.0, and the same in every generated class), so php's default of 512 applies when the sdk writes XDR-JSON. Reading goes through XdrJsonHelper, which passes DECODE_MAX_DEPTH, set to the same 512 (Xdr/XdrJsonHelper.php L40 and L859). The XDR-JSON of authorization trees and ScVal maps that stellar-core accepts nests deeper, so both directions refuse it ("Maximum stack depth exceeded").

## Affected versions

`soneso/stellar-php-sdk` 1.18.0, every version shapekit ran.

## Entries

shapekit's finding PHP-JSON-DEPTH takes 5 corpus entries on this SDK: `soroban.auth-trees.p30-declared-depth-245`, `soroban.auth-trees.p30-declared-depth-246`, `soroban.auth-trees.p30-declared-depth-492`, `soroban.auth-trees.synth-auth-depth-493-deepest-accepted`, `soroban.scval-shapes.synth-scmap-depth-197-deepest-accepted`. The checks below are those of `soroban.auth-trees.synth-auth-depth-493-deepest-accepted`.

## Checks on `soroban.auth-trees.synth-auth-depth-493-deepest-accepted`

On this entry, `soneso/stellar-php-sdk` 1.18.0 answers differently from the Stellar network in 2 checks on 1 version.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

Corpus entry `soroban.auth-trees.synth-auth-depth-493-deepest-accepted` (family `soroban.auth-trees`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `any/decode_json` on 1.18.0

The SDK refused a value the network accepts: JsonException: Maximum stack depth exceeded

```json
{
  "depth_limit": 1000,
  "len_limit": 27852,
  "op": "decode_json",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAATIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyAAAABGRlZXAAAAAAAAAAAQAAAAAAAAABMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIAAAAEZGVlcAAAAAAAAAABAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAATIyMjIyMjIyMjIyMjIyMjIyMjIy... (37136 chars)"
}
```

Expected: `{"depth":997,"json_bytes":80656,"preview":"{\"json\":{\"tx\":{\"signatures\":[{\"hint\":\"07070707\",\"signature\":\"07070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707\"}],\"tx\":{\"cond\":\"none\",\"ext\":{\"v1\":{\"ext\":\"v0\",\"resource_fee\":\"100000\",\"resources\":{\"disk_read_bytes\":0,\"footprint\":{\"read_only\":[],\"read_write\":[]},\"instructions\":1000000,\"write_bytes\":0}}},\"fee\":100,\"memo\":\"none\",\"operations\":[{\"body\":{\"invoke_host_function\":{\"auth\":[{\"credentials\":\"source_account\",\... (700 chars, see the source below)`

Actual: `{"rejected":"JsonException: Maximum stack depth exceeded"}`

### `any/encode_json` on 1.18.0

The SDK refused a value the network accepts: JsonException: Maximum stack depth exceeded

```json
{
  "depth": 997,
  "json_bytes": 80704,
  "preview": "{\"json\":{\"tx\":{\"signatures\":[{\"hint\":\"07070707\",\"signature\":\"07070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707\"}],\"tx\":{\"cond\":\"none\",\"ext\":{\"v1\":{\"ext\":\"v0\",\"resource_fee\":\"100000\",\"resources\":{\"disk_read_bytes\":0,\"footprint\":{\"read_only\":[],\"read_write\":[]},\"instructions\":1000000,\"write_bytes\":0}}},\"fee\":100,\"memo\":\"none\",\"operations\":[{\"body\":{\"invoke_host_function\":{\"auth\":[{\"credentials\":\"source_account\",\"root_invocation\":{\"",
  "sha256": "cc8f7920ec49718124482c941e189230a894069c14b86602b6862e316b7c88bf"
}
```

Expected: `{"xdr":"AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAATIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyAAAABGRlZXAAAAAAAAAAAQAAAAAAAAABMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIAAAAEZGVlcAAAAAAAAAABAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAATIyMjIyMjIyMjIyMjIyMjIyMjIy... (37136 chars)"}`

Actual: `{"rejected":"JsonException: Maximum stack depth exceeded"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`, with the Rust toolchain of its `rust-toolchain.toml` and the toolchain `soneso/stellar-php-sdk` builds with (`shapekit doctor sdk --sdk php` lists what is missing):

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit conformance run --sdk php --version 1.18.0 --entry soroban.auth-trees.synth-auth-depth-493-deepest-accepted
```

The run builds the SDK's testee at that version as `conformance/versions.toml` describes, then runs the entry's checks.

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.synth-auth-depth-493-deepest-accepted/any.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.synth-auth-depth-493-deepest-accepted/any.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.synth-auth-depth-493-deepest-accepted/any.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.synth-auth-depth-493-deepest-accepted/any.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.auth-trees/soroban.auth-trees.synth-auth-depth-493-deepest-accepted/any.xdr

How the corpus entry was built and checked: A source-account authorization entry whose invocation tree is a single chain 493 invocations deep, at the deepest an envelope decodes at depth 1000. core's depth check accepts it; the host converts at most 100 levels (scval.depth) and runs at most 100 frames, so an invocation this deep fails and the transaction lands in the ledger as failed with the full tree intact. a decoder must accept it.

## Sibling issues

- `stellar-dotnet-sdk` (Beans-BV/dotnet-stellar-sdk) diverges on 5 of these entries: shapekit's finding NET-DEPTH, XDR decoders stop at 200 nested types, below stellar-core's limit (filed as https://github.com/Beans-BV/dotnet-stellar-sdk/issues/275).
- `network.lightsail:stellar-sdk` (lightsail-network/java-stellar-sdk) diverges on 5 of these entries: no finding takes it yet.
- `@stellar/stellar-sdk` (stellar/js-stellar-sdk) diverges on 5 of these entries: shapekit's finding JS-16X, 16.x lacks six fixes that 17.x has: map key order and collisions, textual ids, W addresses, decode depth, StrKey checks (filed as https://github.com/stellar/js-stellar-sdk/issues/1907).
- `com.soneso.stellar:stellar-sdk-jvm` (Soneso/kmp-stellar-sdk) diverges on 5 of these entries: shapekit's finding KMP-JSON-DEPTH, XDR-JSON the sdk writes for values nested past 128 containers is refused when the sdk reads it back (filed as https://github.com/Soneso/kmp-stellar-sdk/issues/169).
- `stellar-sdk` (StellarCN/py-stellar-base) diverges on 4 of these entries: no finding takes it yet.
- `stellar-xdr` (stellar/rs-stellar-xdr) diverges on 5 of these entries: shapekit's finding RS-2, the XDR-JSON reader stops at serde_json's recursion limit of 128, so values the crate writes as JSON cannot be read back (filed as https://github.com/stellar/rs-stellar-xdr/issues/628).
- `stellar-cli` (stellar/stellar-cli) diverges on 5 of these entries: shapekit's finding CLI-DEPTH, question: stellar tx hash decodes with a depth limit of 500, below the 1000 stellar-core accepts from protocol 28 (filed as https://github.com/stellar/stellar-cli/issues/2895); CLI-JSON-DEPTH, stellar xdr encode stops reading XDR-JSON at serde_json's nesting default of 128 (covered upstream by https://github.com/stellar/rs-stellar-xdr/issues/628).

shapekit vector: `soroban.auth-trees.synth-auth-depth-493-deepest-accepted`
