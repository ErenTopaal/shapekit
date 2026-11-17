# Infrastructure

Everything shapekit's deliverables run on, what they talk to, and what they record about you
(nothing). Versions are the ones pinned in `pins.toml`; `shapekit pins verify` checks every
pin against its upstream source and `upstream-drift.yml` re-checks them weekly.

## Where it runs

All CI runs on GitHub-hosted `ubuntu-24.04` runners. There are no self-hosted runners, servers,
databases, hosted services or other build systems. Releases are GitHub releases of this
repository. Nothing runs between CI jobs.

You run the tools on your own machine or CI runner. The released binaries are built for Linux and macOS,
each on x86-64 and arm64; `check indexer` and `harness` need Linux, since they read the
process's memory from `/proc`. The commands that start containers (`gen`, `harness`, the `check-lcm` script) need
Docker.

## Toolchains

| Tool | Version | Pinned in |
|---|---|---|
| Rust | 1.99.1 | `rust-toolchain.toml`, `pins.toml` (with the channel manifest sha256), `dtolnay/rust-toolchain@1.99.1` |
| Go | 1.27.4 | `go.work`, each `go.mod`, `pins.toml` (with the archive sha256), `actions/setup-go` |
| Node.js | 26.13.1 | `actions/setup-node`, `pins.toml` (with the archive sha256); `testees/js/package.json` requires `>=26` |
| TypeScript | 7.0.2 | `testees/js/package.json` and its lock file |
| golangci-lint | v2.15.0 | `pr.yml`, `pins.toml` |
| stellar-cli | 30.0.1 (commit `f2a7b741`) | `pins.toml` (`[[release]]`: the `stellar-cli-30.0.1-x86_64-unknown-linux-gnu.tar.gz` release asset and its sha256), installed by `scripts/fetch-stellar-cli.sh` |
| Docker | not pinned | the engine and compose plugin of the `ubuntu-24.04` runner image |

## Stellar stack

