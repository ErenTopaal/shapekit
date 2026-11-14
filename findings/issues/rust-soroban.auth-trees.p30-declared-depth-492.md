<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 30.0.0: the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 15676 (soroban.auth-trees.p30-declared-depth-492)

## What happens

`stellar-xdr` 30.0.0 answers 2 of the checks of corpus entry `soroban.auth-trees.p30-declared-depth-492` differently from the Stellar network.

## Minimal vector

Entry `soroban.auth-trees.p30-declared-depth-492` (kind Xdr, family `soroban.auth-trees`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 15676

```json
{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"1b3d3b6ca85c71bee1bea626ab9049aa49e837a2c45ee3e0a983f10a391b95e7","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"c606bb719e11351274a7c13daf63c23ff9572d253111dc176a47ac2ce24448e9","ext":"v0","fee_pool":"3418713","id_pool":"0","inflation_seq":0,"ledger_seq":112,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"8f3b1514685a1ae027d1101d35ddba1c1e775575a34220cad0e0c853cef6b659","scp_value":{"close_time":"1776902400","ext":{"signed_ms":{"close_time_ms":"1776902400000","lc_value_signature":{"node_id":"GB4WH2VTILAXT2GUDH4KI2ZA2GGYSLJJUYBH6LP42LXI4WNBRIHHDDVT","signature":"30e8673ddd71c9d30804f0cea46e0ef2c3243ef541aa1d1834e9a9e0d39ee48f9748d3f6f0bbca937ae8a9c6d0edaf115f97ddb5a92c7310589c41b72fea4a0b"}}},"tx_set_hash":"e1869c9f3213980137efc424a0d5876f1f1bc7833c98728e7235fb1a144f3e93","upgrades":[]},"skip_list":["8557e7f9c7aaf4e2a40ab2cf9e51b8d961169051c1f5fc6fcf5ca1a610f3aa2e","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000"],"total_coins":"1000000000000000000","tx_set_result_hash":"75f28fca4f3fafdb295f1d093d4110be1731e249a81d8dbed7ddba39285a67f8"}},"scp_info":[],"total_byte_size_of_live_soroban_state":"40407","tx_processing":[{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GAOPYUAM5CZUQOLDWNFCU64WMPTUGAO6LYOWLE3RTPQTLIKLFMOOSH3O","balance":"99996882732","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":111,"seq_time":"1776816000"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776170","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":111}},{"updated":{"data":{"account":{"account_id":"GAOPYUAM5CZU... (87672 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAAAbPTtsqFxxvuG+piarkEmqSeg3osRe4+Cpg/EKORuV5wAAAB6POxUUaFoa4CfREB013bocHndVdaNCIMrQ4MhTzva2WeGGnJ8yE5gBN+/EJKDVh28fG8eDPJhyjnI1+xoUTz6TAAAAAGnpYQAAAAAAAAAAAwAAAZ23ougAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQDDoZz3dccnTCATwzqRuDvLDJD71QaodGDTpqeDTnuSPl0jT9vC7ypN66KnG0O2vEV+X3bWpLHMQWJxBty/qSgt18o/KTz+v2ylfHQk9QRC+FzHiSagdjb7X3bo5KFpn+MYGu3GeETUSdKfBPa9jwj/5Vy0lMRHcF2pHrCziREjpAAAAcA3gtrOnZAAAAAAAAAA0KlkAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGSFV+f5x6r04qQKss+eUbjZYRaQUcH1/G/PXKGmEPOqLgAAAAAAAAAA... (40460 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 15676"}`

### `mainnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 15676

```json
{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"383c87dbc4598e8acd2f59e5d9bd58ceec036b3e97eb0692be23a45ec67b6dff","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"659371fe1f2dbc7a4f2595cb15ae659b6adf85e443e8bcb736a7b1cf91426b64","ext":"v0","fee_pool":"3418688","id_pool":"0","inflation_seq":0,"ledger_seq":112,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"844f3e4746225371476bb2365c5aa73e5d90fc9ae842d7daab40ff2ed9717d3a","scp_value":{"close_time":"1776902400","ext":{"signed_ms":{"close_time_ms":"1776902400000","lc_value_signature":{"node_id":"GDZ3C5SA7QH74NJ7YML2AZC47IR25BR7MUVZNUWE77OJL5OQDDMPCIMS","signature":"3ec3b211677e91a46583891c0e938843e9ceda999de8596e75232e499b4b1fed8838b6db070ee65f816e0ff109e08dcb625e9638599824af3f25d3c3064a5604"}}},"tx_set_hash":"6b990558385b43c0b0b13a4d147db57f7dae26cead29df187fe1f0eea7b360c7","upgrades":[]},"skip_list":["fe340262b88a9a1082210f35abb686c816f68de120736b740daff2732313041b","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000"],"total_coins":"1000000000000000000","tx_set_result_hash":"eb1c6184ee31c01dae0384c20b5c0a0bf3c9a48936ed9e833d376def453e548a"}},"scp_info":[],"total_byte_size_of_live_soroban_state":"40407","tx_processing":[{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GAOPYUAM5CZUQOLDWNFCU64WMPTUGAO6LYOWLE3RTPQTLIKLFMOOSH3O","balance":"99996882757","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":111,"seq_time":"1776816000"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776170","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":111}},{"updated":{"data":{"account":{"account_id":"GAOPYUAM5CZU... (87672 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAAA4PIfbxFmOis0vWeXZvVjO7ANrPpfrBpK+I6Rexntt/wAAAB6ETz5HRiJTcUdrsjZcWqc+XZD8muhC19qrQP8u2XF9OmuZBVg4W0PAsLE6TRR9tX99ribOrSnfGH/h8O6ns2DHAAAAAGnpYQAAAAAAAAAAAwAAAZ23ougAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQD7DshFnfpGkZYOJHA6TiEPpztqZnehZbnUjLkmbSx/tiDi22wcO5l+Bbg/xCeCNy2JeljhZmCSvPyXTwwZKVgTrHGGE7jHAHa4DhMILXAoL88mkiTbtnoM9N23vRT5UimWTcf4fLbx6TyWVyxWuZZtq34XkQ+i8tzansc+RQmtkAAAAcA3gtrOnZAAAAAAAAAA0KkAAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGT+NAJiuIqaEIIhDzWrtobIFvaN4SBza3QNr/JzIxMEGwAAAAAAAAAA... (40460 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 15676"}`

## Reproduce

```sh
shapekit conformance run --sdk rust --version 30.0.0 --entry soroban.auth-trees.p30-declared-depth-492
```

## Expected value and its source

- corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-492/mainnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-492/mainnet.xdr
- corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-492/testnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-492/testnet.xdr

Entry notes: an address entry declaring a chain 492 invocations deep below its root, 493 with the root, the deepest envelope core's depth check accepts (xdr.envelope.depth 1000): core includes it and the host, which decodes at depth 500, traps. the ledger needs xdr.meta.depth (1012) to decode

Related: https://github.com/stellar/wallet-backend/issues/746

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.auth-trees.p30-declared-depth-492`
