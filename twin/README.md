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
image ID, and a rebuild gives a new ID to write there.

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
