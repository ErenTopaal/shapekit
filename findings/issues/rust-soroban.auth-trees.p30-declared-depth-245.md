<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 30.0.0: the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 16252 (soroban.auth-trees.p30-declared-depth-245)

## What happens

`stellar-xdr` 30.0.0 answers 2 of the checks of corpus entry `soroban.auth-trees.p30-declared-depth-245` differently from the Stellar network.

## Minimal vector

Entry `soroban.auth-trees.p30-declared-depth-245` (kind Xdr, family `soroban.auth-trees`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 16252

```json
{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"f543530ff189d48db9542f32a017a7103a5aabe8ca110381bc93c587d331aed6","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"78128d186ad92a946f0af74888492529fba0a19c2c66e6be572c36aaf5e9a2ea","ext":"v0","fee_pool":"3315900","id_pool":"0","inflation_seq":0,"ledger_seq":110,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"8f50a47935bf0df0dd8c54ab8aedfad3a05f83b703f972fbe36da8cda9842f41","scp_value":{"close_time":"1776729600","ext":{"signed_ms":{"close_time_ms":"1776729600000","lc_value_signature":{"node_id":"GB4WH2VTILAXT2GUDH4KI2ZA2GGYSLJJUYBH6LP42LXI4WNBRIHHDDVT","signature":"790b88f62098221f82a0b9ed016eba4e175c9a8af0c84cf1d6cdf4d8b95d988e6eb6e3c09c9b20e01c8e4a2c590b8f339943aab77d8b720caaea8458ffbfe006"}}},"tx_set_hash":"5d6df08e819980fbf151b77c38b38d5c17a6fe342abed6a6cd360a691374c477","upgrades":[]},"skip_list":["8557e7f9c7aaf4e2a40ab2cf9e51b8d961169051c1f5fc6fcf5ca1a610f3aa2e","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000"],"total_coins":"1000000000000000000","tx_set_result_hash":"1dcfa736a1dd35fc497ec4b1b7779e702948b2b5860fa1f86d8822e2a09a48fd"}},"scp_info":[],"total_byte_size_of_live_soroban_state":"40407","tx_processing":[{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GAOPYUAM5CZUQOLDWNFCU64WMPTUGAO6LYOWLE3RTPQTLIKLFMOOSH3O","balance":"99996956709","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":109,"seq_time":"1776643200"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776168","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":109}},{"updated":{"data":{"account":{"account_id":"GAOPYUAM5CZU... (48234 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAAD1Q1MP8YnUjblULzKgF6cQOlqr6MoRA4G8k8WH0zGu1gAAAB6PUKR5Nb8N8N2MVKuK7frToF+DtwP5cvvjbajNqYQvQV1t8I6BmYD78VG3fDizjVwXpv40Kr7Wps02CmkTdMR3AAAAAGnmvgAAAAAAAAAAAwAAAZ2tVjAAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQHkLiPYgmCIfgqC57QFuuk4XXJqK8MhM8dbN9Ni5XZiObrbjwJybIOAcjkosWQuPM5lDqrd9i3IMquqEWP+/4AYdz6c2od01/El+xLG3d55wKUiytYYPofhtiCLioJpI/XgSjRhq2SqUbwr3SIhJJSn7oKGcLGbmvlcsNqr16aLqAAAAbg3gtrOnZAAAAAAAAAAymLwAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGSFV+f5x6r04qQKss+eUbjZYRaQUcH1/G/PXKGmEPOqLgAAAAAAAAAA... (22272 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 16252"}`

### `mainnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 16252

```json
{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"44e084b10e7cc573dd64c3e3463feb88c0b49c4227ff3b616bfe4c16b28731d9","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"ee25351d178e91497669e08548b095a0e124b42fa454275524ea8b943db98f32","ext":"v0","fee_pool":"3315875","id_pool":"0","inflation_seq":0,"ledger_seq":110,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"ada6e5ddc31699621c2bfa817353c4448eba438c0ebd25fa0e48607eb9861ff7","scp_value":{"close_time":"1776729600","ext":{"signed_ms":{"close_time_ms":"1776729600000","lc_value_signature":{"node_id":"GDZ3C5SA7QH74NJ7YML2AZC47IR25BR7MUVZNUWE77OJL5OQDDMPCIMS","signature":"2d2047f363414e55b4eae40f4f54528f8e13030faf2d0480e067f426d87af2142ee606dd8cb050d3110401ac1c2c66156d7100092de74b4c98232a7c219ec301"}}},"tx_set_hash":"5c212cb434cca55f099c73d41b83b3300f1ba3f1096437dc88cd7db3058d1a9e","upgrades":[]},"skip_list":["fe340262b88a9a1082210f35abb686c816f68de120736b740daff2732313041b","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000"],"total_coins":"1000000000000000000","tx_set_result_hash":"9742d2d9701a3cfc06ea981926ad02467d97ccd457b4eb3ced4f5accb1129b1e"}},"scp_info":[],"total_byte_size_of_live_soroban_state":"40407","tx_processing":[{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GAOPYUAM5CZUQOLDWNFCU64WMPTUGAO6LYOWLE3RTPQTLIKLFMOOSH3O","balance":"99996956734","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":109,"seq_time":"1776643200"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776168","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":109}},{"updated":{"data":{"account":{"account_id":"GAOPYUAM5CZU... (48234 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAABE4ISxDnzFc91kw+NGP+uIwLScQif/O2Fr/kwWsocx2QAAAB6tpuXdwxaZYhwr+oFzU8REjrpDjA69JfoOSGB+uYYf91whLLQ0zKVfCZxz1BuDszAPG6PxCWQ33IjNfbMFjRqeAAAAAGnmvgAAAAAAAAAAAwAAAZ2tVjAAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQC0gR/NjQU5VtOrkD09UUo+OEwMPry0EgOBn9CbYevIULuYG3YywUNMRBAGsHCxmFW1xAAkt50tMmCMqfCGewwGXQtLZcBo8/AbqmBkmrQJGfZfM1Fe06zztT1rMsRKbHu4lNR0XjpFJdmnghUiwlaDhJLQvpFQnVSTqi5Q9uY8yAAAAbg3gtrOnZAAAAAAAAAAymKMAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGT+NAJiuIqaEIIhDzWrtobIFvaN4SBza3QNr/JzIxMEGwAAAAAAAAAA... (22272 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 16252"}`

## Reproduce

```sh
shapekit conformance run --sdk rust --version 30.0.0 --entry soroban.auth-trees.p30-declared-depth-245
```

## Expected value and its source

- corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.xdr
- corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.xdr

Entry notes: an address entry whose matched root declares a chain 245 invocations deep below it that no call reaches, the deepest the host still decodes; the transaction succeeds and the declared nodes are never checked

Related: https://github.com/stellar/wallet-backend/issues/746

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.auth-trees.p30-declared-depth-245`
