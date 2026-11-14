<!-- repo: stellar/wallet-backend -->
# ContractDeployProcessor walks the declared auth invocation tree with no depth limit

Checked at `main` [3e22515a](https://github.com/stellar/wallet-backend/commit/3e22515a44979fdc3a558fda3991ca2b48e38cef) (no tagged release yet).

## What happens

`(*ContractDeployProcessor).ProcessOperation` defines a closure, `walkInvocation`, that recurses into `SubInvocations` of the root invocation of every auth entry of an `InvokeHostFunction` operation ([contract_deploy.go lines 115-153](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/contract_deploy.go#L115-L153)). Nothing counts the depth, so the recursion goes as deep as the decoded tree.

The auth tree is submitter input. Today three bounds outside the processor keep the walk short:

- the walk runs only for successful transactions ([line 50](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/contract_deploy.go#L50)), and in the deepest corpus case below the transaction fails;
- from protocol 28, core refuses an envelope nested deeper than 1000 XDR levels ([stellar-core v30.0.0, `validateXDRForProtocol`](https://github.com/stellar/stellar-core/blob/v30.0.0/src/transactions/TransactionUtils.cpp#L1349-L1380));
- the XDR decoder stops at 1500 levels by default ([go-xdr `DecodeDefaultMaxDepth`](https://github.com/stellar/go-xdr/blob/dc590f17552a5063bc59411b7e6b9e928b852435/xdr3/decode.go#L35-L36)).

None of these bounds is set by the indexer, and the walk follows any change to them. A depth counter with a fixed limit, or an iterative walk with an explicit stack, keeps the cost of one operation independent of decoder settings and of how core and the host bound envelopes in later protocols.

Severity: robustness (hardening). It does not halt ingestion at the current limits.

## How it was found

The `IDX002` rule (missing depth or size cap) of `indexerlint`, a static analyzer for Go indexers built on go-stellar-sdk. From a checkout of https://github.com/ErenTopaal/shapekit at tag v0.1.0 ([docs/linter.md](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/linter.md)):

```sh
(cd lint && go build -o indexerlint ./cmd/indexerlint)
git clone https://github.com/stellar/wallet-backend && cd wallet-backend
git checkout 3e22515a44979fdc3a558fda3991ca2b48e38cef
/path/to/shapekit/lint/indexerlint -format json ./...
```

This is the only finding of its three rules at this commit:

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

## Deep trees that reach the walk

The shapekit corpus has ledgers from a standalone stellar-core 30.0.0 network (protocol 30) with deep declared auth trees. In [`soroban.auth-trees.p30-declared-depth-245`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-245/entry.json) (ledger 110, transaction `890b530034908d74c3d5cb6f0275d12ad58cec409dc1dce5c6b212a8c955cec0`), the transaction succeeds with a chain 245 invocations deep below the root, so `walkInvocation` recurses 245 levels. In [`soroban.auth-trees.p30-declared-depth-492`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.p30-declared-depth-492/entry.json) (ledger 112, transaction `e69a59cf52289dcdbabce4d5eef191ba56fcfd1895a6faef0a71d38e1e5aab8f`), core accepts the envelope, but the transaction fails and the walk does not run. Both ledgers ingest without error at 3e22515a.

## Related

- #746 concerns the same walk: rows emitted from declared rather than executed deployments.
