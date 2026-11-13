# Status

A dated public log of what has landed. Newest entry first.

## 2026-11-13: CI coverage, pinned wallet-backend builds, guide publication gate

Fixes from an audit of the CI workflows, the Go adapter and the documentation.

What landed:

- `gen verify-repro` in `nightly.yml` runs with no `--family` filter, so it regenerates every
  chain of every recipe the CLI registers. Before, the nightly list came from
  `scripts/select-repro.sh`, whose source grep missed `cap26.inflation` (declared as
  `INFLATION_FAMILY`), so those two chains were never regenerated. The script now reads every
  `pub const *FAMILY` constant, maps `cap67.unified-events-backfill` to its family, and a
  `pr.yml` corpus step checks that its family list is exactly the set the CLI accepts and that
  every committed chain belongs to a listed family.
- `nightly.yml` runs `shapekit findings check` after the conformance and harness jobs, with an
  `indexerlint` run on wallet-backend at the pin as its third input. Findings still in status
  draft are reported as a warning; any other problem fails the job.
- The wallet-backend adapter builds the image on base images pinned by digest: every `FROM` in
  wallet-backend's Dockerfile is replaced through a buildx named context (golang
  1.25.9-bookworm, debian bookworm-slim, ubuntu jammy, all in `pins.toml`). The per pull request
  and nightly harness jobs cache the clone and the built image, keyed by commit and base digests.
- The Go testee accepts a build against a directory `replace` of go-stellar-sdk when the version
  is set at link time, so an SDK pull request can be tested on its own code.
  `docs/sdk-ci.md` has tested versions files for Rust (path dependency), Go (`replace`) and
  JavaScript (install from the packed checkout).
- The adapter pairs BALANCE rows only within one token event (consecutive `state_change_id`),
  and reports a BALANCE_AUTHORIZATION holder that is neither `G` nor `C` as unmapped.
  `docs/adapter-interface.md` states why `clawback` and every arm of `to_muxed_id` stay claimed.
- `.github/CODEOWNERS`, `SECURITY.md` (private reports through GitHub security advisories), the
  branch ruleset to enable (`docs/maintenance.md`), and a `NOTICE` entry for the wallet-backend
  excerpts in the linter's test data.

### Guide publication gate

The RFP gates publication of the guide on review by a stellar-core or platform engineer.

- Guide: `docs/guide/indexer-hardening.mdx`.
- Required reviewer: a stellar-core engineer or a platform engineer at SDF.
- State: **not yet reviewed**. The guide is a draft and is not published.
- stellar-docs pull request: drafted, not opened. It adds the page as
  `docs/data/indexers/build-your-own/indexer-hardening.mdx` with an entry in `docs.json`, on a
  local branch `indexer-hardening-guide` of stellar/stellar-docs that has not been pushed. The
  tables in it are regenerated with `shapekit guide table` from the commit that is submitted.
- When the review is done, this section records the reviewer, the date and the reviewed commit,
  and the comment lines in the guide's front matter are removed.

## 2026-11-12: CI tiers, release workflow, maintenance plan

The CI and release side of the project: per pull request and nightly tiers, upstream tracking,
a release workflow, a reusable workflow for SDK repositories, and the maintenance documents.

What landed:

- `pr.yml`: one release build of the CLI shared by parallel jobs for Rust (fmt, clippy, tests),
  Go (vet, tests, golangci-lint, `indexerlint` over `adapters/`), the JavaScript testee, the
  corpus (registry, manifest round trip, regenerated synthetic entries and manifest compared
  with the committed files, coverage, guide, goldens agreement, committed chains, the release
  archive, em dashes), `gen verify-repro` under both passphrases, conformance on the current
  SDK versions and the wallet-backend harness on three testnet chains.
- `gen verify-repro` per pull request covers a rotating window of four families (moved by the
  pull request number) plus every family the change touches, and every family when shared
  generator code, the registry, the contracts or the pins change. `scripts/select-repro.sh`
  makes the choice, deterministically, and shards it over up to four runners.
- `nightly.yml`: every chain regenerated under both passphrases, the docker-backed tests and the
  `serve` memory bound, the whole SDK matrix and its reproductions, the harness over every chain
  in both delivery modes with the memory growth check, the wallet-backend reproductions, core's
  `--check-lcm` on the goldens, coverage.
- `upstream-drift.yml` now opens one tracking issue, carrying the runbook checklist, per new
  stellar-core, stellar-xdr or SDK release and per change to core's golden index
  (`scripts/golden-drift.sh`).
