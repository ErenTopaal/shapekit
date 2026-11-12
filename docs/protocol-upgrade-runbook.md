# Protocol upgrade runbook

The steps for taking shapekit to a new Stellar protocol version. Every command below is a real
command of this repository, run from the repository root with `shapekit` built from the same
checkout (`cargo build --release -p shapekit-cli`, then `target/release/shapekit`). The
examples use protocol 31 and stellar-core v31.0.0; substitute the real tags.

`upstream-drift.yml` opens a tracking issue with the checklist at the end of this page when a new
stellar-core, stellar-xdr or SDK release, or a change to core's golden index, appears. Work
through the issue in order; each step is a pull request or a commit on one upgrade branch.

## Which artifacts are protocol-tier specific

These carry the protocol or the core version in their name or content and change on every
upgrade:

| Artifact | Where |
|---|---|
| Core images (release and `-buildtests`), core, stellar-xdr and stellar-rpc tags | `pins.toml` |
| Rust XDR crate and Soroban host, simulation and SDK versions | `Cargo.toml` workspace dependencies, `contracts/Cargo.toml` |
| Newest protocol the tools generate and cover | `MAX_PROTOCOL` in `crates/shapekit-core-driver/src/config.rs`, `PROTOCOL` in `crates/shapekit-core-driver/src/smoke.rs`, `COVERED_THROUGH_PROTOCOL` in `crates/shapekit-registry/src/lib.rs` |
| Hosts linked for older protocols | `SIMULATED_PROTOCOLS` and the renamed dependencies in `crates/shapekit-core-driver/src/sim_legacy.rs` |
| Contract interface protocol | `CONTRACTS_INTERFACE_PROTOCOL` in `crates/shapekit-core-driver/src/contracts.rs`, `contracts/wasm/` |
| CAP records, families, fields, limits | `registry/*.toml` |
| Recipe protocols and chains | `crates/shapekit-gen/src/recipe/mod.rs` (`all()`), `corpus/chains/<family>.p<N>/` |
| Imported goldens | `corpus/goldens/core-v<version>/`, and the `--tag` and `--index` defaults of `shapekit import goldens` and `shapekit goldens verify` in `crates/shapekit-cli/src/main.rs` |
| SDK version matrix and `max_protocol` per version | `conformance/versions.toml`, `testees/rust/versions/`, `testees/go/versions/`, `testees/js/versions/` |
| SDK version to protocol map of the wallet-backend adapter | `sdkProtocols` in `adapters/walletbackend/source.go` |
| Coverage and guide tables | `corpus/coverage/`, `docs/guide/indexer-hardening.mdx` |

Everything else (the manifest format, the testee protocol, the adapter protocol, the linter
rules) is protocol independent and changes only when the new protocol needs it.

## Steps

### 1. Read what changed

```sh
git clone --filter=blob:none https://github.com/stellar/stellar-xdr "$TMP/xdr"
git -C "$TMP/xdr" diff v30.0 v31.0 -- '*.x'
git clone --filter=blob:none https://github.com/stellar/stellar-protocol "$TMP/protocol"
ls "$TMP/protocol/core" | grep -E 'cap-00(89|90)'
```

For every CAP in the protocol, note the XDR types and arms it adds and whether an indexer can
see them. Read the core release notes for meta changes that come without a CAP.

### 2. Move the pins

Edit `pins.toml`: the two core images (tag and digest), the stellar-core, stellar-xdr and
stellar-rpc tags with their commit shas, the `stellar-xdr`, `soroban-env-host`,
`soroban-simulation` and `soroban-sdk` crates, the newest `@stellar/stellar-sdk` and
`go-stellar-sdk` releases, and the wallet-backend commit. A digest is read with:

```sh
docker pull stellar/stellar-core:<tag>
docker inspect --format '{{index .RepoDigests 0}}' stellar/stellar-core:<tag>
```

Then:

```sh
shapekit pins verify
```

It must exit 0. A pin is never moved by editing a workflow; workflows read `pins.toml` through
`scripts/pin.sh`.

### 3. Move the Rust stack

Update the workspace dependencies in `Cargo.toml` and `contracts/Cargo.toml`. Keep the previous
`soroban-env-host` and `soroban-simulation` as renamed dependencies (as `-p28` is today) and add
its protocol to `SIMULATED_PROTOCOLS`, so chains at the old protocol can still be priced. Raise
`MAX_PROTOCOL`, `smoke::PROTOCOL` and `COVERED_THROUGH_PROTOCOL`. Rebuild the contracts if the
SDK's interface protocol moved (`contracts/build.sh`, and `contracts/build-legacy.sh` for the
older interface builds), then:

```sh
cargo build --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
SHAPEKIT_DOCKER_TESTS=1 cargo test -p shapekit-core-driver
```

### 4. Registry

Add one `[[cap]]` record per new CAP to `registry/caps.toml`, families and their signature
fragments to `registry/families.toml`, any newly validated fields to `registry/fields.toml` and
new limits to `registry/caps-limits.toml`. Every record cites its source.

```sh
shapekit registry check
```

### 5. Recipes and chains

