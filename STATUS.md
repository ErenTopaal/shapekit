# Status

A dated public log of what has landed. Newest entry first.

## 2026-11-12: repository foundation

The first phase: a licensed repository that builds, with every upstream reference pinned and
checked, and CI defined. No corpus, harness, linter, guide or conformance runner exists yet.

What landed:

- License and notices: Apache-2.0 `LICENSE`, `NOTICE`, this file and `README.md`.
- Pins: `pins.toml` lists 87 upstream references with the purpose of each: the Rust, Go and
  Node toolchains with their release checksums, five container images by digest (stellar-core,
  the stellar-core test build, timescaledb, seaweedfs, fake-gcs-server), stellar-core,
  stellar-xdr, stellar-rpc, stellar-etl, stellar-protocol and wallet-backend commits, the
  GitHub Actions used, every Rust crate, npm package and Go module the plan uses, and the SDK
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