| Component | Version | Used for |
|---|---|---|
| stellar-core | 30.0.0, `stellar/stellar-core:30.0.0-3642.0e49751aa.noble` by digest `sha256:eb7a7c81a29b...` | standalone core that records every chain |
| stellar-core test build | `stellar/unsafe-stellar-core:30.0.0-3642.0e49751aa.noble-buildtests` by digest `sha256:a8cb428ce497...` | only core's own `test --check-lcm` run in `nightly.yml` |
| stellar-core source | tag `v30.0.0` (commit `0e49751a`) | the `test-lcm-current` goldens |
| XDR definitions | stellar-xdr tag `v30.0` (commit `4f524bba`) | the protocol 30 XDR definitions; `upstream-drift.yml` watches for a newer tag |
| `stellar-xdr` crate | 30.0.0 (28.0.0 and 28.0.1 in the Rust testee matrix) | decoding, round trips, the Rust testee |
| `stellar-strkey` | 1.0.0 | strkey encoding in the Rust testee |
| `soroban-env-host` / `soroban-simulation` | 30.0.0, plus renamed 22.1.4, 24.0.0 and 28.0.2 for older protocols | transaction simulation in the generator |
| `soroban-sdk` | 29.0.1 (22.0.11, 23.5.3 and 28.0.0 for the legacy contracts) | test contracts in `contracts/` |
| `@stellar/stellar-sdk` | 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1, 17.3.0, 17.4.0 (current) | JavaScript testee matrix |
| `github.com/stellar/go-stellar-sdk` | v0.7.1, v0.7.2, v0.7.3, v0.8.0, v0.9.0 (current) | Go testee matrix (all), the wallet-backend adapter and the linter's analysis target (v0.8.0 only) |
| stellar-rpc | tag `v30.0.1` (commit `6b6ec60c`, image `stellar/stellar-rpc:30.0.1` `sha256:b6d41d314808...`) | the JSON-RPC behaviour `shapekit serve` follows ([replay-server.md](replay-server.md)), and the image the stellar-rpc adapter runs as a harness target |
| stellar/wallet-backend | commit `3e22515a` (current, `main` on 2026-11-13), plus the pre-fix and fixed commits in `adapters/walletbackend/reproductions.toml` (`e967ce8b`, the previous current pin, is the fixed side of #716) | reference harness target |

On 2026-11-14 each of these is the newest stable upstream release: `scripts/upstream-releases.sh
--dry-run` reports stellar-core, the XDR definitions, the `stellar-xdr` crate,
`@stellar/stellar-sdk`, go-stellar-sdk and stellar-cli at their newest releases, and crates.io,
the GitHub releases of stellar-rpc and the `main` branch of wallet-backend give the other
versions.

The test contracts in `contracts/` are built with `stellar contract build` from the official
stellar-cli 30.0.1 release binary (`contracts/build.sh`, and `contracts/build-legacy.sh` for the
older soroban-sdk builds). `scripts/fetch-stellar-cli.sh` downloads the pinned release asset,
refuses it unless its sha256 matches `pins.toml`, and installs `stellar` under
`target/tools/stellar-cli-30.0.1/`; `shapekit pins verify` checks that the tag still names commit
`f2a7b741` and that GitHub reports the same sha256 for the asset. The asset also carries a GitHub
build attestation from stellar-cli's release workflow (`gh attestation verify <asset> --repo
stellar/stellar-cli`). Besides compiling, `stellar contract build` adds a `cliver` meta entry
naming its version and commit, removes unused spec entries (soroban-sdk 28 and later) and runs
wasm-opt, so the committed wasm depends on the stellar-cli release; `pr.yml` rebuilds the
contracts and fails if the bytes differ from the committed ones.

The full lists (every crate, npm package and Go module with its purpose) are in `pins.toml`.

## Container images

All pulled by digest, never by tag; `scripts/pin.sh image <name>` prints the reference.

| Image | Tag | Used by |
|---|---|---|
| `stellar/stellar-core` | `30.0.0-3642.0e49751aa.noble` | `shapekit gen`, the docker-backed tests |
| `stellar/unsafe-stellar-core` | `30.0.0-3642.0e49751aa.noble-buildtests` | `scripts/check-lcm.sh` |
| `timescale/timescaledb` | `2.28.2-pg17` | wallet-backend's database in the reference adapter |
| `chrislusf/seaweedfs` | `4.46` | the S3 store for the adapter's `sep54-s3` delivery mode |

The adapter also builds `shapekit-wallet-backend:<commit>` from wallet-backend's own
`Dockerfile` at the requested commit. That Dockerfile names its base images by tag; the adapter
replaces each `FROM` with the digest pinned here, through `docker buildx build
--build-context <tag>=docker-image://<repository>@<digest>`, and refuses to build a commit whose
Dockerfile names a base it has no pin for (`BaseImages` in `adapters/walletbackend/docker.go`,
checked against `pins.toml` by its tests):

| Image | Tag | `FROM` in wallet-backend's Dockerfile |
|---|---|---|
| `golang` | `1.25.9-bookworm` | build stage, every pinned commit |
| `debian` | `bookworm-slim` | runtime stage, the current pin and the newer reproduction commits |
| `ubuntu` | `jammy` | runtime stage, the older reproduction commits |

The packages the Dockerfile installs with `apt-get` and `go install` are not pinned by this
repository; they come from wallet-backend's Dockerfile as it is. `pr.yml` and the nightly
harness cache the adapter's clone and the built image (`docker save`) with `actions/cache`,
keyed by the wallet-backend commit and the base image digests, so a run with a cache hit builds
nothing and fetches nothing for wallet-backend.

Containers are isolated from the network: the core driver runs stellar-core on a Docker network
created with `--internal` and refuses to start otherwise, with all capabilities dropped; the
harness creates an `--internal` network for the target and binds the replay server to that
network's gateway address.

## CI

| Workflow | Trigger | What it does |
|---|---|---|
| `pr.yml` | every pull request, pushes to `main` | Release build of the CLI shared by the other jobs. Rust: rustfmt, clippy with warnings denied, `cargo test`. Go: `go vet`, `go test`, golangci-lint per module, and `indexerlint` over `adapters/`. JavaScript testee: `npm ci`, typecheck, build, `node --test`. Contracts: `contracts/build.sh` and `contracts/build-legacy.sh` with the pinned stellar-cli, compared byte for byte with the committed wasm. Corpus: registry check, manifest validation (round trip of every entry), synthetic entries and manifest regenerated and compared with the committed files, `oracle run` over the whole corpus compared with the committed oracle files, `coverage --check`, `guide check`, `goldens agree`, `pack verify`, adapter schema check, the release archive builds, no em dashes. `gen verify-repro` under both passphrases for a rotating window of four families plus every family the change touches (every family when shared generator code, the registry, the contracts or the pins change), sharded over up to four runners; a corpus step checks that the selection script's family list is exactly the set of recipe families the CLI registers. `conformance run` on rust 30.0.0, go v0.9.0 and js 17.4.0. soroban-sdk testee: rustfmt, clippy with warnings denied, its unit tests and `conformance run` on soroban-sdk 29.0.1. `harness run` with the wallet-backend adapter at the current pin, testnet variant, on three chains |
| `nightly.yml` | daily 03:00 UTC, and by hand | `goldens verify` and core's own `--check-lcm` run against the pinned goldens; `oracle run` over the whole corpus compared with the committed oracle files; `gen verify-repro` with no family filter, so every chain of every registered recipe, under both passphrases; `cargo test` with the docker-backed tests and the `shapekit serve` memory bound; `conformance run --all-versions` per SDK, the same run across the Rust, Go and JavaScript SDKs with `--flag references-disagree`, and `conformance reproduce`; `shapekit fuzz` seeded with the date, and a replay of the stored reproducers in `findings/fuzz/` ([differential fuzzing](differential-fuzzing.md)); `harness run` over every chain in both delivery modes (rpc on both networks run twice in one process with the memory growth check, sep54-s3 on testnet); `harness reproduce` of the wallet-backend sightings; coverage; `findings check` over the night's conformance matrices, harness report and an `indexerlint` run on wallet-backend at the pin, failing on any problem other than findings still in status draft |
| `audit.yml` | daily 04:40 UTC, and by hand (inputs `seed`, `n`) | Draws 3,000 distinct ledgers from the whole pubnet history in the public SEP-54 data lake, with the date (yyyymmdd) as the seed, which one job sets for all the others. `laws`: `shapekit audit run --network pubnet --n 3000 --seed <seed>` checks every law of the law book on each ledger. `sdk`, once per SDK (rust, go, js), with `--sdk <sdk> --version current`: every ledger goes through the testee of that SDK's current version. Each ledger gets `decode_encode`, each envelope gets `tx_hash` (fee bump inners too), and `balance_id` and `contract_id` run wherever the ledger holds them. Each answer is compared with what the network wrote. Every job writes its report to the job summary (`shapekit audit summary`) and keeps it as an artifact for 90 days. A ledger the lake does not return fails the job; it is never skipped |
| `sentinel.yml` | hourly at minute 17, and by hand (inputs `pubnet_start`, `testnet_start`) | For pubnet and testnet: `shapekit audit watch` checks every ledger closed after the cursor stored on the `sentinel-state` branch (`pubnet.json`, `testnet.json`), oldest first and at most 1,500 per run, then commits the new cursor to that branch. Runs per network are serialized, so consecutive runs neither skip nor repeat a ledger, and the header chain is checked across runs. A cursor left behind by runs that did not happen catches up over the following runs; the job summary lists the exact ledger ranges checked, how many ledgers were behind the newest, how many this run checked and how many are left. A missing cursor fails the run: nothing is skipped unless the workflow is dispatched with the ledger to start at |
| `upstream-drift.yml` | weekly, Monday 06:00 UTC, and by hand | `shapekit pins verify`; `shapekit conformance sync-versions --check`, opening a tracking issue when the SDK version matrix is behind the registries; then `scripts/upstream-releases.sh`: opens one tracking issue, with the checklist of `docs/protocol-upgrade-runbook.md`, per new stellar-core, stellar-xdr, SDK or stellar-cli release, per change to core's golden index (at the newest core release, and the index stamps on core's default branch) and per change to a CAP text on stellar-protocol's default branch. Each issue holds what `scripts/upstream-regenerate.sh` produced against the new version: the schema diff and family dating for new XDR definitions, the protocol gate list for a new core, a conformance run of a new SDK version, the changed CAP texts with the protocol each names |
| `release.yml` | a pushed `v*` tag | Refuses a tag whose signature GitHub does not verify. Builds `shapekit`, `indexerlint` and the wallet-backend adapter for linux and macos on x86-64 and arm64, each on a runner of that platform (`ubuntu-24.04`, `ubuntu-24.04-arm`, `macos-15-intel`, `macos-15`), the corpus archive (after `pack verify`; `corpus pack` itself also checks every chain against its `chain.json` and refuses strays under `chains/`) and the golden snapshot; builds the two archives twice on separate runners and requires equal checksums; publishes them with one `SHA256SUMS`, a build provenance attestation per asset and release notes from the newest `STATUS.md` section |
| `sdk-conformance.yml` | `workflow_call` from an SDK repository | Downloads a pinned release, checks the archive and binary checksums, runs that SDK's testee with the caller's versions file and known-failures list. See [sdk-ci.md](sdk-ci.md) |
| `site.yml` | pull requests and pushes to `main` that touch the site, `docs/`, `README.md` or the files the dashboard reads, and by hand | Builds the CLI, runs `shapekit site data`, then `npm ci`, the typecheck, the linter and the build of `site/`; checks that every docs, CAP and dashboard page has a prerendered file and validates `site/Caddyfile`. It builds and checks only; see [the website](site.md) |

