# shapekit

[![pr](https://github.com/ErenTopaal/shapekit/actions/workflows/pr.yml/badge.svg?branch=main)](https://github.com/ErenTopaal/shapekit/actions/workflows/pr.yml)
[![nightly](https://github.com/ErenTopaal/shapekit/actions/workflows/nightly.yml/badge.svg?branch=main)](https://github.com/ErenTopaal/shapekit/actions/workflows/nightly.yml)
[![release](https://img.shields.io/github/v/release/ErenTopaal/shapekit)](https://github.com/ErenTopaal/shapekit/releases)
[![license](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)
[![docs](https://img.shields.io/badge/docs-shapekit.org-blue)](https://shapekit.org/docs)

A test kit for software that reads the Stellar ledger: a corpus of rare but valid XDR shapes,
most of them recorded on a real stellar-core, a replay harness that feeds them to an indexer, a conformance
runner that feeds them to the JavaScript, Go, Rust, Python, Java, Dart, PHP, .NET and Kotlin Multiplatform SDKs, to
soroban-sdk, the Rust contract SDK, and to the `stellar xdr` commands of stellar-cli, a linter for Go indexer code, and an indexer-hardening guide.

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
| Replay harness, the wallet-backend reference adapter, adapters for stellar-etl and stellar-rpc, and the mapped adapter for PostgreSQL indexers | `crates/shapekit-harness`, `crates/shapekit-mapped`, `adapters/` | [adapter interface](docs/adapter-interface.md), [mapping file](docs/mapping-adapter.md), [replay server](docs/replay-server.md), [running it in an indexer's CI](docs/indexer-ci.md) |
| SDK conformance runner and the testees for the Rust, Go, JavaScript, Python, Java, Dart, PHP, .NET and Kotlin Multiplatform SDKs and soroban-sdk | `crates/shapekit-conformance`, `testees/`, `conformance/` | [testee protocol](docs/testee-protocol.md), [running it in an SDK's CI](docs/sdk-ci.md), [seeded faults](docs/seeded-faults.md) |
| Fuzzing: stellar-xdr against go-stellar-sdk, a coverage-guided search across the Rust, Go and JavaScript SDKs, and each SDK's own fuzzer checked by the encoding laws | `crates/shapekit-fuzz`, `testees/*/fuzz` | [differential fuzzing](docs/differential-fuzzing.md) |
| Linter for Go indexers built on go-stellar-sdk | `lint/` | [indexerlint](docs/linter.md) |
| Indexer-hardening guide, written for developers.stellar.org; a draft until a stellar-core or platform engineer has reviewed it (see the [guide publication gate](STATUS.md#guide-publication-gate)) | `docs/guide/` | [the guide](docs/guide/indexer-hardening.mdx) |
| Findings from running the corpus, each with its severity, affected component and corpus entry | `findings/` | [findings report](findings/FINDINGS.md) |
| CI, releases, maintenance | `.github/workflows/` | [infrastructure](docs/infrastructure.md), [maintenance](docs/maintenance.md), [protocol upgrade runbook](docs/protocol-upgrade-runbook.md), [security policy](SECURITY.md) |

## Install

### From a release

Each release has, for each `<platform>` of `linux-x86_64`, `linux-arm64`, `macos-x86_64` and
`macos-arm64`:

- `shapekit-<platform>`, the CLI;
- `indexerlint-<platform>`, the linter;
- `wb-adapter-<platform>`, the wallet-backend reference adapter for the harness;

and, for every platform:

- `corpus-<version>.tar.zst`, the corpus;
- `goldens-core-<tag>.tar.zst`, stellar-core's `test-lcm-current` goldens the corpus was
  checked against;
- `SHA256SUMS` over all of them.

The current release is [v0.2.0](https://github.com/ErenTopaal/shapekit/releases/tag/v0.2.0),
with corpus 0.2.0. To download and check its Linux x86-64 assets:

```sh
gh release download v0.2.0 --repo ErenTopaal/shapekit \
  --pattern SHA256SUMS --pattern corpus-0.2.0.tar.zst \
  --pattern 'shapekit-linux-x86_64' --pattern 'indexerlint-linux-x86_64' --pattern 'wb-adapter-linux-x86_64'
sha256sum -c --ignore-missing SHA256SUMS      # macOS: shasum -a 256 -c --ignore-missing SHA256SUMS
gh attestation verify shapekit-linux-x86_64 --repo ErenTopaal/shapekit
chmod +x shapekit-linux-x86_64 indexerlint-linux-x86_64 wb-adapter-linux-x86_64
```

Without `gh`, each asset is at
`https://github.com/ErenTopaal/shapekit/releases/download/v0.2.0/<asset>`. The sha256 values of
v0.2.0, as its `SHA256SUMS` lists them:

```text
6fa534d82f6cb46b39311904ff2d6f1233e2b39f71aef391afcdce664fa65c04  corpus-0.2.0.tar.zst
0a11bf9c0d981be29833e133b20f703ef027c8541366aefc246abb3c4310ef6e  goldens-core-v30.0.0.tar.zst
0702a6b66d8bba87a802275f3290de07e24cc685352b8224d89710c79f5b834b  indexerlint-linux-arm64
591504f89a07a20a0305e8766ff3d4fe06c96183652ebc34d4cd8b1ec1b20125  indexerlint-linux-x86_64
c97deb7a80bbac4f8f00178c31efe02a7de4f9545fafdad0ee4fee9f56884202  indexerlint-macos-arm64
f95e8c82210642f53cca6717dbd1d118234757fbdf16bc3af12430dde561fa90  indexerlint-macos-x86_64
cc37401a07cfd3f06de7c9e91ee6a4c5b430d20fc5a336af61f6b1184f1b30e1  shapekit-linux-arm64
9b6c29ba1be088b76678777523b1cbcd17fc2c6194c6306e6dda33e890b6484e  shapekit-linux-x86_64
4e4ee6db8d89e713d8dd87f996056b97a63a4a7b431398b7846011d71ede0055  shapekit-macos-arm64
11184dce82ce5c1a4d5ab35b76108f338ae9a71e30fdd5b0cc2737d3049786c8  shapekit-macos-x86_64
1a310137739e86e7e2c5789006c823f68d57ce766d4f5dc93fea87b5f01179bd  wb-adapter-linux-arm64
03a8407e6ead2aa334ff70d0e4ccb9597b8feeb8256771a03f3b8023076e58cc  wb-adapter-linux-x86_64
548e5b7ca74daf264d0f1821ecb61f818faef5b4086a4952acbadbb9f69e0e14  wb-adapter-macos-arm64
ba23828f768078d6969d77ae2eda6440dc3f4f9f657d555665993912d7c45713  wb-adapter-macos-x86_64
```

Each platform's binaries are built on a GitHub runner of that platform. The project's own CI
runs only the Linux x86-64 build; release builds for the other platforms run `--version` and
nothing more before they are published. A binary downloaded with a browser on macOS carries the quarantine attribute, and
macOS refuses to start an unsigned binary that has it; `xattr -d com.apple.quarantine
<file>` removes it. Downloads with `gh release download` or `curl` do not get it.

### From source

Toolchains are pinned: Rust 1.99.1 (`rust-toolchain.toml`), Go 1.27.4, Node.js 26.13.1,
Flutter 3.50.0 for the Dart testee, PHP 8.5.12 with Composer 2.11.2 for the PHP testee, and the .NET SDK 11.0.100 for the .NET
testee.

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
[corpus entries](https://shapekit.org/dashboard/corpus/entries).

### Run an SDK over the corpus

Each SDK needs its own toolchain: cargo for rust, soroban and stellar-cli, go, node for js, uv
with CPython 3.15.2 for python, JDK 27 and Maven for java and kmp, Flutter for dart, PHP and
Composer for php, and the .NET SDK for dotnet. `shapekit doctor sdk --sdk <sdk>` lists what is
missing. For python, java and kmp, `export PATH="$(scripts/python-toolchain.sh):$PATH"` and
`export PATH="$(scripts/java-toolchain.sh):$PATH"` fetch the pinned versions into
`target/toolchains/` and check each download against its sha256 (the java and kmp builds set
`JAVA_HOME` to the pinned JDK themselves); for dart,
`export PATH="$(scripts/flutter-toolchain.sh):$PATH"` does the same for Flutter.

```sh
shapekit check sdk --sdk js                  # current version, @stellar/stellar-sdk 17.4.0
shapekit check sdk --sdk go --all-versions   # every version in conformance/versions.toml
shapekit check sdk                           # every SDK at its current version
```

The divergence matrix is written to `target/conformance/MATRIX.md`. `shapekit check sdk` is
`shapekit conformance run` with the options most runs need; `shapekit conformance run --help`
lists the rest. To run the corpus in an SDK repository's own CI against the pull request's
code, for any of the eleven SDKs above, see [docs/sdk-ci.md](docs/sdk-ci.md).

### Replay chains into an indexer

Needs Linux and Docker. The harness binds the replay server to the gateway of an internal
Docker network, and the adapter connects to its containers' addresses. On macOS, Docker runs
in a VM and neither is reachable from the host, so `check indexer` and `harness run` stop with
that error there. The CI workflows run on Linux runners. The first run of a wallet-backend
commit clones it and builds its image; after that it runs offline.

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

An indexer that runs in Docker and stores in PostgreSQL connects through a mapping file, with no
adapter code: the file says how to build and start it, how ledgers reach it, and which tables
and columns hold which facts. wallet-backend's is `adapters/walletbackend/adapter.toml`:

```sh
shapekit check indexer --mapping adapters/walletbackend/adapter.toml \
  --var commit=$(scripts/pin.sh git wallet-backend) --chain cap40.signed-payload-signer.p30
```

[docs/mapping-adapter.md](docs/mapping-adapter.md) describes the file.

stellar-etl is built from its pinned release tag on first use, like wallet-backend, and reads
the chains from a GCS emulator:

```sh
docker pull "$(scripts/pin.sh image fake-gcs-server)"
(cd adapters && go build -o bin/etl-adapter ./stellaretl/cmd/etl-adapter)
shapekit check indexer --adapter "adapters/bin/etl-adapter --commit $(scripts/pin.sh git stellar-etl)" \
  --chain cap40.signed-payload-signer.p30
```

For any other indexer, write an adapter as described in
[docs/adapter-interface.md](docs/adapter-interface.md). To replay the corpus in an indexer
repository's own CI, see [docs/indexer-ci.md](docs/indexer-ci.md).

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
shapekit lint <your indexer> --baseline <baseline file> --json
```

`shapekit lint` starts `indexerlint`, so the linter binary has to be on `PATH` (or named with
`--linter`), and the Go toolchain too, since the linter loads packages with it. Run on its own,
`indexerlint ./...` does the same. A relative `--baseline` path is read from the directory you
run `shapekit lint` in, not from the indexer's directory. Rules, suppressions and the baseline file are described in
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
publication-gate comment lines removed and corpus entries linked at the v0.2.0 tag (`shapekit
guide table --corpus-url https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus`), on top
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

To make the patch again after the guide changes, continue on that branch with `SK` set to the
root of this repository:

```sh
f=docs/data/indexers/build-your-own/indexer-hardening.mdx
sed '2,5d' "$SK/docs/guide/indexer-hardening.mdx" > "$f"    # drops the publication-gate comment
(cd "$SK" && shapekit guide table --file "$OLDPWD/$f" \
  --corpus-url https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus)
git commit -a --amend --no-edit
git format-patch -1 --stdout > "$SK/docs/guide/stellar-docs.patch"
```

`shapekit guide table` runs from the repository root because it reads `registry/` and
`corpus/manifest.json` from there.

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

Maintained by [Eren Topal](https://github.com/ErenTopaal) through the first protocol upgrade after delivery
(protocol 31). The upgrade runbook is already rehearsed against protocol 31 with core's
next-protocol build; the outputs are in `next/`. The rules for changes, triage and how SDKs pin
corpus releases, and the rehearsal, are in [docs/maintenance.md](docs/maintenance.md).

## License

Apache License 2.0, see [LICENSE](LICENSE) and [NOTICE](NOTICE).
