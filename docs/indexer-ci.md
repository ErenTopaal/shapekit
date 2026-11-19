# Running the corpus in an indexer's CI

An indexer repository replays the chains of a pinned shapekit corpus release into the indexer it
builds, on every pull request, and gets the harness verdicts for each corpus entry. This page
describes what the indexer repository keeps and the reusable workflow that does the run. It is
the indexer counterpart of [running the corpus in an SDK's CI](sdk-ci.md).

## What the indexer repository keeps

1. **A pinned release.** The shapekit release tag, the corpus archive name and two sha256 values
   from the release's `SHA256SUMS`: the corpus archive and the `shapekit-linux-x86_64` binary.
   The workflow fails when a downloaded asset does not match its sha256. To move to a newer
   release, change these four values and the tag in `uses:`.
2. **An adapter**, the program that starts the indexer, points it at the harness's ledger
   delivery endpoints and reads back what it stored. It is described in
   [adapter-interface.md](adapter-interface.md). `shapekit init adapter --lang go|ts|python`
   writes a skeleton to start from.
3. **Optionally, an expected-verdicts file**, written by `shapekit harness expect --report
   <out>/harness-report.json --write <file>` from a run you reviewed. The `expect-mode` input
   decides how the run is held to it (see
   [Failing a pull request on a worse verdict](#failing-a-pull-request-on-a-worse-verdict)).
   Without it, `mishandled` and `dropped` verdicts are reported but do not fail the run; it
   fails when a chain cannot be run (an adapter or target error), a dump fails, or a memory
   check of the run fails.

## Calling the workflow

```yaml title=".github/workflows/shapekit.yml"
# .github/workflows/shapekit.yml in the indexer repository
name: shapekit replay
on: [pull_request]
permissions:
  contents: read
jobs:
  corpus:
    uses: ErenTopaal/shapekit/.github/workflows/indexer-conformance.yml@<release tag>
    with:
      adapter: bin/my-adapter
      setup: go build -o bin/my-adapter ./cmd/my-adapter
      go-version: "1.27.4"
      shapekit-tag: <release tag>
      corpus-asset: corpus-<version>.tar.zst
      corpus-sha256: <sha256 of corpus-<version>.tar.zst from SHA256SUMS>
      shapekit-sha256: <sha256 of shapekit-linux-x86_64 from SHA256SUMS>
      expect: ci/shapekit/expected-verdicts.toml
      expect-mode: regressions
```

The values in angle brackets come from the release page. Reference the reusable workflow by the
same tag, so the workflow and the corpus come from one release.

| input | required | meaning |
|---|---|---|
| `adapter` | one of `adapter` and `mapping` | the adapter command line, run from the root of your repository |
| `mapping` | one of `adapter` and `mapping` | path, in your repository, of a mapping file that connects a PostgreSQL indexer with no adapter program ([mapping-adapter.md](mapping-adapter.md)); needs a shapekit release that has `--mapping` |
| `vars` | no | variables of the mapping file, `NAME=VALUE` separated by spaces, for example `commit=<full commit hash>` |
| `setup` | no | shell commands run from the root of your repository before the replay: build the adapter, build or pull the images the indexer needs |
| `shapekit-tag`, `corpus-asset`, `corpus-sha256`, `shapekit-sha256` | yes | the pinned release |
| `chains` | no | chain ids to replay, separated by spaces; every chain of the corpus when empty |
| `families` | no | families whose chains to replay, separated by spaces; with `chains`, a chain must match both |
| `network` | no | `testnet` (default), `mainnet` or `both` |
| `delivery` | no | delivery mode to ask the adapter for: `sep54-filesystem`, `sep54-s3`, `sep54-gcs` or `rpc`; the adapter's default when empty |
| `expect` | no | path, in your repository, of an expected-verdicts file |
| `expect-mode` | no | `exact` (default): any difference from `expect` fails, and the run must be of the build the file was written at. `regressions`: any build; only a worse verdict fails, see below |
| `shards` | no | the number of jobs the replay is split across, 1 (one job) by default; see [Splitting the replay across jobs](#splitting-the-replay-across-jobs) |
| `go-version`, `node-version`, `python-version` | no | toolchains to install before `setup`; none by default |
| `timeout-minutes` | no | time limit of the job, 120 by default |
| `shapekit-repository` | no | the repository the release comes from, `ErenTopaal/shapekit` by default |
| `baseline` | no | path, in your repository, of an earlier `harness-report.json` or an expected-verdicts file the report compares the run with; `expect` when empty |
| `comment` | no | `true` to post the run's summary as a pull request comment (see below); `false` by default |

The job checks out your repository into `indexer/` and shapekit at the release tag into
`shapekit/` (the findings triage the report links to is read from there), downloads and verifies the two assets,
extracts the corpus, installs the toolchains you named, runs `setup`, and then runs from
`indexer/`:

```sh
shapekit check indexer --adapter "<adapter>" --corpus <corpus> --network <network> \
  --out "$RUNNER_TEMP/harness" [--chain <id>]... [--family <family>]... \
  [--delivery <mode>] [--expect <expect> --expect-mode <expect-mode>]
```

With `mapping`, `--adapter "<adapter>"` is `--mapping <mapping>` followed by one
`--var NAME=VALUE` for each of `vars`.

The runner is `ubuntu-24.04`, which has Docker. The harness creates an internal Docker network
for each chain and the adapter attaches the indexer's containers to it, as
[adapter-interface.md](adapter-interface.md#transport) describes. Indexer checks run on Linux
only: on macOS, Docker runs in a VM whose networks the host cannot reach (see
[Network access](adapter-interface.md#network-access)).

After the run, whether it passed or failed, the job runs [`shapekit report`](report.md) on it,
from `shapekit/`, with `baseline` (or `expect`) as the baseline when one is given:

```sh
shapekit report "$RUNNER_TEMP/harness" --manifest <corpus>/manifest.json \
  --out "$RUNNER_TEMP/harness/report.html" --summary "$RUNNER_TEMP/harness/summary.md" \
  [--baseline "$GITHUB_WORKSPACE/indexer/<baseline>"]
```

The summary (with a baseline: new, changed, fixed and unchanged failures; and the failures
grouped by finding) and then `HARNESS.md` go to the job summary. `harness-report.json`,
`HARNESS.md`, `report.html`, `summary.md` and the logs are uploaded as an artifact named
`shapekit-harness`, kept for 30 days; the workflow output `report-artifact` holds that name.
Open `report.html` from the downloaded artifact for each group's finding, guide sections and
reproduce command.

## Failing a pull request on a worse verdict

An expected-verdicts file records every entry's verdict and reason, and the target build the
run was of (`target_version`; the commit, for the wallet-backend and stellar-etl adapters).
With `expect-mode: exact` the run fails on any difference, including another build, so a
pull request, whose build is a new commit, always fails. Use `exact` for a fixed build, such
as a nightly run of a pinned commit.

For pull requests, commit the file on your default branch and set `expect-mode:
regressions`. The run is then held to the file at any build of the same target:

| the file says | the run gets | result |
|---|---|---|
| `pass` or `out-of-scope` | `mishandled`, `dropped` or `not-evaluated` | fails: a new failure |
| a failure | another failing verdict, or the same verdict with another reason | fails: a changed failure |
| a failure | `pass` or `out-of-scope` | passes; the job log lists it as fixed |
| an entry | nothing, because the entry was not in the run | fails |
| nothing | a failure | fails: a new failure |
| nothing, or another non-failing verdict | `pass` or `out-of-scope` | passes; the job log lists it |

The job log of the replay step (or of the merge job, with `shards`) prints one
`expectation:` line per failure and one `expectation note:` line per fix or other change.
The report compares the run with the same file, so the job summary and the pull request
comment list the new, changed and fixed failures by entry. A failure the file lists and the
run still gets is counted as unchanged.

The workflow reads the file from the checkout of the pull request, which is your default
branch with the pull request's changes. A pull request that fixes an entry passes without
touching the file. After it merges, regenerate the file from a run of the default branch, so
the fix becomes the new floor and a later return of the failure fails:

```sh
shapekit harness expect --report <artifact>/harness-report.json --write ci/shapekit/expected-verdicts.toml
```

A pull request that changes the file shows the new expectations in its diff, so a reviewer
sees any verdict it accepts as worse.

## Splitting the replay across jobs

A replay of every chain of the corpus takes hours in one job. With `shards: <n>` the
workflow runs three kinds of jobs instead of the single `harness` job:

1. `plan` checks the inputs and lists the shard numbers.
2. `harness (<i>/<n>)`, one job per shard, each with the same checkout, toolchains and
   `setup`, replays its share of the selected chains with `--shard <i>/<n>`: the chains,
   sorted by name, are dealt out in turn, and both networks of a chain go to the same shard.
   Each uploads its output as the artifact `shapekit-harness-shard-<i>`, kept for one day.
3. `merge` puts the shard reports together with `shapekit harness merge` into the
   `harness-report.json` and `HARNESS.md` one job would have written, copies every shard's
   logs next to them, checks `expect` (in `expect-mode`) against the merged report, and then
   writes the report and uploads the artifact `shapekit-harness` as the single job does.

`timeout-minutes` applies to each shard job; the merge job has 15 minutes. A shard job fails
when a chain of its share cannot be run; the merge job still runs and fails when a shard's
report is missing. Each shard must get at least one chain, so set `shards` no higher than
the number of chains you select, and at most 64. With `comment: true` the merge job's summary
is posted. The single job, with `shards` left at 1, is unchanged.

[Measured durations](infrastructure.md#measured-durations) lists the time of whole-corpus runs
of each shapekit adapter, from which you can size `shards` and `timeout-minutes`.

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
    uses: ErenTopaal/shapekit/.github/workflows/indexer-conformance.yml@<release tag>
    with:
      adapter: bin/my-adapter
      comment: true
      # the other inputs as above
```

The comment is found again by its first line, `<!-- shapekit-report harness -->`, among the
comments of `github-actions[bot]`. A pull request from a fork runs with a read-only token, so
the comment job skips it; the job summary and the artifact are there as usual. Without
`comment` (the default) the workflow needs `contents: read` only.

## Indexers shapekit has an adapter for

shapekit's own adapters for wallet-backend, stellar-etl and stellar-rpc live in `adapters/`
of the shapekit checkout, which the workflow places at `shapekit/`, next to your repository's
`indexer/`. `setup` builds the adapter from there, and the adapter builds or runs the commit
under test. Each example checks three chains on pull requests against the expectations
committed on the default branch. Write that file once from a reviewed run of the default
branch (leave out `expect` for that run; its `harness-report.json` is in the artifact).

wallet-backend: the adapter builds the image from the checkout with the repository's
`Dockerfile` (`--repo .`) and runs it with TimescaleDB.

```yaml title=".github/workflows/shapekit.yml"
jobs:
  corpus:
    permissions:
      contents: read
      pull-requests: write
    uses: ErenTopaal/shapekit/.github/workflows/indexer-conformance.yml@<release tag>
    with:
      go-version: "1.27.4"
      setup: go build -C ../shapekit/adapters -o bin/wb-adapter ./walletbackend/cmd/wb-adapter
      adapter: ../shapekit/adapters/bin/wb-adapter --repo . --commit ${{ github.sha }}
      chains: cap27.muxed-accounts.p30 cap40.signed-payload-signer.p30 cap73.sac-trust.p30
      expect: ci/shapekit/expected-verdicts.toml
      expect-mode: regressions
      comment: true
      # shapekit-tag, corpus-asset, corpus-sha256 and shapekit-sha256 as above
```

stellar-etl: the same, with `./stellaretl/cmd/etl-adapter` built to `bin/etl-adapter`; the
adapter builds `docker/Dockerfile`.

stellar-rpc: `setup` builds the image from the checkout with the repository's Dockerfile, and
the adapter runs that image. The adapter checks that `getVersionInfo` answers the version and
commit it is given, so the build argument and `--version` must agree; the commit is read
from the `.git` directory of the build context.

```yaml title=".github/workflows/shapekit.yml"
jobs:
  corpus:
    permissions:
      contents: read
      pull-requests: write
    uses: ErenTopaal/shapekit/.github/workflows/indexer-conformance.yml@<release tag>
    with:
      go-version: "1.27.4"
      setup: |
        docker build -f cmd/stellar-rpc/docker/Dockerfile --build-arg REPOSITORY_VERSION=pr -t stellar-rpc:pr .
        go build -C ../shapekit/adapters -o bin/rpc-adapter ./stellarrpc/cmd/rpc-adapter
      adapter: ../shapekit/adapters/bin/rpc-adapter --image stellar-rpc:pr --version pr --commit ${{ github.sha }}
      chains: cap27.muxed-accounts.p30 cap40.signed-payload-signer.p30 cap73.sac-trust.p30
      expect: ci/shapekit/expected-verdicts.toml
      expect-mode: regressions
      comment: true
      # shapekit-tag, corpus-asset, corpus-sha256 and shapekit-sha256 as above
```

Measured for this stellar-rpc setup, on a shallow clone of stellar-rpc's `main`, with the
image build limited to 4 CPUs and 6 GB on a shared host: the image build (zstd, RocksDB, the
Rust libraries and the Go binary, from an empty build cache) took 935 s, 505 s of which was
the first `apt install`, and the replay of the three chains on testnet took 74 s, with all
35 entries passing. `getVersionInfo` answered `pr` at the checkout's commit, as the adapter
line expects. Without a build cache, plan for about 20 minutes per pull request.

## Version tables of the wallet-backend and stellar-etl adapters

The wallet-backend and stellar-etl adapters build the commit under test from source, and keep
two tables of what they know about such commits:

- `sdkProtocols` (in `adapters/walletbackend/source.go` and `adapters/stellaretl/build.go`):
  the newest ledger protocol each `go-stellar-sdk` version decodes. It sets the top of the
  protocol range the adapter reports, and chains of a newer protocol are judged
  `out-of-scope`.
- `BaseImages` (in `adapters/walletbackend/docker.go` and `adapters/stellaretl/build.go`):
  the digest, as `pins.toml` lists it, each `FROM` image of the indexer's Dockerfile is built
  on, so the tag a Dockerfile names never decides the bytes.

A pull request that bumps `go-stellar-sdk` or a base image past these tables still runs:

| the commit has | the adapter |
|---|---|
| a `go-stellar-sdk` version in `sdkProtocols` | uses its protocol |
| a newer version than a listed one | uses the protocol of the newest listed version before it. A `go-stellar-sdk` release decodes every protocol an older one does, so this is never more than the version decodes; chains of a newer protocol are judged `out-of-scope` until a shapekit release lists the version |
| a version older than every listed one, or not a semantic version | stops with an error that names the table and asks for a shapekit release that lists it |
| a `FROM` image in `BaseImages` | builds on its pinned digest |
| a `FROM` image the Dockerfile names by digest (`image@sha256:...`) | builds on that digest |
| any other `FROM` image | resolves its tag to a digest through the registry (`docker buildx imagetools inspect`) and builds on that digest. The image records it in a label, so the later chains of the run build on the same digest without asking the registry again. When the registry cannot be reached or refuses the request (Docker Hub's rate limit, for example), it stops with an error that names the table |

Each case the tables do not list is written to the adapter log as a `note:` line and recorded
with the protocol or digest the run used, in the `components` of each chain in
`harness-report.json` (`go-stellar-sdk.note`, `base-images.note`). To have the version or image listed, open an issue
in the shapekit repository; the next release adds it to the table (and to `pins.toml`).

## Reproducing a result locally

On a Linux host with Docker, from a checkout of shapekit at the release tag, with your adapter
built:

```sh
shapekit check indexer --adapter "<path to your adapter>" --chain <chain id>
```

The verdicts, the reason for each and the adapter's log are in `harness-out/`. A verdict you
believe is wrong is either an adapter mapping problem or a wrong corpus entry or oracle fact;
for the second, open an issue in the shapekit repository
([maintenance.md](maintenance.md#triage) describes how such reports are handled).
