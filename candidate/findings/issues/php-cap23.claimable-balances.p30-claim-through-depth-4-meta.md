<!-- repo: Soneso/stellar-php-sdk -->
# soneso/stellar-php-sdk 1.18.0: a claimable balance ledger key renders without its balance_id member in XDR-JSON

## What happens

A claimable balance ledger key renders without its balance_id member in XDR-JSON.

## Cause

XdrLedgerKeyBase::toJsonValue writes the claimable_balance arm as the bare ClaimableBalanceID, and fromJsonValue reads it that way (Soneso/StellarSDK/Xdr/XdrLedgerKeyBase.php L148 and L183), where the schema's arm is the struct LedgerKeyClaimableBalance { balance_id }, so the canonical XDR-JSON of every removed or restored claimable balance is refused ("Expected string for XdrClaimableBalanceID JSON value, got array").

## Affected versions

`soneso/stellar-php-sdk` 1.18.0, every version shapekit ran.

## Entries

shapekit's finding PHP-CBKEY-JSON takes 15 corpus entries on this SDK: `cap03.offer-liabilities.mainnet-37466023`, `cap03.offer-liabilities.mainnet-44679511`, `cap23.claimable-balances.p30-claim-through-depth-4-meta`, `cap35.clawback.p30-clawback-claimable-balance-meta`, `cap67.unified-events.p22-backfill-claim-claimable-balance`, `cap67.unified-events.p22-backfill-clawback-claimable-balance`, `cap67.unified-events.p22-claim-claimable-balance`, `cap67.unified-events.p22-clawback-claimable-balance`, `cap67.unified-events.p30-claim-claimable-balance`, `cap67.unified-events.p30-clawback-claimable-balance`, `classic.failed-tx.p30-revoke-only-transferable`, `explore.witnesses.f62c-r0000-l86`, and 3 more. The checks below are those of `cap23.claimable-balances.p30-claim-through-depth-4-meta`.

## Checks on `cap23.claimable-balances.p30-claim-through-depth-4-meta`

On this entry, `soneso/stellar-php-sdk` 1.18.0 answers differently from the Stellar network in 4 checks on 1 version.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

A check named `mainnet/...` or `testnet/...` names the network passphrase it hashes or signs with. The data is a ledger stellar-core closed on a private network shapekit ran locally (chain `cap23.claimable-balances.p30`), not a transaction or ledger of that network.