Permissions: `contents: read` everywhere, except `issues: write` for the drift workflow (its
tracking issues), `contents: write` for the `watch` job of `sentinel.yml`, used only to push
its cursor to the `sentinel-state` branch as `github-actions[bot]`, and, in the release job
only, `contents: write`, `id-token: write`, `attestations: write` and `artifact-metadata: write`
(publishing and attestations). The only
token used is the built-in `GITHUB_TOKEN`. No other secrets exist.

### Measured durations

Measured on GitHub-hosted `ubuntu-24.04` runners, from the job start and end times GitHub
records.

A `pr.yml` run on 2026-11-14 that regenerated the rotating window of families took 7 minutes 29
seconds from the first job start to the last job end. `build`, `rust`, `js`, `contracts` and
`repro-select` start together; the other jobs start when `build` has finished:

| Job | Duration |
|---|---|
| build (release build of the CLI) | 224 s |
| rust (rustfmt, clippy, `cargo test`) | 442 s |
| go | 139 s |
| js | 39 s |
| contracts | 167 s |
| corpus | 28 s |
| repro, four shards | 94 s, 75 s, 219 s, 50 s |
| conformance | 95 s |
| harness (three chains) | 143 s |

A change to the contracts selects every family for `gen verify-repro`. The `pr.yml` run for
such a change on 2026-11-14 took 17 minutes 40 seconds: `build` took 269 s and the slowest of
the four shards 785 s. Every `pr.yml` job has a timeout of 15 minutes or less.

