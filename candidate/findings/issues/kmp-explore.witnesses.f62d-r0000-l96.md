<!-- repo: Soneso/kmp-stellar-sdk -->
# com.soneso.stellar:stellar-sdk-jvm 1.18.0: the transaction object refuses values the network applied: max time 2^64-1 and offer id -1

## What happens

The transaction object refuses values the network applied: max time 2^64-1 and offer id -1.

## Cause

AbstractTransaction.fromEnvelopeXdr refuses transactions core applied: TimeBounds reads the uint64 max time into a signed Long and requires it to be >= 0 (commonMain/kotlin/com/soneso/stellar/sdk/TimeBounds.kt L25-L26 at 1.18.0, "maxTime must be >= 0" for 2^64-1), and the manage offer operations refuse offer id -1 ("Offer ID must be non-negative", Operation.kt L441 and L496), so the transaction hash cannot be had; the class of the candidate lane's other transaction-object refusals.

## Affected versions

`com.soneso.stellar:stellar-sdk-jvm` 1.18.0, every version shapekit ran.

## Entries

shapekit's finding KMP-TXOBJECT takes 2 corpus entries on this SDK: `explore.witnesses.f62d-r0000-l96`, `explore.witnesses.f62d-r0004-l69`. The checks below are those of `explore.witnesses.f62d-r0000-l96`.

## Checks on `explore.witnesses.f62d-r0000-l96`

On this entry, `com.soneso.stellar:stellar-sdk-jvm` 1.18.0 answers differently from the Stellar network in 2 checks on 1 version.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

A check named `mainnet/...` or `testnet/...` names the network passphrase it hashes or signs with. The data is a ledger stellar-core closed on a private network shapekit ran locally (chain `explore.f62d.r0000`), not a transaction or ledger of that network.

