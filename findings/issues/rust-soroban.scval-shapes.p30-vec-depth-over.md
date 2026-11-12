<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 30.0.0: the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 6831 (soroban.scval-shapes.p30-vec-depth-over)

## What happens

`stellar-xdr` 30.0.0 answers 2 of the checks of corpus entry `soroban.scval-shapes.p30-vec-depth-over` differently from the Stellar network.

## Minimal vector

Entry `soroban.scval-shapes.p30-vec-depth-over` (kind Xdr, family `soroban.scval-shapes`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 6831

```json
{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"5d65d266e381108cccd35068a6e176a33f1cf5b9df0e1abde7cd071827df3ec2","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"182430a0c67a9e1384bb298238d80c0aa18b578a6b421d296b1228e9d2117434","ext":"v0","fee_pool":"463057","id_pool":"0","inflation_seq":0,"ledger_seq":72,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"a8b450004ef9fcefd3154a0769509d3508fe0ae44791d07606a89138f0b252b2","scp_value":{"close_time":"1773446400","ext":{"signed_ms":{"close_time_ms":"1773446400000","lc_value_signature":{"node_id":"GB4WH2VTILAXT2GUDH4KI2ZA2GGYSLJJUYBH6LP42LXI4WNBRIHHDDVT","signature":"a94f042a744c1818467983b3e9f09085cea4dd60c3c12e84a38204e767da0980aa6164277013a4adf774b8a78cb46a631aefbaaefb0734ea9a1eeee1d7c07e0d"}}},"tx_set_hash":"4e15eefe3e7229ed156f7e912093b09bc5d71e6bd1d8885ca9fe4ef1d920fadc","upgrades":[]},"skip_list":["cae38e901e92751bb0fec40c46084c82fc79f31a7bc44b6cbba7071359518d23","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000"],"total_coins":"1000000000000000000","tx_set_result_hash":"22aafa3052209da7ea2caed2002cc1047edb3596c057f20319354c32dfe142e3"}},"scp_info":[],"total_byte_size_of_live_soroban_state":"52498","tx_processing":[{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999836019","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":71,"seq_time":"1773360000"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776131","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":71}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6IC... (7822 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAABdZdJm44EQjMzTUGim4XajPxz1ud8OGr3nzQcYJ98+wgAAAB6otFAATvn879MVSgdpUJ01CP4K5EeR0HYGqJE48LJSsk4V7v4+cintFW9+kSCTsJvF1x5r0diIXKn+TvHZIPrcAAAAAGm0pQAAAAAAAAAAAwAAAZzppIgAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQKlPBCp0TBgYRnmDs+nwkIXOpN1gw8EuhKOCBOdn2gmAqmFkJ3ATpK33dLinjLRqYxrvuq77BzTqmh7u4dfAfg0iqvowUiCdp+osrtIALMEEfts1lsBX8gMZNUwy3+FC4xgkMKDGep4ThLspgjjYDAqhi1eKa0IdKWsSKOnSEXQ0AAAASA3gtrOnZAAAAAAAAAAHENEAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTK446QHpJ1G7D+xAxGCEyC/HnzGnvES2y7pwcTWVGNIwAAAAAAAAAA... (4572 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 6831"}`

### `mainnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 6831

```json
{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"07ab48edc0706dad7e269e7faef44cbf205e9954a98ab47487469307c9ed098d","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"704f9bc2a3bd648254e11030396a282d3d93c706023efa719226ecad9532e262","ext":"v0","fee_pool":"463057","id_pool":"0","inflation_seq":0,"ledger_seq":72,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"733b7c618ba8e2cddcf8e3a404ba32374dbc28483fb7fd744d2f414b6e43e0b8","scp_value":{"close_time":"1773446400","ext":{"signed_ms":{"close_time_ms":"1773446400000","lc_value_signature":{"node_id":"GDZ3C5SA7QH74NJ7YML2AZC47IR25BR7MUVZNUWE77OJL5OQDDMPCIMS","signature":"20f4a870a69611c5dba529ed464d379489808ef53e249a41f5bfd3f921c9318fb084dba153119ea2ee9c011ae5df8d9d75f9a4c2ae9c12505d8dfbf094ef6303"}}},"tx_set_hash":"c4d82078a15708a89588a4bf29f89ecebaa668c5e9b24b9beb896b17ed5a9b9a","upgrades":[]},"skip_list":["cc1293025d3ed2c1585ffc20eadb7d1ec553fc085377739c4e112abadfccad9f","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000"],"total_coins":"1000000000000000000","tx_set_result_hash":"4cd6589c40fad99183773e88aa5f1d7c93b361489c9ec8cb54404c319d51a936"}},"scp_info":[],"total_byte_size_of_live_soroban_state":"52498","tx_processing":[{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999836019","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":71,"seq_time":"1773360000"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776131","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":71}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6IC... (7822 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAAAHq0jtwHBtrX4mnn+u9Ey/IF6ZVKmKtHSHRpMHye0JjQAAAB5zO3xhi6jizdz446QEujI3TbwoSD+3/XRNL0FLbkPguMTYIHihVwiolYikvyn4ns66pmjF6bJLm+uJaxftWpuaAAAAAGm0pQAAAAAAAAAAAwAAAZzppIgAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQCD0qHCmlhHF26Up7UZNN5SJgI71PiSaQfW/0/khyTGPsITboVMRnqLunAEa5d+NnXX5pMKunBJQXY378JTvYwNM1licQPrZkYN3PoiqXx18k7NhSJyeyMtUQEwxnVGpNnBPm8KjvWSCVOEQMDlqKC09k8cGAj76cZIm7K2VMuJiAAAASA3gtrOnZAAAAAAAAAAHENEAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTMEpMCXT7SwVhf/CDq230exVP8CFN3c5xOESq638ytnwAAAAAAAAAA... (4572 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 6831"}`

## Reproduce

```sh
shapekit conformance run --sdk rust --version 30.0.0 --entry soroban.scval-shapes.p30-vec-depth-over
```

## Expected value and its source

- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-over/mainnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-over/mainnet.xdr
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-over/testnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-over/testnet.xdr

Entry notes: 100 nested vecs, one more than the host accepts (simulation: HostError: Error(Context, ExceededLimit)): core includes the invocation and fails it with invoke_host_function Trapped, so the value exists only in the envelope

Related: https://github.com/stellar/js-stellar-sdk/pull/1766, https://github.com/stellar/js-stellar-sdk/issues/1789

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.scval-shapes.p30-vec-depth-over`
