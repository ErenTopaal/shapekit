<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.3.0, 17.2.0: the sdk orders the pair differently from core (cap38.liquidity-pools.synth-assets-js1766-issuers)

## What happens

`@stellar/stellar-sdk` 16.3.0, 17.2.0 answers 8 of the checks of corpus entry `cap38.liquidity-pools.synth-assets-js1766-issuers` differently from the Stellar network.

## Minimal vector

Entry `cap38.liquidity-pools.synth-assets-js1766-issuers` (kind Ordering, family `cap38.liquidity-pools`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `case 0/asset_order` on 16.3.0, 17.2.0

the sdk orders the pair differently from core

```json
{
  "a": "AAAAAVVTRAAAAAAAOjo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo=",
  "b": "AAAAAVVTRAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
  "op": "asset_order"
}
```

Expected: `{"order":1}`

Actual: `{"order":-1}`

### `case 1/asset_order` on 16.3.0, 17.2.0

the sdk orders the pair differently from core

```json
{
  "a": "AAAAAVVTRAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
  "b": "AAAAAVVTRAAAAAAAOjo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo=",
  "op": "asset_order"
}
```

Expected: `{"order":-1}`

Actual: `{"order":1}`

### `case 2/pool_id` on 16.3.0, 17.2.0

the sdk accepted a value the network refuses

```json
{
  "a": "AAAAAVVTRAAAAAAAOjo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo=",
  "b": "AAAAAVVTRAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
  "fee": 30,
  "op": "pool_id"
}
```

Expected: `"rejected"`

Actual: `{"pool_id":"fa3690a8f1055b0d0eb270639ad24d1e2ee10966790efaa997a1fc7e5114699f"}`

### `case 3/pool_id` on 16.3.0, 17.2.0

the sdk refused a value the network accepts: Error: Assets are not in lexicographic order

```json
{
  "a": "AAAAAVVTRAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
  "b": "AAAAAVVTRAAAAAAAOjo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo=",
  "fee": 30,
  "op": "pool_id"
}
```

Expected: `{"pool_id":"e041ee6dd27f1bab553b46c9f7eb00ea83edfe3d5a71f3ac0551c58eb4a14e99"}`

Actual: `{"rejected":"Error: Assets are not in lexicographic order"}`

## Reproduce

```sh
shapekit conformance run --sdk js --version 16.3.0 --entry cap38.liquidity-pools.synth-assets-js1766-issuers
```

## Expected value and its source

- corpus/vectors/ordering/cap38.liquidity-pools.synth-assets-js1766-issuers.json case 0
- corpus/vectors/ordering/cap38.liquidity-pools.synth-assets-js1766-issuers.json case 1
- corpus/vectors/ordering/cap38.liquidity-pools.synth-assets-js1766-issuers.json case 2
- corpus/vectors/ordering/cap38.liquidity-pools.synth-assets-js1766-issuers.json case 3

Entry notes: the js-stellar-sdk#1766 issuers: 32 bytes of 0x3a sort first as strkey text (GA5D... before GAAA...), 32 zero bytes sort first as key bytes, which is what core uses. same code on both sides, so the issuer decides. expected order computed with core's rule (asset type, code bytes with zero padding, issuer key bytes; isPoolShareAssetValid in stellar-core v30.0.0 src/util/types.cpp and the xdrpp union and xarray comparisons it uses), checked against stellar-xdr's derived Ord. pool_id cases expect the id core derives (sha256 of LiquidityPoolParameters with fee 30) when a < b, and a refusal otherwise, because core answers CHANGE_TRUST_MALFORMED for parameters that are not strictly ordered.

Related: https://github.com/stellar/js-stellar-sdk/pull/1766

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `cap38.liquidity-pools.synth-assets-js1766-issuers`
