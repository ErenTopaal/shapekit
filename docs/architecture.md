# Technical architecture

This document describes the technical architecture of shapekit and how it connects to the
Stellar network software. It covers release v0.2.0 and the `main` branch of the repository,
and the planned work that the repository records.

- Repository: <https://github.com/ErenTopaal/shapekit> (Apache-2.0)
- Release: <https://github.com/ErenTopaal/shapekit/releases/tag/v0.2.0>
- Documentation and dashboard: <https://shapekit.org>
- Dated status log: [STATUS.md](../STATUS.md)
- Maintainer: [Eren Topal](https://github.com/ErenTopaal)

Every number in this document comes from a committed file of the repository, from a command of
the shapekit CLI, or from the public issue trackers on 2026-11-20 (section 13.1). The source of
each number is named next to it.

## 1. Purpose and scope

Software that reads the Stellar ledger must decode XDR. Valid but rare XDR shapes cause
failures in SDKs and indexers. Examples are signed-payload signers (CAP-40), muxed accounts
(CAP-27) and deep Soroban authorization trees. Other examples are trustlines that the Stellar
Asset Contract creates (CAP-73) and `ScMap` keys that collide after conversion.

shapekit is a test kit for this problem. It answers the SDF RFP "XDR/CAP-Coverage Audit Tool
and Indexer-Hardening Guide". It covers protocol 30: stellar-core 30.0.0 and the stellar-xdr
definitions v30.0.

shapekit has these parts:

| Part | What it does | Where |
|---|---|---|
| Corpus | XDR files of rare shapes, a language-neutral manifest with expected decoded values, and the recorded ledger chains | `corpus/` |
| Generator | Records chains on a standalone stellar-core on an internal Docker network, under the testnet and mainnet passphrases | `crates/shapekit-gen`, `crates/shapekit-core-driver` |
| SDK conformance runner | Sends the corpus through 11 Stellar SDKs and tools and reports where each one diverges from the network | `crates/shapekit-conformance`, `testees/` |
| Replay harness | Replays the chains into an indexer and compares what the indexer stored with the transaction meta | `crates/shapekit-harness`, `crates/shapekit-serve`, `adapters/` |
| indexerlint | Static analysis of Go indexer code built on go-stellar-sdk | `lint/` |
| Probe | Measures which envelope fields stellar-core validates, and its limits | `crates/shapekit-probe` |
| Indexer-hardening guide | A page for developers.stellar.org, generated in part from the registry and the probe results | `docs/guide/` |
| Fuzzers and the coverage twin | Differential and coverage-guided fuzzing of the SDK decoders, and code coverage of stellar-core | `crates/shapekit-fuzz`, `crates/shapekit-twin`, `twin/` |
| Public network checks | The law book over the public SEP-54 ledger lake, as a daily audit and an hourly watch | `crates/shapekit-laws`, `crates/shapekit-sentinel` |
| Findings | Every failure mapped to a root cause with severity, component, corpus entry and upstream issue | `findings/`, `candidate/findings/` |
| CI and release | GitHub Actions workflows, reusable workflows for SDK and indexer repositories, signed releases | `.github/workflows/` |
| Site | Documentation and a dashboard over the committed results | `site/` |

shapekit does not run Horizon and does not submit transactions to a public network. It needs
no keys or funds on a public network. It collects no telemetry.

## 2. System overview

The Rust workspace has 25 crates (`crates/*`, workspace version 0.2.0, Rust 1.99.1). The Go
workspace (`go.work`, Go 1.27.4) has three modules: `adapters`, `lint` and `testees/go`. The
testees of the other SDKs live under `testees/` in their own languages.

```text
 STELLAR INPUTS                      GENERATION
 stellar-core 30.0.0 image  ------>  shapekit gen, import, capture,
 (pinned by digest)                  synth: core driver on an
                                     internal Docker network,
                                     recipes and recipe IR,
                                     key-stable normalization
                                                |
 core test-lcm-current goldens -------------+   |
 (tag v30.0.0)                              |   |
 public SEP-54 ledger lake -----------------+   |
 (pubnet and testnet history)               |   |
 stellar-xdr v30.0 definitions, ------------+   |
 stellar-xdr crate 30.0.0                   v   v
 +------------------------------------------------------------+
 | CORPUS (corpus/)                                           |
 | chains: SEP-54 lake, history archive, chain.json           |
 | entries: .xdr, .expected.json, .oracle.json                |
 | manifest.json (schema/manifest.schema.json)                |
 +------------------------------------------------------------+
            |                                  |
            v                                  v
 SDK CONFORMANCE                    INDEXER REPLAY HARNESS
 conformance runner                 shapekit serve (stellar-rpc
 -> 11 testees                      shaped JSON-RPC, SEP-54 lake,
    (testee protocol v2)            history archive)
 -> divergence matrix               -> adapters: wallet-backend,
            |                          stellar-etl, stellar-rpc,
            |                          mapped
            |                       meta oracle -> harness report
            |                                  |
            |          indexerlint             |
            |               |                  |
            +-------------> FINDINGS <---------+
                            RECORD
                              |
                              v
                     shapekit.org dashboard
```

The data flow from stellar-core to each consumer:

| Consumer | Path of the data |
|---|---|
| An SDK | stellar-core writes `LedgerCloseMeta` on a standalone network. The recorder normalizes it and cuts entries. The runner sends each entry's bytes to the SDK's testee and compares the answer with the manifest. |
| wallet-backend | The replay server serves a chain through `getLedgers` (JSON-RPC) or through a SEP-54 lake in an S3 store, and serves the history archive. wallet-backend ingests into TimescaleDB. The adapter reads the rows back. |
| stellar-etl | The adapter uploads the chain's SEP-54 lake into a GCS emulator. stellar-etl runs its export commands. The adapter reads the exported rows. |
| stellar-rpc | The adapter copies the chain's SEP-54 lake into a filesystem datastore. stellar-rpc backfills from it. The adapter reads `getEvents`, `getTransactions` or `getLedgers`. |
| Any indexer | The same delivery modes through the adapter protocol, or a mapping file for an indexer that runs in Docker and stores in PostgreSQL. |

## 3. Stellar inputs

### 3.1 stellar-core

All pins are in [pins.toml](../pins.toml).
`shapekit pins verify` checks each pin against its upstream source.

| Input | Pin | Use |
|---|---|---|
| Release image | `stellar/stellar-core:30.0.0-3642.0e49751aa.noble`, digest `sha256:eb7a7c81a29b8665c8878d55e1b3979f0d7527426350c4ff258a9c7647ae5c51` | Records every chain; the probe; the explorer |
| Test build image | `stellar/unsafe-stellar-core:30.0.0-3642.0e49751aa.noble-buildtests`, digest `sha256:a8cb428ce497695321fdffc8cdb23b8102c1074ebe480f1c9e923fe1052e22ff` | Core's own `test '[tx]' --check-lcm` run, and the meta dump of core's transaction tests |
| Source | tag `v30.0.0`, commit `0e49751aa9ddf892302cca02cea7f3f5d97df9f1` | The `test-lcm-current` goldens, the protocol gate list, the coverage twin build |

The core driver (`crates/shapekit-core-driver`) runs each standalone core in its own container.
It creates the Docker network with `--internal` and checks that the network is internal before
core starts (`crates/shapekit-core-driver/src/node.rs`). It refuses to start otherwise. So the
mainnet passphrase is used only on a network that has no route off the host. The container runs
with `--cap-drop ALL` and `no-new-privileges`.

The driver fixes close times: ledger `seq` closes at `1767225600 + seq * 86400`
(`CHAIN_EPOCH` and `CLOSE_STEP` in `crates/shapekit-core-driver/src/config.rs`). The newest
protocol the driver records is 30 (`MAX_PROTOCOL`).

### 3.2 Network passphrases

| Network | Passphrase | Where used |
|---|---|---|
| Testnet | `Test SDF Network ; September 2015` | Every recipe chain, testnet history captures |
| Mainnet | `Public Global Stellar Network ; September 2015` | Every recipe chain, mainnet history captures |
| Core test passphrase | `(V) (;,,;) (V)` | Golden ledgers and core test metas, which core's own tests write |

Each recipe chain is recorded twice, once per passphrase. An entry from a recipe has a `testnet`
and a `mainnet` variant. An entry from core's goldens or core's tests has one `any` variant.

### 3.3 Network history

`shapekit capture history` reads one ledger from the public SEP-54 data lake
(`aws-public-blockchain`, us-east-2) or, with `--via rpc`, from a stellar-rpc endpoint. The
corpus holds 16 mainnet-history entries and 3 testnet-history entries
(`corpus/manifest.json`). They come from mainnet ledgers 25000000, 36196205, 37466023,
44679511, 51000294, 57369500, 57500008, 63092988, 63092990 and 63092996, and from testnet
ledgers 4437357, 5521432 and 5695387. Each entry records its provenance URL in the lake.

Mainnet ledger 25000000 (protocol 11) is the only corpus entry with the arm
`StellarValueExt:basic`. A current core always signs the values it proposes, so a local core
cannot write that arm (`corpus/coverage/COVERAGE.md`).

### 3.4 XDR

| Input | Pin |
|---|---|
| XDR definitions | stellar-xdr tag `v30.0`, commit `4f524bbac80c781c06e4fb5fc93a2d0d331d6705` |
| Rust XDR library | `stellar-xdr` crate 30.0.0 |
| Definitions as data | `schema/xdr/<tag>/xdr.json`, written by `scripts/fetch-xdr-definitions.sh` with the generator that the `stellar-xdr` crate uses (rs-stellar-xdr tag `v30.0.0`). The repository holds the tags v20 to v30.0 and `next`. |

`crates/shapekit-schema` reads these files. It gives the schema model, reachability from
`LedgerCloseMeta`, recursion groups and the history of each name (`shapekit schema diff`,
`schema first-seen`, `schema families`).

The corpus measures coverage as union arms reachable from `LedgerCloseMeta`: 499 arms
(`corpus/coverage/coverage.json`). The corpus holds `LedgerCloseMeta` v0 (174 entries), v1 (84)
and v2 (532). It holds `TransactionMeta` v2 (57), v3 (30) and v4 (738). The counts come from
`lcm_version` and `tx_meta_version` in `corpus/manifest.json`.

Decode limits follow stellar-core. Core's Rust parser reads envelopes at depth 1000 from
protocol 28 (`xdr.envelope.depth` in `registry/caps-limits.toml`). A `LedgerCloseMeta` holds
its envelopes up to 12 levels deeper, so `LedgerCloseMeta` gets depth 1012
(`crates/shapekit-manifest/src/depth.rs`). The length limit is the input length.

### 3.5 Protocols

The corpus has entries generated at 21 protocol versions: 1, 9, 10, 11, 12, 14, 15, 17, 18, 19
and 20 to 30 (`protocol_tiers` in `corpus/manifest.json`). 617 entries are generated at
protocol 30. `COVERED_THROUGH_PROTOCOL` in `crates/shapekit-registry/src/lib.rs` is 30.

Soroban chains below protocol 30 need the contract host of their own protocol for simulation.
The workspace links renamed `soroban-env-host` and `soroban-simulation` releases for protocols
21 to 29 next to 30.0.0 (`Cargo.toml`, `SIMULATED_PROTOCOLS` in
`crates/shapekit-core-driver/src/sim_legacy.rs`).

### 3.6 CAPs and SEPs

`registry/caps.toml` holds 102 CAP records and one `PRE-CAP` record for shapes that existed
before the CAP process (`shapekit registry check`). 45 CAPs change what an indexer can see.

| Standard | Version | How shapekit uses it |
|---|---|---|
| SEP-23 strkeys | 1.4.0 (stellar-protocol `ced008c3`) | `strkey` corpus entries, the `strkey_decode` and `strkey_encode` testee ops, address decoding in the adapters and the carry check, the guide's strkey section |
| SEP-51 XDR-JSON | 2.0.1 | The expected decoded value of every entry (`<variant>.expected.json`), the `decode_json` and `encode_json` ops, the `json` format of the replay server |
| SEP-54 ledger data lake | 0.2.0 | The committed form of every chain, a harness delivery mode, the input of the public network checks |
| SEP-41 token interface | | The `sep41.muxed-addresses` family and the oracle's `token-event` kind |
| SEP-11 asset strings | | Asset values in the oracle vocabulary (`native` or `CODE:ISSUER`) |

## 4. The corpus

### 4.1 Entries and the manifest

[corpus/manifest.json](../corpus/manifest.json)
lists every entry. Its JSON Schema is `schema/manifest.schema.json`. Any pipeline can read the
corpus without shapekit.

Corpus 0.2.0 has 1,092 entries in 37 families (`corpus/manifest.json`, `shapekit manifest
validate`):

| Source class | Entries | Meaning |
|---|---|---|
| `local-core` | 844 | Bytes that the pinned core wrote into a ledger on a standalone network |
| `core-golden` | 107 | Ledgers imported from core's `test-lcm-current` goldens |
| `synthetic` | 104 | Hand-built values, marked `synthetic: true` |
| `local-core-refused` | 18 | Envelopes that core refused at submission, with core's result code |
| `mainnet-history` | 16 | Ledgers captured from mainnet history |
| `testnet-history` | 3 | Ledgers captured from testnet history |

970 entries are core-emitted. Coverage counts only these.

By kind: 1,037 `xdr` entries, 23 `strkey`, 17 `ordering`, 8 `native-projection` and 7
`textual-id`. By XDR type: 791 `LedgerCloseMeta`, 128 `TransactionEnvelope`, 77
`TransactionResultMetaV1`, 18 `LedgerEntry`, 10 `ScVal`, 5 `ContractEvent`, 4
`TransactionResultMeta`, and one each of `Asset`, `SignerKey`, `SorobanAuthorizationEntry` and
`ScSpecFunctionV0`.

Each entry carries:

- its CAPs and protocol range;
- its source class and provenance;
- the expected decode result: `must-accept`, `must-reject` or `implementation-defined`;
- the fields it exercises and the upstream issues it reproduces;
- per variant, its XDR file, its expected XDR-JSON and, for transactions and ledgers, its
  oracle file, each with a sha256.

This is a real record from the manifest:

```json
{
  "caps": ["CAP-0040"],
  "expected": { "decode": "must-accept", "roundtrip": true, "validity": "valid" },
  "family": "cap40.signed-payload-signer",
  "fields_exercised": ["op.set_options.signer", "signer_key.signed_payload.payload"],
  "flags": [],
  "id": "cap40.signed-payload-signer.p30-set-options-payload-1-envelope",
  "kind": "xdr",
  "linked_issues": [
    "https://github.com/stellar/wallet-backend/pull/729",
    "https://github.com/stellar/wallet-backend/issues/716"
  ],
  "notes": "SetOptions adds a signed-payload signer with a 1 byte payload to the source account",
  "passphrase_dependent": true,
  "protocol": { "generated_at": 30, "introduced": 19, "valid_min": 19 },
  "source": {
    "chain": "cap40.signed-payload-signer.p30",
    "class": "local-core",
    "ledger_seq": 65,
    "recipe": "cap40.signed-payload-signer",
    "tx_index": 0
  },
  "synthetic": false,
  "variants": {
    "mainnet": {
      "expected_json_file": "entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p30-set-options-payload-1-envelope/mainnet.expected.json",
      "expected_json_sha256": "2bb9d5c1aa553e8e51f34fc3e219690f476effbde399aa9f4b3670c7374b43a0",
      "network_passphrase": "Public Global Stellar Network ; September 2015",
      "oracle_file": "entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p30-set-options-payload-1-envelope/mainnet.oracle.json",
      "oracle_sha256": "534685c4b9ac5e23c2409ceaf85cf57528bc4986e3a9eb7d1009c309c7250d4c",
      "xdr_file": "entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p30-set-options-payload-1-envelope/mainnet.xdr",
      "xdr_sha256": "9fec50d5c6f69c843af93546b0a0244ccce88887647e0977518fefd8f7f10aed"
    },
    "testnet": {
      "expected_json_file": "entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p30-set-options-payload-1-envelope/testnet.expected.json",
      "expected_json_sha256": "6d5efb702450348903165825bd7bfc51c9b2b83eceb15e580ac305c602626105",
      "network_passphrase": "Test SDF Network ; September 2015",
      "oracle_file": "entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p30-set-options-payload-1-envelope/testnet.oracle.json",
      "oracle_sha256": "bdd1dfa43269de3a1f0278d7957c37fe708cb0f17e3d6b8e4bde76ff8dab82e1",
      "xdr_file": "entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p30-set-options-payload-1-envelope/testnet.xdr",
      "xdr_sha256": "0d6c2624b4e2d5b9d08764e529afafbdcc80e7108e2e8f64bd23e82ccfcb9bad"
    }
  },
  "xdr_type": "TransactionEnvelope"
}
```

The expected XDR-JSON is the rendering of the pinned `stellar-xdr` crate, corrected where it
departs from SEP-51 v2.0.1 (`crates/shapekit-manifest/src/sep51.rs`).

The manifest validator (`shapekit manifest validate`) decodes and re-encodes every entry and
requires the same bytes. It also checks the protocol range of every entry and the caps of every
explorer entry against the arms it witnesses. File references must stay inside the corpus.

### 4.2 Chains

A chain is one recorded run of a recipe at one protocol under one passphrase. The corpus has 135
chain directories under `corpus/chains/`, each with a `testnet` and a `mainnet` network
directory. Each network directory holds:

| File | Content |
|---|---|
| `chain.json` | Protocol, passphrase, core image, close schedule, ledger range, the sha256 of each ledger (canonical and raw), the normalization, the lake batches with their sha256, and the archive members |
| `ledgers/` | A SEP-54 lake of the canonical `LedgerCloseMeta`: `.config.json` and one zstd `LedgerCloseMetaBatch` per ledger, 64000 per partition (the Galexie layout). It is reproducible byte for byte. |
| `ledgers.xdr` | Core's raw framed stream, kept as provenance |
| `archive.tar` | The node's history archive, with at least checkpoint 63 |

Every recipe chain starts the same way. The protocol upgrade happens in ledger 2. Empty ledgers
follow through ledger 63, core's first history checkpoint. The network settings and the recipe
start at ledger 64. An indexer that bootstraps from a checkpoint can then start before the first
shape.

### 4.3 Recipes

A recipe is a Rust program in `crates/shapekit-gen/src/recipe/` that drives the core driver and
captures entries. `all()` in `crates/shapekit-gen/src/recipe/mod.rs` registers 44 recipe values.
`corpus/recipes/chains.toml` registers 91 more chains recorded from recipe IR files
(`schema/recipe-ir.schema.json`). The recipe explorer wrote those IR files (see 5.5).
44 plus 91 gives the 135 chains.

The recipe context (`ChainCtx`) gives:

- deterministic keys per role;
- transaction builders for v0 and v1 envelopes;
- Soroban upload, deploy and invoke, with simulation;
- config upgrade sets, and several transactions in one ledger;
- captures.

Captures select a whole ledger
(`LedgerCloseMeta`), one transaction (`TransactionEnvelope` and its result meta) or one ledger
entry. `capture_rejected` records an envelope that core refuses at submission, with core's
result code.

Soroban recipes use the test contracts in `contracts/` (`auth_tree`, `delegating_account`,
`echo`, `exec_ref`, `factory`, and legacy builds for older protocols). `stellar contract build`
from the pinned stellar-cli 30.0.1 release binary builds them. `pr.yml` rebuilds them and fails
when the bytes differ from the committed wasm.

### 4.4 The registry

`registry/` holds the facts that every check reads:

| File | Content (`shapekit registry check`) |
|---|---|
| `caps.toml` | 102 CAPs and the pre-CAP record, each with its protocol, status, XDR additions and whether an indexer can see it |
| `families.toml` | 41 families. Each names its CAPs, protocol range, signature fragments, fields and linked issues. |
| `fields.toml` | 88 envelope fields with `validated_by_core`, the risk, the handling the guide recommends and the go-stellar-sdk accessors |
| `caps-limits.toml` | 35 limits, such as decode depth and vector lengths |
| `lint-rules.toml` | The 3 linter rules |
| `uncovered.toml` | The reason for each arm, fragment or CAP without a core-emitted entry |
| `gaps/` | Native type losses per language (`go.toml`, `js.toml`, `rust.toml`) |
| `guide-checklist.toml` | The guide's self-audit checklist items |
| `obligation-sources.toml` | The fragments that the obligation ledger lists |
| `generated/` | Measured data: `validation.json`, `limits.json`, `gates.json` |

An entry belongs to a family when the shape signature of its bytes matches a fragment
(`crates/shapekit-manifest/src/signature.rs`).

### 4.5 Coverage and gaps

`shapekit coverage --check` gives (run on `main`):

```text
coverage ok: 1092 entries (970 core-emitted); 44 of 45 indexer-visible caps core-emitted, 1 with a reason; 436 of 499 arms covered; 318 of 328 fragments covered; 30 of 30 not fully validated fields exercised
```

The site and this document count coverage the same way
(`ledgerArms` and `capCoverage` in `site/src/lib/metrics.ts`):

- **Union arms.** `LedgerCloseMeta` reaches 499 union arms. 58 of them cannot occur in any
  ledger: core refuses 47 before a ledger (`refused-before-ledger`), and the pinned core has no
  code path that writes the other 11 (`never-emitted`). A ledger can hold the other 441 arms,
  and core-emitted entries cover 436 of them (98.9%).
- **CAPs.** 44 indexer-visible CAPs have a core-emitted entry. The 45th, CAP-83, is
  history-only: its only shape, `StellarValueExt:empty_tx_set`, comes from the ballot protocol
  when a node drops a transaction set. A standalone node never does this. The record names the
  testnet and mainnet ranges that were scanned and found none.

The per-CAP table is [corpus/coverage/COVERAGE.md](../corpus/coverage/COVERAGE.md). The 63
uncovered arms each have a blocker and a reason with core source references
(`registry/uncovered.toml`):

| Blocker | Arms | Meaning |
|---|---|---|
| `refused-before-ledger` | 47 | Core refuses the transaction before it reaches a ledger |
| `never-emitted` | 11 | The pinned core has no code path that writes it |
| `history-only` | 2 | Only a public network can hold it, and no capture holds it yet |
| `not-producible-locally` | 2 | Core has the code path, but a standalone network cannot reach it |
| `old-protocol-only` | 1 | Only ledgers below some protocol carry it |

Probe refusals in the candidate lane confirm 31 of the 47 `refused-before-ledger` arms
(`COVERAGE.md`).

### 4.6 The obligation ledger

[corpus/coverage/obligations.json](../corpus/coverage/obligations.json)
lists each obligation that the corpus owes a witness for. `shapekit coverage --check` fails on
an obligation without a status, and on a witness that names a missing entry, chain, recipe or
dataset. The counts (`COVERAGE.md`, "Obligations"):

| Kind | Met | Open | Total |
|---|---|---|---|
| `format` (XDR schema decision points) | 3,869 | 0 | 3,869 |
| `gap` (native type losses) | 9 | 4 | 13 |
| `core-test-section` (core's own transaction test sections) | 29,691 | 0 | 29,691 |
| `gate-side` (sides of core's protocol gates) | 375 by branch count, 130 inferred from an execution count | 185 | 690 |
| `field` (envelope fields asked of core) | 203 | 0 | 203 |
| `sdk-version` (SDK versions the corpus runs against) | 28 | 0 | 28 |
| `seed` (faults seeded into the SDKs) | 72 | 142 | 214 |

No obligation is unmeetable. Each open obligation carries its cause.

### 4.7 The candidate lane

`candidate/` has the corpus layout and holds entries that new generators wrote but that are not
in the release corpus yet. It has 6,245 entries (`candidate/manifest.json`): 3,869
`format.xdr-grammar` vectors, 2,276 probe witnesses of envelope fields, 71 probe witnesses of
limits, 14 explorer ledgers, and 15 synthetic entries for native type losses in the
`cap03.offer-liabilities`, `cap27.muxed-accounts` and `soroban.scval-shapes` families.

`shapekit corpus promote` moves an entry into the corpus under one rule. The current versions of
the rust, go and js SDKs must pass the entry, or a finding must take each failure. Every other
failure of any SDK version must be attributed to a finding, a candidate finding or a record in
`candidate/finding-candidates.toml`. Corpus 0.2.0 received 376 entries this way (STATUS.md,
2026-11-18).

## 5. Generation and verification

### 5.1 Recording a chain

`shapekit gen chains --family <family> --network both` runs each selected recipe under each
passphrase, each chain on its own core container. For each chain:

1. The captures of the two networks are paired by slug. Both runs must capture the same slugs,
   types, ledgers and core answers.
2. `passphrase_dependent` is set when the testnet and mainnet bytes differ.
3. Every variant must round-trip through `stellar-xdr` 30.0.0 and match a family fragment.
4. The entries go through the manifest validator.
5. Only when every check passes do the chains and entries move into the corpus.

Recipes are deterministic. Keys come from the recipe id and a role. Payloads and salts come
from hashes of fixed strings. Close times are fixed by the driver. Recipes do not depend on
absolute ledger numbers.

### 5.2 Normalization

Release core emits each list of ledger entry changes in a per-process order. The recorder
writes a canonical stream: it sorts every change list stably by ledger key and keeps the order
of each key's changes. It checks that the result is a permutation of core's output. So a
canonical ledger is a byte string that core itself can emit. Entries with change lists record
`source.normalization = "key-stable"`.

`shapekit gen verify-raw-order` compares the raw streams of two independent recordings. On
2026-11-13, 88 recordings (44 chains under both passphrases) gave 0 per-key order differences.
The recordings held 10,968 keys with two or more changes in one list
(`docs/adding-a-shape.md`).

### 5.3 Goldens

`shapekit import goldens` reads stellar-core's `test-lcm-current` goldens at tag `v30.0.0`.
For each family fragment it imports the smallest golden ledger whose protocol is in the family's
range, and runs the oracle. The index `corpus/goldens/core-v30.0.0/index.json` records 10
directories, 866 files and 15,306 ledgers, from which 107 entries are imported.

Golden agreement has two checks:

- Literal: `scripts/check-lcm.sh` runs core's own `test '[tx]' --check-lcm` in the pinned
  `-buildtests` image against the pinned goldens. The nightly `goldens` job runs it.
- Structural: `shapekit goldens agree` checks the structural agreement between the golden
  entries and the local-core or history entries of each family. On `main` it reports
  `28 families compared, 51 golden combinations checked, 0 failures`.

The release publishes the goldens it used as `goldens-core-v30.0.0.tar.zst`.

### 5.4 Reproducibility under both passphrases

`shapekit gen verify-repro` records chains again under both passphrases into a temporary
directory. It compares:

- per chain, the canonical ledger hashes, the ledger range and the SEP-54 lake files;
- per entry, the set of ids, every `entry.json`, and every `.xdr` and `.expected.json` file
  byte for byte;
- every `.oracle.json`, after it runs the oracle again.

Exit status 1 means a difference. Two workflows run it:

- `pr.yml` runs it for a rotating window of four families plus each family that the change
  touches. It runs every family when shared generator code, the registry, the contracts or the
  pins change. `scripts/select-repro.sh` packs the chains into shards of at most 360 seconds,
  by the measured seconds per chain in `scripts/repro-seconds.tsv`.
- `nightly.yml` runs it on every chain of every registered recipe.

`shapekit pack verify` checks every committed chain against its `chain.json` and checks that
the lake holds exactly the canonical form of `ledgers.xdr`.

### 5.5 Synthetic entries and the recipe explorer

`shapekit synth` writes the hand-built entries (`synthetic: true`) and the non-XDR vector kinds:
strkeys, orderings, textual ids and native projections. Examples are the bool word 2 in an
`ScVal` and in an envelope, and maps whose keys collide after conversion. Another example is an
asset pair whose issuers sort differently by strkey text and by XDR bytes.

The recipe explorer (`crates/shapekit-explore`, `shapekit explore`) fills transactions from the
XDR schema and the chain state. It runs them on a standalone core. It keeps a recipe when the
ledgers show a union arm, a signature path or a core code region not seen before. It writes
recipe IR. `shapekit explore record` records a kept recipe under both passphrases, and
`shapekit explore entries` cuts the fewest ledgers that hold the credited arms. The corpus holds
350 `explore.witnesses` entries and 23 `explore.pairs` entries. A pair is two recipes from one
start state that reach the same ledger entry in two ways.

### 5.6 Reproducible release archive

`shapekit corpus pack` writes `corpus-<version>.tar.zst` deterministically
(`crates/shapekit-coverage/src/pack.rs`). It refuses a file that the manifest or a
`chain.json` does not vouch for. `release.yml` builds the corpus archive and the golden archive
twice on separate runners and requires equal sha256 values.

## 6. SDK conformance

### 6.1 The testee protocol

A testee is a small program that wraps one SDK at one exact version. The runner starts it as a
child process and talks to it in JSON lines over stdin and stdout. The protocol is version 2
([docs/testee-protocol.md](testee-protocol.md),
`schema/testee-protocol.schema.json`).

`hello` reports the package, the exact linked version, the ops it implements and whether it
honours decode limits. The runner refuses a testee whose package or version differs from the
versions file. The 17 ops:

| Op | What the SDK does |
|---|---|
| `decode_encode` | Decodes bytes as one value of a type and encodes it again |
| `decode_json`, `encode_json` | Writes and reads SEP-51 XDR-JSON |
| `strkey_decode`, `strkey_encode` | SEP-23 strkey codec |
| `asset_order` | Compares two assets |
| `pool_id` | Derives a liquidity pool id |
| `map_sort`, `map_validate` | Sorts or checks an `ScMap` |
| `id_parse` | Reads a textual or JSON muxed account id or offer id |
| `to_native` | Converts an `ScVal` to a native value |
| `address_from_scval` | Converts an `ScVal` to the SDK's address type |
| `tx_hash` | Hashes a transaction envelope under a passphrase |
| `balance_id` | Derives a claimable balance id |
| `contract_id` | Derives a contract id |
| `coverage_snapshot`, `coverage_new` | Reports code coverage counters (for the fuzzers and seeded faults) |

Error classes are `rejected`, `panic`, `bad-request` and `internal`. A `panic` always counts as
`diverges-from-network`, because the correct answer to a bad input is a rejection.

### 6.2 The runner

`shapekit conformance run` (`crates/shapekit-conformance`) turns each manifest entry into
requests:

- `must-accept` XDR: the decode must succeed and round-trip; `decode_json` must equal the
  expected JSON; `encode_json` of the expected JSON must give the input bytes.
- `must-reject` XDR: the answer must be `rejected`.
- A `LedgerCloseMeta` that the network wrote: `tx_hash` for every envelope (and the inner
  transaction of each fee bump), `balance_id` and `contract_id` where they apply. The expected
  values come from the law library (`crates/shapekit-laws`) and are used only where the ledger
  confirms them.
- `implementation-defined` XDR: any answer that is not a crash passes. The matrix records each
  answer and the spread of answers.

The runner applies three limits to each testee:

- a per-request timeout, 10 s by default;
- a limit of 256 MiB per response line;
- a memory limit with `setrlimit`, `RLIMIT_AS` or `RLIMIT_DATA` per SDK.

An SDK version with a `max_protocol` reports a divergence on an entry from a later protocol as
`unsupported`. A crash is never downgraded this way.

The Rust testee renders XDR-JSON with the same library that wrote the expected files. So the
matrix marks it as the reference for those checks and counts them as `reference`, not `pass`.
`shapekit conformance crosscheck` sends every such check to the Go and JS testees as
independent evidence.

### 6.3 Known failures and the matrix

`conformance/known-failures/<sdk>.txt` lists one known failing check per line, with its result
and an issue link:

```text
<entry-id> <version or *> <result> <issue link or findings/issues/ draft> <check>[, <check>...]
```

A failing check without a line fails the run. A listed check that now passes, or fails in
another way, also fails the run. So the file always equals the run. The runner writes
`divergence-matrix.json` and `MATRIX.md`.

`shapekit conformance reproduce` runs the upstream sightings of
`conformance/reproductions.toml` on their pre-fix and fixed versions:

- js-stellar-sdk #1766 (issuer order and map key order);
- js-stellar-sdk #1785 (non-canonical ids);
- js-stellar-sdk #1789 (colliding map keys);
- two go-stellar-sdk sightings (asset ordering and non-canonical strkey characters);
- the stellar-xdr advisory GHSA-3xhm-p452-9wjh.

### 6.4 The 11 SDKs

`conformance/versions.toml` holds 28 versions of 11 SDKs. `shapekit conformance sync-versions`
writes it from the package registries. For each release line it takes the newest stable release
whose XDR knows protocol 29 or later. It reads the `max_protocol` of each version from the
stellar-xdr commit that generated the SDK's XDR code.

| SDK key | Package | Versions | Current | Testee | Failing entries at the current version |
|---|---|---|---|---|---|
| `rust` | `stellar-xdr` (crate) | 28.0.0 to 30.0.0 | 30.0.0 | Rust | 15 |
| `go` | `github.com/stellar/go-stellar-sdk` | v0.7.1 to v0.9.0 | v0.9.0 | Go | 6 |
| `js` | `@stellar/stellar-sdk` | 16.3.0 to 17.4.0 | 17.4.0 | TypeScript | 4 |
| `soroban` | `soroban-sdk` | 28.0.0, 29.0.1 | 29.0.1 | Rust, through the Soroban host | 0 |
| `python` | `stellar-sdk` (py-stellar-base) | 16.1.1, 17.0.1 | 17.0.1 | Python | 862 |
| `java` | `network.lightsail:stellar-sdk` | 5.0.0, 6.0.0 | 6.0.0 | Java | 841 |
| `dart` | `stellar_flutter_sdk` | 3.12.0 | 3.12.0 | Dart | 47 |
| `php` | `soneso/stellar-php-sdk` | 1.18.0 | 1.18.0 | PHP | 137 |
| `dotnet` | `stellar-dotnet-sdk` | 16.0.0, 17.0.0 | 17.0.0 | C# | 90 |
| `kmp` | `com.soneso.stellar:stellar-sdk-jvm` | 1.18.0 | 1.18.0 | Kotlin (JVM target) | 80 |
| `stellar-cli` | `stellar-cli` | 28.1.0, 30.0.1 | 30.0.1 | Rust wrapper of the release binary | 132 |

The last column counts the distinct entries that the SDK's known-failures file lists for the
current version or for `*` (`conformance/known-failures/*.txt`). The dashboard's SDKs page
takes the same counts from the same files (`shapekit site data`). Each line links an upstream
issue, a finding draft or a candidate record. The rust, go and js lines are depth, strkey and
textual id cases. They link rs-stellar-xdr #627 and #628, go-stellar-sdk #6046 and #6047,
js-stellar-sdk #1908, and one candidate finding. Most python and java failures are the SDK's
XDR-JSON names, such as `txsuccess` where SEP-51 has `tx_success` (STATUS.md, 2026-11-17).

Each testee pins its SDK. For example:

- the Python testee builds a venv per version from hash-locked requirements;
- the Java and KMP builds check their jars against `lib.sha256`;
- the stellar-cli testee checks each release asset against `asset.sha256`.

### 6.5 Core's own test metas

`shapekit coretest run` runs every `[tx]` test of the pinned `-buildtests` image at every
protocol. `shapekit coretest convert` checks every meta against core's committed short hashes.
`shapekit conformance run --dataset` sends each distinct meta through the SDKs.
[conformance/dataset/DATASET.md](../conformance/dataset/DATASET.md)
records 431,546 metas, 28,142 distinct, in 29,691 test sections. go v0.9.0, js 17.4.0 and rust
30.0.0 round-trip all 28,142.

## 7. The indexer replay harness

### 7.1 Run sequence

`shapekit harness run` (`crates/shapekit-harness`) replays each chain into an indexer through
an adapter:

1. It starts the adapter process and sends `describe`. A chain outside the adapter's protocol
   range is `out-of-scope`.
2. It creates a Docker network with `--internal` and starts the replay server in process, bound
   to the network's gateway address. The visible head is the bootstrap ledger.
3. It sends `start` and waits until the target has ingested the bootstrap ledger.
4. For each later ledger, it moves the head and checks that the server serves the ledger. Then
   it sends `advance` and polls `status` until the target ingests the ledger, dies or stops.
5. It sends `export` and compares the records with the oracle files. With the `dump`
   capability it also keeps every stored table.
6. It sends `stop`, then removes the network and the work directory.

Each chain gets a new adapter process, so a halt in one family does not hide the others. The
harness needs Linux with Docker. On macOS it stops and says so.

### 7.2 The replay server

`shapekit serve` (`crates/shapekit-serve`) serves one committed chain over HTTP. It follows
stellar-rpc v30.0.1 (commit `6b6ec60c8bde5a93984b6290065d3882d384b25d`) and uses the request
and response types of go-stellar-sdk v0.8.0 `protocols/rpc`
([docs/replay-server.md](replay-server.md)).

| Route | Answers |
|---|---|
| `POST /`, `POST /rpc` | JSON-RPC 2.0: `getHealth`, `getNetwork`, `getVersionInfo`, `getLatestLedger`, `getLedgers`, `getLedgerEntries`, `getTransaction`, `getTransactions`. `sendTransaction` is refused. |
| `GET /ledgers/<key>` | The chain's SEP-54 lake, up to the head |
| `GET /archive/<path>` | The history archive as it stood at the newest checkpoint at or below the head |
| `GET /control/head`, `POST /control/head` | Read or move the visible head |

The tests compare its behaviour with captures from the public testnet stellar-rpc 30.0.1
endpoint (`crates/shapekit-serve/tests/fixtures/stellar-rpc/`). The server keeps stellar-rpc's
batch limit of 10 calls and a 64 MiB response budget per request, and answers at most 4
requests at once.

`adapters/protocol/consumercheck` checks that go-stellar-sdk's own ingestion code accepts this
delivery: `BufferedStorageBackend` over the lake, `RPCLedgerBackend` against `shapekit serve`
and the history archive client.

### 7.3 The adapter protocol

The adapter protocol is version 2, JSON lines over stdin and stdout
([docs/adapter-interface.md](adapter-interface.md),
`schema/adapter-protocol.schema.json`). Commands: `describe`, `start`, `advance`, `status`,
`export`, `stop`, and the optional `dump` and `restart`.

- Delivery modes: `rpc`, `sep54-filesystem`, `sep54-s3` and `sep54-gcs`.
- Bootstrap: `checkpoint` (the target loads the archive's latest checkpoint) or `ledger`.
- Limits: a memory limit for the target (4 GiB by default), per-request timeouts, a bootstrap
  timeout and a per-ledger timeout.
- Claims: the adapter states which oracle facts the target records. A missing fact inside a
  claim is a finding. A fact outside the claims is `out-of-scope`.

`shapekit init adapter --lang go|ts|python` writes a skeleton that speaks the protocol. The Go
side of the protocol is `adapters/protocol/wire`.

### 7.4 The meta oracle

The oracle (`crates/shapekit-oracle`) reads only `LedgerCloseMeta`. It takes participants and
changes from the meta and its events, not from the envelope. For each transaction it writes the
actual participants, the changes and the fee. It keeps the values that the envelope declares
apart, and never uses them as expected participants. The vocabulary is
version 2 and has 31 change kinds, from `account-created` to `token-event`. Addresses are
canonical strkeys (`G`, `C`, `L`, `B`); a mux id never replaces the base address; signers keep
their `G`, `T`, `X` or `P` form.

The oracle crate also checks the conservation of lumens from meta alone, by the rule of core's
`ConservationOfLumens` invariant (`crates/shapekit-oracle/src/conservation.rs`).

### 7.5 Verdicts and checks

| Verdict | Meaning |
|---|---|
| `pass` | Every in-scope fact was exported with equal values, and nothing undecodable or extra was stored |
| `dropped` | An in-scope participant or change has no counterpart in the export |
| `mishandled` | A wrong value, an undecodable address, or a halt, crash, OOM kill or timeout at the entry's ledger |
| `not-evaluated` | The target stopped earlier, the entry is at or before the bootstrap ledger, or the chain could not run |
| `out-of-scope` | No claimed fact, a protocol outside the range, or a claimed event fact that the meta format cannot show |

More checks run on the stored data:

- Carry check: with `dump`, every stored cell is matched against the identifiers and amounts
  that the chain carries. A cell that is a strict prefix of a known address is `truncated`. A
  well-formed strkey that the chain cannot explain is `fabricated`.
- Replay invariance: `shapekit harness relate` compares the dumps of two runs with another
  delivery mode, a restart in the middle, or the other passphrase.
- State comparison: `shapekit harness state` compares the target's final accounts,
  trustlines, Stellar Asset Contract balances and pools with core's final entries.
- Free field inertness: `shapekit harness free-field` checks that a field core does not
  validate does not change what the indexer stores.
- Memory: `--repeat 2` runs the selection twice in one process and fails when memory grows
  past a slack, or when a later round gives other verdicts.

### 7.6 The three indexers

| Indexer | Pin | Adapter | Delivery | Result over every chain on both networks |
|---|---|---|---|---|
| wallet-backend (reference) | commit `3e22515a44979fdc3a558fda3991ca2b48e38cef` | `adapters/walletbackend` (Go), or `adapters/walletbackend/adapter.toml` (mapping file) | `rpc`, `sep54-s3` | 1,206 pass, 166 mishandled, 56 dropped, 224 out of scope |
| stellar-etl | tag `v2.8.35`, commit `7e3fc9af95359dad6ba939fa88791f49cdf531ae` | `adapters/stellaretl` (Go) | `sep54-gcs` | 1,388 pass, 28 dropped, 54 out of scope, 182 not evaluated |
| stellar-rpc | image `stellar/stellar-rpc:30.0.1`, digest `sha256:b6d41d3148087adf5d6856380a40854bd1e25b29d62a7ae484258e290e73b096` | `adapters/stellarrpc` (Go) | `sep54-filesystem` | 1,416 pass, 236 out of scope |

The results come from the committed reports in `findings/inputs/`
(`harness-wallet-backend-3e22515a.json`, `harness-stellar-etl-7e3fc9af.json`,
`harness-stellar-rpc-30.0.1.json`). Each report has 1,652 rows: 826 entries, each replayed on
testnet and on mainnet. Each indexer gives the same verdicts on both networks.

The dashboard's Indexers page and the landing page show each result as the replays with wrong
or missing rows, that is the replays with the verdict `mishandled` or `dropped`
(`foundText` in `site/src/lib/metrics.ts`):

| Indexer | Replays with wrong or missing rows |
|---|---|
| wallet-backend | 222 (166 mishandled, 56 dropped) |
| stellar-etl | 28 (all dropped) |
| stellar-rpc | none |

- wallet-backend: the adapter builds the commit's own `Dockerfile` with each base image pinned
  by digest, and runs it with `timescale/timescaledb:2.28.2-pg17`. It reads `transactions`,
  `transactions_accounts` and `state_changes`. wallet-backend writes classic state changes from
  a port of Horizon's effects. So four kinds of difference are intended and are listed with
  Horizon's source in `candidate/finding-candidates.toml`, not as findings.
- stellar-etl: the adapter runs `export_transactions`, `export_operations`, `export_effects`,
  `export_token_transfer` and `export_contract_events` for each released ledger. The 182 entries
  not evaluated are chains with transaction meta below version 4, which `export_token_transfer`
  does not read (finding ETL-META-V4, recorded as intended).
- stellar-rpc: the adapter checks that `getVersionInfo` reports
  `30.0.1-6b6ec60c8bde5a93984b6290065d3882d384b25d`. stellar-rpc backfills from the datastore,
  and the adapter reads events through `getEvents`, `getTransactions` or `getLedgers`
  (`--events-from`). `crates/shapekit-harness/expect/stellar-rpc.toml` holds its expected
  verdicts. `getLedgerEntries` cannot be checked, because stellar-rpc answers it from captive
  core, which follows the network only over SCP.

shapekit has no Horizon adapter.

### 7.7 Reproductions of the wallet-backend sightings

`adapters/walletbackend/reproductions.toml` holds 6 reproductions: wallet-backend #729, #731,
#716 (signed payload and muxed), #753 and #792. Each runs a pre-fix and a fixed commit.
`shapekit harness reproduce` reports `24 of 24 table rows hold (20 entry verdicts, 4 carry
checks)` (`docs/maintenance.md`, measured on 2026-11-17).

### 7.8 Mapping files for PostgreSQL indexers

An indexer that runs in Docker and stores in PostgreSQL needs no adapter program.
`shapekit check indexer --mapping <file>` runs the built-in mapped adapter
(`crates/shapekit-mapped`) from one TOML file. The file says how to build and start the
indexer and how ledgers reach it. It also says how to read its progress, and which tables and
columns hold which oracle facts ([docs/mapping-adapter.md](mapping-adapter.md)).
Through `adapters/walletbackend/adapter.toml`, wallet-backend at `3e22515a` gives the same
verdicts as the Go adapter on every entry (STATUS.md, 2026-11-18).

### 7.9 Sharding and expectations

`--shard <index>/<count>` replays one part of the chains. `shapekit harness merge` puts the
shard reports together. `--expect <file>` holds a run to an expectations file, `exact` or
`regressions`. In `regressions` mode only a new or changed `mishandled`, `dropped` or
`not-evaluated` verdict fails the run.

## 8. indexerlint

`indexerlint` (`lint/`) is a static analyzer built on `golang.org/x/tools/go/analysis`. It
checks Go indexer code built on go-stellar-sdk for the three ingestion mistakes of the guide
([docs/linter.md](linter.md)):

| Rule | Name | What it reports |
|---|---|---|
| IDX001 | unvalidated-field-trusted | A field that core does not validate is used as a key, stored, or converted lossily without a check |
| IDX002 | missing-depth-or-size-cap | A recursive XDR structure is walked, or a decoded length is allocated, without a cap; or the decode depth limit is turned off |
| IDX003 | participants-from-envelope | Participants come from the envelope and the function's call tree reads no transaction meta |

IDX001 reads its field list from `registry/fields.toml`, through a generated table
(`lint/internal/fieldtable`). A test fails when the table and the registry differ, so the linter
and the guide use the same facts.

Seeded tests: each rule has a testdata module that imports go-stellar-sdk v0.8.0. All 15
IDX001, 17 IDX002 and 11 IDX003 seeded violations are detected, with no unexpected finding.

On wallet-backend at the current pin `3e22515a`, indexerlint reports one finding: IDX002 on the
`walkInvocation` closure in `ContractDeployProcessor.ProcessOperation`. It is filed as
[wallet-backend#820](https://github.com/stellar/wallet-backend/issues/820) and accepted in
`lint/baselines/wallet-backend.json`. With that baseline the run exits 0. The nightly workflow
runs this check.

Run it with `indexerlint ./...`, `shapekit lint <module>`, or `go vet -vettool=$(command -v
indexerlint) ./...`. Exit codes: 0 no findings, 1 findings, 2 a usage, load or analysis error.

## 9. The probe and field validation

The probe (`crates/shapekit-probe`, `shapekit probe all`) asks the pinned core which envelope
fields it validates. Each attempt starts from one valid transaction, changes exactly one field,
signs again and submits it to a standalone core. Core refuses it at submission, includes it and
fails it, or includes it and applies it. Each attempt uses its own source account.

`registry/generated/validation.json` records 203 fields and 1,312 attempts: 174 fields are
validated and 29 are not. `registry/generated/limits.json` records 21 bounded quantities
measured by bisection on core, each with the largest accepted value, the first refused value
and where core refuses it. For example, the largest wasm module that uploads is 130,972 bytes,
and 130,973 bytes fails at apply with `resource_limit_exceeded`.

`shapekit guide check` compares each row of `registry/fields.toml` with the measured verdicts.
On `main` it reports `against core: 88 rows, 86 agree, 2 settled by core's source, 0 problems`.
The two settled rows cite the core file and lines that explain them
(`crates/shapekit-probe/src/compare.rs`).

The probe witnesses are entries of the candidate lane (`probe.envelope-fields`,
`probe.limits`).

## 10. The indexer-hardening guide

[docs/guide/indexer-hardening.mdx](guide/indexer-hardening.mdx) is
written for developers.stellar.org. Its sections:

1. Validated and unvalidated fields, with a field table and the corpus entries for each field.
2. Recommended caps, with a caps table.
3. Participants from meta, not from the envelope.
4. Muxed and signed-payload addresses, with every SEP-23 strkey form.
5. A self-audit checklist for indexers in any language.

`crates/shapekit-guide` generates the field and caps tables from the registry.
`shapekit guide check` fails when a table is stale, a field is not exercised, or a row disagrees
with the probe. On `main`: 88 fields in the table, 85 exercised by the 1,092 corpus entries.

`docs/guide/stellar-docs.patch` adds the page to stellar/stellar-docs as
`docs/data/indexers/build-your-own/indexer-hardening.mdx`, on stellar-docs `main` at `62f29bbb`.
Publication waits for review by a stellar-core or platform engineer (see 19.2).

## 11. Fuzzing, seeded faults and the coverage twin

### 11.1 Differential fuzzing

`shapekit fuzz` (`crates/shapekit-fuzz`) sends generated and mutated XDR values to `stellar-xdr`
30.0.0 in process and to the go-stellar-sdk testee. It reports every input on which they
disagree, groups the inputs into classes and shrinks one reproducer per class. Inputs come from
`stellar-xdr`'s `arbitrary` feature and from word-aware mutation of corpus entries. Both sides
use the corpus decode limits. A known depth accounting difference (go-stellar-sdk#6047) is
filtered only when lifting the depth limit alone makes both sides agree.

The nightly workflow runs 60,000,000 inputs with the UTC date as the seed, then replays the
stored reproducers in `findings/fuzz/`. The first run (2026-11-12) found one root cause in eight
classes. stellar-xdr reads a bool word other than 0 or 1 as false, and go-stellar-sdk refuses
it. Core's xdrpp reads the same word as true. It is filed as
[rs-stellar-xdr#629](https://github.com/stellar/rs-stellar-xdr/issues/629) and
[stellar-core#5509](https://github.com/stellar/stellar-core/issues/5509).

### 11.2 Coverage-guided search and each SDK's own fuzzer

`shapekit guided-fuzz` runs the coverage builds of the rust, go and js testees side by side. It
keeps an input that reaches new code in any of them. Over 100,000 inputs per run, the feedback
reached more code ranges in every SDK in both measured seeds
([conformance/guided-fuzz/seed-1.md](../conformance/guided-fuzz/seed-1.md),
[seed-2.md](../conformance/guided-fuzz/seed-2.md)).

`shapekit native-fuzz` runs cargo-fuzz on stellar-xdr, `go test -fuzz` on go-stellar-sdk and
jazzer.js on `@stellar/stellar-sdk`, checked by the encoding laws. `native-fuzz.yml` runs each
for 600 seconds every day. A new class goes to the candidate lane and does not fail the run.

### 11.3 Seeded faults

`shapekit conformance seed-check` plants faults in an SDK with its language's mutation tool and
counts the faults that the corpus notices
([docs/seeded-faults.md](seeded-faults.md)):

| SDK | Tool | Caught by the corpus | Caught with the candidate lane |
|---|---|---|---|
| stellar-xdr 30.0.0 | cargo-mutants 27.1.0 | 7 of 38 | 12 of 38 |
| go-stellar-sdk v0.9.0 | go-mutesting v2.3.1 | 14 of 76 | 14 of 76 |
| @stellar/stellar-sdk 17.4.0 | Stryker 10.1.1 | 36 of 100 | 46 of 100 |

The catch rate is a lower bound on what the corpus checks. Each missed fault has a class in
`conformance/seed-check/classes/<sdk>.json`. Most are in code that no testee op calls.

### 11.4 The coverage twin

The coverage twin (`crates/shapekit-twin`, `twin/`) is stellar-core v30.0.0 built with
`--enable-codecoverage`, packaged as a local image. `shapekit twin verify` checks that the twin
writes the same canonical ledgers as the release image. The twin measures:

- which regions of core's transaction and ledger code each chain reaches
  (`corpus/coverage/core-coverage.json`);
- the protocol gates: `shapekit twin gates` lists the 345 protocol version comparisons in core's
  non-test source (`registry/generated/gates.json`), and `shapekit twin gate-witnesses` marks the
  sides that a chain takes. 375 of the 690 sides are met by branch count and 130 more are
  inferred from an execution count;
- the Soroban host coverage of a chain (`twin/host-cov.sh`), where each replayed invocation must
  give the same outcome that core recorded.

## 12. Public network checks

The law book (`crates/shapekit-laws`) holds 11 laws that every `LedgerCloseMeta` must satisfy.
The truth comes from the input itself or from hashes the network signed:

| Law | Statement |
|---|---|
| `roundtrip` | Decoding and re-encoding gives the input bytes |
| `noncanonical-rejected` | The input with a trailing word, or with its last byte cut, does not decode |
| `header-hash` | The sha256 of the header is the hash stored next to it |
| `header-chain` | `previousLedgerHash` is the hash of the ledger before |
| `tx-set-hash` | The transaction set's hash is `scpValue.txSetHash` |
| `tx-set-previous` | The transaction set names the previous ledger hash |
| `result-set-hash` | The sha256 of the result set is `txSetResultHash` |
| `tx-hash` | The envelope hashes are the hashes core wrote into the results |
| `pool-id` | Pool ids derive from the pool parameters |
| `contract-id` | A created contract lives at the id derived from its preimage |
| `balance-id` | A created claimable balance has the id derived from its operation |

`crates/shapekit-sentinel` reads the public SEP-54 lake and streams the ledgers. It writes
nothing to disk.

- `audit.yml` runs daily. It draws 3,000 distinct ledgers from the whole pubnet history with the
  date as the seed. It checks every law, and runs each ledger through the rust, go and js testees
  at their current versions (`decode_encode`, `tx_hash`, `balance_id`, `contract_id`). A ledger
  that the lake does not return fails the job.
- `sentinel.yml` runs every hour. For pubnet and testnet it checks every ledger closed after the
  cursor on the `sentinel-state` branch, at most 1,500 per run. It also checks the header
  chain across runs.

## 13. Findings and triage

`crates/shapekit-findings` maps every non-pass harness verdict, every SDK divergence and every
linter finding to a triaged root cause. `findings/triage.toml` names the inputs and the
matchers. Each finding has a severity, a component, its CAPs, a minimal corpus entry and a
reproduce command. Its upstream status is `filed`, `duplicate-of`, `wont-fix`,
`private-report` or `draft`.

`shapekit findings check` fails while any failure has no finding, any finding has no upstream
link or decision, or a committed input changed since its import. It compares each harness
report's replay digest with the committed manifest.

| Record | Command output on `main` | By status |
|---|---|---|
| Main ([findings/FINDINGS.md](../findings/FINDINGS.md)) | `25 findings, 891 failures, 0 unmapped; 0 of 25 findings pending filing (status draft)`; 215 more failures held by candidate findings | 16 filed, 7 duplicates, 2 won't fix |
| Candidate lane (`candidate/findings/FINDINGS.md`) | `46 findings, 847 failures, 0 unmapped; 1 of 46 findings pending filing (status draft): PHP-UINT64` | 27 filed, 7 duplicates, 11 won't fix, 1 draft |

The main record covers wallet-backend (11 findings), go-stellar-sdk (5), js-stellar-sdk (5),
rs-stellar-xdr (3) and stellar-core (1) (`findings/findings.json`). The candidate record covers
the Dart, PHP, .NET and KMP SDKs, stellar-cli, stellar-etl and go-stellar-sdk.

A reported finding is a finding with an upstream decision: `filed`, `duplicate-of` or
`private-report`. Over both records there are 57 reported findings in 11 components: 23 in the
main record and 34 in the candidate record. They link 42 issues filed upstream by shapekit and
13 issues reported before by others. Two or more findings can link the same issue, so each
issue counts once. The landing page and every dashboard page take this split from one function,
`issueSplit` in `site/src/lib/metrics.ts`, over the data that `shapekit site data` writes from
both `findings.json` files.

Filing: `shapekit conformance draft-issues` writes one issue body per SDK and diverging entry.
Each body lists the other SDKs that diverge on the same entry, so one defect becomes sibling
issues. `shapekit conformance file-issues` opens the drafts with `gh` and refuses to run without
`--confirm`. Security-sensitive findings go through the project's private disclosure channel.

`shapekit report <run>` writes one self-contained HTML page per run. It groups failures by
finding and links the smallest entry, the upstream issue, the guide sections and the rerun
command ([docs/report.md](report.md)).

### 13.1 Upstream response

On 2026-11-20 the public issue trackers show this response to the 42 issues that shapekit filed:

- 6 issues are fixed and closed, each by a merged pull request:

  | Issue | Fixed by |
  |---|---|
  | [stellar-php-sdk#239](https://github.com/Soneso/stellar-php-sdk/issues/239) | [stellar-php-sdk#251](https://github.com/Soneso/stellar-php-sdk/pull/251) |
  | [stellar-php-sdk#242](https://github.com/Soneso/stellar-php-sdk/issues/242) | [stellar-php-sdk#250](https://github.com/Soneso/stellar-php-sdk/pull/250) |
  | [stellar-php-sdk#244](https://github.com/Soneso/stellar-php-sdk/issues/244) | [stellar-php-sdk#249](https://github.com/Soneso/stellar-php-sdk/pull/249) |
  | [stellar_flutter_sdk#292](https://github.com/Soneso/stellar_flutter_sdk/issues/292) | [stellar_flutter_sdk#302](https://github.com/Soneso/stellar_flutter_sdk/pull/302) |
  | [stellar_flutter_sdk#296](https://github.com/Soneso/stellar_flutter_sdk/issues/296) | [stellar_flutter_sdk#303](https://github.com/Soneso/stellar_flutter_sdk/pull/303) |
  | [kmp-stellar-sdk#168](https://github.com/Soneso/kmp-stellar-sdk/issues/168) | [kmp-stellar-sdk#174](https://github.com/Soneso/kmp-stellar-sdk/pull/174) |

- 12 more issues are confirmed by the maintainers, who state that they will fix them:
  stellar_flutter_sdk
  [#291](https://github.com/Soneso/stellar_flutter_sdk/issues/291),
  [#293](https://github.com/Soneso/stellar_flutter_sdk/issues/293),
  [#294](https://github.com/Soneso/stellar_flutter_sdk/issues/294),
  [#295](https://github.com/Soneso/stellar_flutter_sdk/issues/295) and
  [#297](https://github.com/Soneso/stellar_flutter_sdk/issues/297); stellar-php-sdk
  [#240](https://github.com/Soneso/stellar-php-sdk/issues/240),
  [#241](https://github.com/Soneso/stellar-php-sdk/issues/241) and
  [#243](https://github.com/Soneso/stellar-php-sdk/issues/243); kmp-stellar-sdk
  [#169](https://github.com/Soneso/kmp-stellar-sdk/issues/169),
  [#170](https://github.com/Soneso/kmp-stellar-sdk/issues/170),
  [#171](https://github.com/Soneso/kmp-stellar-sdk/issues/171) and
  [#172](https://github.com/Soneso/kmp-stellar-sdk/issues/172).
- [rs-stellar-xdr#629](https://github.com/stellar/rs-stellar-xdr/issues/629) is closed as a
  duplicate, and the crate's main branch refuses a bool word other than 0 or 1 since
  2026-11-19 (section 19.4).

The six fixes are merged after the newest release of each SDK (stellar-php-sdk 1.18.0,
stellar_flutter_sdk 3.12.0, kmp-stellar-sdk 1.18.0). `upstream-drift.yml` runs the corpus on
the next release of each SDK and lists the findings whose entries then pass (section 18.2).

## 14. CI and automation

All CI runs on GitHub-hosted runners. There are no self-hosted runners, servers or databases
([docs/infrastructure.md](infrastructure.md)).

| Workflow | Trigger | What it does |
|---|---|---|
| `pr.yml` | Every pull request, pushes to `main` | Rust format, clippy and tests; Go vet, tests and golangci-lint; every testee's build, tests and corpus run at its current version; contract rebuild; corpus checks (registry, manifest, oracle, coverage, guide, goldens, pack, findings, public text, links); `gen verify-repro` shards; the wallet-backend harness on three testnet chains |
| `nightly.yml` | Daily 03:00 UTC | Goldens and `--check-lcm`; every chain regenerated; docker tests; every SDK version and the reproductions; fuzzing; the candidate lane; the harness over every chain (wallet-backend in 6 rpc shards and 4 sep54-s3 shards, stellar-etl in 5, stellar-rpc in 3 per method); the wallet-backend reproductions; the findings check |
| `native-fuzz.yml` | Daily 04:30 UTC | Each SDK's own fuzzer for 600 seconds |
| `audit.yml` | Daily 04:40 UTC | The law book and three SDKs over 3,000 pubnet ledgers |
| `sentinel.yml` | Hourly at minute 17 | The law book over every new pubnet and testnet ledger |
| `upstream-drift.yml` | Weekly, Monday 06:00 UTC | `pins verify`, the SDK version check, and one tracking issue per upstream change (see section 18) |
| `release.yml` | A pushed `v*` tag | The signed release (see section 15) |
| `site.yml` | Changes to the site, docs or dashboard inputs | Builds and checks the site |
| `sdk-conformance.yml` | `workflow_call` | Reusable: runs one SDK's testee against the calling SDK repository's checkout |
| `indexer-conformance.yml` | `workflow_call` | Reusable: replays the corpus into the calling indexer repository's build |

Measured durations are in
[docs/infrastructure.md](infrastructure.md#measured-durations).
A `pr.yml` run on 2026-11-14 took 7 minutes 29 seconds. A `nightly.yml` run on 2026-11-14 took
2 hours 13 minutes on the 716-entry corpus. The durations of the sharded harness legs on the
1,092-entry corpus are estimates until a GitHub run measures them.

The `main` branch ruleset requires a pull request, resolved conversations and nine `pr.yml`
status checks, and blocks force pushes (`docs/maintenance.md`, "Repository settings").

## 15. Release, supply chain and privacy

Pins:

- `pins.toml` pins every upstream reference. It holds toolchains, container images by digest,
  git commits, release assets, and packages from crates.io, npm, the Go proxy, PyPI and Maven.
  `shapekit pins verify` checks each one against its live source, and `upstream-drift.yml`
  runs it every week.
- Workflows read pins through `scripts/pin.sh`. A pin never moves by an edit of a workflow.
- Every GitHub Action in every workflow is pinned by commit sha, for example
  `actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1`.
- The wallet-backend and stellar-etl adapters build each `FROM` image of the upstream
  Dockerfile on a digest from `pins.toml` (`BaseImages`, `adapters/pinning`).
- Toolchains, each pinned with its archive checksum:
  - Rust 1.99.1, Go 1.27.4 and Node.js 26.13.1;
  - CPython 3.15.2 with uv 0.13.9;
  - Temurin JDK 27.0.1 with Maven 3.10.1;
  - Flutter 3.50.0;
  - PHP 8.5.12 with Composer 2.11.2, and libsodium 1.0.22 (sha256 and minisign key);
  - .NET SDK 11.0.100.

Release (`release.yml`):

1. It refuses a tag whose signature GitHub does not verify.
2. It builds `shapekit`, `indexerlint` and `wb-adapter` for Linux and macOS on x86-64 and arm64,
   each on a runner of that platform.
3. It builds the corpus archive and the golden archive twice on separate runners and requires
   equal sha256 values.
4. It publishes every asset with one `SHA256SUMS`, a build provenance attestation per asset and
   release notes from `STATUS.md`.

v0.2.0 holds 14 assets and their `SHA256SUMS`. The file gives, for example,
`6fa534d82f6cb46b39311904ff2d6f1233e2b39f71aef391afcdce664fa65c04` for `corpus-0.2.0.tar.zst`
and `9b6c29ba1be088b76678777523b1cbcd17fc2c6194c6306e6dda33e890b6484e` for
`shapekit-linux-x86_64` (README.md). To check an asset:

```sh
sha256sum -c --ignore-missing SHA256SUMS
gh attestation verify shapekit-linux-x86_64 --repo ErenTopaal/shapekit
```

Two independent local builds of the Linux x86-64 assets from clean checkouts gave the same
`SHA256SUMS` (STATUS.md, 2026-11-18).

Privacy: shapekit collects no telemetry. It does not phone home, report usage or check for
updates. Only named commands use the network, each when you run it explicitly
([docs/infrastructure.md](infrastructure.md#network-access-and-telemetry)).
Security reports about shapekit go through GitHub security advisories (`SECURITY.md`).

## 16. Site and dashboard

<https://shapekit.org> is built from `site/`: React Router 8.5.1 in framework mode on Vite
8.3.8, Fumadocs 16.19.0 for the docs, Tailwind CSS 4 and shadcn/ui
([docs/site.md](site.md)). Every page is prerendered to a
static file. The site has no analytics, no cookies and no third-party requests.

- `/docs`: one page per document in `docs/`, including this one at
  <https://shapekit.org/docs/architecture>, and pages made from `README.md`, with `llms.txt`,
  `llms-full.txt` and a markdown copy of each page.
- `/dashboard`: one page for each question a reader asks, in this order:
  - Start: the summary of every other page;
  - SDKs: the diverging entries of each SDK at each version, from its known-failures file;
  - Indexers: each indexer's verdicts, shown as the replays with wrong or missing rows;
  - Findings: the reported findings and the split into upstream issues (section 13);
  - Corpus: coverage, with one page per CAP, the entry list and one page per corpus entry. An
    entry page shows its decoded XDR-JSON, its provenance, its results and the commands that run
    it again;
  - Glossary: one meaning for each term that the pages use.

  A Sources page lists the pinned upstream versions, the docs and every committed file that the
  data comes from. A term on a dashboard page shows its short definition in a tooltip on hover,
  keyboard focus or tap, and links to its glossary entry (`site/src/components/term.tsx`).

`shapekit site data` writes the dashboard data from committed files only. The output is
deterministic. `crates/shapekit-cli/tests/site_data.rs` checks that the summaries equal the
source files. `site.yml` builds the site and checks that every docs, CAP, dashboard and entry
page has a prerendered file. Hosting is outside the repository.

## 17. Integration with Stellar projects

shapekit applies on the RFP track. The integration-track building blocks are anchors and ramps,
DeFi protocols, bridges, the Stellar Disbursement Platform and wallet connection layers.
shapekit uses none of them, because it is test infrastructure for SDKs and indexers. Its
integration is with the Stellar components below.

### 17.1 Components and interfaces

| Component | Pin | Interface | Integration status |
|---|---|---|---|
| stellar-core | 30.0.0 image by digest, source tag `v30.0.0` | Docker container on an internal network, core's HTTP commands, the meta stream, the history archive; `test --check-lcm`; `[tx]` test dumps | Built: generator, probe, explorer, coverage twin, goldens |
| stellar-xdr definitions and crate | tag `v30.0`, crate 30.0.0 | `ReadXdr` and `WriteXdr` under core's limits, the definitions JSON | Built: every decode, round trip and schema check |
| SEP-54 public ledger lake | public bucket | `.config.json` and zstd batches | Built: captures, audit, sentinel |
| stellar-rpc | v30.0.1, image by digest | Replay server follows its JSON-RPC; adapter runs the release image | Built: replay server and harness target |
| wallet-backend | commit `3e22515a` | Go adapter and mapping file; TimescaleDB rows | Built: reference adapter, 6 reproductions, linter target |
| stellar-etl | v2.8.35 | Go adapter; export commands over a GCS emulator | Built: second harness target |
| 11 SDKs and tools | `conformance/versions.toml` | Testee protocol v2 per SDK | Built: 28 versions in the matrix |
| stellar-docs | `main` at `62f29bbb` | `docs/guide/stellar-docs.patch` | Ready; waits for review |

### 17.2 Adoption by an SDK repository

An SDK repository calls the reusable workflow `sdk-conformance.yml`. The SDK repository keeps
three things ([docs/sdk-ci.md](sdk-ci.md)):

- a pinned release: the tag, the archive name and two sha256 values;
- a versions file that builds the testee against the pull request's checkout;
- a known-failures directory.

The workflow file from the docs:

```yaml
# .github/workflows/shapekit.yml in the sdk repository
name: shapekit conformance
on: [pull_request]
permissions:
  contents: read
jobs:
  corpus:
    uses: ErenTopaal/shapekit/.github/workflows/sdk-conformance.yml@v0.2.0
    with:
      sdk: js
      shapekit-tag: v0.2.0
      corpus-asset: corpus-0.2.0.tar.zst
      corpus-sha256: 6fa534d82f6cb46b39311904ff2d6f1233e2b39f71aef391afcdce664fa65c04
      shapekit-sha256: 9b6c29ba1be088b76678777523b1cbcd17fc2c6194c6306e6dda33e890b6484e
      versions-file: ci/shapekit/versions.toml
      known-failures: ci/shapekit/known-failures
```

`sdk` takes any of the 11 keys. The workflow sets up only that SDK's toolchain, checks both
sha256 values, runs `shapekit conformance run` with the caller's files and then `shapekit
report`. It uploads the matrix and the report, and with `comment: true` it posts a pull request
comment. `docs/sdk-ci.md` has a versions file and a workflow file for each SDK. Each versions
file builds the testee against the SDK checkout:

| SDK | How the testee uses the checkout |
|---|---|
| Rust (`stellar-xdr`), soroban-sdk | A path dependency |
| Go | A `replace` directive |
| JavaScript, Python | An install from the checkout |
| Java | A jar from the Gradle build |
| KMP | The JVM jar |
| Dart | A pub path dependency |
| PHP | A Composer path repository |
| .NET | A project reference |
| stellar-cli | The `stellar` binary built from the checkout |

An SDK pins a release, never a branch. Moving to a newer corpus is a deliberate change in the
SDK's own repository.

### 17.3 Adoption by an indexer repository

An indexer repository calls the reusable workflow `indexer-conformance.yml`. It keeps a pinned
release, an adapter or a mapping file, and optionally an expected-verdicts file
([docs/indexer-ci.md](indexer-ci.md)). The wallet-backend form
from the docs:

```yaml
jobs:
  corpus:
    permissions:
      contents: read
      pull-requests: write
    uses: ErenTopaal/shapekit/.github/workflows/indexer-conformance.yml@v0.2.0
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

stellar-etl uses the same form with `etl-adapter`. For stellar-rpc, `setup` builds the image
from the checkout and the adapter runs it with `--image stellar-rpc:pr --version pr --commit
${{ github.sha }}`. `docs/indexer-ci.md` records a measurement of that stellar-rpc setup. The
image build from an empty cache took 935 s. The replay of the three chains on testnet took 74 s,
and all 35 entries passed.

With `expect-mode: regressions`, a pull request fails only on a worse verdict. With `shards`,
the replay splits across jobs and a merge job puts the reports together.

The wallet-backend and stellar-etl adapters keep a table of go-stellar-sdk versions and their
newest protocol (`sdkProtocols`). A commit that bumps go-stellar-sdk past the table still runs.
It gets the protocol of the newest listed version before it, and the run records a note.

### 17.4 Adoption proposals

On 2026-11-19 one proposal issue per project asked to run the corpus in that project's CI with
the reusable workflows (STATUS.md). The state on 2026-11-20:

| Project | Proposal | State |
|---|---|---|
| rs-stellar-xdr | [stellar/rs-stellar-xdr#646](https://github.com/stellar/rs-stellar-xdr/issues/646) | Open |
| go-stellar-sdk | [stellar/go-stellar-sdk#6059](https://github.com/stellar/go-stellar-sdk/issues/6059) | Open |
| js-stellar-sdk | [stellar/js-stellar-sdk#1914](https://github.com/stellar/js-stellar-sdk/issues/1914) | Open |
| rs-soroban-sdk | [stellar/rs-soroban-sdk#2164](https://github.com/stellar/rs-soroban-sdk/issues/2164) | Open |
| stellar-cli | [stellar/stellar-cli#2896](https://github.com/stellar/stellar-cli/issues/2896) | Open |
| py-stellar-base | [StellarCN/py-stellar-base#1277](https://github.com/StellarCN/py-stellar-base/issues/1277) | Open |
| java-stellar-sdk | [lightsail-network/java-stellar-sdk#845](https://github.com/lightsail-network/java-stellar-sdk/issues/845) | Open |
| stellar_flutter_sdk | [Soneso/stellar_flutter_sdk#298](https://github.com/Soneso/stellar_flutter_sdk/issues/298) | Closed by the maintainers on 2026-11-19 |
| stellar-php-sdk | [Soneso/stellar-php-sdk#247](https://github.com/Soneso/stellar-php-sdk/issues/247) | Closed by the maintainers on 2026-11-19 |
| dotnet-stellar-sdk | [Beans-BV/dotnet-stellar-sdk#279](https://github.com/Beans-BV/dotnet-stellar-sdk/issues/279) | Open |
| kmp-stellar-sdk | [Soneso/kmp-stellar-sdk#173](https://github.com/Soneso/kmp-stellar-sdk/issues/173) | Closed by the maintainers on 2026-11-19 |
| wallet-backend | [stellar/wallet-backend#835](https://github.com/stellar/wallet-backend/issues/835) | Open |
| stellar-etl | [stellar/stellar-etl#519](https://github.com/stellar/stellar-etl/issues/519) | Open |
| stellar-rpc | [stellar/stellar-rpc#1202](https://github.com/stellar/stellar-rpc/issues/1202) | Open |

The maintainers of stellar_flutter_sdk, stellar-php-sdk and kmp-stellar-sdk keep their CI on
workflows they maintain themselves, so they closed the proposals. They state that each fix for
the reported issues comes with regression tests in the SDK's own test suite. shapekit runs these
SDKs from its own repository (section 17.5).

### 17.5 Integration without an upstream change

shapekit also runs every SDK and indexer from its own repository, so the results do not depend
on adoption. `nightly.yml` runs every SDK version and the three indexers at their pins.
`upstream-drift.yml` runs each new SDK or indexer release over the corpus and opens an issue in
the shapekit repository. Nothing is written upstream by a workflow.

## 18. Maintenance and protocol upgrades

### 18.1 Commitment

Eren Topal maintains shapekit from the production release through the first protocol upgrade
after delivery, protocol 31. CAP-89 and CAP-90 target protocol 31; both were in Draft status on
2026-11-18. The commitment ends when a shapekit release that covers protocol 31 is published by
the runbook, with every CI tier green
([docs/maintenance.md](maintenance.md)). After that the
repository stays public under Apache-2.0. The corpus format, the testee protocol and the adapter
interface are documented, so they do not depend on the repository staying active.

### 18.2 Upstream drift

`upstream-drift.yml` runs every Monday. It opens a tracking issue for:

- a new stellar-core, stellar-xdr or stellar-cli release;
- a change to core's golden index, or a change to a CAP text;
- an SDK registry with a newer current version than `conformance/versions.toml`, with the
  corpus run at that version;
- a newer wallet-backend `main` commit, stellar-etl tag or stellar-rpc tag, with the harness run
  at that version.

An issue for a core, XDR, CLI, golden or CAP change holds the runbook checklist and the output
of `scripts/upstream-regenerate.sh` against the new version. Each SDK or indexer issue lists the
findings whose entries now pass, the entries that newly fail, and the commands that move the
pin. Moving a pin is always a pull request.

### 18.3 The protocol upgrade runbook

[docs/protocol-upgrade-runbook.md](protocol-upgrade-runbook.md)
has 11 steps:

1. Read what changed: fetch the new XDR definitions, `schema diff`, `schema first-seen`, the CAP
   texts that name the protocol.
2. Move the pins in `pins.toml`; `shapekit pins verify` passes.
3. Move the Rust stack and the protocol constants, recapture testnet's Soroban settings, and
   record the smoke chain on both networks.
4. Add registry records for the new CAPs, families, fields and limits.
5. Write recipes for new families, regenerate every chain under both passphrases, and
   regenerate vectors, the gate list and the core test dataset.
6. Import the goldens at the new tag; `goldens verify`, `check-lcm.sh` and `goldens agree` pass.
7. `coverage --check` and `guide check` pass.
8. `conformance sync-versions`, `conformance run --all-versions` and `conformance reproduce`.
9. Update the adapter protocol tables and the indexer pins; `harness run` and
   `harness reproduce` pass.
10. Run `indexerlint` over the adapters and the new wallet-backend pin.
11. `scripts/acceptance.sh --full` passes, a `STATUS.md` entry, a signed tag.

The runbook also defines a next tier. It rehearses the steps before a protocol release, with
core's next-protocol image, and writes only under `next/`.

### 18.4 Protocol 31 rehearsal

The runbook already ran against protocol 31 on core's next-protocol image (core `362dbc51`).
The outputs are in [next/](../next):

- the smoke chain recorded at protocol 31 under both passphrases;
- 109 entries imported from `test-lcm-next`;
- draft registry records for the protocol 31 CAPs;
- `next/rehearsal-build.diff`, the code changes the protocol 31 build needs;
- the protocol gates of the next core source.

Results (`docs/maintenance.md`): `shapekit schema diff v30.0 next` shows 2 names added and none
removed. All 144 `[tx]` test cases pass at every protocol from 0 to 31, and all 452,555 metas
they write match core's `test-tx-meta-baseline-next`.

### 18.5 Rules for changes

A change to an expected value, a known-failures line or a reproduction table names its
upstream source: core behaviour, a CAP or an SEP. A known-failures line needs an issue
link and is never added to silence a red run. Generated files change only through their command,
and CI checks this. `scripts/acceptance.sh` is the gate for every change to `main`.

## 19. Planned work

This section lists only work that the repository or the public issue trackers record. It has no
dates, because none are recorded.

### 19.1 Adoption in upstream CI

11 of the 14 proposals of section 17.4 are open and wait for each project's decision. The
reusable workflows exist at v0.2.0. `docs/sdk-ci.md` and `docs/indexer-ci.md` hold the files
that each SDK and indexer repository needs.

### 19.2 Guide publication

The guide is a draft until a stellar-core engineer or a platform engineer at SDF reviews it.
The page goes to developers.stellar.org only after that review. Then `STATUS.md` records the
reviewer, the date and the reviewed commit. The patched pages pass stellar-docs' prettier
configuration and its pad check. `mint validate` and `mint broken-links` have not been run on
them (README.md).

### 19.3 Protocol 31

The maintenance commitment runs through protocol 31. The work follows the runbook (18.3) when
stellar-core, stellar-xdr and the crates release protocol 31. The rehearsal (18.4) has already
produced the build changes and the next-tier outputs.

### 19.4 Open items in STATUS.md and the coverage files

- rs-stellar-xdr refuses a bool word other than 0 or 1 on its main branch since 2026-11-19. No
  release holds the fix yet. The drift workflow reports the findings that pass at a new release.
- PHP-UINT64 stays a draft.
  [stellar-php-sdk#229](https://github.com/Soneso/stellar-php-sdk/pull/229) fixes it on the
  SDK's main branch. It is rerun on the next release and filed only if it still fails there.
- The nightly harness leg durations on the 1,092-entry corpus are estimates until a GitHub run
  measures them.
- 14 explorer ledgers stay in the candidate lane. They are cut before their chain's first
  checkpoint, so wallet-backend and stellar-rpc could evaluate no entry of their chain.
- Open obligations: 4 `gap`, 185 `gate-side` and 142 `seed`, each with its cause in
  `corpus/coverage/obligations.json`.
- The six upstream fixes of section 13.1 are on the main branches of stellar-php-sdk,
  stellar_flutter_sdk and kmp-stellar-sdk. The corpus runs them at each SDK's next release.
- 5 of the 441 union arms that a ledger can hold, and CAP-83, have no core-emitted entry. Each
  has a blocker and a reason (section 4.5).

## 20. Links

| What | Link |
|---|---|
| Repository | <https://github.com/ErenTopaal/shapekit> |
| Release v0.2.0 | <https://github.com/ErenTopaal/shapekit/releases/tag/v0.2.0> |
| Status log | [STATUS.md](../STATUS.md) |
| Site | <https://shapekit.org> |
| Docs | <https://shapekit.org/docs> |
| Dashboard | <https://shapekit.org/dashboard> |
| Corpus entries | <https://shapekit.org/dashboard/corpus/entries> |
| Findings | <https://shapekit.org/dashboard/findings> |
| Corpus coverage per CAP | [corpus/coverage/COVERAGE.md](../corpus/coverage/COVERAGE.md) |
| Findings record | [findings/FINDINGS.md](../findings/FINDINGS.md) |
| Adding a shape | [docs/adding-a-shape.md](adding-a-shape.md) |
| Testee protocol | [docs/testee-protocol.md](testee-protocol.md) |
| SDK CI | [docs/sdk-ci.md](sdk-ci.md) |
| Adapter interface | [docs/adapter-interface.md](adapter-interface.md) |
| Mapping file | [docs/mapping-adapter.md](mapping-adapter.md) |
| Replay server | [docs/replay-server.md](replay-server.md) |
| Indexer CI | [docs/indexer-ci.md](indexer-ci.md) |
| indexerlint | [docs/linter.md](linter.md) |
| Indexer-hardening guide | [docs/guide/indexer-hardening.mdx](guide/indexer-hardening.mdx) |
| Infrastructure | [docs/infrastructure.md](infrastructure.md) |
| Maintenance | [docs/maintenance.md](maintenance.md) |
| Protocol upgrade runbook | [docs/protocol-upgrade-runbook.md](protocol-upgrade-runbook.md) |
| Differential fuzzing | [docs/differential-fuzzing.md](differential-fuzzing.md) |
