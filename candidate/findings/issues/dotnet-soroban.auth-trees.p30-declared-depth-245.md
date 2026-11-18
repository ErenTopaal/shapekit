<!-- repo: Beans-BV/dotnet-stellar-sdk -->
# stellar-dotnet-sdk 17.0.0: the sdk refused a value the network accepts: InvalidDataException: Maximum decoding depth reached while decoding Hash (soroban.auth-trees.p30-declared-depth-245)

## What happens

On the input below, `stellar-dotnet-sdk` 17.0.0 answers differently from the Stellar network in 4 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `soroban.auth-trees.p30-declared-depth-245` (family `soroban.auth-trees`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `testnet/decode_encode` on 17.0.0

the sdk refused a value the network accepts: InvalidDataException: Maximum decoding depth reached while decoding Hash

```json
{
  "depth_limit": 1012,
  "len_limit": 16704,
  "op": "decode_encode",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAAD1Q1MP8YnUjblULzKgF6cQOlqr6MoRA4G8k8WH0zGu1gAAAB6PUKR5Nb8N8N2MVKuK7frToF+DtwP5cvvjbajNqYQvQV1t8I6BmYD78VG3fDizjVwXpv40Kr7Wps02CmkTdMR3AAAAAGnmvgAAAAAAAAAAAwAAAZ2tVjAAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQHkLiPYgmCIfgqC57QFuuk4XXJqK8MhM8dbN9Ni5XZiObrbjwJybIOAcjkosWQuPM5lDqrd9i3IMquqEWP+/4AYdz6c2od01/El+xLG3d55wKUiytYYPofhtiCLioJpI/XgSjRhq2SqUbwr3SIhJJSn7oKGcLGbmvlcsNqr16aLqAAAAbg3gtrOnZAAAAAAAAAAymLwAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGSFV+f5x6r04qQKss+eUbjZYRaQUcH1/G/PXKGmEPOqLgAAAAAAAAAA... (22272 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAAD1Q1MP8YnUjblULzKgF6cQOlqr6MoRA4G8k8WH0zGu1gAAAB6PUKR5Nb8N8N2MVKuK7frToF+DtwP5cvvjbajNqYQvQV1t8I6BmYD78VG3fDizjVwXpv40Kr7Wps02CmkTdMR3AAAAAGnmvgAAAAAAAAAAAwAAAZ2tVjAAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQHkLiPYgmCIfgqC57QFuuk4XXJqK8MhM8dbN9Ni5XZiObrbjwJybIOAcjkosWQuPM5lDqrd9i3IMquqEWP+/4AYdz6c2od01/El+xLG3d55wKUiytYYPofhtiCLioJpI/XgSjRhq2SqUbwr3SIhJJSn7oKGcLGbmvlcsNqr16aLqAAAAbg3gtrOnZAAAAAAAAAAymLwAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGSFV+f5x6r04qQKss+eUbjZYRaQUcH1/G/PXKGmEPOqLgAAAAAAAAAA... (22272 chars)"}`

Actual: `{"rejected":"InvalidDataException: Maximum decoding depth reached while decoding Hash"}`

### `testnet/tx_hash 0` on 17.0.0

the sdk refused a value the network accepts: InvalidDataException: Maximum decoding depth reached while decoding Hash

```json
{
  "depth_limit": 1000,
  "len_limit": 14672,
  "network_passphrase": "Test SDF Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAgAAAAAc/FAM6LNIOWOzSip7lmPnQwHeXh1lk3Gb4TWhSysc6QACCwYAAABEAAAAKQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAFOJwJWLh6vOxL00nqTsD01DTtLe6VVdx6uHHqQsyaZ/QAAAARub2RlAAAAAgAAABAAAAABAAAAAAAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAGZmFub3V0AAAAAAADAAAAAAAAAA8AAAAGc2lnbmVyAAAAAAASAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsAAAAAEAAAABAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsALCbZJzVbG2AAAA0QAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAKcHVibGljX2tleQAAAAAADQAAACDNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDT... (19564 chars)"
}
```

Expected: `{"hash":"890b530034908d74c3d5cb6f0275d12ad58cec409dc1dce5c6b212a8c955cec0"}`

Actual: `{"rejected":"InvalidDataException: Maximum decoding depth reached while decoding Hash"}`

### `mainnet/decode_encode` on 17.0.0

the sdk refused a value the network accepts: InvalidDataException: Maximum decoding depth reached while decoding Hash

```json
{
  "depth_limit": 1012,
  "len_limit": 16704,
  "op": "decode_encode",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAABE4ISxDnzFc91kw+NGP+uIwLScQif/O2Fr/kwWsocx2QAAAB6tpuXdwxaZYhwr+oFzU8REjrpDjA69JfoOSGB+uYYf91whLLQ0zKVfCZxz1BuDszAPG6PxCWQ33IjNfbMFjRqeAAAAAGnmvgAAAAAAAAAAAwAAAZ2tVjAAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQC0gR/NjQU5VtOrkD09UUo+OEwMPry0EgOBn9CbYevIULuYG3YywUNMRBAGsHCxmFW1xAAkt50tMmCMqfCGewwGXQtLZcBo8/AbqmBkmrQJGfZfM1Fe06zztT1rMsRKbHu4lNR0XjpFJdmnghUiwlaDhJLQvpFQnVSTqi5Q9uY8yAAAAbg3gtrOnZAAAAAAAAAAymKMAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGT+NAJiuIqaEIIhDzWrtobIFvaN4SBza3QNr/JzIxMEGwAAAAAAAAAA... (22272 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAABE4ISxDnzFc91kw+NGP+uIwLScQif/O2Fr/kwWsocx2QAAAB6tpuXdwxaZYhwr+oFzU8REjrpDjA69JfoOSGB+uYYf91whLLQ0zKVfCZxz1BuDszAPG6PxCWQ33IjNfbMFjRqeAAAAAGnmvgAAAAAAAAAAAwAAAZ2tVjAAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQC0gR/NjQU5VtOrkD09UUo+OEwMPry0EgOBn9CbYevIULuYG3YywUNMRBAGsHCxmFW1xAAkt50tMmCMqfCGewwGXQtLZcBo8/AbqmBkmrQJGfZfM1Fe06zztT1rMsRKbHu4lNR0XjpFJdmnghUiwlaDhJLQvpFQnVSTqi5Q9uY8yAAAAbg3gtrOnZAAAAAAAAAAymKMAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGT+NAJiuIqaEIIhDzWrtobIFvaN4SBza3QNr/JzIxMEGwAAAAAAAAAA... (22272 chars)"}`

Actual: `{"rejected":"InvalidDataException: Maximum decoding depth reached while decoding Hash"}`

### `mainnet/tx_hash 0` on 17.0.0

the sdk refused a value the network accepts: InvalidDataException: Maximum decoding depth reached while decoding Hash

```json
{
  "depth_limit": 1000,
  "len_limit": 14672,
  "network_passphrase": "Public Global Stellar Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAgAAAAAc/FAM6LNIOWOzSip7lmPnQwHeXh1lk3Gb4TWhSysc6QACCwYAAABEAAAAKQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAFxd08LnGZurodrreWHhrpcj7t2sOvbiWLD+3BQqRx/8QAAAARub2RlAAAAAgAAABAAAAABAAAAAAAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAGZmFub3V0AAAAAAADAAAAAAAAAA8AAAAGc2lnbmVyAAAAAAASAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsAAAAAEAAAABAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsALCbZJzVbG2AAAA0QAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAKcHVibGljX2tleQAAAAAADQAAACDNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDT... (19564 chars)"
}
```

Expected: `{"hash":"52e24f8269ff968937afdff11a52d62194ae00e7b4921ea70d687d73de35de8f"}`

Actual: `{"rejected":"InvalidDataException: Maximum decoding depth reached while decoding Hash"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`:

```sh
shapekit conformance run --sdk dotnet --version 17.0.0 --entry soroban.auth-trees.p30-declared-depth-245
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.xdr): decode = must-accept, roundtrip = true
- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.xdr): decode = must-accept, roundtrip = true

How the corpus entry was built and checked: an address entry whose matched root declares a chain 245 invocations deep below it that no call reaches, the deepest the host still decodes; the transaction succeeds and the declared nodes are never checked

Related: https://github.com/stellar/wallet-backend/issues/746

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `soroban.auth-trees.p30-declared-depth-245`
