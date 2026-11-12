# Infrastructure

This page lists what shapekit runs on and what it talks to. It is the first version, written
when only the repository foundation exists. It grows as the corpus, harness and runner land.

## CI

All CI runs on GitHub-hosted `ubuntu-24.04` runners. There are no self-hosted runners and no
other build services.

Workflows in `.github/workflows/`:

| Workflow | Trigger | What it does |
|---|---|---|
| `pr.yml` | every pull request, and pushes to `main` | Rust: `cargo fmt --check`, `cargo clippy` with warnings denied, `cargo test`. Go: `go vet`, `go test` and `golangci-lint` in every module listed in `go.work`. JavaScript: `npm ci`, `tsc --noEmit`, build and `node --test` in `testees/js`. Text: fails if any tracked file contains an em dash |
| `upstream-drift.yml` | weekly (Monday 06:00 UTC) and by hand | Runs `shapekit pins verify`, then `scripts/upstream-releases.sh`, which compares the newest upstream releases with `pins.toml` and opens or updates one issue in this repository when they differ |

Workflows get `contents: read` only. The drift workflow also gets `issues: write` so it can
open its report issue with the built-in `GITHUB_TOKEN`. No other secrets are used.

## Toolchains

Pinned exactly, the same locally and in CI:

| Tool | Version | Where it is pinned |
|---|---|---|
| Rust | 1.99.1 | `rust-toolchain.toml`, `dtolnay/rust-toolchain@1.99.1` |
| Go | 1.27.4 | `go.work`, each `go.mod`, `actions/setup-go` |
| Node.js | 26.13.1 | `actions/setup-node`; `testees/js/package.json` requires `>=26` |
| TypeScript | 7.0.2 | `testees/js/package.json` and `package-lock.json` |
| golangci-lint | v2.15.0 | installed with `go install` in `pr.yml` |

GitHub Actions used: `actions/checkout@v7`, `actions/setup-node@v7`, `actions/setup-go@v7`,
`dtolnay/rust-toolchain@1.99.1`, `Swatinem/rust-cache@v2`.

## Container images

The container images shapekit will use (stellar-core for chain generation and golden checks,
and the databases and object stores the reference adapters need) are listed with their purpose
and digest in `pins.toml`. No workflow pulls an image yet.

Docker itself is not pinned. The project uses whichever Docker engine and compose plugin the
`ubuntu-24.04` runner image ships. The versions seen in CI will be recorded here once a
workflow first uses Docker.

## Network access and telemetry

shapekit collects no telemetry of any kind. It does not phone home, report usage, check for
updates or send crash reports.

Only commands that exist to talk to the network do so, and only when run explicitly. Today there
is one: `shapekit pins verify`, which checks every entry in `pins.toml` against the upstream git
hosts, crates.io, npm, the Go module proxy and container registries. Everything else is meant
to work offline.

`scripts/upstream-releases.sh` also reads public release information from GitHub, npm,
crates.io and the Go module proxy. It is a maintenance script run by the weekly drift workflow,
not part of the tools users run.
