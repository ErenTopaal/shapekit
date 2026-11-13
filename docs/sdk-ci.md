# Running the corpus in an SDK's CI

An SDK repository runs the shapekit corpus against the SDK version it builds, on every pull
request, with a pinned corpus release and its own list of known failures. This page describes
the files the SDK repository keeps and the reusable workflow that does the run.

## What the SDK repository keeps

1. **A pinned release.** The shapekit release tag, the corpus archive name and two sha256 values
   from the release's `SHA256SUMS`: the corpus archive and the `shapekit-linux-x86_64` binary.
   The workflow refuses assets that do not match. Moving to a newer corpus is a change to these
   four values.
2. **A versions file**, in the format of [conformance/versions.toml](../conformance/versions.toml),
   with one `[sdk.<name>]` table whose single version describes how to build the testee against
   the SDK checkout under test, so a pull request is tested on its own code. Build steps and the
   testee command run from the root of the shapekit checkout, so the SDK checkout is reachable as
   `../sdk`. The `version` must be what the testee's `hello` reports for that build (the runner
   checks it). Ready-made files for each SDK are under "Versions files per SDK" below.
3. **A known-failures directory** holding `<sdk>.txt` (`rust.txt`, `go.txt` or `js.txt`) in the
   format described at the top of `crates/shapekit-conformance/src/known.rs`: one line per
   `<entry-id> <version|*> <result> <issue>`. An entry that fails without a line fails the run; a
   line whose entry now passes or fails differently also fails the run, so the file cannot go
   stale. Every line links an issue.

## Calling the workflow

```yaml
# .github/workflows/shapekit.yml in the sdk repository
name: shapekit conformance
on: [pull_request]
permissions:
  contents: read
jobs:
  corpus:
    uses: ErenTopaal/shapekit/.github/workflows/sdk-conformance.yml@<release tag>
    with:
      sdk: js
      shapekit-tag: <release tag>
      corpus-asset: corpus-<version>.tar.zst
      corpus-sha256: <sha256 of corpus-<version>.tar.zst from SHA256SUMS>
      shapekit-sha256: <sha256 of shapekit-linux-x86_64 from SHA256SUMS>
      versions-file: ci/shapekit/versions.toml
      known-failures: ci/shapekit/known-failures
```

The values in angle brackets come from the release page. Reference the reusable workflow by the
same tag, so the workflow and the corpus come from one release.

The job checks out the calling repository into `sdk/`, checks out shapekit at the tag into
`shapekit/` (the testee sources live there), downloads and verifies the two assets, extracts the
corpus, sets up the toolchain for the chosen SDK and runs:

```sh
shapekit conformance run --sdk <sdk> --manifest <corpus>/manifest.json \
  --versions-file ../sdk/<versions-file> --known-failures ../sdk/<known-failures>
```

The divergence matrix (`divergence-matrix.json`, `MATRIX.md`) goes to the job summary and to an
artifact named `shapekit-conformance-<sdk>`.

## Versions files per SDK

To test a pull request's own code, the versions file builds the testee against the checkout in
`sdk/`, not against a published version. The build steps run from the root of the shapekit
checkout, so the SDK checkout is `../sdk` there. Each example below has one version whose
`version` is what the testee's `hello` reports for that build; the runner refuses the build if
the two differ. The examples were run against a local layout of the same shape (an SDK checkout
next to the shapekit checkout) before they were written down.

Known-failures lines name a version or `*`. Use `*` for lines that should hold on every pull
request, since the version string of a development build can change.

### Rust (`stellar-xdr`): a path dependency

The testee reports `stellar_xdr::VERSION.pkg`, the `version` in the checkout's `Cargo.toml`, so a
path dependency reports the version under development. Keep a copy of
`testees/rust/versions/<current>/Cargo.toml` in the SDK repository, for example as
`ci/shapekit/Cargo.toml`, with the `stellar-xdr` line changed to a path:

```toml
stellar-xdr = { path = "../../../../../sdk", default-features = false, features = ["std", "serde", "serde_json", "type_enum"] }
```

The path is relative to where the copy is placed, `shapekit/testees/rust/versions/dev/`, so
five levels up is the directory holding both checkouts. Keep the rest of the copy as it is: its
`[[bin]]` path `../../src/main.rs` points at the testee source. The versions file:

```toml
schema = 1

[sdk.rust]
package = "stellar-xdr"
current = "30.0.0"            # the version in the checkout's Cargo.toml
issues_repo = "stellar/rs-stellar-xdr"
timeout_ms = 10000
memory = { resource = "as", mib = 4096 }

[[sdk.rust.versions]]
version = "30.0.0"
build = [
  { argv = ["mkdir", "-p", "testees/rust/versions/dev"] },
  { argv = ["cp", "../sdk/ci/shapekit/Cargo.toml", "testees/rust/versions/dev/Cargo.toml"] },
  { argv = ["cargo", "build", "--release", "--manifest-path", "testees/rust/versions/dev/Cargo.toml", "--target-dir", "target/testees/rust/dev"] },
]
command = ["target/testees/rust/dev/release/shapekit-testee-rust"]
max_protocol = 30
```

There is no lock file for this manifest, so the build runs without `--locked` and resolves the
testee's other dependencies to the exact versions its manifest pins.

### Go (`go-stellar-sdk`): a `replace` directive