Corpus entry `explore.witnesses.f62d-r0000-l96` (family `explore.witnesses`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `testnet/tx_hash 0` on 1.18.0

The SDK refused a value the network accepts: IllegalArgumentException: maxTime must be >= 0

```json
{
  "depth_limit": 1000,
  "len_limit": 432,
  "network_passphrase": "Test SDF Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAgAAAADuNWWO3rX2RzlCu6U+/qrXObnsLObpsei7i3Z2iQ+c1AAAAGQAAABAAAAACQAAAAEAAAAAadGmAP//////////AAAAAAAAAAEAAAABAAAAAFcDGhVgMTQUM2bhKpDZboi2XN1axvAEogNuScrpNRNpAAAAAgAAAAAAAAAADRzvAQAAAACZ1fT9fwPHqbSviKB8N1RNe1BjcaulgCYSfOLBdG7+OwAAAAFVU0QAAAAAAO41ZY7etfZHOUK7pT7+qtc5uews5umx6LuLdnaJD5zUAAAAC6PvJ/gAAAABAAAAAkxPTkdBU1NFVDEyAAAAAADuNWWO3rX2RzlCu6U+/qrXObnsLObpsei7i3Z2iQ+c1AAAAAAAAAACiQ+c1AAAAEC57iWQ9bokm5s4MWxr4wG0HXiSSUAmHN3P1yaCAEzPGynUN1L2uo5wkMPiSSghN1uWfnTP/Fgrn0uBXru2KRIA6TUTaQAAAECghJYyQwNseQG6tGN+Vzx/... (576 chars)"
}
```

Expected: `{"hash":"056ee5d0f5634053a37d8fa07cc2ccc72dcf6546048b4c73aec12c5982ae9c8b"}`

Actual: `{"rejected":"IllegalArgumentException: maxTime must be >= 0"}`

### `mainnet/tx_hash 0` on 1.18.0

The SDK refused a value the network accepts: IllegalArgumentException: maxTime must be >= 0

```json
{
  "depth_limit": 1000,
  "len_limit": 432,
  "network_passphrase": "Public Global Stellar Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAgAAAADuNWWO3rX2RzlCu6U+/qrXObnsLObpsei7i3Z2iQ+c1AAAAGQAAABAAAAACQAAAAEAAAAAadGmAP//////////AAAAAAAAAAEAAAABAAAAAFcDGhVgMTQUM2bhKpDZboi2XN1axvAEogNuScrpNRNpAAAAAgAAAAAAAAAADRzvAQAAAACZ1fT9fwPHqbSviKB8N1RNe1BjcaulgCYSfOLBdG7+OwAAAAFVU0QAAAAAAO41ZY7etfZHOUK7pT7+qtc5uews5umx6LuLdnaJD5zUAAAAC6PvJ/gAAAABAAAAAkxPTkdBU1NFVDEyAAAAAADuNWWO3rX2RzlCu6U+/qrXObnsLObpsei7i3Z2iQ+c1AAAAAAAAAACiQ+c1AAAAEAikRLhvDwUmOSsBHoUGMZN+S7Pl7G3F0xw09PPKJC2b/8+r0kXfkglxRN2jlhm7sP2XjXwskWesLRzQx+X9UgM6TUTaQAAAEBTMv04BvnHVKLxP21qTJzq... (576 chars)"
}
```

Expected: `{"hash":"dcc08d2e4a0ed6745269e0da8ed22c5841ba14280f738db4e538152453c71790"}`

Actual: `{"rejected":"IllegalArgumentException: maxTime must be >= 0"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`, with the Rust toolchain of its `rust-toolchain.toml` and the toolchain `com.soneso.stellar:stellar-sdk-jvm` builds with (`shapekit doctor sdk --sdk kmp` lists what is missing):

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit conformance run --sdk kmp --version 1.18.0 --entry explore.witnesses.f62d-r0000-l96
```

The run builds the SDK's testee at that version as `conformance/versions.toml` describes, then runs the entry's checks.

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/explore.witnesses/explore.witnesses.f62d-r0000-l96/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/explore.witnesses/explore.witnesses.f62d-r0000-l96/mainnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/explore.witnesses/explore.witnesses.f62d-r0000-l96/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/explore.witnesses/explore.witnesses.f62d-r0000-l96/testnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair

How the corpus entry was built and checked: A classic recipe of the explorer's run f62d (seed 65). ledger 96 of chain explore.f62d.r0000 at protocol 30, recorded from corpus/recipes/explore.witnesses/f62d/r0000.json; picked for the arms it holds that no earlier pick holds: OperationBody:path_payment_strict_receive, OperationResultTr:path_payment_strict_receive, PathPaymentStrictReceiveResult:no_trust

## Sibling issues

- `stellar_flutter_sdk` (Soneso/stellar_flutter_sdk) diverges on 2 of these entries: shapekit's finding DART-MAXTIME, a transaction with max time 2^64-1 hashes to another value once the sdk rebuilds it (filed as https://github.com/Soneso/stellar_flutter_sdk/issues/293); DART-OFFERU64, question: offer ids are generated as XdrUint64, so the XDR-JSON of a negative offer id is another number and the canonical form does not encode (filed as https://github.com/Soneso/stellar_flutter_sdk/issues/295).
- `network.lightsail:stellar-sdk` (lightsail-network/java-stellar-sdk) diverges on 2 of these entries: no finding takes it yet.
- `soneso/stellar-php-sdk` (Soneso/stellar-php-sdk) diverges on 2 of these entries: shapekit's finding PHP-OFFERID, rebuilding a transaction with a manage sell offer of offer id -1 throws (not filed); PHP-UINT64, 64-bit values above 2^63-1 and negative int64 values lose their XDR-JSON form (not filed yet).
- `stellar-sdk` (StellarCN/py-stellar-base) diverges on 2 of these entries: no finding takes it yet.
- `stellar-cli` (stellar/stellar-cli) diverges on `explore.witnesses.f62d-r0004-l69`, one of these entries: shapekit's finding CLI-TXHASH-V1, stellar tx hash refuses v0 and fee bump envelopes the network applied (covered upstream by https://github.com/stellar/stellar-cli/issues/2769).

shapekit vector: `explore.witnesses.f62d-r0000-l96`
