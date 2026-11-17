# Coverage twin

The coverage twin measures which code of stellar-core a recorded chain runs.
It answers the question the corpus alone cannot: which branches of core's
transaction and ledger code have a witness, and which have none.

Core's C++ code and its Rust contract host are measured separately, because
core's `--enable-codecoverage` build option instruments only C and C++.

## Building the twin

```sh
TMPDIR=<a directory on disk> twin/build.sh [--jobs N] [--memory SIZE] [--cpus N] [--workdir DIR]
```

The script builds the stellar-core commit pinned in `pins.toml` (`[[git]]
stellar-core`) the way core's own `docker/Dockerfile.testing` does, on Ubuntu
24.04 with clang 20 and libc++, plus `--enable-codecoverage`, and packages the
binary as the local image `shapekit/core-twin:<core version>-cov`.

- The source and its submodules are fetched at the pinned commit into the work
  directory (default `$TMPDIR/twin-build`). It holds the build tree, about
  8 GB, so it must be on disk; a path under `/tmp` is refused.
- The compile runs in a container with memory and cpu limits (defaults 6 GB
  and 4 cpus, `make -j3`). A rerun continues where the last one stopped.
- Rust is the version core pins in `rust-toolchain.toml`, installed with
  core's `install-rust.sh`.
- Core's release profile builds its Rust library, which links all ten contract
  hosts, with fat LTO, one codegen unit and full debug info. That one compiler
  invocation does not fit in 6 GB, so the twin builds it without LTO, in 16
  codegen units and without debug info. This changes how the Rust code is
  optimized, not what it computes; `shapekit twin verify` checks that the twin
  writes the same ledgers as the release image.
- Core's test suite is compiled in (configure's default), so the twin also runs
  `stellar-core test`. The release image is built without it.
- The version string is the full commit, so `docker run --rm <image> version`
  prints it on its first line. The script checks this.

The script ends with a table of wall time per phase, the compile container's
peak memory (cgroup `memory.peak`), the work directory's size and the image
size, also written to `<work dir>/measurements.txt`.

The image is local: it has no registry digest. `twin/pins.toml` pins it by
image ID. The build is not bit-reproducible: a rebuild of the same inputs gives
a new image ID, which has to be written into the pin and checked again with
`shapekit twin verify`. The script refuses to reuse an existing tag, so a
rebuild never untags the pinned image; give it a new `--tag`.

## Contract host coverage

`twin/host-cov.sh` measures the contract host, `soroban-env-host`, for one
recorded chain:

```sh
twin/host-cov.sh <chain dir> <out dir>
```

`<chain dir>` is a directory a recording writes (`chain.json` and
`ledgers.xdr`), for example one of `corpus/chains/<chain>/testnet` or the output
of `shapekit gen smoke`. `<out dir>` must not exist yet.

The script builds `shapekit-host-cov` with `-C instrument-coverage` and runs
it on the chain. The program rebuilds the ledger state from the chain's meta
and calls the host's `e2e_invoke::invoke_host_function` entry point for every
contract invocation, with the inputs stellar-core v30.0.0 passes: the
footprint entries and their TTLs, the network settings from the ledger, the
protocol and base reserve in force during apply, the PRNG seed core derives
from the transaction set, and a module cache holding every contract already in
the ledger.

A measurement describes the chain only if the host does in this process what
it did inside core. Each invocation therefore gets a verdict:

| verdict | meaning |
|---|---|
| verified | Same return value, ledger writes, contract events and success hash as core recorded, or the failure core recorded (`TRAPPED`, or `RESOURCE_LIMIT_EXCEEDED` from the instruction or memory limit) |
| differs | The host's outcome contradicts the chain. The script fails and reports no coverage |
| unverified | Core failed the operation with `RESOURCE_LIMIT_EXCEEDED`, which it also checks before calling the host, while the host call succeeds. The run is counted |
| not run by core | Core never called the host (`ENTRY_ARCHIVED`, `MALFORMED`, or the transaction failed before the operation), so the replay does not call it either |
| skipped | The replay cannot rebuild the inputs exactly: protocols other than 30, or footprints that restore archived entries |

The out directory receives `replay.txt` (one line per invocation),
`host.profdata` (the merged profile) and `host-coverage.json` (region and line
counts per host source file). The script prints the host's totals.

