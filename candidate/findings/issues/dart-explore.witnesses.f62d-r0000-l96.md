<!-- repo: Soneso/stellar_flutter_sdk -->
# stellar_flutter_sdk 3.12.0: a transaction with max time 2^64-1 hashes to another value once the sdk rebuilds it

## What happens

A transaction with max time 2^64-1 hashes to another value once the sdk rebuilds it.

## Cause

TimeBounds.fromXdr reads the max time with BigInt.toInt (lib/src/transaction.dart L1088-L1093 at 3.12.0, toInt at L1090-L1091), which clamps 18446744073709551615 to 9223372036854775807, and toXdr writes the clamped value back (L1096-L1101), so a transaction core applied with max_time 2^64-1 hashes to another value.

## Affected versions

`stellar_flutter_sdk` 3.12.0, every version shapekit ran.

## Entries

shapekit's finding DART-MAXTIME takes 1 corpus entry on this SDK: `explore.witnesses.f62d-r0000-l96`. The checks below are those of `explore.witnesses.f62d-r0000-l96`.

## Checks on `explore.witnesses.f62d-r0000-l96`

On this entry, `stellar_flutter_sdk` 3.12.0 answers differently from the Stellar network in 2 checks on 1 version.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

A check named `mainnet/...` or `testnet/...` names the network passphrase it hashes or signs with. The data is a ledger stellar-core closed on a private network shapekit ran locally (chain `explore.f62d.r0000`), not a transaction or ledger of that network.

Corpus entry `explore.witnesses.f62d-r0000-l96` (family `explore.witnesses`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `testnet/tx_hash 0` on 3.12.0

The SDK's hash differs from the one the network wrote

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

Actual: `{"hash":"68ce6cb803b3c7f36c8ea520555a3111b6ed1876c3b68f0c2622cbbecde21dda"}`

### `mainnet/tx_hash 0` on 3.12.0

The SDK's hash differs from the one the network wrote

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

Actual: `{"hash":"f4a04fb30ef0f242d651027e67b1e3f61961d35976011a929bd4c290b79b56a8"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`, with the Rust toolchain of its `rust-toolchain.toml` and the toolchain `stellar_flutter_sdk` builds with (`shapekit doctor sdk --sdk dart` lists what is missing):

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit conformance run --sdk dart --version 3.12.0 --entry explore.witnesses.f62d-r0000-l96
```

The run builds the SDK's testee at that version as `conformance/versions.toml` describes, then runs the entry's checks.

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/explore.witnesses/explore.witnesses.f62d-r0000-l96/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/explore.witnesses/explore.witnesses.f62d-r0000-l96/mainnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/explore.witnesses/explore.witnesses.f62d-r0000-l96/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/explore.witnesses/explore.witnesses.f62d-r0000-l96/testnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair

How the corpus entry was built and checked: A classic recipe of the explorer's run f62d (seed 65). ledger 96 of chain explore.f62d.r0000 at protocol 30, recorded from corpus/recipes/explore.witnesses/f62d/r0000.json; picked for the arms it holds that no earlier pick holds: OperationBody:path_payment_strict_receive, OperationResultTr:path_payment_strict_receive, PathPaymentStrictReceiveResult:no_trust

## Sibling issues

- `network.lightsail:stellar-sdk` (lightsail-network/java-stellar-sdk) diverges on the same entry: no finding takes it yet.
- `com.soneso.stellar:stellar-sdk-jvm` (Soneso/kmp-stellar-sdk) diverges on the same entry: shapekit's finding KMP-TXOBJECT, the transaction object refuses values the network applied: max time 2^64-1 and offer id -1 (filed as https://github.com/Soneso/kmp-stellar-sdk/issues/171).
- `soneso/stellar-php-sdk` (Soneso/stellar-php-sdk) diverges on the same entry: shapekit's finding PHP-UINT64, 64-bit values above 2^63-1 and negative int64 values lose their XDR-JSON form (not filed yet).
- `stellar-sdk` (StellarCN/py-stellar-base) diverges on the same entry: no finding takes it yet.

shapekit vector: `explore.witnesses.f62d-r0000-l96`
