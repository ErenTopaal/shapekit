<!-- repo: Soneso/stellar-php-sdk -->
# soneso/stellar-php-sdk 1.18.0: the sdk's hash differs from the one the network wrote (explore.witnesses.g2-r0015-l71)

## What happens

On the input below, `soneso/stellar-php-sdk` 1.18.0 answers differently from the Stellar network in 2 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `explore.witnesses.g2-r0015-l71` (family `explore.witnesses`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `testnet/tx_hash 0` on 1.18.0

the sdk's hash differs from the one the network wrote

```json
{
  "depth_limit": 1000,
  "len_limit": 356,
  "network_passphrase": "Test SDF Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAAHPfPMKl1Cf3tp/iWY9jfYse3nBlX2pVa6t5lRCid8KAAABLAAAAEAAAAADAAAAAAAAAAEAAAAAAAAAAwAAAAAAAAAHAAAAAO17/F0lOid71HUBq+lVCPZUN9QVqoKjhSYSrWDp8dWHAAAAAVVTRAAAAAABAAAAAQAAAAABz3zzCpdQn97af4lmPY32LHt5wZV9qVWureZUQonfCgAAAAcAAAAAAc988wqXUJ/e2n+JZj2N9ix7ecGVfalVrq3mVEKJ3woAAAACTE9OR0FTU0VUMTIAAAAAAQAAAAAAAAAFAAAAAAAAAAAAAAAAAAAAAQAAAP8AAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAFCid8KAAAAQKbfMCOkZBIfMlQ7DtRjigHN6N8+p+cWMS403kE6GGRcBHbJ8GfPxGEX+mdqdMCXdvPriDjkMsFMsoYvnAVURAQ="
}
```

Expected: `{"hash":"f191a7d5f93a78d79b3cdf0631da286c8a54f7b38400cde9290a6865929e695c"}`

Actual: `{"hash":"80cf2c97022222b22f29d78d9fcbc3a38ae671f7263658cadd493fc499432a01"}`

### `mainnet/tx_hash 0` on 1.18.0

the sdk's hash differs from the one the network wrote

```json
{
  "depth_limit": 1000,
  "len_limit": 356,
  "network_passphrase": "Public Global Stellar Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAAHPfPMKl1Cf3tp/iWY9jfYse3nBlX2pVa6t5lRCid8KAAABLAAAAEAAAAADAAAAAAAAAAEAAAAAAAAAAwAAAAAAAAAHAAAAAO17/F0lOid71HUBq+lVCPZUN9QVqoKjhSYSrWDp8dWHAAAAAVVTRAAAAAABAAAAAQAAAAABz3zzCpdQn97af4lmPY32LHt5wZV9qVWureZUQonfCgAAAAcAAAAAAc988wqXUJ/e2n+JZj2N9ix7ecGVfalVrq3mVEKJ3woAAAACTE9OR0FTU0VUMTIAAAAAAQAAAAAAAAAFAAAAAAAAAAAAAAAAAAAAAQAAAP8AAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAFCid8KAAAAQBQUx4YLt7VNTdjJGkgfnUnVG1sJAYtoRwFiUHks7T5ZAckxl5CA8vQ/DfFIb8vDMb6blKeL3zqmxMiRzao3uAk="
}
```

Expected: `{"hash":"855d3afef6c40c225c4f1149ebf34a1f8888b1c1da7a69dcaf94b32e2dd8c93d"}`

Actual: `{"hash":"35ec280266647d0c53d75198cb4fec37dc19a8b225e709d7cecb9d552aa54615"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`:

```sh
shapekit conformance run --sdk php --version 1.18.0 --entry explore.witnesses.g2-r0015-l71
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/explore.witnesses/explore.witnesses.g2-r0015-l71/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/explore.witnesses/explore.witnesses.g2-r0015-l71/mainnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/explore.witnesses/explore.witnesses.g2-r0015-l71/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/explore.witnesses/explore.witnesses.g2-r0015-l71/testnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair

How the corpus entry was built and checked: A classic recipe of the explorer's run g2 (seed 202). ledger 71 of chain explore.g2.r0015 at protocol 10, recorded from corpus/recipes/explore.witnesses/g2/r0015.json; picked for the arms it holds that no earlier pick holds: AllowTrustResult:self_not_allowed, AllowTrustResult:trust_not_required

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `explore.witnesses.g2-r0015-l71`
