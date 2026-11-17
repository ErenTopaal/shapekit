<!-- repo: Soneso/stellar_flutter_sdk -->
# stellar_flutter_sdk 3.12.0: the sdk refused a value the network accepts: FormatException: FormatException: Invalid UTF-8 byte (at offset 0) (classic.memo-bytes.p30-memo-ff)

## What happens

On the input below, `stellar_flutter_sdk` 3.12.0 answers differently from the Stellar network in 8 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `classic.memo-bytes.p30-memo-ff` (family `classic.memo-bytes`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `testnet/decode_encode` on 3.12.0

the sdk refused a value the network accepts: FormatException: FormatException: Invalid UTF-8 byte (at offset 0)

```json
{
  "depth_limit": 1012,
  "len_limit": 2220,
  "op": "decode_encode",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAACrmQhbtA4Kvaf0RioFKrJL7XpjXM520/QahfjxxUHmdAAAAB6TOdgQmgEKdn9/1Mo0GotFRaBtjmePIiN9/kkQAVxqU9A/pdAgK/ksc5d71Un8nuHMZRYlMl4gsMfx1bCT5CNBAAAAAGmraoAAAAAAAAAAAwAAAZzFmAQAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQNCCNi7v1X3hbRYAraNOzNBsoeCj9noxSj+ciEtjz6Co/XDad8t34oELUlqUNVuaAf4Etda+0c6hULMP0tAEXAuhm+YJz5R+8zQxjuMqIDWPB7p/y7Yx42V1+33uoEzPSYJNAG9dROQNXrfu3CO5/kescfu9eQaUuFH5rjYRiMaaAAAAQQ3gtrOnZAAAAAAAAAAAASwAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGRvQtmygRpCRALZWz3iaGe1Q8UzxLe/B1W41XQ/vtVjaAAAAAAAAAAA... (2960 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAACrmQhbtA4Kvaf0RioFKrJL7XpjXM520/QahfjxxUHmdAAAAB6TOdgQmgEKdn9/1Mo0GotFRaBtjmePIiN9/kkQAVxqU9A/pdAgK/ksc5d71Un8nuHMZRYlMl4gsMfx1bCT5CNBAAAAAGmraoAAAAAAAAAAAwAAAZzFmAQAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQNCCNi7v1X3hbRYAraNOzNBsoeCj9noxSj+ciEtjz6Co/XDad8t34oELUlqUNVuaAf4Etda+0c6hULMP0tAEXAuhm+YJz5R+8zQxjuMqIDWPB7p/y7Yx42V1+33uoEzPSYJNAG9dROQNXrfu3CO5/kescfu9eQaUuFH5rjYRiMaaAAAAQQ3gtrOnZAAAAAAAAAAAASwAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGRvQtmygRpCRALZWz3iaGe1Q8UzxLe/B1W41XQ/vtVjaAAAAAAAAAAA... (2960 chars)"}`

Actual: `{"rejected":"FormatException: FormatException: Invalid UTF-8 byte (at offset 0)"}`

### `testnet/decode_json` on 3.12.0

the sdk refused a value the network accepts: FormatException: FormatException: Invalid UTF-8 byte (at offset 0)

```json
{
  "depth_limit": 1012,
  "len_limit": 2220,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAACrmQhbtA4Kvaf0RioFKrJL7XpjXM520/QahfjxxUHmdAAAAB6TOdgQmgEKdn9/1Mo0GotFRaBtjmePIiN9/kkQAVxqU9A/pdAgK/ksc5d71Un8nuHMZRYlMl4gsMfx1bCT5CNBAAAAAGmraoAAAAAAAAAAAwAAAZzFmAQAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQNCCNi7v1X3hbRYAraNOzNBsoeCj9noxSj+ciEtjz6Co/XDad8t34oELUlqUNVuaAf4Etda+0c6hULMP0tAEXAuhm+YJz5R+8zQxjuMqIDWPB7p/y7Yx42V1+33uoEzPSYJNAG9dROQNXrfu3CO5/kescfu9eQaUuFH5rjYRiMaaAAAAQQ3gtrOnZAAAAAAAAAAAASwAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGRvQtmygRpCRALZWz3iaGe1Q8UzxLe/B1W41XQ/vtVjaAAAAAAAAAAA... (2960 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"ab99085bb40e0abda7f4462a052ab24bed7a635cce76d3f41a85f8f1c541e674","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"824d006f5d44e40d5eb7eedc23b9fe47ac71fbbd790694b851f9ae361188c69a","ext":"v0","fee_pool":"300","id_pool":"0","inflation_seq":0,"ledger_seq":65,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"9339d8109a010a767f7fd4ca341a8b4545a06d8e678f22237dfe4910015c6a53","scp_value":{"close_time":"1772841600","ext":{"signed_ms":{"close_time_ms":"1772841600000","lc_value_signature... (6440 chars, see the source below)`

Actual: `{"rejected":"FormatException: FormatException: Invalid UTF-8 byte (at offset 0)"}`

### `testnet/encode_json` on 3.12.0

the sdk refused a value the network accepts: FormatException: FormatException: XDR-JSON XdrMemo key "text" resolves to bytes that are not valid UTF-8: "\\xff"

```json
{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"ab99085bb40e0abda7f4462a052ab24bed7a635cce76d3f41a85f8f1c541e674","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"824d006f5d44e40d5eb7eedc23b9fe47ac71fbbd790694b851f9ae361188c69a","ext":"v0","fee_pool":"300","id_pool":"0","inflation_seq":0,"ledger_seq":65,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"9339d8109a010a767f7fd4ca341a8b4545a06d8e678f22237dfe4910015c6a53","scp_value":{"close_time":"1772841600","ext":{"signed_ms":{"close_time_ms":"1772841600000","lc_value_signature":{"node_id":"GB4WH2VTILAXT2GUDH4KI2ZA2GGYSLJJUYBH6LP42LXI4WNBRIHHDDVT","signature":"d082362eefd57de16d1600ada34eccd06ca1e0a3f67a314a3f9c884b63cfa0a8fd70da77cb77e2810b525a94355b9a01fe04b5d6bed1cea150b30fd2d0045c0b"}}},"tx_set_hash":"d03fa5d0202bf92c73977bd549fc9ee1cc651625325e20b0c7f1d5b093e42341","upgrades":[]},"skip_list":["6f42d9b2811a424402d95b3de26867b543c533c4b7bf0755b8d5743fbed56368","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000","0000000000000000000000000000000000000000000000000000000000000000"],"total_coins":"1000000000000000000","tx_set_result_hash":"a19be609cf947ef334318ee32a20358f07ba7fcbb631e36575fb7deea04ccf49"}},"scp_info":[],"total_byte_size_of_live_soroban_state":"0","tx_processing":[{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GDWC5XJM77XUV2YB2J4JB6I3MY3M7IJFWFMWDLD6P25JRDF2VMQBMQAG","balance":"100000000000","ext":"v0","flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906944","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":64}},{"updated":{"data":{"account":{"account_id":"GDWC5XJM77XUV2YB2J4JB6I3MY3M7IJFWFMWDLD6P25JRDF2VMQBMQAG","balance":"99999999900","ext":"v0","flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906944","signers":[],"thresholds":"0... (6484 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAACrmQhbtA4Kvaf0RioFKrJL7XpjXM520/QahfjxxUHmdAAAAB6TOdgQmgEKdn9/1Mo0GotFRaBtjmePIiN9/kkQAVxqU9A/pdAgK/ksc5d71Un8nuHMZRYlMl4gsMfx1bCT5CNBAAAAAGmraoAAAAAAAAAAAwAAAZzFmAQAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQNCCNi7v1X3hbRYAraNOzNBsoeCj9noxSj+ciEtjz6Co/XDad8t34oELUlqUNVuaAf4Etda+0c6hULMP0tAEXAuhm+YJz5R+8zQxjuMqIDWPB7p/y7Yx42V1+33uoEzPSYJNAG9dROQNXrfu3CO5/kescfu9eQaUuFH5rjYRiMaaAAAAQQ3gtrOnZAAAAAAAAAAAASwAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGRvQtmygRpCRALZWz3iaGe1Q8UzxLe/B1W41XQ/vtVjaAAAAAAAAAAA... (2960 chars)"}`

Actual: `{"rejected":"FormatException: FormatException: XDR-JSON XdrMemo key \"text\" resolves to bytes that are not valid UTF-8: \"\\\\xff\""}`

### `testnet/tx_hash 0` on 3.12.0

the sdk refused a value the network accepts: FormatException: FormatException: Invalid UTF-8 byte (at offset 0)

```json
{
  "depth_limit": 1000,
  "len_limit": 208,
  "network_passphrase": "Test SDF Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAgAAAADsLt0s/+9K6wHSeJD5G2Y2z6ElsVlhrH5+upiMuqsgFgAAAGQAAABAAAAAAQAAAAAAAAABAAAAAf8AAAAAAAABAAAAAAAAAAEAAAAAKh+DmTZIeNBmtdkgnetsFbhUKOtC3Ge4X1mGtY/iq8YAAAAAAAAAAAAAAAEAAAAAAAAAAbqrIBYAAABAHqKaR5yP16/xJUdD/uLX437fnLHuMWRC6TC1VSpcSuNJYTm3M8dLBcBKjAZHUGnb/jDn4gy5MULmChGyPol9Bw=="
}
```

Expected: `{"hash":"0da58ec0335794e936efbb99b0c03fe10107294a01a9f62a27937ab32ff60dfa"}`

Actual: `{"rejected":"FormatException: FormatException: Invalid UTF-8 byte (at offset 0)"}`

### `mainnet/decode_encode` on 3.12.0

the sdk refused a value the network accepts: FormatException: FormatException: Invalid UTF-8 byte (at offset 0)

```json
{
  "depth_limit": 1012,
  "len_limit": 2220,
  "op": "decode_encode",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAABt0ho0R+2Wr22Ill1RWAAoCLPQL9cJxufnEmJTDkajtQAAAB51nBeqyVlqtodgZRrV3UHoLmER9DOE8o/A6GzWqoK/5hGDXpN0FRqPMkgDcMTrYDYPkuR1E9D1HFM+OSobITOSAAAAAGmraoAAAAAAAAAAAwAAAZzFmAQAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQAl1h013fQgT4YocWL0i7NYugVYoVA9ZIhNEsE0xH0QiGIsRJdjHX5kB3WqIjj5iMaL/1WDS0nFwev6PDh6PKQzFuMfKysH5r8VEpBmWPvTxvHI5znzmf5qM0SdCjhNoFcprPMNgt8ShTHU0JmUVNC2qd7bQAU+uDo9/+7TRn8FNAAAAQQ3gtrOnZAAAAAAAAAAAASwAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS8VWIx6tYjutJL65m7hPkg5EsiFFVkCvi8EFdERD2NQAAAAAAAAAAA... (2960 chars)"
}
```

Expected: `{"xdr":"AAAAAgAAAABt0ho0R+2Wr22Ill1RWAAoCLPQL9cJxufnEmJTDkajtQAAAB51nBeqyVlqtodgZRrV3UHoLmER9DOE8o/A6GzWqoK/5hGDXpN0FRqPMkgDcMTrYDYPkuR1E9D1HFM+OSobITOSAAAAAGmraoAAAAAAAAAAAwAAAZzFmAQAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQAl1h013fQgT4YocWL0i7NYugVYoVA9ZIhNEsE0xH0QiGIsRJdjHX5kB3WqIjj5iMaL/1WDS0nFwev6PDh6PKQzFuMfKysH5r8VEpBmWPvTxvHI5znzmf5qM0SdCjhNoFcprPMNgt8ShTHU0JmUVNC2qd7bQAU+uDo9/+7TRn8FNAAAAQQ3gtrOnZAAAAAAAAAAAASwAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS8VWIx6tYjutJL65m7hPkg5EsiFFVkCvi8EFdERD2NQAAAAAAAAAAA... (2960 chars)"}`

Actual: `{"rejected":"FormatException: FormatException: Invalid UTF-8 byte (at offset 0)"}`

3 more checks fail; the command below lists every one.

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk dart --version 3.12.0 --entry classic.memo-bytes.p30-memo-ff
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-ff/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-ff/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-ff/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-ff/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-ff/mainnet.xdr
- [`corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-ff/mainnet.xdr,`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-ff/mainnet.xdr,) envelope 0 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-ff/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-ff/mainnet.xdr): decode = must-accept, roundtrip = true
- [`corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-ff/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-ff/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-ff/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-ff/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-ff/testnet.xdr
- [`corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-ff/testnet.xdr,`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-ff/testnet.xdr,) envelope 0 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-ff/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/classic.memo-bytes/classic.memo-bytes.p30-memo-ff/testnet.xdr): decode = must-accept, roundtrip = true

How the corpus entry was built and checked: a payment whose 1 byte text memo is ff, not valid UTF-8, accepted by core

Related: https://github.com/stellar/js-stellar-sdk/issues/1823

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `classic.memo-bytes.p30-memo-ff`
