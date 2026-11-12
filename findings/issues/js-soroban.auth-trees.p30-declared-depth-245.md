<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.4.1: the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth (soroban.auth-trees.p30-declared-depth-245)

## What happens

`@stellar/stellar-sdk` 16.4.1 answers 4 of the checks of corpus entry `soroban.auth-trees.p30-declared-depth-245` differently from the Stellar network.

## Minimal vector

Entry `soroban.auth-trees.p30-declared-depth-245` (kind Xdr, family `soroban.auth-trees`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 16704,
  "op": "decode_encode",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAAD1zf+Bnvd5R4SNH02S9L8fa0MwgNkqh4isQ9SQZotMiwAAAB4q01JCnML9EOpNbZ6Pqx+5StO554IaobRX4khyPEk9Bkjd167cUNfkWsuZYpvfrf3NwK/T4xv9BTeulx2QddgMAAAAAGnmvgAAAAAAAAAAAwAAAZ2tVjAAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQGqrlRhKGshsE3v5PvEqTetZ89XTtE2Oba/N4EsvY0LMlYG8+fxkpSSnbBYF4GoZvzVeLCmptJaw++XSiytTKgKCEPkfGCHwgju2eFqwk+2Jb1olYAYov0P/B/Yj5El9d/7XMPic3zUa7NIUkQhcMqxlHC0/foCECQBWwx3fgk7IAAAAbg3gtrOnZAAAAAAAAAA6cU8AAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS895lCgOFAu+57xt/a86Z5HtOCHE24Yt9JRckZqOvhEAAAAAAAAAAA... (22272 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAAD1zf+Bnvd5R4SNH02S9L8fa0MwgNkqh4isQ9SQZotMiwAAAB4q01JCnML9EOpNbZ6Pqx+5StO554IaobRX4khyPEk9Bkjd167cUNfkWsuZYpvfrf3NwK/T4xv9BTeulx2QddgMAAAAAGnmvgAAAAAAAAAAAwAAAZ2tVjAAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQGqrlRhKGshsE3v5PvEqTetZ89XTtE2Oba/N4EsvY0LMlYG8+fxkpSSnbBYF4GoZvzVeLCmptJaw++XSiytTKgKCEPkfGCHwgju2eFqwk+2Jb1olYAYov0P/B/Yj5El9d/7XMPic3zUa7NIUkQhcMqxlHC0/foCECQBWwx3fgk7IAAAAbg3gtrOnZAAAAAAAAAA6cU8AAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS895lCgOFAu+57xt/a86Z5HtOCHE24Yt9JRckZqOvhEAAAAAAAAAAA... (22272 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `testnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 16704,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAAD1zf+Bnvd5R4SNH02S9L8fa0MwgNkqh4isQ9SQZotMiwAAAB4q01JCnML9EOpNbZ6Pqx+5StO554IaobRX4khyPEk9Bkjd167cUNfkWsuZYpvfrf3NwK/T4xv9BTeulx2QddgMAAAAAGnmvgAAAAAAAAAAAwAAAZ2tVjAAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQGqrlRhKGshsE3v5PvEqTetZ89XTtE2Oba/N4EsvY0LMlYG8+fxkpSSnbBYF4GoZvzVeLCmptJaw++XSiytTKgKCEPkfGCHwgju2eFqwk+2Jb1olYAYov0P/B/Yj5El9d/7XMPic3zUa7NIUkQhcMqxlHC0/foCECQBWwx3fgk7IAAAAbg3gtrOnZAAAAAAAAAA6cU8AAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS895lCgOFAu+57xt/a86Z5HtOCHE24Yt9JRckZqOvhEAAAAAAAAAAA... (22272 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"f5cdff819ef77947848d1f4d92f4bf1f6b433080d92a8788ac43d490668b4c8b","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"fed730f89cdf351aecd21491085c32ac651c2d3f7e8084090056c31ddf824ec8","ext":"v0","fee_pool":"3830095","id_pool":"0","inflation_seq":0,"ledger_seq":110,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"2ad352429cc2fd10ea4d6d9e8fab1fb94ad3b9e7821aa1b457e248723c493d06","scp_value":{"close_time":"1776729600","ext":{"signed_ms":{"close_time_ms":"1776729600000","lc_value_sign... (48190 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 16704,
  "op": "decode_encode",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAACxVHDCOYWwVJOP2Wq9duvqvyo1VBZLoRJ/dZinAR2/OwAAAB69g1aCtwIe4kxn9qEAsZKO57WQ9whvGqHHS7cOfukn8ckRV4+qmYIBXy6M7D47DRC7ON7rSZI2vQEq+Hih+yaoAAAAAGnmvgAAAAAAAAAAAwAAAZ2tVjAAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQPpVtl9XBM0MZrh0j4F7/3sAYVdPlLDahihMJXiK0VSj8CccpeTG6iG+UUFxV8tatUJF8PKMpWUnfFy0HG7CKQ77AVYvCgbCABIhMBxqaAswuRDpgnuVreIDF9AywdLcIEz/OV9T1XiN+FARoYWXNutZ3Gkye6f5qoIoV4H+Dh8CAAAAbg3gtrOnZAAAAAAAAAA6cTYAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS7H5VTtpbHjkI+EWlCcjyF38hijS2tHyp2D9DOaDPk3QAAAAAAAAAA... (22272 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAACxVHDCOYWwVJOP2Wq9duvqvyo1VBZLoRJ/dZinAR2/OwAAAB69g1aCtwIe4kxn9qEAsZKO57WQ9whvGqHHS7cOfukn8ckRV4+qmYIBXy6M7D47DRC7ON7rSZI2vQEq+Hih+yaoAAAAAGnmvgAAAAAAAAAAAwAAAZ2tVjAAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQPpVtl9XBM0MZrh0j4F7/3sAYVdPlLDahihMJXiK0VSj8CccpeTG6iG+UUFxV8tatUJF8PKMpWUnfFy0HG7CKQ77AVYvCgbCABIhMBxqaAswuRDpgnuVreIDF9AywdLcIEz/OV9T1XiN+FARoYWXNutZ3Gkye6f5qoIoV4H+Dh8CAAAAbg3gtrOnZAAAAAAAAAA6cTYAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS7H5VTtpbHjkI+EWlCcjyF38hijS2tHyp2D9DOaDPk3QAAAAAAAAAA... (22272 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 16704,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAACxVHDCOYWwVJOP2Wq9duvqvyo1VBZLoRJ/dZinAR2/OwAAAB69g1aCtwIe4kxn9qEAsZKO57WQ9whvGqHHS7cOfukn8ckRV4+qmYIBXy6M7D47DRC7ON7rSZI2vQEq+Hih+yaoAAAAAGnmvgAAAAAAAAAAAwAAAZ2tVjAAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQPpVtl9XBM0MZrh0j4F7/3sAYVdPlLDahihMJXiK0VSj8CccpeTG6iG+UUFxV8tatUJF8PKMpWUnfFy0HG7CKQ77AVYvCgbCABIhMBxqaAswuRDpgnuVreIDF9AywdLcIEz/OV9T1XiN+FARoYWXNutZ3Gkye6f5qoIoV4H+Dh8CAAAAbg3gtrOnZAAAAAAAAAA6cTYAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS7H5VTtpbHjkI+EWlCcjyF38hijS2tHyp2D9DOaDPk3QAAAAAAAAAA... (22272 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"b15470c23985b054938fd96abd76ebeabf2a3554164ba1127f7598a7011dbf3b","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"4cff395f53d5788df85011a1859736eb59dc69327ba7f9aa82285781fe0e1f02","ext":"v0","fee_pool":"3830070","id_pool":"0","inflation_seq":0,"ledger_seq":110,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"bd835682b7021ee24c67f6a100b1928ee7b590f7086f1aa1c74bb70e7ee927f1","scp_value":{"close_time":"1776729600","ext":{"signed_ms":{"close_time_ms":"1776729600000","lc_value_sign... (48190 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

## Reproduce

```sh
shapekit conformance run --sdk js --version 16.4.1 --entry soroban.auth-trees.p30-declared-depth-245
```

## Expected value and its source

- corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.xdr: decode = must-accept, roundtrip = true
- corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.xdr: decode = must-accept, roundtrip = true

Entry notes: an address entry whose matched root declares a chain 245 invocations deep below it that no call reaches, the deepest the host still decodes; the transaction succeeds and the declared nodes are never checked

Related: https://github.com/stellar/wallet-backend/issues/746

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.auth-trees.p30-declared-depth-245`
