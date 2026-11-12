<!-- repo: stellar/wallet-backend -->
# Contract deployments without a matching auth entry (asset contracts, contract-to-contract deploys) record no contract creation

## What happens

At e967ce8b7362b79c277e700e789181d76fbb5967, [`internal/indexer/processors/contract_deploy.go` line 36](https://github.com/stellar/wallet-backend/blob/e967ce8b7362b79c277e700e789181d76fbb5967/internal/indexer/processors/contract_deploy.go#L36): `ContractDeployProcessor.ProcessOperation` records a contract creation for the operation's own `CreateContract` only when its preimage is `FromAddress` (`emitTopLevel` skips `FromAsset`, the Stellar Asset Contract), and for nested deploys only when the deploy is declared in an auth entry (`walkInvocation`). A SAC deployment, or a contract deployed by another contract without an auth entry for it, creates a contract instance in the meta and gets no `ACCOUNT`/`CREATE` row.

Severity: data-loss. CAPs: CAP-0046, CAP-0046-02, CAP-0046-05, CAP-0046-06, CAP-0046-07, CAP-0046-08, CAP-0046-12, CAP-0054, CAP-0058, CAP-0073, CAP-0085.

## Minimal entry

Corpus entry `cap73.sac-trust.p28-deploy-xlm-sac-envelope` of the shapekit corpus: chain `cap73.sac-trust.p28`, ledger 66, recorded on a standalone stellar-core 30.0.0 under the testnet passphrase (the mainnet variant gives the same result). Transaction(s): `654410f98298b201d559998674893b50f1d3417e7a5bc2025825d7f3ff8c7041`.

## Reproduce

```sh
(cd adapters && go build -o bin/wb-adapter ./walletbackend/cmd/wb-adapter)
shapekit harness run --chain cap73.sac-trust.p28 --network testnet --adapter "adapters/bin/wb-adapter --commit e967ce8b7362b79c277e700e789181d76fbb5967"
```

The harness replays the chain through a replay stellar-rpc and history archive into wallet-backend's live ingestion (TimescaleDB, `migrate up`, `ingest`), reads back `transactions`, `transactions_accounts` and `state_changes`, and compares them with what the ledger meta shows. Output: `harness-out/HARNESS.md`.

## Expected (ledger meta) and stored

From the harness report of the run above (`harness-report.json`, entry `cap73.sac-trust.p28-deploy-xlm-sac-envelope`, testnet):

- dropped `contract-deployed`: contract-deployed is missing
  - ledger: `{"contract": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"}`

Verdict for the entry: dropped: 1 of 3 in-scope fact(s) missing.

## Suggested fix

Read deployments from the contract instance entries the operation's meta creates, for every preimage type and whether or not an auth entry declares them; #746 asks for the same correlation in the other direction.

## Other entries with the same cause

24 problem(s) over 12 entries (mainnet, testnet):

- `cap73.sac-trust.p28-deploy-credit-sac-envelope`
- `cap73.sac-trust.p28-deploy-credit-sac-meta`
- `cap73.sac-trust.p28-deploy-xlm-sac-meta`
- `cap73.sac-trust.p30-deploy-credit-sac-envelope`
- `cap73.sac-trust.p30-deploy-credit-sac-meta`
- `cap73.sac-trust.p30-deploy-xlm-sac-envelope`
- `cap73.sac-trust.p30-deploy-xlm-sac-meta`
- `cap85.executable-tag.p30-contract-deploys-external-ref-envelope`
- `cap85.executable-tag.p30-contract-deploys-external-ref-meta`
- `soroban.invoke-basics.p30-contract-deploys-with-constructor-envelope`
- `soroban.invoke-basics.p30-contract-deploys-with-constructor-meta`

## Related

- https://github.com/stellar/wallet-backend/issues/746
