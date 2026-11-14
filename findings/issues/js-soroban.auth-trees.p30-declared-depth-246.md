<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.4.1: the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth (soroban.auth-trees.p30-declared-depth-246)

## What happens

`@stellar/stellar-sdk` 16.4.1 answers 4 of the checks of corpus entry `soroban.auth-trees.p30-declared-depth-246` differently from the Stellar network.

## Minimal vector

Entry `soroban.auth-trees.p30-declared-depth-246` (kind Xdr, family `soroban.auth-trees`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 16568,
  "op": "decode_encode",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAACPOxUUaFoa4CfREB013bocHndVdaNCIMrQ4MhTzva2WQAAAB71Q1MP8YnUjblULzKgF6cQOlqr6MoRA4G8k8WH0zGu1q+S3/eGDwjRwav4JZ60GhTP6p2f4hoShum9xu/YERBcAAAAAGnoD4AAAAAAAAAAAwAAAZ2yfIwAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQL278lHnL/SnKS6m2yKsyl+snCsz+IocHzD/XZZeSJNO3k222JgmWmni9i75eC8DdPRkGVSIwia28zfPqG311AH6f6vSklSDC9f6BYWwXj8Bk8R6SNgiuKKZ60O1vKdOXhCHbcIy3FTGy/W3styQ7gg7Zsby2f8IG0owspk/1ibIAAAAbw3gtrOnZAAAAAAAAAAzJuUAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGSFV+f5x6r04qQKss+eUbjZYRaQUcH1/G/PXKGmEPOqLgAAAAAAAAAA... (22092 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAACPOxUUaFoa4CfREB013bocHndVdaNCIMrQ4MhTzva2WQAAAB71Q1MP8YnUjblULzKgF6cQOlqr6MoRA4G8k8WH0zGu1q+S3/eGDwjRwav4JZ60GhTP6p2f4hoShum9xu/YERBcAAAAAGnoD4AAAAAAAAAAAwAAAZ2yfIwAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQL278lHnL/SnKS6m2yKsyl+snCsz+IocHzD/XZZeSJNO3k222JgmWmni9i75eC8DdPRkGVSIwia28zfPqG311AH6f6vSklSDC9f6BYWwXj8Bk8R6SNgiuKKZ60O1vKdOXhCHbcIy3FTGy/W3styQ7gg7Zsby2f8IG0owspk/1ibIAAAAbw3gtrOnZAAAAAAAAAAzJuUAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGSFV+f5x6r04qQKss+eUbjZYRaQUcH1/G/PXKGmEPOqLgAAAAAAAAAA... (22092 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `testnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 16568,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAACPOxUUaFoa4CfREB013bocHndVdaNCIMrQ4MhTzva2WQAAAB71Q1MP8YnUjblULzKgF6cQOlqr6MoRA4G8k8WH0zGu1q+S3/eGDwjRwav4JZ60GhTP6p2f4hoShum9xu/YERBcAAAAAGnoD4AAAAAAAAAAAwAAAZ2yfIwAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQL278lHnL/SnKS6m2yKsyl+snCsz+IocHzD/XZZeSJNO3k222JgmWmni9i75eC8DdPRkGVSIwia28zfPqG311AH6f6vSklSDC9f6BYWwXj8Bk8R6SNgiuKKZ60O1vKdOXhCHbcIy3FTGy/W3styQ7gg7Zsby2f8IG0owspk/1ibIAAAAbw3gtrOnZAAAAAAAAAAzJuUAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGSFV+f5x6r04qQKss+eUbjZYRaQUcH1/G/PXKGmEPOqLgAAAAAAAAAA... (22092 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"8f3b1514685a1ae027d1101d35ddba1c1e775575a34220cad0e0c853cef6b659","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"10876dc232dc54c6cbf5b7b2dc90ee083b66c6f2d9ff081b4a30b2993fd626c8","ext":"v0","fee_pool":"3352293","id_pool":"0","inflation_seq":0,"ledger_seq":111,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"f543530ff189d48db9542f32a017a7103a5aabe8ca110381bc93c587d331aed6","scp_value":{"close_time":"1776816000","ext":{"signed_ms":{"close_time_ms":"1776816000000","lc_value_sign... (47776 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 16568,
  "op": "decode_encode",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAACETz5HRiJTcUdrsjZcWqc+XZD8muhC19qrQP8u2XF9OgAAAB5E4ISxDnzFc91kw+NGP+uIwLScQif/O2Fr/kwWsocx2cG+ScHi8qiNjrY1NMdmtczKaH00XDP7u+uBvuyANpGzAAAAAGnoD4AAAAAAAAAAAwAAAZ2yfIwAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQMHrdciJvzBz8zdPKUwJq0vdwz6UNh5qgGNhlisdV/7ItGQ+hxGuMyAajYpvXE0XpCV3tWIa3CTSclYDLTBjcAJ+DSV/Xv0Xni3eJDTm9YNymTruVEJgZBpGnhzEtmr3M0S+6T6JLlFUp7Jh//IKcEc2zAy03GTlSkNN71KJBqCoAAAAbw3gtrOnZAAAAAAAAAAzJswAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGT+NAJiuIqaEIIhDzWrtobIFvaN4SBza3QNr/JzIxMEGwAAAAAAAAAA... (22092 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAACETz5HRiJTcUdrsjZcWqc+XZD8muhC19qrQP8u2XF9OgAAAB5E4ISxDnzFc91kw+NGP+uIwLScQif/O2Fr/kwWsocx2cG+ScHi8qiNjrY1NMdmtczKaH00XDP7u+uBvuyANpGzAAAAAGnoD4AAAAAAAAAAAwAAAZ2yfIwAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQMHrdciJvzBz8zdPKUwJq0vdwz6UNh5qgGNhlisdV/7ItGQ+hxGuMyAajYpvXE0XpCV3tWIa3CTSclYDLTBjcAJ+DSV/Xv0Xni3eJDTm9YNymTruVEJgZBpGnhzEtmr3M0S+6T6JLlFUp7Jh//IKcEc2zAy03GTlSkNN71KJBqCoAAAAbw3gtrOnZAAAAAAAAAAzJswAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGT+NAJiuIqaEIIhDzWrtobIFvaN4SBza3QNr/JzIxMEGwAAAAAAAAAA... (22092 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 16568,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAACETz5HRiJTcUdrsjZcWqc+XZD8muhC19qrQP8u2XF9OgAAAB5E4ISxDnzFc91kw+NGP+uIwLScQif/O2Fr/kwWsocx2cG+ScHi8qiNjrY1NMdmtczKaH00XDP7u+uBvuyANpGzAAAAAGnoD4AAAAAAAAAAAwAAAZ2yfIwAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQMHrdciJvzBz8zdPKUwJq0vdwz6UNh5qgGNhlisdV/7ItGQ+hxGuMyAajYpvXE0XpCV3tWIa3CTSclYDLTBjcAJ+DSV/Xv0Xni3eJDTm9YNymTruVEJgZBpGnhzEtmr3M0S+6T6JLlFUp7Jh//IKcEc2zAy03GTlSkNN71KJBqCoAAAAbw3gtrOnZAAAAAAAAAAzJswAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGT+NAJiuIqaEIIhDzWrtobIFvaN4SBza3QNr/JzIxMEGwAAAAAAAAAA... (22092 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"844f3e4746225371476bb2365c5aa73e5d90fc9ae842d7daab40ff2ed9717d3a","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"44bee93e892e5154a7b261fff20a704736cc0cb4dc64e54a434def528906a0a8","ext":"v0","fee_pool":"3352268","id_pool":"0","inflation_seq":0,"ledger_seq":111,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"44e084b10e7cc573dd64c3e3463feb88c0b49c4227ff3b616bfe4c16b28731d9","scp_value":{"close_time":"1776816000","ext":{"signed_ms":{"close_time_ms":"1776816000000","lc_value_sign... (47776 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

## Reproduce

```sh
shapekit conformance run --sdk js --version 16.4.1 --entry soroban.auth-trees.p30-declared-depth-246
```

## Expected value and its source

- corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-246/mainnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-246/mainnet.xdr: decode = must-accept, roundtrip = true
- corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-246/testnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-246/testnet.xdr: decode = must-accept, roundtrip = true

Entry notes: an address entry declaring a chain 246 invocations deep below its root, one past what the host decodes at depth 500: core includes it and the host traps

Related: https://github.com/stellar/wallet-backend/issues/746

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.auth-trees.p30-declared-depth-246`
