<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 30.0.0: the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 6831 (soroban.scval-shapes.p30-vec-depth-over)

## What happens

On the input below, `stellar-xdr` 30.0.0 answers differently from the Stellar network in 2 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `soroban.scval-shapes.p30-vec-depth-over` (family `soroban.scval-shapes`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `testnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 6831

```json
{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"9b42fdffcfbd923ce5713040cc4f882a0454e2e424996cfcb63983e11e2def7b","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"d4eede158f5d66dcdef0907a759177d956e84290f5c1169a135bf53c1f6ee55e","ext":"v0","fee_pool":"396274","id_pool":"0","inflation_seq":0,"ledger_seq":72,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"07bb25bbcdaec309eb412cc57459b793cf099768aff106c6dbbe4904feb846e7","scp_value":{"close_time":"1773446400","ext":{"signed_ms":{"close_time_ms":"1773446400000","lc_value_signature":{"node_id":"GB4WH2VTILAXT2GUDH4KI2ZA2GGYSLJJUYBH6LP42LXI4WNBRIHHDDVT","signature":"e14c7783f40166b3ab339027cd475272c202d3d8230148cbb2af6ced984d4cf597f7fa034b61909164302ec807b3a5a3e66da2ebfaa35aad71dfa08bcc1da800"}}},"tx_set_hash":"04c413d1247f800447c33593764eadb981dbd39f2db11754ebd7356751f1b732","upgrades":[]},"skip_list":["cae38e901e92751bb0fec40c46084c82fc79f31a7bc44b6cbba7071359518d23","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000"],"total_coins":"1000000000000000000","tx_set_result_hash":"cf28431cb94f6e161c2bbf86f189b8dce8fbb8511609a411335d02ceb13594f9"}},"scp_info":[],"total_byte_size_of_live_soroban_state":"40407","tx_processing":[{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999844062","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":71,"seq_time":"1773360000"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776131","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":71}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6IC... (7822 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAACbQv3/z72SPOVxMEDMT4gqBFTi5CSZbPy2OYPhHi3vewAAAB4HuyW7za7DCetBLMV0WbeTzwmXaK/xBsbbvkkE/rhG5wTEE9Ekf4AER8M1k3ZOrbmB29OfLbEXVOvXNWdR8bcyAAAAAGm0pQAAAAAAAAAAAwAAAZzppIgAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQOFMd4P0AWazqzOQJ81HUnLCAtPYIwFIy7KvbO2YTUz1l/f6A0thkJFkMC7IB7Olo+Ztouv6o1qtcd+gi8wdqADPKEMcuU9uFhwrv4bxibjc6Pu4URYJpBEzXQLOsTWU+dTu3hWPXWbc3vCQenWRd9lW6EKQ9cEWmhNb9TwfbuVeAAAASA3gtrOnZAAAAAAAAAAGC/IAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTK446QHpJ1G7D+xAxGCEyC/HnzGnvES2y7pwcTWVGNIwAAAAAAAAAA... (4572 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 6831"}`

### `mainnet/encode_json` on 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 6831

```json
{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"2fef7cd093e5795922c81997522d7dfacc3cdfb7b8342ccfd8138b8fde53fdfd","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"02895bb1724080bb9d42a48daf1aebac5b1d8ea7690dd666c66cb68bcc0729de","ext":"v0","fee_pool":"396274","id_pool":"0","inflation_seq":0,"ledger_seq":72,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"0e1abf08c9343f7f7d6a336d456bb0ca04d881739e3a77df3fc8cf04ef3a60ab","scp_value":{"close_time":"1773446400","ext":{"signed_ms":{"close_time_ms":"1773446400000","lc_value_signature":{"node_id":"GDZ3C5SA7QH74NJ7YML2AZC47IR25BR7MUVZNUWE77OJL5OQDDMPCIMS","signature":"8fb2ff7c25ee19e4f26a9ec7cb3b950d6dc92bdf3ed37a1affb1a24d0cac53c3409cc1d4d32b6b451811323589038beac6ffe4f54d0e31fee6973e50b6cf8503"}}},"tx_set_hash":"8a55286c025c4fb8d2748432e52b88d78caab4d828e3c09478ade21c349c27b7","upgrades":[]},"skip_list":["cc1293025d3ed2c1585ffc20eadb7d1ec553fc085377739c4e112abadfccad9f","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000"],"total_coins":"1000000000000000000","tx_set_result_hash":"cf7602ff8df8495dc576f60385950e03a1d4817c94551dd041f8616164b43390"}},"scp_info":[],"total_byte_size_of_live_soroban_state":"40407","tx_processing":[{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6ICQVVSQIUIHH3Y3H777BATB4FLL6QWMZFZDLWYNMJX","balance":"99999844062","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":71,"seq_time":"1773360000"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"292057776131","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":71}},{"updated":{"data":{"account":{"account_id":"GDDSYMVSWJE5X6IC... (7822 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAAAv73zQk+V5WSLIGZdSLX36zDzft7g0LM/YE4uP3lP9/QAAAB4OGr8IyTQ/f31qM21Fa7DKBNiBc546d98/yM8E7zpgq4pVKGwCXE+40nSEMuUriNeMqrTYKOPAlHit4hw0nCe3AAAAAGm0pQAAAAAAAAAAAwAAAZzppIgAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQI+y/3wl7hnk8mqex8s7lQ1tySvfPtN6Gv+xok0MrFPDQJzB1NMra0UYETI1iQOL6sb/5PVNDjH+5pc+ULbPhQPPdgL/jfhJXcV29gOFlQ4DodSBfJRVHdBB+GFhZLQzkAKJW7FyQIC7nUKkja8a66xbHY6naQ3WZsZstovMByneAAAASA3gtrOnZAAAAAAAAAAGC/IAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTMEpMCXT7SwVhf/CDq230exVP8CFN3c5xOESq638ytnwAAAAAAAAAA... (4572 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 6831"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk rust --version 30.0.0 --entry soroban.scval-shapes.p30-vec-depth-over
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-over/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-over/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-over/mainnet.xdr
- [`corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-over/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-over/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-over/testnet.xdr

How the corpus entry was built and checked: 100 nested vecs, one more than the host accepts (simulation: HostError: Error(Context, ExceededLimit)): core includes the invocation and fails it with invoke_host_function Trapped, so the value exists only in the envelope

Related: https://github.com/stellar/js-stellar-sdk/pull/1766, https://github.com/stellar/js-stellar-sdk/issues/1789

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `soroban.scval-shapes.p30-vec-depth-over`