Write a recipe for each new family as described in [adding-a-shape.md](adding-a-shape.md) and
add it to `all()` in `crates/shapekit-gen/src/recipe/mod.rs`. Existing families that the new
protocol changes get an additional recipe at the new protocol; the old protocol's recipe stays.
Then regenerate every chain under both passphrases on the new core:

```sh
docker pull "$(scripts/pin.sh image stellar-core)"
shapekit gen chains --network both --jobs 2
shapekit pack verify
shapekit oracle run
shapekit synth build
shapekit manifest build
shapekit manifest validate
shapekit gen verify-repro --jobs 2
```

`gen verify-repro` must report 0 differences. Bytes of existing entries may change because core
changed; review the diff of every `.expected.json` under the two-reviewer rule in
[maintenance.md](maintenance.md).

### 6. Goldens

```sh
git clone --depth 1 --branch v31.0.0 --filter=blob:none --sparse \
  https://github.com/stellar/stellar-core "$TMP/core"
git -C "$TMP/core" sparse-checkout set test-lcm-current
shapekit import goldens --core-src "$TMP/core" --tag v31.0.0
shapekit goldens verify --core-src "$TMP/core" --tag v31.0.0 \
  --index corpus/goldens/core-v31.0.0/index.json
scripts/check-lcm.sh "$TMP/core" 2 5
shapekit goldens agree
```

`check-lcm.sh` takes the `-buildtests` image from `pins.toml`. Then move the `--tag` and
`--index` defaults in `crates/shapekit-cli/src/main.rs` to the new snapshot, remove the old
snapshot directory once nothing refers to it, and update the attribution in `NOTICE`.

### 7. Coverage and guide

```sh
shapekit coverage
shapekit coverage --check
shapekit guide table
shapekit guide check
```

Every new fragment is either covered by an entry or listed in `registry/uncovered.toml` with a
reason.

### 8. SDK testees and the version matrix

For each SDK release that supports the new protocol, add a version:

- Rust: `testees/rust/versions/<version>/Cargo.toml` and its `Cargo.lock`.
- Go: `testees/go/versions/<version>.mod` and `.sum`.
- JavaScript: `testees/js/versions/<version>/package.json` and `package-lock.json`.

Add the version to `conformance/versions.toml` with its build steps and `max_protocol`, move
`current`, then:

```sh
shapekit conformance run --all-versions
shapekit conformance reproduce
shapekit conformance draft-issues
```

A new divergence gets a line in `conformance/known-failures/<sdk>.txt` that links its drafted
issue, and goes through the findings process. Filing upstream needs the owner's approval per
batch (`shapekit conformance file-issues --i-have-owner-approval`).

### 9. Harness and the reference adapter

Add the new `go-stellar-sdk` version and its protocol to `sdkProtocols` in
`adapters/walletbackend/source.go`, move the wallet-backend pin, then:

```sh
(cd adapters && go build -o bin/wb-adapter ./walletbackend/cmd/wb-adapter)
shapekit harness run --adapter "adapters/bin/wb-adapter --commit $(scripts/pin.sh git wallet-backend)" --network both
shapekit harness run --adapter "adapters/bin/wb-adapter --commit $(scripts/pin.sh git wallet-backend)" --network testnet --delivery rpc
shapekit harness reproduce --table adapters/walletbackend/reproductions.toml
```

Read `harness-out/HARNESS.md`; new `mishandled` or `dropped` verdicts on new families are
findings.

### 10. Linter

```sh
(cd lint && go build -o indexerlint ./cmd/indexerlint)
(cd adapters && ../lint/indexerlint ./...)
```

and run it over the new wallet-backend pin. If the protocol adds a recursive structure or a
field core does not validate, add it to `registry/lint-rules.toml` and the analyzers.

### 11. CI, status and release

Run `nightly.yml` by hand (workflow dispatch) on the upgrade branch and wait for it to pass.
Then add a dated section to `STATUS.md` naming the protocol, the new pins and the coverage
numbers, merge, and tag:

```sh
git tag -s v<version> -m "shapekit v<version>: protocol 31"
git push origin v<version>
```

`release.yml` refuses an unsigned tag.

## Checklist

The drift workflow copies this list into its tracking issue.

<!-- checklist:start -->
- [ ] 1. Read the XDR diff, the CAPs and the core release notes
- [ ] 2. Move the pins in `pins.toml`; `shapekit pins verify` passes
- [ ] 3. Move the Rust stack and protocol constants; build, clippy and tests pass
- [ ] 4. Registry records for new CAPs, families, fields and limits; `shapekit registry check` passes
- [ ] 5. Recipes for new families; every chain regenerated under both passphrases; `shapekit gen verify-repro` reports 0 differences
- [ ] 6. Goldens imported at the new tag; `goldens verify`, `scripts/check-lcm.sh` and `goldens agree` pass
- [ ] 7. `shapekit coverage --check` and `shapekit guide check` pass
- [ ] 8. Testees and `conformance/versions.toml` updated; `conformance run --all-versions` and `conformance reproduce` pass
- [ ] 9. Adapter protocol map and wallet-backend pin updated; `harness run` and `harness reproduce` pass
- [ ] 10. `indexerlint` re-run over the adapters and the new wallet-backend pin
- [ ] 11. `nightly.yml` green on the branch; `STATUS.md` entry; signed tag pushed
<!-- checklist:end -->
