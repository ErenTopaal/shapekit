<!-- repo: Soneso/stellar-php-sdk -->
# soneso/stellar-php-sdk 1.18.0: XDR decoders accept values the schema forbids: arrays and strings past their maximum, nonzero padding, nesting past stellar-core's limit

## What happens

XDR decoders accept values the schema forbids: arrays and strings past their maximum, nonzero padding, nesting past stellar-core's limit.

## Cause

XdrBuffer::readOpaqueVariable and readString take a maxLength the generated decoders never pass, and skip the padding without reading it (Soneso/StellarSDK/Xdr/XdrBuffer.php L158-L173); readArrayLength bounds a count only by the bytes left (XdrPreconditionsV2::decode reads extraSigners<2> with it, Xdr/XdrPreconditionsV2.php L74). Nesting is capped at DECODE_DEPTH_LIMIT (500) recursive decodes (Xdr/XdrBuffer.php L33), not core's limit. 1.18.0 reads 101 operations, 21 signatures, a 29-byte memo, a 33-byte symbol, 65-byte payloads, 6 path assets, 11 claimants, 3 extra signers, 2 spec outputs, nonzero padding in strings, bytes and signer keys, and ScVal and authorization trees past core's depth, all of which stellar-core refuses.

## Affected versions

`soneso/stellar-php-sdk` 1.18.0, every version shapekit ran.

## Entries

shapekit's finding PHP-BOUNDS takes 19 corpus entries on this SDK: `cap21.preconditions.synth-extra-signers-3`, `cap23.claimable-balances.synth-claimants-11`, `cap40.signed-payload-signer.synth-payload-65-bytes`, `cap40.signed-payload-signer.synth-signer-key-nonzero-padding`, `classic.max-length.synth-data-value-65-bytes`, `classic.max-length.synth-memo-text-29-bytes`, `classic.max-length.synth-path-6-assets`, `classic.max-length.synth-signature-65-bytes`, `classic.max-length.synth-tx-101-operations`, `classic.max-length.synth-tx-21-signatures`, `classic.memo-bytes.synth-memo-text-nonzero-padding`, `soroban.auth-trees.p30-declared-depth-493`, and 7 more. The checks below are those of `classic.max-length.synth-tx-101-operations`.

## Checks on `classic.max-length.synth-tx-101-operations`

On this entry, `soneso/stellar-php-sdk` 1.18.0 answers differently from the Stellar network in 1 check on 1 version.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

Corpus entry `classic.max-length.synth-tx-101-operations` (family `classic.max-length`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `any/decode_encode` on 1.18.0

The SDK accepted a value the network refuses

```json
{
  "depth_limit": 1000,
  "len_limit": 5800,
  "op": "decode_encode",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAZQAAAAAAAAABAAAAAAICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMAAAAAAAAAAAAAAAEAAAAAAAAAAQAAAAAEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAAAAAAAAAAAAAAAAQAAAAAAAAABAAAAAAICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMAAAAAAAAAAAAAAAEAAAAAAAAAAQAAAAAEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE... (7736 chars)"
}
```

Expected: `"rejected"`

Actual: `{"xdr":"AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAZQAAAAAAAAABAAAAAAICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMAAAAAAAAAAAAAAAEAAAAAAAAAAQAAAAAEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAAAAAAAAAAAAAAAAQAAAAAAAAABAAAAAAICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMAAAAAAAAAAAAAAAEAAAAAAAAAAQAAAAAEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE... (7736 chars)"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`, with the Rust toolchain of its `rust-toolchain.toml` and the toolchain `soneso/stellar-php-sdk` builds with (`shapekit doctor sdk --sdk php` lists what is missing):

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit conformance run --sdk php --version 1.18.0 --entry classic.max-length.synth-tx-101-operations
```

The run builds the SDK's testee at that version as `conformance/versions.toml` describes, then runs the entry's checks.

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/classic.max-length/classic.max-length.synth-tx-101-operations/any.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/classic.max-length/classic.max-length.synth-tx-101-operations/any.xdr): decode = must-reject (validity InvalidByCore)

How the corpus entry was built and checked: A v1 transaction envelope with 101 operations, one past MAX_OPS_PER_TX = 100 (operations<MAX_OPS_PER_TX> in Stellar-transaction.x). stellar-xdr 30.0.0 rejects it (TransactionEnvelope::from_xdr) with LengthExceedsMax under Limits { depth: 1000 (xdr.envelope.depth), len: 5800 (the input length) }.

## Sibling issues

- `stellar_flutter_sdk` (Soneso/stellar_flutter_sdk) diverges on 15 of these entries: shapekit's finding DART-BOUNDS, XDR decoders accept arrays, strings and opaque values longer than the schema allows, and nesting deeper than stellar-core accepts (not filed yet).
- `stellar-dotnet-sdk` (Beans-BV/dotnet-stellar-sdk) diverges on `classic.max-length.synth-memo-text-29-bytes`, one of these entries: shapekit's finding NET-BOUNDS, the text memo decoder accepts 29 bytes (covered upstream by https://github.com/Beans-BV/dotnet-stellar-sdk/issues/250).
- `github.com/stellar/go-stellar-sdk` (stellar/go-stellar-sdk) diverges on 4 of these entries: shapekit's finding GO-4, xdr.DecodeOptions.MaxDepth counts fewer levels per nesting than stellar-xdr, so the sdk decodes envelopes nested deeper than core accepts (filed as https://github.com/stellar/go-stellar-sdk/issues/6047).
- `network.lightsail:stellar-sdk` (lightsail-network/java-stellar-sdk) diverges on 4 of these entries: no finding takes it yet.
- `@stellar/stellar-sdk` (stellar/js-stellar-sdk) diverges on 4 of these entries: shapekit's finding JS-DEPTH, fromXdr decodes envelopes nested deeper than stellar-core accepts, and no maxDepth value matches core's limit (filed as https://github.com/stellar/js-stellar-sdk/issues/1908).
- `com.soneso.stellar:stellar-sdk-jvm` (Soneso/kmp-stellar-sdk) diverges on 19 of these entries: shapekit's finding KMP-BOUNDS, XDR decoders accept arrays, strings and opaques past their schema maximum and nonzero padding (not filed yet); KMP-UTF8, XDR strings are decoded to Kotlin strings, so memos, symbols and strings that are not UTF-8 change (not filed yet).
- `stellar-sdk` (StellarCN/py-stellar-base) diverges on 7 of these entries: no finding takes it yet.

shapekit vector: `classic.max-length.synth-tx-101-operations`
