# Infrastructure

Everything shapekit's deliverables run on, what they talk to, and what they record about their
users (nothing). Versions are the ones pinned in `pins.toml`; `shapekit pins verify` checks every
pin against its upstream source and `upstream-drift.yml` re-checks them weekly.

## Where it runs

All CI runs on GitHub-hosted `ubuntu-24.04` runners. There are no self-hosted runners, servers,
databases, hosted services or other build systems. Releases are GitHub releases of this
repository. Nothing runs between CI jobs.

Users run the tools on their own machines or CI runners. The tools need Linux on x86-64 (the
released binaries), and Docker for the commands that start containers (`gen`, `harness`, the
`check-lcm` script).

## Toolchains

| Tool | Version | Pinned in |
|---|---|---|
| Rust | 1.99.1 | `rust-toolchain.toml`, `pins.toml` (with the channel manifest sha256), `dtolnay/rust-toolchain@1.99.1` |
| Go | 1.27.4 | `go.work`, each `go.mod`, `pins.toml` (with the archive sha256), `actions/setup-go` |
| Node.js | 26.13.1 | `actions/setup-node`, `pins.toml` (with the archive sha256); `testees/js/package.json` requires `>=26` |
| TypeScript | 7.0.2 | `testees/js/package.json` and its lock file |
| golangci-lint | v2.15.0 | `pr.yml`, `pins.toml` |
| Docker | not pinned | the engine and compose plugin of the `ubuntu-24.04` runner image |

## Stellar stack

| Component | Version | Used for |
|---|---|---|
| stellar-core | 30.0.0, `stellar/stellar-core:30.0.0-3642.0e49751aa.noble` by digest `sha256:eb7a7c81a29b...` | standalone core that records every chain |
| stellar-core test build | `stellar/unsafe-stellar-core:30.0.0-3642.0e49751aa.noble-buildtests` by digest `sha256:a8cb428ce497...` | only core's own `test --check-lcm` run in `nightly.yml` |
| stellar-core source | tag `v30.0.0` (commit `0e49751a`) | the `test-lcm-current` goldens |
| XDR definitions | stellar-xdr tag `v30.0` (commit `4f524bba`) | |
| `stellar-xdr` crate | 30.0.0 (28.0.0 and 28.0.1 in the Rust testee matrix) | decoding, round trips, the Rust testee |
| `stellar-strkey` | 1.0.0 | |
| `soroban-env-host` / `soroban-simulation` | 30.0.0, plus renamed 22.1.4, 24.0.0 and 28.0.2 for older protocols | transaction simulation in the generator |
| `soroban-sdk` | 29.0.1 (22.0.11, 23.5.3 and 28.0.0 for the legacy contracts) | test contracts in `contracts/` |
| `@stellar/stellar-sdk` | 16.3.0, 16.3.1, 16.4.1, 17.2.0, 17.2.1, 17.3.0, 17.4.0 (current) | JavaScript testee matrix |
| `github.com/stellar/go-stellar-sdk` | v0.7.1, v0.7.2, v0.7.3, v0.8.0, v0.9.0 (current) | Go testee matrix (all), the wallet-backend adapter and the linter's analysis target (v0.8.0 only) |
| stellar-rpc | tag `v30.0.0` | the shape `shapekit serve` follows |
| stellar/wallet-backend | commit `e967ce8b` (current), plus the pre-fix and fixed commits in `adapters/walletbackend/reproductions.toml` | reference harness target |

The full lists (every crate, npm package and Go module with its purpose) are in `pins.toml`.

## Container images

All pulled by digest, never by tag; `scripts/pin.sh image <name>` prints the reference.

