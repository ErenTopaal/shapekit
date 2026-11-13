# Adding a shape

How to add a shape family to the corpus and generate its core-emitted entries. A new CAP that
changes what an indexer sees goes through the same steps.

The pieces:

- **Family**: a record in `registry/families.toml` naming the CAPs, the protocol range and the
  signature fragments that define the shape.
- **Recipe**: a Rust program in `crates/shapekit-gen/src/recipe/` that drives a private
  standalone stellar-core and captures what it emits.
- **Chain**: one recorded run of a recipe at one protocol under one passphrase, kept under
  `corpus/chains/<chain-id>/<network>/`.
- **Entry**: one captured value, written to
  `corpus/entries/<family>/<entry-id>/` with a `testnet` and a `mainnet` variant.

`crates/shapekit-gen/src/recipe/cap40.rs` is the worked example. Read it next to this page.

## 1. The family record

Add a `[[family]]` to `registry/families.toml`:

```toml
[[family]]
id = "cap40.signed-payload-signer"
title = "Ed25519 signed-payload signers on accounts, in extra signers and in sponsorship revokes"
caps = ["CAP-0040"]
protocol_introduced = 19
protocol_valid = { min = 19 }
signature = [
    "SignerKey:ed25519_signed_payload",
    ".body:set_options.signer.key:ed25519_signed_payload",
    ".extra_signers[]:ed25519_signed_payload",
]
source_classes = ["local-core", "testnet-history"]
fields = ["op.set_options.signer", "tx.cond.extra_signers"]
linked_issues = ["https://github.com/stellar/wallet-backend/pull/729"]
priority = 1
```

`signature` holds fragments in the grammar documented at the top of
`crates/shapekit-manifest/src/signature.rs`. An entry belongs to the family when the shape
signature of its bytes matches at least one fragment. Run `shapekit registry check` after editing.
Every id in `fields` must exist in `registry/fields.toml`.

## 2. The recipe

Create `crates/shapekit-gen/src/recipe/<name>.rs` and register it in
`crates/shapekit-gen/src/recipe/mod.rs`: one `pub mod <name>;` line and one line per protocol in
`all()`. Nothing else needs to change. The CLI, extraction, checks and reproducibility comparison
pick it up from `all()`.

```rust
pub struct MyRecipe { pub protocol: u32 }

impl Recipe for MyRecipe {
    fn id(&self) -> &str { "cap99.my-shape" }      // derives every key; usually the family id
    fn family(&self) -> &str { "cap99.my-shape" }  // registry family of every capture
    fn protocol(&self) -> u32 { self.protocol }    // ledger protocol of the chain
    fn soroban_settings(&self) -> bool { false }   // true: testnet Soroban settings first (protocol 30 only)
    fn soroban_limits(&self) -> bool { self.soroban_settings() } // core's high Soroban limits, any protocol
    fn min_persistent_ttl(&self) -> Option<u32> { None } // genesis minimum persistent TTL, at least 10
    fn emit_classic_events(&self) -> bool { true }  // EMIT_CLASSIC_EVENTS
    fn backfill_stellar_asset_events(&self) -> bool { true } // BACKFILL_STELLAR_ASSET_EVENTS, needs the above
    fn below_protocol_valid(&self) -> bool { false } // contrast chain below the family's valid range
    fn run(&self, ctx: &mut ChainCtx) -> anyhow::Result<()> { /* steps and captures */ Ok(()) }
}
```

One recipe value is one chain. Its chain id is `<id>.p<protocol>`. Recording the same family at
protocol 30 and 28 means two values in `all()`, as `cap40` does.

### `ChainCtx`

| Method | What it does |
|---|---|
| `account(role) -> SigningKey` | Key derived from the recipe id and the role. Same under both passphrases. Not funded until `fund` |
| `root() -> &SigningKey` | The network root account, funded at genesis |
| `fund(&[roles], stroops)` | One transaction from root creating each role's account. Closes a ledger |
| `classic(source, ops)` | Envelope from `source` with `ops`, base fee per operation, signed by `source`. A v0 envelope below protocol 13, a v1 envelope from then on |
| `tx(source) -> TransactionBuilder` | Builder carrying `source`'s next sequence number, for preconditions, memos, operation sources and fees (`shapekit_core_driver::tx::TransactionBuilder`) |
| `sign(tx, &[keys])` | Signs under this chain's network id, into a v0 envelope below protocol 13 (`tx::to_v0` fails for a muxed source, v2 preconditions or Soroban data). Add signed-payload or hash-x signatures with `shapekit_core_driver::tx::{push_signature, sign_signed_payload, sign_hash_x}` |
| `submit_and_close(env) -> Applied` | Submits, closes the next ledger, fails unless the transaction succeeded |
| `submit_and_close_failing(env) -> Applied` | Same, but fails unless core included it and it failed (a failed transaction in a ledger) |
| `capture(slug, selector, spec)` | Records an entry from the canonical meta of a closed ledger |
| `capture_rejected(slug, env, spec) -> TransactionResult` | Submits an envelope core must refuse at submission and records it as an `invalid-by-core` entry with core's result. No ledger closes |
| `driver() -> &mut ChainDriver` | The chain driver: `upload`, `deploy`, `invoke`, `soroban`, `soroban_with`, `apply`, `apply_together` (several transactions in one ledger), `close_empty`, `close_with_upgrades` (ledger upgrades such as `maxsorobantxsetsize`), `apply_upgrade_set` with `settings_writer` (a config upgrade set on top of the live settings), `node()` for `ledger_entries` queries |
| `network()`, `protocol()`, `network_id()` | The chain's network, protocol and network id |

