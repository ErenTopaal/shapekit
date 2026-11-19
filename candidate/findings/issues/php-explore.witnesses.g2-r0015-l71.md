<!-- repo: Soneso/stellar-php-sdk -->
# soneso/stellar-php-sdk 1.18.0: a set options operation with clear_flags 0, set_flags 0 or a home domain of "" or "0" hashes to another value once the sdk rebuilds it

## What happens

A set options operation with clear_flags 0, set_flags 0 or a home domain of "" or "0" hashes to another value once the sdk rebuilds it.

## Cause

AbstractTransaction::fromEnvelopeXdr(..)->hash(network) rebuilds the operation, and SetOptionsOperation::toOperationBody writes inflationDestination, clearFlags, setFlags and homeDomain only when they are truthy (Soneso/StellarSDK/SetOptionsOperation.php L200-L224 at 1.18.0, `if ($this->clearFlags)`), so a present 0, an empty home domain and the one-character home domain "0", which php also treats as false, become absent and the sdk hashes other bytes than the network. #15 fixed the same pattern for the weights and thresholds in 2022.

## Affected versions

`soneso/stellar-php-sdk` 1.18.0, every version shapekit ran.

## Entries

shapekit's finding PHP-SETOPTIONS takes 2 corpus entries on this SDK: `explore.witnesses.f62d-r0007-l77`, `explore.witnesses.g2-r0015-l71`. The checks below are those of `explore.witnesses.g2-r0015-l71`.

## Checks on `explore.witnesses.g2-r0015-l71`

On this entry, `soneso/stellar-php-sdk` 1.18.0 answers differently from the Stellar network in 2 checks on 1 version.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

A check named `mainnet/...` or `testnet/...` names the network passphrase it hashes or signs with. The data is a ledger stellar-core closed on a private network shapekit ran locally (chain `explore.g2.r0015`), not a transaction or ledger of that network.

Corpus entry `explore.witnesses.g2-r0015-l71` (family `explore.witnesses`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `testnet/tx_hash 0` on 1.18.0

The SDK's hash differs from the one the network wrote

```json
{
  "depth_limit": 1000,
  "len_limit": 356,
  "network_passphrase": "Test SDF Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAAHPfPMKl1Cf3tp/iWY9jfYse3nBlX2pVa6t5lRCid8KAAABLAAAAEAAAAADAAAAAAAAAAEAAAAAAAAAAwAAAAAAAAAHAAAAAO17/F0lOid71HUBq+lVCPZUN9QVqoKjhSYSrWDp8dWHAAAAAVVTRAAAAAABAAAAAQAAAAABz3zzCpdQn97af4lmPY32LHt5wZV9qVWureZUQonfCgAAAAcAAAAAAc988wqXUJ/e2n+JZj2N9ix7ecGVfalVrq3mVEKJ3woAAAACTE9OR0FTU0VUMTIAAAAAAQAAAAAAAAAFAAAAAAAAAAAAAAAAAAAAAQAAAP8AAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAFCid8KAAAAQKbfMCOkZBIfMlQ7DtRjigHN6N8+p+cWMS403kE6GGRcBHbJ8GfPxGEX+mdqdMCXdvPriDjkMsFMsoYvnAVURAQ="
}
```

Expected: `{"hash":"f191a7d5f93a78d79b3cdf0631da286c8a54f7b38400cde9290a6865929e695c"}`

Actual: `{"hash":"80cf2c97022222b22f29d78d9fcbc3a38ae671f7263658cadd493fc499432a01"}`

### `mainnet/tx_hash 0` on 1.18.0

The SDK's hash differs from the one the network wrote

```json
{
  "depth_limit": 1000,
  "len_limit": 356,
  "network_passphrase": "Public Global Stellar Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAAHPfPMKl1Cf3tp/iWY9jfYse3nBlX2pVa6t5lRCid8KAAABLAAAAEAAAAADAAAAAAAAAAEAAAAAAAAAAwAAAAAAAAAHAAAAAO17/F0lOid71HUBq+lVCPZUN9QVqoKjhSYSrWDp8dWHAAAAAVVTRAAAAAABAAAAAQAAAAABz3zzCpdQn97af4lmPY32LHt5wZV9qVWureZUQonfCgAAAAcAAAAAAc988wqXUJ/e2n+JZj2N9ix7ecGVfalVrq3mVEKJ3woAAAACTE9OR0FTU0VUMTIAAAAAAQAAAAAAAAAFAAAAAAAAAAAAAAAAAAAAAQAAAP8AAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAFCid8KAAAAQBQUx4YLt7VNTdjJGkgfnUnVG1sJAYtoRwFiUHks7T5ZAckxl5CA8vQ/DfFIb8vDMb6blKeL3zqmxMiRzao3uAk="
}
```

Expected: `{"hash":"855d3afef6c40c225c4f1149ebf34a1f8888b1c1da7a69dcaf94b32e2dd8c93d"}`

Actual: `{"hash":"35ec280266647d0c53d75198cb4fec37dc19a8b225e709d7cecb9d552aa54615"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`, with the Rust toolchain of its `rust-toolchain.toml` and the toolchain `soneso/stellar-php-sdk` builds with (`shapekit doctor sdk --sdk php` lists what is missing):

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit conformance run --sdk php --version 1.18.0 --entry explore.witnesses.g2-r0015-l71
```

The run builds the SDK's testee at that version as `conformance/versions.toml` describes, then runs the entry's checks.

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/explore.witnesses/explore.witnesses.g2-r0015-l71/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/explore.witnesses/explore.witnesses.g2-r0015-l71/mainnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/explore.witnesses/explore.witnesses.g2-r0015-l71/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/explore.witnesses/explore.witnesses.g2-r0015-l71/testnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair

How the corpus entry was built and checked: A classic recipe of the explorer's run g2 (seed 202). ledger 71 of chain explore.g2.r0015 at protocol 10, recorded from corpus/recipes/explore.witnesses/g2/r0015.json; picked for the arms it holds that no earlier pick holds: AllowTrustResult:self_not_allowed, AllowTrustResult:trust_not_required

## Sibling issues

- `stellar-dotnet-sdk` (Beans-BV/dotnet-stellar-sdk) diverges on `explore.witnesses.g2-r0015-l71`, one of these entries: shapekit's finding NET-DEPRECATED-OPS, Transaction.FromEnvelopeXdr refuses envelopes with an allow trust or inflation operation (not filed yet).
- `network.lightsail:stellar-sdk` (lightsail-network/java-stellar-sdk) diverges on 2 of these entries: no finding takes it yet.
- `stellar-sdk` (StellarCN/py-stellar-base) diverges on 2 of these entries: no finding takes it yet.
- `stellar-cli` (stellar/stellar-cli) diverges on 2 of these entries: shapekit's finding CLI-TXHASH-V1, stellar tx hash refuses v0 and fee bump envelopes the network applied (covered upstream by https://github.com/stellar/stellar-cli/issues/2769).

shapekit vector: `explore.witnesses.g2-r0015-l71`