Corpus entry `cap23.claimable-balances.p30-claim-through-depth-4-meta` (family `cap23.claimable-balances`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `testnet/decode_json` on 1.18.0

The SDK's XDR-JSON differs from the expected value

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

The SDK refused a value the network accepts: InvalidArgumentException: Expected string for XdrClaimableBalanceID JSON value, got array

```json
{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"100000000000","ext":"v0","flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906944","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":64}},{"updated":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"99999999900","ext":"v0","flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906944","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":66}}],"post_tx_apply_fee_processing":[],"result":{"result":{"ext":"v0","fee_charged":"100","result":{"tx_success":[{"op_inner":{"claim_claimable_balance":"success"}}]}},"transaction_hash":"e21911e104f314d546bf3486b8c67dba96727af3153de08e99d311ae34c0d105"},"tx_apply_processing":{"v4":{"diagnostic_events":[],"events":[{"event":{"body":{"v0":{"data":{"i128":"100"},"topics":[{"symbol":"fee"},{"address":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU"}]}},"contract_id":"CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC","ext":"v0","type":"contract"},"stage":"before_all_txs"}],"ext":"v0","operations":[{"changes":[{"state":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"99999999900","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":66,"seq_time":"1772928000"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906945","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":66}},{"updated":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"100999999900","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":66,"seq_time... (6935 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAOIZEeEE8xTVRr80hrjGfbqWcnrzFT3gjpnTEa40wNEFAAAAAAAAAGQAAAAAAAAAAQAAAAAAAAAPAAAAAAAAAAAAAAACAAAAAwAAAEAAAAAAAAAAAB85Ou56zTQverZmh/ncjRctbQu9zdxtDyUXhj6iK93HAAAAF0h26AAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAQAAAEIAAAAAAAAAAB85Ou56zTQverZmh/ncjRctbQu9zdxtDyUXhj6iK93HAAAAF0h255wAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAACAAAAAwAAAEIAAAAAAAAAAB85Ou56zTQverZmh/ncjRctbQu9zdxtDyUXhj6iK93HAAAAF0h255wAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAQAAAEIAAAAA... (3200 chars)"}`

Actual: `{"rejected":"InvalidArgumentException: Expected string for XdrClaimableBalanceID JSON value, got array"}`

### `mainnet/decode_json` on 1.18.0

The SDK's XDR-JSON differs from the expected value

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

The SDK refused a value the network accepts: InvalidArgumentException: Expected string for XdrClaimableBalanceID JSON value, got array

```json
{"json":{"ext":"v0","fee_processing":[{"state":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"100000000000","ext":"v0","flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906944","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":64}},{"updated":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"99999999900","ext":"v0","flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906944","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":66}}],"post_tx_apply_fee_processing":[],"result":{"result":{"ext":"v0","fee_charged":"100","result":{"tx_success":[{"op_inner":{"claim_claimable_balance":"success"}}]}},"transaction_hash":"d7178d84d4dfa75779e6b5b3f738660a1db0b0cd0e99d7ac67948ae6495713c8"},"tx_apply_processing":{"v4":{"diagnostic_events":[],"events":[{"event":{"body":{"v0":{"data":{"i128":"100"},"topics":[{"symbol":"fee"},{"address":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU"}]}},"contract_id":"CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA","ext":"v0","type":"contract"},"stage":"before_all_txs"}],"ext":"v0","operations":[{"changes":[{"state":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"99999999900","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":66,"seq_time":"1772928000"}},"num_sponsored":0,"num_sponsoring":0,"signer_sponsoring_i_ds":[]}},"liabilities":{"buying":"0","selling":"0"}}},"flags":0,"home_domain":"","inflation_dest":null,"num_sub_entries":0,"seq_num":"274877906945","signers":[],"thresholds":"01000000"}},"ext":"v0","last_modified_ledger_seq":66}},{"updated":{"data":{"account":{"account_id":"GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU","balance":"100999999900","ext":{"v1":{"ext":{"v2":{"ext":{"v3":{"ext":"v0","seq_ledger":66,"seq_time... (6935 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAANcXjYTU36dXeea1s/c4ZgodsLDNDpnXrGeUiuZJVxPIAAAAAAAAAGQAAAAAAAAAAQAAAAAAAAAPAAAAAAAAAAAAAAACAAAAAwAAAEAAAAAAAAAAAB85Ou56zTQverZmh/ncjRctbQu9zdxtDyUXhj6iK93HAAAAF0h26AAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAQAAAEIAAAAAAAAAAB85Ou56zTQverZmh/ncjRctbQu9zdxtDyUXhj6iK93HAAAAF0h255wAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAACAAAAAwAAAEIAAAAAAAAAAB85Ou56zTQverZmh/ncjRctbQu9zdxtDyUXhj6iK93HAAAAF0h255wAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAQAAAEIAAAAA... (3200 chars)"}`

Actual: `{"rejected":"InvalidArgumentException: Expected string for XdrClaimableBalanceID JSON value, got array"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`, with the Rust toolchain of its `rust-toolchain.toml` and the toolchain `soneso/stellar-php-sdk` builds with (`shapekit doctor sdk --sdk php` lists what is missing):

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit conformance run --sdk php --version 1.18.0 --entry cap23.claimable-balances.p30-claim-through-depth-4-meta
```

The run builds the SDK's testee at that version as `conformance/versions.toml` describes, then runs the entry's checks.

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap23.claimable-balances/cap23.claimable-balances.p30-claim-through-depth-4-meta/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap23.claimable-balances/cap23.claimable-balances.p30-claim-through-depth-4-meta/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/cap23.claimable-balances/cap23.claimable-balances.p30-claim-through-depth-4-meta/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap23.claimable-balances/cap23.claimable-balances.p30-claim-through-depth-4-meta/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/cap23.claimable-balances/cap23.claimable-balances.p30-claim-through-depth-4-meta/mainnet.xdr
- [`corpus/entries/cap23.claimable-balances/cap23.claimable-balances.p30-claim-through-depth-4-meta/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap23.claimable-balances/cap23.claimable-balances.p30-claim-through-depth-4-meta/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/cap23.claimable-balances/cap23.claimable-balances.p30-claim-through-depth-4-meta/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap23.claimable-balances/cap23.claimable-balances.p30-claim-through-depth-4-meta/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/cap23.claimable-balances/cap23.claimable-balances.p30-claim-through-depth-4-meta/testnet.xdr

How the corpus entry was built and checked: The claimant whose predicate is a depth 4 and, or, not tree claims the balance, which deletes the entry

## Sibling issues

- `stellar_flutter_sdk` (Soneso/stellar_flutter_sdk) diverges on `cap03.offer-liabilities.mainnet-37466023`, one of these entries: shapekit's finding DART-PRICE, a transaction whose offer price is not in lowest terms hashes to another value once the sdk rebuilds it (not filed yet).
- `stellar-dotnet-sdk` (Beans-BV/dotnet-stellar-sdk) diverges on `cap03.offer-liabilities.mainnet-37466023`, one of these entries: shapekit's finding NET-DEPRECATED-OPS, Transaction.FromEnvelopeXdr refuses envelopes with an allow trust or inflation operation (not filed yet); NET-PRICE, a transaction whose offer price the sdk does not get back from a decimal hashes to another value (not filed yet).
- `network.lightsail:stellar-sdk` (lightsail-network/java-stellar-sdk) diverges on 15 of these entries: no finding takes it yet.
- `com.soneso.stellar:stellar-sdk-jvm` (Soneso/kmp-stellar-sdk) diverges on `explore.witnesses.f62c-r0000-l86`, one of these entries: shapekit's finding KMP-MUXED-SOURCE, Transaction.fromEnvelopeXdr drops the muxed id of a transaction's source, so the transaction hash differs from the network's (not filed yet).
- `stellar-sdk` (StellarCN/py-stellar-base) diverges on 15 of these entries: no finding takes it yet.
- `stellar-cli` (stellar/stellar-cli) diverges on 2 of these entries: shapekit's finding CLI-TXHASH-V1, stellar tx hash refuses v0 and fee bump envelopes the network applied (covered upstream by https://github.com/stellar/stellar-cli/issues/2769).

shapekit vector: `cap23.claimable-balances.p30-claim-through-depth-4-meta`
