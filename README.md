# shapekit

[![pr](https://github.com/ErenTopaal/shapekit/actions/workflows/pr.yml/badge.svg?branch=main)](https://github.com/ErenTopaal/shapekit/actions/workflows/pr.yml)
[![nightly](https://github.com/ErenTopaal/shapekit/actions/workflows/nightly.yml/badge.svg?branch=main)](https://github.com/ErenTopaal/shapekit/actions/workflows/nightly.yml)
[![release](https://img.shields.io/github/v/release/ErenTopaal/shapekit)](https://github.com/ErenTopaal/shapekit/releases)
[![license](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)
[![docs](https://img.shields.io/badge/docs-shapekit.org-blue)](https://shapekit.org/docs)

A test kit for software that reads the Stellar ledger: a corpus of rare but valid XDR shapes,
most of them recorded on a real stellar-core, a replay harness that feeds them to an indexer, a conformance
runner that feeds them to the JavaScript, Go and Rust SDKs and to soroban-sdk, the Rust contract
SDK, a linter for Go indexer code, and an indexer-hardening guide.

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
| Generator: recipes recorded on a standalone stellar-core on a Docker network with no route off the host | `crates/shapekit-gen`, `crates/shapekit-core-driver` | [adding a shape](docs/adding-a-shape.md) |
| Replay harness, the wallet-backend reference adapter, and adapters for stellar-etl and stellar-rpc | `crates/shapekit-harness`, `adapters/` | [adapter interface](docs/adapter-interface.md), [replay server](docs/replay-server.md) |
| SDK conformance runner and the testees for the Rust, Go and JavaScript SDKs and soroban-sdk | `crates/shapekit-conformance`, `testees/`, `conformance/` | [testee protocol](docs/testee-protocol.md), [running it in an SDK's CI](docs/sdk-ci.md), [seeded faults](docs/seeded-faults.md) |
| Fuzzing: stellar-xdr against go-stellar-sdk, a coverage-guided search across the Rust, Go and JavaScript SDKs, and each SDK's own fuzzer checked by the encoding laws | `crates/shapekit-fuzz`, `testees/*/fuzz` | [differential fuzzing](docs/differential-fuzzing.md) |
| Linter for Go indexers built on go-stellar-sdk | `lint/` | [indexerlint](docs/linter.md) |
| Indexer-hardening guide, written for developers.stellar.org; a draft until a stellar-core or platform engineer has reviewed it (see the publication gate in [STATUS.md](STATUS.md)) | `docs/guide/` | [the guide](docs/guide/indexer-hardening.mdx) |
| Findings from running the corpus, each with its severity, affected component and corpus entry | `findings/` | [findings report](findings/FINDINGS.md) |
| CI, releases, maintenance | `.github/workflows/` | [infrastructure](docs/infrastructure.md), [maintenance](docs/maintenance.md), [protocol upgrade runbook](docs/protocol-upgrade-runbook.md), [security policy](SECURITY.md) |

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

Run the commands in the [quick start](#quick-start) from the root of a checkout of this
repository at the release tag, with `shapekit` and `indexerlint` on `PATH` and the adapter at
`adapters/bin/wb-adapter`. Their default paths (`corpus/`, `registry/`, `conformance/`,
`testees/`, `pins.toml`) are relative to that root.

### Check your setup

```sh
shapekit doctor                 # everything check sdk, check indexer and lint need
shapekit doctor sdk --sdk js    # only what check sdk --sdk js needs
```

`doctor` looks for every program the SDK build steps in `conformance/versions.toml` start,
compares Rust, Go and Node.js with the versions in `pins.toml`, checks that the Docker daemon
answers, that `indexerlint` and the corpus are there and how much disk is free, and that the
package registries the SDK builds fetch from can be reached (`--offline` skips that). For
each missing piece it prints how to install it, and it exits 1 while anything is missing.

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
`schema/manifest.schema.json`. To browse the entries, with their decoded XDR-JSON and what
each SDK version and the reference indexer did with them, use the
[corpus explorer](https://shapekit.org/dashboard/corpus).

### Run an SDK over the corpus

Needs the toolchain of that SDK (cargo, go or node).

```sh
shapekit check sdk --sdk js                  # current version, @stellar/stellar-sdk 17.4.0
shapekit check sdk --sdk go --all-versions   # every version in conformance/versions.toml
shapekit check sdk                           # every SDK at its current version
```

The divergence matrix is written to `target/conformance/MATRIX.md`. `shapekit check sdk` is
`shapekit conformance run` with the options most runs need; `shapekit conformance run --help`
lists the rest. To run the corpus in an SDK repository's own CI, see
[docs/sdk-ci.md](docs/sdk-ci.md).

### Replay chains into an indexer

Needs Docker. The first run of a wallet-backend commit clones it and builds its image; after that
it runs offline.

```sh
docker pull "$(scripts/pin.sh image timescaledb)"
shapekit check indexer --adapter "adapters/bin/wb-adapter --commit $(scripts/pin.sh git wallet-backend)" \
  --chain cap40.signed-payload-signer.p30
```

Without `--chain` or `--family` every chain in the corpus is replayed. Results are in
`harness-out/HARNESS.md`. `shapekit check indexer` is `shapekit harness run` with the options
most runs need; `shapekit harness run --help` lists the rest.

stellar-rpc runs from its release image, pinned by digest, and serves the chains it backfilled
from their SEP-54 lakes:

```sh
docker pull "$(scripts/pin.sh image stellar-rpc)"
(cd adapters && go build -o bin/rpc-adapter ./stellarrpc/cmd/rpc-adapter)
shapekit check indexer --adapter "adapters/bin/rpc-adapter --events-from events" --network both
```

To put another indexer behind the harness, write an adapter as described in
[docs/adapter-interface.md](docs/adapter-interface.md).

### Read the results

```sh
shapekit report harness-out                          # writes harness-out/report.html
shapekit report target/conformance --known-failures conformance/known-failures
```

The page groups the failures of one run by finding, and gives each group its smallest corpus
entry, its upstream issue, the guide sections for the fields involved and the command that
reruns it. With a baseline it marks failures as new, changed, fixed or unchanged. See
[docs/report.md](docs/report.md).

### Lint Go indexer code

```sh
shapekit lint <your indexer>                                  # indexerlint ./... in that module
shapekit lint <your indexer> --baseline indexerlint-baseline.json --json
```

`shapekit lint` starts `indexerlint`, so the linter binary has to be on `PATH` (or named with
`--linter`), and the Go toolchain too, since the linter loads packages with it. Run on its own,
`indexerlint ./...` does the same. Rules, suppressions and the baseline file are described in
[docs/linter.md](docs/linter.md). The same checks are a checklist in the guide for indexers in
other languages.

### Put your own indexer or SDK behind shapekit

```sh
shapekit init adapter --lang go --target my-indexer --target-version <commit>   # go, ts or python
shapekit init testee --lang python                                              # go, ts or python
```

Each writes a skeleton that speaks the protocol and a README that says what to fill in and how
to run it with `shapekit check indexer` or `shapekit check sdk`. See "Writing an adapter" in
[docs/adapter-interface.md](docs/adapter-interface.md#writing-an-adapter) and "Writing a
testee" in [docs/testee-protocol.md](docs/testee-protocol.md#writing-a-testee).

### Internal commands

`shapekit --help` lists the commands above. The corpus generator, its checks, the upstream
reproductions and the other maintenance tools (`gen`, `twin`, `conformance`, `harness`,
`registry` and the rest) work the same way but are left out of that list; `shapekit dev` lists
them with one line each.

### The guide as a stellar-docs change

`docs/guide/stellar-docs.patch` adds the guide to stellar/stellar-docs as
`docs/data/indexers/build-your-own/indexer-hardening.mdx`, with a link from the Build Your Own
index and an entry in `docs.json`. It is made from the guide in this commit with the
publication-gate comment lines removed and corpus entries linked at the v0.1.0 tag (`shapekit
guide table --corpus-url https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus`), on top
of stellar-docs `main` at `62f29bbb`. The two patched `.mdx` files pass stellar-docs' prettier
configuration and its `scripts/pad-component-tags.mjs --check`; `mint validate` and `mint
broken-links` have not been run on them. To apply the patch and repeat those two checks:

```sh
cd stellar-docs && git switch -c indexer-hardening-guide 62f29bbb
git am /path/to/shapekit/docs/guide/stellar-docs.patch
pnpm install
pnpm exec prettier --config .prettierrc.js --check docs/data/indexers/build-your-own/*.mdx
node scripts/pad-component-tags.mjs --check docs/data/indexers/build-your-own/*.mdx
```

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

# the soroban-sdk testee, outside the workspace, at its current version
m=testees/soroban/versions/29.0.1/Cargo.toml t=target/testees/soroban/29.0.1
cargo clippy --locked --all-targets --manifest-path $m --target-dir $t -- -D warnings
cargo test --locked --manifest-path $m --target-dir $t

# node 26; testees/js/.npmrc sets engine-strict, so npm ci fails on an older node
cd testees/js && npm ci && npx tsc --noEmit && npx tsc && node --test "dist/**/*.test.js"

scripts/check-no-em-dash.sh
```

## Infrastructure and privacy

shapekit collects no telemetry. The few commands that use the network, and everything the
project runs on, are listed in [docs/infrastructure.md](docs/infrastructure.md).

## Maintenance

Maintained by GitHub user `ErenTopaal` through the first protocol upgrade after delivery
(protocol 31). The upgrade runbook is already rehearsed against protocol 31 with core's
next-protocol build; the outputs are in `next/`. The rules for changes, triage and how SDKs pin
corpus releases, and the rehearsal, are in [docs/maintenance.md](docs/maintenance.md).

## License

Apache License 2.0, see [LICENSE](LICENSE) and [NOTICE](NOTICE).
