<!-- repo: Beans-BV/dotnet-stellar-sdk -->
# stellar-dotnet-sdk 17.0.0: Transaction.FromEnvelopeXdr refuses envelopes with an allow trust or inflation operation

## What happens

Transaction.FromEnvelopeXdr refuses envelopes with an allow trust or inflation operation.

## Cause

Operation.FromXdr throws NotSupportedException for ALLOW_TRUST and INFLATION (StellarDotnetSdk/Operations/Operation.cs L101-L106 at 17.0.0, since PR #15 removed the builders), so the sdk builds no transaction object, nor hash, for envelopes core applied. Allow trust is still a valid operation in protocol 30; inflation was valid until protocol 12. Mainnet ledger 37466023 envelope 108 (allow trust) and testnet ledger 5521432 envelopes 2 and 8 cannot be hashed.

## Affected versions

`stellar-dotnet-sdk` 16.0.0, 17.0.0, every version shapekit ran.

## Entries

shapekit's finding NET-DEPRECATED-OPS takes 32 corpus entries on this SDK: `cap03.offer-liabilities.mainnet-37466023`, `cap18.maintain-liabilities.golden-3f24be68a9962089-4`, `cap18.maintain-liabilities.p30-allow-trust-authorize-alphanum4`, `cap18.maintain-liabilities.p30-allow-trust-authorized-to-maintain-alphanum4`, `cap18.maintain-liabilities.p30-allow-trust-maintain-to-revoked-alphanum4`, `cap18.maintain-liabilities.p30-allow-trust-unauthorized-to-maintain-alphanum12`, `cap18.maintain-liabilities.p30-set-flags-and-allow-trust-maintain`, `cap26.inflation.p11-inflation-not-time`, `cap26.inflation.p11-inflation-payout`, `cap38.liquidity-pools.testnet-5521432`, `classic.failed-tx.p12-allow-trust-self-not-allowed`, `classic.failed-tx.p12-allow-trust-trust-not-required`, and 20 more. The checks below are those of `cap18.maintain-liabilities.golden-3f24be68a9962089-4`.

## Checks on `cap18.maintain-liabilities.golden-3f24be68a9962089-4`

On this entry, `stellar-dotnet-sdk` 17.0.0 answers differently from the Stellar network in 1 check on 1 version.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

Corpus entry `cap18.maintain-liabilities.golden-3f24be68a9962089-4` (family `cap18.maintain-liabilities`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `any/tx_hash 0` on 17.0.0

The SDK refused a value the network accepts: NotSupportedException: AllowTrustOperation is no longer supported

```json
{
  "depth_limit": 1000,
  "len_limit": 200,
  "network_passphrase": "(V) (;,,;) (V)",
  "op": "tx_hash",
  "xdr": "AAAAAgAAAACMTXz01XEANUvVc2WIDk4YQsp9Hd4bQ+GMXuPR6va8NgAAAGQAAAACAAAAAgAAAAAAAAAAAAAAAQAAAAAAAAAHAAAAAKzPvHNItJ7B2BpAit6MFbiug14LUhz00tODySgBTIW7AAAAAVVTRAAAAAABAAAAAAAAAAHq9rw2AAAAQJr0wBNq0MU7i416LjF/hT3x1ftN443zD+Eq+gYZKkGn62fDvhX7O6VqnTJifKFJ0hLPA1az1O+xbLylKuqRrgA="
}
```

Expected: `{"hash":"0b31cc9e2768310382ecbcc4840bdd4e3f35de9fe5cddc5ebdd1c2cd09f3176d"}`

Actual: `{"rejected":"NotSupportedException: AllowTrustOperation is no longer supported."}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`, with the Rust toolchain of its `rust-toolchain.toml` and the toolchain `stellar-dotnet-sdk` builds with (`shapekit doctor sdk --sdk dotnet` lists what is missing):

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit conformance run --sdk dotnet --version 17.0.0 --entry cap18.maintain-liabilities.golden-3f24be68a9962089-4
```

The run builds the SDK's testee at that version as `conformance/versions.toml` describes, then runs the entry's checks.

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap18.maintain-liabilities/cap18.maintain-liabilities.golden-3f24be68a9962089-4/any.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap18.maintain-liabilities/cap18.maintain-liabilities.golden-3f24be68a9962089-4/any.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair

How the corpus entry was built and checked: Core test "deauth removes offers on frozen account", ledger 6 (record 5 of 12 in the upstream file). smallest golden ledger matching OperationBody:allow_trust, .body:allow_trust.asset:credit_alphanum4

## Sibling issues

- `stellar_flutter_sdk` (Soneso/stellar_flutter_sdk) diverges on 5 of these entries: shapekit's finding DART-INFLATION, AbstractTransaction.fromEnvelopeXdr refuses an envelope with an inflation operation (not filed yet); DART-PRICE, a transaction whose offer price is not in lowest terms hashes to another value once the sdk rebuilds it (not filed yet).
- `network.lightsail:stellar-sdk` (lightsail-network/java-stellar-sdk) diverges on 32 of these entries: no finding takes it yet.
- `@stellar/stellar-sdk` (stellar/js-stellar-sdk) diverges on `cap38.liquidity-pools.testnet-5521432`, one of these entries: shapekit's finding JS-1766, Asset.compare and scvSortedMap ordered by text instead of the network's byte order (covered upstream by https://github.com/stellar/js-stellar-sdk/pull/1766).
- `soneso/stellar-php-sdk` (Soneso/stellar-php-sdk) diverges on 6 of these entries: shapekit's finding PHP-CBKEY-JSON, a claimable balance ledger key renders without its balance_id member in XDR-JSON (not filed yet); PHP-INFLATION, AbstractTransaction::fromEnvelopeXdr refuses an envelope with an inflation operation (not filed yet); PHP-SETOPTIONS, a set options operation with clear_flags 0, set_flags 0 or a home domain of "" or "0" hashes to another value once the sdk rebuilds it (not filed yet).
- `stellar-sdk` (StellarCN/py-stellar-base) diverges on 32 of these entries: no finding takes it yet.
- `stellar-cli` (stellar/stellar-cli) diverges on 10 of these entries: shapekit's finding CLI-TXHASH-V1, stellar tx hash refuses v0 and fee bump envelopes the network applied (covered upstream by https://github.com/stellar/stellar-cli/issues/2769).

shapekit vector: `cap18.maintain-liabilities.golden-3f24be68a9962089-4`
