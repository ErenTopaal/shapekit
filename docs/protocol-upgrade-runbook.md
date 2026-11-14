# Protocol upgrade runbook

The steps for taking shapekit to a new Stellar protocol version. Every command below is a real
command of this repository, run from the repository root with `shapekit` built from the same
checkout (`cargo build --release -p shapekit-cli`, then `target/release/shapekit`). The
examples use protocol 31 and stellar-core v31.0.0; substitute the real tags. `$TMP` is a
temporary directory outside the checkout:

```sh
TMP=$(mktemp -d)
```

Before the protocol is released (no core, stellar-xdr or crate release at the new version
yet), do not follow the steps on the main branch: rehearse them in the separate next tier
described in [Rehearsing before the release](#rehearsing-before-the-release-next-tier).

`upstream-drift.yml` runs weekly and opens a tracking issue with the checklist at the end of
this page when a new stellar-core, stellar-xdr, SDK or stellar-cli release, or a change to
core's golden index, appears. Work
through the issue in order; each step is a pull request or a commit on one upgrade branch.

## Which artifacts are protocol-tier specific

These carry the protocol or the core version in their name or content and change on every
upgrade:

| Artifact | Where |
|---|---|
| Core images (release and `-buildtests`), core, stellar-xdr and stellar-rpc tags | `pins.toml` |
| Soroban settings captured from testnet at the current protocol (`apply_settings` refuses them on any other protocol) | `crates/shapekit-core-driver/data/testnet-config-settings.json`, its `.provenance.json`, `TESTNET_SETTINGS_SHA256` in `crates/shapekit-core-driver/src/settings.rs` |
| Rust XDR crate and Soroban host, simulation and SDK versions | `Cargo.toml` workspace dependencies, `contracts/Cargo.toml` |
| Newest protocol the tools generate and cover | `MAX_PROTOCOL` in `crates/shapekit-core-driver/src/config.rs`, `PROTOCOL` in `crates/shapekit-core-driver/src/smoke.rs`, `COVERED_THROUGH_PROTOCOL` in `crates/shapekit-registry/src/lib.rs` |
| Hosts linked for older protocols | `SIMULATED_PROTOCOLS` (it lists the current host's protocol too) and the renamed dependencies and per-protocol modules in `crates/shapekit-core-driver/src/sim_legacy.rs` |
| Contract interface protocol | `CONTRACTS_INTERFACE_PROTOCOL` in `crates/shapekit-core-driver/src/contracts.rs`, `contracts/wasm/` |
| CAP records, families, fields, limits | `registry/*.toml` |
| Recipe protocols and chains | `crates/shapekit-gen/src/recipe/mod.rs` (`all()`), `corpus/chains/<family>.p<N>/` |
| Imported goldens | `corpus/goldens/core-v<version>/`, and the `--tag` and `--index` defaults of `shapekit import goldens` and `shapekit goldens verify` in `crates/shapekit-cli/src/main.rs` |
| Tests that assert the current protocol | for example `crates/shapekit-core-driver/tests/node_chain.rs`, `tx_sim_snapshot.rs`, `node_protocols.rs` and the settings test in `settings.rs`; find them with `git grep -nw 30 -- 'crates/*/tests/*.rs' 'crates/*/src/*.rs'` |
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
grep -l 'Protocol version: 31' "$TMP/protocol/core"/cap-*.md
docker run --rm "$(scripts/pin.sh image stellar-core)" version
```

The `grep` finds every CAP whose preamble names the protocol; do not rely on a list of numbers.
For every CAP in the protocol, note the XDR types and arms it adds and whether an indexer can
see them. A CAP's own XDR patch is not the merged XDR: check each type against the diff. Read
the core release notes for meta changes that come without a CAP. `stellar-core version` (run on
the new image once step 2 has pinned it) lists, per Soroban host, the `rs-stellar-xdr` version
and the base XDR commit that protocol is built against; that commit, not the CAP text, is what
the new core emits.

Until stellar-xdr tags the protocol there is no `v31.0`; diff `v30.0` against `origin/main`
and `origin/next` instead, and take each CAP's XDR from its text.

### 2. Move the pins

Edit `pins.toml`: the two core images (tag and digest), the stellar-core, stellar-xdr and
stellar-rpc tags with their commit shas, the `stellar-xdr`, `soroban-env-host`,
`soroban-simulation` and `soroban-sdk` crates, the newest `@stellar/stellar-sdk` and
`go-stellar-sdk` releases, and the wallet-backend commit. Release images are in
`stellar/stellar-core`; the `-buildtests` images, and the next-protocol builds
(`<version>-<build>.<commit>.noble-vnext` and `...-vnext-buildtests`, core configured with
`--enable-next-protocol-version-unsafe-for-production`), are only in
`stellar/unsafe-stellar-core`. The digest to pin is the one the Docker registry reports for
the tag in its `Docker-Content-Digest` header (the index digest for a multi-arch tag).
`pins verify` reads the digest the same way and, on a mismatch, prints
`registry digest is <live digest>, pinned <pinned digest>`.

Then:

```sh
shapekit pins verify
```

It must exit 0. A pin is never moved by editing a workflow; workflows read `pins.toml` through
`scripts/pin.sh`.

### 3. Move the Rust stack

Update the workspace dependencies in `Cargo.toml` and `contracts/Cargo.toml`, then refresh the
lock file for them. The lock file holds several versions of `soroban-env-host` and
`soroban-simulation`, so name the version you are replacing:

```sh
cargo update -p stellar-xdr@30.0.0 -p soroban-env-host@30.0.0 -p soroban-simulation@30.0.0
```

The `--locked` build below fails until the lock file matches. Keep the previous
`soroban-env-host` and `soroban-simulation` as renamed dependencies (as `-p22`, `-p24` and
`-p28` are) with a module for them in `sim_legacy.rs`. Add the new protocol to
`SIMULATED_PROTOCOLS` and keep the old one, which the renamed host now serves, so chains at
the old protocol can still be priced. Raise `MAX_PROTOCOL`, `smoke::PROTOCOL` and
`COVERED_THROUGH_PROTOCOL`, and update the tests that assert the old protocol.

Recapture the Soroban settings once testnet runs the new protocol: write
`crates/shapekit-core-driver/data/testnet-config-settings.json` and its provenance file the way
the provenance file describes (`getLedgerEntries` on every `CONFIG_SETTING` id, `getNetwork`),
and update `TESTNET_SETTINGS_SHA256`. `ChainDriver::apply_settings` refuses settings captured at
another protocol, so no Soroban chain records until this is done; if testnet has not upgraded
yet, the release is not ready (see the next tier section for the rehearsal workaround).

Rebuild the contracts if the SDK's interface protocol moved (`contracts/build.sh`, and
`contracts/build-legacy.sh` for the older interface builds). Both run `stellar contract build`
from the stellar-cli release pinned in `pins.toml` (`[[release]] stellar-cli`). Move that pin
to the stellar-cli release for the new protocol first, for two reasons:

- From soroban-sdk 30 on, the sdk's build script refuses a stellar-cli whose major version
  (`STELLAR_CLI_VERSION`) is below its own.
- The cli's post-processing (spec shaking, wasm-opt, the `cliver` meta entry) is part of the
  committed bytes.

Then:

```sh
cargo build --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p shapekit-core-driver -- --include-ignored
docker pull "$(scripts/pin.sh image stellar-core)"
shapekit gen smoke --network testnet --out "$TMP/smoke-testnet"
shapekit gen smoke --network mainnet --out "$TMP/smoke-mainnet"
```

The smoke chain is the cheapest end-to-end check of the new core, host and settings together:
it must print `protocol 31`, and a second run must print the same canonical stream sha256
(the raw stream of the upgrade ledgers differs between runs, which is why the canonical stream
exists).

### 4. Registry

Give each new CAP its full `[[cap]]` record in `registry/caps.toml`. If the CAP already has
a Draft placeholder there (`status = "Draft"`, `xdr_additions = []`,
`indexer_visible = false`), replace it instead of adding a second record, which
`registry check` rejects as a duplicate id. Add families and their signature fragments to
`registry/families.toml`, any newly validated fields to `registry/fields.toml` and new limits to
`registry/caps-limits.toml`. Every record cites its source. `registry check` resolves each
signature fragment against the `stellar-xdr` crate the build links, so a family that names a
new type or arm only passes once step 3 has moved to a crate that has it.

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

`gen verify-repro` must report 0 differences. Where the new core emits different bytes, the
bytes of existing entries change with it. Review the diff of every `.expected.json` under the
two-review rule in [maintenance.md](maintenance.md#changes-to-expected-values).

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

The import takes, per family fragment, the smallest golden ledger whose protocol lies in the
family's `protocol_valid` range, tags it with the family's caps that exist at that protocol, and
runs the oracle over the corpus; fragments carried only outside the range are listed in the
index as not imported. Any change to a family's fragments or range needs a re-import: the
nightly `goldens` job re-imports into a copy and fails on any difference.

Both commands read `test-lcm-current` by default, the tier a release build of core checks.
`check-lcm.sh` takes the `-buildtests` image from `pins.toml` and
needs `test-lcm-current` in the checkout. Then move the `--tag` and
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
issue, and goes through [triage](maintenance.md#triage). `shapekit conformance file-issues`
opens the drafted issues upstream with `gh`; it refuses to run unless you pass `--confirm`,
which confirms that this batch of drafts was reviewed and is to be filed upstream.

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
git init "$TMP/wb"
git -C "$TMP/wb" fetch --depth 1 https://github.com/stellar/wallet-backend "$(scripts/pin.sh git wallet-backend)"
git -C "$TMP/wb" checkout FETCH_HEAD
lint="$PWD/lint/indexerlint"; (cd "$TMP/wb" && "$lint" ./...)
```

`indexerlint` exits 0 with no findings, 1 with findings and 2 on a usage, load or analysis
error. Findings on wallet-backend go through [triage](maintenance.md#triage). If the protocol
adds a recursive structure or a field core does not validate, add it to
`registry/lint-rules.toml` and the analyzers.

### 11. CI, status and release

Run `nightly.yml` by hand (workflow dispatch) on the upgrade branch and wait for it to pass.
Then add a dated section to `STATUS.md` naming the protocol, the new pins and the coverage
numbers, merge, and tag:

```sh
git tag -s v<version> -m "shapekit v<version>: protocol 31"
git push origin v<version>
```

`release.yml` refuses an unsigned tag.

## Rehearsing before the release (next tier)

Between protocol releases the next protocol can already be exercised: core's next-protocol
image runs it, `test-lcm-next` holds its goldens, and the CAPs are Drafts. The rehearsal keeps
everything out of the release:

- Everything it writes goes under `next/` at the repository root, never into `corpus/`,
  `registry/` or `pins.toml`. `shapekit corpus pack` packs `--corpus` (default `corpus`) and
  refuses any file the manifest or a chain's `chain.json` does not vouch for, so `next/` never
  reaches the corpus archive.
- Pins: `next/pins.toml` (same format) names the vnext image as `stellar-core` and its
  `-vnext-buildtests` image, the core commit the image was built from, and the other
  references the rehearsal uses. Check it with `shapekit pins verify --file next/pins.toml`.
- Rust stack: no crate release exists for the new protocol. Core's vnext build runs it with
  the current `soroban-env-host` built with its `next` feature (`stellar-core version` shows
  the host at the new ledger protocol with pre-release 1). Do the step 3 changes in a separate
  export (`mkdir "$TMP/shapekit" && git archive HEAD | tar -x -C "$TMP/shapekit"`), with
  `soroban-env-host = { version = "<current>", features = ["next"] }` and its own
  `CARGO_TARGET_DIR`, never on the main branch. Cargo unifies the features of one crate
  version, so that build cannot also keep the same host version without `next` for the old
  protocol: it simulates only the new one.
- Settings: no network runs the new protocol yet, so there is nothing to recapture. In the
  export, relax the protocol check in `ChainDriver::apply_settings` to accept settings
  captured at an older protocol; core then decides (it refuses an upgrade set whose cost
  parameter entries do not fit the protocol), and `sorobaninfo` must report the applied set.
- `next/rehearsal-build.diff` holds the export's changes as rehearsed for protocol 31.
- Smoke chain: run the export build's `shapekit gen smoke --pins next/pins.toml --out
  next/chains/smoke.p31/<network>` for both networks, and decode the streams with the current
  `stellar-xdr` release
  (`stellar-xdr decode --type LedgerCloseMeta --input stream-framed next/chains/smoke.p31/<network>/ledgers.xdr`).
  Arms the current crate cannot decode are what the new protocol adds for indexers.
- Registry: draft records go to `next/registry/` (`caps.toml` records replace the Draft
  placeholders of the same id, families and fields are new).
  `next/overlay-registry.sh "$TMP/reg"` merges them into a copy of `registry/`, then
  `shapekit registry check --dir "$TMP/reg"`. Families that name types the current crate does
  not have fail that check until step 3 can move to a crate with them; that failure is the
  expected result, not a reason to drop the family.
- Goldens: `shapekit import goldens --core-src "$TMP/core" --tag <tag> --tier test-lcm-next
  --corpus next` and `shapekit goldens verify --core-src "$TMP/core" --tag <tag> --tier
  test-lcm-next --index next/goldens/core-<tag>/index.json`, with `"$TMP/core"` a checkout
  holding `test-lcm-next`. The
  importer requires a tagged checkout; vnext commits are untagged, so use the newest release
  tag after checking that `git diff <tag> <vnext commit> -- test-lcm-next` is empty.
  `scripts/check-lcm.sh` reads `test-lcm-current` only.

## Checklist

The drift workflow copies this list into its tracking issue.

<!-- checklist:start -->
- [ ] 1. Read the XDR diff, the CAPs and the core release notes
- [ ] 2. Move the pins in `pins.toml`; `shapekit pins verify` passes
- [ ] 3. Move the Rust stack and protocol constants, recapture testnet's Soroban settings; build, clippy and tests pass; `gen smoke` records the new protocol on both networks
- [ ] 4. Registry records for new CAPs (replacing their Draft placeholders), families, fields and limits; `shapekit registry check` passes
- [ ] 5. Recipes for new families; every chain regenerated under both passphrases; `shapekit gen verify-repro` reports 0 differences
- [ ] 6. Goldens imported at the new tag; `goldens verify`, `scripts/check-lcm.sh` and `goldens agree` pass
- [ ] 7. `shapekit coverage --check` and `shapekit guide check` pass
- [ ] 8. Testees and `conformance/versions.toml` updated; `conformance run --all-versions` and `conformance reproduce` pass
- [ ] 9. Adapter protocol map and wallet-backend pin updated; `harness run` and `harness reproduce` pass
- [ ] 10. `indexerlint` re-run over the adapters and the new wallet-backend pin
- [ ] 11. `nightly.yml` green on the branch; `STATUS.md` entry; signed tag pushed
<!-- checklist:end -->
