<!-- repo: stellar/stellar-cli -->
# stellar-cli 30.0.1: the sdk refused a value the network accepts: ❌ error: error decoding JSON: recursion limit exceeded at line 1 column 11224 (soroban.auth-trees.p30-declared-depth-245)

## What happens

On the input below, `stellar-cli` 30.0.1 answers differently from the Stellar network in 6 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `soroban.auth-trees.p30-declared-depth-245` (family `soroban.auth-trees`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `testnet/decode_encode` on 30.0.1

the sdk refused a value the network accepts: ❌ error: error decoding JSON: recursion limit exceeded at line 1 column 11224

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

Actual: `{"rejected":"❌ error: error decoding JSON: recursion limit exceeded at line 1 column 11224"}`

### `testnet/encode_json` on 30.0.1

the sdk refused a value the network accepts: ❌ error: error decoding JSON: recursion limit exceeded at line 1 column 16252

```json
{
  "depth": 513,
  "json_bytes": 48234,
  "preview": "{\"json\":{\"v2\":{\"evicted_keys\":[],\"ext\":\"v0\",\"ledger_header\":{\"ext\":\"v0\",\"hash\":\"f543530ff189d48db9542f32a017a7103a5aabe8ca110381bc93c587d331aed6\",\"header\":{\"base_fee\":100,\"base_reserve\":100000000,\"bucket_list_hash\":\"78128d186ad92a946f0af74888492529fba0a19c2c66e6be572c36aaf5e9a2ea\",\"ext\":\"v0\",\"fee_pool\":\"3315900\",\"id_pool\":\"0\",\"inflation_seq\":0,\"ledger_seq\":110,\"ledger_version\":30,\"max_tx_set_size\":100,\"previous_ledger_hash\":\"8f50a47935bf0df0dd8c54ab8aedfad3a05f83b703f972fbe36da8cda9842f41\",\"scp_value\":{\"clo",
  "sha256": "cecc489e86cd97708a2c4a061f8986476340a6d4facf6240341d475866c46f99"
}
```

Expected: `{"xdr":"AAAAAgAAAAD1Q1MP8YnUjblULzKgF6cQOlqr6MoRA4G8k8WH0zGu1gAAAB6PUKR5Nb8N8N2MVKuK7frToF+DtwP5cvvjbajNqYQvQV1t8I6BmYD78VG3fDizjVwXpv40Kr7Wps02CmkTdMR3AAAAAGnmvgAAAAAAAAAAAwAAAZ2tVjAAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQHkLiPYgmCIfgqC57QFuuk4XXJqK8MhM8dbN9Ni5XZiObrbjwJybIOAcjkosWQuPM5lDqrd9i3IMquqEWP+/4AYdz6c2od01/El+xLG3d55wKUiytYYPofhtiCLioJpI/XgSjRhq2SqUbwr3SIhJJSn7oKGcLGbmvlcsNqr16aLqAAAAbg3gtrOnZAAAAAAAAAAymLwAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGSFV+f5x6r04qQKss+eUbjZYRaQUcH1/G/PXKGmEPOqLgAAAAAAAAAA... (22272 chars)"}`

Actual: `{"rejected":"❌ error: error decoding JSON: recursion limit exceeded at line 1 column 16252"}`

### `testnet/tx_hash 0` on 30.0.1

the sdk refused a value the network accepts: ❌ error: failed to decode XDR: depth limit exceeded

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

Actual: `{"rejected":"❌ error: failed to decode XDR: depth limit exceeded"}`

### `mainnet/decode_encode` on 30.0.1

the sdk refused a value the network accepts: ❌ error: error decoding JSON: recursion limit exceeded at line 1 column 11224

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

Actual: `{"rejected":"❌ error: error decoding JSON: recursion limit exceeded at line 1 column 11224"}`

### `mainnet/encode_json` on 30.0.1

the sdk refused a value the network accepts: ❌ error: error decoding JSON: recursion limit exceeded at line 1 column 16252

```json
{
  "depth": 513,
  "json_bytes": 48234,
  "preview": "{\"json\":{\"v2\":{\"evicted_keys\":[],\"ext\":\"v0\",\"ledger_header\":{\"ext\":\"v0\",\"hash\":\"44e084b10e7cc573dd64c3e3463feb88c0b49c4227ff3b616bfe4c16b28731d9\",\"header\":{\"base_fee\":100,\"base_reserve\":100000000,\"bucket_list_hash\":\"ee25351d178e91497669e08548b095a0e124b42fa454275524ea8b943db98f32\",\"ext\":\"v0\",\"fee_pool\":\"3315875\",\"id_pool\":\"0\",\"inflation_seq\":0,\"ledger_seq\":110,\"ledger_version\":30,\"max_tx_set_size\":100,\"previous_ledger_hash\":\"ada6e5ddc31699621c2bfa817353c4448eba438c0ebd25fa0e48607eb9861ff7\",\"scp_value\":{\"clo",
  "sha256": "53438a2fa5f18b42def799ec4dbce738e9b306aee2e43d02117c67a0fb3f16c4"
}
```

Expected: `{"xdr":"AAAAAgAAAABE4ISxDnzFc91kw+NGP+uIwLScQif/O2Fr/kwWsocx2QAAAB6tpuXdwxaZYhwr+oFzU8REjrpDjA69JfoOSGB+uYYf91whLLQ0zKVfCZxz1BuDszAPG6PxCWQ33IjNfbMFjRqeAAAAAGnmvgAAAAAAAAAAAwAAAZ2tVjAAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQC0gR/NjQU5VtOrkD09UUo+OEwMPry0EgOBn9CbYevIULuYG3YywUNMRBAGsHCxmFW1xAAkt50tMmCMqfCGewwGXQtLZcBo8/AbqmBkmrQJGfZfM1Fe06zztT1rMsRKbHu4lNR0XjpFJdmnghUiwlaDhJLQvpFQnVSTqi5Q9uY8yAAAAbg3gtrOnZAAAAAAAAAAymKMAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGT+NAJiuIqaEIIhDzWrtobIFvaN4SBza3QNr/JzIxMEGwAAAAAAAAAA... (22272 chars)"}`

Actual: `{"rejected":"❌ error: error decoding JSON: recursion limit exceeded at line 1 column 16252"}`

1 more checks fail; the command below lists every one.

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`:

```sh
shapekit conformance run --sdk stellar-cli --version 30.0.1 --entry soroban.auth-trees.p30-declared-depth-245
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.xdr
- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.xdr): decode = must-accept, roundtrip = true
- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.xdr
- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.xdr): decode = must-accept, roundtrip = true

How the corpus entry was built and checked: an address entry whose matched root declares a chain 245 invocations deep below it that no call reaches, the deepest the host still decodes; the transaction succeeds and the declared nodes are never checked

Related: https://github.com/stellar/wallet-backend/issues/746

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `soroban.auth-trees.p30-declared-depth-245`
