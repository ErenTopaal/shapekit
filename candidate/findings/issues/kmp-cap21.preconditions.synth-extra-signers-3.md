<!-- repo: Soneso/kmp-stellar-sdk -->
# com.soneso.stellar:stellar-sdk-jvm 1.18.0: XDR decoders accept arrays, strings and opaques past their schema maximum and nonzero padding

## What happens

XDR decoders accept arrays, strings and opaques past their schema maximum and nonzero padding.

## Cause

The generated decoders read arrays with XdrReader.readArrayLength, bounded only by the bytes left (PreconditionsV2Xdr reads extraSigners<2> with it, commonMain/kotlin/com/soneso/stellar/sdk/xdr/PreconditionsV2Xdr.kt L86), and XdrReader skips padding without reading it (jvmMain/kotlin/com/soneso/stellar/sdk/xdr/XdrReader.jvm.kt L64-L67), so values over their bound and padding that is not zero decode where stellar-core refuses them.

## Affected versions

`com.soneso.stellar:stellar-sdk-jvm` 1.18.0, every version shapekit ran.

## Entries

shapekit's finding KMP-BOUNDS takes 18 corpus entries on this SDK: `cap21.preconditions.synth-extra-signers-3`, `cap23.claimable-balances.synth-claimants-11`, `cap40.signed-payload-signer.synth-payload-65-bytes`, `cap40.signed-payload-signer.synth-signer-key-nonzero-padding`, `classic.max-length.synth-data-value-65-bytes`, `classic.max-length.synth-memo-text-29-bytes`, `classic.max-length.synth-path-6-assets`, `classic.max-length.synth-signature-65-bytes`, `classic.max-length.synth-tx-101-operations`, `classic.max-length.synth-tx-21-signatures`, `soroban.auth-trees.p30-declared-depth-493`, `soroban.auth-trees.synth-auth-depth-494-over-cap`, and 6 more. The checks below are those of `cap21.preconditions.synth-extra-signers-3`.

## Checks on `cap21.preconditions.synth-extra-signers-3`

On this entry, `com.soneso.stellar:stellar-sdk-jvm` 1.18.0 answers differently from the Stellar network in 1 check on 1 version.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

Corpus entry `cap21.preconditions.synth-extra-signers-3` (family `cap21.preconditions`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `any/decode_encode` on 1.18.0

The SDK accepted a value the network refuses

```json
{
  "depth_limit": 1000,
  "len_limit": 336,
  "op": "decode_encode",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAADAAAAAhQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUAAAAAhUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVAAAAAhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWAAAAAAAAAAEAAAAAAAAAAQAAAAACAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgAAAAAAAAAAAAAAAQAAAAAAAAABBwcHBwAAAEAHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcH"
}
```

Expected: `"rejected"`

Actual: `{"xdr":"AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAADAAAAAhQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUAAAAAhUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVAAAAAhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWAAAAAAAAAAEAAAAAAAAAAQAAAAACAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgAAAAAAAAAAAAAAAQAAAAAAAAABBwcHBwAAAEAHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcH"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`, with the Rust toolchain of its `rust-toolchain.toml` and the toolchain `com.soneso.stellar:stellar-sdk-jvm` builds with (`shapekit doctor sdk --sdk kmp` lists what is missing):

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit conformance run --sdk kmp --version 1.18.0 --entry cap21.preconditions.synth-extra-signers-3
```

The run builds the SDK's testee at that version as `conformance/versions.toml` describes, then runs the entry's checks.

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap21.preconditions/cap21.preconditions.synth-extra-signers-3/any.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap21.preconditions/cap21.preconditions.synth-extra-signers-3/any.xdr): decode = must-reject (validity InvalidByCore)

How the corpus entry was built and checked: Preconditions v2 with 3 extra signers, one past extraSigners<2>. stellar-xdr 30.0.0 rejects it (TransactionEnvelope::from_xdr) with LengthExceedsMax under Limits { depth: 1000 (xdr.envelope.depth), len: 336 (the input length) }.

## Sibling issues

- `stellar_flutter_sdk` (Soneso/stellar_flutter_sdk) diverges on 15 of these entries: shapekit's finding DART-BOUNDS, XDR decoders accept arrays, strings and opaque values longer than the schema allows, and nesting deeper than stellar-core accepts (filed as https://github.com/Soneso/stellar_flutter_sdk/issues/291).
- `stellar-dotnet-sdk` (Beans-BV/dotnet-stellar-sdk) diverges on `classic.max-length.synth-memo-text-29-bytes`, one of these entries: shapekit's finding NET-BOUNDS, the text memo decoder accepts 29 bytes (covered upstream by https://github.com/Beans-BV/dotnet-stellar-sdk/issues/250).
- `github.com/stellar/go-stellar-sdk` (stellar/go-stellar-sdk) diverges on 4 of these entries: shapekit's finding GO-4, xdr.DecodeOptions.MaxDepth counts fewer levels per nesting than stellar-xdr, so the sdk decodes envelopes nested deeper than core accepts (filed as https://github.com/stellar/go-stellar-sdk/issues/6047).
- `network.lightsail:stellar-sdk` (lightsail-network/java-stellar-sdk) diverges on 4 of these entries: no finding takes it yet.
- `@stellar/stellar-sdk` (stellar/js-stellar-sdk) diverges on 4 of these entries: shapekit's finding JS-DEPTH, fromXdr decodes envelopes nested deeper than stellar-core accepts, and no maxDepth value matches core's limit (filed as https://github.com/stellar/js-stellar-sdk/issues/1908).
- `soneso/stellar-php-sdk` (Soneso/stellar-php-sdk) diverges on 18 of these entries: shapekit's finding PHP-BOUNDS, XDR decoders accept values the schema forbids: arrays and strings past their maximum, nonzero padding, nesting past stellar-core's limit (filed as https://github.com/Soneso/stellar-php-sdk/issues/239).
- `stellar-sdk` (StellarCN/py-stellar-base) diverges on 6 of these entries: no finding takes it yet.

shapekit vector: `cap21.preconditions.synth-extra-signers-3`
