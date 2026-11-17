# Seeded faults

`shapekit conformance seed-check` measures how much of an SDK's behaviour the corpus actually
watches. It plants faults in the SDK with that language's mutation tool, runs the corpus against
each faulty build, and counts the faults the corpus notices.

```sh
shapekit conformance seed-check --sdk js              # 100 sampled faults, seed 20261116
shapekit conformance seed-check --sdk go --sample 100
shapekit conformance seed-check --sdk rust --sample 40
```

With `--candidate candidate/manifest.json --candidate-flag format-vector`, every probe also
runs the candidate lane's format vectors (3,869 entries). A fault only they catch counts as
caught by the lane, apart from the corpus.

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

Every fault the corpus alone misses carries a class, in
`conformance/seed-check/classes/<sdk>.json`, with the reason for it:

- `equivalent`: no input can tell it from the unmodified SDK;
- `outside-the-ops`: the code is on no path a testee op takes;
- `corpus-gap`: a testee op reaches it, but no corpus entry does;
- `other`: none of the above.

Each missed fault is listed by file, line, mutation and class in
[conformance/seed-check/SEED-CHECK.md](../conformance/seed-check/SEED-CHECK.md). Each sampled
fault is also a `seed` obligation in the obligation ledger
(`conformance/seed-check/<sdk>.obligations.json`):

- a caught fault is met, with up to five of the corpus or candidate entries whose result it
  changed as witnesses and the number of changed entries in its evidence (a fault that stops
  the run changes every entry);
- a missed fault stays open.

So each SDK has as many met `seed` obligations as faults the corpus and the lane catch.

## Measured

| SDK | Version | Tool | Faults listed | Sampled | Did not build | Caught by the corpus alone | Caught with the candidate lane |
|---|---|---|---|---|---|---|---|
| rust | 30.0.0 | cargo-mutants 27.1.0 | 11,776 | 40 | 2 | 7 of 38 (18.4%) | 12 of 38 (31.6%) |
| go | v0.9.0 | go-mutesting v2.3.1 | 1,924 | 100 | 24 | 14 of 76 (18.4%) | 14 of 76 (18.4%) |
| js | 17.4.0 | Stryker 10.1.1 | 16,456 | 100 | 0 | 36 of 100 (36.0%) | 46 of 100 (46.0%) |

No sampled fault timed out. The faults the corpus alone misses, by class:

| SDK | equivalent | outside-the-ops | corpus-gap | other | of which the candidate lane catches |
|---|---|---|---|---|---|
| rust | 1 | 25 | 5 | 0 | 5 |
| go | 7 | 54 | 0 | 1 | 0 |
| js | 5 | 40 | 12 | 7 | 10 |

Most missed faults are in code no testee op calls (`outside-the-ops`), such as type names
that only feed `Display`. The `corpus-gap` faults are the ones new entries can catch: 17 across
the three SDKs, 15 of which the format vectors in the candidate lane already catch.

The corpus also catches the three released js-stellar-sdk faults it has reproductions for:

- [#1785](https://github.com/stellar/js-stellar-sdk/issues/1785), non-canonical ids;
- [#1789](https://github.com/stellar/js-stellar-sdk/issues/1789), colliding map keys;
- [#1766](https://github.com/stellar/js-stellar-sdk/pull/1766), issuer order and map key order.