| Image | Tag | Used by |
|---|---|---|
| `stellar/stellar-core` | `30.0.0-3642.0e49751aa.noble` | `shapekit gen`, the docker-backed tests |
| `stellar/unsafe-stellar-core` | `30.0.0-3642.0e49751aa.noble-buildtests` | `scripts/check-lcm.sh` |
| `timescale/timescaledb` | `2.28.2-pg17` | wallet-backend's database in the reference adapter |
| `chrislusf/seaweedfs` | `4.46` | the S3 store for the adapter's `sep54-s3` delivery mode |
| `fsouza/fake-gcs-server` | `1.56.1` | pinned for the stellar-etl adapter; no workflow uses it yet |

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
| `pr.yml` | every pull request, pushes to `main` | Release build of the CLI shared by the other jobs. Rust: rustfmt, clippy with warnings denied, `cargo test`. Go: `go vet`, `go test`, golangci-lint per module, and `indexerlint` over `adapters/`. JavaScript testee: `npm ci`, typecheck, build, `node --test`. Corpus: registry check, manifest validation (round trip of every entry), synthetic entries and manifest regenerated and compared with the committed files, `coverage --check`, `guide check`, `goldens agree`, `pack verify`, adapter schema check, the release archive builds, no em dashes. `gen verify-repro` under both passphrases for a rotating window of four families plus every family the change touches (every family when shared generator code, the registry, the contracts or the pins change), sharded over up to four runners; a corpus step checks that the selection script's family list is exactly the set of recipe families the CLI registers. `conformance run` on rust 30.0.0, go v0.9.0 and js 17.4.0. `harness run` with the wallet-backend adapter at the current pin, testnet variant, on three chains |
| `nightly.yml` | daily 03:00 UTC, and by hand | `goldens verify` and core's own `--check-lcm` run against the pinned goldens; `gen verify-repro` with no family filter, so every chain of every registered recipe, under both passphrases; `cargo test` with the docker-backed tests and the `shapekit serve` memory bound; `conformance run --all-versions` and `conformance reproduce`; `harness run` over every chain in both delivery modes (rpc on both networks run twice in one process with the memory growth check, sep54-s3 on testnet); `harness reproduce` of the wallet-backend sightings; coverage; `findings check` over the night's conformance matrices, harness report and an `indexerlint` run on wallet-backend at the pin, failing on any problem other than findings still in status draft |
| `upstream-drift.yml` | weekly, Monday 06:00 UTC, and by hand | `shapekit pins verify`, then `scripts/upstream-releases.sh`: opens one tracking issue, with the checklist of `docs/protocol-upgrade-runbook.md`, per new stellar-core, stellar-xdr or SDK release and per change to core's golden index (at the newest core release, and the index stamps on core's default branch) |
| `release.yml` | a pushed `v*` tag | Refuses a tag whose signature GitHub does not verify. Builds `shapekit`, `indexerlint` and the wallet-backend adapter for linux x86-64, the corpus archive and the golden snapshot; builds the two archives twice on separate runners and requires equal checksums; publishes them with one `SHA256SUMS`, a build provenance attestation per asset and release notes from the newest `STATUS.md` section |
| `sdk-conformance.yml` | `workflow_call` from an SDK repository | Downloads a pinned release, checks the archive and binary checksums, runs that SDK's testee with the caller's versions file and known-failures list. See [sdk-ci.md](sdk-ci.md) |

Permissions: `contents: read` everywhere, except `issues: write` for the drift workflow (its
tracking issues) and, in the release job only, `contents: write`, `id-token: write`,
`attestations: write` and `artifact-metadata: write` (publishing and attestations). The only
token used is the built-in `GITHUB_TOKEN`. No other secrets exist.

### Measured durations

The budget is 15 minutes of wall clock per pull request on standard hosted runners
(4 vCPUs). These numbers were measured on 2026-11-12 on the development machine, not yet on
GitHub: a 10-core x86-64 box shared with other work (load average 8 to 11 during the runs),
with builds limited to 2 parallel jobs (`CARGO_BUILD_JOBS=2`, `GOFLAGS=-p=2`,
`verify-repro --jobs 2`) and a fresh cargo target directory, so they are the cold-cache case.
The Go and npm download caches were warm. They will be replaced by the durations of the first
three green runs on GitHub.

The corpus has grown since these runs. It now holds 32 recipe families, 43 chains and 86
recordings (one per chain and network; `shapekit pack verify` reports "86 chains checked"), so
the full verify-repro and the nightly harness rows understate today's work by about the ratio of
the chain counts (43 to 38, and 43 to 33). The rows say which corpus size they were measured on.

