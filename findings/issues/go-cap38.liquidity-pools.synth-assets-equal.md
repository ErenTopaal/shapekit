<!-- repo: stellar/go-stellar-sdk -->
# github.com/stellar/go-stellar-sdk v0.7.1: the sdk accepted a value the network refuses (cap38.liquidity-pools.synth-assets-equal)

## What happens

`github.com/stellar/go-stellar-sdk` v0.7.1 answers 2 of the checks of corpus entry `cap38.liquidity-pools.synth-assets-equal` differently from the Stellar network.

## Minimal vector

Entry `cap38.liquidity-pools.synth-assets-equal` (kind Ordering, family `cap38.liquidity-pools`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `case 2/pool_id` on v0.7.1

the sdk accepted a value the network refuses

```json
{
  "a": "AAAAAVVTRAAAAAAAAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=",
  "b": "AAAAAVVTRAAAAAAAAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=",
  "fee": 30,
  "op": "pool_id"
}
```

Expected: `"rejected"`

Actual: `{"pool_id":"28e3deb3767f1e81058204cfcc0cfba986f6f5e8b8aae1ed45086e0ef34d5350"}`

### `case 3/pool_id` on v0.7.1

the sdk accepted a value the network refuses

```json
{
  "a": "AAAAAVVTRAAAAAAAAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=",
  "b": "AAAAAVVTRAAAAAAAAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=",
  "fee": 30,
  "op": "pool_id"
}
```

Expected: `"rejected"`

Actual: `{"pool_id":"28e3deb3767f1e81058204cfcc0cfba986f6f5e8b8aae1ed45086e0ef34d5350"}`

## Reproduce

```sh
shapekit conformance run --sdk go --version v0.7.1 --entry cap38.liquidity-pools.synth-assets-equal
```

## Expected value and its source

- corpus/vectors/ordering/cap38.liquidity-pools.synth-assets-equal.json case 2
- corpus/vectors/ordering/cap38.liquidity-pools.synth-assets-equal.json case 3

Entry notes: the same asset twice. core requires assetA < assetB strictly, so a pool of one asset with itself is malformed. expected order computed with core's rule (asset type, code bytes with zero padding, issuer key bytes; isPoolShareAssetValid in stellar-core v30.0.0 src/util/types.cpp and the xdrpp union and xarray comparisons it uses), checked against stellar-xdr's derived Ord. pool_id cases expect the id core derives (sha256 of LiquidityPoolParameters with fee 30) when a < b, and a refusal otherwise, because core answers CHANGE_TRUST_MALFORMED for parameters that are not strictly ordered.

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `cap38.liquidity-pools.synth-assets-equal`
