<!-- repo: Beans-BV/dotnet-stellar-sdk -->
# stellar-dotnet-sdk 16.0.0, 17.0.0: a transaction whose offer price the sdk does not get back from a decimal hashes to another value

## What happens

A transaction whose offer price the sdk does not get back from a decimal hashes to another value.

## Cause

ManageSellOfferOperation.FromXdr turns the price into decimal.Divide(N, D) written as a string, and ToOperationBody reads it back with Price.FromString (StellarDotnetSdk/Operations/ManageSellOfferOperation.cs L83 and L95-L106, Price.cs L55-L96, a continued fraction bounded by int.MaxValue), which writes another fraction for prices not in lowest terms and turns 1/2147483647 into 0/1, so Transaction.FromEnvelopeXdr(..).Hash(network) is not the network hash. ManageBuyOfferOperation.cs (L85, L104) and CreatePassiveSellOfferOperation.cs (L74, L90) take the same round trip.

## Affected versions

`stellar-dotnet-sdk` 16.0.0, 17.0.0, every version shapekit ran.

## Entries

shapekit's finding NET-PRICE takes 7 corpus entries on this SDK: `cap03.offer-liabilities.mainnet-37466023`, `cap15.fee-bump.mainnet-57369500`, `explore.witnesses.g2-r0007-l69`, `explore.witnesses.g2-r0010-l92`, `explore.witnesses.g2-r0027-l67`, `explore.witnesses.g2-r0031-l67`, `explore.witnesses.g2-r0038-l67`. The checks below are those of `cap03.offer-liabilities.mainnet-37466023`.

## Checks on `cap03.offer-liabilities.mainnet-37466023`

On this entry, `stellar-dotnet-sdk` 16.0.0, 17.0.0 answers differently from the Stellar network in 3 checks on 2 versions.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