The replay links `soroban-env-host` 30.0.0 from crates.io. Its source is
identical to the protocol 30 host core v30.0.0 builds (the `src/rust/soroban/p30`
submodule, commit `6e6c4a87`), apart from test data.

## Protocol gates

`shapekit twin gates` lists every protocol version comparison in core's
non-test C++ source: each call of `protocolVersionIsBefore`,
`protocolVersionStartsFrom` and `protocolVersionEquals`
(`src/util/ProtocolVersion.h`), with its file, line, column, enclosing function
and the protocol it compares against.

```sh
git clone https://github.com/stellar/stellar-core <dir>
git -C <dir> checkout <commit pinned for stellar-core in pins.toml>
shapekit twin gates --core-src <dir> --out registry/generated/gates.json
```

The command refuses a checkout at any other commit. It reads `src/` without
the test directories, the XDR definitions (`protocol-curr`), the vendored
contract hosts (`rust/soroban`) and the load generator (`simulation`). Every
gate is written with both sides open; the sides become met or unmeetable as
chains witness them.

For stellar-core v30.0.0 the command prints:

- 354 matches of the three function names in 560 files. Six are the
  declarations and definitions of the functions themselves.
- 348 calls. 345 compare against a protocol constant and go into the list.
  Three compare against a runtime value (`needUpgradeToVersion` twice,
  `CommandHandler::ensureProtocolVersion`) and are printed instead, because a
  gate needs a protocol to compare against.
- 186 of the listed gates are in `src/transactions`. The constants range from
  `V_2` to `V_30`.

The scanner is a tokenizer, not a compiler. It strips comments, literals and
preprocessor lines and follows braces to find the enclosing function. It stops
with an error when it cannot read a function name or an argument list, rather
than guessing.

## Byte equality with the release image

```sh
shapekit twin verify --out <dir>
```

Coverage measured on the twin describes the release build only if both write
the same ledgers. The command records the same chains under both passphrases
on the release image (`pins.toml`) and on the twin (`twin/pins.toml`) and
compares the SHA-256 of their canonical streams. For corpus chains it also
compares the twin's hash with the one committed in
`corpus/chains/<chain>/<network>/chain.json`, and for the smoke chain it checks
the release image's testnet hash against the value measured before the twin
existed. It exits 1 on any difference.

The default chains are the smoke chain, `cap40.signed-payload-signer.p30`,
`cap73.sac-trust.p30`, `classic.failed-tx.change-trust-native.p9` and
`cap67.unified-events.p22`: protocols 9 to 30, classic and contract
transactions, failed transactions and the unified events backfill. Pass
`--chain` (repeatable) for others.

## Core coverage of a chain

```sh
shapekit twin coverage --chain <smoke or corpus chain id> --out <dir> \
    [--show src/transactions/SetOptionsOpFrame.cpp:308-315]
```

The command records the chain under both passphrases on the twin, with every
container writing its LLVM profile into `<dir>/profiles`. Core writes the
profile when it exits, and the driver stops each node with SIGTERM, which core
handles by shutting down normally. For corpus chains the twin's canonical
hashes must equal the committed ones, otherwise the command stops.

The profiles are merged with `llvm-profdata` and exported with `llvm-cov
export` inside the twin image. The measured code is what decides ledger
contents: `src/transactions` and `src/ledger`, without their tests. Regions
are the code regions of the coverage mapping; a region compiled into several
functions (templates, inline functions) counts as reached when any copy ran.

`<dir>` receives `core-coverage.json` (reached and unreached region counts per
file and every unreached line range, in the core coverage contract),
`counts.json` (every region with its count) and `profiles/merged.profdata`.
`--show` prints the regions overlapping the given lines with their counts.

## Regions core's tests reach and no chain does

```sh
shapekit twin core-tests --spec '[tx]' --jobs 3 --out <tests dir>
shapekit twin r-list --chain-dir <coverage dir> [--chain-dir ...] --tests <tests dir> \
    --out corpus/coverage/core-coverage.json
```

