<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 30.0.0: the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 15676 (soroban.auth-trees.p30-declared-depth-246)

## What happens

`stellar-xdr` 30.0.0 answers 2 of the checks of corpus entry `soroban.auth-trees.p30-declared-depth-246` differently from the Stellar network.

## Minimal vector

Entry `soroban.auth-trees.p30-declared-depth-246` (kind Xdr, family `soroban.auth-trees`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 15676

```json
{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"5b5b24254afdd3a5f6c0cab8d8d7ea9c32f060fdd85f733d6e4ed4f5c0eb189d","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"a5bbc0d12e3cc01040db40a5857cec0440521684a96a568ef11424dd012fcb57","ext":"v0","fee_pool":"3866489","id_pool":"0","inflation_seq":0,"ledger_seq":111,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"f5cdff819ef77947848d1f4d92f4bf1f6b433080d92a8788ac43d490668b4c8b","scp_value":{"close_time":"1776816000","ext":{"signed_ms":{"close_time_ms":"1776816000000","lc_value_signature":{"node_id":"GB4WH2VTILAXT2GUDH4KI2ZA2GGYSLJJUYBH6LP42LXI4WNBRIHHDDVT","signature":"a2e16f16acb1eced5ae4db215ffdb50b31aacfa88a2815d7a4e50f7fde6f4731ae3a13d14e4e7d0d3284b759cd725dbe0f133992e95fdce43894d562e3186403"}}},"tx_set_hash":"e067c15e1ecfa49fbd7604be7255bcebc067a6801c01559b88ec3f6021473082","upgrades":[]},"skip_list":["bcf7994280e140bbee7bc6dfdaf3a6791ed3821c4db862df4945c919a8ebe110","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000"],"total_coins":"1000000000000000000","tx_set_result_hash":"ccf41f8e4020fe7e243a8e6e5df5aa07cc5839a1d7ddb64475faa1810c00d3fc"}},"scp_info":[],"total_byte_size_of_live_soroban_state":"52498","tx_processing":[{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GAOPYUAM5CZUQOLDWNFCU64WMPTUGAO6LYOWLE3RTPQTLIKLFMOOSH3O","balance":"99996463670","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":110,"seq_time":"1776729600"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776169","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":110}},{"updated":{"data":{"account":{"account_id":"GAOPYUAM5CZU... (47820 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAABbWyQlSv3TpfbAyrjY1+qcMvBg/dhfcz1uTtT1wOsYnQAAAB71zf+Bnvd5R4SNH02S9L8fa0MwgNkqh4isQ9SQZotMi+BnwV4ez6SfvXYEvnJVvOvAZ6aAHAFVm4jsP2AhRzCCAAAAAGnoD4AAAAAAAAAAAwAAAZ2yfIwAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQKLhbxasseztWuTbIV/9tQsxqs+oiigV16TlD3/eb0cxrjoT0U5OfQ0yhLdZzXJdvg8TOZLpX9zkOJTVYuMYZAPM9B+OQCD+fiQ6jm5d9aoHzFg5odfdtkR1+qGBDADT/KW7wNEuPMAQQNtApYV87ARAUhaEqWpWjvEUJN0BL8tXAAAAbw3gtrOnZAAAAAAAAAA6/3kAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS895lCgOFAu+57xt/a86Z5HtOCHE24Yt9JRckZqOvhEAAAAAAAAAAA... (22092 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 15676"}`

### `mainnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 15676

```json
{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"9cbf46b4e26e89475bcb78352bb3e5424edb9367ef691515d1c64090334920d6","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"d003ebdce5d31a255f649ebb24498fb42c2e56ff3d80fb452e6a06dce72f75cf","ext":"v0","fee_pool":"3866464","id_pool":"0","inflation_seq":0,"ledger_seq":111,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"b15470c23985b054938fd96abd76ebeabf2a3554164ba1127f7598a7011dbf3b","scp_value":{"close_time":"1776816000","ext":{"signed_ms":{"close_time_ms":"1776816000000","lc_value_signature":{"node_id":"GDZ3C5SA7QH74NJ7YML2AZC47IR25BR7MUVZNUWE77OJL5OQDDMPCIMS","signature":"c088fb05ee5d3c295965040f33ff17ec1429d9c954c32ed247aee5627f6c7f226b69bd2e49df3c7c70d83b44577ef6b42533eaa77b2941aa7c37dd6c84b42707"}}},"tx_set_hash":"a79fdd2aa6bac132d11fb79863a18afc50a9e97c0ebaec884dd0f126d6bf9bb1","upgrades":[]},"skip_list":["bb1f9553b696c78e423e116942723c85dfc8628d2dad1f2a760fd0ce6833e4dd","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000"],"total_coins":"1000000000000000000","tx_set_result_hash":"89d4c2690595be893097c90f6f223e58e15b242c82b228a2dda884864d93a6cb"}},"scp_info":[],"total_byte_size_of_live_soroban_state":"52498","tx_processing":[{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GAOPYUAM5CZUQOLDWNFCU64WMPTUGAO6LYOWLE3RTPQTLIKLFMOOSH3O","balance":"99996463695","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":110,"seq_time":"1776729600"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776169","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":110}},{"updated":{"data":{"account":{"account_id":"GAOPYUAM5CZU... (47820 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAACcv0a04m6JR1vLeDUrs+VCTtuTZ+9pFRXRxkCQM0kg1gAAAB6xVHDCOYWwVJOP2Wq9duvqvyo1VBZLoRJ/dZinAR2/O6ef3SqmusEy0R+3mGOhivxQqel8DrrsiE3Q8SbWv5uxAAAAAGnoD4AAAAAAAAAAAwAAAZ2yfIwAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQMCI+wXuXTwpWWUEDzP/F+wUKdnJVMMu0keu5WJ/bH8ia2m9LknfPHxw2DtEV372tCUz6qd7KUGqfDfdbIS0JweJ1MJpBZW+iTCXyQ9vIj5Y4VskLIKyKKLdqISGTZOmy9AD69zl0xolX2SeuyRJj7QsLlb/PYD7RS5qBtznL3XPAAAAbw3gtrOnZAAAAAAAAAA6/2AAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS7H5VTtpbHjkI+EWlCcjyF38hijS2tHyp2D9DOaDPk3QAAAAAAAAAA... (22092 chars)"}`

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
