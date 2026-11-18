<!-- repo: Beans-BV/dotnet-stellar-sdk -->
# stellar-dotnet-sdk 17.0.0: the sdk's hash differs from the one the network wrote (cap71.credentials.golden-e25799a2e2e9eea2-17)

## What happens

On the input below, `stellar-dotnet-sdk` 17.0.0 answers differently from the Stellar network in 1 check.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap71.credentials.golden-e25799a2e2e9eea2-17` (family `cap71.credentials`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `any/tx_hash 0` on 17.0.0

the sdk's hash differs from the one the network wrote

```json
{
  "depth_limit": 1000,
  "len_limit": 1320,
  "network_passphrase": "(V) (;,,;) (V)",
  "op": "tx_hash",
  "xdr": "AAAAAgAAAAC4Qtwt8EhGjBf7w268yEIqfaGsHtxHwBK7lmcetX/yRgAehOQAAAAAAAAAEQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAHKQWfL2CVWM6UijlKrmSIG7PT7s19buYLjhRO1OmO3qwAAAAd0cmVlX2ZuAAAAAAIAAAAQAAAAAQAAAAEAAAASAAAAAQG4KQ/Um1vRMw8Opxi96mcpMgYg9lzhp2NlfFdjjFgLAAAAEQAAAAEAAAAEAAAADwAAAAhjaGlsZHJlbgAAABAAAAABAAAAAAAAAA8AAAAIY29udHJhY3QAAAASAAAAAcpBZ8vYJVYzpSKOUquZIgbs9PuzX1u5guOFE7U6Y7erAAAADwAAAAluZWVkX2F1dGgAAAAAAAAQAAAAAQAAAAEAAAAAAAAAAQAAAA8AAAAIdHJ5X2NhbGwAAAAAAAAAAAAAAAIAAAACAAAAAQG4KQ/Um1vRMw8Opxi96mcpMgYg9lzhp2NlfFdjjFgL... (1760 chars)"
}
```

Expected: `{"hash":"286f533bd01832894092f0393bf7a511757bd7f167551882ffa36d86847caefd"}`

Actual: `{"hash":"d14d233f97b14935d0fed960b1a494d65bbe9800cf62508c0fa5246867a91f4d"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`:

```sh
shapekit conformance run --sdk dotnet --version 17.0.0 --entry cap71.credentials.golden-e25799a2e2e9eea2-17
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap71.credentials/cap71.credentials.golden-e25799a2e2e9eea2-17/any.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap71.credentials/cap71.credentials.golden-e25799a2e2e9eea2-17/any.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair

How the corpus entry was built and checked: core test "Soroban authorization", section "custom account|single call|unused auth entries|failure with malformed function name", ledger 19 (record 18 of 18 in the upstream file). smallest golden ledger matching SorobanCredentials:address_v2

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap71.credentials.golden-e25799a2e2e9eea2-17`