`Applied` carries the ledger, the transaction hash, core's `TransactionResult` and the meta.
Every submitted transaction closes its own ledger, so a ledger holds exactly one transaction, unless
the recipe submits several through `driver().apply_together` (`cap63` does, for a parallel
transaction set). Config upgrades on top of testnet's settings go through
`driver().apply_upgrade_set`; `cap46.rs` has the helpers the settings recipes share.

### Selectors

| Selector | Entries written | XDR types |
|---|---|---|
| `Selector::transaction(&applied)` | `<slug>-envelope` and `<slug>-meta` | `TransactionEnvelope`; `TransactionResultMetaV1` (v2 ledgers) or `TransactionResultMeta` |
| `Selector::ledger_of(&applied)` or `Selector::Ledger(seq)` | `<slug>` | `LedgerCloseMeta` |
| `Selector::LedgerEntry { ledger, key }` | `<slug>` | `LedgerEntry`, the last created, updated or restored state of `key` in that ledger's meta, in apply order |

Selection always reads the canonical meta (see Normalization below), never core's raw output.

### `Spec`

```rust
Spec::new("what the entry shows, one sentence")
    .fields(&["op.set_options.signer"])   // ids from registry/fields.toml
    .flags(&["max-length-vec"])           // optional, lowercase kebab-case
    .issues(&["https://..."])             // optional, default: the family's linked_issues
```

### Entry ids

`capture("set-options-payload-1", ...)` in the protocol 30 chain of family `cap40.signed-payload-signer`
gives `cap40.signed-payload-signer.p30-set-options-payload-1-envelope` and `...-meta`. Slugs are
lowercase kebab-case and unique within a recipe; the protocol prefix keeps the chains of one
family apart.

### Rules a recipe must follow

- **Every captured value must match a family fragment, per variant.** `gen chains` fails otherwise.
  An envelope whose only distinctive part is a signature (decorated signatures carry no signer key
  in the grammar) matches nothing on its own: capture its ledger instead, whose meta shows the
  signer on the account. `cap40` does this for the transaction authorised by a signed payload.
- **Determinism.** No randomness, no wall-clock values, no iteration over hash maps. Keys come
  from `account(role)`, payloads and salts from hashes of fixed strings (see `cap40::payload`).
  Close times are fixed by the driver: `1767225600 + seq * 86400` (the standalone virtual clock
  runs ahead of wall time, so close times step one day per ledger).
- **Assert what makes the shape.** Check core's answer where the shape depends on it (for
  example the result code of a refused twin, or the signers on an account) and fail with a
  message. A recipe never passes by accident.
- **Negative twins.** Where core refuses a malformed variant, submit it with `capture_rejected`
  and check the returned result code. The entry gets `expected.validity = invalid-by-core` and
  `source.core_rejection { stage: submit, result_code, result_xdr }`. A malformed transaction that
  core includes and fails is a normal `submit_and_close_failing` plus `capture`.
- **Only core-emitted bytes.** Never hand-build an entry. Hand-crafted values belong to
  `shapekit synth` with `synthetic: true`.
- **Soroban.** `soroban_settings() -> true` applies testnet's live CONFIG_SETTING values before
  `run` (protocol 30 only, they were captured there). Use `driver().upload`, `deploy`, `invoke`
  and `soroban_with`. `driver().soroban(...)` simulates with soroban-simulation 30.0.0, which
  supports protocol 30 only, and accepts source-account auth only. Below protocol 29 the
  committed contracts trap (soroban-sdk 29 declares interface protocol 29), so an older-protocol
  Soroban recipe uses the legacy contract builds and host of the next item.
