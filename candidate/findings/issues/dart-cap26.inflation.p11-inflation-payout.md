<!-- repo: Soneso/stellar_flutter_sdk -->
# stellar_flutter_sdk 3.12.0: AbstractTransaction.fromEnvelopeXdr refuses an envelope with an inflation operation

## What happens

AbstractTransaction.fromEnvelopeXdr refuses an envelope with an inflation operation.

## Cause

Operation.fromXdr has no case for INFLATION and throws "Unknown operation body OperationType.9" (lib/src/operation.dart L258-L259), so the sdk cannot build a transaction object, nor its hash, for envelopes from before protocol 12 that core applied.

## Affected versions

`stellar_flutter_sdk` 3.12.0, every version shapekit ran.

## Entries

shapekit's finding DART-INFLATION takes 4 corpus entries on this SDK: `cap26.inflation.p11-inflation-not-time`, `cap26.inflation.p11-inflation-payout`, `explore.witnesses.f62d-r0004-l74`, `explore.witnesses.g2-r0010-l74`. The checks below are those of `cap26.inflation.p11-inflation-payout`.

## Checks on `cap26.inflation.p11-inflation-payout`

On this entry, `stellar_flutter_sdk` 3.12.0 answers differently from the Stellar network in 2 checks on 1 version.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

A check named `mainnet/...` or `testnet/...` names the network passphrase it hashes or signs with. The data is a ledger stellar-core closed on a private network shapekit ran locally (chain `cap26.inflation.p11`), not a transaction or ledger of that network.

Corpus entry `cap26.inflation.p11-inflation-payout` (family `cap26.inflation`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `testnet/tx_hash 0` on 3.12.0

The SDK refused a value the network accepts: _Exception: Exception: Unknown operation body OperationType.9

```json
{
  "depth_limit": 1000,
  "len_limit": 148,
  "network_passphrase": "Test SDF Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAEdtbmJXSbc1sZf26nS8piOAYsvwzbWym1i/Qw0Zcx4vAAAAZAAAAEAAAAABAAAAAAAAAAAAAAABAAAAAAAAAAkAAAAAAAAAARlzHi8AAABAMbg/5pTvIC1XmHzbE9NNxmreSmgPw4953SFTDbGoDppGa7seuEG8kTOdk1deqayIJ4bE8O7Eu8IlbquAVU+LAQ=="
}
```

Expected: `{"hash":"ab4f721233114b85a2119425c64c87907e7a4b988f7ee0b79e4bc9d81002f2fb"}`

Actual: `{"rejected":"_Exception: Exception: Unknown operation body OperationType.9"}`

### `mainnet/tx_hash 0` on 3.12.0

The SDK refused a value the network accepts: _Exception: Exception: Unknown operation body OperationType.9

```json
{
  "depth_limit": 1000,
  "len_limit": 148,
  "network_passphrase": "Public Global Stellar Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAEdtbmJXSbc1sZf26nS8piOAYsvwzbWym1i/Qw0Zcx4vAAAAZAAAAEAAAAABAAAAAAAAAAAAAAABAAAAAAAAAAkAAAAAAAAAARlzHi8AAABAm738QzgiV0QBrrlJ29nrCWBZNvPll2293iOb2OZ6iy3f3JSsqc1OoC1HuDgo4FF/2t5J6gz4wvZ0fM+ASwFbAg=="
}
```

Expected: `{"hash":"53d2dc73ef881cf38ad42d3a0636e4f2299053c015f4cef678c9b6a224ed629e"}`

Actual: `{"rejected":"_Exception: Exception: Unknown operation body OperationType.9"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`, with the Rust toolchain of its `rust-toolchain.toml` and the toolchain `stellar_flutter_sdk` builds with (`shapekit doctor sdk --sdk dart` lists what is missing):

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit conformance run --sdk dart --version 3.12.0 --entry cap26.inflation.p11-inflation-payout
```

The run builds the SDK's testee at that version as `conformance/versions.toml` describes, then runs the entry's checks.

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap26.inflation/cap26.inflation.p11-inflation-payout/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap26.inflation/cap26.inflation.p11-inflation-payout/mainnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap26.inflation/cap26.inflation.p11-inflation-payout/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap26.inflation/cap26.inflation.p11-inflation-payout/testnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair

How the corpus entry was built and checked: An inflation operation at protocol 11 paying the one account the root voted for; the payout is a balance change on an account the envelope never names

## Sibling issues

- `stellar-dotnet-sdk` (Beans-BV/dotnet-stellar-sdk) diverges on 4 of these entries: shapekit's finding NET-DEPRECATED-OPS, Transaction.FromEnvelopeXdr refuses envelopes with an allow trust or inflation operation (not filed yet).
- `network.lightsail:stellar-sdk` (lightsail-network/java-stellar-sdk) diverges on 4 of these entries: no finding takes it yet.
- `soneso/stellar-php-sdk` (Soneso/stellar-php-sdk) diverges on 4 of these entries: shapekit's finding PHP-INFLATION, AbstractTransaction::fromEnvelopeXdr refuses an envelope with an inflation operation (not filed yet).
- `stellar-sdk` (StellarCN/py-stellar-base) diverges on 4 of these entries: no finding takes it yet.
- `stellar-cli` (stellar/stellar-cli) diverges on 4 of these entries: shapekit's finding CLI-TXHASH-V1, stellar tx hash refuses v0 and fee bump envelopes the network applied (covered upstream by https://github.com/stellar/stellar-cli/issues/2769).

shapekit vector: `cap26.inflation.p11-inflation-payout`
