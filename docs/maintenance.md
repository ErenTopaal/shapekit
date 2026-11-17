# Maintenance

This page says who maintains shapekit, for how long, and the rules every change follows. The
steps for a new protocol version are in [protocol-upgrade-runbook.md](protocol-upgrade-runbook.md).

## Maintainers and duration

- Maintainer: GitHub user `ErenTopaal`.
- Duration: from the production release through the first protocol upgrade after delivery,
  protocol 31. CAP-89 and CAP-90 target protocol 31; both are in Draft status on 2026-11-14.
  The commitment ends when a shapekit release covering protocol 31 is published by following
  the runbook, with every CI tier green.

After the commitment ends the repository stays public under Apache-2.0. Anyone can fork it or
follow the runbook; the corpus format, the testee protocol and the adapter interface are
documented so they do not depend on this repository staying active.

## Protocol 31 rehearsal

The runbook has already been run against protocol 31 before its release, using core's
next-protocol build. The outputs are in [`next/`](../next), kept apart from the release corpus:

- `next/chains/smoke.p31/`: the smoke chain recorded at ledger protocol 31 under both the
  testnet and mainnet passphrases, with the next-protocol core image pinned by digest in
  `next/pins.toml`;
- `next/entries/`: 109 entries imported from stellar-core's `test-lcm-next` goldens, with their
  index in `next/goldens/`;
- `next/registry/`: draft registry records for the protocol 31 CAPs, merged into a copy of
  `registry/` by `next/overlay-registry.sh`;
- `next/rehearsal-build.diff`: the code changes the protocol 31 build needs;
- `next/generated/gates.json`: the protocol gates of the vnext core source (core commit
  `362dbc51`), written by `shapekit twin gates --pins next/pins.toml`.

What the rehearsal measured on the vnext image (core `362dbc51`, built for ledger protocol 31):

- `shapekit schema diff v30.0 next`: 2 names added (`TestNextType` and its field), none
  removed, no definition changed. Protocol 31 adds no XDR an indexer reads yet.
- `shapekit coretest run --pins next/pins.toml --shards 3`: all 144 `[tx]` test cases exit 0
  at every protocol from 0 to 31. They write 452,555 transaction metas (4.96 GB of dump) in
  3,588 s with 3 containers at 1 CPU and 1536 MB each. At 1536 MB the kernel kills 12 of the
  32 per-protocol runs of `pathpayment` and 9 of `revoke from pool`; each one passes when it
  is repeated at 3 GB, and no run stays killed.
- `shapekit coretest convert --baseline next`: all 452,555 metas convert and every short hash
  matches core's `test-tx-meta-baseline-next`, with no baseline hash left unreached.
- `shapekit twin gates`: 344 protocol gates, 187 of them in `src/transactions`, comparing
  against `V_2` to `V_30`. No gate names protocol 31 yet. Against core v30.0.0 (345 gates),
  9 gates exist only in v30.0.0 and 8 only in the vnext source. Most are the same comparison
  in a renamed or moved function. Two changes are new: the vnext source adds a check
  that the protocol is before `V_28` in
  `InvokeHostFunctionApplyHelper::recordStorageChanges`. It also drops the `V_25`
  comparisons in `LedgerManagerImpl::setLastClosedLedger` and `ApplicationImpl::start`.