`twin core-tests` runs each of core's test cases matching the Catch2 spec on
the twin, one container of one cpu per test case, at every protocol version
(`--all-versions --rng-seed 12345`, the arguments core's own meta dumps use),
and keeps each test case's region counts in `<tests dir>/tests/<n>/counts.json`,
listed with exit code and duration in `<tests dir>/tests.tsv`. It exits 1 if
any test case does not exit 0.

`twin r-list` adds up the counts of `twin coverage` output directories (all
from the same twin image), and writes the core coverage summary over all of
them. Each line range no chain reaches is `reachable-by-core-tests`, with the
names of the test cases that reach it, or `unknown` when no test case does.
The first kind is a list of regions known to be reachable, with a core test
showing how; a recipe for each is a search target. The second kind may or may
not be reachable at all.

Region counts of the same chain can differ by a region or two between runs,
because some of core's code runs on timers and background threads.

## Why the unknown regions are unreached

```sh
shapekit twin classify --core-src <checkout at the summary's commit>
```

reads `corpus/coverage/core-coverage.json`, `registry/generated/gates.json` and
`twin/unknown-labels.toml` and writes `corpus/coverage/unknown-regions.json`:
every `unknown` range with one cause and how the cause was decided.

| cause | meaning |
|---|---|
| defensive | a throw, an assertion or an abort: fires only when core's own state is inconsistent |
| diagnostics | logging, metrics, tracing or debug output |
| config-or-test | code compiled only with `BUILD_TESTS`, or run only under configuration or a node mode the chains never use (postgres, catchup, utility commands) |
| protocol-outside-chains | the body of an `if` on a single protocol gate whose side no chain's protocol is on |
| ordinary | none of the above: the target list for building new chains |

Static rules decide first, in this order: `build-tests-only`,
`defensive-statement`, `diagnostic-statements`, `diagnostic-function`,
`test-function`, `mode-function`, `config-condition`, `gate-branch`. Ranges no
rule decides take a hand label from `twin/unknown-labels.toml` (by file, by
function or by range, each with its reason), and the rest are `ordinary` with
`decided_by: residual`, meaning nobody has shown they are anything else. A
label that matches no remaining range is an error.

## Gate sides witnessed

```sh
shapekit twin gate-witnesses --chain-dir <twin coverage dir> [--chain-dir ...] \
    --core-src <checkout at the gate list's commit> [--explore <explore-gate-sides.json>]
```

clang's coverage mapping keeps a branch region for every condition, with how
often it was true and how often false. A protocol gate tested as a condition
therefore has its own true and false counts at the gate call's position. The
command reads them from `twin coverage` output (`branches.json`) of corpus
chains and rewrites the sides in `registry/generated/gates.json`. A side a
chain takes is `met`, with `chain:<id>` as witness. A negated gate
(`!protocolVersion...`) has its region on the `!`, so its counts swap.

`--explore` adds the recipe reruns of `shapekit explore gates`. Those runs
replay a recipe at other protocols and see that the gate call ran during the
recipe's own steps at a protocol that takes the side; they infer the outcome
from the protocol rather than read it. Their witnesses are written
`recipe:<IR path>@p<protocol>` and join the chains'. For a gate with no branch
region they are the only evidence. A side met before is never set back to
open. When a rerun at a recipe's own protocol claims a side that the same
recipe's committed chain evaluated and never took, the command prints a
conflict instead of choosing one.

Every met side's `evidence` says which kinds of witness it has. Every open
side says why it is open:

- the gate's result is stored, not tested, so coverage records no side;
- it needs a protocol no chain runs;
- no chain evaluates the condition;
- chains evaluate it but never with this outcome.

llvm-cov writes `i64::MAX` for branch counts it cannot compute. Such branches
are left out; none of them is at a gate in core v30.0.0.

## The probe on the twin

`shapekit probe all`, `probe limits` and `probe verify-repro` take
`--profile-dir <dir>` with `--pins twin/pins.toml`. Every node the probe starts
then writes its coverage profiles into `<dir>/<chain name>`, for example
`<dir>/probe.envelope-fields.p30`, so each probe chain's coverage can be read
apart from the others. With a registry image the flag is refused, because a
release build writes no profiles. The tables and the candidate manifest record
the stellar-core source commit and stack from `--source-pins` (default
`pins.toml`), since the twin's pins file names only the image.

```sh
shapekit probe verify-repro --pins twin/pins.toml --profile-dir <dir>
shapekit twin profiles --dir <dir>/probe.envelope-fields.p30 \
    --label chain:probe.envelope-fields.p30 --protocol 30 --out <coverage dir>
```

`probe verify-repro` names the `chain.json` fields that differ. On the twin
they are `core_image` and `archive` (the history archive records the core
version string), while the canonical hashes and the ledger lake agree.
`twin profiles` turns one chain's profiles into the outputs of `twin coverage`,
ready for `twin gate-witnesses`.
