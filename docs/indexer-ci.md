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
   <out>/harness-report.json --write <file>` from a run you reviewed. With it, the run fails
   when any entry's verdict or reason changes, an entry is missing or unlisted, or the target
   build is another one. Without it, `mishandled` and `dropped` verdicts are reported but do
   not fail the run; it fails when a chain cannot be run (an adapter or target error), a dump
   fails, or a memory check of the run fails.

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
```

The values in angle brackets come from the release page. Reference the reusable workflow by the
same tag, so the workflow and the corpus come from one release.

| input | required | meaning |
|---|---|---|
| `adapter` | yes | the adapter command line, run from the root of your repository |
| `setup` | no | shell commands run from the root of your repository before the replay: build the adapter, build or pull the images the indexer needs |
| `shapekit-tag`, `corpus-asset`, `corpus-sha256`, `shapekit-sha256` | yes | the pinned release |
| `chains` | no | chain ids to replay, separated by spaces; every chain of the corpus when empty |
| `families` | no | families whose chains to replay, separated by spaces |
| `network` | no | `testnet` (default), `mainnet` or `both` |
| `delivery` | no | delivery mode to ask the adapter for: `sep54-filesystem`, `sep54-s3`, `sep54-gcs` or `rpc`; the adapter's default when empty |
| `expect` | no | path, in your repository, of an expected-verdicts file |
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
  [--delivery <mode>] [--expect <expect>]
```

The runner is `ubuntu-24.04`, which has Docker. The harness creates an internal Docker network
for each chain and the adapter attaches the indexer's containers to it, as
[adapter-interface.md](adapter-interface.md#transport) describes.

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

## Reproducing a result locally

From a checkout of shapekit at the release tag, with your adapter built:

```sh
shapekit check indexer --adapter "<path to your adapter>" --chain <chain id>
```

The verdicts, the reason for each and the adapter's log are in `harness-out/`. A verdict you
believe is wrong is either an adapter mapping problem or a wrong corpus entry or oracle fact;
for the second, open an issue in the shapekit repository
([maintenance.md](maintenance.md#triage) describes how such reports are handled).
