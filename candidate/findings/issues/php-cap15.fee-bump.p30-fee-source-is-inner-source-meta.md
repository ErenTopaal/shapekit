<!-- repo: Soneso/stellar-php-sdk -->
# soneso/stellar-php-sdk 1.18.0: inner transaction hashes and config upgrade content hashes render as the hex of their hex in XDR-JSON, and their XDR-JSON does not encode

## What happens

Inner transaction hashes and config upgrade content hashes render as the hex of their hex in XDR-JSON, and their XDR-JSON does not encode.

## Cause

XdrInnerTransactionResultPair and XdrConfigUpgradeSetKey keep their hash as hex text (bin2hex in decode, Soneso/StellarSDK/Xdr/XdrInnerTransactionResultPair.php L21 and Xdr/XdrConfigUpgradeSetKey.php L18), while the base classes' toJsonValue hex-encodes the field again (Xdr/XdrInnerTransactionResultPairBase.php L52), so XDR-JSON carries a 128-character string. Reading that JSON back fails ("Unexpected length for value. Has length 16, expected 32") or crashes with a TypeError in XdrEncoder::opaqueFixed. The bytes round trip; only XDR-JSON is wrong. Encoding the pair from a decoded value also raises php warnings (pack(): illegal hex digit) on stderr.

## Affected versions

`soneso/stellar-php-sdk` 1.18.0, every version shapekit ran.

## Entries

shapekit's finding PHP-HEXHASH takes 60 corpus entries on this SDK: `cap15.fee-bump.golden-f5fa0a4b15bfd9d9-12`, `cap15.fee-bump.mainnet-51000294-a25b8226-meta`, `cap15.fee-bump.mainnet-57369500`, `cap15.fee-bump.p30-fee-source-is-inner-source-meta`, `cap15.fee-bump.p30-fee-source-is-inner-source-pre-auth-signer-meta`, `cap15.fee-bump.p30-inner-bad-auth`, `cap15.fee-bump.p30-inner-bad-auth-extra`, `cap15.fee-bump.p30-inner-bad-min-seq-gap`, `cap15.fee-bump.p30-inner-bad-seq`, `cap15.fee-bump.p30-inner-bad-sponsorship`, `cap15.fee-bump.p30-inner-failed-meta`, `cap15.fee-bump.p30-inner-success-meta`, and 48 more. The checks below are those of `cap15.fee-bump.p30-fee-source-is-inner-source-meta`.

## Checks on `cap15.fee-bump.p30-fee-source-is-inner-source-meta`

On this entry, `soneso/stellar-php-sdk` 1.18.0 answers differently from the Stellar network in 4 checks on 1 version.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

A check named `mainnet/...` or `testnet/...` names the network passphrase it hashes or signs with. The data is a ledger stellar-core closed on a private network shapekit ran locally (chain `cap15.fee-bump.p30`), not a transaction or ledger of that network.

