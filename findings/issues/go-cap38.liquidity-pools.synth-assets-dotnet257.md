<!-- repo: stellar/go-stellar-sdk -->
# github.com/stellar/go-stellar-sdk v0.7.1: the sdk orders the pair differently from core (cap38.liquidity-pools.synth-assets-dotnet257)

## What happens

`github.com/stellar/go-stellar-sdk` v0.7.1 answers 4 of the checks of corpus entry `cap38.liquidity-pools.synth-assets-dotnet257` differently from the Stellar network.

## Minimal vector

Entry `cap38.liquidity-pools.synth-assets-dotnet257` (kind Ordering, family `cap38.liquidity-pools`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `case 0/asset_order` on v0.7.1

the sdk orders the pair differently from core

```json
{
  "a": "AAAAAVVTRAAAAAAAamr04C6RvjyT1+KuqUFk1akfTt+6AQhsDKXOqs81tUA=",
  "b": "AAAAAVVTRAAAAAAAe56/kfAB87NhvYs2vShaC9WJlfqAGOjWvGQZYxqDIL0=",
  "op": "asset_order"
}
```

Expected: `{"order":-1}`

Actual: `{"order":1}`

### `case 1/asset_order` on v0.7.1

the sdk orders the pair differently from core

```json
{
  "a": "AAAAAVVTRAAAAAAAe56/kfAB87NhvYs2vShaC9WJlfqAGOjWvGQZYxqDIL0=",
  "b": "AAAAAVVTRAAAAAAAamr04C6RvjyT1+KuqUFk1akfTt+6AQhsDKXOqs81tUA=",
  "op": "asset_order"
}
```

Expected: `{"order":1}`

Actual: `{"order":-1}`

### `case 2/pool_id` on v0.7.1

the sdk refused a value the network accepts: xdr.NewPoolId: AssetA must be < AssetB

```json
{
  "a": "AAAAAVVTRAAAAAAAamr04C6RvjyT1+KuqUFk1akfTt+6AQhsDKXOqs81tUA=",
  "b": "AAAAAVVTRAAAAAAAe56/kfAB87NhvYs2vShaC9WJlfqAGOjWvGQZYxqDIL0=",
  "fee": 30,
  "op": "pool_id"
}
```

Expected: `{"pool_id":"52e8a2eb6dab5a256f7020cf2e68807c0d53928853854cd4601591681ab1772e"}`

Actual: `{"rejected":"xdr.NewPoolId: AssetA must be < AssetB"}`

### `case 3/pool_id` on v0.7.1

the sdk accepted a value the network refuses

```json
{
  "a": "AAAAAVVTRAAAAAAAe56/kfAB87NhvYs2vShaC9WJlfqAGOjWvGQZYxqDIL0=",
  "b": "AAAAAVVTRAAAAAAAamr04C6RvjyT1+KuqUFk1akfTt+6AQhsDKXOqs81tUA=",
  "fee": 30,
  "op": "pool_id"
}
```

Expected: `"rejected"`

Actual: `{"pool_id":"5d7ed1b0a01d73c36620cfdf3f680ab924aaf9985d5de112e53e901bc8726059"}`

## Reproduce

```sh
shapekit conformance run --sdk go --version v0.7.1 --entry cap38.liquidity-pools.synth-assets-dotnet257
```

## Expected value and its source

- corpus/vectors/ordering/cap38.liquidity-pools.synth-assets-dotnet257.json case 0
- corpus/vectors/ordering/cap38.liquidity-pools.synth-assets-dotnet257.json case 1
- corpus/vectors/ordering/cap38.liquidity-pools.synth-assets-dotnet257.json case 2
- corpus/vectors/ordering/cap38.liquidity-pools.synth-assets-dotnet257.json case 3

Entry notes: the Beans-BV/dotnet-stellar-sdk#257 issuer pair (the issue is open at the time of writing). the issue names only the issuers; the code USD is the same on both sides so the issuer decides. expected order computed with core's rule (asset type, code bytes with zero padding, issuer key bytes; isPoolShareAssetValid in stellar-core v30.0.0 src/util/types.cpp and the xdrpp union and xarray comparisons it uses), checked against stellar-xdr's derived Ord. pool_id cases expect the id core derives (sha256 of LiquidityPoolParameters with fee 30) when a < b, and a refusal otherwise, because core answers CHANGE_TRUST_MALFORMED for parameters that are not strictly ordered.

Related: https://github.com/Beans-BV/dotnet-stellar-sdk/issues/257

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `cap38.liquidity-pools.synth-assets-dotnet257`
