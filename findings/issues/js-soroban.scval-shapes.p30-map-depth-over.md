<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.4.1: the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth (soroban.scval-shapes.p30-map-depth-over)

## What happens

`@stellar/stellar-sdk` 16.4.1 answers 4 of the checks of corpus entry `soroban.scval-shapes.p30-map-depth-over` differently from the Stellar network.

## Minimal vector

Entry `soroban.scval-shapes.p30-map-depth-over` (kind Xdr, family `soroban.scval-shapes`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 4604,
  "op": "decode_encode",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAAAUNEvhf8hgY9heC0YJZYKcS8I1YAqQGEGkbYHED9byYgAAAB5SqCbyKPCiSvVXF6t3BWInQ7sl/JHqivYwEAik1YaQuZCfXHk+SmuvxoDWSCnVmZ3I4gYNBsN3gV1p2lSRxdVXAAAAAGm3SAAAAAAAAAAAAwAAAZzz8UAAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQHaUivBJ3LoSniT8QfPW662W3a7zltvO8yJS7e/D4VlO5kR5ShUd5PPFWn7poWCxtKC5z/EQ8TcTUnQZ5VB45govpcIiJFlhlt+e2vMJ852VS13Kozgph6A+/eDRv+nIL2WxId/Lu6FNURh+EHQT+9owLoYXVt3O39dQXv7shJeEAAAASg3gtrOnZAAAAAAAAAAJUMYAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTK446QHpJ1G7D+xAxGCEyC/HnzGnvES2y7pwcTWVGNIwAAAAAAAAAA... (6140 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAAAUNEvhf8hgY9heC0YJZYKcS8I1YAqQGEGkbYHED9byYgAAAB5SqCbyKPCiSvVXF6t3BWInQ7sl/JHqivYwEAik1YaQuZCfXHk+SmuvxoDWSCnVmZ3I4gYNBsN3gV1p2lSRxdVXAAAAAGm3SAAAAAAAAAAAAwAAAZzz8UAAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQHaUivBJ3LoSniT8QfPW662W3a7zltvO8yJS7e/D4VlO5kR5ShUd5PPFWn7poWCxtKC5z/EQ8TcTUnQZ5VB45govpcIiJFlhlt+e2vMJ852VS13Kozgph6A+/eDRv+nIL2WxId/Lu6FNURh+EHQT+9owLoYXVt3O39dQXv7shJeEAAAASg3gtrOnZAAAAAAAAAAJUMYAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTK446QHpJ1G7D+xAxGCEyC/HnzGnvES2y7pwcTWVGNIwAAAAAAAAAA... (6140 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `testnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 4604,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAAAUNEvhf8hgY9heC0YJZYKcS8I1YAqQGEGkbYHED9byYgAAAB5SqCbyKPCiSvVXF6t3BWInQ7sl/JHqivYwEAik1YaQuZCfXHk+SmuvxoDWSCnVmZ3I4gYNBsN3gV1p2lSRxdVXAAAAAGm3SAAAAAAAAAAAAwAAAZzz8UAAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQHaUivBJ3LoSniT8QfPW662W3a7zltvO8yJS7e/D4VlO5kR5ShUd5PPFWn7poWCxtKC5z/EQ8TcTUnQZ5VB45govpcIiJFlhlt+e2vMJ852VS13Kozgph6A+/eDRv+nIL2WxId/Lu6FNURh+EHQT+9owLoYXVt3O39dQXv7shJeEAAAASg3gtrOnZAAAAAAAAAAJUMYAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTK446QHpJ1G7D+xAxGCEyC/HnzGnvES2y7pwcTWVGNIwAAAAAAAAAA... (6140 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"14344be17fc86063d85e0b460965829c4bc235600a901841a46d81c40fd6f262","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"65b121dfcbbba14d51187e107413fbda302e861756ddcedfd7505efeec849784","ext":"v0","fee_pool":"610502","id_pool":"0","inflation_seq":0,"ledger_seq":74,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"52a826f228f0a24af55717ab7705622743bb25fc91ea8af6301008a4d58690b9","scp_value":{"close_time":"1773619200","ext":{"signed_ms":{"close_time_ms":"1773619200000","lc_value_signat... (10644 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 4604,
  "op": "decode_encode",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAADyOcwbQkjJ8YMrEQUkeugiF4r6k8XWcaOUct49iPrgpQAAAB6ZD8OTfYZUGGZSgnBfmB0jmYn+aU6YuV8gXCYR5tH++UQ7Spdq3HpzIkUx+mzY30blA574SbHGuJakcNcXLzlJAAAAAGm3SAAAAAAAAAAAAwAAAZzz8UAAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQJVID7HmXd6GXTniwVwaipoLJcLsj45kuG/Nin7ORkfYz2wQaBlKFVcuwVYEYfbHlQ6PXikEdPwhaH2I1f2YfQK0uuK+ttIT28Z5mGS87RSGclrrqIEJZAtgs/NEfJVnZ0M5DQPjtVxVlqrX2LnGBQkD+6jImvfTAUkjkjX1LIhYAAAASg3gtrOnZAAAAAAAAAAJUMYAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTMEpMCXT7SwVhf/CDq230exVP8CFN3c5xOESq638ytnwAAAAAAAAAA... (6140 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAADyOcwbQkjJ8YMrEQUkeugiF4r6k8XWcaOUct49iPrgpQAAAB6ZD8OTfYZUGGZSgnBfmB0jmYn+aU6YuV8gXCYR5tH++UQ7Spdq3HpzIkUx+mzY30blA574SbHGuJakcNcXLzlJAAAAAGm3SAAAAAAAAAAAAwAAAZzz8UAAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQJVID7HmXd6GXTniwVwaipoLJcLsj45kuG/Nin7ORkfYz2wQaBlKFVcuwVYEYfbHlQ6PXikEdPwhaH2I1f2YfQK0uuK+ttIT28Z5mGS87RSGclrrqIEJZAtgs/NEfJVnZ0M5DQPjtVxVlqrX2LnGBQkD+6jImvfTAUkjkjX1LIhYAAAASg3gtrOnZAAAAAAAAAAJUMYAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTMEpMCXT7SwVhf/CDq230exVP8CFN3c5xOESq638ytnwAAAAAAAAAA... (6140 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 4604,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAADyOcwbQkjJ8YMrEQUkeugiF4r6k8XWcaOUct49iPrgpQAAAB6ZD8OTfYZUGGZSgnBfmB0jmYn+aU6YuV8gXCYR5tH++UQ7Spdq3HpzIkUx+mzY30blA574SbHGuJakcNcXLzlJAAAAAGm3SAAAAAAAAAAAAwAAAZzz8UAAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQJVID7HmXd6GXTniwVwaipoLJcLsj45kuG/Nin7ORkfYz2wQaBlKFVcuwVYEYfbHlQ6PXikEdPwhaH2I1f2YfQK0uuK+ttIT28Z5mGS87RSGclrrqIEJZAtgs/NEfJVnZ0M5DQPjtVxVlqrX2LnGBQkD+6jImvfTAUkjkjX1LIhYAAAASg3gtrOnZAAAAAAAAAAJUMYAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTMEpMCXT7SwVhf/CDq230exVP8CFN3c5xOESq638ytnwAAAAAAAAAA... (6140 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"f239cc1b4248c9f1832b1105247ae822178afa93c5d671a39472de3d88fae0a5","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"43390d03e3b55c5596aad7d8b9c6050903fba8c89af7d30149239235f52c8858","ext":"v0","fee_pool":"610502","id_pool":"0","inflation_seq":0,"ledger_seq":74,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"990fc3937d865418665282705f981d239989fe694e98b95f205c2611e6d1fef9","scp_value":{"close_time":"1773619200","ext":{"signed_ms":{"close_time_ms":"1773619200000","lc_value_signat... (10644 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

## Reproduce

```sh
shapekit conformance run --sdk js --version 16.4.1 --entry soroban.scval-shapes.p30-map-depth-over
```

## Expected value and its source

- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-over/mainnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-over/mainnet.xdr: decode = must-accept, roundtrip = true
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-over/testnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-over/testnet.xdr: decode = must-accept, roundtrip = true

Entry notes: 99 nested maps, one more than the host accepts (simulation: HostError: Error(Value, InvalidInput)): core includes the invocation and fails it with invoke_host_function Trapped, so the value exists only in the envelope

Related: https://github.com/stellar/js-stellar-sdk/pull/1766, https://github.com/stellar/js-stellar-sdk/issues/1789

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.scval-shapes.p30-map-depth-over`
