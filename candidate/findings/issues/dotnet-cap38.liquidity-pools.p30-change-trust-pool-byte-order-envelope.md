<!-- repo: Beans-BV/dotnet-stellar-sdk -->
# stellar-dotnet-sdk 17.0.0: the sdk refused a value the network accepts: ArgumentException: Asset A must be < Asset B (Lexicographic Order) (cap38.liquidity-pools.p30-change-trust-pool-byte-order-envelope)

## What happens

On the input below, `stellar-dotnet-sdk` 17.0.0 answers differently from the Stellar network in 2 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap38.liquidity-pools.p30-change-trust-pool-byte-order-envelope` (family `cap38.liquidity-pools`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `testnet/tx_hash` on 17.0.0

the sdk refused a value the network accepts: ArgumentException: Asset A must be < Asset B (Lexicographic Order).

```json
{
  "depth_limit": 1000,
  "len_limit": 260,
  "network_passphrase": "Test SDF Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAgAAAACnbAvXIoAunCttD+bL9QfFluTBpqPy0kzU7LaYMd+/MwAAAGQAAABAAAAAAgAAAAAAAAAAAAAAAQAAAAAAAAAGAAAAAwAAAAAAAAABVVNEAAAAAAAwveUwyxoo3Bs7b+tMaqVvAdtgl078r50nLPahdtAizQAAAAFVU0QAAAAAAD3mKj0hnIyMl270um6OzPIdrt6oMwVK5s/2AckUIUmuAAAAHn//////////AAAAAAAAAAEx378zAAAAQLai28UAy5rQQRMlSjUapVEIUrTob/oZNjCxlYuU1S/DHeq9wIFenPxfzys9qFpE8pEnRTifkuqGKGWNcHts8wE="
}
```

Expected: `{"hash":"2f145dc968cee53d5998708bcbbe2a4c5c57190646e459c270334c381721ac43"}`

Actual: `{"rejected":"ArgumentException: Asset A must be < Asset B (Lexicographic Order)."}`

### `mainnet/tx_hash` on 17.0.0

the sdk refused a value the network accepts: ArgumentException: Asset A must be < Asset B (Lexicographic Order).

```json
{
  "depth_limit": 1000,
  "len_limit": 260,
  "network_passphrase": "Public Global Stellar Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAgAAAACnbAvXIoAunCttD+bL9QfFluTBpqPy0kzU7LaYMd+/MwAAAGQAAABAAAAAAgAAAAAAAAAAAAAAAQAAAAAAAAAGAAAAAwAAAAAAAAABVVNEAAAAAAAwveUwyxoo3Bs7b+tMaqVvAdtgl078r50nLPahdtAizQAAAAFVU0QAAAAAAD3mKj0hnIyMl270um6OzPIdrt6oMwVK5s/2AckUIUmuAAAAHn//////////AAAAAAAAAAEx378zAAAAQEB7cnfL2z4FCA3VI/CVOGAO5xqNAHCrcf1/BtEHhrgVbRUx5GSduutqV5G+A9C3nRaypKQ+Qud0QVjeYf+yoQw="
}
```

Expected: `{"hash":"576f099ef97bda8527d8ea2bf70a454271c360ee686519ed555fec744aed5286"}`

Actual: `{"rejected":"ArgumentException: Asset A must be < Asset B (Lexicographic Order)."}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk dotnet --version 17.0.0 --entry cap38.liquidity-pools.p30-change-trust-pool-byte-order-envelope
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap38.liquidity-pools/cap38.liquidity-pools.p30-change-trust-pool-byte-order-envelope/mainnet.xdr,`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap38.liquidity-pools/cap38.liquidity-pools.p30-change-trust-pool-byte-order-envelope/mainnet.xdr,) applied in ledger 70 of chain cap38.liquidity-pools.p30: transactionHash of the envelope's result pair
- [`corpus/entries/cap38.liquidity-pools/cap38.liquidity-pools.p30-change-trust-pool-byte-order-envelope/testnet.xdr,`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap38.liquidity-pools/cap38.liquidity-pools.p30-change-trust-pool-byte-order-envelope/testnet.xdr,) applied in ledger 70 of chain cap38.liquidity-pools.p30: transactionHash of the envelope's result pair

How the corpus entry was built and checked: a pool-share trustline whose asset a is USD:GAYL3ZJQZMNCRXA3HNX6WTDKUVXQDW3AS5HPZL45E4WPNILW2ARM3JBS and asset b is USD:GA66MKR5EGOIZDEXN32LU3UOZTZB3LW6VAZQKSXGZ73ADSIUEFE24Y5C: ordered by issuer key bytes, which is the reverse of their strkey order

Related: https://github.com/stellar/js-stellar-sdk/pull/1766, https://github.com/StellarCN/py-stellar-base/issues/1246, https://github.com/Beans-BV/dotnet-stellar-sdk/issues/257

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap38.liquidity-pools.p30-change-trust-pool-byte-order-envelope`
