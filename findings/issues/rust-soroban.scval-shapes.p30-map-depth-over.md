<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 30.0.0: the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 7674 (soroban.scval-shapes.p30-map-depth-over)

## What happens

On the input below, `stellar-xdr` 30.0.0 answers differently from the Stellar network in 2 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `soroban.scval-shapes.p30-map-depth-over` (family `soroban.scval-shapes`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `testnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 7674

```json
{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"14ebd82540945380eb3b942af0adbc6653b60065472304be6ff6589826fc7686","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"127c647872bdb028621531800b618fe74aeb7f0492414ccdfba67702329fef0c","ext":"v0","fee_pool":"543717","id_pool":"0","inflation_seq":0,"ledger_seq":74,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"c72afbd964fc7d06722d5ab558717a6d3d550e66abf4f560b91be26461ef3e71","scp_value":{"close_time":"1773619200","ext":{"signed_ms":{"close_time_ms":"1773619200000","lc_value_signature":{"node_id":"GB4WH2VTILAXT2GUDH4KI2ZA2GGYSLJJUYBH6LP42LXI4WNBRIHHDDVT","signature":"86d6f350f9deca19d2b98770358061e98bd01a6d38e6ec0b9280922b610e8e11ce741b62cdf6f32c4573ddffdcc88ec949359cb875b9b385c4f89975dbc17005"}}},"tx_set_hash":"e0df822a9aae7ff4933eb29e10c05de9961a3653bc1e997ba80d4fcd399c2a6b","upgrades":[]},"skip_list":["cae38e901e92751bb0fec40c46084c82fc79f31a7bc44b6cbba7071359518d23","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000"],"total_coins":"1000000000000000000","tx_set_result_hash":"5cf31f89f000f53031fd6b4604df5f22d0718e5ee7a1acc26f5d344d1620c3f4"}},"scp_info":[],"total_byte_size_of_live_soroban_state":"40407","tx_processing":[{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999701733","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":73,"seq_time":"1773532800"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776133","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":73}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6IC... (10688 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAAAU69glQJRTgOs7lCrwrbxmU7YAZUcjBL5v9liYJvx2hgAAAB7HKvvZZPx9BnItWrVYcXptPVUOZqv09WC5G+JkYe8+ceDfgiqarn/0kz6ynhDAXemWGjZTvB6Ze6gNT805nCprAAAAAGm3SAAAAAAAAAAAAwAAAZzz8UAAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQIbW81D53soZ0rmHcDWAYemL0BptOObsC5KAkithDo4RznQbYs328yxFc93/3MiOyUk1nLh1ubOFxPiZddvBcAVc8x+J8AD1MDH9a0YE318i0HGOXuehrMJvXTRNFiDD9BJ8ZHhyvbAoYhUxgAthj+dK638EkkFMzfumdwIyn+8MAAAASg3gtrOnZAAAAAAAAAAIS+UAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTK446QHpJ1G7D+xAxGCEyC/HnzGnvES2y7pwcTWVGNIwAAAAAAAAAA... (6140 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 7674"}`

### `mainnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 7674

```json
{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"0ebe3c7663be6a641fa6be6a298db4325d57b720e8d599462d54a2b36632e723","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"acbd02eb563795271442950ce800219dbd46720e6d8994edfd7064fae8384932","ext":"v0","fee_pool":"543717","id_pool":"0","inflation_seq":0,"ledger_seq":74,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"c768c73ef48e04f433f07318f0c165ffe1b651d5b78ca14086a4b09ef85dd129","scp_value":{"close_time":"1773619200","ext":{"signed_ms":{"close_time_ms":"1773619200000","lc_value_signature":{"node_id":"GDZ3C5SA7QH74NJ7YML2AZC47IR25BR7MUVZNUWE77OJL5OQDDMPCIMS","signature":"9386c388c903d32a15250e992506c9445eb9a23ab4d950e30cf556f14240710097bbade6685bc19a21c42191929af3a817670a80386a89b46cc4b38438a7d30f"}}},"tx_set_hash":"0ea4f66cf0619b4e921e2f059ca7da7a7c8e0ec10baa096fd876906e11cbc5db","upgrades":[]},"skip_list":["cc1293025d3ed2c1585ffc20eadb7d1ec553fc085377739c4e112abadfccad9f","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000"],"total_coins":"1000000000000000000","tx_set_result_hash":"ba5615c4388d6c1e55e31e013f72d791bb31b0951711d1dc6e28cf47754853f6"}},"scp_info":[],"total_byte_size_of_live_soroban_state":"40407","tx_processing":[{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999701733","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":73,"seq_time":"1773532800"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776133","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":73}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6IC... (10688 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAAAOvjx2Y75qZB+mvmopjbQyXVe3IOjVmUYtVKKzZjLnIwAAAB7HaMc+9I4E9DPwcxjwwWX/4bZR1beMoUCGpLCe+F3RKQ6k9mzwYZtOkh4vBZyn2np8jg7BC6oJb9h2kG4Ry8XbAAAAAGm3SAAAAAAAAAAAAwAAAZzz8UAAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQJOGw4jJA9MqFSUOmSUGyUReuaI6tNlQ4wz1VvFCQHEAl7ut5mhbwZohxCGRkprzqBdnCoA4aom0bMSzhDin0w+6VhXEOI1sHlXjHgE/cteRuzGwlRcR0dxuKM9HdUhT9qy9AutWN5UnFEKVDOgAIZ29RnIObYmU7f1wZProOEkyAAAASg3gtrOnZAAAAAAAAAAIS+UAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTMEpMCXT7SwVhf/CDq230exVP8CFN3c5xOESq638ytnwAAAAAAAAAA... (6140 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 7674"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk rust --version 30.0.0 --entry soroban.scval-shapes.p30-map-depth-over
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-over/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-over/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-over/mainnet.xdr
- [`corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-over/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-over/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-over/testnet.xdr

How the corpus entry was built and checked: 99 nested maps, one more than the host accepts (simulation: HostError: Error(Value, InvalidInput)): core includes the invocation and fails it with invoke_host_function Trapped, so the value exists only in the envelope

Related: https://github.com/stellar/js-xdr/pull/155

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `soroban.scval-shapes.p30-map-depth-over`
