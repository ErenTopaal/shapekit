<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 30.0.0: the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 15676 (soroban.auth-trees.p30-declared-depth-246)

## What happens

`stellar-xdr` 30.0.0 answers 2 of the checks of corpus entry `soroban.auth-trees.p30-declared-depth-246` differently from the Stellar network.

## Minimal vector

Entry `soroban.auth-trees.p30-declared-depth-246` (kind Xdr, family `soroban.auth-trees`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 15676

```json
{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"8f3b1514685a1ae027d1101d35ddba1c1e775575a34220cad0e0c853cef6b659","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"10876dc232dc54c6cbf5b7b2dc90ee083b66c6f2d9ff081b4a30b2993fd626c8","ext":"v0","fee_pool":"3352293","id_pool":"0","inflation_seq":0,"ledger_seq":111,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"f543530ff189d48db9542f32a017a7103a5aabe8ca110381bc93c587d331aed6","scp_value":{"close_time":"1776816000","ext":{"signed_ms":{"close_time_ms":"1776816000000","lc_value_signature":{"node_id":"GB4WH2VTILAXT2GUDH4KI2ZA2GGYSLJJUYBH6LP42LXI4WNBRIHHDDVT","signature":"bdbbf251e72ff4a7292ea6db22acca5fac9c2b33f88a1c1f30ff5d965e48934ede4db6d898265a69e2f62ef9782f0374f464195488c226b6f337cfa86df5d401"}}},"tx_set_hash":"af92dff7860f08d1c1abf8259eb41a14cfea9d9fe21a1286e9bdc6efd811105c","upgrades":[]},"skip_list":["8557e7f9c7aaf4e2a40ab2cf9e51b8d961169051c1f5fc6fcf5ca1a610f3aa2e","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000"],"total_coins":"1000000000000000000","tx_set_result_hash":"fa7fabd29254830bd7fa0585b05e3f0193c47a48d822b8a299eb43b5bca74e5e"}},"scp_info":[],"total_byte_size_of_live_soroban_state":"40407","tx_processing":[{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GAOPYUAM5CZUQOLDWNFCU64WMPTUGAO6LYOWLE3RTPQTLIKLFMOOSH3O","balance":"99996919125","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":110,"seq_time":"1776729600"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776169","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":110}},{"updated":{"data":{"account":{"account_id":"GAOPYUAM5CZU... (47820 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAACPOxUUaFoa4CfREB013bocHndVdaNCIMrQ4MhTzva2WQAAAB71Q1MP8YnUjblULzKgF6cQOlqr6MoRA4G8k8WH0zGu1q+S3/eGDwjRwav4JZ60GhTP6p2f4hoShum9xu/YERBcAAAAAGnoD4AAAAAAAAAAAwAAAZ2yfIwAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQL278lHnL/SnKS6m2yKsyl+snCsz+IocHzD/XZZeSJNO3k222JgmWmni9i75eC8DdPRkGVSIwia28zfPqG311AH6f6vSklSDC9f6BYWwXj8Bk8R6SNgiuKKZ60O1vKdOXhCHbcIy3FTGy/W3styQ7gg7Zsby2f8IG0owspk/1ibIAAAAbw3gtrOnZAAAAAAAAAAzJuUAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGSFV+f5x6r04qQKss+eUbjZYRaQUcH1/G/PXKGmEPOqLgAAAAAAAAAA... (22092 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 15676"}`

### `mainnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 15676

```json
{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"844f3e4746225371476bb2365c5aa73e5d90fc9ae842d7daab40ff2ed9717d3a","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"44bee93e892e5154a7b261fff20a704736cc0cb4dc64e54a434def528906a0a8","ext":"v0","fee_pool":"3352268","id_pool":"0","inflation_seq":0,"ledger_seq":111,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"44e084b10e7cc573dd64c3e3463feb88c0b49c4227ff3b616bfe4c16b28731d9","scp_value":{"close_time":"1776816000","ext":{"signed_ms":{"close_time_ms":"1776816000000","lc_value_signature":{"node_id":"GDZ3C5SA7QH74NJ7YML2AZC47IR25BR7MUVZNUWE77OJL5OQDDMPCIMS","signature":"c1eb75c889bf3073f3374f294c09ab4bddc33e94361e6a806361962b1d57fec8b4643e8711ae33201a8d8a6f5c4d17a42577b5621adc24d27256032d30637002"}}},"tx_set_hash":"c1be49c1e2f2a88d8eb63534c766b5ccca687d345c33fbbbeb81beec803691b3","upgrades":[]},"skip_list":["fe340262b88a9a1082210f35abb686c816f68de120736b740daff2732313041b","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000"],"total_coins":"1000000000000000000","tx_set_result_hash":"7e0d257f5efd179e2dde2434e6f58372993aee544260641a469e1cc4b66af733"}},"scp_info":[],"total_byte_size_of_live_soroban_state":"40407","tx_processing":[{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GAOPYUAM5CZUQOLDWNFCU64WMPTUGAO6LYOWLE3RTPQTLIKLFMOOSH3O","balance":"99996919150","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":110,"seq_time":"1776729600"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776169","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":110}},{"updated":{"data":{"account":{"account_id":"GAOPYUAM5CZU... (47820 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAACETz5HRiJTcUdrsjZcWqc+XZD8muhC19qrQP8u2XF9OgAAAB5E4ISxDnzFc91kw+NGP+uIwLScQif/O2Fr/kwWsocx2cG+ScHi8qiNjrY1NMdmtczKaH00XDP7u+uBvuyANpGzAAAAAGnoD4AAAAAAAAAAAwAAAZ2yfIwAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQMHrdciJvzBz8zdPKUwJq0vdwz6UNh5qgGNhlisdV/7ItGQ+hxGuMyAajYpvXE0XpCV3tWIa3CTSclYDLTBjcAJ+DSV/Xv0Xni3eJDTm9YNymTruVEJgZBpGnhzEtmr3M0S+6T6JLlFUp7Jh//IKcEc2zAy03GTlSkNN71KJBqCoAAAAbw3gtrOnZAAAAAAAAAAzJswAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGT+NAJiuIqaEIIhDzWrtobIFvaN4SBza3QNr/JzIxMEGwAAAAAAAAAA... (22092 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 15676"}`

## Reproduce

```sh
shapekit conformance run --sdk rust --version 30.0.0 --entry soroban.auth-trees.p30-declared-depth-246
```

## Expected value and its source

- corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-246/mainnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-246/mainnet.xdr
- corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-246/testnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-246/testnet.xdr

Entry notes: an address entry declaring a chain 246 invocations deep below its root, one past what the host decodes at depth 500: core includes it and the host traps

Related: https://github.com/stellar/wallet-backend/issues/746

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.auth-trees.p30-declared-depth-246`
