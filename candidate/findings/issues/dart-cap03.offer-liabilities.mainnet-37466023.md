<!-- repo: Soneso/stellar_flutter_sdk -->
# stellar_flutter_sdk 3.12.0: a transaction whose offer price is not in lowest terms hashes to another value once the sdk rebuilds it

## What happens

A transaction whose offer price is not in lowest terms hashes to another value once the sdk rebuilds it.

## Cause

AbstractTransaction.fromEnvelopeXdr(..).hash(network) rebuilds every operation before hashing. ManageSellOfferOperation builder() turns the XDR price into Price(n, d).toDecimalString() and toOperationBody reads it back with Price.fromString (lib/src/manage_sell_offer_operation.dart L109 and L135-L143 at 3.12.0), which since PR #196 recovers the fraction exactly but in lowest terms (lib/src/price.dart L185-L195), so it writes the reduced fraction and an offer or liquidity pool deposit bound such as 6836306/1144362402, 7/7 or 2147483647/2147483647 is hashed as another fraction (lib/src/transaction.dart L108-L110). ManageBuyOfferOperation (L110, L144), CreatePassiveSellOfferOperation (L94, L124) and LiquidityPoolDepositOperation (L135-L136, L165-L168) take the same round trip.

## Affected versions

`stellar_flutter_sdk` 3.12.0, every version shapekit ran.

## Entries

shapekit's finding DART-PRICE takes 6 corpus entries on this SDK: `cap03.offer-liabilities.mainnet-37466023`, `cap15.fee-bump.mainnet-57369500`, `explore.witnesses.g1-r0047-l69`, `explore.witnesses.g2-r0007-l69`, `explore.witnesses.g2-r0010-l92`, `explore.witnesses.g2-r0038-l67`. The checks below are those of `cap03.offer-liabilities.mainnet-37466023`.

## Checks on `cap03.offer-liabilities.mainnet-37466023`

On this entry, `stellar_flutter_sdk` 3.12.0 answers differently from the Stellar network in 3 checks on 1 version.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

Corpus entry `cap03.offer-liabilities.mainnet-37466023` (family `cap03.offer-liabilities`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `mainnet/tx_hash 12` on 3.12.0

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

### `mainnet/tx_hash 236` on 3.12.0

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

### `mainnet/tx_hash 236 inner` on 3.12.0

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

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`, with the Rust toolchain of its `rust-toolchain.toml` and the toolchain `stellar_flutter_sdk` builds with (`shapekit doctor sdk --sdk dart` lists what is missing):

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit conformance run --sdk dart --version 3.12.0 --entry cap03.offer-liabilities.mainnet-37466023
```

The run builds the SDK's testee at that version as `conformance/versions.toml` describes, then runs the entry's checks.

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-37466023/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-37466023/mainnet.xdr), envelope 12 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-37466023/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-37466023/mainnet.xdr), envelope 236 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-37466023/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-37466023/mainnet.xdr), envelope 236 of the transaction set: transactionHash of the fee bump's inner result pair

How the corpus entry was built and checked: Captured 2026-11-16 through the sep-54 public data lake from https://aws-public-blockchain.s3.us-east-2.amazonaws.com/v1.1/stellar/ledgers/pubnet/FDC4B5FF--37440000-37503999/FDC45058--37466023.xdr.zst. ledger 37466023 hash 29935e95979f372b9c98a448369396a419e0d66e96cd2f2997932e607268a71d, protocol 17, LedgerCloseMeta v0. the public lake holds history as re-emitted by a current core (TransactionMeta v4 for every protocol). the random audit found this ledger reaches @stellar/stellar-sdk code no corpus entry reaches, through the transaction hash path; kept as its witness until a recipe reproduces it

## Sibling issues

- `stellar-dotnet-sdk` (Beans-BV/dotnet-stellar-sdk) diverges on 5 of these entries: shapekit's finding NET-DEPRECATED-OPS, Transaction.FromEnvelopeXdr refuses envelopes with an allow trust or inflation operation (not filed yet); NET-PRICE, a transaction whose offer price the sdk does not get back from a decimal hashes to another value (not filed yet).
- `network.lightsail:stellar-sdk` (lightsail-network/java-stellar-sdk) diverges on 6 of these entries: no finding takes it yet.
- `soneso/stellar-php-sdk` (Soneso/stellar-php-sdk) diverges on 2 of these entries: shapekit's finding PHP-CBKEY-JSON, a claimable balance ledger key renders without its balance_id member in XDR-JSON (not filed yet); PHP-HEXHASH, inner transaction hashes and config upgrade content hashes render as the hex of their hex in XDR-JSON, and their XDR-JSON does not encode (not filed yet).
- `stellar-sdk` (StellarCN/py-stellar-base) diverges on 6 of these entries: no finding takes it yet.
- `stellar-cli` (stellar/stellar-cli) diverges on 4 of these entries: shapekit's finding CLI-TXHASH-V1, stellar tx hash refuses v0 and fee bump envelopes the network applied (covered upstream by https://github.com/stellar/stellar-cli/issues/2769).

shapekit vector: `cap03.offer-liabilities.mainnet-37466023`
