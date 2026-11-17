<!-- repo: Soneso/stellar-php-sdk -->
# soneso/stellar-php-sdk 1.18.0: the sdk's XDR-JSON differs from the expected value (cap23.claimable-balances.p30-claim-through-depth-4-meta)

## What happens

On the input below, `soneso/stellar-php-sdk` 1.18.0 answers differently from the Stellar network in 4 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap23.claimable-balances.p30-claim-through-depth-4-meta` (family `cap23.claimable-balances`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `testnet/decode_json` on 1.18.0

the sdk's XDR-JSON differs from the expected value

```json
{
  "depth_limit": 1000,
  "len_limit": 2400,
  "op": "decode_json",
  "type": "TransactionResultMetaV1",
  "xdr": "AAAAAOIZEeEE8xTVRr80hrjGfbqWcnrzFT3gjpnTEa40wNEFAAAAAAAAAGQAAAAAAAAAAQAAAAAAAAAPAAAAAAAAAAAAAAACAAAAAwAAAEAAAAAAAAAAAB85Ou56zTQverZmh/ncjRctbQu9zdxtDyUXhj6iK93HAAAAF0h26AAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAQAAAEIAAAAAAAAAAB85Ou56zTQverZmh/ncjRctbQu9zdxtDyUXhj6iK93HAAAAF0h255wAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAACAAAAAwAAAEIAAAAAAAAAAB85Ou56zTQverZmh/ncjRctbQu9zdxtDyUXhj6iK93HAAAAF0h255wAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAQAAAEIAAAAA... (3200 chars)"
}
```

Expected: `{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"100000000000","ext":"v0","flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906944","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":64}},{"updated":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"99999999900","ext":"v0","flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906944","signers":[],"thr... (6883 chars, see the source below)`

Actual: `{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"100000000000","ext":"v0","flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906944","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":64}},{"updated":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"99999999900","ext":"v0","flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906944","signers":[],"thr... (6868 chars, see the source below)`

### `testnet/encode_json` on 1.18.0

the sdk refused a value the network accepts: InvalidArgumentException: Expected string for XdrClaimableBalanceID JSON value, got array

```json
{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"100000000000","ext":"v0","flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906944","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":64}},{"updated":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"99999999900","ext":"v0","flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906944","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":66}}],"post_tx_apply_fee_processing":[],"result":{"result":{"ext":"v0","fee_charged":"100","result":{"tx_success":[{"op_inner":{"claim_claimable_balance":"success"}}]}},"transaction_hash":"e21911e104f314d546bf3486b8c67dba96727af3153de08e99d311ae34c0d105"},"tx_apply_processing":{"v4":{"diagnostic_events":[],"events":[{"event":{"body":{"v0":{"data":{"i128":"100"},"topics":[{"symbol":"fee"},{"address":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU"}]}},"contract_id":"CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC","ext":"v0","type":"contract"},"stage":"before_all_txs"}],"ext":"v0","operations":[{"changes":[{"state":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"99999999900","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":66,"seq_time":"1772928000"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906945","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":66}},{"updated":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"100999999900","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":66,"seq_time... (6935 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAOIZEeEE8xTVRr80hrjGfbqWcnrzFT3gjpnTEa40wNEFAAAAAAAAAGQAAAAAAAAAAQAAAAAAAAAPAAAAAAAAAAAAAAACAAAAAwAAAEAAAAAAAAAAAB85Ou56zTQverZmh/ncjRctbQu9zdxtDyUXhj6iK93HAAAAF0h26AAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAQAAAEIAAAAAAAAAAB85Ou56zTQverZmh/ncjRctbQu9zdxtDyUXhj6iK93HAAAAF0h255wAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAACAAAAAwAAAEIAAAAAAAAAAB85Ou56zTQverZmh/ncjRctbQu9zdxtDyUXhj6iK93HAAAAF0h255wAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAQAAAEIAAAAA... (3200 chars)"}`

Actual: `{"rejected":"InvalidArgumentException: Expected string for XdrClaimableBalanceID JSON value, got array"}`

### `mainnet/decode_json` on 1.18.0

the sdk's XDR-JSON differs from the expected value

```json
{
  "depth_limit": 1000,
  "len_limit": 2400,
  "op": "decode_json",
  "type": "TransactionResultMetaV1",
  "xdr": "AAAAANcXjYTU36dXeea1s/c4ZgodsLDNDpnXrGeUiuZJVxPIAAAAAAAAAGQAAAAAAAAAAQAAAAAAAAAPAAAAAAAAAAAAAAACAAAAAwAAAEAAAAAAAAAAAB85Ou56zTQverZmh/ncjRctbQu9zdxtDyUXhj6iK93HAAAAF0h26AAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAQAAAEIAAAAAAAAAAB85Ou56zTQverZmh/ncjRctbQu9zdxtDyUXhj6iK93HAAAAF0h255wAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAACAAAAAwAAAEIAAAAAAAAAAB85Ou56zTQverZmh/ncjRctbQu9zdxtDyUXhj6iK93HAAAAF0h255wAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAQAAAEIAAAAA... (3200 chars)"
}
```

Expected: `{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"100000000000","ext":"v0","flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906944","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":64}},{"updated":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"99999999900","ext":"v0","flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906944","signers":[],"thr... (6883 chars, see the source below)`

Actual: `{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"100000000000","ext":"v0","flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906944","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":64}},{"updated":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"99999999900","ext":"v0","flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906944","signers":[],"thr... (6868 chars, see the source below)`

### `mainnet/encode_json` on 1.18.0

the sdk refused a value the network accepts: InvalidArgumentException: Expected string for XdrClaimableBalanceID JSON value, got array

```json
{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"100000000000","ext":"v0","flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906944","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":64}},{"updated":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"99999999900","ext":"v0","flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906944","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":66}}],"post_tx_apply_fee_processing":[],"result":{"result":{"ext":"v0","fee_charged":"100","result":{"tx_success":[{"op_inner":{"claim_claimable_balance":"success"}}]}},"transaction_hash":"d7178d84d4dfa75779e6b5b3f738660a1db0b0cd0e99d7ac67948ae6495713c8"},"tx_apply_processing":{"v4":{"diagnostic_events":[],"events":[{"event":{"body":{"v0":{"data":{"i128":"100"},"topics":[{"symbol":"fee"},{"address":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU"}]}},"contract_id":"CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA","ext":"v0","type":"contract"},"stage":"before_all_txs"}],"ext":"v0","operations":[{"changes":[{"state":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"99999999900","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":66,"seq_time":"1772928000"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906945","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":66}},{"updated":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"100999999900","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":66,"seq_time... (6935 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAANcXjYTU36dXeea1s/c4ZgodsLDNDpnXrGeUiuZJVxPIAAAAAAAAAGQAAAAAAAAAAQAAAAAAAAAPAAAAAAAAAAAAAAACAAAAAwAAAEAAAAAAAAAAAB85Ou56zTQverZmh/ncjRctbQu9zdxtDyUXhj6iK93HAAAAF0h26AAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAQAAAEIAAAAAAAAAAB85Ou56zTQverZmh/ncjRctbQu9zdxtDyUXhj6iK93HAAAAF0h255wAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAACAAAAAwAAAEIAAAAAAAAAAB85Ou56zTQverZmh/ncjRctbQu9zdxtDyUXhj6iK93HAAAAF0h255wAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAQAAAEIAAAAA... (3200 chars)"}`

Actual: `{"rejected":"InvalidArgumentException: Expected string for XdrClaimableBalanceID JSON value, got array"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk php --version 1.18.0 --entry cap23.claimable-balances.p30-claim-through-depth-4-meta
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap23.claimable-balances/cap23.claimable-balances.p30-claim-through-depth-4-meta/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap23.claimable-balances/cap23.claimable-balances.p30-claim-through-depth-4-meta/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/cap23.claimable-balances/cap23.claimable-balances.p30-claim-through-depth-4-meta/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap23.claimable-balances/cap23.claimable-balances.p30-claim-through-depth-4-meta/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/cap23.claimable-balances/cap23.claimable-balances.p30-claim-through-depth-4-meta/mainnet.xdr
- [`corpus/entries/cap23.claimable-balances/cap23.claimable-balances.p30-claim-through-depth-4-meta/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap23.claimable-balances/cap23.claimable-balances.p30-claim-through-depth-4-meta/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/cap23.claimable-balances/cap23.claimable-balances.p30-claim-through-depth-4-meta/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap23.claimable-balances/cap23.claimable-balances.p30-claim-through-depth-4-meta/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/cap23.claimable-balances/cap23.claimable-balances.p30-claim-through-depth-4-meta/testnet.xdr

How the corpus entry was built and checked: the claimant whose predicate is a depth 4 and, or, not tree claims the balance, which deletes the entry

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap23.claimable-balances.p30-claim-through-depth-4-meta`
