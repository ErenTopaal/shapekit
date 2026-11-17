<!-- repo: Soneso/stellar-php-sdk -->
# soneso/stellar-php-sdk 1.18.0: the sdk's XDR-JSON differs from the expected value (cap15.fee-bump.p30-fee-source-is-inner-source-meta)

## What happens

On the input below, `soneso/stellar-php-sdk` 1.18.0 answers differently from the Stellar network in 4 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap15.fee-bump.p30-fee-source-is-inner-source-meta` (family `cap15.fee-bump`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `testnet/decode_json` on 1.18.0

the sdk's XDR-JSON differs from the expected value

```json
{
  "depth_limit": 1000,
  "len_limit": 1868,
  "op": "decode_json",
  "type": "TransactionResultMetaV1",
  "xdr": "AAAAAIDKeqyoOvBp090Cjwb/eoOlbSQ7CmRC8f0IHdlyBHyQAAAAAAAAAMgAAAAB9Ck/zOrezz7PG8zSk8UyJjjTWRaGKiQN3jsbP5qrxbwAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAACAAAAAwAAAEQAAAAAAAAAAEoInXCKrhYuB4fjM/vhDUqq3w/vezlScqvmKq6EXM+qAAAAF0dFuwAAAABAAAAABAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAAAAAARAAAAABpr18AAAAAAAAAAAEAAABFAAAAAAAAAABKCJ1wiq4WLgeH4zP74Q1Kqt8P73s5UnKr5iquhFzPqgAAABdHRbo4AAAAQAAAAAQAAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAA... (2492 chars)"
}
```

Expected: `{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX","balance":"99980000000","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":68,"seq_time":"1773100800"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906948","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":68}},{"updated":{"data":{"account":{"account_id":"GBFARHLQRKXB... (5926 chars, see the source below)`

Actual: `{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX","balance":"99980000000","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":68,"seq_time":"1773100800"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906948","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":68}},{"updated":{"data":{"account":{"account_id":"GBFARHLQRKXB... (5990 chars, see the source below)`

### `testnet/encode_json` on 1.18.0

the sdk refused a value the network accepts: InvalidArgumentException: Unexpected length for value. Has length 16, expected 32

```json
{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX","balance":"99980000000","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":68,"seq_time":"1773100800"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906948","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":68}},{"updated":{"data":{"account":{"account_id":"GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX","balance":"99979999800","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":68,"seq_time":"1773100800"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906948","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":69}}],"post_tx_apply_fee_processing":[],"result":{"result":{"ext":"v0","fee_charged":"200","result":{"tx_fee_bump_inner_success":{"result":{"ext":"v0","fee_charged":"0","result":{"tx_success":[{"op_inner":{"payment":"success"}}]}},"transaction_hash":"f4293fcceadecf3ecf1bccd293c5322638d35916862a240dde3b1b3f9aabc5bc"}}},"transaction_hash":"80ca7aaca83af069d3dd028f06ff7a83a56d243b0a6442f1fd081dd972047c90"},"tx_apply_processing":{"v4":{"diagnostic_events":[],"events":[{"event":{"body":{"v0":{"data":{"i128":"200"},"topics":[{"symbol":"fee"},{"address":"GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX"}]}},"contract_id":"CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC","ext":"v0","type":"contract"},"stage":"before_all_txs"}],"ext":"v0","operations":[{"changes":[{"state":{"data":{"account":{"account_id":"GAE24PMV3GAMPVGKAIEY6ZFAW5BSVQ4ML4R4FGU4DVAKOHHO2C6YVYOK","balance":"100020000000","ext":"v0","flags":0,"home_domain":"","inflati... (5978 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAIDKeqyoOvBp090Cjwb/eoOlbSQ7CmRC8f0IHdlyBHyQAAAAAAAAAMgAAAAB9Ck/zOrezz7PG8zSk8UyJjjTWRaGKiQN3jsbP5qrxbwAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAACAAAAAwAAAEQAAAAAAAAAAEoInXCKrhYuB4fjM/vhDUqq3w/vezlScqvmKq6EXM+qAAAAF0dFuwAAAABAAAAABAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAAAAAARAAAAABpr18AAAAAAAAAAAEAAABFAAAAAAAAAABKCJ1wiq4WLgeH4zP74Q1Kqt8P73s5UnKr5iquhFzPqgAAABdHRbo4AAAAQAAAAAQAAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAA... (2492 chars)"}`

Actual: `{"rejected":"InvalidArgumentException: Unexpected length for value. Has length 16, expected 32"}`

### `mainnet/decode_json` on 1.18.0

the sdk's XDR-JSON differs from the expected value

```json
{
  "depth_limit": 1000,
  "len_limit": 1868,
  "op": "decode_json",
  "type": "TransactionResultMetaV1",
  "xdr": "AAAAAMgWO0iAnan7xFVrH/ai66BbWs+h+wnIXl2xgMxi4p6/AAAAAAAAAMgAAAAB3EA548YFw2aw+ccTkLt8PX2RhHP3rfufn3zvbrzbRrkAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAACAAAAAwAAAEQAAAAAAAAAAEoInXCKrhYuB4fjM/vhDUqq3w/vezlScqvmKq6EXM+qAAAAF0dFuwAAAABAAAAABAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAAAAAARAAAAABpr18AAAAAAAAAAAEAAABFAAAAAAAAAABKCJ1wiq4WLgeH4zP74Q1Kqt8P73s5UnKr5iquhFzPqgAAABdHRbo4AAAAQAAAAAQAAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAA... (2492 chars)"
}
```

Expected: `{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX","balance":"99980000000","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":68,"seq_time":"1773100800"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906948","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":68}},{"updated":{"data":{"account":{"account_id":"GBFARHLQRKXB... (5926 chars, see the source below)`

Actual: `{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX","balance":"99980000000","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":68,"seq_time":"1773100800"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906948","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":68}},{"updated":{"data":{"account":{"account_id":"GBFARHLQRKXB... (5990 chars, see the source below)`

### `mainnet/encode_json` on 1.18.0

the sdk refused a value the network accepts: InvalidArgumentException: Unexpected length for value. Has length 16, expected 32

```json
{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX","balance":"99980000000","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":68,"seq_time":"1773100800"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906948","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":68}},{"updated":{"data":{"account":{"account_id":"GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX","balance":"99979999800","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":68,"seq_time":"1773100800"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906948","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":69}}],"post_tx_apply_fee_processing":[],"result":{"result":{"ext":"v0","fee_charged":"200","result":{"tx_fee_bump_inner_success":{"result":{"ext":"v0","fee_charged":"0","result":{"tx_success":[{"op_inner":{"payment":"success"}}]}},"transaction_hash":"dc4039e3c605c366b0f9c71390bb7c3d7d918473f7adfb9f9f7cef6ebcdb46b9"}}},"transaction_hash":"c8163b48809da9fbc4556b1ff6a2eba05b5acfa1fb09c85e5db180cc62e29ebf"},"tx_apply_processing":{"v4":{"diagnostic_events":[],"events":[{"event":{"body":{"v0":{"data":{"i128":"200"},"topics":[{"symbol":"fee"},{"address":"GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX"}]}},"contract_id":"CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA","ext":"v0","type":"contract"},"stage":"before_all_txs"}],"ext":"v0","operations":[{"changes":[{"state":{"data":{"account":{"account_id":"GAE24PMV3GAMPVGKAIEY6ZFAW5BSVQ4ML4R4FGU4DVAKOHHO2C6YVYOK","balance":"100020000000","ext":"v0","flags":0,"home_domain":"","inflati... (5978 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAMgWO0iAnan7xFVrH/ai66BbWs+h+wnIXl2xgMxi4p6/AAAAAAAAAMgAAAAB3EA548YFw2aw+ccTkLt8PX2RhHP3rfufn3zvbrzbRrkAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAACAAAAAwAAAEQAAAAAAAAAAEoInXCKrhYuB4fjM/vhDUqq3w/vezlScqvmKq6EXM+qAAAAF0dFuwAAAABAAAAABAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAAAAAARAAAAABpr18AAAAAAAAAAAEAAABFAAAAAAAAAABKCJ1wiq4WLgeH4zP74Q1Kqt8P73s5UnKr5iquhFzPqgAAABdHRbo4AAAAQAAAAAQAAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAA... (2492 chars)"}`

Actual: `{"rejected":"InvalidArgumentException: Unexpected length for value. Has length 16, expected 32"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk php --version 1.18.0 --entry cap15.fee-bump.p30-fee-source-is-inner-source-meta
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap15.fee-bump/cap15.fee-bump.p30-fee-source-is-inner-source-meta/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap15.fee-bump/cap15.fee-bump.p30-fee-source-is-inner-source-meta/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/cap15.fee-bump/cap15.fee-bump.p30-fee-source-is-inner-source-meta/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap15.fee-bump/cap15.fee-bump.p30-fee-source-is-inner-source-meta/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/cap15.fee-bump/cap15.fee-bump.p30-fee-source-is-inner-source-meta/mainnet.xdr
- [`corpus/entries/cap15.fee-bump/cap15.fee-bump.p30-fee-source-is-inner-source-meta/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap15.fee-bump/cap15.fee-bump.p30-fee-source-is-inner-source-meta/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/cap15.fee-bump/cap15.fee-bump.p30-fee-source-is-inner-source-meta/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap15.fee-bump/cap15.fee-bump.p30-fee-source-is-inner-source-meta/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/cap15.fee-bump/cap15.fee-bump.p30-fee-source-is-inner-source-meta/testnet.xdr

How the corpus entry was built and checked: a fee bump whose fee source is the inner transaction's own source account

Related: https://github.com/stellar/stellar-etl/issues/477

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap15.fee-bump.p30-fee-source-is-inner-source-meta`
