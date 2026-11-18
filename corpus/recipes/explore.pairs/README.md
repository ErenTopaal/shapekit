# Recipe pairs

Each pair is two recipes that start from the same chain state and differ in one transaction.
That transaction names the same thing in two ways. Core leaves the ledger entries the pair is
about byte for byte the same under both, so an indexer has to store the same state for both.
The route equivalence relation of the indexer harness checks that it does.

Every pair directory here holds:

- `a.json` and `b.json`, the two recipes in recipe IR (`schema/recipe-ir.schema.json`);
- `pair.json`, the entries the pair compares, with each entry's encoded size and whether core
  wrote it identically under both recipes (`records_equal`).

The chains of both recipes, recorded under both network passphrases, are the chain
directories `pairs.<pair>.a` and `pairs.<pair>.b` under `corpus/chains/`, listed in
`corpus/recipes/chains.toml`.

| Pair | Recipe `a` | Recipe `b` | Entries compared | Result |
|---|---|---|---|---|
| `muxed-payment` | a payment to the payee's account | the same payment to the payee's account muxed with id 1234 | every entry the payment ledger changed: the payer's account (144 bytes) and the payee's account (92 bytes) | equal |
| `sac-trust-p28` | the holder calls `trust(holder)` on the Stellar Asset Contract of the asset (CAP-73), protocol 28 | the holder adds the trustline with ChangeTrust and limit `i64::MAX`, protocol 28 | the holder's trustline (116 bytes) | equal |
| `sac-trust-p30` | the same call, protocol 30 | the same ChangeTrust, protocol 30 | the holder's trustline (116 bytes) | equal |

`pair.json` compares the testnet chains. Both passphrases are recorded, so you can replay
either network to an indexer.

## Making pairs

`shapekit explore pairs --recipes <dir> --out <dir>` rewrites the transactions of the recipes
in a directory. A destination or source account becomes muxed, or an operation gets an
explicit source. It keeps each rewrite whose changed entries core writes byte for byte the
same. The `muxed-payment` pair is the muxed destination rewrite of a single payment.

No rewrite turns a contract call into a classic operation, so the trust pairs are written by
hand. You record a pair with

```sh
shapekit explore pair --a a.json --b b.json --out <dir>              # every changed entry
shapekit explore pair --a a.json --b b.json --only trustline --out <dir>
```

and put a new pair in the candidate lane first: move `<dir>/a` and `<dir>/b` to
`candidate/chains/pairs.<pair>.a` and `candidate/chains/pairs.<pair>.b`, the recipes and
`pair.json` to `candidate/recipes/explore.pairs/<pair>/`, and list both chains in
`candidate/recipes/chains.toml`. `shapekit corpus promote` moves them here with their entries.
