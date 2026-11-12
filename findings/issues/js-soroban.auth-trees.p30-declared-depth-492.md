<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.4.1: the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth (soroban.auth-trees.p30-declared-depth-492)

## What happens

`@stellar/stellar-sdk` 16.4.1 answers 4 of the checks of corpus entry `soroban.auth-trees.p30-declared-depth-492` differently from the Stellar network.

## Minimal vector

Entry `soroban.auth-trees.p30-declared-depth-492` (kind Xdr, family `soroban.auth-trees`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 30344,
  "op": "decode_encode",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAAC0fpqfe04/KGU38zHLUo3+mZsCD1rB2HrS/bm54RF3BwAAAB5bWyQlSv3TpfbAyrjY1+qcMvBg/dhfcz1uTtT1wOsYnQKplVxCPkghvLDgAiDvZfcxIo7PozsAbWm11YRgODWnAAAAAGnpYQAAAAAAAAAAAwAAAZ23ougAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQMkkk+g/0jL+NAP0Yby/OgFZlGVusYgq195HZt+8SoChYD+Df+bgNR4IM4go8Xzj41z8dIK6ps+/Bifov7wthQdaRMtb+gZSW5J5gpUT+gqmpnyqfsTQATxaa0+7rk/r9eCbArPnNopP7z3lJ6D3GJelUE6TFdX+PDxznQB9VYscAAAAcA3gtrOnZAAAAAAAAAA8Au4AAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS895lCgOFAu+57xt/a86Z5HtOCHE24Yt9JRckZqOvhEAAAAAAAAAAA... (40460 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAAC0fpqfe04/KGU38zHLUo3+mZsCD1rB2HrS/bm54RF3BwAAAB5bWyQlSv3TpfbAyrjY1+qcMvBg/dhfcz1uTtT1wOsYnQKplVxCPkghvLDgAiDvZfcxIo7PozsAbWm11YRgODWnAAAAAGnpYQAAAAAAAAAAAwAAAZ23ougAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQMkkk+g/0jL+NAP0Yby/OgFZlGVusYgq195HZt+8SoChYD+Df+bgNR4IM4go8Xzj41z8dIK6ps+/Bifov7wthQdaRMtb+gZSW5J5gpUT+gqmpnyqfsTQATxaa0+7rk/r9eCbArPnNopP7z3lJ6D3GJelUE6TFdX+PDxznQB9VYscAAAAcA3gtrOnZAAAAAAAAAA8Au4AAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS895lCgOFAu+57xt/a86Z5HtOCHE24Yt9JRckZqOvhEAAAAAAAAAAA... (40460 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `testnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 30344,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAAC0fpqfe04/KGU38zHLUo3+mZsCD1rB2HrS/bm54RF3BwAAAB5bWyQlSv3TpfbAyrjY1+qcMvBg/dhfcz1uTtT1wOsYnQKplVxCPkghvLDgAiDvZfcxIo7PozsAbWm11YRgODWnAAAAAGnpYQAAAAAAAAAAAwAAAZ23ougAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQMkkk+g/0jL+NAP0Yby/OgFZlGVusYgq195HZt+8SoChYD+Df+bgNR4IM4go8Xzj41z8dIK6ps+/Bifov7wthQdaRMtb+gZSW5J5gpUT+gqmpnyqfsTQATxaa0+7rk/r9eCbArPnNopP7z3lJ6D3GJelUE6TFdX+PDxznQB9VYscAAAAcA3gtrOnZAAAAAAAAAA8Au4AAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS895lCgOFAu+57xt/a86Z5HtOCHE24Yt9JRckZqOvhEAAAAAAAAAAA... (40460 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"b47e9a9f7b4e3f286537f331cb528dfe999b020f5ac1d87ad2fdb9b9e1117707","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"e09b02b3e7368a4fef3de527a0f71897a5504e9315d5fe3c3c739d007d558b1c","ext":"v0","fee_pool":"3932910","id_pool":"0","inflation_seq":0,"ledger_seq":112,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"5b5b24254afdd3a5f6c0cab8d8d7ea9c32f060fdd85f733d6e4ed4f5c0eb189d","scp_value":{"close_time":"1776902400","ext":{"signed_ms":{"close_time_ms":"1776902400000","lc_value_sign... (87628 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 30344,
  "op": "decode_encode",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAABUB7SBb8+NRK5HXS/+31di3hXLXt/XS0X1jRIm/ly8hQAAAB6cv0a04m6JR1vLeDUrs+VCTtuTZ+9pFRXRxkCQM0kg1sIjtJzg8SVg+Hmum7gQG5/Zrl1xwIHKV1ia4v9fPLLHAAAAAGnpYQAAAAAAAAAAAwAAAZ23ougAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQPg9et6g+W3mdb6GVggUfil87TuDyV1Ro16s9jFcgQicW47ehAkIO3YyKppA48Wz0xyRwZzrXp9zR2XPaxMmGwYiQPd8U512bf1ipEy3MHo7yJ9aLld9N9kdwa4nW10TyxDJyocmdq/Kjk2JIMXHoF1QsZkWtgYqlzVasBWFVHJZAAAAcA3gtrOnZAAAAAAAAAA8AtUAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS7H5VTtpbHjkI+EWlCcjyF38hijS2tHyp2D9DOaDPk3QAAAAAAAAAA... (40460 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAABUB7SBb8+NRK5HXS/+31di3hXLXt/XS0X1jRIm/ly8hQAAAB6cv0a04m6JR1vLeDUrs+VCTtuTZ+9pFRXRxkCQM0kg1sIjtJzg8SVg+Hmum7gQG5/Zrl1xwIHKV1ia4v9fPLLHAAAAAGnpYQAAAAAAAAAAAwAAAZ23ougAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQPg9et6g+W3mdb6GVggUfil87TuDyV1Ro16s9jFcgQicW47ehAkIO3YyKppA48Wz0xyRwZzrXp9zR2XPaxMmGwYiQPd8U512bf1ipEy3MHo7yJ9aLld9N9kdwa4nW10TyxDJyocmdq/Kjk2JIMXHoF1QsZkWtgYqlzVasBWFVHJZAAAAcA3gtrOnZAAAAAAAAAA8AtUAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS7H5VTtpbHjkI+EWlCcjyF38hijS2tHyp2D9DOaDPk3QAAAAAAAAAA... (40460 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 30344,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAABUB7SBb8+NRK5HXS/+31di3hXLXt/XS0X1jRIm/ly8hQAAAB6cv0a04m6JR1vLeDUrs+VCTtuTZ+9pFRXRxkCQM0kg1sIjtJzg8SVg+Hmum7gQG5/Zrl1xwIHKV1ia4v9fPLLHAAAAAGnpYQAAAAAAAAAAAwAAAZ23ougAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQPg9et6g+W3mdb6GVggUfil87TuDyV1Ro16s9jFcgQicW47ehAkIO3YyKppA48Wz0xyRwZzrXp9zR2XPaxMmGwYiQPd8U512bf1ipEy3MHo7yJ9aLld9N9kdwa4nW10TyxDJyocmdq/Kjk2JIMXHoF1QsZkWtgYqlzVasBWFVHJZAAAAcA3gtrOnZAAAAAAAAAA8AtUAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS7H5VTtpbHjkI+EWlCcjyF38hijS2tHyp2D9DOaDPk3QAAAAAAAAAA... (40460 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"5407b4816fcf8d44ae475d2ffedf5762de15cb5edfd74b45f58d1226fe5cbc85","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"10c9ca872676afca8e4d8920c5c7a05d50b19916b6062a97355ab01585547259","ext":"v0","fee_pool":"3932885","id_pool":"0","inflation_seq":0,"ledger_seq":112,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"9cbf46b4e26e89475bcb78352bb3e5424edb9367ef691515d1c64090334920d6","scp_value":{"close_time":"1776902400","ext":{"signed_ms":{"close_time_ms":"1776902400000","lc_value_sign... (87628 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

## Reproduce

```sh
shapekit conformance run --sdk js --version 16.4.1 --entry soroban.auth-trees.p30-declared-depth-492
```

## Expected value and its source

- corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-492/mainnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-492/mainnet.xdr: decode = must-accept, roundtrip = true
- corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-492/testnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-492/testnet.xdr: decode = must-accept, roundtrip = true

Entry notes: an address entry declaring a chain 492 invocations deep below its root, 493 with the root, the deepest envelope core's depth check accepts (xdr.envelope.depth 1000): core includes it and the host, which decodes at depth 500, traps. the ledger needs xdr.meta.depth (1012) to decode

Related: https://github.com/stellar/wallet-backend/issues/746

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.auth-trees.p30-declared-depth-492`
