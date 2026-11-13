<!-- repo: stellar/wallet-backend -->
# ContractDeployProcessor walks the declared auth invocation tree with no depth limit

## What happens

`(*ContractDeployProcessor).ProcessOperation` (`internal/indexer/processors/contract_deploy.go`,
at 3e22515a44979fdc3a558fda3991ca2b48e38cef, line 116) defines a closure `walkInvocation` that
recurses into `SubInvocations` of every `SorobanAuthorizedInvocation` of every auth entry of an
`InvokeHostFunction` operation. Nothing counts the depth, so the recursion goes as deep as the
decoded tree.

The auth tree is submitter-controlled data that core does not interpret before apply. Today the
walk is bounded from outside: go-stellar-sdk's decoder applies its default depth limit (1500),
and from protocol 28 core refuses transactions nested deeper than 1000. Neither bound belongs to
the indexer, and the walk would follow any change to them. A fixed cap in the walk (the guide's
rule: carry a depth counter and stop at a fixed limit, or walk iteratively with an explicit
stack) keeps the cost of one operation independent of the decoder's settings.

Severity: robustness (hardening). It does not halt ingestion at the current limits.

## How it was found

`indexerlint` rule IDX002 (missing depth or size cap), run on the current pin:

```sh
git clone https://github.com/stellar/wallet-backend && cd wallet-backend
git checkout 3e22515a44979fdc3a558fda3991ca2b48e38cef
indexerlint -format json ./...
```

```json
{
  "rule": "IDX002",
  "package": "github.com/stellar/wallet-backend/internal/indexer/processors",
  "function": "(*ContractDeployProcessor).ProcessOperation",
  "file": "internal/indexer/processors/contract_deploy.go",
  "line": 116,
  "column": 2,
  "message": "walkInvocation (closure in (*ContractDeployProcessor).ProcessOperation) recurses over xdr.SorobanAuthorizedInvocation with no depth bound; carry a depth counter and stop at a fixed limit"
}
```

This is the only finding of the three rules (IDX001, IDX002, IDX003) at this commit.

## Corpus entries that exercise the path

`soroban.auth-trees.p30-declared-depth-245`, `-246` and `-492` are protocol 30 ledgers whose
transactions declare auth trees 245, 246 and 492 invocations below the root (492 is the
deepest envelope core's depth check accepts). In the replay harness run at this commit all
three pass on both networks: the walk finishes because the decoder already bounded the tree,
which is the outside bound described above, not one the processor sets.

## Related

- #746 (open) concerns the same walk: rows emitted from declared rather than executed
  deployments.
