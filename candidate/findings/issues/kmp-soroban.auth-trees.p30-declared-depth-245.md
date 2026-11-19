<!-- repo: Soneso/kmp-stellar-sdk -->
# com.soneso.stellar:stellar-sdk-jvm 1.18.0: XDR-JSON the sdk writes for values nested past 128 containers is refused when the sdk reads it back

## What happens

XDR-JSON the sdk writes for values nested past 128 containers is refused when the sdk reads it back.

## Cause

XdrJson caps nesting at RECURSION_CAP, 128 (commonMain/kotlin/com/soneso/stellar/sdk/xdr/XdrJson.kt L50 at 1.18.0, checked at L448 and L901). On the witnesses, writing XDR-JSON (decode_json) passes and only reading it back (encode_json) refuses ("input nests deeper than the limit of 128"), so the sdk writes XDR-JSON of authorization trees and ScVal vectors stellar-core accepted that its own reader refuses. PR #137 keeps the cap at 128 for JSON on purpose and docs/sep/sep-51.md L491 documents the limit.

## Affected versions

`com.soneso.stellar:stellar-sdk-jvm` 1.18.0, every version shapekit ran.

## Entries

shapekit's finding KMP-JSON-DEPTH takes 14 corpus entries on this SDK: `soroban.auth-trees.p30-declared-depth-245`, `soroban.auth-trees.p30-declared-depth-246`, `soroban.auth-trees.p30-declared-depth-492`, `soroban.auth-trees.synth-auth-depth-493-deepest-accepted`, `soroban.scval-shapes.p30-map-depth-max-envelope`, `soroban.scval-shapes.p30-map-depth-max-meta`, `soroban.scval-shapes.p30-map-depth-over`, `soroban.scval-shapes.p30-map-mixed-keys-sorted-meta`, `soroban.scval-shapes.p30-vec-depth-max-envelope`, `soroban.scval-shapes.p30-vec-depth-max-meta`, `soroban.scval-shapes.p30-vec-depth-over`, `soroban.scval-shapes.synth-event-vec-depth-101`, and 2 more. The checks below are those of `soroban.auth-trees.p30-declared-depth-245`.

## Checks on `soroban.auth-trees.p30-declared-depth-245`

On this entry, `com.soneso.stellar:stellar-sdk-jvm` 1.18.0 answers differently from the Stellar network in 2 checks on 1 version.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

A check named `mainnet/...` or `testnet/...` names the network passphrase it hashes or signs with. The data is a ledger stellar-core closed on a private network shapekit ran locally (chain `soroban.auth-trees.p30`), not a transaction or ledger of that network.

