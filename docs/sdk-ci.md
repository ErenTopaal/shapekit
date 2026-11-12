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
   the SDK checkout under test. Build steps and the testee command run from the root of the
   shapekit checkout, so the SDK checkout is reachable as `../sdk`. The `version` must be what the
   testee's `hello` reports for that build (the runner checks it).
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

These are what each testee needs to report the build under test. Copy the build steps of the
current version in shapekit's own `conformance/versions.toml` and point them at `../sdk`.

### Rust (`stellar-xdr`)

The testee reports `stellar_xdr::VERSION.pkg`, so a path dependency works. Keep a copy of
`testees/rust/versions/<current>/Cargo.toml` in the SDK repository with the `stellar-xdr`
dependency changed to `{ path = "../../../../sdk", default-features = false, features = [...] }`
(relative to the manifest's location once it is copied into `shapekit/testees/rust/versions/`),
copy it in with a build step, and build with `--target-dir target/testees/rust/dev`. The lock
file is generated on the first build, so drop `--locked` for this version.

### JavaScript (`@stellar/stellar-sdk`)

The testee reads the version from the installed package's `package.json`, so an `npm pack`
tarball of the checkout works: one step runs the SDK's own build and `npm pack` in `../sdk`,
the next installs the tarball into `target/testees/js/dev` next to a copy of `testees/js/dist`,
as the steps for the pinned versions do with `npm ci`.

### Go (`go-stellar-sdk`)

The testee reads the SDK version from the binary's build info and refuses a module replaced by a
directory, because such a build has no version to report. A Go SDK repository therefore runs
the corpus against a version string the module system resolves: a release tag, or a commit's
pseudo-version (`go mod edit -require=github.com/stellar/go-stellar-sdk@<pseudo-version>` on a
copy of `testees/go/versions/<current>.mod`, built with `-modfile` as shapekit's own versions
file does). Running against an unpushed working tree is not supported by the Go testee.

## Reporting back

A failure in an SDK's run is either a bug in the SDK or a wrong corpus entry. Reproduce it with
`shapekit conformance run --sdk <sdk> --entry <entry-id>`; if the entry is wrong, open an issue
in the shapekit repository (see [maintenance.md](maintenance.md) for how such reports are
handled). Sibling issues for the other SDKs are drafted by `shapekit conformance draft-issues`.
