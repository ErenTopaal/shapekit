<!-- repo: stellar/js-stellar-sdk -->
# @stellar/stellar-sdk 16.4.1: the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth (soroban.scval-shapes.p30-map-depth-over)

## What happens

`@stellar/stellar-sdk` 16.4.1 answers 4 of the checks of corpus entry `soroban.scval-shapes.p30-map-depth-over` differently from the Stellar network.

## Minimal vector

Entry `soroban.scval-shapes.p30-map-depth-over` (kind Xdr, family `soroban.scval-shapes`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `testnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 4604,
  "op": "decode_encode",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAAAU69glQJRTgOs7lCrwrbxmU7YAZUcjBL5v9liYJvx2hgAAAB7HKvvZZPx9BnItWrVYcXptPVUOZqv09WC5G+JkYe8+ceDfgiqarn/0kz6ynhDAXemWGjZTvB6Ze6gNT805nCprAAAAAGm3SAAAAAAAAAAAAwAAAZzz8UAAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQIbW81D53soZ0rmHcDWAYemL0BptOObsC5KAkithDo4RznQbYs328yxFc93/3MiOyUk1nLh1ubOFxPiZddvBcAVc8x+J8AD1MDH9a0YE318i0HGOXuehrMJvXTRNFiDD9BJ8ZHhyvbAoYhUxgAthj+dK638EkkFMzfumdwIyn+8MAAAASg3gtrOnZAAAAAAAAAAIS+UAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTK446QHpJ1G7D+xAxGCEyC/HnzGnvES2y7pwcTWVGNIwAAAAAAAAAA... (6140 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAAAU69glQJRTgOs7lCrwrbxmU7YAZUcjBL5v9liYJvx2hgAAAB7HKvvZZPx9BnItWrVYcXptPVUOZqv09WC5G+JkYe8+ceDfgiqarn/0kz6ynhDAXemWGjZTvB6Ze6gNT805nCprAAAAAGm3SAAAAAAAAAAAAwAAAZzz8UAAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQIbW81D53soZ0rmHcDWAYemL0BptOObsC5KAkithDo4RznQbYs328yxFc93/3MiOyUk1nLh1ubOFxPiZddvBcAVc8x+J8AD1MDH9a0YE318i0HGOXuehrMJvXTRNFiDD9BJ8ZHhyvbAoYhUxgAthj+dK638EkkFMzfumdwIyn+8MAAAASg3gtrOnZAAAAAAAAAAIS+UAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTK446QHpJ1G7D+xAxGCEyC/HnzGnvES2y7pwcTWVGNIwAAAAAAAAAA... (6140 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `testnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 4604,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAAAU69glQJRTgOs7lCrwrbxmU7YAZUcjBL5v9liYJvx2hgAAAB7HKvvZZPx9BnItWrVYcXptPVUOZqv09WC5G+JkYe8+ceDfgiqarn/0kz6ynhDAXemWGjZTvB6Ze6gNT805nCprAAAAAGm3SAAAAAAAAAAAAwAAAZzz8UAAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQIbW81D53soZ0rmHcDWAYemL0BptOObsC5KAkithDo4RznQbYs328yxFc93/3MiOyUk1nLh1ubOFxPiZddvBcAVc8x+J8AD1MDH9a0YE318i0HGOXuehrMJvXTRNFiDD9BJ8ZHhyvbAoYhUxgAthj+dK638EkkFMzfumdwIyn+8MAAAASg3gtrOnZAAAAAAAAAAIS+UAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTK446QHpJ1G7D+xAxGCEyC/HnzGnvES2y7pwcTWVGNIwAAAAAAAAAA... (6140 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"14ebd82540945380eb3b942af0adbc6653b60065472304be6ff6589826fc7686","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"127c647872bdb028621531800b618fe74aeb7f0492414ccdfba67702329fef0c","ext":"v0","fee_pool":"543717","id_pool":"0","inflation_seq":0,"ledger_seq":74,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"c72afbd964fc7d06722d5ab558717a6d3d550e66abf4f560b91be26461ef3e71","scp_value":{"close_time":"1773619200","ext":{"signed_ms":{"close_time_ms":"1773619200000","lc_value_signat... (10644 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 4604,
  "op": "decode_encode",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAAAOvjx2Y75qZB+mvmopjbQyXVe3IOjVmUYtVKKzZjLnIwAAAB7HaMc+9I4E9DPwcxjwwWX/4bZR1beMoUCGpLCe+F3RKQ6k9mzwYZtOkh4vBZyn2np8jg7BC6oJb9h2kG4Ry8XbAAAAAGm3SAAAAAAAAAAAAwAAAZzz8UAAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQJOGw4jJA9MqFSUOmSUGyUReuaI6tNlQ4wz1VvFCQHEAl7ut5mhbwZohxCGRkprzqBdnCoA4aom0bMSzhDin0w+6VhXEOI1sHlXjHgE/cteRuzGwlRcR0dxuKM9HdUhT9qy9AutWN5UnFEKVDOgAIZ29RnIObYmU7f1wZProOEkyAAAASg3gtrOnZAAAAAAAAAAIS+UAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTMEpMCXT7SwVhf/CDq230exVP8CFN3c5xOESq638ytnwAAAAAAAAAA... (6140 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAAAOvjx2Y75qZB+mvmopjbQyXVe3IOjVmUYtVKKzZjLnIwAAAB7HaMc+9I4E9DPwcxjwwWX/4bZR1beMoUCGpLCe+F3RKQ6k9mzwYZtOkh4vBZyn2np8jg7BC6oJb9h2kG4Ry8XbAAAAAGm3SAAAAAAAAAAAAwAAAZzz8UAAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQJOGw4jJA9MqFSUOmSUGyUReuaI6tNlQ4wz1VvFCQHEAl7ut5mhbwZohxCGRkprzqBdnCoA4aom0bMSzhDin0w+6VhXEOI1sHlXjHgE/cteRuzGwlRcR0dxuKM9HdUhT9qy9AutWN5UnFEKVDOgAIZ29RnIObYmU7f1wZProOEkyAAAASg3gtrOnZAAAAAAAAAAIS+UAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTMEpMCXT7SwVhf/CDq230exVP8CFN3c5xOESq638ytnwAAAAAAAAAA... (6140 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 4604,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAAAOvjx2Y75qZB+mvmopjbQyXVe3IOjVmUYtVKKzZjLnIwAAAB7HaMc+9I4E9DPwcxjwwWX/4bZR1beMoUCGpLCe+F3RKQ6k9mzwYZtOkh4vBZyn2np8jg7BC6oJb9h2kG4Ry8XbAAAAAGm3SAAAAAAAAAAAAwAAAZzz8UAAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQJOGw4jJA9MqFSUOmSUGyUReuaI6tNlQ4wz1VvFCQHEAl7ut5mhbwZohxCGRkprzqBdnCoA4aom0bMSzhDin0w+6VhXEOI1sHlXjHgE/cteRuzGwlRcR0dxuKM9HdUhT9qy9AutWN5UnFEKVDOgAIZ29RnIObYmU7f1wZProOEkyAAAASg3gtrOnZAAAAAAAAAAIS+UAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTMEpMCXT7SwVhf/CDq230exVP8CFN3c5xOESq638ytnwAAAAAAAAAA... (6140 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"0ebe3c7663be6a641fa6be6a298db4325d57b720e8d599462d54a2b36632e723","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"acbd02eb563795271442950ce800219dbd46720e6d8994edfd7064fae8384932","ext":"v0","fee_pool":"543717","id_pool":"0","inflation_seq":0,"ledger_seq":74,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"c768c73ef48e04f433f07318f0c165ffe1b651d5b78ca14086a4b09ef85dd129","scp_value":{"close_time":"1773619200","ext":{"signed_ms":{"close_time_ms":"1773619200000","lc_value_signat... (10644 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

## Reproduce

```sh
shapekit conformance run --sdk js --version 16.4.1 --entry soroban.scval-shapes.p30-map-depth-over
```

## Expected value and its source

- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-over/mainnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-over/mainnet.xdr: decode = must-accept, roundtrip = true
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-over/testnet.expected.json: SEP-51 rendering by stellar-xdr 30.0.0
- corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-map-depth-over/testnet.xdr: decode = must-accept, roundtrip = true

Entry notes: 99 nested maps, one more than the host accepts (simulation: HostError: Error(Value, InvalidInput)): core includes the invocation and fails it with invoke_host_function Trapped, so the value exists only in the envelope

Related: https://github.com/stellar/js-stellar-sdk/pull/1766, https://github.com/stellar/js-stellar-sdk/issues/1789

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.scval-shapes.p30-map-depth-over`