Corpus entry `cap15.fee-bump.p30-fee-source-is-inner-source-meta` (family `cap15.fee-bump`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `testnet/decode_json` on 1.18.0

The SDK's XDR-JSON differs from the expected value

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

The SDK refused a value the network accepts: InvalidArgumentException: Unexpected length for value. Has length 16, expected 32

```json
{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX","balance":"99980000000","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":68,"seq_time":"1773100800"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906948","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":68}},{"updated":{"data":{"account":{"account_id":"GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX","balance":"99979999800","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":68,"seq_time":"1773100800"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906948","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":69}}],"post_tx_apply_fee_processing":[],"result":{"result":{"ext":"v0","fee_charged":"200","result":{"tx_fee_bump_inner_success":{"result":{"ext":"v0","fee_charged":"0","result":{"tx_success":[{"op_inner":{"payment":"success"}}]}},"transaction_hash":"f4293fcceadecf3ecf1bccd293c5322638d35916862a240dde3b1b3f9aabc5bc"}}},"transaction_hash":"80ca7aaca83af069d3dd028f06ff7a83a56d243b0a6442f1fd081dd972047c90"},"tx_apply_processing":{"v4":{"diagnostic_events":[],"events":[{"event":{"body":{"v0":{"data":{"i128":"200"},"topics":[{"symbol":"fee"},{"address":"GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX"}]}},"contract_id":"CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC","ext":"v0","type":"contract"},"stage":"before_all_txs"}],"ext":"v0","operations":[{"changes":[{"state":{"data":{"account":{"account_id":"GAE24PMV3GAMPVGKAIEY6ZFAW5BSVQ4ML4R4FGU4DVAKOHHO2C6YVYOK","balance":"100020000000","ext":"v0","flags":0,"home_domain":"","inflati... (5978 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAIDKeqyoOvBp090Cjwb/eoOlbSQ7CmRC8f0IHdlyBHyQAAAAAAAAAMgAAAAB9Ck/zOrezz7PG8zSk8UyJjjTWRaGKiQN3jsbP5qrxbwAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAACAAAAAwAAAEQAAAAAAAAAAEoInXCKrhYuB4fjM/vhDUqq3w/vezlScqvmKq6EXM+qAAAAF0dFuwAAAABAAAAABAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAAAAAARAAAAABpr18AAAAAAAAAAAEAAABFAAAAAAAAAABKCJ1wiq4WLgeH4zP74Q1Kqt8P73s5UnKr5iquhFzPqgAAABdHRbo4AAAAQAAAAAQAAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAA... (2492 chars)"}`

Actual: `{"rejected":"InvalidArgumentException: Unexpected length for value. Has length 16, expected 32"}`

### `mainnet/decode_json` on 1.18.0

The SDK's XDR-JSON differs from the expected value

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

The SDK refused a value the network accepts: InvalidArgumentException: Unexpected length for value. Has length 16, expected 32

```json
{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX","balance":"99980000000","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":68,"seq_time":"1773100800"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906948","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":68}},{"updated":{"data":{"account":{"account_id":"GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX","balance":"99979999800","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":68,"seq_time":"1773100800"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906948","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":69}}],"post_tx_apply_fee_processing":[],"result":{"result":{"ext":"v0","fee_charged":"200","result":{"tx_fee_bump_inner_success":{"result":{"ext":"v0","fee_charged":"0","result":{"tx_success":[{"op_inner":{"payment":"success"}}]}},"transaction_hash":"dc4039e3c605c366b0f9c71390bb7c3d7d918473f7adfb9f9f7cef6ebcdb46b9"}}},"transaction_hash":"c8163b48809da9fbc4556b1ff6a2eba05b5acfa1fb09c85e5db180cc62e29ebf"},"tx_apply_processing":{"v4":{"diagnostic_events":[],"events":[{"event":{"body":{"v0":{"data":{"i128":"200"},"topics":[{"symbol":"fee"},{"address":"GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX"}]}},"contract_id":"CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA","ext":"v0","type":"contract"},"stage":"before_all_txs"}],"ext":"v0","operations":[{"changes":[{"state":{"data":{"account":{"account_id":"GAE24PMV3GAMPVGKAIEY6ZFAW5BSVQ4ML4R4FGU4DVAKOHHO2C6YVYOK","balance":"100020000000","ext":"v0","flags":0,"home_domain":"","inflati... (5978 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAMgWO0iAnan7xFVrH/ai66BbWs+h+wnIXl2xgMxi4p6/AAAAAAAAAMgAAAAB3EA548YFw2aw+ccTkLt8PX2RhHP3rfufn3zvbrzbRrkAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAACAAAAAwAAAEQAAAAAAAAAAEoInXCKrhYuB4fjM/vhDUqq3w/vezlScqvmKq6EXM+qAAAAF0dFuwAAAABAAAAABAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAAAAAARAAAAABpr18AAAAAAAAAAAEAAABFAAAAAAAAAABKCJ1wiq4WLgeH4zP74Q1Kqt8P73s5UnKr5iquhFzPqgAAABdHRbo4AAAAQAAAAAQAAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAA... (2492 chars)"}`

Actual: `{"rejected":"InvalidArgumentException: Unexpected length for value. Has length 16, expected 32"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`, with the Rust toolchain of its `rust-toolchain.toml` and the toolchain `soneso/stellar-php-sdk` builds with (`shapekit doctor sdk --sdk php` lists what is missing):

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit conformance run --sdk php --version 1.18.0 --entry cap15.fee-bump.p30-fee-source-is-inner-source-meta
```

The run builds the SDK's testee at that version as `conformance/versions.toml` describes, then runs the entry's checks.

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap15.fee-bump/cap15.fee-bump.p30-fee-source-is-inner-source-meta/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap15.fee-bump/cap15.fee-bump.p30-fee-source-is-inner-source-meta/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/cap15.fee-bump/cap15.fee-bump.p30-fee-source-is-inner-source-meta/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap15.fee-bump/cap15.fee-bump.p30-fee-source-is-inner-source-meta/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/cap15.fee-bump/cap15.fee-bump.p30-fee-source-is-inner-source-meta/mainnet.xdr
- [`corpus/entries/cap15.fee-bump/cap15.fee-bump.p30-fee-source-is-inner-source-meta/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap15.fee-bump/cap15.fee-bump.p30-fee-source-is-inner-source-meta/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/cap15.fee-bump/cap15.fee-bump.p30-fee-source-is-inner-source-meta/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap15.fee-bump/cap15.fee-bump.p30-fee-source-is-inner-source-meta/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/cap15.fee-bump/cap15.fee-bump.p30-fee-source-is-inner-source-meta/testnet.xdr

How the corpus entry was built and checked: A fee bump whose fee source is the inner transaction's own source account

Related: https://github.com/stellar/stellar-etl/issues/477

## Sibling issues

- `stellar_flutter_sdk` (Soneso/stellar_flutter_sdk) diverges on `cap15.fee-bump.mainnet-57369500`, one of these entries: shapekit's finding DART-PRICE, a transaction whose offer price is not in lowest terms hashes to another value once the sdk rebuilds it (filed as https://github.com/Soneso/stellar_flutter_sdk/issues/296).
- `stellar-dotnet-sdk` (Beans-BV/dotnet-stellar-sdk) diverges on `cap15.fee-bump.mainnet-57369500`, one of these entries: shapekit's finding NET-PRICE, a transaction whose offer price the sdk does not get back from a decimal hashes to another value (filed as https://github.com/Beans-BV/dotnet-stellar-sdk/issues/276).
- `network.lightsail:stellar-sdk` (lightsail-network/java-stellar-sdk) diverges on 47 of these entries: no finding takes it yet.
- `com.soneso.stellar:stellar-sdk-jvm` (Soneso/kmp-stellar-sdk) diverges on `explore.witnesses.g2-r0018-l122`, one of these entries: shapekit's finding KMP-MUXED-SOURCE, Transaction.fromEnvelopeXdr drops the muxed id of a transaction's source, so the transaction hash differs from the network's (filed as https://github.com/Soneso/kmp-stellar-sdk/issues/170).
- `stellar-sdk` (StellarCN/py-stellar-base) diverges on 60 of these entries: no finding takes it yet.
- `stellar-cli` (stellar/stellar-cli) diverges on 38 of these entries: shapekit's finding CLI-TXHASH-V1, stellar tx hash refuses v0 and fee bump envelopes the network applied (covered upstream by https://github.com/stellar/stellar-cli/issues/2769).

shapekit vector: `cap15.fee-bump.p30-fee-source-is-inner-source-meta`
