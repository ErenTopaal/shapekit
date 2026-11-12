<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 30.0.0: the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 15676 (soroban.auth-trees.p30-declared-depth-492)

## What happens

`stellar-xdr` 30.0.0 answers 2 of the checks of corpus entry `soroban.auth-trees.p30-declared-depth-492` differently from the Stellar network.

## Minimal vector

Entry `soroban.auth-trees.p30-declared-depth-492` (kind Xdr, family `soroban.auth-trees`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 15676

```json
{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"b47e9a9f7b4e3f286537f331cb528dfe999b020f5ac1d87ad2fdb9b9e1117707","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"e09b02b3e7368a4fef3de527a0f71897a5504e9315d5fe3c3c739d007d558b1c","ext":"v0","fee_pool":"3932910","id_pool":"0","inflation_seq":0,"ledger_seq":112,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"5b5b24254afdd3a5f6c0cab8d8d7ea9c32f060fdd85f733d6e4ed4f5c0eb189d","scp_value":{"close_time":"1776902400","ext":{"signed_ms":{"close_time_ms":"1776902400000","lc_value_signature":{"node_id":"GB4WH2VTILAXT2GUDH4KI2ZA2GGYSLJJUYBH6LP42LXI4WNBRIHHDDVT","signature":"c92493e83fd232fe3403f461bcbf3a015994656eb1882ad7de4766dfbc4a80a1603f837fe6e0351e08338828f17ce3e35cfc7482baa6cfbf0627e8bfbc2d8507"}}},"tx_set_hash":"02a9955c423e4821bcb0e00220ef65f731228ecfa33b006d69b5d584603835a7","upgrades":[]},"skip_list":["bcf7994280e140bbee7bc6dfdaf3a6791ed3821c4db862df4945c919a8ebe110","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000"],"total_coins":"1000000000000000000","tx_set_result_hash":"5a44cb5bfa06525b9279829513fa0aa6a67caa7ec4d0013c5a6b4fbbae4febf5"}},"scp_info":[],"total_byte_size_of_live_soroban_state":"52498","tx_processing":[{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GAOPYUAM5CZUQOLDWNFCU64WMPTUGAO6LYOWLE3RTPQTLIKLFMOOSH3O","balance":"99996427276","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":111,"seq_time":"1776816000"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776170","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":111}},{"updated":{"data":{"account":{"account_id":"GAOPYUAM5CZU... (87672 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAAC0fpqfe04/KGU38zHLUo3+mZsCD1rB2HrS/bm54RF3BwAAAB5bWyQlSv3TpfbAyrjY1+qcMvBg/dhfcz1uTtT1wOsYnQKplVxCPkghvLDgAiDvZfcxIo7PozsAbWm11YRgODWnAAAAAGnpYQAAAAAAAAAAAwAAAZ23ougAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQMkkk+g/0jL+NAP0Yby/OgFZlGVusYgq195HZt+8SoChYD+Df+bgNR4IM4go8Xzj41z8dIK6ps+/Bifov7wthQdaRMtb+gZSW5J5gpUT+gqmpnyqfsTQATxaa0+7rk/r9eCbArPnNopP7z3lJ6D3GJelUE6TFdX+PDxznQB9VYscAAAAcA3gtrOnZAAAAAAAAAA8Au4AAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS895lCgOFAu+57xt/a86Z5HtOCHE24Yt9JRckZqOvhEAAAAAAAAAAA... (40460 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 15676"}`

### `mainnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 15676

```json
{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"5407b4816fcf8d44ae475d2ffedf5762de15cb5edfd74b45f58d1226fe5cbc85","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"10c9ca872676afca8e4d8920c5c7a05d50b19916b6062a97355ab01585547259","ext":"v0","fee_pool":"3932885","id_pool":"0","inflation_seq":0,"ledger_seq":112,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"9cbf46b4e26e89475bcb78352bb3e5424edb9367ef691515d1c64090334920d6","scp_value":{"close_time":"1776902400","ext":{"signed_ms":{"close_time_ms":"1776902400000","lc_value_signature":{"node_id":"GDZ3C5SA7QH74NJ7YML2AZC47IR25BR7MUVZNUWE77OJL5OQDDMPCIMS","signature":"f83d7adea0f96de675be865608147e297ced3b83c95d51a35eacf6315c81089c5b8ede8409083b76322a9a40e3c5b3d31c91c19ceb5e9f734765cf6b13261b06"}}},"tx_set_hash":"c223b49ce0f12560f879ae9bb8101b9fd9ae5d71c081ca57589ae2ff5f3cb2c7","upgrades":[]},"skip_list":["bb1f9553b696c78e423e116942723c85dfc8628d2dad1f2a760fd0ce6833e4dd","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000"],"total_coins":"1000000000000000000","tx_set_result_hash":"2240f77c539d766dfd62a44cb7307a3bc89f5a2e577d37d91dc1ae275b5d13cb"}},"scp_info":[],"total_byte_size_of_live_soroban_state":"52498","tx_processing":[{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GAOPYUAM5CZUQOLDWNFCU64WMPTUGAO6LYOWLE3RTPQTLIKLFMOOSH3O","balance":"99996427301","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":111,"seq_time":"1776816000"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776170","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":111}},{"updated":{"data":{"account":{"account_id":"GAOPYUAM5CZU... (87672 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAABUB7SBb8+NRK5HXS/+31di3hXLXt/XS0X1jRIm/ly8hQAAAB6cv0a04m6JR1vLeDUrs+VCTtuTZ+9pFRXRxkCQM0kg1sIjtJzg8SVg+Hmum7gQG5/Zrl1xwIHKV1ia4v9fPLLHAAAAAGnpYQAAAAAAAAAAAwAAAZ23ougAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQPg9et6g+W3mdb6GVggUfil87TuDyV1Ro16s9jFcgQicW47ehAkIO3YyKppA48Wz0xyRwZzrXp9zR2XPaxMmGwYiQPd8U512bf1ipEy3MHo7yJ9aLld9N9kdwa4nW10TyxDJyocmdq/Kjk2JIMXHoF1QsZkWtgYqlzVasBWFVHJZAAAAcA3gtrOnZAAAAAAAAAA8AtUAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS7H5VTtpbHjkI+EWlCcjyF38hijS2tHyp2D9DOaDPk3QAAAAAAAAAA... (40460 chars)"}`

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
