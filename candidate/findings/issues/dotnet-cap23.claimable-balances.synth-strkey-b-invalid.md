<!-- repo: Beans-BV/dotnet-stellar-sdk -->
# stellar-dotnet-sdk 16.0.0, 17.0.0: StrKey decoders accept claimable balance and signed payload strkeys SEP-23 refuses

## What happens

StrKey decoders accept claimable balance and signed payload strkeys SEP-23 refuses.

## Cause

DecodeClaimableBalanceId checks only the version byte and checksum (StellarDotnetSdk/StrKey.cs L447-L450 at 17.0.0), so a balance id with sub-version 1 decodes; DecodeSignedPayload reads the payload with the XDR decoder and does not check that it consumed the whole strkey (L405-L420), so a payload one byte long or a length prefix shorter than the bytes decodes.

## Affected versions

`stellar-dotnet-sdk` 16.0.0, 17.0.0, every version shapekit ran.

## Entries

shapekit's finding NET-STRKEY takes 2 corpus entries on this SDK: `cap23.claimable-balances.synth-strkey-b-invalid`, `cap40.signed-payload-signer.synth-strkey-p-invalid`. The checks below are those of `cap23.claimable-balances.synth-strkey-b-invalid`.

## Checks on `cap23.claimable-balances.synth-strkey-b-invalid`

On this entry, `stellar-dotnet-sdk` 16.0.0, 17.0.0 answers differently from the Stellar network in 2 checks on 2 versions.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

Corpus entry `cap23.claimable-balances.synth-strkey-b-invalid` (family `cap23.claimable-balances`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `case 13/strkey_decode` on 16.0.0, 17.0.0

The SDK accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "BAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAADQFQ"
}
```

Expected: `"rejected"`

Actual: `{"kind":"claimable_balance","payload":"AQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}`

### `case 15/strkey_decode` on 16.0.0, 17.0.0

The SDK accepted a value the network refuses

```json
{
  "op": "strkey_decode",
  "text": "BAAT6DBUX6J22DMZOHIEZTEQ64CVCHEDRKWZONFEUL5Q26QD7R76RGXACA"
}
```

Expected: `"rejected"`

Actual: `{"kind":"claimable_balance","payload":"AT8MNL+TrQ2ZcdBMzJD3BVEcg4qtlzSkovsNegP8f+ia"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`, with the Rust toolchain of its `rust-toolchain.toml` and the toolchain `stellar-dotnet-sdk` builds with (`shapekit doctor sdk --sdk dotnet` lists what is missing):

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit conformance run --sdk dotnet --version 17.0.0 --entry cap23.claimable-balances.synth-strkey-b-invalid
```

The run builds the SDK's testee at that version as `conformance/versions.toml` describes, then runs the entry's checks.

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/vectors/strkey/cap23.claimable-balances.synth-strkey-b-invalid.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/vectors/strkey/cap23.claimable-balances.synth-strkey-b-invalid.json) cases 13, 15

How the corpus entry was built and checked: Invalid claimable balance ids (B, 1 << 3): a version byte (0 for v0) and 32 hash bytes. expectations are stellar-strkey 1.0.0's verdicts; SEP-23 1.4.0 says a strkey is valid only if re-encoding its binary gives back the same string, which rules out wrong lengths, set trailing bits, padding and lowercase. the reject reason is stellar-strkey's error and is informational; a testee passes by rejecting.

## Sibling issues

- `github.com/stellar/go-stellar-sdk` (stellar/go-stellar-sdk) diverges on 2 of these entries: shapekit's finding GO-1, strkey.Decode, DecodeAny and IsValidClaimableBalance accept a claimable balance id whose type byte is not v0 (filed as https://github.com/stellar/go-stellar-sdk/issues/6046); GO-2, v0.7.1 strkey decoders accept payloads of the wrong length (covered upstream by https://github.com/stellar/go-stellar-sdk/pull/5977).
- `network.lightsail:stellar-sdk` (lightsail-network/java-stellar-sdk) diverges on `cap23.claimable-balances.synth-strkey-b-invalid`, one of these entries: no finding takes it yet.
- `@stellar/stellar-sdk` (stellar/js-stellar-sdk) diverges on 2 of these entries: shapekit's finding JS-16X, 16.x lacks six fixes that 17.x has: map key order and collisions, textual ids, W addresses, decode depth, StrKey checks (filed as https://github.com/stellar/js-stellar-sdk/issues/1907).
- `stellar-sdk` (StellarCN/py-stellar-base) diverges on 2 of these entries: no finding takes it yet.
- `stellar-cli` (stellar/stellar-cli) diverges on 2 of these entries: shapekit's finding CLI-STRKEY-TRIM, stellar strkey decode trims whitespace around its input before decoding it (not filed).

shapekit vector: `cap23.claimable-balances.synth-strkey-b-invalid`
