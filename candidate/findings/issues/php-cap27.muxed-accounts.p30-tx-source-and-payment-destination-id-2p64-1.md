<!-- repo: Soneso/stellar-php-sdk -->
# soneso/stellar-php-sdk 1.18.0: the sdk refused a value the network accepts: InvalidArgumentException: uint64ToString accepts only non-negative values; got -1. For uint64 values above PHP_INT_MAX, pass the value as a string to stringToUint64() (cap27.muxed-accounts.p30-tx-source-and-payment-destination-id-2p64-1)

## What happens

On the input below, `soneso/stellar-php-sdk` 1.18.0 answers differently from the Stellar network in 2 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap27.muxed-accounts.p30-tx-source-and-payment-destination-id-2p64-1` (family `cap27.muxed-accounts`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `testnet/decode_json` on 1.18.0

the sdk refused a value the network accepts: InvalidArgumentException: uint64ToString accepts only non-negative values; got -1. For uint64 values above PHP_INT_MAX, pass the value as a string to stringToUint64().

```json
{
  "depth_limit": 1012,
  "len_limit": 2384,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAADjf9PjjZSgf2pGJF0Nd9f9saeVNK8u/rt4ugill+X9KgAAAB4VPSoLAeluMLoANZFq9Gw1VG5eVRYfX/Wvm7tIuc8tnnEoJ030bYIGu1g669n2kDAG6lAa/KYOc6FSazrkeXViAAAAAGmvXwAAAAAAAAAAAwAAAZzVCxgAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQBgRvYndyq4YZmqL7Pm4JdF0oTeqBkh3rQnCDxfKc0Crsxge+m5qAAJCdKYCKwdXzv0eeB+phP+A+1+HREHPGABvx25wQXVjFkA6TwJGdkkOUH2GrZnsoPXh4na8TSy7DiJ6bLByB2nv2NtJfz/GrSL0Mp2Ykywc1jse21gZp5NKAAAARA3gtrOnZAAAAAAAAAAAA4QAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGRvQtmygRpCRALZWz3iaGe1Q8UzxLe/B1W41XQ/vtVjaAAAAAAAAAAA... (3180 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"e37fd3e38d94a07f6a46245d0d77d7fdb1a79534af2efebb78ba08a597e5fd2a","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"227a6cb0720769efd8db497f3fc6ad22f4329d98932c1cd63b1edb5819a7934a","ext":"v0","fee_pool":"900","id_pool":"0","inflation_seq":0,"ledger_seq":68,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"153d2a0b01e96e30ba0035916af46c35546e5e55161f5ff5af9bbb48b9cf2d9e","scp_value":{"close_time":"1773100800","ext":{"signed_ms":{"close_time_ms":"1773100800000","lc_value_signature... (7060 chars, see the source below)`

Actual: `{"rejected":"InvalidArgumentException: uint64ToString accepts only non-negative values; got -1. For uint64 values above PHP_INT_MAX, pass the value as a string to stringToUint64()."}`

### `mainnet/decode_json` on 1.18.0

the sdk refused a value the network accepts: InvalidArgumentException: uint64ToString accepts only non-negative values; got -1. For uint64 values above PHP_INT_MAX, pass the value as a string to stringToUint64().

```json
{
  "depth_limit": 1012,
  "len_limit": 2384,
  "op": "decode_json",
  "type": "LedgerCloseMeta",
  "xdr": "AAAAAgAAAABry95odRWiGPxBSZ9uxbYontWkkRenZdsdV+6uxI0nFAAAAB4doYobaSIWZzjo53MBr30mZobJpp7r/3fUhXRfWLwEYKxRwX5wj6lCarVe9JG9P0xDjBXJqw0fYxsrR6Fz+HECAAAAAGmvXwAAAAAAAAAAAwAAAZzVCxgAAAAAAPOxdkD8D/41P8MXoGRc+iOuhj9lK5bSxP/clfXQGNjxAAAAQFFt/eclbEhe9sMcelJFXnlWU6wzf8foOipENydVlkkYMPOt4b7U3i5K0qsYDhQLM8HT3nH3MT1yR6/i5MGYkwyGspk6uDQ6riE/Nm7O8VsspmlKPLIXUbnA+KkOFHTTaaIwYDAywlvD3sRYUv8f6m1+3yBSAGyXAkTASA26OyoGAAAARA3gtrOnZAAAAAAAAAAAA4QAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGS8VWIx6tYjutJL65m7hPkg5EsiFFVkCvi8EFdERD2NQAAAAAAAAAAA... (3180 chars)"
}
```

Expected: `{"json":{"v2":{"evicted_keys":[],"ext":"v0","ledger_header":{"ext":"v0","hash":"6bcbde687515a218fc41499f6ec5b6289ed5a49117a765db1d57eeaec48d2714","header":{"base_fee":100,"base_reserve":100000000,"bucket_list_hash":"a230603032c25bc3dec45852ff1fea6d7edf2052006c970244c0480dba3b2a06","ext":"v0","fee_pool":"900","id_pool":"0","inflation_seq":0,"ledger_seq":68,"ledger_version":30,"max_tx_set_size":100,"previous_ledger_hash":"1da18a1b6922166738e8e77301af7d266686c9a69eebff77d485745f58bc0460","scp_value":{"close_time":"1773100800","ext":{"signed_ms":{"close_time_ms":"1773100800000","lc_value_signature... (7060 chars, see the source below)`

Actual: `{"rejected":"InvalidArgumentException: uint64ToString accepts only non-negative values; got -1. For uint64 values above PHP_INT_MAX, pass the value as a string to stringToUint64()."}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk php --version 1.18.0 --entry cap27.muxed-accounts.p30-tx-source-and-payment-destination-id-2p64-1
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap27.muxed-accounts/cap27.muxed-accounts.p30-tx-source-and-payment-destination-id-2p64-1/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap27.muxed-accounts/cap27.muxed-accounts.p30-tx-source-and-payment-destination-id-2p64-1/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/cap27.muxed-accounts/cap27.muxed-accounts.p30-tx-source-and-payment-destination-id-2p64-1/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap27.muxed-accounts/cap27.muxed-accounts.p30-tx-source-and-payment-destination-id-2p64-1/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0

How the corpus entry was built and checked: a payment whose transaction source and destination are both muxed accounts with id 2p64-1

Related: https://github.com/stellar/wallet-backend/issues/716

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap27.muxed-accounts.p30-tx-source-and-payment-destination-id-2p64-1`
