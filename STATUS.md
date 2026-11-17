# Status

A dated public log of what has landed in shapekit. Newest entry first.

## 2026-11-17: Kotlin Multiplatform SDK conformance

- `testees/kmp` runs the JVM target of Soneso's Kotlin Multiplatform SDK
  (`com.soneso.stellar:stellar-sdk-jvm`) 1.18.0, the newest 1.x release, on the Java testee's
  pinned JDK and Maven, with the resolved jars pinned in `lib.sha256`. `conformance
  sync-versions` reads its versions from Maven Central and its XDR from the `XDR_COMMIT` of
  `tools/xdrgen-kt/xdr-source.cfg` at the release tag, a `curr` commit generated from v30.0
  (protocols up to 30). The type table is generated from the schema and the SDK jar.
- Corpus: 630 entries pass, 19 are unsupported and 67 diverge from the network. The 67 are five
  candidate findings, drafted and not filed: the muxed id of a transaction's source dropped when
  the SDK rebuilds the transaction, so its hash is not the network's (KMP-MUXED-SOURCE); strings
  decoded as UTF-8 (KMP-UTF8); decoders that accept over-long values and nonzero padding
  (KMP-BOUNDS); XDR-JSON stopped at a nesting cap of 128 (KMP-JSON-DEPTH); and no XDR-JSON for an
  empty signed payload (KMP-SIGNEDPAYLOAD-JSON).
- Candidate lane: 5,713 entries pass and 198 diverge. Besides the same findings, it shows
  PRECOND_V2 preconditions holding only defaults written back as PRECOND_NONE (another hash),
  transaction-object checks that refuse values the network applied, and per-type decode depth
  caps (129 for SorobanDelegateSignature) below stellar-xdr's. The 30 checks on fixed opaques
  and bare 64-bit integers are stellar-xdr's rendering, not the SDK's.
- The `sdk-version` obligations grow from 25 to 26, the new one met.

## 2026-11-17: .NET SDK conformance

- `testees/dotnet` runs `stellar-dotnet-sdk` 16.0.0 and 17.0.0, the newest release of each line
  whose XDR knows protocol 29. The SDK names no XDR commit, so `conformance sync-versions`
  matches its `.x` files to stellar-xdr commits by git blob id (16.0.0: protocols up to 29,
  17.0.0: up to 30), and reads the versions from NuGet. The SDK has no XDR-JSON, so most corpus
  entries count as `unsupported` even where their bytes pass.