The `nightly.yml` run on 2026-11-14 took 2 hours 13 minutes. Its jobs run in parallel after
`build` (272 s):

| Job | Duration |
|---|---|
| harness (rpc delivery, both networks, two rounds) | 7659 s |
| harness (sep54-s3 delivery, testnet) | 4467 s |
| harness-reproduce | 2662 s |
| repro (every chain, both passphrases) | 2169 s |
| fuzz | 2013 s |
| docker-tests | 1096 s |
| goldens | 420 s |
| conformance (every version, and the reproductions) | 349 s |
| findings | 60 s |

## Network access and telemetry

shapekit collects no telemetry. It does not phone home, report usage, check for updates, or
send crash reports, and no workflow uploads anything about who runs the tools.

These are the only commands that use the network, each only when run explicitly:

| Command | Talks to | Why |
|---|---|---|
| `shapekit pins verify` | GitHub (git and the releases api), static.rust-lang.org, go.dev, nodejs.org, crates.io index, npm registry, Go module proxy, Docker Hub registry | checks every pin still resolves to the pinned bytes |
| `shapekit capture history` | the public SEP-54 data lake (`aws-public-blockchain`, us-east-2), or with `--via rpc` a stellar-rpc endpoint (default `https://soroban-testnet.stellar.org`) | captures one ledger from public network history |
| `shapekit doctor`, with `sdk` or no argument, unless `--offline` | index.crates.io, proxy.golang.org, registry.npmjs.org, one HEAD request each, only for the toolchains the selected SDKs build with | checks that the SDK builds can fetch their packages |
| `shapekit conformance file-issues` | GitHub, through `gh` | opens the drafted issues upstream; it refuses to run without the `--confirm` flag |
| `shapekit conformance run` and `reproduce`, and `shapekit fuzz`, unless `--no-build` | crates.io, the Go module proxy, the npm registry, through `cargo`, `go` and `npm` | the build steps in `conformance/versions.toml` fetch the pinned SDK versions; running the testees is offline |
| `shapekit audit run` and `shapekit audit watch`, unless `--lake <dir>` | the public SEP-54 data lake (`aws-public-blockchain`, us-east-2): `.config.json`, bucket listings for the newest ledger and the testnet prefix, and one object per ledger checked | checks the law book on public ledgers; objects are streamed and never written to disk. With `--sdk`, the SDK's build steps from `conformance/versions.toml` fetch packages unless `--no-build` is given |
| `wb-adapter` (used by `shapekit harness`) | GitHub and the image registries, on first use of a commit | clones wallet-backend and builds its image; later runs reuse both and are offline |

Everything else (`gen`, `manifest`, `registry`, `synth`, `oracle`, `goldens`, `coverage`,
`guide`, `pack`, `corpus pack`, `serve`, `harness run` once images exist, `indexerlint`,
`audit summary`, and `audit run` and `audit watch` with `--lake <dir>`) runs offline. `gen` never pulls the core image; it fails with the `docker pull` command to run when
the image is missing. `shapekit serve` listens on `127.0.0.1:8000` unless told otherwise and
serves only the chain it was given.

Maintenance scripts, which are not part of the tools you run: `scripts/upstream-releases.sh`
and `scripts/golden-drift.sh` read public release and tag information from GitHub, npm,
crates.io and the Go module proxy, and fetch core's golden index files.
`scripts/upstream-regenerate.sh` fetches the new stellar-xdr definitions, a shallow copy of a
new stellar-core tag or the stellar-protocol texts, and for an SDK release the packages its
testee build needs. The drift workflow runs all three. `nightly.yml` and `release.yml` clone stellar-core at the pinned tag for the goldens.
`scripts/fetch-stellar-cli.sh` (run by `contracts/build.sh` and `contracts/build-legacy.sh`)
downloads the pinned stellar-cli release asset from GitHub once and reuses the verified copy
afterwards; the contract builds themselves fetch the pinned crates through `cargo`.
