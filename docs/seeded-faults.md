# Seeded faults

`shapekit conformance seed-check` measures how much of an SDK's behaviour the corpus actually
watches. It plants faults in the SDK with that language's mutation tool, runs the corpus against
each faulty build, and counts the faults the corpus notices.

```sh
shapekit conformance seed-check --sdk js              # 100 sampled faults, seed 20261116
shapekit conformance seed-check --sdk go --sample 100
shapekit conformance seed-check --sdk rust --sample 40
```

| SDK | Tool | Code the faults go into |
|---|---|---|
| rust | cargo-mutants | the published `stellar-xdr` crate with every feature on, without its command line (`src/bin`, `src/cli`) |
| go | go-mutesting | the `go-stellar-sdk` module, without the two generated xdr files |
| js | Stryker | the compiled files of the installed `@stellar/stellar-sdk` package |

The tool lists every fault it can make in that code. A seeded sample of the list is run
(`--sample`, `--seed`), so a rerun with the same settings runs the same faults.

## What counts as caught

A fault is caught when the corpus run on the faulty build differs from the run on the
unmodified build. Any of these counts:

- an entry's checks change result;
- an entry's answer changes;
- the run hangs;
- the testee no longer starts.

Known failures are compared like every other result. A fault that changes an answer the corpus
already marks as wrong is caught too. A fault whose effect no entry observes is missed.

A fault that does not compile is not counted either way.

## What the catch rate means

The catch rate is the share of built faults that the corpus run notices. It is a lower bound on
what the corpus checks, not a score of the SDK. A missed fault means one of two things:

- no corpus entry reaches the changed code, for example a type name used only in error
  messages;
- an entry reaches the code, but nothing it checks depends on the change.

Each missed fault is listed by file, line and mutation in
[conformance/seed-check/SEED-CHECK.md](../conformance/seed-check/SEED-CHECK.md). Each sampled
fault is also a `seed` obligation in the obligation ledger
(`conformance/seed-check/<sdk>.obligations.json`):

- a caught fault is met, with up to five of the entries whose result it changed as witnesses
  and the number of changed entries in its evidence (a fault that stops the run changes every
  entry);
- a missed fault stays open.

So each SDK has as many met `seed` obligations as caught faults.

## Measured

| SDK | Version | Tool | Faults listed | Sampled | Caught | Missed | Did not build | Caught of those built |
|---|---|---|---|---|---|---|---|---|
| rust | 30.0.0 | cargo-mutants 27.1.0 | 11,776 | 40 | 7 | 31 | 2 | 7 of 38 (18.4%) |
| go | v0.9.0 | go-mutesting v2.3.1 | 1,924 | 100 | 12 | 64 | 24 | 12 of 76 (15.8%) |
| js | 17.4.0 | Stryker 10.1.1 | 16,456 | 100 | 35 | 65 | 0 | 35 of 100 (35.0%) |

No sampled fault timed out.

The corpus also catches the three released js-stellar-sdk faults it has reproductions for:

- [#1785](https://github.com/stellar/js-stellar-sdk/issues/1785), non-canonical ids;
- [#1789](https://github.com/stellar/js-stellar-sdk/issues/1789), colliding map keys;
- [#1766](https://github.com/stellar/js-stellar-sdk/pull/1766), issuer order and map key order.
