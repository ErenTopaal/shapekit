<!-- repo: stellar/go-stellar-sdk -->
# github.com/stellar/go-stellar-sdk v0.7.1: the sdk accepted a value the network refuses (cap38.liquidity-pools.synth-assets-equal)

## What happens

On the input below, `github.com/stellar/go-stellar-sdk` v0.7.1 answers differently from the Stellar network in 2 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap38.liquidity-pools.synth-assets-equal` (family `cap38.liquidity-pools`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

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

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk go --version v0.7.1 --entry cap38.liquidity-pools.synth-assets-equal
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/vectors/ordering/cap38.liquidity-pools.synth-assets-equal.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/vectors/ordering/cap38.liquidity-pools.synth-assets-equal.json) cases 2, 3

How the corpus entry was built and checked: the same asset twice. core requires assetA < assetB strictly, so a pool of one asset with itself is malformed. expected order computed with core's rule (asset type, code bytes with zero padding, issuer key bytes; isPoolShareAssetValid in stellar-core v30.0.0 src/util/types.cpp and the xdrpp union and xarray comparisons it uses), checked against stellar-xdr's derived Ord. pool_id cases expect the id core derives (sha256 of LiquidityPoolParameters with fee 30) when a < b, and a refusal otherwise, because core answers CHANGE_TRUST_MALFORMED for parameters that are not strictly ordered.

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap38.liquidity-pools.synth-assets-equal`
