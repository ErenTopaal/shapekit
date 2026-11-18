<!-- repo: Beans-BV/dotnet-stellar-sdk -->
# stellar-dotnet-sdk 16.0.0, 17.0.0: the sdk's hash differs from the one the network wrote (cap03.offer-liabilities.mainnet-37466023)

## What happens

On the input below, `stellar-dotnet-sdk` 16.0.0, 17.0.0 answers differently from the Stellar network in 8 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap03.offer-liabilities.mainnet-37466023` (family `cap03.offer-liabilities`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `mainnet/tx_hash 12` on 16.0.0, 17.0.0

the sdk's hash differs from the one the network wrote

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

### `mainnet/tx_hash 108` on 16.0.0, 17.0.0

the sdk refused a value the network accepts: NotSupportedException: AllowTrustOperation is no longer supported.

```json
{
  "depth_limit": 1000,
  "len_limit": 8320,
  "network_passphrase": "Public Global Stellar Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAgAAAABl4UoXD/IRhWi4OexpQcfNPlLKU9GlQQ2zhhhbgt/qzQBqz8ABw+mZAASa8gAAAAEAAAAAAAAAAAAAAABhS21PAAAAAAAAAEYAAAABAAAAAGIzyyvmVORp86MZ3bdtQLNxddfaniHyw+NNy5hBFrINAAAABwAAAABl4UoXD/IRhWi4OexpQcfNPlLKU9GlQQ2zhhhbgt/qzQAAAAFUU0xBAAAAAQAAAAEAAAAAYjPLK+ZU5Gnzoxndt21As3F119qeIfLD403LmEEWsg0AAAAHAAAAAGXhShcP8hGFaLg57GlBx80+UspT0aVBDbOGGFuC3+rNAAAAAUFNWk4AAAABAAAAAQAAAABiM8sr5lTkafOjGd23bUCzcXXX2p4h8sPjTcuYQRayDQAAAAcAAAAAZeFKFw/yEYVouDnsaUHHzT5SylPRpUENs4YYW4Lf6s0AAAACU1A1MDAAAAAAAAAAAAAAAQAAAAEAAAAAYjPLK+ZU5Gnzoxnd... (11096 chars)"
}
```

Expected: `{"hash":"d36de74ac821ce1addee8afaf971e05e9e56f889007d8d103c64c5bd2d4fb8da"}`

Actual: `{"rejected":"NotSupportedException: AllowTrustOperation is no longer supported."}`

### `mainnet/tx_hash 236` on 16.0.0, 17.0.0

the sdk's hash differs from the one the network wrote

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

the sdk's hash differs from the one the network wrote

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

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`:

```sh
shapekit conformance run --sdk dotnet --version 16.0.0 --entry cap03.offer-liabilities.mainnet-37466023
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-37466023/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-37466023/mainnet.xdr), envelope 108 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-37466023/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-37466023/mainnet.xdr), envelope 12 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-37466023/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-37466023/mainnet.xdr), envelope 236 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-37466023/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap03.offer-liabilities/cap03.offer-liabilities.mainnet-37466023/mainnet.xdr), envelope 236 of the transaction set: transactionHash of the fee bump's inner result pair

How the corpus entry was built and checked: captured 2026-11-16 through the sep-54 public data lake from https://aws-public-blockchain.s3.us-east-2.amazonaws.com/v1.1/stellar/ledgers/pubnet/FDC4B5FF--37440000-37503999/FDC45058--37466023.xdr.zst. ledger 37466023 hash 29935e95979f372b9c98a448369396a419e0d66e96cd2f2997932e607268a71d, protocol 17, LedgerCloseMeta v0. the public lake holds history as re-emitted by a current core (TransactionMeta v4 for every protocol). the random audit found this ledger reaches @stellar/stellar-sdk code no corpus entry reaches, through the transaction hash path; kept as its witness until a recipe reproduces it

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap03.offer-liabilities.mainnet-37466023`
