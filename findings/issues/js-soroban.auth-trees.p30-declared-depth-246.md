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
  "xdr": "AAAAAgAAAABbWyQlSv3TpfbAyrjY1+qcMvBg/dhfcz1uTtT1wOsYnQAAAB71zf+Bnvd5R4SNH02S9L8fa0MwgNkqh4isQ9SQZotMi+BnwV4ez6SfvXYEvnJVvOvAZ6aAHAFVm4jsP2AhRzCCAAAAAGnoD4AAAAAAAAAAAwAAAZ2yfIwAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQKLhbxasseztWuTbIV/9tQsxqs+oiigV16TlD3/eb0cxrjoT0U5OfQ0yhLdZzXJdvg8TOZLpX9zkOJTVYuMYZAPM9B+OQCD+fiQ6jm5d9aoHzFg5odfdtkR1+qGBDADT/KW7wNEuPMAQQNtApYV87ARAUhaEqWpWjvEUJN0BL8tXAAAAbw3gtrOnZAAAAAAAAAA6/3kAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS895lCgOFAu+57xt/a86Z5HtOCHE24Yt9JRckZqOvhEAAAAAAAAAAA... (22092 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAABbWyQlSv3TpfbAyrjY1+qcMvBg/dhfcz1uTtT1wOsYnQAAAB71zf+Bnvd5R4SNH02S9L8fa0MwgNkqh4isQ9SQZotMi+BnwV4ez6SfvXYEvnJVvOvAZ6aAHAFVm4jsP2AhRzCCAAAAAGnoD4AAAAAAAAAAAwAAAZ2yfIwAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQKLhbxasseztWuTbIV/9tQsxqs+oiigV16TlD3/eb0cxrjoT0U5OfQ0yhLdZzXJdvg8TOZLpX9zkOJTVYuMYZAPM9B+OQCD+fiQ6jm5d9aoHzFg5odfdtkR1+qGBDADT/KW7wNEuPMAQQNtApYV87ARAUhaEqWpWjvEUJN0BL8tXAAAAbw3gtrOnZAAAAAAAAAA6/3kAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS895lCgOFAu+57xt/a86Z5HtOCHE24Yt9JRckZqOvhEAAAAAAAAAAA... (22092 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `testnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 16568,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAABbWyQlSv3TpfbAyrjY1+qcMvBg/dhfcz1uTtT1wOsYnQAAAB71zf+Bnvd5R4SNH02S9L8fa0MwgNkqh4isQ9SQZotMi+BnwV4ez6SfvXYEvnJVvOvAZ6aAHAFVm4jsP2AhRzCCAAAAAGnoD4AAAAAAAAAAAwAAAZ2yfIwAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQKLhbxasseztWuTbIV/9tQsxqs+oiigV16TlD3/eb0cxrjoT0U5OfQ0yhLdZzXJdvg8TOZLpX9zkOJTVYuMYZAPM9B+OQCD+fiQ6jm5d9aoHzFg5odfdtkR1+qGBDADT/KW7wNEuPMAQQNtApYV87ARAUhaEqWpWjvEUJN0BL8tXAAAAbw3gtrOnZAAAAAAAAAA6/3kAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS895lCgOFAu+57xt/a86Z5HtOCHE24Yt9JRckZqOvhEAAAAAAAAAAA... (22092 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"5b5b24254afdd3a5f6c0cab8d8d7ea9c32f060fdd85f733d6e4ed4f5c0eb189d","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"a5bbc0d12e3cc01040db40a5857cec0440521684a96a568ef11424dd012fcb57","ext":"v0","fee_pool":"3866489","id_pool":"0","inflation_seq":0,"ledger_seq":111,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"f5cdff819ef77947848d1f4d92f4bf1f6b433080d92a8788ac43d490668b4c8b","scp_value":{"close_time":"1776816000","ext":{"signed_ms":{"close_time_ms":"1776816000000","lc_value_sign... (47776 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 16568,
  "op": "decode_encode",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAACcv0a04m6JR1vLeDUrs+VCTtuTZ+9pFRXRxkCQM0kg1gAAAB6xVHDCOYWwVJOP2Wq9duvqvyo1VBZLoRJ/dZinAR2/O6ef3SqmusEy0R+3mGOhivxQqel8DrrsiE3Q8SbWv5uxAAAAAGnoD4AAAAAAAAAAAwAAAZ2yfIwAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQMCI+wXuXTwpWWUEDzP/F+wUKdnJVMMu0keu5WJ/bH8ia2m9LknfPHxw2DtEV372tCUz6qd7KUGqfDfdbIS0JweJ1MJpBZW+iTCXyQ9vIj5Y4VskLIKyKKLdqISGTZOmy9AD69zl0xolX2SeuyRJj7QsLlb/PYD7RS5qBtznL3XPAAAAbw3gtrOnZAAAAAAAAAA6/2AAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS7H5VTtpbHjkI+EWlCcjyF38hijS2tHyp2D9DOaDPk3QAAAAAAAAAA... (22092 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAACcv0a04m6JR1vLeDUrs+VCTtuTZ+9pFRXRxkCQM0kg1gAAAB6xVHDCOYWwVJOP2Wq9duvqvyo1VBZLoRJ/dZinAR2/O6ef3SqmusEy0R+3mGOhivxQqel8DrrsiE3Q8SbWv5uxAAAAAGnoD4AAAAAAAAAAAwAAAZ2yfIwAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQMCI+wXuXTwpWWUEDzP/F+wUKdnJVMMu0keu5WJ/bH8ia2m9LknfPHxw2DtEV372tCUz6qd7KUGqfDfdbIS0JweJ1MJpBZW+iTCXyQ9vIj5Y4VskLIKyKKLdqISGTZOmy9AD69zl0xolX2SeuyRJj7QsLlb/PYD7RS5qBtznL3XPAAAAbw3gtrOnZAAAAAAAAAA6/2AAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS7H5VTtpbHjkI+EWlCcjyF38hijS2tHyp2D9DOaDPk3QAAAAAAAAAA... (22092 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 16568,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAACcv0a04m6JR1vLeDUrs+VCTtuTZ+9pFRXRxkCQM0kg1gAAAB6xVHDCOYWwVJOP2Wq9duvqvyo1VBZLoRJ/dZinAR2/O6ef3SqmusEy0R+3mGOhivxQqel8DrrsiE3Q8SbWv5uxAAAAAGnoD4AAAAAAAAAAAwAAAZ2yfIwAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQMCI+wXuXTwpWWUEDzP/F+wUKdnJVMMu0keu5WJ/bH8ia2m9LknfPHxw2DtEV372tCUz6qd7KUGqfDfdbIS0JweJ1MJpBZW+iTCXyQ9vIj5Y4VskLIKyKKLdqISGTZOmy9AD69zl0xolX2SeuyRJj7QsLlb/PYD7RS5qBtznL3XPAAAAbw3gtrOnZAAAAAAAAAA6/2AAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS7H5VTtpbHjkI+EWlCcjyF38hijS2tHyp2D9DOaDPk3QAAAAAAAAAA... (22092 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"9cbf46b4e26e89475bcb78352bb3e5424edb9367ef691515d1c64090334920d6","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"d003ebdce5d31a255f649ebb24498fb42c2e56ff3d80fb452e6a06dce72f75cf","ext":"v0","fee_pool":"3866464","id_pool":"0","inflation_seq":0,"ledger_seq":111,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"b15470c23985b054938fd96abd76ebeabf2a3554164ba1127f7598a7011dbf3b","scp_value":{"close_time":"1776816000","ext":{"signed_ms":{"close_time_ms":"1776816000000","lc_value_sign... (47776 chars, see the source below)`

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