- Corpus: 17.0.0 passes 68 entries and diverges on 58; 16.0.0 passes 65 and diverges on 27.
  Eight candidate findings, drafted and not filed: a crash on create-contract operations of the
  first version (NET-CREATECONTRACT), allow trust and inflation operations refused
  (NET-DEPRECATED-OPS), issuers ordered as strkey text, the open
  [dotnet-stellar-sdk#257](https://github.com/Beans-BV/dotnet-stellar-sdk/issues/257)
  (NET-ISSUER-ORDER), a decode depth of 200 below core's (NET-DEPTH), strings decoded as UTF-8
  so memos and their hashes change (NET-UTF8), strkey checks SEP-23 asks for (NET-STRKEY), an
  ScVal vec without a body refused (NET-SCVEC) and a 29-byte memo accepted (NET-BOUNDS).
- Candidate lane: the transaction hash differs from the network's for offers whose price
  fraction is not in lowest terms, as in the Dart SDK, and for the same reason.
- The `sdk-version` obligations grow from 23 to 25, both new ones met.

## 2026-11-17: PHP SDK conformance

- `testees/php` runs `soneso/stellar-php-sdk` 1.18.0, the newest 1.x release, whose XDR comes
  from the same stellar-xdr commit as the Dart SDK's (protocols up to 30). `conformance
  sync-versions` reads its versions from Packagist. `pins.toml` pins PHP 8.5.12 and Composer
  2.11.2.
- Corpus: 616 entries pass, 15 are unsupported and 85 diverge from the network. The 85 are
  eight candidate findings, drafted and not filed: decoders that accept over-long values,
  nonzero padding and nesting past core's limit (PHP-BOUNDS); 64-bit values above 2^63-1 and
  negative sequence numbers without a correct XDR-JSON form (PHP-UINT64); hashes written as the
  hex of their hex (PHP-HEXHASH); claimable balance ledger keys without their `balance_id`
  member (PHP-CBKEY-JSON); XDR-JSON stopped at PHP's JSON nesting default (PHP-JSON-DEPTH); no
  XDR-JSON for an empty signed payload (PHP-SIGNEDPAYLOAD-JSON); no transaction object for an
  inflation operation (PHP-INFLATION); and negative muxed ids taken and wrapped (PHP-MUXEDID).
- Candidate lane: 5,631 entries pass and 388 diverge. Besides the same findings, the lane shows
  transaction hashes that differ from the network's because the SDK drops a set options field
  whose value is 0 when it rebuilds the operation. 353 of the lane's 570 failing checks are
  not triaged yet; `candidate/finding-candidates.toml` lists them by kind.
- The `sdk-version` obligations grow from 22 to 23, the new one met.

## 2026-11-17: Dart SDK conformance

- `testees/dart` runs `stellar_flutter_sdk` 3.12.0, the newest 3.x release. Its XDR comes from
  stellar-xdr's `curr` branch at the commit generated from the v30.0 tag, so it knows protocols
  up to 30. `conformance sync-versions` reads its versions from pub.dev. The testee needs
  Flutter 3.50.0 to resolve the package, which `pins.toml` pins.
- Corpus: 661 entries pass, 18 are unsupported and 37 diverge from the network. The 37 are
  seven candidate findings in `candidate/findings/triage.toml`, drafted and not filed: decoders
  that check no schema maximum (DART-BOUNDS), nesting limits that are not stellar-core's
  (DART-DEPTH), XDR strings read as UTF-8 so ledgers with other bytes in a memo, symbol or
  string do not decode (DART-UTF8), offer ids read from hex, signed or out-of-range text
  (DART-OFFERTEXT), offer ids typed as unsigned (DART-OFFERU64), no XDR-JSON for a signed-payload
  signer with an empty payload (DART-SIGNEDPAYLOAD-JSON) and no transaction object for an
  inflation operation (DART-INFLATION).
- Candidate lane: 5,943 entries pass and 164 diverge. Besides the same findings, the lane shows
  that the transaction hash differs from the network's for mainnet offers whose price fraction
  is not in lowest terms, because the SDK rebuilds the operation with a reduced price
  (`candidate/finding-candidates.toml`).
- The `sdk-version` obligations grow from 21 to 22, the new one met.

## 2026-11-17: Java stellar-sdk conformance

- `testees/java` runs java-stellar-sdk (`network.lightsail:stellar-sdk`) at 5.0.0 and 6.0.0,
  the newest release of each line whose XDR knows protocol 29, on JDK 27, each version built by
  Maven with its resolved jars checked against a committed hash list. It implements every op
  except `to_native` and `map_validate`, for which the SDK has no API, and passes the runner's
  decode limits to the SDK. `conformance sync-versions` derives its versions from Maven Central.
- Corpus (716 entries): 6.0.0 passes 208, diverges from the network on 488 and cannot run 20;
  5.0.0 passes 191, diverges on 106 and cannot run 419 (most are protocol 30 entries). Most
  divergences are the SDK's XDR-JSON names, as in Python. Beyond them: `Asset.compareTo` still
  orders issuers by text, so 6.0.0 refuses to read a change trust to a pool from a ledger core
  closed; textual offer ids with a sign, whitespace or a fraction are read and one past the
  int64 range is clamped; and on the candidate manifest a precondition V2 with no fields gives
  a wrong transaction hash. Each cause is in `candidate/finding-candidates.toml` with source
  lines; none is filed.
- The `sdk-version` obligations grow from 19 to 21, both new ones met.

## 2026-11-17: Python stellar-sdk conformance

- `testees/python` runs the Python `stellar-sdk` (py-stellar-base) at 16.1.1 and 17.0.1, the
  newest release of each line whose XDR knows protocol 29, under CPython 3.15 in a venv per
  version built from hash-locked requirements. It implements every op except `map_validate`,
  for which the SDK has no API. `conformance sync-versions` derives its versions from PyPI.
- Corpus (716 entries): 17.0.1 passes 195, diverges from the network on 504 and cannot run 17;
  16.1.1 passes 182, diverges on 513 and cannot run 21. Most divergences are the SDK's XDR-JSON
  names (`txsuccess` where SEP-51 has `tx_success`); the others include non-zero XDR padding
  that decodes, a trailing `=` that strkey decoding accepts, ids past 64 bits that wrap, map
  keys that merge in the native projection, the default depth of 512, and on 16.1.1 asset
  issuers ordered by text. Each is listed with source lines in
  `candidate/finding-candidates.toml`, marked as the finding of another SDK it shares or as
  new; none is filed.
- The `sdk-version` obligations grow from 17 to 19, both new ones met.


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
- New testee op `address_from_scval`: an `ScVal` handed to the SDK's conversion into its
  address type. The candidate vector `soroban.scval-shapes.synth-address-from-scval` shows
  `MuxedAddress::try_from_val` in soroban-sdk 28.0.0 and 29.0.1 panicking on six values that are
  not addresses, where its signature promises a `ConversionError`. This is
  [rs-soroban-sdk#2090](https://github.com/stellar/rs-soroban-sdk/issues/2090), fixed on the main
  branch by [#2099](https://github.com/stellar/rs-soroban-sdk/pull/2099) after 29.0.1 was
  released.

## 2026-11-17: obligation ledger, what core validates, candidate lane

- Obligation ledger: `corpus/coverage/obligations.json` lists everything the corpus owes a
  witness for, and `shapekit coverage --check` fails on an obligation without a status. By
  kind (met, open, total):

  | Kind | Met | Open | Total |
  |---|---|---|---|
  | `format`: XDR schema decision points | 3,869 | 0 | 3,869 |
  | `gap`: native type losses per language | 9 | 4 | 13 |
  | `core-test-section`: stellar-core's own transaction test sections | 29,691 | 0 | 29,691 |
  | `gate-side`: sides of stellar-core's protocol gates | 375 by branch count, 126 inferred from execution count | 189 | 690 |
  | `field`: envelope fields asked of core | 203 | 0 | 203 |
  | `sdk-version`: SDK versions the corpus runs against | 26 | 0 | 26 |
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
  instrumented core witness 375 of their 690 sides. For 126 more, a recipe run at a protocol
  that takes the side executes the gate, so the side is inferred from the execution count. Each
  of the 189 open sides carries its cause.
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
- stellar-rpc is a harness target ([adapter](docs/adapter-interface.md#the-stellar-rpc-adapter)):
  the v30.0.1 release image backfills each chain from its SEP-54 lake, and the harness judges
  the events that `getEvents`, `getTransactions` and `getLedgers` serve. Over both networks
  through `getEvents`, 906 entries: 820 pass and 86 are out of scope (their meta carries no
  events), with none dropped or mishandled; the carry check finds no truncated or fabricated
  value in 4,739,572 served cells. `getLedgerEntries` cannot be checked this way, because
  stellar-rpc answers it from captive core, which follows the network only over SCP. Two
  backfill defects wait in the candidate lane; nothing is filed.
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
- SDK conformance: `@stellar/stellar-sdk` 16.3.0 to 17.4.0, go-stellar-sdk v0.7.1 to v0.9.0,
  the `stellar-xdr` crate 28.0.0 to 30.0.0 and the Python `stellar-sdk` (py-stellar-base) 16.1.1
  and 17.0.1, with the reproductions of js-stellar-sdk #1766,
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
