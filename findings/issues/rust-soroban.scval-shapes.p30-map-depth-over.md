<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 30.0.0: the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 7674 (soroban.scval-shapes.p30-map-depth-over)

## What happens

`stellar-xdr` 30.0.0 answers 2 of the checks of corpus entry `soroban.scval-shapes.p30-map-depth-over` differently from the Stellar network.

## Minimal vector

Entry `soroban.scval-shapes.p30-map-depth-over` (kind Xdr, family `soroban.scval-shapes`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 7674

```json
{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"14344be17fc86063d85e0b460965829c4bc235600a901841a46d81c40fd6f262","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"65b121dfcbbba14d51187e107413fbda302e861756ddcedfd7505efeec849784","ext":"v0","fee_pool":"610502","id_pool":"0","inflation_seq":0,"ledger_seq":74,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"52a826f228f0a24af55717ab7705622743bb25fc91ea8af6301008a4d58690b9","scp_value":{"close_time":"1773619200","ext":{"signed_ms":{"close_time_ms":"1773619200000","lc_value_signature":{"node_id":"GB4WH2VTILAXT2GUDH4KI2ZA2GGYSLJJUYBH6LP42LXI4WNBRIHHDDVT","signature":"76948af049dcba129e24fc41f3d6ebad96ddaef396dbcef32252edefc3e1594ee644794a151de4f3c55a7ee9a160b1b4a0b9cff110f13713527419e55078e60a"}}},"tx_set_hash":"909f5c793e4a6bafc680d64829d5999dc8e2060d06c377815d69da5491c5d557","upgrades":[]},"skip_list":["cae38e901e92751bb0fec40c46084c82fc79f31a7bc44b6cbba7071359518d23","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000"],"total_coins":"1000000000000000000","tx_set_result_hash":"2fa5c22224596196df9edaf309f39d954b5dcaa3382987a03efde0d1bfe9c82f"}},"scp_info":[],"total_byte_size_of_live_soroban_state":"52498","tx_processing":[{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999693689","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":73,"seq_time":"1773532800"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776133","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":73}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6IC... (10688 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAAAUNEvhf8hgY9heC0YJZYKcS8I1YAqQGEGkbYHED9byYgAAAB5SqCbyKPCiSvVXF6t3BWInQ7sl/JHqivYwEAik1YaQuZCfXHk+SmuvxoDWSCnVmZ3I4gYNBsN3gV1p2lSRxdVXAAAAAGm3SAAAAAAAAAAAAwAAAZzz8UAAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQHaUivBJ3LoSniT8QfPW662W3a7zltvO8yJS7e/D4VlO5kR5ShUd5PPFWn7poWCxtKC5z/EQ8TcTUnQZ5VB45govpcIiJFlhlt+e2vMJ852VS13Kozgph6A+/eDRv+nIL2WxId/Lu6FNURh+EHQT+9owLoYXVt3O39dQXv7shJeEAAAASg3gtrOnZAAAAAAAAAAJUMYAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTK446QHpJ1G7D+xAxGCEyC/HnzGnvES2y7pwcTWVGNIwAAAAAAAAAA... (6140 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 7674"}`

### `mainnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 7674

```json
{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"f239cc1b4248c9f1832b1105247ae822178afa93c5d671a39472de3d88fae0a5","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"43390d03e3b55c5596aad7d8b9c6050903fba8c89af7d30149239235f52c8858","ext":"v0","fee_pool":"610502","id_pool":"0","inflation_seq":0,"ledger_seq":74,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"990fc3937d865418665282705f981d239989fe694e98b95f205c2611e6d1fef9","scp_value":{"close_time":"1773619200","ext":{"signed_ms":{"close_time_ms":"1773619200000","lc_value_signature":{"node_id":"GDZ3C5SA7QH74NJ7YML2AZC47IR25BR7MUVZNUWE77OJL5OQDDMPCIMS","signature":"95480fb1e65dde865d39e2c15c1a8a9a0b25c2ec8f8e64b86fcd8a7ece4647d8cf6c1068194a15572ec1560461f6c7950e8f5e290474fc21687d88d5fd987d02"}}},"tx_set_hash":"443b4a976adc7a73224531fa6cd8df46e5039ef849b1c6b896a470d7172f3949","upgrades":[]},"skip_list":["cc1293025d3ed2c1585ffc20eadb7d1ec553fc085377739c4e112abadfccad9f","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000"],"total_coins":"1000000000000000000","tx_set_result_hash":"b4bae2beb6d213dbc6799864bced1486725aeba88109640b60b3f3447c956767"}},"scp_info":[],"total_byte_size_of_live_soroban_state":"52498","tx_processing":[{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999693689","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":73,"seq_time":"1773532800"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776133","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":73}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6IC... (10688 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAADyOcwbQkjJ8YMrEQUkeugiF4r6k8XWcaOUct49iPrgpQAAAB6ZD8OTfYZUGGZSgnBfmB0jmYn+aU6YuV8gXCYR5tH++UQ7Spdq3HpzIkUx+mzY30blA574SbHGuJakcNcXLzlJAAAAAGm3SAAAAAAAAAAAAwAAAZzz8UAAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQJVID7HmXd6GXTniwVwaipoLJcLsj45kuG/Nin7ORkfYz2wQaBlKFVcuwVYEYfbHlQ6PXikEdPwhaH2I1f2YfQK0uuK+ttIT28Z5mGS87RSGclrrqIEJZAtgs/NEfJVnZ0M5DQPjtVxVlqrX2LnGBQkD+6jImvfTAUkjkjX1LIhYAAAASg3gtrOnZAAAAAAAAAAJUMYAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTMEpMCXT7SwVhf/CDq230exVP8CFN3c5xOESq638ytnwAAAAAAAAAA... (6140 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 7674"}`

## Reproduce

```sh
shapekit conformance run --sdk rust --version 30.0.0 --entry soroban.scval-shapes.p30-map-depth-over
```

## Expected value and its source

- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-over/mainnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-over/mainnet.xdr
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-over/testnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-over/testnet.xdr

Entry notes: 99 nested maps, one more than the host accepts (simulation: HostError: Error(Value, InvalidInput)): core includes the invocation and fails it with invoke_host_function Trapped, so the value exists only in the envelope

Related: https://github.com/stellar/js-stellar-sdk/pull/1766, https://github.com/stellar/js-stellar-sdk/issues/1789

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.scval-shapes.p30-map-depth-over`
