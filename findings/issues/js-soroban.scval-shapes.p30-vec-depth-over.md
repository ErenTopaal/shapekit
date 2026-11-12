<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.4.1: the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth (soroban.scval-shapes.p30-vec-depth-over)

## What happens

`@stellar/stellar-sdk` 16.4.1 answers 4 of the checks of corpus entry `soroban.scval-shapes.p30-vec-depth-over` differently from the Stellar network.

## Minimal vector

Entry `soroban.scval-shapes.p30-vec-depth-over` (kind Xdr, family `soroban.scval-shapes`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 3428,
  "op": "decode_encode",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAABdZdJm44EQjMzTUGim4XajPxz1ud8OGr3nzQcYJ98+wgAAAB6otFAATvn879MVSgdpUJ01CP4K5EeR0HYGqJE48LJSsk4V7v4+cintFW9+kSCTsJvF1x5r0diIXKn+TvHZIPrcAAAAAGm0pQAAAAAAAAAAAwAAAZzppIgAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQKlPBCp0TBgYRnmDs+nwkIXOpN1gw8EuhKOCBOdn2gmAqmFkJ3ATpK33dLinjLRqYxrvuq77BzTqmh7u4dfAfg0iqvowUiCdp+osrtIALMEEfts1lsBX8gMZNUwy3+FC4xgkMKDGep4ThLspgjjYDAqhi1eKa0IdKWsSKOnSEXQ0AAAASA3gtrOnZAAAAAAAAAAHENEAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTK446QHpJ1G7D+xAxGCEyC/HnzGnvES2y7pwcTWVGNIwAAAAAAAAAA... (4572 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAABdZdJm44EQjMzTUGim4XajPxz1ud8OGr3nzQcYJ98+wgAAAB6otFAATvn879MVSgdpUJ01CP4K5EeR0HYGqJE48LJSsk4V7v4+cintFW9+kSCTsJvF1x5r0diIXKn+TvHZIPrcAAAAAGm0pQAAAAAAAAAAAwAAAZzppIgAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQKlPBCp0TBgYRnmDs+nwkIXOpN1gw8EuhKOCBOdn2gmAqmFkJ3ATpK33dLinjLRqYxrvuq77BzTqmh7u4dfAfg0iqvowUiCdp+osrtIALMEEfts1lsBX8gMZNUwy3+FC4xgkMKDGep4ThLspgjjYDAqhi1eKa0IdKWsSKOnSEXQ0AAAASA3gtrOnZAAAAAAAAAAHENEAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTK446QHpJ1G7D+xAxGCEyC/HnzGnvES2y7pwcTWVGNIwAAAAAAAAAA... (4572 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `testnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 3428,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAABdZdJm44EQjMzTUGim4XajPxz1ud8OGr3nzQcYJ98+wgAAAB6otFAATvn879MVSgdpUJ01CP4K5EeR0HYGqJE48LJSsk4V7v4+cintFW9+kSCTsJvF1x5r0diIXKn+TvHZIPrcAAAAAGm0pQAAAAAAAAAAAwAAAZzppIgAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQKlPBCp0TBgYRnmDs+nwkIXOpN1gw8EuhKOCBOdn2gmAqmFkJ3ATpK33dLinjLRqYxrvuq77BzTqmh7u4dfAfg0iqvowUiCdp+osrtIALMEEfts1lsBX8gMZNUwy3+FC4xgkMKDGep4ThLspgjjYDAqhi1eKa0IdKWsSKOnSEXQ0AAAASA3gtrOnZAAAAAAAAAAHENEAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTK446QHpJ1G7D+xAxGCEyC/HnzGnvES2y7pwcTWVGNIwAAAAAAAAAA... (4572 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"5d65d266e381108cccd35068a6e176a33f1cf5b9df0e1abde7cd071827df3ec2","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"182430a0c67a9e1384bb298238d80c0aa18b578a6b421d296b1228e9d2117434","ext":"v0","fee_pool":"463057","id_pool":"0","inflation_seq":0,"ledger_seq":72,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"a8b450004ef9fcefd3154a0769509d3508fe0ae44791d07606a89138f0b252b2","scp_value":{"close_time":"1773446400","ext":{"signed_ms":{"close_time_ms":"1773446400000","lc_value_signat... (7778 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 3428,
  "op": "decode_encode",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAAAHq0jtwHBtrX4mnn+u9Ey/IF6ZVKmKtHSHRpMHye0JjQAAAB5zO3xhi6jizdz446QEujI3TbwoSD+3/XRNL0FLbkPguMTYIHihVwiolYikvyn4ns66pmjF6bJLm+uJaxftWpuaAAAAAGm0pQAAAAAAAAAAAwAAAZzppIgAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQCD0qHCmlhHF26Up7UZNN5SJgI71PiSaQfW/0/khyTGPsITboVMRnqLunAEa5d+NnXX5pMKunBJQXY378JTvYwNM1licQPrZkYN3PoiqXx18k7NhSJyeyMtUQEwxnVGpNnBPm8KjvWSCVOEQMDlqKC09k8cGAj76cZIm7K2VMuJiAAAASA3gtrOnZAAAAAAAAAAHENEAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTMEpMCXT7SwVhf/CDq230exVP8CFN3c5xOESq638ytnwAAAAAAAAAA... (4572 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAAAHq0jtwHBtrX4mnn+u9Ey/IF6ZVKmKtHSHRpMHye0JjQAAAB5zO3xhi6jizdz446QEujI3TbwoSD+3/XRNL0FLbkPguMTYIHihVwiolYikvyn4ns66pmjF6bJLm+uJaxftWpuaAAAAAGm0pQAAAAAAAAAAAwAAAZzppIgAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQCD0qHCmlhHF26Up7UZNN5SJgI71PiSaQfW/0/khyTGPsITboVMRnqLunAEa5d+NnXX5pMKunBJQXY378JTvYwNM1licQPrZkYN3PoiqXx18k7NhSJyeyMtUQEwxnVGpNnBPm8KjvWSCVOEQMDlqKC09k8cGAj76cZIm7K2VMuJiAAAASA3gtrOnZAAAAAAAAAAHENEAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTMEpMCXT7SwVhf/CDq230exVP8CFN3c5xOESq638ytnwAAAAAAAAAA... (4572 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 3428,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAAAHq0jtwHBtrX4mnn+u9Ey/IF6ZVKmKtHSHRpMHye0JjQAAAB5zO3xhi6jizdz446QEujI3TbwoSD+3/XRNL0FLbkPguMTYIHihVwiolYikvyn4ns66pmjF6bJLm+uJaxftWpuaAAAAAGm0pQAAAAAAAAAAAwAAAZzppIgAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQCD0qHCmlhHF26Up7UZNN5SJgI71PiSaQfW/0/khyTGPsITboVMRnqLunAEa5d+NnXX5pMKunBJQXY378JTvYwNM1licQPrZkYN3PoiqXx18k7NhSJyeyMtUQEwxnVGpNnBPm8KjvWSCVOEQMDlqKC09k8cGAj76cZIm7K2VMuJiAAAASA3gtrOnZAAAAAAAAAAHENEAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTMEpMCXT7SwVhf/CDq230exVP8CFN3c5xOESq638ytnwAAAAAAAAAA... (4572 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"07ab48edc0706dad7e269e7faef44cbf205e9954a98ab47487469307c9ed098d","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"704f9bc2a3bd648254e11030396a282d3d93c706023efa719226ecad9532e262","ext":"v0","fee_pool":"463057","id_pool":"0","inflation_seq":0,"ledger_seq":72,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"733b7c618ba8e2cddcf8e3a404ba32374dbc28483fb7fd744d2f414b6e43e0b8","scp_value":{"close_time":"1773446400","ext":{"signed_ms":{"close_time_ms":"1773446400000","lc_value_signat... (7778 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

## Reproduce

```sh
shapekit conformance run --sdk js --version 16.4.1 --entry soroban.scval-shapes.p30-vec-depth-over
```

## Expected value and its source

- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-over/mainnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-over/mainnet.xdr: decode = must-accept, roundtrip = true
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-over/testnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-over/testnet.xdr: decode = must-accept, roundtrip = true

Entry notes: 100 nested vecs, one more than the host accepts (simulation: HostError: Error(Context, ExceededLimit)): core includes the invocation and fails it with invoke_host_function Trapped, so the value exists only in the envelope

Related: https://github.com/stellar/js-stellar-sdk/pull/1766, https://github.com/stellar/js-stellar-sdk/issues/1789

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.scval-shapes.p30-vec-depth-over`
