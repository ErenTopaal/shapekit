<!-- repo: Beans-BV/dotnet-stellar-sdk -->
# stellar-dotnet-sdk 17.0.0: Transaction.FromEnvelopeXdr refuses an ScVal vec whose body is absent

## What happens

Transaction.FromEnvelopeXdr refuses an ScVal vec whose body is absent.

## Cause

SCVal.FromXdr refuses SCV_VEC with an absent body ("SCV_VEC with an absent vector body", StellarDotnetSdk/Soroban/SCVal.cs L1753), which a contract's arguments can carry in an envelope core applied, so the transaction object and its hash cannot be built.

## Affected versions

`stellar-dotnet-sdk` 17.0.0 fail; shapekit ran 16.0.0, 17.0.0.

## Entries

shapekit's finding NET-SCVEC takes 1 corpus entry on this SDK: `soroban.auth-trees.golden-5f615b0f98ae09f3-11`. The checks below are those of `soroban.auth-trees.golden-5f615b0f98ae09f3-11`.

## Checks on `soroban.auth-trees.golden-5f615b0f98ae09f3-11`

On this entry, `stellar-dotnet-sdk` 17.0.0 answers differently from the Stellar network in 1 check on 1 version.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

Corpus entry `soroban.auth-trees.golden-5f615b0f98ae09f3-11` (family `soroban.auth-trees`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `any/tx_hash 0` on 17.0.0

The SDK refused a value the network accepts: ArgumentException: SCV_VEC with an absent vector body (Parameter 'xdrVal')

```json
{
  "depth_limit": 1000,
  "len_limit": 768,
  "network_passphrase": "(V) (;,,;) (V)",
  "op": "tx_hash",
  "xdr": "AAAAAgAAAAC4Qtwt8EhGjBf7w268yEIqfaGsHtxHwBK7lmcetX/yRgAehOQAAAAAAAAACgAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEBuCkP1Jtb0TMPDqcYvepnKTIGIPZc4adjZXxXY4xYCwAAAAd0cmVlX2ZuAAAAAAIAAAAQAAAAAQAAAAEAAAASAAAAAAAAAACJw5GqzvKDwxypMCrfuSdW0aG7KfjD5FYqrwyaCHGA7wAAABEAAAABAAAABAAAAA8AAAAIY2hpbGRyZW4AAAAQAAAAAQAAAAAAAAAPAAAACGNvbnRyYWN0AAAAEgAAAAEBuCkP1Jtb0TMPDqcYvepnKTIGIPZc4adjZXxXY4xYCwAAAA8AAAAJbmVlZF9hdXRoAAAAAAAAEAAAAAEAAAABAAAAAAAAAAEAAAAPAAAACHRyeV9jYWxsAAAAAAAAAAAAAAABAAAAAQAAAAAAAAAAicORqs7yg8McqTAq37knVtGhuyn4w+RW... (1024 chars)"
}
```

Expected: `{"hash":"a565296bb051df841d85c9c47e2f7c5e94724012da1522f354d6c7eaac724938"}`

Actual: `{"rejected":"ArgumentException: SCV_VEC with an absent vector body (Parameter 'xdrVal')"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`, with the Rust toolchain of its `rust-toolchain.toml` and the toolchain `stellar-dotnet-sdk` builds with (`shapekit doctor sdk --sdk dotnet` lists what is missing):

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit conformance run --sdk dotnet --version 17.0.0 --entry soroban.auth-trees.golden-5f615b0f98ae09f3-11
```

The run builds the SDK's testee at that version as `conformance/versions.toml` describes, then runs the entry's checks.

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.golden-5f615b0f98ae09f3-11/any.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.golden-5f615b0f98ae09f3-11/any.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair

How the corpus entry was built and checked: Core test "Soroban classic account authentication", section "account with weights|uninitialized vector signature", ledger 13 (record 12 of 12 in the upstream file). smallest golden ledger matching SorobanCredentials:address

## Sibling issues

- `network.lightsail:stellar-sdk` (lightsail-network/java-stellar-sdk) diverges on the same entry: no finding takes it yet.
- `stellar-sdk` (StellarCN/py-stellar-base) diverges on the same entry: no finding takes it yet.

shapekit vector: `soroban.auth-trees.golden-5f615b0f98ae09f3-11`
