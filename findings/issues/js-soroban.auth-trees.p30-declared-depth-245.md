<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.4.1: the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth (soroban.auth-trees.p30-declared-depth-245)

## What happens

On the input below, `@stellar/stellar-sdk` 16.4.1 answers differently from the Stellar network in 4 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `soroban.auth-trees.p30-declared-depth-245` (family `soroban.auth-trees`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `testnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

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

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `testnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 16704,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAAD1Q1MP8YnUjblULzKgF6cQOlqr6MoRA4G8k8WH0zGu1gAAAB6PUKR5Nb8N8N2MVKuK7frToF+DtwP5cvvjbajNqYQvQV1t8I6BmYD78VG3fDizjVwXpv40Kr7Wps02CmkTdMR3AAAAAGnmvgAAAAAAAAAAAwAAAZ2tVjAAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQHkLiPYgmCIfgqC57QFuuk4XXJqK8MhM8dbN9Ni5XZiObrbjwJybIOAcjkosWQuPM5lDqrd9i3IMquqEWP+/4AYdz6c2od01/El+xLG3d55wKUiytYYPofhtiCLioJpI/XgSjRhq2SqUbwr3SIhJJSn7oKGcLGbmvlcsNqr16aLqAAAAbg3gtrOnZAAAAAAAAAAymLwAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGSFV+f5x6r04qQKss+eUbjZYRaQUcH1/G/PXKGmEPOqLgAAAAAAAAAA... (22272 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"f543530ff189d48db9542f32a017a7103a5aabe8ca110381bc93c587d331aed6","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"78128d186ad92a946f0af74888492529fba0a19c2c66e6be572c36aaf5e9a2ea","ext":"v0","fee_pool":"3315900","id_pool":"0","inflation_seq":0,"ledger_seq":110,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"8f50a47935bf0df0dd8c54ab8aedfad3a05f83b703f972fbe36da8cda9842f41","scp_value":{"close_time":"1776729600","ext":{"signed_ms":{"close_time_ms":"1776729600000","lc_value_sign... (48190 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

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

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 16704,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAABE4ISxDnzFc91kw+NGP+uIwLScQif/O2Fr/kwWsocx2QAAAB6tpuXdwxaZYhwr+oFzU8REjrpDjA69JfoOSGB+uYYf91whLLQ0zKVfCZxz1BuDszAPG6PxCWQ33IjNfbMFjRqeAAAAAGnmvgAAAAAAAAAAAwAAAZ2tVjAAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQC0gR/NjQU5VtOrkD09UUo+OEwMPry0EgOBn9CbYevIULuYG3YywUNMRBAGsHCxmFW1xAAkt50tMmCMqfCGewwGXQtLZcBo8/AbqmBkmrQJGfZfM1Fe06zztT1rMsRKbHu4lNR0XjpFJdmnghUiwlaDhJLQvpFQnVSTqi5Q9uY8yAAAAbg3gtrOnZAAAAAAAAAAymKMAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGT+NAJiuIqaEIIhDzWrtobIFvaN4SBza3QNr/JzIxMEGwAAAAAAAAAA... (22272 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"44e084b10e7cc573dd64c3e3463feb88c0b49c4227ff3b616bfe4c16b28731d9","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"ee25351d178e91497669e08548b095a0e124b42fa454275524ea8b943db98f32","ext":"v0","fee_pool":"3315875","id_pool":"0","inflation_seq":0,"ledger_seq":110,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"ada6e5ddc31699621c2bfa817353c4448eba438c0ebd25fa0e48607eb9861ff7","scp_value":{"close_time":"1776729600","ext":{"signed_ms":{"close_time_ms":"1776729600000","lc_value_sign... (48190 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk js --version 16.4.1 --entry soroban.auth-trees.p30-declared-depth-245
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.xdr): decode = must-accept, roundtrip = true
- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.xdr): decode = must-accept, roundtrip = true

How the corpus entry was built and checked: an address entry whose matched root declares a chain 245 invocations deep below it that no call reaches, the deepest the host still decodes; the transaction succeeds and the declared nodes are never checked

Related: https://github.com/stellar/wallet-backend/issues/746

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `soroban.auth-trees.p30-declared-depth-245`
