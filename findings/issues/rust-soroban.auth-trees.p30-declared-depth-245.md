<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 30.0.0: the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 16252 (soroban.auth-trees.p30-declared-depth-245)

## What happens

`stellar-xdr` 30.0.0 answers 2 of the checks of corpus entry `soroban.auth-trees.p30-declared-depth-245` differently from the Stellar network.

## Minimal vector

Entry `soroban.auth-trees.p30-declared-depth-245` (kind Xdr, family `soroban.auth-trees`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 16252

```json
{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"f5cdff819ef77947848d1f4d92f4bf1f6b433080d92a8788ac43d490668b4c8b","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"fed730f89cdf351aecd21491085c32ac651c2d3f7e8084090056c31ddf824ec8","ext":"v0","fee_pool":"3830095","id_pool":"0","inflation_seq":0,"ledger_seq":110,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"2ad352429cc2fd10ea4d6d9e8fab1fb94ad3b9e7821aa1b457e248723c493d06","scp_value":{"close_time":"1776729600","ext":{"signed_ms":{"close_time_ms":"1776729600000","lc_value_signature":{"node_id":"GB4WH2VTILAXT2GUDH4KI2ZA2GGYSLJJUYBH6LP42LXI4WNBRIHHDDVT","signature":"6aab95184a1ac86c137bf93ef12a4deb59f3d5d3b44d8e6dafcde04b2f6342cc9581bcf9fc64a524a76c1605e06a19bf355e2c29a9b496b0fbe5d28b2b532a02"}}},"tx_set_hash":"48ddd7aedc50d7e45acb99629bdfadfdcdc0afd3e31bfd0537ae971d9075d80c","upgrades":[]},"skip_list":["bcf7994280e140bbee7bc6dfdaf3a6791ed3821c4db862df4945c919a8ebe110","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000"],"total_coins":"1000000000000000000","tx_set_result_hash":"8210f91f1821f0823bb6785ab093ed896f5a25600628bf43ff07f623e4497d77"}},"scp_info":[],"total_byte_size_of_live_soroban_state":"52498","tx_processing":[{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GAOPYUAM5CZUQOLDWNFCU64WMPTUGAO6LYOWLE3RTPQTLIKLFMOOSH3O","balance":"99996501255","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":109,"seq_time":"1776643200"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776168","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":109}},{"updated":{"data":{"account":{"account_id":"GAOPYUAM5CZU... (48234 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAAD1zf+Bnvd5R4SNH02S9L8fa0MwgNkqh4isQ9SQZotMiwAAAB4q01JCnML9EOpNbZ6Pqx+5StO554IaobRX4khyPEk9Bkjd167cUNfkWsuZYpvfrf3NwK/T4xv9BTeulx2QddgMAAAAAGnmvgAAAAAAAAAAAwAAAZ2tVjAAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQGqrlRhKGshsE3v5PvEqTetZ89XTtE2Oba/N4EsvY0LMlYG8+fxkpSSnbBYF4GoZvzVeLCmptJaw++XSiytTKgKCEPkfGCHwgju2eFqwk+2Jb1olYAYov0P/B/Yj5El9d/7XMPic3zUa7NIUkQhcMqxlHC0/foCECQBWwx3fgk7IAAAAbg3gtrOnZAAAAAAAAAA6cU8AAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS895lCgOFAu+57xt/a86Z5HtOCHE24Yt9JRckZqOvhEAAAAAAAAAAA... (22272 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 16252"}`

### `mainnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 16252

```json
{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"b15470c23985b054938fd96abd76ebeabf2a3554164ba1127f7598a7011dbf3b","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"4cff395f53d5788df85011a1859736eb59dc69327ba7f9aa82285781fe0e1f02","ext":"v0","fee_pool":"3830070","id_pool":"0","inflation_seq":0,"ledger_seq":110,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"bd835682b7021ee24c67f6a100b1928ee7b590f7086f1aa1c74bb70e7ee927f1","scp_value":{"close_time":"1776729600","ext":{"signed_ms":{"close_time_ms":"1776729600000","lc_value_signature":{"node_id":"GDZ3C5SA7QH74NJ7YML2AZC47IR25BR7MUVZNUWE77OJL5OQDDMPCIMS","signature":"fa55b65f5704cd0c66b8748f817bff7b0061574f94b0da86284c25788ad154a3f0271ca5e4c6ea21be51417157cb5ab54245f0f28ca565277c5cb41c6ec2290e"}}},"tx_set_hash":"c911578faa9982015f2e8cec3e3b0d10bb38deeb499236bd012af878a1fb26a8","upgrades":[]},"skip_list":["bb1f9553b696c78e423e116942723c85dfc8628d2dad1f2a760fd0ce6833e4dd","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000"],"total_coins":"1000000000000000000","tx_set_result_hash":"fb01562f0a06c2001221301c6a680b30b910e9827b95ade20317d032c1d2dc20"}},"scp_info":[],"total_byte_size_of_live_soroban_state":"52498","tx_processing":[{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GAOPYUAM5CZUQOLDWNFCU64WMPTUGAO6LYOWLE3RTPQTLIKLFMOOSH3O","balance":"99996501280","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":109,"seq_time":"1776643200"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776168","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":109}},{"updated":{"data":{"account":{"account_id":"GAOPYUAM5CZU... (48234 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAACxVHDCOYWwVJOP2Wq9duvqvyo1VBZLoRJ/dZinAR2/OwAAAB69g1aCtwIe4kxn9qEAsZKO57WQ9whvGqHHS7cOfukn8ckRV4+qmYIBXy6M7D47DRC7ON7rSZI2vQEq+Hih+yaoAAAAAGnmvgAAAAAAAAAAAwAAAZ2tVjAAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQPpVtl9XBM0MZrh0j4F7/3sAYVdPlLDahihMJXiK0VSj8CccpeTG6iG+UUFxV8tatUJF8PKMpWUnfFy0HG7CKQ77AVYvCgbCABIhMBxqaAswuRDpgnuVreIDF9AywdLcIEz/OV9T1XiN+FARoYWXNutZ3Gkye6f5qoIoV4H+Dh8CAAAAbg3gtrOnZAAAAAAAAAA6cTYAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS7H5VTtpbHjkI+EWlCcjyF38hijS2tHyp2D9DOaDPk3QAAAAAAAAAA... (22272 chars)"}`

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
