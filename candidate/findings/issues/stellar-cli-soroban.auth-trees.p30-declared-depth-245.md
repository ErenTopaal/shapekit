<!-- repo: stellar/stellar-cli -->
# stellar-cli 30.0.1: question: stellar tx hash decodes with a depth limit of 500, below the 1000 stellar-core accepts from protocol 28

## What happens

Question: stellar tx hash decodes with a depth limit of 500, below the 1000 stellar-core accepts from protocol 28.

## Cause

Tx_envelope_from_input reads the envelope with Limits::depth(XDR_DEPTH_LIMIT) (soroban-cli 30.0.1 src/commands/tx/xdr.rs L41), and XDR_DEPTH_LIMIT is 500 (src/utils.rs L21; utils.rs L60 re-encodes the signature payload under the same limit), so the hash of a transaction whose authorization tree or ScVal stellar-core accepted can be refused ("failed to decode XDR: depth limit exceeded"). PR #2675 set 500 on purpose against stack overflow; from protocol 28 core's limit is 1000 (TransactionUtils.cpp validateXDRForProtocol). The witnesses are local chains.

## Affected versions

`stellar-cli` 30.0.1 fail; shapekit ran 28.1.0, 30.0.1.

## Entries

shapekit's finding CLI-DEPTH takes 4 corpus entries on this CLI: `soroban.auth-trees.p30-declared-depth-245`, `soroban.auth-trees.p30-declared-depth-246`, `soroban.auth-trees.p30-declared-depth-492`, `soroban.scval-shapes.p30-map-depth-over`. The checks below are those of `soroban.auth-trees.p30-declared-depth-245`.

## Checks on `soroban.auth-trees.p30-declared-depth-245`

On this entry, `stellar-cli` 30.0.1 answers differently from the Stellar network in 2 checks on 1 version.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

A check named `mainnet/...` or `testnet/...` names the network passphrase it hashes or signs with. The data is a ledger stellar-core closed on a private network shapekit ran locally (chain `soroban.auth-trees.p30`), not a transaction or ledger of that network.

Corpus entry `soroban.auth-trees.p30-declared-depth-245` (family `soroban.auth-trees`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `testnet/tx_hash 0` on 30.0.1

The CLI refused a value the network accepts: ❌ error: failed to decode XDR: depth limit exceeded

```json
{
  "depth_limit": 1000,
  "len_limit": 14672,
  "network_passphrase": "Test SDF Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAgAAAAAc/FAM6LNIOWOzSip7lmPnQwHeXh1lk3Gb4TWhSysc6QACCwYAAABEAAAAKQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAFOJwJWLh6vOxL00nqTsD01DTtLe6VVdx6uHHqQsyaZ/QAAAARub2RlAAAAAgAAABAAAAABAAAAAAAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAGZmFub3V0AAAAAAADAAAAAAAAAA8AAAAGc2lnbmVyAAAAAAASAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsAAAAAEAAAABAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsALCbZJzVbG2AAAA0QAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAKcHVibGljX2tleQAAAAAADQAAACDNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDT... (19564 chars)"
}
```

Expected: `{"hash":"890b530034908d74c3d5cb6f0275d12ad58cec409dc1dce5c6b212a8c955cec0"}`

Actual: `{"rejected":"❌ error: failed to decode XDR: depth limit exceeded"}`

### `mainnet/tx_hash 0` on 30.0.1

The CLI refused a value the network accepts: ❌ error: failed to decode XDR: depth limit exceeded

```json
{
  "depth_limit": 1000,
  "len_limit": 14672,
  "network_passphrase": "Public Global Stellar Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAgAAAAAc/FAM6LNIOWOzSip7lmPnQwHeXh1lk3Gb4TWhSysc6QACCwYAAABEAAAAKQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAFxd08LnGZurodrreWHhrpcj7t2sOvbiWLD+3BQqRx/8QAAAARub2RlAAAAAgAAABAAAAABAAAAAAAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAGZmFub3V0AAAAAAADAAAAAAAAAA8AAAAGc2lnbmVyAAAAAAASAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsAAAAAEAAAABAAAAAAAAAADNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDTgjMgsALCbZJzVbG2AAAA0QAAABAAAAABAAAAAQAAABEAAAABAAAAAgAAAA8AAAAKcHVibGljX2tleQAAAAAADQAAACDNa7/tKRGIRLOUq04ODAP1S9zA/M+7uK8ljxDT... (19564 chars)"
}
```

Expected: `{"hash":"52e24f8269ff968937afdff11a52d62194ae00e7b4921ea70d687d73de35de8f"}`

Actual: `{"rejected":"❌ error: failed to decode XDR: depth limit exceeded"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`, with the Rust toolchain of its `rust-toolchain.toml` and the toolchain `stellar-cli` builds with (`shapekit doctor sdk --sdk stellar-cli` lists what is missing):

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit conformance run --sdk stellar-cli --version 30.0.1 --entry soroban.auth-trees.p30-declared-depth-245
```

The run builds the CLI's testee at that version as `conformance/versions.toml` describes, then runs the entry's checks.

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/mainnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/testnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair

How the corpus entry was built and checked: An address entry whose matched root declares a chain 245 invocations deep below it that no call reaches, the deepest the host still decodes; the transaction succeeds and the declared nodes are never checked

Related: https://github.com/stellar/wallet-backend/issues/746

## Sibling issues

- `stellar-dotnet-sdk` (Beans-BV/dotnet-stellar-sdk) diverges on 4 of these entries: shapekit's finding NET-DEPTH, XDR decoders stop at 200 nested types, below stellar-core's limit (filed as https://github.com/Beans-BV/dotnet-stellar-sdk/issues/275).
- `network.lightsail:stellar-sdk` (lightsail-network/java-stellar-sdk) diverges on 4 of these entries: no finding takes it yet.
- `@stellar/stellar-sdk` (stellar/js-stellar-sdk) diverges on 4 of these entries: shapekit's finding JS-16X, 16.x lacks six fixes that 17.x has: map key order and collisions, textual ids, W addresses, decode depth, StrKey checks (filed as https://github.com/stellar/js-stellar-sdk/issues/1907).
- `com.soneso.stellar:stellar-sdk-jvm` (Soneso/kmp-stellar-sdk) diverges on 4 of these entries: shapekit's finding KMP-JSON-DEPTH, XDR-JSON the sdk writes for values nested past 128 containers is refused when the sdk reads it back (filed as https://github.com/Soneso/kmp-stellar-sdk/issues/169).
- `soneso/stellar-php-sdk` (Soneso/stellar-php-sdk) diverges on 3 of these entries: shapekit's finding PHP-JSON-DEPTH, XDR-JSON stops at 512 nested arrays, below stellar-core's limits (filed as https://github.com/Soneso/stellar-php-sdk/issues/243).
- `stellar-sdk` (StellarCN/py-stellar-base) diverges on 4 of these entries: no finding takes it yet.
- `stellar-xdr` (stellar/rs-stellar-xdr) diverges on 4 of these entries: shapekit's finding RS-2, the XDR-JSON reader stops at serde_json's recursion limit of 128, so values the crate writes as JSON cannot be read back (filed as https://github.com/stellar/rs-stellar-xdr/issues/628).

shapekit vector: `soroban.auth-trees.p30-declared-depth-245`
