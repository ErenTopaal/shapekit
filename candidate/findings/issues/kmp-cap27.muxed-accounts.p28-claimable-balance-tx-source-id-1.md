<!-- repo: Soneso/kmp-stellar-sdk -->
# com.soneso.stellar:stellar-sdk-jvm 1.18.0: the sdk's hash differs from the one the network wrote (cap27.muxed-accounts.p28-claimable-balance-tx-source-id-1)

## What happens

On the input below, `com.soneso.stellar:stellar-sdk-jvm` 1.18.0 answers differently from the Stellar network in 2 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap27.muxed-accounts.p28-claimable-balance-tx-source-id-1` (family `cap27.muxed-accounts`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `testnet/tx_hash 0` on 1.18.0

the sdk's hash differs from the one the network wrote

```json
{
  "depth_limit": 1000,
  "len_limit": 220,
  "network_passphrase": "Test SDF Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAgAAAQAAAAAAAAAAAZoTxr4wGZDC/9ku5hA3QE1JlHDk7Cda8ORYLhHbJ1NaAAAAZAAAAEAAAAAIAAAAAAAAAAAAAAABAAAAAAAAAA4AAAAAAAAAAACYloAAAAABAAAAAAAAAABSpOQ5Hjzo6xpcSTqRRnbDTRoEupwXlZqHzS8Er14vtQAAAAAAAAAAAAAAAdsnU1oAAABA8AQlsI4WqTrcTJKg3RaDraImC2r/Svq/lgBkcLCra87fiubvXkQeBKVpUIbIgJdh11f5378yUQLzReOAuUxlBw=="
}
```

Expected: `{"hash":"76a94147c2cbc7a339355e75a1667d39e837e001071f124dc1db3d55c6e0b7eb"}`

Actual: `{"hash":"8a72e4af63445ebf64594254879f32784fdce6122e170cbbed0d23d523551526"}`

### `mainnet/tx_hash 0` on 1.18.0

the sdk's hash differs from the one the network wrote

```json
{
  "depth_limit": 1000,
  "len_limit": 220,
  "network_passphrase": "Public Global Stellar Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAgAAAQAAAAAAAAAAAZoTxr4wGZDC/9ku5hA3QE1JlHDk7Cda8ORYLhHbJ1NaAAAAZAAAAEAAAAAIAAAAAAAAAAAAAAABAAAAAAAAAA4AAAAAAAAAAACYloAAAAABAAAAAAAAAABSpOQ5Hjzo6xpcSTqRRnbDTRoEupwXlZqHzS8Er14vtQAAAAAAAAAAAAAAAdsnU1oAAABAbvLDG0o9BBPBq6J1gh5KjrriS7p8joK04zFC44ddNPn0HQwE5XmCzIfDVPqFR8v0o7ary5Q5skpwrOWYWEOQBg=="
}
```

Expected: `{"hash":"24992af14dd04b864dd1ceffb99eee87618890c6680a923beed618f55362e92e"}`

Actual: `{"hash":"00f644f7450bb8bff9d904b889ae2fa8a89818a842aa67b1811f02082b941be6"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk kmp --version 1.18.0 --entry cap27.muxed-accounts.p28-claimable-balance-tx-source-id-1
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap27.muxed-accounts/cap27.muxed-accounts.p28-claimable-balance-tx-source-id-1/mainnet.xdr,`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap27.muxed-accounts/cap27.muxed-accounts.p28-claimable-balance-tx-source-id-1/mainnet.xdr,) envelope 0 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap27.muxed-accounts/cap27.muxed-accounts.p28-claimable-balance-tx-source-id-1/testnet.xdr,`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap27.muxed-accounts/cap27.muxed-accounts.p28-claimable-balance-tx-source-id-1/testnet.xdr,) envelope 0 of the transaction set: transactionHash of the envelope's result pair

How the corpus entry was built and checked: a claimable balance created by a transaction whose source is a muxed account with id 1

Related: https://github.com/stellar/wallet-backend/issues/716

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap27.muxed-accounts.p28-claimable-balance-tx-source-id-1`