| Job | Step | Cold | Dependencies cached |
|---|---|---|---|
| build | `cargo build --release -p shapekit-cli` | 366 s | 98 s |
| rust | `cargo clippy --workspace --all-targets` | 79 s | 9 s |
| rust | `cargo test --workspace` build | 99 s | 30 s |
| rust | `cargo test --workspace` run | 27 s | 27 s |
| go | vet, test and golangci-lint over three modules, indexerlint on `adapters/` | 22 s, plus 6 s to install golangci-lint | |
| js | `npm ci`, typecheck, build, `node --test` | 10 s | |
| corpus | every check listed above together | under 10 s | |
| repro | `gen verify-repro` of every chain the corpus held then (31 selectable families, 38 chains, 76 recordings), on one machine | 661 s | |
| repro | slowest of four shards when every family is selected (chain seconds / 2 jobs) | about 230 s | |
| repro | slowest single family, `soroban.auth-trees` (two chains of 67 s, run in parallel) | about 70 s | |
| conformance | `conformance run`, current versions, including the testee builds | 47 s | |
| harness | `harness run`, three chains, testnet, image already built | 88 s | |
| nightly harness | `harness run`, testnet, rpc delivery, the 33 chains the harness ran then, in one round | 1351 s | |
| harness | wallet-backend image build at the pin (`docker build --no-cache`, base images present) | 59 s | |

Not measured locally: runner start, artifact upload and download, and `docker pull` of the
core image (789 MB) and the timescaledb image (1.85 GB) on a hosted runner.

The longest dependency chain of `pr.yml` is the release build followed by the slowest
verify-repro shard: about 10 minutes cold and about 6 minutes with the cargo cache, plus image
pulls. Every job has a timeout of 15 minutes or less, so a run that would exceed the budget fails
instead of passing slowly. Nothing was dropped from the per-PR tier to meet the budget; the
split into parallel jobs and the verify-repro shards is what keeps the wall clock under it.

## Network access and telemetry

shapekit collects no telemetry. It does not phone home, report usage, check for updates, or
send crash reports, and no workflow uploads anything about who runs the tools.

These are the only commands that use the network, each only when run explicitly:

| Command | Talks to | Why |
|---|---|---|
| `shapekit pins verify` | GitHub (git), static.rust-lang.org, go.dev, nodejs.org, crates.io index, npm registry, Go module proxy, Docker Hub registry | checks every pin still resolves to the pinned bytes |
| `shapekit capture history` | the public SEP-54 data lake (`aws-public-blockchain`, us-east-2), or with `--via rpc` a stellar-rpc endpoint (default `https://soroban-testnet.stellar.org`) | captures one ledger from public network history |
| `shapekit conformance file-issues` | GitHub, through `gh` | opens drafted issues upstream; refuses without `--i-have-owner-approval` |
| `shapekit conformance run` and `reproduce`, unless `--no-build` | crates.io, the Go module proxy, the npm registry, through `cargo`, `go` and `npm` | the build steps in `conformance/versions.toml` fetch the pinned SDK versions; running the testees is offline |
| `wb-adapter` (used by `shapekit harness`) | GitHub and the image registries, on first use of a commit | clones wallet-backend and builds its image; later runs reuse both and are offline |

Everything else (`gen`, `manifest`, `registry`, `synth`, `oracle`, `goldens`, `coverage`,
`guide`, `pack`, `corpus pack`, `serve`, `harness run` once images exist, `indexerlint`) runs
offline. `gen` never pulls the core image; it fails with the `docker pull` command to run when
the image is missing. `shapekit serve` listens on `127.0.0.1:8000` unless told otherwise and
serves only the chain it was given.

Maintenance scripts, which are not part of the tools users run: `scripts/upstream-releases.sh`
and `scripts/golden-drift.sh` read public release and tag information from GitHub, npm,
crates.io and the Go module proxy, and fetch core's golden index files; the drift workflow runs
them. `nightly.yml` and `release.yml` clone stellar-core at the pinned tag for the goldens.
