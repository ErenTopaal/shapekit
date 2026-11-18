<!-- repo: Soneso/stellar_flutter_sdk -->
# stellar_flutter_sdk 3.12.0: the sdk's hash differs from the one the network wrote (explore.witnesses.f62d-r0000-l96)

## What happens

On the input below, `stellar_flutter_sdk` 3.12.0 answers differently from the Stellar network in 2 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `explore.witnesses.f62d-r0000-l96` (family `explore.witnesses`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `testnet/tx_hash 0` on 3.12.0

the sdk's hash differs from the one the network wrote

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

the sdk's hash differs from the one the network wrote

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

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`:

```sh
shapekit conformance run --sdk dart --version 3.12.0 --entry explore.witnesses.f62d-r0000-l96
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/explore.witnesses/explore.witnesses.f62d-r0000-l96/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/explore.witnesses/explore.witnesses.f62d-r0000-l96/mainnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/explore.witnesses/explore.witnesses.f62d-r0000-l96/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/explore.witnesses/explore.witnesses.f62d-r0000-l96/testnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair

How the corpus entry was built and checked: A classic recipe of the explorer's run f62d (seed 65). ledger 96 of chain explore.f62d.r0000 at protocol 30, recorded from corpus/recipes/explore.witnesses/f62d/r0000.json; picked for the arms it holds that no earlier pick holds: OperationBody:path_payment_strict_receive, OperationResultTr:path_payment_strict_receive, PathPaymentStrictReceiveResult:no_trust

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `explore.witnesses.f62d-r0000-l96`
