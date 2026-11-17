# Status

A dated public log of what has landed in shapekit. Newest entry first.

## 2026-11-17: soroban-sdk conformance

- `testees/soroban` runs `soroban-sdk`, the SDK contracts are written with, at 28.0.0 and
  29.0.1, the newest release of each line whose XDR knows protocol 29. It reads `ScVal` bytes
  as a contract does, through `FromXdr` and the Soroban host, and checks map sorting and
  validation, the native projection, address strkeys and contract IDs; the SDK has no API for
  the other ops. `conformance sync-versions` derives its versions from crates.io.
- Corpus: both versions pass 27 entries; the other 689 need ops or types the SDK does not
  have. Candidate lane: both pass 8 entries and refuse 7 `ScVal`s the network accepts, each
  by a documented host rule (ledger-only `ScVal` arms, and nesting past the host's depth
  limits), recorded with source lines in `candidate/finding-candidates.toml`.
- The `sdk-version` obligations grow from 15 to 17, both new ones met.

## 2026-11-17: obligation ledger, what core validates, candidate lane

- Obligation ledger: `corpus/coverage/obligations.json` lists everything the corpus owes a
  witness for, and `shapekit coverage --check` fails on an obligation without a status. By
  kind (met, open, total):

  | Kind | Met | Open | Total |
  |---|---|---|---|
  | `format`: XDR schema decision points | 3,869 | 0 | 3,869 |
  | `gap`: native type losses per language | 9 | 4 | 13 |
  | `core-test-section`: stellar-core's own transaction test sections | 29,691 | 0 | 29,691 |
  | `gate-side`: sides of stellar-core's protocol gates | 353 by branch count, 129 inferred from execution count | 208 | 690 |
  | `field`: envelope fields asked of core | 203 | 0 | 203 |
  | `sdk-version`: SDK versions the corpus runs against | 17 | 0 | 17 |
  | `seed`: faults seeded into the SDKs | 72 | 142 | 214 |

- Envelope fields: `shapekit probe all` asks a standalone stellar-core v30.0.0 about every
  envelope field in 1,310 attempts, each a valid transaction with one field changed. 178 of
  the 203 fields are validated and 25 are not (`registry/generated/validation.json`). `shapekit guide check`
  compares each of the 88 rows of `registry/fields.toml` with these verdicts: 86 agree, and 2
  differ for a reason core's source shows, recorded with file and lines.
- Limits: 20 bounded quantities measured on core by bisection
  (`registry/generated/limits.json`), each with the largest accepted value, the first refused
  value and where core refuses it. For example, the largest wasm module that uploads is 130,972
  bytes.
- Protocol gates: `registry/generated/gates.json` lists the 345 protocol version comparisons in
  stellar-core v30.0.0's source. The branch counts of corpus and candidate chains on an
  instrumented core witness 353 of their 690 sides. For 129 more, a recipe run at a protocol
  that takes the side executes the gate, so the side is inferred from the execution count. Each
  of the 208 open sides carries its cause.
- Candidate lane: `candidate/` holds 6,229 entries that new runs wrote, such as probe witnesses
  and format vectors, apart from the release corpus; `shapekit corpus promote` moves an entry
  into the corpus. `registry/uncovered.toml` is checked against the lane: a probe refusal
  confirms 31 of the 47 union arms recorded as refused before any ledger, and no record is
  contradicted.
- Seeded faults ([docs/seeded-faults.md](docs/seeded-faults.md)): of the sampled faults that
  build, the corpus catches 7 of 38 in stellar-xdr 30.0.0, 14 of 76 in go-stellar-sdk v0.9.0
  and 36 of 100 in `@stellar/stellar-sdk` 17.4.0, plus the three released js-stellar-sdk faults
  it reproduces. With the candidate lane's format vectors it catches 12 of 38, 14 of 76 and 46
  of 100. Every missed fault carries a class; most are in code no testee op calls.
