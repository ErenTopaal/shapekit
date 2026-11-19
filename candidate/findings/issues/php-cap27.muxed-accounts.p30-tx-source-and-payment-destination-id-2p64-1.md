<!-- repo: Soneso/stellar-php-sdk -->
# soneso/stellar-php-sdk 1.18.0: 64-bit values above 2^63-1 and negative int64 values lose their XDR-JSON form

## What happens

64-bit values above 2^63-1 and negative int64 values lose their XDR-JSON form.

## Cause

Uint64 fields are php ints, which hold 2^64-1 as -1, and XdrJsonHelper::uint64ToString refuses negative ints (Soneso/StellarSDK/Xdr/XdrJsonHelper.php L290-L300), so a muxed id or memo id at or above 2^63 has no XDR-JSON. In the other direction XdrSequenceNumber is read as an unsigned BigInteger (Xdr/XdrBuffer.php readBigInteger64), so a bump_to of -1 renders as "18446744073709551615" where the schema's int64 says -1. Fixed on main by #229 (2026-11-18, closing #228), not in a release yet.

## Affected versions

`soneso/stellar-php-sdk` 1.18.0, every version shapekit ran.

## Entries

shapekit's finding PHP-UINT64 takes 26 corpus entries on this SDK: `cap01.bump-sequence.p30-bump-negative`, `cap27.muxed-accounts.p28-path-payment-strict-receive-destination-id-2p64-1`, `cap27.muxed-accounts.p28-tx-source-and-payment-destination-id-2p64-1`, `cap27.muxed-accounts.p30-path-payment-strict-receive-destination-id-2p64-1`, `cap27.muxed-accounts.p30-tx-source-and-payment-destination-id-2p64-1`, `cap67.unified-events.p22-backfill-account-merge-muxed-destination`, `cap67.unified-events.p22-backfill-payment-memo-id`, `cap67.unified-events.p22-payment-memo-id`, `cap67.unified-events.p30-account-merge-muxed-destination`, `cap67.unified-events.p30-payment-memo-id`, `cap84.muxed-contract.p30-xlm-transfer-to-muxed-contract`, `classic.memo-arms.p30-memo-id-2-63`, and 14 more. The checks below are those of `cap27.muxed-accounts.p30-tx-source-and-payment-destination-id-2p64-1`.

## Checks on `cap27.muxed-accounts.p30-tx-source-and-payment-destination-id-2p64-1`

On this entry, `soneso/stellar-php-sdk` 1.18.0 answers differently from the Stellar network in 2 checks on 1 version.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

A check named `mainnet/...` or `testnet/...` names the network passphrase it hashes or signs with. The data is a ledger stellar-core closed on a private network shapekit ran locally (chain `cap27.muxed-accounts.p30`), not a transaction or ledger of that network.

Corpus entry `cap27.muxed-accounts.p30-tx-source-and-payment-destination-id-2p64-1` (family `cap27.muxed-accounts`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `testnet/decode_json` on 1.18.0

The SDK refused a value the network accepts: InvalidArgumentException: uint64ToString accepts only non-negative values; got -1. For uint64 values above PHP_INT_MAX, pass the value as a string to stringToUint64()

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

The SDK refused a value the network accepts: InvalidArgumentException: uint64ToString accepts only non-negative values; got -1. For uint64 values above PHP_INT_MAX, pass the value as a string to stringToUint64()

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

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`, with the Rust toolchain of its `rust-toolchain.toml` and the toolchain `soneso/stellar-php-sdk` builds with (`shapekit doctor sdk --sdk php` lists what is missing):

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit conformance run --sdk php --version 1.18.0 --entry cap27.muxed-accounts.p30-tx-source-and-payment-destination-id-2p64-1
```

The run builds the SDK's testee at that version as `conformance/versions.toml` describes, then runs the entry's checks.

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap27.muxed-accounts/cap27.muxed-accounts.p30-tx-source-and-payment-destination-id-2p64-1/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap27.muxed-accounts/cap27.muxed-accounts.p30-tx-source-and-payment-destination-id-2p64-1/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0
- [`corpus/entries/cap27.muxed-accounts/cap27.muxed-accounts.p30-tx-source-and-payment-destination-id-2p64-1/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap27.muxed-accounts/cap27.muxed-accounts.p30-tx-source-and-payment-destination-id-2p64-1/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0

How the corpus entry was built and checked: A payment whose transaction source and destination are both muxed accounts with id 2p64-1

Related: https://github.com/stellar/wallet-backend/issues/716

## Sibling issues

- `stellar_flutter_sdk` (Soneso/stellar_flutter_sdk) diverges on 2 of these entries: shapekit's finding DART-MAXTIME, a transaction with max time 2^64-1 hashes to another value once the sdk rebuilds it (not filed yet); DART-UTF8, XDR strings are decoded as UTF-8, so valid ledgers with other bytes in a memo, symbol or string do not decode (not filed yet).
- `network.lightsail:stellar-sdk` (lightsail-network/java-stellar-sdk) diverges on 23 of these entries: no finding takes it yet.
- `com.soneso.stellar:stellar-sdk-jvm` (Soneso/kmp-stellar-sdk) diverges on 4 of these entries: shapekit's finding KMP-MUXED-SOURCE, Transaction.fromEnvelopeXdr drops the muxed id of a transaction's source, so the transaction hash differs from the network's (not filed yet); KMP-TXOBJECT, the transaction object refuses values the network applied: max time 2^64-1 and offer id -1 (not filed yet); KMP-UTF8, XDR strings are decoded to Kotlin strings, so memos, symbols and strings that are not UTF-8 change (not filed yet).
- `stellar-sdk` (StellarCN/py-stellar-base) diverges on 23 of these entries: no finding takes it yet.

shapekit vector: `cap27.muxed-accounts.p30-tx-source-and-payment-destination-id-2p64-1`