Corpus entry `soroban.auth-trees.p30-declared-depth-245` (family `soroban.auth-trees`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `testnet/encode_json` on 1.18.0

The SDK refused a value the network accepts: IllegalArgumentException: LedgerCloseMetaXdr: input nests deeper than the limit of 128

```json
{
  "depth": 513,
  "json_bytes": 48234,
  "preview": "{\"json\":{\"v2\":{\"evicted_keys\":[],\"ext\":\"v0\",\"ledger_header\":{\"ext\":\"v0\",\"hash\":\"f543530ff189d48db9542f32a017a7103a5aabe8ca110381bc93c587d331aed6\",\"header\":{\"base_fee\":100,\"base_reserve\":100000000,\"bucket_list_hash\":\"78128d186ad92a946f0af74888492529fba0a19c2c66e6be572c36aaf5e9a2ea\",\"ext\":\"v0\",\"fee_pool\":\"3315900\",\"id_pool\":\"0\",\"inflation_seq\":0,\"ledger_seq\":110,\"ledger_version\":30,\"max_tx_set_size\":100,\"previous_ledger_hash\":\"8f50a47935bf0df0dd8c54ab8aedfad3a05f83b703f972fbe36da8cda9842f41\",\"scp_value\":{\"clo",
  "sha256": "cecc489e86cd97708a2c4a061f8986476340a6d4facf6240341d475866c46f99"
}
```

Expected: `{"xdr":"AAAAAgAAAAD1Q1MP8YnUjblULzKgF6cQOlqr6MoRA4G8k8WH0zGu1gAAAB6PUKR5Nb8N8N2MVKuK7frToF+DtwP5cvvjbajNqYQvQV1t8I6BmYD78VG3fDizjVwXpv40Kr7Wps02CmkTdMR3AAAAAGnmvgAAAAAAAAAAAwAAAZ2tVjAAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQHkLiPYgmCIfgqC57QFuuk4XXJqK8MhM8dbN9Ni5XZiObrbjwJybIOAcjkosWQuPM5lDqrd9i3IMquqEWP+/4AYdz6c2od01/El+xLG3d55wKUiytYYPofhtiCLioJpI/XgSjRhq2SqUbwr3SIhJJSn7oKGcLGbmvlcsNqr16aLqAAAAbg3gtrOnZAAAAAAAAAAymLwAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGSFV+f5x6r04qQKss+eUbjZYRaQUcH1/G/PXKGmEPOqLgAAAAAAAAAA... (22272 chars)"}`

Actual: `{"rejected":"IllegalArgumentException: LedgerCloseMetaXdr: input nests deeper than the limit of 128"}`

### `mainnet/encode_json` on 1.18.0

The SDK refused a value the network accepts: IllegalArgumentException: LedgerCloseMetaXdr: input nests deeper than the limit of 128

```json
{
  "depth": 513,
  "json_bytes": 48234,
  "preview": "{\"json\":{\"v2\":{\"evicted_keys\":[],\"ext\":\"v0\",\"ledger_header\":{\"ext\":\"v0\",\"hash\":\"44e084b10e7cc573dd64c3e3463feb88c0b49c4227ff3b616bfe4c16b28731d9\",\"header\":{\"base_fee\":100,\"base_reserve\":100000000,\"bucket_list_hash\":\"ee25351d178e91497669e08548b095a0e124b42fa454275524ea8b943db98f32\",\"ext\":\"v0\",\"fee_pool\":\"3315875\",\"id_pool\":\"0\",\"inflation_seq\":0,\"ledger_seq\":110,\"ledger_version\":30,\"max_tx_set_size\":100,\"previous_ledger_hash\":\"ada6e5ddc31699621c2bfa817353c4448eba438c0ebd25fa0e48607eb9861ff7\",\"scp_value\":{\"clo",
  "sha256": "53438a2fa5f18b42def799ec4dbce738e9b306aee2e43d02117c67a0fb3f16c4"
}
```

Expected: `{"xdr":"AAAAAgAAAABE4ISxDnzFc91kw+NGP+uIwLScQif/O2Fr/kwWsocx2QAAAB6tpuXdwxaZYhwr+oFzU8REjrpDjA69JfoOSGB+uYYf91whLLQ0zKVfCZxz1BuDszAPG6PxCWQ33IjNfbMFjRqeAAAAAGnmvgAAAAAAAAAAAwAAAZ2tVjAAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQC0gR/NjQU5VtOrkD09UUo+OEwMPry0EgOBn9CbYevIULuYG3YywUNMRBAGsHCxmFW1xAAkt50tMmCMqfCGewwGXQtLZcBo8/AbqmBkmrQJGfZfM1Fe06zztT1rMsRKbHu4lNR0XjpFJdmnghUiwlaDhJLQvpFQnVSTqi5Q9uY8yAAAAbg3gtrOnZAAAAAAAAAAymKMAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGT+NAJiuIqaEIIhDzWrtobIFvaN4SBza3QNr/JzIxMEGwAAAAAAAAAA... (22272 chars)"}`

Actual: `{"rejected":"IllegalArgumentException: LedgerCloseMetaXdr: input nests deeper than the limit of 128"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`, with the Rust toolchain of its `rust-toolchain.toml` and the toolchain `com.soneso.stellar:stellar-sdk-jvm` builds with (`shapekit doctor sdk --sdk kmp` lists what is missing):

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit conformance run --sdk kmp --version 1.18.0 --entry soroban.auth-trees.p30-declared-depth-245
```

The run builds the SDK's testee at that version as `conformance/versions.toml` describes, then runs the entry's checks.

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.xdr
- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.xdr

How the corpus entry was built and checked: An address entry whose matched root declares a chain 245 invocations deep below it that no call reaches, the deepest the host still decodes; the transaction succeeds and the declared nodes are never checked

Related: https://github.com/stellar/wallet-backend/issues/746

## Sibling issues

- `stellar-dotnet-sdk` (Beans-BV/dotnet-stellar-sdk) diverges on 14 of these entries: shapekit's finding NET-DEPTH, XDR decoders stop at 200 nested types, below stellar-core's limit (filed as https://github.com/Beans-BV/dotnet-stellar-sdk/issues/275).
- `network.lightsail:stellar-sdk` (lightsail-network/java-stellar-sdk) diverges on 12 of these entries: no finding takes it yet.
- `@stellar/stellar-sdk` (stellar/js-stellar-sdk) diverges on 14 of these entries: shapekit's finding JS-16X, 16.x lacks six fixes that 17.x has: map key order and collisions, textual ids, W addresses, decode depth, StrKey checks (filed as https://github.com/stellar/js-stellar-sdk/issues/1907).
- `soneso/stellar-php-sdk` (Soneso/stellar-php-sdk) diverges on 5 of these entries: shapekit's finding PHP-JSON-DEPTH, XDR-JSON stops at 512 nested arrays, below stellar-core's limits (filed as https://github.com/Soneso/stellar-php-sdk/issues/243).
- `stellar-sdk` (StellarCN/py-stellar-base) diverges on 10 of these entries: no finding takes it yet.
- `stellar-xdr` (stellar/rs-stellar-xdr) diverges on 14 of these entries: shapekit's finding RS-2, the XDR-JSON reader stops at serde_json's recursion limit of 128, so values the crate writes as JSON cannot be read back (filed as https://github.com/stellar/rs-stellar-xdr/issues/628).
- `stellar-cli` (stellar/stellar-cli) diverges on 14 of these entries: shapekit's finding CLI-DEPTH, question: stellar tx hash decodes with a depth limit of 500, below the 1000 stellar-core accepts from protocol 28 (filed as https://github.com/stellar/stellar-cli/issues/2895); CLI-JSON-DEPTH, stellar xdr encode stops reading XDR-JSON at serde_json's nesting default of 128 (covered upstream by https://github.com/stellar/rs-stellar-xdr/issues/628).

shapekit vector: `soroban.auth-trees.p30-declared-depth-245`
