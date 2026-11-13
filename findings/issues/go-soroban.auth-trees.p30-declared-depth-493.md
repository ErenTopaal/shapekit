<!-- repo: stellar/go-stellar-sdk -->
# github.com/stellar/go-stellar-sdk v0.8.0, v0.9.0: the sdk accepted a value the network refuses (soroban.auth-trees.p30-declared-depth-493)

## What happens

`github.com/stellar/go-stellar-sdk` v0.8.0, v0.9.0 answers 4 of the checks of corpus entry `soroban.auth-trees.p30-declared-depth-493` differently from the Stellar network.

## Minimal vector

Entry `soroban.auth-trees.p30-declared-depth-493` (kind Xdr, family `soroban.auth-trees`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/decode_encode` on v0.8.0, v0.9.0

the sdk accepted a value the network refuses

```json
{
  "depth_limit": 1000,
  "len_limit": 28560,
  "op": "decode_encode",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAAAc/FAM6LNIOWOzSip7lmPnQwHeXh1lk3Gb4TWhSysc6QACCwkAAABEAAAALAAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAFOJwJWLh6vOxL00nqTsD01DTtLe6VVdx6uHHqQsyaZ/QAAAARub2RlAAAAAgAAABAAAAABAAAAAAAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAGZmFub3V0AAAAAAADAAAAAAAAAA8AAAAGc2lnbmVyAAAAAAASAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsAAAAAEAAAABAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsDAOb2ds6gtRAAAA1AAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAKcHVibGljX2tleQAAAAAADQAAACDNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDT... (38080 chars)"
}
```

Expected: `"rejected"`

Actual: `{"xdr":"AAAAAgAAAAAc/FAM6LNIOWOzSip7lmPnQwHeXh1lk3Gb4TWhSysc6QACCwkAAABEAAAALAAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAFOJwJWLh6vOxL00nqTsD01DTtLe6VVdx6uHHqQsyaZ/QAAAARub2RlAAAAAgAAABAAAAABAAAAAAAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAGZmFub3V0AAAAAAADAAAAAAAAAA8AAAAGc2lnbmVyAAAAAAASAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsAAAAAEAAAABAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsDAOb2ds6gtRAAAA1AAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAKcHVibGljX2tleQAAAAAADQAAACDNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDT... (38080 chars)"}`

### `mainnet/decode_encode` on v0.8.0, v0.9.0

the sdk accepted a value the network refuses

```json
{
  "depth_limit": 1000,
  "len_limit": 28560,
  "op": "decode_encode",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAAAc/FAM6LNIOWOzSip7lmPnQwHeXh1lk3Gb4TWhSysc6QACCwkAAABEAAAALAAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAFxd08LnGZurodrreWHhrpcj7t2sOvbiWLD+3BQqRx/8QAAAARub2RlAAAAAgAAABAAAAABAAAAAAAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAGZmFub3V0AAAAAAADAAAAAAAAAA8AAAAGc2lnbmVyAAAAAAASAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsAAAAAEAAAABAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsDAOb2ds6gtRAAAA1AAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAKcHVibGljX2tleQAAAAAADQAAACDNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDT... (38080 chars)"
}
```

Expected: `"rejected"`

Actual: `{"xdr":"AAAAAgAAAAAc/FAM6LNIOWOzSip7lmPnQwHeXh1lk3Gb4TWhSysc6QACCwkAAABEAAAALAAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAFxd08LnGZurodrreWHhrpcj7t2sOvbiWLD+3BQqRx/8QAAAARub2RlAAAAAgAAABAAAAABAAAAAAAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAGZmFub3V0AAAAAAADAAAAAAAAAA8AAAAGc2lnbmVyAAAAAAASAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsAAAAAEAAAABAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsDAOb2ds6gtRAAAA1AAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAKcHVibGljX2tleQAAAAAADQAAACDNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDT... (38080 chars)"}`

## Reproduce

```sh
shapekit conformance run --sdk go --version v0.8.0 --entry soroban.auth-trees.p30-declared-depth-493
```

## Expected value and its source

- corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-493/mainnet.xdr: decode = must-reject (validity InvalidByCore)
- corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-493/testnet.xdr: decode = must-reject (validity InvalidByCore)

Entry notes: an address entry declaring a chain 493 invocations deep below its root, 494 with the root, one past what core's rust parser reads at depth 1000 (can_parse_transaction): core refuses the envelope with txMALFORMED at submission. submitted with ledger 112 as the last closed ledger

Related: https://github.com/stellar/wallet-backend/issues/746

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.auth-trees.p30-declared-depth-493`
