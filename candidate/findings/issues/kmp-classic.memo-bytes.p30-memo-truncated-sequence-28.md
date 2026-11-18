<!-- repo: Soneso/kmp-stellar-sdk -->
# com.soneso.stellar:stellar-sdk-jvm 1.18.0: the sdk's bytes differ from the expected bytes (classic.memo-bytes.p30-memo-truncated-sequence-28)

## What happens

On the input below, `com.soneso.stellar:stellar-sdk-jvm` 1.18.0 answers differently from the Stellar network in 8 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `classic.memo-bytes.p30-memo-truncated-sequence-28` (family `classic.memo-bytes`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `testnet/decode_encode` on 1.18.0

the sdk's bytes differ from the expected bytes

```json
{
  "depth_limit": 1012,
  "len_limit": 2424,
  "op": "decode_encode",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAACXxMNZcy5PXPnKh07H8QAVTP8FcO3V0r7TYogkatJ1fQAAAB5O6NUaQovk2lVK4QOf3upWj/sE1glRQfoVH+1XBAt2bvEBdjnCIu4Rw88mSmOyUBnWk8rxMxPnLZe7k4v7TCz0AAAAAGmyAgAAAAAAAAAAAwAAAZzfV9AAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQEh7wTCetTv4I4RjgLl300Rvo0Ybun0DlJBhLdPjyD6Wk9ebu0zHwUzdeP0EX/ysa8zN2WKJ3wHLGCs8GIc9dQ38GakrLXy/7d3h1QlW3EDK5MxizydonE9yXuB/PZUChqJc0S/t31vKfjmuQAjZ+UbWRYGzKhV9uCuylRXdIFu5AAAARg3gtrOnZAAAAAAAAAAAAyAAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGRvQtmygRpCRALZWz3iaGe1Q8UzxLe/B1W41XQ/vtVjaAAAAAAAAAAA... (3232 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAACXxMNZcy5PXPnKh07H8QAVTP8FcO3V0r7TYogkatJ1fQAAAB5O6NUaQovk2lVK4QOf3upWj/sE1glRQfoVH+1XBAt2bvEBdjnCIu4Rw88mSmOyUBnWk8rxMxPnLZe7k4v7TCz0AAAAAGmyAgAAAAAAAAAAAwAAAZzfV9AAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQEh7wTCetTv4I4RjgLl300Rvo0Ybun0DlJBhLdPjyD6Wk9ebu0zHwUzdeP0EX/ysa8zN2WKJ3wHLGCs8GIc9dQ38GakrLXy/7d3h1QlW3EDK5MxizydonE9yXuB/PZUChqJc0S/t31vKfjmuQAjZ+UbWRYGzKhV9uCuylRXdIFu5AAAARg3gtrOnZAAAAAAAAAAAAyAAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGRvQtmygRpCRALZWz3iaGe1Q8UzxLe/B1W41XQ/vtVjaAAAAAAAAAAA... (3232 chars)"}`

Actual: `{"xdr":"AAAAAgAAAACXxMNZcy5PXPnKh07H8QAVTP8FcO3V0r7TYogkatJ1fQAAAB5O6NUaQovk2lVK4QOf3upWj/sE1glRQfoVH+1XBAt2bvEBdjnCIu4Rw88mSmOyUBnWk8rxMxPnLZe7k4v7TCz0AAAAAGmyAgAAAAAAAAAAAwAAAZzfV9AAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQEh7wTCetTv4I4RjgLl300Rvo0Ybun0DlJBhLdPjyD6Wk9ebu0zHwUzdeP0EX/ysa8zN2WKJ3wHLGCs8GIc9dQ38GakrLXy/7d3h1QlW3EDK5MxizydonE9yXuB/PZUChqJc0S/t31vKfjmuQAjZ+UbWRYGzKhV9uCuylRXdIFu5AAAARg3gtrOnZAAAAAAAAAAAAyAAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGRvQtmygRpCRALZWz3iaGe1Q8UzxLe/B1W41XQ/vtVjaAAAAAAAAAAA... (3244 chars)"}`

### `testnet/decode_json` on 1.18.0

the sdk's XDR-JSON differs from the expected value

```json
{
  "depth_limit": 1012,
  "len_limit": 2424,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAACXxMNZcy5PXPnKh07H8QAVTP8FcO3V0r7TYogkatJ1fQAAAB5O6NUaQovk2lVK4QOf3upWj/sE1glRQfoVH+1XBAt2bvEBdjnCIu4Rw88mSmOyUBnWk8rxMxPnLZe7k4v7TCz0AAAAAGmyAgAAAAAAAAAAAwAAAZzfV9AAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQEh7wTCetTv4I4RjgLl300Rvo0Ybun0DlJBhLdPjyD6Wk9ebu0zHwUzdeP0EX/ysa8zN2WKJ3wHLGCs8GIc9dQ38GakrLXy/7d3h1QlW3EDK5MxizydonE9yXuB/PZUChqJc0S/t31vKfjmuQAjZ+UbWRYGzKhV9uCuylRXdIFu5AAAARg3gtrOnZAAAAAAAAAAAAyAAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGRvQtmygRpCRALZWz3iaGe1Q8UzxLe/B1W41XQ/vtVjaAAAAAAAAAAA... (3232 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"97c4c359732e4f5cf9ca874ec7f100154cff0570edd5d2bed36288246ad2757d","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"a25cd12feddf5bca7e39ae4008d9f946d64581b32a157db82bb29515dd205bb9","ext":"v0","fee_pool":"800","id_pool":"0","inflation_seq":0,"ledger_seq":70,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"4ee8d51a428be4da554ae1039fdeea568ffb04d6095141fa151fed57040b766e","scp_value":{"close_time":"1773273600","ext":{"signed_ms":{"close_time_ms":"1773273600000","lc_value_signature... (7080 chars, see the source below)`

Actual: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"97c4c359732e4f5cf9ca874ec7f100154cff0570edd5d2bed36288246ad2757d","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"a25cd12feddf5bca7e39ae4008d9f946d64581b32a157db82bb29515dd205bb9","ext":"v0","fee_pool":"800","id_pool":"0","inflation_seq":0,"ledger_seq":70,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"4ee8d51a428be4da554ae1039fdeea568ffb04d6095141fa151fed57040b766e","scp_value":{"close_time":"1773273600","ext":{"signed_ms":{"close_time_ms":"1773273600000","lc_value_signature... (7090 chars, see the source below)`

### `testnet/encode_json` on 1.18.0

the sdk's bytes differ from the expected bytes

```json
{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"97c4c359732e4f5cf9ca874ec7f100154cff0570edd5d2bed36288246ad2757d","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"a25cd12feddf5bca7e39ae4008d9f946d64581b32a157db82bb29515dd205bb9","ext":"v0","fee_pool":"800","id_pool":"0","inflation_seq":0,"ledger_seq":70,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"4ee8d51a428be4da554ae1039fdeea568ffb04d6095141fa151fed57040b766e","scp_value":{"close_time":"1773273600","ext":{"signed_ms":{"close_time_ms":"1773273600000","lc_value_signature":{"node_id":"GB4WH2VTILAXT2GUDH4KI2ZA2GGYSLJJUYBH6LP42LXI4WNBRIHHDDVT","signature":"487bc1309eb53bf823846380b977d3446fa3461bba7d039490612dd3e3c83e9693d79bbb4cc7c14cdd78fd045ffcac6bcccdd96289df01cb182b3c18873d750d"}}},"tx_set_hash":"f1017639c222ee11c3cf264a63b25019d693caf13313e72d97bb938bfb4c2cf4","upgrades":[]},"skip_list":["6f42d9b2811a424402d95b3de26867b543c533c4b7bf0755b8d5743fbed56368","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000"],"total_coins":"1000000000000000000","tx_set_result_hash":"fc19a92b2d7cbfeddde1d50956dc40cae4cc62cf27689c4f725ee07f3d950286"}},"scp_info":[],"total_byte_size_of_live_soroban_state":"0","tx_processing":[{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GDWC5XJM77XUV2YB2J4JB6I3MY3M7IJFWFMWDLD6P25JRDF2VMQBMQAG","balance":"99999999495","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":69,"seq_time":"1773187200"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906949","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":69}},{"updated":{"data":{"account":{"account_id":"GDWC5XJM77XUV2YB2J4JB6I... (7124 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAACXxMNZcy5PXPnKh07H8QAVTP8FcO3V0r7TYogkatJ1fQAAAB5O6NUaQovk2lVK4QOf3upWj/sE1glRQfoVH+1XBAt2bvEBdjnCIu4Rw88mSmOyUBnWk8rxMxPnLZe7k4v7TCz0AAAAAGmyAgAAAAAAAAAAAwAAAZzfV9AAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQEh7wTCetTv4I4RjgLl300Rvo0Ybun0DlJBhLdPjyD6Wk9ebu0zHwUzdeP0EX/ysa8zN2WKJ3wHLGCs8GIc9dQ38GakrLXy/7d3h1QlW3EDK5MxizydonE9yXuB/PZUChqJc0S/t31vKfjmuQAjZ+UbWRYGzKhV9uCuylRXdIFu5AAAARg3gtrOnZAAAAAAAAAAAAyAAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGRvQtmygRpCRALZWz3iaGe1Q8UzxLe/B1W41XQ/vtVjaAAAAAAAAAAA... (3232 chars)"}`

Actual: `{"xdr":"AAAAAgAAAACXxMNZcy5PXPnKh07H8QAVTP8FcO3V0r7TYogkatJ1fQAAAB5O6NUaQovk2lVK4QOf3upWj/sE1glRQfoVH+1XBAt2bvEBdjnCIu4Rw88mSmOyUBnWk8rxMxPnLZe7k4v7TCz0AAAAAGmyAgAAAAAAAAAAAwAAAZzfV9AAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQEh7wTCetTv4I4RjgLl300Rvo0Ybun0DlJBhLdPjyD6Wk9ebu0zHwUzdeP0EX/ysa8zN2WKJ3wHLGCs8GIc9dQ38GakrLXy/7d3h1QlW3EDK5MxizydonE9yXuB/PZUChqJc0S/t31vKfjmuQAjZ+UbWRYGzKhV9uCuylRXdIFu5AAAARg3gtrOnZAAAAAAAAAAAAyAAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGRvQtmygRpCRALZWz3iaGe1Q8UzxLe/B1W41XQ/vtVjaAAAAAAAAAAA... (3244 chars)"}`

### `testnet/tx_hash 0` on 1.18.0

the sdk refused a value the network accepts: IllegalArgumentException: Text memo cannot be more than 28 bytes. The provided text is 29 bytes when UTF-8 encoded.

```json
{
  "depth_limit": 1000,
  "len_limit": 232,
  "network_passphrase": "Test SDF Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAgAAAADsLt0s/+9K6wHSeJD5G2Y2z6ElsVlhrH5+upiMuqsgFgAAAGQAAABAAAAABgAAAAAAAAABAAAAHHNoYXBla2l0IG1lbW8gY3V0IGF0IGJ5dGUg4oIAAAABAAAAAAAAAAEAAAAAKh+DmTZIeNBmtdkgnetsFbhUKOtC3Ge4X1mGtY/iq8YAAAAAAAAAAAAAAAEAAAAAAAAAAbqrIBYAAABA6wCQPaNUSUhaiN6efYSXgm32GQPioyALbv40BQmfL92/4iM+4dZB4JDcke699HTfJoYd94vlD8TgG+8NScrQAA=="
}
```

Expected: `{"hash":"16e4ed80574749f076bdd4395bad0db1399e2cadef25bc251701242976aab452"}`

Actual: `{"rejected":"IllegalArgumentException: Text memo cannot be more than 28 bytes. The provided text is 29 bytes when UTF-8 encoded."}`

### `mainnet/decode_encode` on 1.18.0

the sdk's bytes differ from the expected bytes

```json
{
  "depth_limit": 1012,
  "len_limit": 2424,
  "op": "decode_encode",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAADt5uO7xF+DL3g1PfwBtGrBAXKkVgqRnyueJUWMAitagwAAAB5MacppiS1NVpIdJkpg6qJ9+spuq8pq2N9MCwV2mRsR8ivxLGrgAetfEBC9AavoV5mNALEWNWg1CmNqh3vyyxF7AAAAAGmyAgAAAAAAAAAAAwAAAZzfV9AAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQM3bpDR82wXTfMbI5CFayvJVtbPCUOJP8f3W1kC8AY4KQhCg0OstC69+zrBh7EGG6HLs0x2ASgMqnHkr3mBZSw0Zts8SwIjc8WgSGR6cflZcHeT/3vSqx9GoDiOH3A+ZqHn3hDfYaIJcMJZktddQbaKokiVucqOHPSc0pQe8bfBGAAAARg3gtrOnZAAAAAAAAAAAAyAAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS8VWIx6tYjutJL65m7hPkg5EsiFFVkCvi8EFdERD2NQAAAAAAAAAAA... (3232 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAADt5uO7xF+DL3g1PfwBtGrBAXKkVgqRnyueJUWMAitagwAAAB5MacppiS1NVpIdJkpg6qJ9+spuq8pq2N9MCwV2mRsR8ivxLGrgAetfEBC9AavoV5mNALEWNWg1CmNqh3vyyxF7AAAAAGmyAgAAAAAAAAAAAwAAAZzfV9AAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQM3bpDR82wXTfMbI5CFayvJVtbPCUOJP8f3W1kC8AY4KQhCg0OstC69+zrBh7EGG6HLs0x2ASgMqnHkr3mBZSw0Zts8SwIjc8WgSGR6cflZcHeT/3vSqx9GoDiOH3A+ZqHn3hDfYaIJcMJZktddQbaKokiVucqOHPSc0pQe8bfBGAAAARg3gtrOnZAAAAAAAAAAAAyAAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS8VWIx6tYjutJL65m7hPkg5EsiFFVkCvi8EFdERD2NQAAAAAAAAAAA... (3232 chars)"}`

Actual: `{"xdr":"AAAAAgAAAADt5uO7xF+DL3g1PfwBtGrBAXKkVgqRnyueJUWMAitagwAAAB5MacppiS1NVpIdJkpg6qJ9+spuq8pq2N9MCwV2mRsR8ivxLGrgAetfEBC9AavoV5mNALEWNWg1CmNqh3vyyxF7AAAAAGmyAgAAAAAAAAAAAwAAAZzfV9AAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQM3bpDR82wXTfMbI5CFayvJVtbPCUOJP8f3W1kC8AY4KQhCg0OstC69+zrBh7EGG6HLs0x2ASgMqnHkr3mBZSw0Zts8SwIjc8WgSGR6cflZcHeT/3vSqx9GoDiOH3A+ZqHn3hDfYaIJcMJZktddQbaKokiVucqOHPSc0pQe8bfBGAAAARg3gtrOnZAAAAAAAAAAAAyAAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS8VWIx6tYjutJL65m7hPkg5EsiFFVkCvi8EFdERD2NQAAAAAAAAAAA... (3244 chars)"}`

3 more checks fail; the command below lists every one.

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`:

```sh
shapekit conformance run --sdk kmp --version 1.18.0 --entry classic.memo-bytes.p30-memo-truncated-sequence-28
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-truncated-sequence-28/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-truncated-sequence-28/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-truncated-sequence-28/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-truncated-sequence-28/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-truncated-sequence-28/mainnet.xdr
- [`corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-truncated-sequence-28/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-truncated-sequence-28/mainnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-truncated-sequence-28/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-truncated-sequence-28/mainnet.xdr): decode = must-accept, roundtrip = true
- [`corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-truncated-sequence-28/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-truncated-sequence-28/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-truncated-sequence-28/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-truncated-sequence-28/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-truncated-sequence-28/testnet.xdr
- [`corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-truncated-sequence-28/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-truncated-sequence-28/testnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-truncated-sequence-28/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-truncated-sequence-28/testnet.xdr): decode = must-accept, roundtrip = true

How the corpus entry was built and checked: a payment whose 28 byte text memo is 73686170656b6974206d656d6f20637574206174206279746520e282, not valid UTF-8, accepted by core

Related: https://github.com/stellar/js-stellar-sdk/issues/1823

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `classic.memo-bytes.p30-memo-truncated-sequence-28`
