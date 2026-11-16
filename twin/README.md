# Coverage twin

The coverage twin measures which code of stellar-core a recorded chain runs.
It answers the question the corpus alone cannot: which branches of core's
transaction and ledger code have a witness, and which have none.

Core's C++ code and its Rust contract host are measured separately, because
core's `--enable-codecoverage` build option instruments only C and C++.

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
