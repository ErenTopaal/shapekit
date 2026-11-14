<!-- repo: stellar/go-stellar-sdk -->
# github.com/stellar/go-stellar-sdk v0.7.1: the sdk orders the pair differently from core (cap38.liquidity-pools.synth-assets-py1246-testnet-pool)

## What happens

On the input below, `github.com/stellar/go-stellar-sdk` v0.7.1 answers differently from the Stellar network in 4 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap38.liquidity-pools.synth-assets-py1246-testnet-pool` (family `cap38.liquidity-pools`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `case 0/asset_order` on v0.7.1

the sdk orders the pair differently from core

```json
{
  "a": "AAAAAVVTRAAAAAAA96TA5bLRFwmw77hliDpn58MyAH9oo7Kz9x0ESsvSz4U=",
  "b": "AAAAAVVTRAAAAAAA4wX4hf/0IDWYv3p7spz23LBfnsxE8VpcVVfvZlwOAfc=",
  "op": "asset_order"
}
```

Expected: `{"order":1}`

Actual: `{"order":-1}`

### `case 1/asset_order` on v0.7.1

the sdk orders the pair differently from core

```json
{
  "a": "AAAAAVVTRAAAAAAA4wX4hf/0IDWYv3p7spz23LBfnsxE8VpcVVfvZlwOAfc=",
  "b": "AAAAAVVTRAAAAAAA96TA5bLRFwmw77hliDpn58MyAH9oo7Kz9x0ESsvSz4U=",
  "op": "asset_order"
}
```

Expected: `{"order":-1}`

Actual: `{"order":1}`

### `case 2/pool_id` on v0.7.1

the sdk accepted a value the network refuses

```json
{
  "a": "AAAAAVVTRAAAAAAA96TA5bLRFwmw77hliDpn58MyAH9oo7Kz9x0ESsvSz4U=",
  "b": "AAAAAVVTRAAAAAAA4wX4hf/0IDWYv3p7spz23LBfnsxE8VpcVVfvZlwOAfc=",
  "fee": 30,
  "op": "pool_id"
}
```

Expected: `"rejected"`

Actual: `{"pool_id":"c1324f878b3d11af9f923abb7d692fc74e0a7bcfd9a4edefd564efe5fdae9fdb"}`

### `case 3/pool_id` on v0.7.1

the sdk refused a value the network accepts: xdr.NewPoolId: AssetA must be < AssetB

```json
{
  "a": "AAAAAVVTRAAAAAAA4wX4hf/0IDWYv3p7spz23LBfnsxE8VpcVVfvZlwOAfc=",
  "b": "AAAAAVVTRAAAAAAA96TA5bLRFwmw77hliDpn58MyAH9oo7Kz9x0ESsvSz4U=",
  "fee": 30,
  "op": "pool_id"
}
```

Expected: `{"pool_id":"c2a730127d006897b021d6fcb279da68b4f65f9b6a408c74eb2cdd85ea862d5e"}`

Actual: `{"rejected":"xdr.NewPoolId: AssetA must be < AssetB"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk go --version v0.7.1 --entry cap38.liquidity-pools.synth-assets-py1246-testnet-pool
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/vectors/ordering/cap38.liquidity-pools.synth-assets-py1246-testnet-pool.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/vectors/ordering/cap38.liquidity-pools.synth-assets-py1246-testnet-pool.json) cases 0, 1, 2, 3

How the corpus entry was built and checked: the py-stellar-base#1246 pair from testnet pool c2a730127d006897b021d6fcb279da68b4f65f9b6a408c74eb2cdd85ea862d5e (transaction 0126748091930959a3aefac9296e1773b947f964a9b73f37ce856c4df32d346f, ledger 5521432). the pool id computed from the pair in core order equals the real pool id, which the build checks. expected order computed with core's rule (asset type, code bytes with zero padding, issuer key bytes; isPoolShareAssetValid in stellar-core v30.0.0 src/util/types.cpp and the xdrpp union and xarray comparisons it uses), checked against stellar-xdr's derived Ord. pool_id cases expect the id core derives (sha256 of LiquidityPoolParameters with fee 30) when a < b, and a refusal otherwise, because core answers CHANGE_TRUST_MALFORMED for parameters that are not strictly ordered.

Related: https://github.com/StellarCN/py-stellar-base/issues/1246

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap38.liquidity-pools.synth-assets-py1246-testnet-pool`
