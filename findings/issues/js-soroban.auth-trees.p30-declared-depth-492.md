<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.4.1: the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth (soroban.auth-trees.p30-declared-depth-492)

## What happens

On the input below, `@stellar/stellar-sdk` 16.4.1 answers differently from the Stellar network in 4 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `soroban.auth-trees.p30-declared-depth-492` (family `soroban.auth-trees`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `testnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 30344,
  "op": "decode_encode",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAAAbPTtsqFxxvuG+piarkEmqSeg3osRe4+Cpg/EKORuV5wAAAB6POxUUaFoa4CfREB013bocHndVdaNCIMrQ4MhTzva2WeGGnJ8yE5gBN+/EJKDVh28fG8eDPJhyjnI1+xoUTz6TAAAAAGnpYQAAAAAAAAAAAwAAAZ23ougAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQDDoZz3dccnTCATwzqRuDvLDJD71QaodGDTpqeDTnuSPl0jT9vC7ypN66KnG0O2vEV+X3bWpLHMQWJxBty/qSgt18o/KTz+v2ylfHQk9QRC+FzHiSagdjb7X3bo5KFpn+MYGu3GeETUSdKfBPa9jwj/5Vy0lMRHcF2pHrCziREjpAAAAcA3gtrOnZAAAAAAAAAA0KlkAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGSFV+f5x6r04qQKss+eUbjZYRaQUcH1/G/PXKGmEPOqLgAAAAAAAAAA... (40460 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAAAbPTtsqFxxvuG+piarkEmqSeg3osRe4+Cpg/EKORuV5wAAAB6POxUUaFoa4CfREB013bocHndVdaNCIMrQ4MhTzva2WeGGnJ8yE5gBN+/EJKDVh28fG8eDPJhyjnI1+xoUTz6TAAAAAGnpYQAAAAAAAAAAAwAAAZ23ougAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQDDoZz3dccnTCATwzqRuDvLDJD71QaodGDTpqeDTnuSPl0jT9vC7ypN66KnG0O2vEV+X3bWpLHMQWJxBty/qSgt18o/KTz+v2ylfHQk9QRC+FzHiSagdjb7X3bo5KFpn+MYGu3GeETUSdKfBPa9jwj/5Vy0lMRHcF2pHrCziREjpAAAAcA3gtrOnZAAAAAAAAAA0KlkAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGSFV+f5x6r04qQKss+eUbjZYRaQUcH1/G/PXKGmEPOqLgAAAAAAAAAA... (40460 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `testnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 30344,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAAAbPTtsqFxxvuG+piarkEmqSeg3osRe4+Cpg/EKORuV5wAAAB6POxUUaFoa4CfREB013bocHndVdaNCIMrQ4MhTzva2WeGGnJ8yE5gBN+/EJKDVh28fG8eDPJhyjnI1+xoUTz6TAAAAAGnpYQAAAAAAAAAAAwAAAZ23ougAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQDDoZz3dccnTCATwzqRuDvLDJD71QaodGDTpqeDTnuSPl0jT9vC7ypN66KnG0O2vEV+X3bWpLHMQWJxBty/qSgt18o/KTz+v2ylfHQk9QRC+FzHiSagdjb7X3bo5KFpn+MYGu3GeETUSdKfBPa9jwj/5Vy0lMRHcF2pHrCziREjpAAAAcA3gtrOnZAAAAAAAAAA0KlkAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGSFV+f5x6r04qQKss+eUbjZYRaQUcH1/G/PXKGmEPOqLgAAAAAAAAAA... (40460 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"1b3d3b6ca85c71bee1bea626ab9049aa49e837a2c45ee3e0a983f10a391b95e7","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"c606bb719e11351274a7c13daf63c23ff9572d253111dc176a47ac2ce24448e9","ext":"v0","fee_pool":"3418713","id_pool":"0","inflation_seq":0,"ledger_seq":112,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"8f3b1514685a1ae027d1101d35ddba1c1e775575a34220cad0e0c853cef6b659","scp_value":{"close_time":"1776902400","ext":{"signed_ms":{"close_time_ms":"1776902400000","lc_value_sign... (87628 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 30344,
  "op": "decode_encode",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAAA4PIfbxFmOis0vWeXZvVjO7ANrPpfrBpK+I6Rexntt/wAAAB6ETz5HRiJTcUdrsjZcWqc+XZD8muhC19qrQP8u2XF9OmuZBVg4W0PAsLE6TRR9tX99ribOrSnfGH/h8O6ns2DHAAAAAGnpYQAAAAAAAAAAAwAAAZ23ougAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQD7DshFnfpGkZYOJHA6TiEPpztqZnehZbnUjLkmbSx/tiDi22wcO5l+Bbg/xCeCNy2JeljhZmCSvPyXTwwZKVgTrHGGE7jHAHa4DhMILXAoL88mkiTbtnoM9N23vRT5UimWTcf4fLbx6TyWVyxWuZZtq34XkQ+i8tzansc+RQmtkAAAAcA3gtrOnZAAAAAAAAAA0KkAAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGT+NAJiuIqaEIIhDzWrtobIFvaN4SBza3QNr/JzIxMEGwAAAAAAAAAA... (40460 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAAA4PIfbxFmOis0vWeXZvVjO7ANrPpfrBpK+I6Rexntt/wAAAB6ETz5HRiJTcUdrsjZcWqc+XZD8muhC19qrQP8u2XF9OmuZBVg4W0PAsLE6TRR9tX99ribOrSnfGH/h8O6ns2DHAAAAAGnpYQAAAAAAAAAAAwAAAZ23ougAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQD7DshFnfpGkZYOJHA6TiEPpztqZnehZbnUjLkmbSx/tiDi22wcO5l+Bbg/xCeCNy2JeljhZmCSvPyXTwwZKVgTrHGGE7jHAHa4DhMILXAoL88mkiTbtnoM9N23vRT5UimWTcf4fLbx6TyWVyxWuZZtq34XkQ+i8tzansc+RQmtkAAAAcA3gtrOnZAAAAAAAAAA0KkAAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGT+NAJiuIqaEIIhDzWrtobIFvaN4SBza3QNr/JzIxMEGwAAAAAAAAAA... (40460 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 30344,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAAA4PIfbxFmOis0vWeXZvVjO7ANrPpfrBpK+I6Rexntt/wAAAB6ETz5HRiJTcUdrsjZcWqc+XZD8muhC19qrQP8u2XF9OmuZBVg4W0PAsLE6TRR9tX99ribOrSnfGH/h8O6ns2DHAAAAAGnpYQAAAAAAAAAAAwAAAZ23ougAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQD7DshFnfpGkZYOJHA6TiEPpztqZnehZbnUjLkmbSx/tiDi22wcO5l+Bbg/xCeCNy2JeljhZmCSvPyXTwwZKVgTrHGGE7jHAHa4DhMILXAoL88mkiTbtnoM9N23vRT5UimWTcf4fLbx6TyWVyxWuZZtq34XkQ+i8tzansc+RQmtkAAAAcA3gtrOnZAAAAAAAAAA0KkAAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGT+NAJiuIqaEIIhDzWrtobIFvaN4SBza3QNr/JzIxMEGwAAAAAAAAAA... (40460 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"383c87dbc4598e8acd2f59e5d9bd58ceec036b3e97eb0692be23a45ec67b6dff","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"659371fe1f2dbc7a4f2595cb15ae659b6adf85e443e8bcb736a7b1cf91426b64","ext":"v0","fee_pool":"3418688","id_pool":"0","inflation_seq":0,"ledger_seq":112,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"844f3e4746225371476bb2365c5aa73e5d90fc9ae842d7daab40ff2ed9717d3a","scp_value":{"close_time":"1776902400","ext":{"signed_ms":{"close_time_ms":"1776902400000","lc_value_sign... (87628 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk js --version 16.4.1 --entry soroban.auth-trees.p30-declared-depth-492
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-492/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-492/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-492/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-492/mainnet.xdr): decode = must-accept, roundtrip = true
- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-492/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-492/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-492/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-492/testnet.xdr): decode = must-accept, roundtrip = true

How the corpus entry was built and checked: an address entry declaring a chain 492 invocations deep below its root, 493 with the root, the deepest envelope core's depth check accepts (xdr.envelope.depth 1000): core includes it and the host, which decodes at depth 500, traps. the ledger needs xdr.meta.depth (1012) to decode

Related: https://github.com/stellar/wallet-backend/issues/746

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `soroban.auth-trees.p30-declared-depth-492`
