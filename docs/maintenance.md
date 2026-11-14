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
counts as correct for every SDK and indexer that runs the corpus. A change to any of them needs
two approving reviews, and the pull request says which upstream source (stellar-core behaviour,
a CAP, an SEP) justifies the new value. Generated files change only through the command that
generates them, and CI checks that the committed files are what the command writes.

One of the two reviews comes from a reviewer outside the project, an SDK maintainer or a
stellar-core engineer.

### Repository settings

`.github/CODEOWNERS` names a code owner for every path and lists the expected-value paths above
(`corpus/`, `conformance/known-failures/`, both reproduction tables, the fuzz class list), the
workflows and the pins separately. A branch ruleset on `main` (Settings, Rules) enforces the
rules:

- require a pull request before merging, with **2** required approvals;
- require review from Code Owners;
- dismiss stale approvals when new commits are pushed, and require approval of the most recent
  push;
- require conversation resolution;
- require the status checks of the `pr.yml` jobs that run on every pull request (build, rust,
  go, js, contracts, corpus, repro-select, conformance, harness) and that the branch is up to date. The
  `repro (<shard>)` jobs are not listed: their number depends on the selection, and a failing
  shard fails the workflow run;
- block force pushes and deletion.

Private vulnerability reporting (Settings, Code security) is on, for the reports `SECURITY.md`
describes.

GitHub applies the approval count to the whole branch: a ruleset cannot ask for two approvals
on some paths and one on others, so every pull request needs two approvals. GitHub counts only
approvals from accounts with write access, so the outside reviewer of an expected-value change
is added as a collaborator for that review.

## How SDKs pin a corpus release

An SDK consumes a release, never a branch. Each release publishes `corpus-<version>.tar.zst`
and a `SHA256SUMS` file, and the release notes repeat the checksums. An SDK's CI pins both the
release tag and the archive's sha256, and fails if the downloaded archive does not match. The
reusable workflow `.github/workflows/sdk-conformance.yml` does exactly that; how to wire it into
an SDK repository is in [sdk-ci.md](sdk-ci.md).

Moving an SDK to a newer corpus is a deliberate change in that SDK's repository: bump the tag and
checksum, run, and update the SDK's known-failures file with issue links for any new divergence.
