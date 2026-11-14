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
  "xdr": "AAAAAgAAAACbQv3/z72SPOVxMEDMT4gqBFTi5CSZbPy2OYPhHi3vewAAAB4HuyW7za7DCetBLMV0WbeTzwmXaK/xBsbbvkkE/rhG5wTEE9Ekf4AER8M1k3ZOrbmB29OfLbEXVOvXNWdR8bcyAAAAAGm0pQAAAAAAAAAAAwAAAZzppIgAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQOFMd4P0AWazqzOQJ81HUnLCAtPYIwFIy7KvbO2YTUz1l/f6A0thkJFkMC7IB7Olo+Ztouv6o1qtcd+gi8wdqADPKEMcuU9uFhwrv4bxibjc6Pu4URYJpBEzXQLOsTWU+dTu3hWPXWbc3vCQenWRd9lW6EKQ9cEWmhNb9TwfbuVeAAAASA3gtrOnZAAAAAAAAAAGC/IAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTK446QHpJ1G7D+xAxGCEyC/HnzGnvES2y7pwcTWVGNIwAAAAAAAAAA... (4572 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAACbQv3/z72SPOVxMEDMT4gqBFTi5CSZbPy2OYPhHi3vewAAAB4HuyW7za7DCetBLMV0WbeTzwmXaK/xBsbbvkkE/rhG5wTEE9Ekf4AER8M1k3ZOrbmB29OfLbEXVOvXNWdR8bcyAAAAAGm0pQAAAAAAAAAAAwAAAZzppIgAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQOFMd4P0AWazqzOQJ81HUnLCAtPYIwFIy7KvbO2YTUz1l/f6A0thkJFkMC7IB7Olo+Ztouv6o1qtcd+gi8wdqADPKEMcuU9uFhwrv4bxibjc6Pu4URYJpBEzXQLOsTWU+dTu3hWPXWbc3vCQenWRd9lW6EKQ9cEWmhNb9TwfbuVeAAAASA3gtrOnZAAAAAAAAAAGC/IAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTK446QHpJ1G7D+xAxGCEyC/HnzGnvES2y7pwcTWVGNIwAAAAAAAAAA... (4572 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `testnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 3428,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAACbQv3/z72SPOVxMEDMT4gqBFTi5CSZbPy2OYPhHi3vewAAAB4HuyW7za7DCetBLMV0WbeTzwmXaK/xBsbbvkkE/rhG5wTEE9Ekf4AER8M1k3ZOrbmB29OfLbEXVOvXNWdR8bcyAAAAAGm0pQAAAAAAAAAAAwAAAZzppIgAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQOFMd4P0AWazqzOQJ81HUnLCAtPYIwFIy7KvbO2YTUz1l/f6A0thkJFkMC7IB7Olo+Ztouv6o1qtcd+gi8wdqADPKEMcuU9uFhwrv4bxibjc6Pu4URYJpBEzXQLOsTWU+dTu3hWPXWbc3vCQenWRd9lW6EKQ9cEWmhNb9TwfbuVeAAAASA3gtrOnZAAAAAAAAAAGC/IAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTK446QHpJ1G7D+xAxGCEyC/HnzGnvES2y7pwcTWVGNIwAAAAAAAAAA... (4572 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"9b42fdffcfbd923ce5713040cc4f882a0454e2e424996cfcb63983e11e2def7b","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"d4eede158f5d66dcdef0907a759177d956e84290f5c1169a135bf53c1f6ee55e","ext":"v0","fee_pool":"396274","id_pool":"0","inflation_seq":0,"ledger_seq":72,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"07bb25bbcdaec309eb412cc57459b793cf099768aff106c6dbbe4904feb846e7","scp_value":{"close_time":"1773446400","ext":{"signed_ms":{"close_time_ms":"1773446400000","lc_value_signat... (7778 chars, see the source below)`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_encode` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 3428,
  "op": "decode_encode",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAAAv73zQk+V5WSLIGZdSLX36zDzft7g0LM/YE4uP3lP9/QAAAB4OGr8IyTQ/f31qM21Fa7DKBNiBc546d98/yM8E7zpgq4pVKGwCXE+40nSEMuUriNeMqrTYKOPAlHit4hw0nCe3AAAAAGm0pQAAAAAAAAAAAwAAAZzppIgAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQI+y/3wl7hnk8mqex8s7lQ1tySvfPtN6Gv+xok0MrFPDQJzB1NMra0UYETI1iQOL6sb/5PVNDjH+5pc+ULbPhQPPdgL/jfhJXcV29gOFlQ4DodSBfJRVHdBB+GFhZLQzkAKJW7FyQIC7nUKkja8a66xbHY6naQ3WZsZstovMByneAAAASA3gtrOnZAAAAAAAAAAGC/IAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTMEpMCXT7SwVhf/CDq230exVP8CFN3c5xOESq638ytnwAAAAAAAAAA... (4572 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAAAv73zQk+V5WSLIGZdSLX36zDzft7g0LM/YE4uP3lP9/QAAAB4OGr8IyTQ/f31qM21Fa7DKBNiBc546d98/yM8E7zpgq4pVKGwCXE+40nSEMuUriNeMqrTYKOPAlHit4hw0nCe3AAAAAGm0pQAAAAAAAAAAAwAAAZzppIgAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQI+y/3wl7hnk8mqex8s7lQ1tySvfPtN6Gv+xok0MrFPDQJzB1NMra0UYETI1iQOL6sb/5PVNDjH+5pc+ULbPhQPPdgL/jfhJXcV29gOFlQ4DodSBfJRVHdBB+GFhZLQzkAKJW7FyQIC7nUKkja8a66xbHY6naQ3WZsZstovMByneAAAASA3gtrOnZAAAAAAAAAAGC/IAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTMEpMCXT7SwVhf/CDq230exVP8CFN3c5xOESq638ytnwAAAAAAAAAA... (4572 chars)"}`

Actual: `{"rejected":"TypeError: XDR Read Error: exceeded max decoding depth"}`

### `mainnet/decode_json` on 16.4.1

the sdk refused a value the network accepts: TypeError: XDR Read Error: exceeded max decoding depth

```json
{
  "depth_limit": 1012,
  "len_limit": 3428,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAAAv73zQk+V5WSLIGZdSLX36zDzft7g0LM/YE4uP3lP9/QAAAB4OGr8IyTQ/f31qM21Fa7DKBNiBc546d98/yM8E7zpgq4pVKGwCXE+40nSEMuUriNeMqrTYKOPAlHit4hw0nCe3AAAAAGm0pQAAAAAAAAAAAwAAAZzppIgAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQI+y/3wl7hnk8mqex8s7lQ1tySvfPtN6Gv+xok0MrFPDQJzB1NMra0UYETI1iQOL6sb/5PVNDjH+5pc+ULbPhQPPdgL/jfhJXcV29gOFlQ4DodSBfJRVHdBB+GFhZLQzkAKJW7FyQIC7nUKkja8a66xbHY6naQ3WZsZstovMByneAAAASA3gtrOnZAAAAAAAAAAGC/IAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGTMEpMCXT7SwVhf/CDq230exVP8CFN3c5xOESq638ytnwAAAAAAAAAA... (4572 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"2fef7cd093e5795922c81997522d7dfacc3cdfb7b8342ccfd8138b8fde53fdfd","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"02895bb1724080bb9d42a48daf1aebac5b1d8ea7690dd666c66cb68bcc0729de","ext":"v0","fee_pool":"396274","id_pool":"0","inflation_seq":0,"ledger_seq":72,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"0e1abf08c9343f7f7d6a336d456bb0ca04d881739e3a77df3fc8cf04ef3a60ab","scp_value":{"close_time":"1773446400","ext":{"signed_ms":{"close_time_ms":"1773446400000","lc_value_signat... (7778 chars, see the source below)`

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