How to repeat the rehearsal for the next protocol is in
[protocol-upgrade-runbook.md](protocol-upgrade-runbook.md#rehearsing-before-the-release-next-tier).

## What maintenance covers

- Keeping the pins current: `upstream-drift.yml` runs every Monday and opens a tracking issue
  when stellar-core, the stellar-xdr definitions, the `stellar-xdr` crate,
  `@stellar/stellar-sdk`, `go-stellar-sdk` or the core golden index changes upstream. Each such
  issue carries the runbook checklist and is worked through or closed with a reason.
- Keeping CI green: `pr.yml` on every pull request, `nightly.yml` every night. A red nightly
  run is either a bug in shapekit or a real upstream change. A known-failures line is added
  only with an issue link, never to silence a red run.
- Triage of corpus contributions, SDK-reported divergences and harness or linter reports, by the
  rules below.
- Releases: a signed tag runs `release.yml`, which publishes the binaries, the corpus archive
  and the golden snapshot with checksums and attestations.

## Triage

Policy, in the order issues are looked at:

1. A report that an indexer halts or loses data on a valid corpus entry, or that an entry is not
   something stellar-core emits, comes first. A wrong entry is worse than a missing one, because
   SDKs and indexers test against it.
2. A divergence an SDK maintainer reports against a corpus entry is reproduced with
   `shapekit conformance run --sdk <sdk> --entry <id>` before anything else. If the corpus is
   wrong, the entry is fixed under the expected-value rule below. If the SDK is wrong, the
   divergence goes into `findings/` and the SDK's issue tracker, with sibling issues for every
   other SDK that shares it (`shapekit conformance draft-issues`).
3. A corpus contribution (a new family, recipe or captured entry) is reviewed for provenance:
   local-core entries must regenerate byte for byte under both passphrases
   (`shapekit gen verify-repro --family <family>`), history entries must name the ledger they
   came from, and anything hand-made must carry `synthetic: true`.
4. Requests for new SDKs, adapters or linter rules are welcome as issues; the testee protocol and
   the adapter interface let you build them outside this repository.

Security-sensitive findings in other projects go through that project's private disclosure channel, not a public issue; reports
about shapekit itself go through GitHub security advisories, as `SECURITY.md` describes.

## Changes to expected values

Expected decoded values (`<variant>.expected.json`), the `expected` block of an entry, the
known-failures files in `conformance/known-failures/` and the reproduction tables decide what
counts as correct for every SDK and indexer that runs the corpus. A pull request that changes
any of them says which upstream source (stellar-core behaviour, a CAP, an SEP) justifies the new
value. Generated files change only through the command that generates them, and CI checks that
the committed files are what the command writes.

### Repository settings

`.github/CODEOWNERS` names a code owner for every path and lists the expected-value paths above
(`corpus/`, `conformance/known-failures/`, both reproduction tables, the fuzz class list), the
workflows and the pins separately, so GitHub requests the code owner's review on every pull
request that touches them. A branch ruleset on `main` (Settings, Rules) enforces the rules:

- require a pull request before merging;
- require conversation resolution;
- require the status checks of the `pr.yml` jobs that run on every pull request (build, rust,
  go, js, contracts, corpus, repro-select, conformance, harness) and that the branch is up to date. The
  `repro (<shard>)` jobs are not listed: their number depends on the selection, and a failing
  shard fails the workflow run;
- block force pushes and deletion.

Repository administrators can bypass the ruleset.

Private vulnerability reporting (Settings, Code security) is on, for the reports `SECURITY.md`
describes.

## Acceptance gate

`scripts/acceptance.sh` runs the checks every change to `main` passes, in two modes. Run it
from anywhere in the repository; it needs `cargo`, `go`, `golangci-lint`, `node` and `npm` on `PATH` and writes run
outputs under `target/acceptance/`. It stops at the first failing check and names it.

- Quick mode, `scripts/acceptance.sh`, is the gate for every change: formatting, clippy and
  the workspace tests; `go vet`, `go test` and golangci-lint in every module of `go.work`, and
  `indexerlint` over `adapters/`; the JavaScript testee's typecheck, build and tests; the
  registry, the corpus and candidate manifests, the synthetic entries, manifest and oracle
  files as the tools write them, the contract schemas, coverage, the guide, golden agreement,
  the committed chains, the repro selection, the adapter protocol schema and the release
  archive; the current version of each SDK over the corpus; the findings record; the em dash
  check; and
  `scripts/check-public-text.sh`, which fails when a tracked file holds an absolute home
  directory, a scratch or worktree directory name, a private or tailnet address, the build
  machine's name, or a name from the pattern file. That file names people, so it is not
  tracked: the script reads `info/text-patterns` in the clone's git directory, or the file
  `SHAPEKIT_TEXT_PATTERNS` names, one perl regex per line, and stops with exit status 2 when
  neither exists.
- Full mode, `scripts/acceptance.sh --full`, runs quick mode and then what the nightly tier
  runs: `shapekit pins verify` against the live sources, every SDK version and the upstream
  sightings (`conformance reproduce`), the wallet-backend reproductions table, the linter on
  wallet-backend at the pinned commit with its committed baseline, the stellar-rpc adapter
  over every chain of both networks once per `--events-from` method, checked against
  `crates/shapekit-harness/expect/stellar-rpc.toml`, `findings check --allow-draft`, every
  chain regenerated under both passphrases
  (`gen verify-repro`), the test contracts rebuilt with the pinned stellar-cli and compared
  with the committed wasm, and the site built as `site.yml` builds it. It also needs docker,
  network access and the `wasm32v1-none` Rust target. The last lines of its
  reproduction step and of the findings step give the totals, measured on 2026-11-17:
  `harness reproduce: 24 of 24 table rows hold (20 entry verdicts, 4 carry checks)` and
  `24 findings, 516 failures, 0 unmapped`.

## How SDKs pin a corpus release

An SDK consumes a release, never a branch. Each release publishes `corpus-<version>.tar.zst`
and a `SHA256SUMS` file, and the release notes repeat the checksums. An SDK's CI pins both the
release tag and the archive's sha256, and fails if the downloaded archive does not match. The
reusable workflow `.github/workflows/sdk-conformance.yml` does exactly that; how to wire it into
an SDK repository is in [sdk-ci.md](sdk-ci.md).

Moving an SDK to a newer corpus is a deliberate change in that SDK's repository: bump the tag and
checksum, run, and update the SDK's known-failures file with issue links for any new divergence.