- **Soroban below protocol 30.** Return `soroban_limits() -> true` and `soroban_settings() -> false`:
  the node starts with `TESTING_SOROBAN_HIGH_LIMIT_OVERRIDE` and no settings upgrade. The driver
  simulates with the host of the chain's protocol (`sim_legacy`, protocols 22, 24 and 28), and
  `shapekit_core_driver::contracts::echo_wasm_for(protocol)` gives an echo build that runs there.
- **Archived entries.** `min_persistent_ttl() -> Some(10)` sets
  `TESTING_MINIMUM_PERSISTENT_ENTRY_LIFETIME` (parsed by release builds, floor 10), so persistent
  entries created under the genesis settings archive ten ledgers after creation; close empty
  ledgers until the query server reports them archived. At protocol 30 the testnet settings
  replace the minimum before `run`, so only entries created before that upgrade (the settings
  writer's code and instance) archive quickly. `soroban.muxed-source` does both.
- **Classic events.** Chains run with `EMIT_CLASSIC_EVENTS` and `BACKFILL_STELLAR_ASSET_EVENTS`
  on by default, so protocol 23 and later ledgers carry the CAP-67 events. Below protocol 23 the backfill flag gives
  TransactionMeta v4 with CAP-67 events, which is what a current core replaying old history
  emits; set both hooks to `false` for the meta a pre-23 ledger had in its day (v3, no classic
  events, `EventManager.cpp` and `TransactionMeta.cpp`, v30.0.0). From protocol 23 on the
  backfill flag has no effect. `cap67.unified-events` records both variants at protocol 22.
- **Protocols below 13.** Core 30.0.0 runs fresh networks from protocol 1 up, but before
  protocol 13 it refuses every envelope except v0 with txNOT_SUPPORTED. `classic` and `sign`
  build v0 there (`tx::sign_for_protocol`); a v0 transaction hashes like its v1 form
  (`tx::transaction_v0_hash`, checked against mainnet protocol 12 history in
  `crates/shapekit-core-driver/tests/tx_mainnet_v0_kat.rs`). There are no muxed accounts and
  no preconditions beyond time bounds. Ledgers below protocol 20 are LedgerCloseMeta v0; with
  both event hooks off the meta is TransactionMeta v2, which is what such a ledger held when
  core closed it. `old_protocols.rs` records chains at protocols 1, 9, 11, 12 and 17.
- **Contrast chains.** A chain that shows what a ledger looked like before its family's shape
  existed runs below `protocol_valid.min`. Return `below_protocol_valid() -> true`: the
  generator then requires the protocol to be below the range (and no longer refuses it), and
  flags every entry of the chain `below-protocol-valid`. Such an entry's protocol range
  describes its bytes, not the family's shape: `protocol.valid_min` is the protocol the chain
  ran at and `protocol.valid_max` the last protocol before the family's `protocol_valid.min`
  (an inflation payout recorded at 11 has `valid_min = 11`, `valid_max = 11`, `introduced =
  12`). `introduced` stays the family's, the protocol whose change the entry contrasts with.
  The manifest validator requires `valid_max` on every `below-protocol-valid` entry, below
  `introduced`, and `generated_at` inside `valid_min` to `valid_max` on every entry. Two
  chains of one family at the same protocol need different recipe ids (the chain id is
  `<id>.p<protocol>`) and different slugs (entry ids are `<family>.p<protocol>-<slug>`).

## 3. Generating

```sh
shapekit gen chains --family cap99.my-shape --network both --jobs 4
shapekit manifest build
shapekit manifest validate
shapekit gen verify-repro --family cap99.my-shape --jobs 4
```

`gen chains` runs every selected recipe under every selected passphrase, each chain on its own
core container, `--jobs` at a time. Then, for each chain:

1. The captures of the two networks are paired by slug. Both runs must capture the same slugs
   with the same types, ledgers and core answers.
2. `passphrase_dependent` is set when the testnet and mainnet bytes differ.
3. Every variant must round-trip through `stellar-xdr` 30.0.0 and match a family fragment
   (the matched fragments are printed per entry).
4. The produced entries go through the manifest validator.
5. Only if every chain and check passed, chains and entries are moved into the corpus. Entries
   the chain produced before but no longer produces are removed. Entries of other source classes
   are never touched; an id collision with one is an error.

`--network testnet` or `mainnet` records one passphrase and keeps the other variant from the
committed entry, so passphrase dependence is still decided on both.

`gen verify-repro` regenerates under both passphrases into a temporary directory and compares
with the corpus: per chain the canonical ledger hashes, ledger range and the SEP-54 lake files; per
entry the set of ids, `entry.json` (oracle references excluded) and every `.xdr` and
`.expected.json` file byte for byte. It then runs `shapekit oracle run` over the regenerated
chains and compares each entry's `.oracle.json` files with the committed ones, so an oracle
change whose files were not regenerated is a difference too. Exit status 1 when anything
differs.

## 4. What is written

```
corpus/chains/<chain-id>/<network>/
  chain.json               schema 2: protocol, passphrase, validator, core image, close
                           schedule, ledger range, per-ledger sha256 (canonical and raw),
                           normalization, the lake's batches with their sha256 and the zstd
                           level and library, and the archive members with the sha256 of
                           their decompressed content
  ledgers/                 SEP-54 lake of the canonical LedgerCloseMeta: .config.json and one
                           zstd LedgerCloseMetaBatch per ledger, 64000 per partition (the
                           galexie layout), reproducible byte for byte
  ledgers.xdr              core's raw framed stream, kept as provenance, not reproducible
  archive.tar              the node's history archive, holding at least checkpoint 63;
                           gzip members differ per run, their contents do not
corpus/entries/<family>/<entry-id>/
  entry.json
  testnet.xdr, testnet.expected.json
  mainnet.xdr, mainnet.expected.json
```

Every recipe chain starts the same way: the protocol upgrade in ledger 2, empty ledgers
through ledger 63 (core's first history checkpoint), then the network settings and the
recipe from ledger 64. Indexers that bootstrap from the archive's latest checkpoint, such as
wallet-backend, can then start before the first shape (see `docs/adapter-interface.md`).
A recipe never depends on absolute ledger numbers, only on `last_closed()`.

`gen chains` writes chains in this form directly. `shapekit pack sep54` converts a chain still
in the recorder's own form (`ledgers.canonical.xdr` and an `archive/` directory) and leaves
converted ones alone; `shapekit pack verify` checks every chain against its `chain.json` and
that the lake holds exactly the canonical form of `ledgers.xdr`. `shapekit pack archive-at
--checkpoint N` writes the archive as it stood at one checkpoint, and `shapekit serve --chain
<chain-id> --network <network>` replays a chain over HTTP: the lake, the archive and a
read-only JSON-RPC endpoint shaped like stellar-rpc, with `POST /control/head` to release
ledgers one at a time.

Entry fields filled by the generator: `source.class = local-core`, `source.recipe`,
`source.chain`, `source.ledger_seq` (absent for refused envelopes), `source.tx_index`,
`protocol.generated_at`, `lcm_version` and `tx_meta_version` where they apply, `caps` and the
protocol range from the family (for contrast entries, the range above). `source.tx_hash` is
left out because a transaction hash covers the network id and so differs between the
variants; it is derivable from each variant's envelope.

### Normalization

Release core emits each ledger-entry change list in a per-process order. The recorder writes a
canonical stream in which every change list is stably sorted by ledger key: the keys come in a
fixed order and each key's changes keep the order core emitted them in, and it checks the result
is a permutation of core's output. Within one key core's order is the logical sequence (a
`RESTORED` before the `UPDATED` of the same entry, a `STATE` before its `UPDATED`) and does not
change between runs; only the order between keys comes from core's hash map, so a canonical LCM
is a byte string core itself can emit. Core's own `normalizeMeta` (src/util/MetaUtils.cpp) is
not used: it also sorts by change type, which can put a `RESTORED` after its `UPDATED`, an order
core never emits and one that makes ingest code reading changes pairwise fail. Entries that
carry change lists (`LedgerCloseMeta`, `TransactionResultMeta`, `TransactionResultMetaV1`,
`TransactionMeta`) record `source.normalization = "key-stable"`; the validator requires it on
such local-core entries.

The claim that only the order between keys varies is checked on real output, not only argued
from core's source. `shapekit gen verify-raw-order --other <corpus>` compares core's raw
`ledgers.xdr` of every chain in two independent recordings, ledger by ledger: each change list
must hold the same changes in both, each key's changes must come in the same order, and the
ledgers must be equal once both are canonical. To run it, copy `corpus/` and record the copy
again with `shapekit gen chains --corpus <copy>`. On 2026-11-13, two independent core runs of
every chain (88 recordings, both passphrases, 6940 ledgers, 9346 change lists) gave 0 per-key
order differences over 10968 keys with two or more changes in one list and 0 content
differences; 580 lists differed, all only in the order between keys. 10 of the 88 raw streams
were byte-identical in both runs, all of them chains at protocols 1, 9, 11 and 12. The
same second recording reproduced every local-core entry and every lake byte for byte.

## 5. Timing

Measured on the cap40 family: each chain (ten ledgers, eighteen captures) records in about 4.5 s;
the four chains (protocols 30 and 28, both passphrases) take about 5 s with `--jobs 4`.
