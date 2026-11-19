<!-- repo: Soneso/kmp-stellar-sdk -->
# com.soneso.stellar:stellar-sdk-jvm 1.18.0: Transaction.fromEnvelopeXdr drops the muxed id of a transaction's source, so the transaction hash differs from the network's

## What happens

Transaction.fromEnvelopeXdr drops the muxed id of a transaction's source, so the transaction hash differs from the network's.

## Cause

Transaction.fromV1EnvelopeXdr keeps only MuxedAccount.fromXdr(tx.sourceAccount).accountId, the G account (commonMain/kotlin/com/soneso/stellar/sdk/Transaction.kt L332 at 1.18.0), and toV1Xdr writes MuxedAccount(sourceAccount) back (L191), so a muxed source becomes a plain account: the rebuilt envelope is another transaction and AbstractTransaction.hash() is not the hash the network wrote. Mainnet transaction 17fee99b... (ledger 36196205) hashes to 0039d670... on 1.18.0.

## Affected versions

`com.soneso.stellar:stellar-sdk-jvm` 1.18.0, every version shapekit ran.

## Entries

shapekit's finding KMP-MUXED-SOURCE takes 27 corpus entries on this SDK: `cap27.muxed-accounts.p28-claimable-balance-tx-source-id-1`, `cap27.muxed-accounts.p28-tx-source-and-payment-destination-id-0`, `cap27.muxed-accounts.p28-tx-source-and-payment-destination-id-1`, `cap27.muxed-accounts.p28-tx-source-and-payment-destination-id-2p53`, `cap27.muxed-accounts.p28-tx-source-and-payment-destination-id-2p64-1`, `cap27.muxed-accounts.p30-claimable-balance-tx-source-id-1`, `cap27.muxed-accounts.p30-tx-source-and-payment-destination-id-0`, `cap27.muxed-accounts.p30-tx-source-and-payment-destination-id-1`, `cap27.muxed-accounts.p30-tx-source-and-payment-destination-id-2p53`, `cap27.muxed-accounts.p30-tx-source-and-payment-destination-id-2p64-1`, `explore.witnesses.f62c-r0000-l69`, `explore.witnesses.f62c-r0000-l77`, and 15 more. The checks below are those of `cap27.muxed-accounts.p28-claimable-balance-tx-source-id-1`.

## Checks on `cap27.muxed-accounts.p28-claimable-balance-tx-source-id-1`

On this entry, `com.soneso.stellar:stellar-sdk-jvm` 1.18.0 answers differently from the Stellar network in 2 checks on 1 version.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

A check named `mainnet/...` or `testnet/...` names the network passphrase it hashes or signs with. The data is a ledger stellar-core closed on a private network shapekit ran locally (chain `cap27.muxed-accounts.p28`), not a transaction or ledger of that network.

Corpus entry `cap27.muxed-accounts.p28-claimable-balance-tx-source-id-1` (family `cap27.muxed-accounts`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `testnet/tx_hash 0` on 1.18.0

The SDK's hash differs from the one the network wrote

```json
{
  "depth_limit": 1000,
  "len_limit": 220,
  "network_passphrase": "Test SDF Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAgAAAQAAAAAAAAAAAZoTxr4wGZDC/9ku5hA3QE1JlHDk7Cda8ORYLhHbJ1NaAAAAZAAAAEAAAAAIAAAAAAAAAAAAAAABAAAAAAAAAA4AAAAAAAAAAACYloAAAAABAAAAAAAAAABSpOQ5Hjzo6xpcSTqRRnbDTRoEupwXlZqHzS8Er14vtQAAAAAAAAAAAAAAAdsnU1oAAABA8AQlsI4WqTrcTJKg3RaDraImC2r/Svq/lgBkcLCra87fiubvXkQeBKVpUIbIgJdh11f5378yUQLzReOAuUxlBw=="
}
```

Expected: `{"hash":"76a94147c2cbc7a339355e75a1667d39e837e001071f124dc1db3d55c6e0b7eb"}`

Actual: `{"hash":"8a72e4af63445ebf64594254879f32784fdce6122e170cbbed0d23d523551526"}`

### `mainnet/tx_hash 0` on 1.18.0

The SDK's hash differs from the one the network wrote

```json
{
  "depth_limit": 1000,
  "len_limit": 220,
  "network_passphrase": "Public Global Stellar Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAgAAAQAAAAAAAAAAAZoTxr4wGZDC/9ku5hA3QE1JlHDk7Cda8ORYLhHbJ1NaAAAAZAAAAEAAAAAIAAAAAAAAAAAAAAABAAAAAAAAAA4AAAAAAAAAAACYloAAAAABAAAAAAAAAABSpOQ5Hjzo6xpcSTqRRnbDTRoEupwXlZqHzS8Er14vtQAAAAAAAAAAAAAAAdsnU1oAAABAbvLDG0o9BBPBq6J1gh5KjrriS7p8joK04zFC44ddNPn0HQwE5XmCzIfDVPqFR8v0o7ary5Q5skpwrOWYWEOQBg=="
}
```

Expected: `{"hash":"24992af14dd04b864dd1ceffb99eee87618890c6680a923beed618f55362e92e"}`

Actual: `{"hash":"00f644f7450bb8bff9d904b889ae2fa8a89818a842aa67b1811f02082b941be6"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`, with the Rust toolchain of its `rust-toolchain.toml` and the toolchain `com.soneso.stellar:stellar-sdk-jvm` builds with (`shapekit doctor sdk --sdk kmp` lists what is missing):

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit conformance run --sdk kmp --version 1.18.0 --entry cap27.muxed-accounts.p28-claimable-balance-tx-source-id-1
```

The run builds the SDK's testee at that version as `conformance/versions.toml` describes, then runs the entry's checks.

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap27.muxed-accounts/cap27.muxed-accounts.p28-claimable-balance-tx-source-id-1/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap27.muxed-accounts/cap27.muxed-accounts.p28-claimable-balance-tx-source-id-1/mainnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap27.muxed-accounts/cap27.muxed-accounts.p28-claimable-balance-tx-source-id-1/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap27.muxed-accounts/cap27.muxed-accounts.p28-claimable-balance-tx-source-id-1/testnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair

How the corpus entry was built and checked: A claimable balance created by a transaction whose source is a muxed account with id 1

Related: https://github.com/stellar/wallet-backend/issues/716

## Sibling issues

- `network.lightsail:stellar-sdk` (lightsail-network/java-stellar-sdk) diverges on 27 of these entries: no finding takes it yet.
- `soneso/stellar-php-sdk` (Soneso/stellar-php-sdk) diverges on 4 of these entries: shapekit's finding PHP-CBKEY-JSON, a claimable balance ledger key renders without its balance_id member in XDR-JSON (not filed yet); PHP-HEXHASH, inner transaction hashes and config upgrade content hashes render as the hex of their hex in XDR-JSON, and their XDR-JSON does not encode (not filed yet); PHP-UINT64, 64-bit values above 2^63-1 and negative int64 values lose their XDR-JSON form (not filed yet).
- `stellar-sdk` (StellarCN/py-stellar-base) diverges on 27 of these entries: no finding takes it yet.
- `stellar-cli` (stellar/stellar-cli) diverges on `explore.witnesses.g2-r0018-l122`, one of these entries: shapekit's finding CLI-TXHASH-V1, stellar tx hash refuses v0 and fee bump envelopes the network applied (covered upstream by https://github.com/stellar/stellar-cli/issues/2769).

shapekit vector: `cap27.muxed-accounts.p28-claimable-balance-tx-source-id-1`