The testee normally reads the SDK version from the binary's build info. A module replaced by a
directory has no version there, so the testee refuses such a build unless the version is given
at link time with `-ldflags "-X main.sdkVersionOverride=<version>"`. With the override, `hello`
reports that version and lists the directory under the component
`github.com/stellar/go-stellar-sdk directory`. The build copies the module file of the current
version, adds the `replace`, lets `go mod tidy` pick up the checkout's own requirements, and
builds:

```toml
schema = 1

[sdk.go]
package = "github.com/stellar/go-stellar-sdk"
current = "v0.9.0-dev"
issues_repo = "stellar/go-stellar-sdk"
timeout_ms = 10000
memory = { resource = "as", mib = 4096 }

[[sdk.go.versions]]
version = "v0.9.0-dev"
build = [
  { argv = ["mkdir", "-p", "target/testees/go/dev"] },
  { argv = ["cp", "testees/go/versions/v0.9.0.mod", "target/testees/go/dev/sdk.mod"] },
  { argv = ["cp", "testees/go/versions/v0.9.0.sum", "target/testees/go/dev/sdk.sum"] },
  { argv = ["go", "mod", "edit", "-modfile", "../../target/testees/go/dev/sdk.mod", "-replace", "github.com/stellar/go-stellar-sdk=../../../sdk"], dir = "testees/go", env = { GOWORK = "off" } },
  { argv = ["go", "mod", "tidy", "-modfile", "../../target/testees/go/dev/sdk.mod"], dir = "testees/go", env = { GOWORK = "off" } },
  { argv = ["go", "build", "-modfile", "../../target/testees/go/dev/sdk.mod", "-ldflags", "-X main.sdkVersionOverride=v0.9.0-dev", "-o", "../../target/testees/go/dev/testee", "."], dir = "testees/go", env = { GOWORK = "off" } },
]
command = ["target/testees/go/dev/testee"]
max_protocol = 30
```

The `replace` path is relative to `shapekit/testees/go`, the testee's module root. `GOWORK=off`
is required: `-modfile` cannot be combined with shapekit's `go.work`. `go mod tidy` fetches the
modules the checkout requires, so this build needs the module proxy. Pick any version string
that is not a released tag (here `v0.9.0-dev`) so a development build cannot be mistaken for a
release in the matrix. Fork pull requests and unpushed commits work, since nothing is resolved
through the module proxy for the SDK itself.

### JavaScript (`@stellar/stellar-sdk`): install from the checkout

The testee reads the version from the installed package's `package.json`. The build runs the
SDK's own build, packs the checkout (`pnpm pack` for js-stellar-sdk, `npm pack` for an npm
project), and installs that tarball next to a copy of
the compiled testee, as the steps for the pinned versions do with `npm ci`. js-stellar-sdk
builds with pnpm; the workflow installs the `packageManager` version the checkout's
`package.json` declares (pnpm, or yarn 1.x) before the run. Keep this `package.json` in the SDK
repository, for example as `ci/shapekit/package.json`; `"type": "module"` is what lets node load
the testee:

```json
{ "name": "shapekit-testee-js-sdk-dev", "version": "0.0.0", "private": true, "type": "module" }
```

```toml
schema = 1

[sdk.js]
package = "@stellar/stellar-sdk"
current = "17.4.0"            # the version in the checkout's package.json
issues_repo = "stellar/js-stellar-sdk"
timeout_ms = 15000
memory = { resource = "data", mib = 4096 }

[[sdk.js.versions]]
version = "17.4.0"
build = [
  { argv = ["pnpm", "install", "--frozen-lockfile"], dir = "../sdk" },
  { argv = ["pnpm", "pack", "--out", "stellar-sdk-under-test.tgz"], dir = "../sdk" },
  { argv = ["npm", "ci", "--no-audit", "--no-fund"], dir = "testees/js" },
  { argv = ["npm", "run", "build"], dir = "testees/js" },
  { argv = ["rm", "-rf", "target/testees/js/dev"] },
  { argv = ["mkdir", "-p", "target/testees/js/dev"] },
  { argv = ["cp", "../sdk/ci/shapekit/package.json", "target/testees/js/dev/package.json"] },
  { argv = ["npm", "install", "--no-audit", "--no-fund", "--ignore-scripts", "../../../../../sdk/stellar-sdk-under-test.tgz"], dir = "target/testees/js/dev" },
  { argv = ["cp", "-R", "testees/js/dist", "target/testees/js/dev/dist"] },
]
command = ["node", "target/testees/js/dev/dist/main.js"]
max_protocol = 30
```

`pnpm install` runs the checkout's `prepare` script, which is its production build; if an SDK's
install does not build, add its build command as a step before packing. The tarball path is
relative to `shapekit/target/testees/js/dev`. The SDK's own dependencies are resolved from the
registry at install time; the tarball carries no lock file. The install and testee steps were
checked with a packed `@stellar/stellar-sdk` 17.4.0 tarball; the pnpm build of the SDK itself
depends on the SDK repository and was not run here.

## Reporting back

A failure in an SDK's run is either a bug in the SDK or a wrong corpus entry. Reproduce it with
`shapekit conformance run --sdk <sdk> --entry <entry-id>`; if the entry is wrong, open an issue
in the shapekit repository (see [maintenance.md](maintenance.md) for how such reports are
handled). Sibling issues for the other SDKs are drafted by `shapekit conformance draft-issues`.
