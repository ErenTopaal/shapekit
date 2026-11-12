# Maintenance

This page says who maintains shapekit, for how long, and the rules every change follows. The
steps for a new protocol version are in [protocol-upgrade-runbook.md](protocol-upgrade-runbook.md).

## Maintainers and duration

- Maintainer: the repository owner, GitHub user `ErenTopaal`.
- Duration: from the production release through the first protocol upgrade after delivery,
  which is expected to be protocol 31 (CAP-89 and CAP-90, both Draft as of 2026-11-12). The
  commitment ends when a shapekit release covering that protocol is published by following the
  runbook, with every CI tier green.
- No co-maintainer from SDF has been named. If one is named, this page is updated to say who.

After the commitment ends the repository stays public under Apache-2.0. Anyone may fork it or
pick up the runbook; the corpus format, the testee protocol and the adapter interface are
documented so they do not depend on this repository staying active.

## What maintenance covers

- Keeping the pins current: `upstream-drift.yml` runs every Monday and opens a tracking issue
  when stellar-core, the stellar-xdr definitions, the `stellar-xdr` crate,
  `@stellar/stellar-sdk`, `go-stellar-sdk` or the core golden index changes upstream. Each such
  issue carries the runbook checklist and is worked through or closed with a reason.
- Keeping CI green: `pr.yml` on every pull request, `nightly.yml` every night. A red nightly
  run is treated as a bug in shapekit or a real upstream change, never silenced by widening a
  known-failures file without an issue link.
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
   the adapter interface let them be built outside this repository.

No response time is promised beyond this ordering. Security-sensitive findings in other
projects go through that project's private disclosure channel, not a public issue.

## Changes to expected values

Expected decoded values (`<variant>.expected.json`), the `expected` block of an entry, the
known-failures files in `conformance/known-failures/` and the reproduction tables decide what
counts as correct for every SDK and indexer that runs the corpus. A change to any of them needs
two approving reviews, and the pull request says which upstream source (stellar-core behaviour,
a CAP, an SEP) justifies the new value. Generated files change only through the command that
generates them, and CI checks that the committed files are what the command writes.

While the project has one maintainer, the second review comes from a reviewer outside the
project (an SDK maintainer or a stellar-core engineer) and is linked in the pull request.

## How SDKs pin a corpus release

An SDK consumes a release, never a branch. Each release publishes `corpus-<version>.tar.zst`
and a `SHA256SUMS` file, and the release notes repeat the checksums. An SDK's CI pins both the
release tag and the archive's sha256, and fails if the downloaded archive does not match. The
reusable workflow `.github/workflows/sdk-conformance.yml` does exactly that; how to wire it into
an SDK repository is in [sdk-ci.md](sdk-ci.md).

Moving an SDK to a newer corpus is a deliberate change in that SDK's repository: bump the tag and
checksum, run, and update the SDK's known-failures file with issue links for any new divergence.