- Fuzzing ([docs/differential-fuzzing.md](docs/differential-fuzzing.md)): a coverage-guided
  search across the three SDKs reaches more code in every SDK than the same budget without the
  feedback, in both seeds measured. Each SDK's own fuzzer (cargo-fuzz, `go test -fuzz`,
  jazzer.js) found no new class in one 600 second run each.
- Protocol 31 rehearsal ([docs/maintenance.md](docs/maintenance.md#protocol-31-rehearsal)): on
  core's next-protocol image all 144 transaction test cases pass at protocols 0 to 31, and all
  452,555 metas they write match core's next baseline.
- Corpus: 716 entries, 594 of them emitted by stellar-core. The wallet-backend harness at
  `3e22515a` runs 906 entries on both networks: 678 pass, 106 mishandled, 44 dropped, 78 out of
  scope.
- Findings: 24 findings covering 516 failures, 0 unmapped. The 12 new failures are two new
  muxed claimable balance entries
  (`cap27.muxed-accounts.p28-claimable-balance-tx-source-id-1` and its p30 twin), which
  wallet-backend fails under two existing findings. Three new findings wait
  in the candidate lane as drafts (two for stellar-etl, one for go-stellar-sdk); nothing new
  is filed.
- `scripts/acceptance.sh` is the gate every change to `main` passes, in a quick and a full
  mode ([docs/maintenance.md](docs/maintenance.md#acceptance-gate)).

## 2026-11-15: 0.1.0 released, findings filed upstream

- shapekit 0.1.0 is released from a signed tag:
  <https://github.com/ErenTopaal/shapekit/releases/tag/v0.1.0>. Every asset has a build
  provenance attestation, and the two independent builds of the corpus archive are byte-identical.
- All 24 findings now link to an upstream decision; `shapekit findings check` passes with no
  draft left. The 17 new reports:
  - wallet-backend: [#812](https://github.com/stellar/wallet-backend/issues/812),
    [#813](https://github.com/stellar/wallet-backend/issues/813),
    [#814](https://github.com/stellar/wallet-backend/issues/814),
    [#815](https://github.com/stellar/wallet-backend/issues/815),
    [#816](https://github.com/stellar/wallet-backend/issues/816),
    [#817](https://github.com/stellar/wallet-backend/issues/817),
    [#818](https://github.com/stellar/wallet-backend/issues/818),
    [#819](https://github.com/stellar/wallet-backend/issues/819),
    [#820](https://github.com/stellar/wallet-backend/issues/820), and a reproducing ledger
    added to [#677](https://github.com/stellar/wallet-backend/issues/677);
  - go-stellar-sdk: [#6046](https://github.com/stellar/go-stellar-sdk/issues/6046),
    [#6047](https://github.com/stellar/go-stellar-sdk/issues/6047);
  - js-stellar-sdk: [#1907](https://github.com/stellar/js-stellar-sdk/issues/1907),
    [#1908](https://github.com/stellar/js-stellar-sdk/issues/1908);
  - rs-stellar-xdr: [#627](https://github.com/stellar/rs-stellar-xdr/issues/627),
    [#628](https://github.com/stellar/rs-stellar-xdr/issues/628),
    [#629](https://github.com/stellar/rs-stellar-xdr/issues/629);
  - stellar-core: [#5509](https://github.com/stellar/stellar-core/issues/5509).
- `conformance/known-failures/*.txt` point at the filed issues.
- `indexerlint` runs clean on wallet-backend at `3e22515a` with `lint/baselines/wallet-backend.json`,
  which baselines the one finding against wallet-backend#820; the nightly workflow checks it.

## 2026-11-14: 0.1.0 release candidate

What 0.1.0 holds:

- Corpus: 712 entries, 590 of them emitted by stellar-core. They cover 436 of the 499
  `LedgerCloseMeta` union arms and 44 of the 45 CAPs an indexer can see. Every arm or CAP
  without an entry has a recorded reason in `registry/uncovered.toml`; the per-CAP view is
  `corpus/coverage/COVERAGE.md`.
- Harness: wallet-backend at commit `3e22515a`, every chain on both networks, 898 entries: 674
  pass, 102 mishandled, 44 dropped, 78 out of scope. The reproductions of wallet-backend #729,
  #731, #716, #753 and #792 hold on their pre-fix and fixed commits (all 20 rows of
  `adapters/walletbackend/reproductions.toml`).
- SDK conformance: `@stellar/stellar-sdk` 16.3.0 to 17.4.0, go-stellar-sdk v0.7.1 to v0.9.0 and
  the `stellar-xdr` crate 28.0.0 to 30.0.0, with the reproductions of js-stellar-sdk #1766,
  #1785 and #1789 in `conformance/reproductions.toml`.
- Linter: `indexerlint` with rules IDX001 to IDX003.
- Guide: the indexer-hardening guide in `docs/guide/`, a draft for developers.stellar.org until
  a stellar-core or platform engineer has reviewed it (the guide publication gate below).
- Findings: 24 findings covering 504 failures, with no failure left unmapped. 17 findings are
  drafts with their upstream issue texts in `findings/issues/`; 7 are duplicates of another
  finding (`findings/FINDINGS.md`).
- Site: the documentation and a dashboard over the committed results at
  <https://shapekit.org>, with `llms.txt`, `llms-full.txt` and a markdown copy of every docs
  page.

Changes in this entry:

- The test contracts are built with `stellar contract build` from the stellar-cli 30.0.1
  release binary (commit `f2a7b741`). `pins.toml` pins the release asset by sha256,
  `scripts/fetch-stellar-cli.sh` downloads and checks it, and `shapekit pins verify` checks that
  GitHub still serves the same asset. `pr.yml` rebuilds the contracts and fails when the bytes
  differ from the committed wasm, and `scripts/upstream-releases.sh` tracks stellar-cli
  releases.
- Besides compiling, `stellar contract build` adds a `cliver` meta entry, drops unused contract
  spec entries and runs wasm-opt. The contracts are smaller as a result (auth_tree 1954 bytes,
  echo 1302 bytes); the hashes are in `contracts/wasm/SHA256SUMS` and
  `contracts/wasm/p<N>/SHA256SUMS`.
- Every chain that holds one of these contracts is recorded again on both networks, and
  `gen verify-repro` regenerates them byte for byte. No synthetic entry changed.
- The widest single-level auth-tree fan-out that still applies grows from 323 to 330 calls,
  because each call instantiates the smaller auth_tree module. The entries are now
  `soroban.auth-trees.p30-fanout-330` and `soroban.auth-trees.p30-fanout-331-fails`; the
  331-call transaction fails with `resource_limit_exceeded`.
- The wallet-backend harness gives every entry that kept its id the same verdict as before the
  rebuild, the committed SDK conformance matrices are unchanged, and so are the findings: 24
  findings, 504 failures, 0 unmapped.
- Negative offer ids written as `int64` values (`"-16"` as text, `-16` as a JSON number) move
  from the refusal cases of `cap03.offer-liabilities.synth-textual-offer-id-noncanonical` to a
  new entry, `cap03.offer-liabilities.synth-textual-offer-id-negative`. `offerID` is an `int64`
  and SEP-51 writes it as a decimal string, so a reader of the XDR type encodes these ids
  exactly; stellar-core refuses them only when it checks the operation. A testee passes the new
  entry by encoding the exact id or by refusing it. The `stellar-xdr` crate and
  `@stellar/stellar-sdk` up to 17.2.1 encode the exact id and 17.3.0 refuses it, so 15 checks on
  the crate and 25 on `@stellar/stellar-sdk` 16.3.0 to 17.2.1 that failed before now pass.
- The `soroban.scval-shapes.p30-*-depth-*` entries link
  [js-xdr#155](https://github.com/stellar/js-xdr/pull/155), which raised js-xdr's decode depth
  limit from 200 to 1500 nested types, in place of the map key order and map collision issues.

## 2026-11-13: stellar-core bool finding, docs site, CI on GitHub

- New corpus entry `soroban.invoke-basics.synth-invoke-arg-bool-2`: a transaction envelope whose
  contract argument carries the bool word 2. stellar-core's decoder reads it as true and
  re-encodes it as 1. The finding has issue drafts for rs-stellar-xdr and stellar-core.
- A conformance run over all three SDKs at once (`--flag references-disagree`) records where an
  SDK diverges from the other two. The nightly workflow runs it, and the findings read it.
- The docs and dashboard site at <https://shapekit.org>: Fumadocs on React Router, prerendered,
  with data from `shapekit site data` ([docs/site.md](docs/site.md)).
- CI runs on GitHub. The pull request, site, upstream-drift and nightly workflows pass,
  including the wallet-backend reproductions on their pre-fix and fixed commits, fuzzing, the
  goldens, the docker-backed tests and the findings check.

## 2026-11-13: wallet-backend and stellar-rpc pins at current upstream

- wallet-backend's pin moves from `e967ce8b` to `3e22515a`, `main` on 2026-11-13. The two
  commits between them, #803 and #802, change only Blend contract data decoding and its
  GraphQL resolvers. `e967ce8b` stays pinned as the fixed side of #716.
- At the new pin every entry the two runs share has the same verdict and the same problems.
  `indexerlint` reports the same single IDX002 finding (`walkInvocation`) on wallet-backend.
- The findings inputs, the per pull request expectations and the wallet-backend issue drafts
  name `3e22515a`.
- stellar-rpc's pin moves to `v30.0.1` (commit `6b6ec60c`). The JSON-RPC behaviour
  `shapekit serve` follows is documented in [docs/replay-server.md](docs/replay-server.md).

## 2026-11-13: corpus data fixes

- Golden import takes only golden ledgers inside a family's protocol range, tagged with the
  family's CAPs that exist at the ledger's protocol. The import also runs the oracle, a
  re-import changes nothing, and the nightly `goldens` job re-imports into a copy and fails on
  any difference.
- `classic.failed-tx` records a native ChangeTrust line at protocol 9, which core closes as
  `tx_internal_error`. This covers `ChangeTrustAsset:native` and gives `tx_internal_error` an
  entry from a release build of core.
- Every `below-protocol-valid` entry carries `protocol.valid_max`, the last protocol at which
  its bytes are valid. The validator requires it and checks `generated_at` against the range on
  every entry.
- Mainnet ledger 25000000 (protocol 11) is captured into `cap03.offer-liabilities`. Its header
  holds `StellarValueExt:basic`, which current core never writes.
- `shapekit gen verify-raw-order` compares core's raw change streams of two independent
  recordings ([docs/adding-a-shape.md](docs/adding-a-shape.md), Normalization).
- Readers of manifest and `chain.json` file references refuse paths outside the corpus or the
  chain and check the recorded hashes.

## 2026-11-13: CI coverage, pinned wallet-backend builds, guide publication gate

- `gen verify-repro` in `nightly.yml` regenerates every chain of every recipe the CLI
  registers. A `pr.yml` step checks that the family list of `scripts/select-repro.sh` is
  exactly the set the CLI accepts and that every committed chain belongs to a listed family.
- `nightly.yml` runs `shapekit findings check` after the conformance and harness jobs, with an
  `indexerlint` run on wallet-backend at the pin as its third input.
- The wallet-backend adapter builds wallet-backend's image on base images pinned by digest in
  `pins.toml`. The per pull request and nightly harness jobs cache the clone and the built
  image.
- The Go testee builds against a local checkout of go-stellar-sdk, so an SDK pull request can
  test its own code. [docs/sdk-ci.md](docs/sdk-ci.md) has versions files for Rust, Go and
  JavaScript.
- `.github/CODEOWNERS`, [SECURITY.md](SECURITY.md) and a `NOTICE` entry for the
  wallet-backend excerpts in the linter's test data.

### Guide publication gate

The RFP gates publication of the guide on review by a stellar-core or platform engineer.

- Guide: `docs/guide/indexer-hardening.mdx`.
- Required reviewer: a stellar-core engineer or a platform engineer at SDF.
- State: draft, awaiting review.
- stellar-docs change: `docs/guide/stellar-docs.patch` adds the page as
  `docs/data/indexers/build-your-own/indexer-hardening.mdx` with a link from the Build Your Own
  index and an entry in `docs.json`, on top of stellar-docs `main` at `62f29bbb`. Corpus
  entries in the page link to the v0.1.0 tag. The patched pages pass stellar-docs' prettier
  configuration and its pad check (`scripts/pad-component-tags.mjs --check`). The README shows
  how to apply it.
- After the review, this section records the reviewer, the date and the reviewed commit.

## 2026-11-12: CI tiers, release workflow, maintenance plan

- `pr.yml` runs on every pull request: one release build of the CLI shared by parallel jobs for
  Rust, Go, the JavaScript testee and the corpus checks, `gen verify-repro` under both
  passphrases, conformance on the current SDK versions and the wallet-backend harness on three
  testnet chains.
- `gen verify-repro` per pull request covers a rotating window of four families, every family
  the change touches, and every family when shared generator code, the registry, the contracts
  or the pins change. `scripts/select-repro.sh` makes the choice and shards it over up to four
  runners.
- `nightly.yml` regenerates every chain, runs the docker-backed tests, the whole SDK matrix and
  its reproductions, the harness over every chain in both delivery modes, the wallet-backend
  reproductions and core's `--check-lcm` on the goldens.
- `upstream-drift.yml` opens one tracking issue, carrying the runbook checklist, per new
  stellar-core, stellar-xdr or SDK release and per change to core's golden index.
- `release.yml` runs on a tag whose signature GitHub verifies. It builds the binaries, the
  corpus archive and the golden snapshot, requires two independent builds of the archives to
  have equal checksums, and publishes everything with `SHA256SUMS`, build provenance
  attestations and release notes taken from this file.
- `sdk-conformance.yml` is a reusable workflow an SDK repository calls to run its testee over a
  pinned corpus release with its own known-failures list.
- Docs: [docs/protocol-upgrade-runbook.md](docs/protocol-upgrade-runbook.md),
  [docs/maintenance.md](docs/maintenance.md), [docs/sdk-ci.md](docs/sdk-ci.md) and
  [docs/infrastructure.md](docs/infrastructure.md), which lists the measured CI durations.

## 2026-11-12: repository foundation

- Apache-2.0 `LICENSE`, `NOTICE`, this file and `README.md`.
- `pins.toml` lists every upstream reference with its purpose: the Rust, Go and Node.js
  toolchains with their release checksums, container images by digest, the stellar-core,
  stellar-xdr, stellar-rpc, stellar-protocol and wallet-backend commits, the GitHub Actions
  used, every Rust crate, npm package and Go module, and the SDK versions under test.
  `shapekit pins verify` checks each pin against git remotes, crates.io, npm, the Go module
  proxy and Docker Hub, and exits non-zero on any mismatch.
- A cargo workspace, a Go workspace (`go.work`) and the JavaScript testee package, each with
  its tests.
- `pr.yml` runs rustfmt, clippy with warnings denied, the cargo tests, `go vet`, `go test`,
  golangci-lint, the JavaScript testee's build and tests, and `scripts/check-no-em-dash.sh`
  over all tracked files. `upstream-drift.yml` runs weekly.
