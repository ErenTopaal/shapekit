<!-- repo: Beans-BV/dotnet-stellar-sdk -->
# stellar-dotnet-sdk 16.0.0, 17.0.0: the sdk crashed instead of answering: NullReferenceException: Object reference not set to an instance of an object. at StellarDotnetSdk.Operations.CreateContractOperation.FromXdr(InvokeHostFunctionOp invokeHostFunctionOp) (cap73.sac-trust.golden-504393015a71bc7a-8)

## What happens

On the input below, `stellar-dotnet-sdk` 16.0.0, 17.0.0 answers differently from the Stellar network in 2 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap73.sac-trust.golden-504393015a71bc7a-8` (family `cap73.sac-trust`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `any/tx_hash 0` on 16.0.0, 17.0.0

the sdk crashed instead of answering: NullReferenceException: Object reference not set to an instance of an object. at StellarDotnetSdk.Operations.CreateContractOperation.FromXdr(InvokeHostFunctionOp invokeHostFunctionOp)

```json
{
  "depth_limit": 1000,
  "len_limit": 252,
  "network_passphrase": "(V) (;,,;) (V)",
  "op": "tx_hash",
  "xdr": "AAAAAgAAAAC4Qtwt8EhGjBf7w268yEIqfaGsHtxHwBK7lmcetX/yRgAPvTwAAAAAAAAABwAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAQAAAAEAAAAAAAAAAQAAAAAAAAABAAAAAAAAAAAAAAABAAAABgAAAAG6o5MQXfdZaTAe2c6C7Mn+ONQGP5esCkHQentQXY6dQwAAABQAAAABAAYagAAAA+gAAAPoAAAAAAAPuVQAAAABtX/yRgAAAEBtLq/LB7yVkj/3iEhQ1N1WfJ5l2jG5VRmrZIjkg0aeVBV2Utmx9eqzWuOx/IbkfmzOdOhtrDqz6EIU4YFfCH8N"
}
```

Expected: `{"hash":"079375e360c7d711b6a902e97d74b9e2e6ad305e0fe944bdda7ee5faf717550d"}`

Actual: `{"error":"panic","message":"NullReferenceException: Object reference not set to an instance of an object. at StellarDotnetSdk.Operations.CreateContractOperation.FromXdr(InvokeHostFunctionOp invokeHostFunctionOp)"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk dotnet --version 16.0.0 --entry cap73.sac-trust.golden-504393015a71bc7a-8
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap73.sac-trust/cap73.sac-trust.golden-504393015a71bc7a-8/any.xdr,`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap73.sac-trust/cap73.sac-trust.golden-504393015a71bc7a-8/any.xdr,) envelope 0 of the transaction set: transactionHash of the envelope's result pair

How the corpus entry was built and checked: core test "merge account then SAC payment scenarios", section "protocol version 30|SAC payment from the merged account", ledger 10 (record 9 of 13 in the upstream file). smallest golden ledger matching ContractIdPreimage:asset, ContractExecutable:stellar_asset

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap73.sac-trust.golden-504393015a71bc7a-8`
