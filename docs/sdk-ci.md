# Running the corpus in an SDK's CI

An SDK repository runs the shapekit corpus against the SDK version it builds, on every pull
request, with a pinned corpus release and its own list of known failures. This page describes
the files the SDK repository keeps and the reusable workflow that does the run.

## What the SDK repository keeps

1. **A pinned release.** The shapekit release tag, the corpus archive name and two sha256 values
   from the release's `SHA256SUMS`: the corpus archive and the `shapekit-linux-x86_64` binary.
   The workflow fails when a downloaded asset does not match its sha256. To move to a newer
   release, change these four values and the tag in `uses:`.
2. **A versions file**, in the format of [conformance/versions.toml](../conformance/versions.toml),
   with one `[sdk.<name>]` table whose single version describes how to build the testee against
   the SDK checkout under test, so a pull request is tested on its own code. Ready-made files
   for each SDK are under [Versions files per SDK](#versions-files-per-sdk) below.
3. **A known-failures directory** holding `<sdk>.txt` (`rust.txt`, `go.txt` or `js.txt`), one
   line per known failure:
   `<entry-id> <version|*> <result> <issue> <check>[, <check>...]`. The format is described in
   [testee-protocol.md](testee-protocol.md#known-failures). A failing check without a line
   fails the run. A listed check that now passes or fails with another result also fails the
   run, so the file stays current. A missing file counts as an empty list.

## Calling the workflow

```yaml title=".github/workflows/shapekit.yml"
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
same tag, so the workflow and the corpus come from one release. `sdk` is `rust`, `go` or `js`.
`versions-file` and `known-failures` are paths in your repository. The optional input
`shapekit-repository` (default `ErenTopaal/shapekit`) names the repository the release is
downloaded from.

The job checks out the calling repository into `sdk/`, checks out shapekit at the tag into
`shapekit/` (the testee sources live there), downloads and verifies the two assets, and
extracts the corpus. The archive must hold exactly one `<dir>/manifest.json`. The job then sets
up the toolchain for the chosen SDK (Rust 1.99.1, Go 1.27.4 or Node 26.13.1) and runs, from
`shapekit/`:

```sh
shapekit conformance run --sdk <sdk> --manifest <corpus>/manifest.json \
  --versions-file "$GITHUB_WORKSPACE/sdk/<versions-file>" \
  --known-failures "$GITHUB_WORKSPACE/sdk/<known-failures>" \
  --out "$RUNNER_TEMP/conformance"
```

The run executes the build steps of the versions file before it starts the testee. After it,
whether it passed or failed, the job runs [`shapekit report`](report.md) on the matrix with your
known-failures directory as the baseline:

```sh
shapekit report "$RUNNER_TEMP/conformance" --manifest <corpus>/manifest.json \
  --known-failures "$GITHUB_WORKSPACE/sdk/<known-failures>" \
  --out "$RUNNER_TEMP/conformance/report.html" --summary "$RUNNER_TEMP/conformance/summary.md"
```

The summary (new, changed, fixed and unchanged failures, and the failures grouped by finding)
and then `MATRIX.md` go to the job summary. `divergence-matrix.json`, `MATRIX.md`,
`report.html` and `summary.md` are uploaded as an artifact named `shapekit-conformance-<sdk>`,
kept for 30 days; the workflow output `matrix-artifact` holds that name. Open `report.html`
from the downloaded artifact for each group's finding, guide sections and reproduce command.

## A comment on the pull request

With `comment: true` the workflow also posts the summary as a comment on the pull request,
and on later pushes updates that comment instead of adding another. The comment job runs with
the permissions of the calling job, so grant it `pull-requests: write`:

```yaml title=".github/workflows/shapekit.yml"
jobs:
  corpus:
    permissions:
      contents: read
      pull-requests: write
    uses: ErenTopaal/shapekit/.github/workflows/sdk-conformance.yml@<release tag>
    with:
      sdk: js
      comment: true
      # the other inputs as above
```

The comment is found again by its first line, `<!-- shapekit-report conformance -->`, among
the comments of `github-actions[bot]`. A pull request from a fork runs with a read-only token,
so the comment job skips it; the job summary and the artifact are there as usual. Without
`comment` (the default) the workflow needs `contents: read` only.

## Versions files per SDK

To test a pull request's own code, the versions file builds the testee against the checkout in
`sdk/`, not against a published version. Build steps and the testee command run from the root
of the shapekit checkout, so the SDK checkout is `../sdk` there. Each example below has one
version, and its `version` must be what the testee's `hello` reports for that build: the runner
refuses the testee when the two differ.

Known-failures lines name a version or `*`. Use `*` for lines that need to hold on every pull
request, since the version string of a development build can change.

### Rust (`stellar-xdr`): a path dependency

The testee reports `stellar_xdr::VERSION.pkg`, the `version` in the checkout's `Cargo.toml`, so a
path dependency reports the version under development. Keep a copy of
`testees/rust/versions/<current>/Cargo.toml` in the SDK repository, for example as
`ci/shapekit/Cargo.toml`, with the `stellar-xdr` line changed to a path:

```toml title="ci/shapekit/Cargo.toml"
stellar-xdr = { path = "../../../../../sdk", default-features = false, features = ["std", "serde", "serde_json", "type_enum"] }
```

The path is relative to where the copy is placed, `shapekit/testees/rust/versions/dev/`, so
five levels up is the directory holding both checkouts. Keep the rest of the copy as it is: its
`[[bin]]` path `../../src/main.rs` points at the testee source. The versions file:

```toml title="ci/shapekit/versions.toml"
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

There is no lock file for this manifest, so the build runs without `--locked`. The testee's
direct dependencies still resolve to the exact versions the manifest pins with `=`.

### Go (`go-stellar-sdk`): a `replace` directive

The testee normally reads the SDK version from the binary's build info. A module replaced by a
directory has no version there, so the testee's `hello` fails for such a build unless the
version is given at link time with `-ldflags "-X main.sdkVersionOverride=<version>"`. With the
override, `hello`
reports that version and lists the directory under the component
`github.com/stellar/go-stellar-sdk directory`. The build copies the module file of the current
version, adds the `replace`, lets `go mod tidy` pick up the checkout's own requirements, and
builds:

```toml title="ci/shapekit/versions.toml"
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
is required: `-modfile` cannot be combined with shapekit's `go.work`. The SDK itself comes from
the checkout, so fork pull requests and unpushed commits work. `go mod tidy` fetches the
modules the checkout requires, so this build needs access to the module proxy. Pick a version
string that is not a released tag (here `v0.9.0-dev`) so that a development build cannot be
mistaken for a release in the matrix.

### JavaScript (`@stellar/stellar-sdk`): install from the checkout

The testee reads the version from the installed package's `package.json`. The build runs the
SDK's own build, packs the checkout (`pnpm pack` for js-stellar-sdk, `npm pack` for an npm
project), and installs that tarball next to a copy of the compiled testee, the same layout the
pinned versions get from `npm ci`. js-stellar-sdk builds with pnpm. Before the run, the workflow
installs the `packageManager` version that the checkout's `package.json` declares (pnpm, or
yarn 1.x). Keep this `package.json` in the SDK repository, for example as
`ci/shapekit/package.json`; `"type": "module"` lets node load the testee, which is an ES
module:

```json title="ci/shapekit/package.json"
{ "name": "shapekit-testee-js-sdk-dev", "version": "0.0.0", "private": true, "type": "module" }
```

```toml title="ci/shapekit/versions.toml"
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
  { argv = ["rm", "-rf", "target/testees/js/build"] },
  { argv = ["mkdir", "-p", "target/testees/js/build"] },
  { argv = ["cp", "testees/js/package.json", "testees/js/package-lock.json", "testees/js/tsconfig.json", "testees/js/.npmrc", "target/testees/js/build/"] },
  { argv = ["cp", "-R", "testees/js/src", "target/testees/js/build/src"] },
  { argv = ["npm", "ci", "--no-audit", "--no-fund"], dir = "target/testees/js/build" },
  { argv = ["npm", "run", "build"], dir = "target/testees/js/build" },
  { argv = ["rm", "-rf", "target/testees/js/dev"] },
  { argv = ["mkdir", "-p", "target/testees/js/dev"] },
  { argv = ["cp", "../sdk/ci/shapekit/package.json", "target/testees/js/dev/package.json"] },
  { argv = ["npm", "install", "--no-audit", "--no-fund", "--ignore-scripts", "../../../../../sdk/stellar-sdk-under-test.tgz"], dir = "target/testees/js/dev" },
  { argv = ["cp", "-R", "target/testees/js/build/dist", "target/testees/js/dev/dist"] },
]
command = ["node", "target/testees/js/dev/dist/main.js"]
max_protocol = 30
```

`pnpm install` runs the checkout's `prepare` script, which in js-stellar-sdk runs its
production build (`build:prod`). For an SDK whose install does not build, add its build command
as a step before packing. The tarball path is relative to `shapekit/target/testees/js/dev`. The
tarball carries no lock file, so the SDK's own dependencies are resolved from the registry at
install time.

## Reporting back

A failure in an SDK's run is either a bug in the SDK or a wrong corpus entry. Reproduce it in a
shapekit checkout with `shapekit conformance run --sdk <sdk> --entry <entry-id>`. If the entry
is wrong, open an issue in the shapekit repository ([maintenance.md](maintenance.md#triage)
describes how such reports are handled). `shapekit conformance draft-issues` writes one issue
body per SDK and diverging entry from a divergence matrix, and each body lists the other SDKs
that diverge on the same entry.
