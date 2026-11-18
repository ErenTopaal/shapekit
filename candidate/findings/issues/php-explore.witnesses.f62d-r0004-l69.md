<!-- repo: Soneso/stellar-php-sdk -->
# soneso/stellar-php-sdk 1.18.0: the sdk refused a value the network accepts: InvalidArgumentException: Invalid offer id: -1 (explore.witnesses.f62d-r0004-l69)

## What happens

On the input below, `soneso/stellar-php-sdk` 1.18.0 answers differently from the Stellar network in 2 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `explore.witnesses.f62d-r0004-l69` (family `explore.witnesses`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `testnet/tx_hash 0` on 1.18.0

the sdk refused a value the network accepts: InvalidArgumentException: Invalid offer id: -1

```json
{
  "depth_limit": 1000,
  "len_limit": 228,
  "network_passphrase": "Test SDF Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAGfCkKADrIUPUvc+CgLveti1zPocODhBQUZ9T6Rp6ARfAAAAZAAAAEAAAAABAAAAAAAAAAAAAAABAAAAAAAAAAMAAAACTE9OR0FTU0VUMTIAAAAAAGfCkKADrIUPUvc+CgLveti1zPocODhBQUZ9T6Rp6ARfAAAAAAAAAOjUpRAAAAAAAQAAAAH//////////wAAAAAAAAABaegEXwAAAEAJhNSpNHRocZ2Ukgm5MOvYB9uPDfE3ScHZw7qLEfwSQCrLH5cnecYdP938XXAjWbjmSGRJ9k99M9yY7nMKJGoM"
}
```

Expected: `{"hash":"b6753b14c03b8bbbbd6fad75bfd26b5a97169fcf7b04ba596dc2205560b7cf7f"}`

Actual: `{"rejected":"InvalidArgumentException: Invalid offer id: -1"}`

### `mainnet/tx_hash 0` on 1.18.0

the sdk refused a value the network accepts: InvalidArgumentException: Invalid offer id: -1

```json
{
  "depth_limit": 1000,
  "len_limit": 228,
  "network_passphrase": "Public Global Stellar Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAGfCkKADrIUPUvc+CgLveti1zPocODhBQUZ9T6Rp6ARfAAAAZAAAAEAAAAABAAAAAAAAAAAAAAABAAAAAAAAAAMAAAACTE9OR0FTU0VUMTIAAAAAAGfCkKADrIUPUvc+CgLveti1zPocODhBQUZ9T6Rp6ARfAAAAAAAAAOjUpRAAAAAAAQAAAAH//////////wAAAAAAAAABaegEXwAAAEBVj1Qk6YFxflxeH8AgUKb2q170yOASmVHVdWL6n8qTjnCNa7QeExM6q8K/Npni2bA6kt4t1Z2nJZOxZS0WQ5YK"
}
```

Expected: `{"hash":"b9657ea7a84f6e3e8b3d5bc2df06b98bf3a56c5c8401d07876dd8222cb9cffe4"}`

Actual: `{"rejected":"InvalidArgumentException: Invalid offer id: -1"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`:

```sh
shapekit conformance run --sdk php --version 1.18.0 --entry explore.witnesses.f62d-r0004-l69
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/explore.witnesses/explore.witnesses.f62d-r0004-l69/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/explore.witnesses/explore.witnesses.f62d-r0004-l69/mainnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/explore.witnesses/explore.witnesses.f62d-r0004-l69/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/explore.witnesses/explore.witnesses.f62d-r0004-l69/testnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair

How the corpus entry was built and checked: A classic recipe of the explorer's run f62d (seed 65). ledger 69 of chain explore.f62d.r0004 at protocol 9, recorded from corpus/recipes/explore.witnesses/f62d/r0004.json; picked for the arms it holds that no earlier pick holds: ManageSellOfferResult:not_found, OperationBody:manage_sell_offer

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `explore.witnesses.f62d-r0004-l69`