Corpus entry `cap03.offer-liabilities.mainnet-37466023` (family `cap03.offer-liabilities`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `mainnet/tx_hash 12` on 16.0.0, 17.0.0

The SDK's hash differs from the one the network wrote

```json
{
  "depth_limit": 1000,
  "len_limit": 580,
  "network_passphrase": "Public Global Stellar Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAgAAAAB1Ia8qanKCYzPBjqDk0fDXDt+HBxpz1n1xlFeOGe4Z3QAAASwCKyDcAAQQdQAAAAEAAAAAAAAAAAAAAABhS2xcAAAAAAAAAAMAAAABAAAAABF2gExuzNZul/nU5auEEXLnJji3r4cdCg3hkgL+CVQoAAAAAwAAAAFFQ0YAAAAAAKZMZ2k7kLMMQjCVufMzZ4Tilb8EftreR8YmRLTB+QOmAAAAAAAAAE6W8RmTAGhQUkQ1laIAAAAAL83SzgAAAAEAAAAAEXaATG7M1m6X+dTlq4QRcucmOLevhx0KDeGSAv4JVCgAAAADAAAAAUdPWAAAAAAABKsm6F8tIA+oSh66W+eRsark6vwOpTEAmTBZWMr03KsAAAAAAAAAG5D5a7sAAAF5AAB6EwAAAAAunSgoAAAAAQAAAAARdoBMbszWbpf51OWrhBFy5yY4t6+HHQoN4ZIC/glUKAAAAAMAAAABR1NEAAAAAABaFV24Jn/qJJ0UC1VZ+paf... (776 chars)"
}
```

Expected: `{"hash":"17d5a36823fc7538309804ecd7f7b1effeedfa85bc6de601c934973c5d049efb"}`

Actual: `{"hash":"fb4973f625fb43191a462b5711a8e9d8bcc77a4f79319add7f670154877877fa"}`

### `mainnet/tx_hash 236` on 16.0.0, 17.0.0

The SDK's hash differs from the one the network wrote

```json
{
  "depth_limit": 1000,
  "len_limit": 2236,
  "network_passphrase": "Public Global Stellar Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAABQAAAABSbyXCZQ9blfju2wIoB/2Xa5Tl4pWkj0W2Y8UvwihiwwAAAAAAAAZAAAAAAgAAAAAyOsZC9ILnAvDrM/vN1BaF9SjP393PIBuxWP13ktNyiAAABdwCNshtAAAIAQAAAAEAAAAAAAAAAAAAAABhS2xfAAAAAQAAABVVUERBVEVfT0ZGRVI6VEBBYTc1WTYAAAAAAAAPAAAAAQAAAAA8SjQlyT5i0Aj9AqymDNTFw0uVmRlNig8TELpJcJd2OQAAABAAAAAAMjrGQvSC5wLw6zP7zdQWhfUoz9/dzyAbsVj9d5LTcogAAAAAAAAADAAAAAJCVVNEMQAAAAAAAAAAAAAAkEMoGeZozjyBmO9GG83mbumISt0ZJIk73AnZVYKcwqEAAAABRVRIMQAAAACQQygZ5mjOPIGY70YbzeZu6YhK3RkkiTvcCdlVgpzCoQAAAAAAAAAOEcDzXwABhqAAAAAAMBng+gAAAAAAAAAMAAAAAkJVU0QxAAAA... (2984 chars)"
}
```

Expected: `{"hash":"005b9145af90eabf085cfb34b8ca16d525cb590b176417aafbf6e32116bf044d"}`

Actual: `{"hash":"4f845cff01a1422a331d17b8774daea7241fe8810a4210bf216272b8da352f3c"}`

### `mainnet/tx_hash 236 inner` on 16.0.0, 17.0.0

The SDK's hash differs from the one the network wrote

```json
{
  "depth_limit": 1000,
  "inner": true,
  "len_limit": 2236,
  "network_passphrase": "Public Global Stellar Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAABQAAAABSbyXCZQ9blfju2wIoB/2Xa5Tl4pWkj0W2Y8UvwihiwwAAAAAAAAZAAAAAAgAAAAAyOsZC9ILnAvDrM/vN1BaF9SjP393PIBuxWP13ktNyiAAABdwCNshtAAAIAQAAAAEAAAAAAAAAAAAAAABhS2xfAAAAAQAAABVVUERBVEVfT0ZGRVI6VEBBYTc1WTYAAAAAAAAPAAAAAQAAAAA8SjQlyT5i0Aj9AqymDNTFw0uVmRlNig8TELpJcJd2OQAAABAAAAAAMjrGQvSC5wLw6zP7zdQWhfUoz9/dzyAbsVj9d5LTcogAAAAAAAAADAAAAAJCVVNEMQAAAAAAAAAAAAAAkEMoGeZozjyBmO9GG83mbumISt0ZJIk73AnZVYKcwqEAAAABRVRIMQAAAACQQygZ5mjOPIGY70YbzeZu6YhK3RkkiTvcCdlVgpzCoQAAAAAAAAAOEcDzXwABhqAAAAAAMBng+gAAAAAAAAAMAAAAAkJVU0QxAAAA... (2984 chars)"
}
```

Expected: `{"hash":"aff7f0f89ed3de1a010d93d65fc9cfe77bd8241458575612614ca69eba7143f4"}`

Actual: `{"hash":"925311b1968c20cf9c846d479f141db878dd0cde93ad305b919278bfb7797c6b"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`, with the Rust toolchain of its `rust-toolchain.toml` and the toolchain `stellar-dotnet-sdk` builds with (`shapekit doctor sdk --sdk dotnet` lists what is missing):

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit conformance run --sdk dotnet --version 17.0.0 --entry cap03.offer-liabilities.mainnet-37466023
```

The run builds the SDK's testee at that version as `conformance/versions.toml` describes, then runs the entry's checks.

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-37466023/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-37466023/mainnet.xdr), envelope 12 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-37466023/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-37466023/mainnet.xdr), envelope 236 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-37466023/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-37466023/mainnet.xdr), envelope 236 of the transaction set: transactionHash of the fee bump's inner result pair

How the corpus entry was built and checked: Captured 2026-11-16 through the sep-54 public data lake from https://aws-public-blockchain.s3.us-east-2.amazonaws.com/v1.1/stellar/ledgers/pubnet/FDC4B5FF--37440000-37503999/FDC45058--37466023.xdr.zst. ledger 37466023 hash 29935e95979f372b9c98a448369396a419e0d66e96cd2f2997932e607268a71d, protocol 17, LedgerCloseMeta v0. the public lake holds history as re-emitted by a current core (TransactionMeta v4 for every protocol). the random audit found this ledger reaches @stellar/stellar-sdk code no corpus entry reaches, through the transaction hash path; kept as its witness until a recipe reproduces it

## Sibling issues

- `stellar_flutter_sdk` (Soneso/stellar_flutter_sdk) diverges on 5 of these entries: shapekit's finding DART-PRICE, a transaction whose offer price is not in lowest terms hashes to another value once the sdk rebuilds it (filed as https://github.com/Soneso/stellar_flutter_sdk/issues/296).
- `network.lightsail:stellar-sdk` (lightsail-network/java-stellar-sdk) diverges on 7 of these entries: no finding takes it yet.
- `soneso/stellar-php-sdk` (Soneso/stellar-php-sdk) diverges on 2 of these entries: shapekit's finding PHP-CBKEY-JSON, a claimable balance ledger key renders without its balance_id member in XDR-JSON (filed as https://github.com/Soneso/stellar-php-sdk/issues/240); PHP-HEXHASH, inner transaction hashes and config upgrade content hashes render as the hex of their hex in XDR-JSON, and their XDR-JSON does not encode (filed as https://github.com/Soneso/stellar-php-sdk/issues/241).
- `stellar-sdk` (StellarCN/py-stellar-base) diverges on 7 of these entries: no finding takes it yet.
- `stellar-cli` (stellar/stellar-cli) diverges on 4 of these entries: shapekit's finding CLI-TXHASH-V1, stellar tx hash refuses v0 and fee bump envelopes the network applied (covered upstream by https://github.com/stellar/stellar-cli/issues/2769).

shapekit vector: `cap03.offer-liabilities.mainnet-37466023`
