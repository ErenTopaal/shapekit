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
3. **A known-failures directory** holding `<sdk>.txt` (for example `python.txt`), one
   line per known failure:
   `<entry-id> <version|*> <result> <issue> <check>[, <check>...]`. The format is described in
   [testee-protocol.md](testee-protocol.md#known-failures). A failing check without a line
   fails the run. A listed check that now passes or fails with another result also fails the
   run, so the file stays current. A missing file counts as an empty list.
   [Starting the known-failures list](#starting-the-known-failures-list) shows how to begin
   with shapekit's own list.

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
same tag, so the workflow and the corpus come from one release. `sdk` is one of `rust`, `go`,
`js`, `python`, `java`, `kmp`, `soroban`, `dart`, `php`, `dotnet` and `stellar-cli`, the keys of
[conformance/versions.toml](../conformance/versions.toml); each SDK's section under
[Versions files per SDK](#versions-files-per-sdk) shows the workflow file for it.
`versions-file` and `known-failures` are paths in your repository. The optional input
`shapekit-repository` (default `ErenTopaal/shapekit`) names the repository the release is
downloaded from.

The job checks out the calling repository into `sdk/`, checks out shapekit at the tag into
`shapekit/` (the testee sources live there), downloads and verifies the two assets, and
extracts the corpus. The archive must hold exactly one `<dir>/manifest.json`. The job then sets
up the toolchain of the chosen SDK, and only that one, at the versions shapekit's own CI uses:

| `sdk` | toolchain |
|---|---|
| `rust`, `soroban`, `stellar-cli` | Rust 1.99.1 |
| `go` | Go 1.27.4 |
| `js` | Node 26.13.1, and the pnpm or yarn 1.x version the checkout's `package.json` names in `packageManager` |
| `python` | uv 0.13.9 and CPython 3.15.2 (`scripts/python-toolchain.sh`) |
| `java`, `kmp` | Temurin JDK 27.0.1 and Maven 3.10.1 on `PATH` (`scripts/java-toolchain.sh`), and `JAVA_HOME` set to Temurin JDK 21.0.13.1 for the SDK's Gradle build (`scripts/java-toolchain.sh --gradle-jdk`) |
| `dart` | Flutter 3.50.0 with its Dart 3.14.0 (`scripts/flutter-toolchain.sh`) |
| `php` | PHP 8.5 with the bcmath, gmp, mbstring and sodium extensions, and Composer 2.11.2 |
| `dotnet` | .NET SDK 11.0.100 |

The scripts download each archive named in [pins.toml](../pins.toml) and check it against its
sha256. The job has 30 minutes, 60 for `stellar-cli`, whose release build from the checkout
takes longest. It then runs, from `shapekit/`:

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

Each section ends with the workflow file for that SDK. Its `with:` block differs from the one
above only in `sdk`.

The Python, Java and KMP testees look SDK types up in a table generated per SDK version, and
refuse a version the table does not list. Their versions files below copy the testee sources
under `target/` and write a table for the checkout's version there with the testee's own
generator, so a version bump in the checkout needs no new shapekit release. The Dart and PHP
testees generate their tables in every build.

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

```yaml title=".github/workflows/shapekit.yml"
name: shapekit conformance
on: [pull_request]
permissions:
  contents: read
jobs:
  corpus:
    uses: ErenTopaal/shapekit/.github/workflows/sdk-conformance.yml@<release tag>
    with:
      sdk: rust
      shapekit-tag: <release tag>
      corpus-asset: corpus-<version>.tar.zst
      corpus-sha256: <sha256 of corpus-<version>.tar.zst from SHA256SUMS>
      shapekit-sha256: <sha256 of shapekit-linux-x86_64 from SHA256SUMS>
      versions-file: ci/shapekit/versions.toml
      known-failures: ci/shapekit/known-failures
```

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

```yaml title=".github/workflows/shapekit.yml"
name: shapekit conformance
on: [pull_request]
permissions:
  contents: read
jobs:
  corpus:
    uses: ErenTopaal/shapekit/.github/workflows/sdk-conformance.yml@<release tag>
    with:
      sdk: go
      shapekit-tag: <release tag>
      corpus-asset: corpus-<version>.tar.zst
      corpus-sha256: <sha256 of corpus-<version>.tar.zst from SHA256SUMS>
      shapekit-sha256: <sha256 of shapekit-linux-x86_64 from SHA256SUMS>
      versions-file: ci/shapekit/versions.toml
      known-failures: ci/shapekit/known-failures
```

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

```yaml title=".github/workflows/shapekit.yml"
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

### Python (`stellar-sdk`, py-stellar-base): install from the checkout

The testee reports the version of the installed distribution, which py-stellar-base reads from
`__version__` in `stellar_sdk/__version__.py`. The build makes a venv with the pinned CPython,
installs the checkout into it with `uv pip install`, copies the testee to
`target/testees/python/dev/testee/` and writes the type table for the checkout's version there:

```toml title="ci/shapekit/versions.toml"
schema = 1

[sdk.python]
package = "stellar-sdk"
current = "17.0.1"            # __version__ in the checkout's stellar_sdk/__version__.py
issues_repo = "StellarCN/py-stellar-base"
timeout_ms = 15000
memory = { resource = "as", mib = 4096 }

[[sdk.python.versions]]
version = "17.0.1"
build = [
  { argv = ["uv", "venv", "--quiet", "--clear", "--no-python-downloads", "--python", "3.15.2", "target/testees/python/dev/venv"] },
  { argv = ["uv", "pip", "install", "--quiet", "--python", "target/testees/python/dev/venv/bin/python", "../sdk"] },
  { argv = ["rm", "-rf", "target/testees/python/dev/testee"] },
  { argv = ["mkdir", "-p", "target/testees/python/dev/testee"] },
  { argv = ["cp", "-R", "testees/python/main.py", "testees/python/shapekit_testee", "target/testees/python/dev/testee/"] },
  { argv = ["target/testees/python/dev/venv/bin/python", "-I", "testees/python/gentypes.py", "--version", "17.0.1", "--venv", "target/testees/python/dev/venv", "--out", "target/testees/python/dev/testee/shapekit_testee/types_gen.py"] },
]
command = ["target/testees/python/dev/venv/bin/python", "-B", "-E", "-s", "target/testees/python/dev/testee/main.py"]
max_protocol = 30
```

`uv pip install` resolves the checkout's own dependencies from PyPI at build time; the pinned
versions use hash-locked requirement files instead. The testee runs on the API of the
released versions (`StrKey` and the generated classes of `stellar_sdk.xdr`) and on the API of
py-stellar-base's default branch (the classes of `stellar-strkey` and `stellar-xdr`, which
`stellar_sdk.strkey` and `stellar_sdk.xdr` re-export), and picks the one the installed SDK has.
The default branch still reports `__version__` 17.0.1, so `current`, `version` and the
`--version` of the type table stay `17.0.1` until the checkout changes it.

Started from the 17.0.1 list, a run of the default branch asks you to remove the lines of the
failures the branch fixes, and names two kinds of failure 17.0.1 does not have:

- `decode_json` of a transaction with an ed25519 signed payload signer whose payload is empty:
  `stellar-xdr` refuses to write it as SEP-51 JSON, though the network accepts it.
- `decode_encode` of an `ScMap` nested one level deeper than the network accepts: `stellar-xdr`
  decodes to a depth of 500 by default. The SDK takes no length limit, so the testee does not
  pass the runner's limits and decodes with the defaults (see the Python testee in
  [testee-protocol.md](testee-protocol.md#the-python-testee)).

```yaml title=".github/workflows/shapekit.yml"
name: shapekit conformance
on: [pull_request]
permissions:
  contents: read
jobs:
  corpus:
    uses: ErenTopaal/shapekit/.github/workflows/sdk-conformance.yml@<release tag>
    with:
      sdk: python
      shapekit-tag: <release tag>
      corpus-asset: corpus-<version>.tar.zst
      corpus-sha256: <sha256 of corpus-<version>.tar.zst from SHA256SUMS>
      shapekit-sha256: <sha256 of shapekit-linux-x86_64 from SHA256SUMS>
      versions-file: ci/shapekit/versions.toml
      known-failures: ci/shapekit/known-failures
```

### Java (`network.lightsail:stellar-sdk`, java-stellar-sdk): a jar from the Gradle build

The testee reads the SDK version from the file name of the jar it loaded the SDK from,
`stellar-sdk-<version>.jar`. The build runs the checkout's Gradle build for the jar and its
POM, installs both into a Maven repository of its own under `target/testees/java/m2`, so a
released jar of the same version in `~/.m2` is never replaced, and builds the testee from a copy
of its sources with a type table for the checkout's version. Keep a copy of
`testees/java/versions/<current>/pom.xml` in the SDK repository, for example as
`ci/shapekit/pom.xml`, with these lines changed:

```xml title="ci/shapekit/pom.xml"
    <directory>${project.basedir}/build</directory>
    <sourceDirectory>${project.basedir}/src/main/java</sourceDirectory>
    <testSourceDirectory>${project.basedir}/src/test/java</testSourceDirectory>
```

and the `<version>` of the `network.lightsail:stellar-sdk` dependency set to the checkout's
version. The versions file:

```toml title="ci/shapekit/versions.toml"
schema = 1

[sdk.java]
package = "network.lightsail:stellar-sdk"
current = "6.0.0"             # the version in the checkout's build.gradle.kts
issues_repo = "lightsail-network/java-stellar-sdk"
timeout_ms = 15000
memory = { resource = "data", mib = 4096 }

[[sdk.java.versions]]
version = "6.0.0"
build = [
  { argv = ["./gradlew", "--no-daemon", "--quiet", "jar", "generatePomFileForMavenJavaPublication"], dir = "../sdk" },
  { argv = ["rm", "-rf", "target/testees/java/dev"] },
  { argv = ["mkdir", "-p", "target/testees/java/dev/src/main"] },
  { argv = ["cp", "-R", "testees/java/src/main/java", "target/testees/java/dev/src/main/java"] },
  { argv = ["cp", "../sdk/ci/shapekit/pom.xml", "target/testees/java/dev/pom.xml"] },
  { argv = ["python3", "-I", "gentypes.py", "--version", "6.0.0", "--jar", "../../../sdk/build/libs/stellar-sdk-6.0.0.jar", "--out", "../../target/testees/java/dev/src/main/java/org/shapekit/testee/TypesGen.java"], dir = "testees/java" },
  { argv = ["mvn", "-B", "-q", "-Dmaven.repo.local=target/testees/java/m2", "install:install-file", "-Dfile=../sdk/build/libs/stellar-sdk-6.0.0.jar", "-DpomFile=../sdk/build/publications/mavenJava/pom-default.xml"], env = { JAVA_HOME = "" } },
  { argv = ["mvn", "-B", "-q", "-Dmaven.repo.local=target/testees/java/m2", "-f", "target/testees/java/dev/pom.xml", "-DskipTests", "package"], env = { JAVA_HOME = "" } },
]
command = ["java", "-jar", "target/testees/java/dev/build/testee.jar"]
max_protocol = 30
```

`gradlew` runs on `JAVA_HOME`, the JDK 21 that java-stellar-sdk's Gradle toolchain asks for.
The two Maven steps clear `JAVA_HOME`, so Maven compiles the testee with the JDK 27 on `PATH`:
the testee's build treats compiler warnings as errors, and the JDK 21 compiler warns about the
SDK's gson annotations, which the testee does not link.

```yaml title=".github/workflows/shapekit.yml"
name: shapekit conformance
on: [pull_request]
permissions:
  contents: read
jobs:
  corpus:
    uses: ErenTopaal/shapekit/.github/workflows/sdk-conformance.yml@<release tag>
    with:
      sdk: java
      shapekit-tag: <release tag>
      corpus-asset: corpus-<version>.tar.zst
      corpus-sha256: <sha256 of corpus-<version>.tar.zst from SHA256SUMS>
      shapekit-sha256: <sha256 of shapekit-linux-x86_64 from SHA256SUMS>
      versions-file: ci/shapekit/versions.toml
      known-failures: ci/shapekit/known-failures
```

### Kotlin Multiplatform (`com.soneso.stellar:stellar-sdk-jvm`, kmp-stellar-sdk): the JVM jar

The testee runs the JVM target of the SDK and reads the version the SDK reports for itself. The
build has the Java testee's shape: the checkout's Gradle build writes the JVM jar and its POM,
both go into a Maven repository under `target/testees/kmp/m2`, and the testee builds from a copy
of its sources with a type table for the checkout's version. Gradle configures every project of
kmp-stellar-sdk's build, the Android and demo apps included, but building the JVM jar needs no
Android SDK. Keep a copy of `testees/kmp/versions/<current>/pom.xml` as `ci/shapekit/pom.xml`,
with these lines changed:

```xml title="ci/shapekit/pom.xml"
    <directory>${project.basedir}/build</directory>
    <sourceDirectory>${project.basedir}/src/main/kotlin</sourceDirectory>
    <testSourceDirectory>${project.basedir}/src/test/kotlin</testSourceDirectory>
```

and the `<version>` of the `com.soneso.stellar:stellar-sdk-jvm` dependency set to the checkout's
version. The versions file:

```toml title="ci/shapekit/versions.toml"
schema = 1

[sdk.kmp]
package = "com.soneso.stellar:stellar-sdk-jvm"
current = "1.18.0"            # the version in the checkout's gradle.properties
issues_repo = "Soneso/kmp-stellar-sdk"
timeout_ms = 15000
memory = { resource = "data", mib = 4096 }

[[sdk.kmp.versions]]
version = "1.18.0"
build = [
  { argv = ["./gradlew", "--no-daemon", "--quiet", ":stellar-sdk:jvmJar", ":stellar-sdk:generatePomFileForJvmPublication"], dir = "../sdk" },
  { argv = ["rm", "-rf", "target/testees/kmp/dev"] },
  { argv = ["mkdir", "-p", "target/testees/kmp/dev/src/main"] },
  { argv = ["cp", "-R", "testees/kmp/src/main/kotlin", "target/testees/kmp/dev/src/main/kotlin"] },
  { argv = ["cp", "../sdk/ci/shapekit/pom.xml", "target/testees/kmp/dev/pom.xml"] },
  { argv = ["python3", "-I", "gentypes.py", "--version", "1.18.0", "--jar", "../../../sdk/stellar-sdk/build/libs/stellar-sdk-jvm-1.18.0.jar", "--out", "../../target/testees/kmp/dev/src/main/kotlin/org/shapekit/testee/TypesGen.kt"], dir = "testees/kmp" },
  { argv = ["mvn", "-B", "-q", "-Dmaven.repo.local=target/testees/kmp/m2", "install:install-file", "-Dfile=../sdk/stellar-sdk/build/libs/stellar-sdk-jvm-1.18.0.jar", "-DpomFile=../sdk/stellar-sdk/build/publications/jvm/pom-default.xml"], env = { JAVA_HOME = "" } },
  { argv = ["mvn", "-B", "-q", "-Dmaven.repo.local=target/testees/kmp/m2", "-f", "target/testees/kmp/dev/pom.xml", "-DskipTests", "package"], env = { JAVA_HOME = "" } },
]
command = ["java", "-jar", "target/testees/kmp/dev/build/testee.jar"]
max_protocol = 30
```

`JAVA_HOME` is handled as for Java: Gradle runs on JDK 21, Maven on the JDK 27 on `PATH`.

```yaml title=".github/workflows/shapekit.yml"
name: shapekit conformance
on: [pull_request]
permissions:
  contents: read
jobs:
  corpus:
    uses: ErenTopaal/shapekit/.github/workflows/sdk-conformance.yml@<release tag>
    with:
      sdk: kmp
      shapekit-tag: <release tag>
      corpus-asset: corpus-<version>.tar.zst
      corpus-sha256: <sha256 of corpus-<version>.tar.zst from SHA256SUMS>
      shapekit-sha256: <sha256 of shapekit-linux-x86_64 from SHA256SUMS>
      versions-file: ci/shapekit/versions.toml
      known-failures: ci/shapekit/known-failures
```

### soroban-sdk: a path dependency

The testee reports the `soroban-sdk`, `soroban-env-host` and `stellar-xdr` versions of the
`Cargo.lock` next to its manifest. As for Rust, keep a copy of
`testees/soroban/versions/<current>/Cargo.toml`, for example as `ci/shapekit/Cargo.toml`, with
the `soroban-sdk` line changed to a path:

```toml title="ci/shapekit/Cargo.toml"
soroban-sdk = { path = "../../../../../sdk/soroban-sdk", features = ["testutils"] }
```

The path is relative to `shapekit/testees/soroban/versions/dev/`, where the build copies it. The
versions file:

```toml title="ci/shapekit/versions.toml"
schema = 1

[sdk.soroban]
package = "soroban-sdk"
current = "30.0.0-rc.1"       # the workspace version in the checkout's Cargo.toml
issues_repo = "stellar/rs-soroban-sdk"
timeout_ms = 10000
memory = { resource = "as", mib = 4096 }

[[sdk.soroban.versions]]
version = "30.0.0-rc.1"
build = [
  { argv = ["mkdir", "-p", "testees/soroban/versions/dev"] },
  { argv = ["cp", "../sdk/ci/shapekit/Cargo.toml", "testees/soroban/versions/dev/Cargo.toml"] },
  { argv = ["cargo", "build", "--release", "--manifest-path", "testees/soroban/versions/dev/Cargo.toml", "--target-dir", "target/testees/soroban/dev"] },
]
command = ["target/testees/soroban/dev/release/shapekit-testee-soroban"]
max_protocol = 30
```

The build runs without `--locked` and writes the lock file the testee reads its versions from.
The checkout's own `[patch]` and lock file do not apply, since the testee's manifest is the root
of the build.

```yaml title=".github/workflows/shapekit.yml"
name: shapekit conformance
on: [pull_request]
permissions:
  contents: read
jobs:
  corpus:
    uses: ErenTopaal/shapekit/.github/workflows/sdk-conformance.yml@<release tag>
    with:
      sdk: soroban
      shapekit-tag: <release tag>
      corpus-asset: corpus-<version>.tar.zst
      corpus-sha256: <sha256 of corpus-<version>.tar.zst from SHA256SUMS>
      shapekit-sha256: <sha256 of shapekit-linux-x86_64 from SHA256SUMS>
      versions-file: ci/shapekit/versions.toml
      known-failures: ci/shapekit/known-failures
```

### Dart (`stellar_flutter_sdk`): a pub path dependency

The testee reports `StellarSDK.versionNumber`. Keep a pubspec whose `stellar_flutter_sdk`
dependency is a path, for example as `ci/shapekit/pubspec.yaml`:

```yaml title="ci/shapekit/pubspec.yaml"
name: shapekit_testee_dart
publish_to: none
environment:
  sdk: ">=3.8.0 <4.0.0"
dependencies:
  stellar_flutter_sdk:
    path: ../../../../../sdk
dev_dependencies:
  test: ^1.26.0
```

The path is relative to `shapekit/target/testees/dart/dev/`, where the build copies the testee
sources and this file. The versions file:

```toml title="ci/shapekit/versions.toml"
schema = 1

[sdk.dart]
package = "stellar_flutter_sdk"
current = "3.12.0"            # StellarSDK.versionNumber in the checkout's lib/src/stellar_sdk.dart
issues_repo = "Soneso/stellar_flutter_sdk"
timeout_ms = 10000
memory = { resource = "data", mib = 4096 }

[[sdk.dart.versions]]
version = "3.12.0"
build = [
  { argv = ["rm", "-rf", "target/testees/dart/dev"] },
  { argv = ["mkdir", "-p", "target/testees/dart/dev"] },
  { argv = ["cp", "-R", "testees/dart/bin", "testees/dart/lib", "testees/dart/test", "testees/dart/tool", "../sdk/ci/shapekit/pubspec.yaml", "target/testees/dart/dev/"] },
  { argv = ["flutter", "pub", "get"], dir = "target/testees/dart/dev" },
  { argv = ["dart", "run", "tool/gentypes.dart", "../../../../schema/xdr/v30.0/xdr.json", "lib/tables_gen.dart"], dir = "target/testees/dart/dev" },
  { argv = ["dart", "compile", "exe", "bin/main.dart", "-o", "testee"], dir = "target/testees/dart/dev" },
]
command = ["target/testees/dart/dev/testee"]
max_protocol = 30
```

There is no lock file, so `flutter pub get` resolves the checkout's dependencies at build time.

```yaml title=".github/workflows/shapekit.yml"
name: shapekit conformance
on: [pull_request]
permissions:
  contents: read
jobs:
  corpus:
    uses: ErenTopaal/shapekit/.github/workflows/sdk-conformance.yml@<release tag>
    with:
      sdk: dart
      shapekit-tag: <release tag>
      corpus-asset: corpus-<version>.tar.zst
      corpus-sha256: <sha256 of corpus-<version>.tar.zst from SHA256SUMS>
      shapekit-sha256: <sha256 of shapekit-linux-x86_64 from SHA256SUMS>
      versions-file: ci/shapekit/versions.toml
      known-failures: ci/shapekit/known-failures
```

### PHP (`soneso/stellar-php-sdk`): a Composer path repository

The testee reports the version Composer installed. stellar-php-sdk's `composer.json` holds no
version, so Composer would derive one from the checkout's git state: `dev-main` on a branch,
`dev-<commit>` on the detached checkout of a pull request. The path repository's `versions`
option sets it instead. Keep this file, for example as
`ci/shapekit/composer.json`:

```json title="ci/shapekit/composer.json"
{
    "name": "shapekit/testee-php",
    "type": "project",
    "license": "Apache-2.0",
    "repositories": [
        {
            "type": "path",
            "url": "../../../../../sdk",
            "options": {"symlink": false, "versions": {"soneso/stellar-php-sdk": "1.18.0"}}
        }
    ],
    "require": {
        "php": ">=8.1",
        "soneso/stellar-php-sdk": "1.18.0"
    },
    "autoload": {
        "psr-4": {"Shapekit\\Testee\\": "src/"}
    }
}
```

The path is relative to `shapekit/target/testees/php/dev/`. Both version strings are the one in
the versions file:

```toml title="ci/shapekit/versions.toml"
schema = 1

[sdk.php]
package = "soneso/stellar-php-sdk"
current = "1.18.0"            # StellarSDK::VERSION_NR in the checkout
issues_repo = "Soneso/stellar-php-sdk"
timeout_ms = 10000
memory = { resource = "as", mib = 4096 }

[[sdk.php.versions]]
version = "1.18.0"
build = [
  { argv = ["rm", "-rf", "target/testees/php/dev"] },
  { argv = ["mkdir", "-p", "target/testees/php/dev"] },
  { argv = ["cp", "-R", "testees/php/bin", "testees/php/src", "testees/php/tests", "testees/php/tools", "../sdk/ci/shapekit/composer.json", "target/testees/php/dev/"] },
  { argv = ["composer", "update", "--no-interaction", "--no-progress", "--no-dev"], dir = "target/testees/php/dev" },
  { argv = ["php", "tools/gentypes.php", "../../../../schema/xdr/v30.0/xdr.json", "src/TablesGen.php"], dir = "target/testees/php/dev" },
]
command = ["php", "target/testees/php/dev/bin/testee.php"]
max_protocol = 30
```

`composer update` resolves the checkout's dependencies at build time, since there is no lock
file.

```yaml title=".github/workflows/shapekit.yml"
name: shapekit conformance
on: [pull_request]
permissions:
  contents: read
jobs:
  corpus:
    uses: ErenTopaal/shapekit/.github/workflows/sdk-conformance.yml@<release tag>
    with:
      sdk: php
      shapekit-tag: <release tag>
      corpus-asset: corpus-<version>.tar.zst
      corpus-sha256: <sha256 of corpus-<version>.tar.zst from SHA256SUMS>
      shapekit-sha256: <sha256 of shapekit-linux-x86_64 from SHA256SUMS>
      versions-file: ci/shapekit/versions.toml
      known-failures: ci/shapekit/known-failures
```

### .NET (`stellar-dotnet-sdk`): a project reference

The testee reports the SDK assembly's informational version, which comes from `<Version>` in
`StellarDotnetSdk/StellarDotnetSdk.csproj`. Keep a project file that references the checkout's
project, for example as `ci/shapekit/ShapekitTestee.csproj`:

```xml title="ci/shapekit/ShapekitTestee.csproj"
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net11.0</TargetFramework>
    <Nullable>enable</Nullable>
    <ImplicitUsings>enable</ImplicitUsings>
    <TreatWarningsAsErrors>true</TreatWarningsAsErrors>
    <InvariantGlobalization>true</InvariantGlobalization>
    <RootNamespace>Shapekit.Testee</RootNamespace>
  </PropertyGroup>
  <ItemGroup>
    <Compile Include="../../Program.cs" />
  </ItemGroup>
  <ItemGroup>
    <ProjectReference Include="../../../../../sdk/StellarDotnetSdk/StellarDotnetSdk.csproj" />
  </ItemGroup>
</Project>
```

The paths are relative to `shapekit/testees/dotnet/versions/dev/`, where the build copies it.
The versions file:

```toml title="ci/shapekit/versions.toml"
schema = 1

[sdk.dotnet]
package = "stellar-dotnet-sdk"
current = "17.0.0"            # <Version> in the checkout's StellarDotnetSdk/StellarDotnetSdk.csproj
issues_repo = "Beans-BV/dotnet-stellar-sdk"
timeout_ms = 10000
memory = { resource = "data", mib = 4096 }

[[sdk.dotnet.versions]]
version = "17.0.0"
build = [
  { argv = ["mkdir", "-p", "testees/dotnet/versions/dev"] },
  { argv = ["cp", "../sdk/ci/shapekit/ShapekitTestee.csproj", "testees/dotnet/versions/dev/ShapekitTestee.csproj"] },
  { argv = ["dotnet", "build", "testees/dotnet/versions/dev/ShapekitTestee.csproj", "-c", "Release", "-o", "target/testees/dotnet/dev"] },
]
command = ["dotnet", "target/testees/dotnet/dev/ShapekitTestee.dll"]
max_protocol = 30
```

The build starts from `shapekit/`, so the checkout's `global.json` does not select the .NET
SDK: the SDK project builds for its `net10.0` target with .NET SDK 11.0.100. Its warnings are
not errors; only the testee's are.

```yaml title=".github/workflows/shapekit.yml"
name: shapekit conformance
on: [pull_request]
permissions:
  contents: read
jobs:
  corpus:
    uses: ErenTopaal/shapekit/.github/workflows/sdk-conformance.yml@<release tag>
    with:
      sdk: dotnet
      shapekit-tag: <release tag>
      corpus-asset: corpus-<version>.tar.zst
      corpus-sha256: <sha256 of corpus-<version>.tar.zst from SHA256SUMS>
      shapekit-sha256: <sha256 of shapekit-linux-x86_64 from SHA256SUMS>
      versions-file: ci/shapekit/versions.toml
      known-failures: ci/shapekit/known-failures
```

### stellar-cli: the `stellar` binary built from the checkout

The testee is a wrapper that runs the `stellar` binary given as its first argument, and reports
the version `stellar version` prints. For a release, the build fetches the release binary; for
the checkout, it builds `stellar` with Cargo:

```toml title="ci/shapekit/versions.toml"
schema = 1

[sdk.stellar-cli]
package = "stellar-cli"
current = "30.0.1"            # the workspace version in the checkout's Cargo.toml
issues_repo = "stellar/stellar-cli"
timeout_ms = 30000
memory = { resource = "as", mib = 4096 }
kind = "cli"

[[sdk.stellar-cli.versions]]
version = "30.0.1"
build = [
  { argv = ["cargo", "build", "--release", "--locked", "--manifest-path", "testees/stellar-cli/Cargo.toml", "--target-dir", "target/testees/stellar-cli/testee"] },
  { argv = ["cargo", "build", "--release", "--locked", "--no-default-features", "--manifest-path", "../sdk/Cargo.toml", "--package", "stellar-cli", "--bin", "stellar", "--target-dir", "target/testees/stellar-cli/dev"] },
]
command = ["target/testees/stellar-cli/testee/release/shapekit-testee-stellar-cli", "target/testees/stellar-cli/dev/release/stellar"]
max_protocol = 30
```

`--locked` builds `stellar` with the checkout's own lock file. `--no-default-features` leaves
out the `additional-libs` feature (ledger and keyring support and wasm-opt), which the `xdr`,
`strkey` and `tx hash` commands the testee runs do not use, and which need the libudev and
libdbus development packages installed first.

```yaml title=".github/workflows/shapekit.yml"
name: shapekit conformance
on: [pull_request]
permissions:
  contents: read
jobs:
  corpus:
    uses: ErenTopaal/shapekit/.github/workflows/sdk-conformance.yml@<release tag>
    with:
      sdk: stellar-cli
      shapekit-tag: <release tag>
      corpus-asset: corpus-<version>.tar.zst
      corpus-sha256: <sha256 of corpus-<version>.tar.zst from SHA256SUMS>
      shapekit-sha256: <sha256 of shapekit-linux-x86_64 from SHA256SUMS>
      versions-file: ci/shapekit/versions.toml
      known-failures: ci/shapekit/known-failures
```

## Starting the known-failures list

Begin with shapekit's own list for the SDK, `conformance/known-failures/<sdk>.txt` at the
release tag: the lines of the SDK's `current` version in that release's
[conformance/versions.toml](../conformance/versions.toml) and the `*` lines, each rewritten to
`*`. From a shapekit checkout at the tag, for Python with current version 17.0.1:

```sh
mkdir -p ../sdk/ci/shapekit/known-failures
awk -v v=17.0.1 '!/^#/ && NF && ($2 == v || $2 == "*") { $2 = "*"; print }' \
  conformance/known-failures/python.txt > ../sdk/ci/shapekit/known-failures/python.txt
```

Then run the corpus as the workflow does (see [Running it locally](#running-it-locally)). When
the checkout differs from the release, the run fails and names each line to change:

- ``<entry> on <version>: `<check>` is listed as a failure (line <n>) but is pass; remove it
  from the line``: the checkout fixed that check.
- ``<entry> on <version>: `<check>` is <result> and not in the known-failures list``: the
  checkout fails a check the release passed. Add a line with the issue that tracks it.

`shapekit report <out> --manifest <corpus>/manifest.json --known-failures <dir>` lists the
same differences as new, changed and fixed failures. The soroban-sdk testee has no known
failures, so its list starts empty.

## Running it locally

Clone shapekit at the release tag and the SDK next to each other as `shapekit/` and `sdk/`, so
`../sdk` in the versions file resolves as in CI, put the toolchain from the table above on
`PATH`, and run from `shapekit/`:

```sh
shapekit conformance run --sdk <sdk> --versions-file ../sdk/ci/shapekit/versions.toml \
  --known-failures ../sdk/ci/shapekit/known-failures --out target/conformance-dev
```

The toolchain scripts print what to export, for example
`export PATH="$(scripts/python-toolchain.sh):$PATH"`; for Java and KMP also
`export JAVA_HOME="$(scripts/java-toolchain.sh --gradle-jdk)"`. Clone the SDK as a directory;
the .NET build fails when `sdk/` is a symbolic link, because MSBuild then reaches the SDK's
projects by two different paths.

## Reporting back

A failure in an SDK's run is either a bug in the SDK or a wrong corpus entry. Reproduce it in a
shapekit checkout with `shapekit conformance run --sdk <sdk> --entry <entry-id>`. If the entry
is wrong, open an issue in the shapekit repository ([maintenance.md](maintenance.md#triage)
describes how such reports are handled). `shapekit conformance draft-issues` writes one issue
body per SDK and diverging entry from a divergence matrix, and each body lists the other SDKs
that diverge on the same entry.
