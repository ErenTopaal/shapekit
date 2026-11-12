# shapekit

A test kit for software that reads the Stellar ledger: a corpus of rare but valid XDR shapes
produced by a real stellar-core, a replay harness that feeds them to an indexer, a conformance
runner that feeds them to the JavaScript, Go and Rust SDKs, a linter for Go indexer code, and an
indexer-hardening guide.

Indexers and SDKs keep breaking on shapes that are valid but rare: signed-payload signers, muxed
accounts, deep Soroban authorization trees, trustlines created by the Stellar Asset Contract,
maps whose keys collide after conversion. Each team finds the same bugs on its own. shapekit is
the shared set of those shapes plus the tools to run them. It is built in response to the SDF
RFP "XDR/CAP-Coverage Audit Tool and Indexer-Hardening Guide", and covers protocol 30
(stellar-core 30.0.0, stellar-xdr v30.0).

The dated log of what has landed is [STATUS.md](STATUS.md).

## What is in it

| Part | Where | Documentation |
|---|---|---|
| Corpus: XDR files plus a language-neutral manifest with expected decoded values, generated under both the testnet and mainnet passphrases, checked against stellar-core's `--check-lcm` goldens | `corpus/` | [coverage per CAP](corpus/coverage/COVERAGE.md), [adding a shape](docs/adding-a-shape.md) |
| Generator: recipes recorded on a private standalone stellar-core | `crates/shapekit-gen`, `crates/shapekit-core-driver` | [adding a shape](docs/adding-a-shape.md) |
| Replay harness and the wallet-backend reference adapter | `crates/shapekit-harness`, `adapters/` | [adapter interface](docs/adapter-interface.md) |
| SDK conformance runner and the Rust, Go and JavaScript testees | `crates/shapekit-conformance`, `testees/`, `conformance/` | [testee protocol](docs/testee-protocol.md), [running it in an SDK's CI](docs/sdk-ci.md) |
| Linter for Go indexers built on go-stellar-sdk | `lint/` | [indexerlint](docs/linter.md) |
| Indexer-hardening guide, written for developers.stellar.org | `docs/guide/` | [the guide](docs/guide/indexer-hardening.mdx) |
| Findings from running the corpus | `findings/` | |
| CI, releases, maintenance | `.github/workflows/` | [infrastructure](docs/infrastructure.md), [maintenance](docs/maintenance.md), [protocol upgrade runbook](docs/protocol-upgrade-runbook.md) |

## Install

### From a release

Each release has, for linux x86-64:

- `shapekit-linux-x86_64`, the CLI;
- `indexerlint-linux-x86_64`, the linter;
- `wb-adapter-linux-x86_64`, the wallet-backend reference adapter for the harness;
- `corpus-<version>.tar.zst`, the corpus;
- `goldens-core-<tag>.tar.zst`, stellar-core's `test-lcm-current` goldens the corpus was
  checked against;
- `SHA256SUMS` over all of them.

```sh
sha256sum -c SHA256SUMS
gh attestation verify shapekit-linux-x86_64 --repo ErenTopaal/shapekit
chmod +x shapekit-linux-x86_64 indexerlint-linux-x86_64 wb-adapter-linux-x86_64
```

### From source

Toolchains are pinned: Rust 1.99.1 (`rust-toolchain.toml`), Go 1.27.4, Node.js 26.13.1.

```sh
cargo build --release --locked -p shapekit-cli     # target/release/shapekit
(cd lint && go build -o indexerlint ./cmd/indexerlint)
(cd adapters && go build -o bin/wb-adapter ./walletbackend/cmd/wb-adapter)
```

Commands below run from the root of a checkout of this repository at the release tag, with the
binaries on `PATH` as `shapekit`, `indexerlint` and `adapters/bin/wb-adapter`: their default paths
(`corpus/`, `registry/`, `conformance/`, `testees/`, `pins.toml`) are relative to it.

## Quick start

### Check the corpus

```sh
shapekit registry check
shapekit manifest validate      # every entry decodes and re-encodes to the same bytes
shapekit coverage --check       # per-CAP coverage is current and every gap has a reason
shapekit goldens agree          # structural agreement with stellar-core's goldens
```

Any pipeline can read the corpus without shapekit: `corpus/manifest.json` lists every entry with
its XDR type, files, CAPs, protocol and expected values; the schema is
`schema/manifest.schema.json`.

### Run an SDK over the corpus

Needs the toolchain of that SDK (cargo, go or node).

```sh
shapekit conformance run --sdk js                  # current version, @stellar/stellar-sdk 17.4.0
shapekit conformance run --sdk go --all-versions   # every version in conformance/versions.toml
shapekit conformance reproduce                     # the upstream sightings fail before and pass after their fixes
```

The divergence matrix is written to `target/conformance/MATRIX.md`. To run the corpus in an SDK
repository's own CI, see [docs/sdk-ci.md](docs/sdk-ci.md).

### Replay chains into an indexer

Needs Docker. The first run of a wallet-backend commit clones it and builds its image; after that
it runs offline.

```sh
docker pull "$(scripts/pin.sh image timescaledb)"
shapekit harness run --adapter "adapters/bin/wb-adapter --commit $(scripts/pin.sh git wallet-backend)" \
  --chain cap40.signed-payload-signer.p30 --network testnet
shapekit harness reproduce --table adapters/walletbackend/reproductions.toml
```

Results are in `harness-out/HARNESS.md`. To put another indexer behind the harness, write an
adapter as described in [docs/adapter-interface.md](docs/adapter-interface.md).

### Lint Go indexer code

```sh
cd <your indexer> && indexerlint ./...
```

Rules, suppressions and the baseline file are described in [docs/linter.md](docs/linter.md).
The same checks are a checklist in the guide for indexers in other languages.

### Regenerate the corpus

Needs Docker and the pinned stellar-core image.

```sh
docker pull "$(scripts/pin.sh image stellar-core)"
shapekit gen verify-repro --family cap40.signed-payload-signer   # regenerate under both passphrases and compare byte for byte
shapekit gen chains --family cap40.signed-payload-signer --network both
```

## Building and checking

The same commands CI runs on every pull request (see [docs/infrastructure.md](docs/infrastructure.md)
for the full list and their measured durations):

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# in each module listed in go.work; adapters/ needs SHAPEKIT_BIN or target/debug/shapekit
go vet ./... && go test ./... && golangci-lint run ./...

cd testees/js && npm ci && npx tsc --noEmit && npx tsc && node --test "dist/**/*.test.js"

scripts/check-no-em-dash.sh
```

## Infrastructure and privacy

shapekit collects no telemetry. The few commands that use the network, and everything the
project runs on, are listed in [docs/infrastructure.md](docs/infrastructure.md).

## Maintenance

Maintained by `ErenTopaal` through the first protocol upgrade after delivery (protocol 31). The
rules for changes, triage and how SDKs pin corpus releases are in
[docs/maintenance.md](docs/maintenance.md).

## License

Apache License 2.0, see [LICENSE](LICENSE) and [NOTICE](NOTICE).