- `release.yml`: runs on a tag whose signature GitHub verifies, builds `shapekit`, `indexerlint`
  and the wallet-backend adapter, the corpus archive and the golden snapshot, requires two
  independent builds of the archives to have equal checksums, and publishes everything with
  `SHA256SUMS`, build provenance attestations and notes taken from this file.
- `sdk-conformance.yml`: a reusable workflow an SDK repository calls to run its testee over a
  pinned corpus release with its own known-failures list.
- Docs: `docs/protocol-upgrade-runbook.md`, `docs/maintenance.md`, `docs/sdk-ci.md`, the final
  `docs/infrastructure.md` and `README.md`.

Decisions:

- The per pull request budget is 15 minutes of wall clock. Measured locally (cold cargo cache,
  two build jobs on a shared machine), the longest chain of jobs is the release build (366 s)
  followed by the slowest verify-repro shard (about 230 s). The checks were split into parallel
  jobs and the regeneration into shards to stay under the budget; no check was dropped from the
  per pull request tier. The numbers are in `docs/infrastructure.md` and will be replaced by
  those of the first runs on GitHub.
- The harness per pull request runs three chains (`cap40.signed-payload-signer.p30`,
  `cap73.sac-trust.p30`, `cap27.muxed-accounts.p30`, 88 s locally); a testnet pass over 33
  chains took 1351 s locally, over the budget, so every chain runs nightly.
- `go-stellar-sdk` v0.9.0 was released on 2026-11-10. The pin stays at v0.8.0, the version the
  current wallet-backend commit uses, until it goes through the runbook; the drift workflow
  opens its tracking issue.

None of the workflows has run on GitHub yet; the repository is not published. Every workflow
passes actionlint 1.7.12 with shellcheck 0.11.0, and their commands were run locally.

## 2026-11-12: repository foundation

The first phase: a licensed repository that builds, with every upstream reference pinned and
checked, and CI defined. No corpus, harness, linter, guide or conformance runner exists yet.

What landed:

- License and notices: Apache-2.0 `LICENSE`, `NOTICE`, this file and `README.md`.
- Pins: `pins.toml` lists 87 upstream references with the purpose of each: the Rust, Go and
  Node toolchains with their release checksums, five container images by digest (stellar-core,
  the stellar-core test build, timescaledb, seaweedfs, fake-gcs-server), stellar-core,
  stellar-xdr, stellar-rpc, stellar-etl, stellar-protocol and wallet-backend commits, the
  GitHub Actions used, every Rust crate, npm package and Go module the project uses, and the SDK
  versions the conformance runner will test.
- Rust: a cargo workspace (`Cargo.toml`, `Cargo.lock`) with the ten crates from the planned
  layout. Nine are empty library crates. `shapekit-cli` builds the `shapekit` binary, whose one
  command is `shapekit pins verify`: it checks each pin against git remotes, crates.io, npm, the
  Go module proxy and Docker Hub, and exits non-zero on any mismatch.
- Go: `go.work` with three modules. `adapters/protocol` holds the SEP-54 object key layout,
  `lint/rules` the rule IDs the linter will report, and `testees/go` a testee that answers the
  `hello` request. Each has tests. `.golangci.yml` configures golangci-lint for all three.
- JavaScript: `testees/js`, a TypeScript 7.0.2 package for Node 26 whose program answers the
  `hello` request over newline-delimited JSON, with `node:test` tests that run the compiled
  program. It reports `version: null` for `@stellar/stellar-sdk` because the SDK is not a
  dependency yet.
- CI: `.github/workflows/pr.yml` runs rustfmt, clippy with warnings denied and cargo tests;
  `go vet`, `go test` and golangci-lint in every Go module; `npm ci`, `tsc --noEmit`, the build
  and `node --test` for the JS testee; and `scripts/check-no-em-dash.sh` over all tracked
  files. `.github/workflows/upstream-drift.yml` runs weekly: `shapekit pins verify`, then
  `scripts/upstream-releases.sh`, which compares the newest stellar-core and stellar-xdr tags
  and the newest `@stellar/stellar-sdk`, `go-stellar-sdk` and `stellar-xdr` crate releases with
  the pins and keeps one issue open in this repository while they differ.
- Docs: `docs/infrastructure.md`, first version.

All of the above was run locally with the same commands the workflows use. The workflows have
not run on GitHub yet, because the repository is not published yet.
