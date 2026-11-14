<!-- repo: stellar/go-stellar-sdk -->
# github.com/stellar/go-stellar-sdk v0.8.0, v0.9.0: the sdk accepted a value the network refuses (soroban.auth-trees.p30-declared-depth-493)

## What happens

On the input below, `github.com/stellar/go-stellar-sdk` v0.8.0, v0.9.0 answers differently from the Stellar network in 4 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `soroban.auth-trees.p30-declared-depth-493` (family `soroban.auth-trees`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `testnet/decode_encode` on v0.8.0, v0.9.0

the sdk accepted a value the network refuses

```json
{
  "depth_limit": 1000,
  "len_limit": 28560,
  "op": "decode_encode",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAAAc/FAM6LNIOWOzSip7lmPnQwHeXh1lk3Gb4TWhSysc6QACCwYAAABEAAAALAAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAFOJwJWLh6vOxL00nqTsD01DTtLe6VVdx6uHHqQsyaZ/QAAAARub2RlAAAAAgAAABAAAAABAAAAAAAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAGZmFub3V0AAAAAAADAAAAAAAAAA8AAAAGc2lnbmVyAAAAAAASAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsAAAAAEAAAABAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsDAOb2ds6gtRAAAA1AAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAKcHVibGljX2tleQAAAAAADQAAACDNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDT... (38080 chars)"
}
```

Expected: `"rejected"`

Actual: `{"xdr":"AAAAAgAAAAAc/FAM6LNIOWOzSip7lmPnQwHeXh1lk3Gb4TWhSysc6QACCwYAAABEAAAALAAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAFOJwJWLh6vOxL00nqTsD01DTtLe6VVdx6uHHqQsyaZ/QAAAARub2RlAAAAAgAAABAAAAABAAAAAAAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAGZmFub3V0AAAAAAADAAAAAAAAAA8AAAAGc2lnbmVyAAAAAAASAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsAAAAAEAAAABAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsDAOb2ds6gtRAAAA1AAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAKcHVibGljX2tleQAAAAAADQAAACDNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDT... (38080 chars)"}`

### `mainnet/decode_encode` on v0.8.0, v0.9.0

the sdk accepted a value the network refuses

```json
{
  "depth_limit": 1000,
  "len_limit": 28560,
  "op": "decode_encode",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAAAc/FAM6LNIOWOzSip7lmPnQwHeXh1lk3Gb4TWhSysc6QACCwYAAABEAAAALAAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAFxd08LnGZurodrreWHhrpcj7t2sOvbiWLD+3BQqRx/8QAAAARub2RlAAAAAgAAABAAAAABAAAAAAAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAGZmFub3V0AAAAAAADAAAAAAAAAA8AAAAGc2lnbmVyAAAAAAASAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsAAAAAEAAAABAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsDAOb2ds6gtRAAAA1AAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAKcHVibGljX2tleQAAAAAADQAAACDNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDT... (38080 chars)"
}
```

Expected: `"rejected"`

Actual: `{"xdr":"AAAAAgAAAAAc/FAM6LNIOWOzSip7lmPnQwHeXh1lk3Gb4TWhSysc6QACCwYAAABEAAAALAAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAFxd08LnGZurodrreWHhrpcj7t2sOvbiWLD+3BQqRx/8QAAAARub2RlAAAAAgAAABAAAAABAAAAAAAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAGZmFub3V0AAAAAAADAAAAAAAAAA8AAAAGc2lnbmVyAAAAAAASAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsAAAAAEAAAABAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsDAOb2ds6gtRAAAA1AAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAKcHVibGljX2tleQAAAAAADQAAACDNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDT... (38080 chars)"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk go --version v0.8.0 --entry soroban.auth-trees.p30-declared-depth-493
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-493/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-493/mainnet.xdr): decode = must-reject (validity InvalidByCore)
- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-493/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-493/testnet.xdr): decode = must-reject (validity InvalidByCore)

How the corpus entry was built and checked: an address entry declaring a chain 493 invocations deep below its root, 494 with the root, one past what core's rust parser reads at depth 1000 (can_parse_transaction): core refuses the envelope with txMALFORMED at submission. submitted with ledger 112 as the last closed ledger

Related: https://github.com/stellar/wallet-backend/issues/746

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `soroban.auth-trees.p30-declared-depth-493`
