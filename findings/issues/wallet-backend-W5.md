<!-- repo: stellar/wallet-backend -->
# Contract deployments that no auth entry declares get no ACCOUNT CREATE state change (factory deploys, Stellar Asset Contracts)

Checked at `main` [3e22515a](https://github.com/stellar/wallet-backend/commit/3e22515a44979fdc3a558fda3991ca2b48e38cef) (no tagged release yet).

## What happens

[state-changes.md line 71](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/docs/architecture/state-changes.md#L71) documents an `ACCOUNT` `CREATE` row for "A contract deployment (row on the contract, creator is the deployer)". `ContractDeployProcessor.ProcessOperation` finds deployments in two places only:

- the operation's own host function, when it is `CreateContract` or `CreateContractV2` with a `FromAddress` preimage ([contract_deploy.go lines 89-101](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/contract_deploy.go#L89-L101)); `emitTopLevel` returns without a row for a `FromAsset` preimage, "a SAC, which has no creator account" ([lines 158-163](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/contract_deploy.go#L158-L163));
- `CreateContract` invocations declared in the operation's auth entries ([lines 103-153](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/contract_deploy.go#L103-L153)). With no auth entries the function returns at [lines 103-104](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/contract_deploy.go#L103-L104).

Two kinds of deployment that the meta shows are in neither place:

1. **A contract that deploys another contract from its own address.** The operation is `InvokeContract`, and the transaction carries no auth entry for the deploy (in the reproduction below it has no auth entries at all). The meta creates the new contract instance; no row is written.
2. **A Stellar Asset Contract deployment.** The preimage is `FromAsset`, so `emitTopLevel` skips it. The deployment has a submitter (the operation's source account), and state-changes.md names no exception for asset contracts. A credit asset's SAC still gets a `contract_tokens` row. The native asset's SAC does not, because `sac.AssetFromContractData` returns false for the native asset contract ([go-stellar-sdk v0.8.0, ingest/sac/contract_data.go lines 90-94](https://github.com/stellar/go-stellar-sdk/blob/v0.8.0/ingest/sac/contract_data.go#L90-L94)), so neither the state changes nor `contract_tokens` record that deployment.

Severity: data-loss (a created contract with no state change). CAPs: CAP-0046-02, CAP-0046-06, CAP-0058.

## Reproduction

Three ledgers, each recorded on a standalone stellar-core 30.0.0 network under the testnet passphrase:

- protocol 30, ledger 87, transaction `0210e58878e55ec3236596aee412c0fedfb630e30abf5107ab1f593f02650ab8`: an `InvokeContract` with no auth entries in which a factory contract deploys a new instance from its own address and runs its constructor;
- protocol 28, ledger 65, transaction `1dfb5aea83ff199f1a0d60ddf779d31f54c3d658072ae008ebe53852816119b9`: the issuer deploys the SAC of its credit asset `TRST`;
- protocol 28, ledger 66, transaction `654410f98298b201d559998674893b50f1d3417e7a5bc2025825d7f3ff8c7041`: an account deploys the SAC of the native asset.

<details>
<summary>Ledger 87 (base64), save as <code>internal/indexer/testdata/factory_deploy_with_constructor.b64</code></summary>

```
AAAAAgAAAAA+ri9ygfO/kwj5lsInJmDdVeoyUVAnIaLE12WrQ3QdngAAAB44hAXINlIWfvj2NBsbmtHQrdO0JzxgQahEdKJyk+bDVuqNWBoSJmdob/ZPMU2Nmix3IJxoCuwm48KnWE6TEWIyAAAAAGnIa4AAAAAAAAAAAwAAAZ024+wAAAAAAHlj6rNCwXno1Bn4pGsg0Y2JLSmmAn8t/NLujlmhig5xAAAAQHOa/vOZhxcYJ+DVRehaUOJwrZPSiyytOTS/TxZvyw/q85L5P2xEXX+CoAgfMrpXHWofdnxQHgxeCpoKqDtbUwFbvBTuIu7DlmAycioKuk1QAGn++xi0OBgKwBvbn1FQ4sFaqnDUI1jlTCEJB4xMz4HXOUL52NrA5qewMaJzU4TNAAAAVw3gtrOnZAAAAAAAAAArF0YAAAAAAAAAAAAAAAAAAABkBfXhAAAAAGRtze1B/fBLzWBJAVuPQx91yxPS6+2IfhF7hWC2JqIMowAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAATiEBcg2UhZ++PY0Gxua0dCt07QnPGBBqER0onKT5sNWAAAAAgAAAAAAAAAAAAAAAQAAAAEAAAAAAAAAZAAAAAEAAAABAAAAAQAAAAIAAAAA88kSfe3YpjWHzIRKFCUcJI9LHjimz/SIyu80eV+RGLkAAG/KAAAARAAAAAIAAAAAAAAAAAAAAAEAAAAAAAAAGAAAAAAAAAAB2LhVhUWZ3pf61ckLAvyhYD3M2SuFk8rvzu6Z8143tuAAAAAQZGVwbG95X3dpdGhfYXJncwAAAAMAAAANAAAAINfHSHtFNVMhSgn8ieUrDmnORQjmi0+AtJlUbFXdlbZdAAAADQAAACBdtf8zB8IaoTUsEI2CFO0JdJWmK90rojmtLe8sStvgzAAAABAAAAABAAAAAQAAABIAAAAAAAAAAPPJEn3t2KY1h8yEShQlHCSPSx44ps/0iMrvNHlfkRi5AAAAAAAAAAEAAAAAAAAAAgAAAAYAAAAB2LhVhUWZ3pf61ckLAvyhYD3M2SuFk8rvzu6Z8143tuAAAAAUAAAAAQAAAAfXx0h7RTVTIUoJ/InlKw5pzkUI5otPgLSZVGxV3ZW2XQAAAAEAAAAGAAAAAZUKpwVZbtTgzijCVoRYA3Q/w+UC/Lymsc4BO175+E51AAAAFAAAAAEADbiCAAAAAAAAAKgAAAAAAABvZgAAAAFfkRi5AAAAQEvssdkbLb+xXs8krrKr4RuZrR300xNwvdFlBHzjHSqeFzRAVpdG41GQNnaTzMBwW4e3HRSxz3osdwzSfcB9uQYAAAABAAAAAAIQ5Yh45V7DI2WWruQSwP7ftjDjCr9RB6sfWT8CZQq4AAAAAAAAUoQAAAAAAAAAAQAAAAAAAAAYAAAAADUzCMiwC+OxUK7SrasW7e04yqH859lKdoPlBydjSxkwAAAAAAAAAAIAAAADAAAAUwAAAAAAAAAA88kSfe3YpjWHzIRKFCUcJI9LHjimz/SIyu80eV+RGLkAAAAXSHauegAAAEQAAAABAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABTAAAAAGnDJYAAAAAAAAAAAQAAAFcAAAAAAAAAAPPJEn3t2KY1h8yEShQlHCSPSx44ps/0iMrvNHlfkRi5AAAAF0h2PrAAAABEAAAAAQAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAAAAAAUwAAAABpwyWAAAAAAAAAAAQAAAAAAAAAAgAAAAMAAABXAAAAAAAAAADzyRJ97dimNYfMhEoUJRwkj0seOKbP9IjK7zR5X5EYuQAAABdIdj6wAAAARAAAAAEAAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAAAAAAAAAAAAAAAAADAAAAAAAAAFMAAAAAacMlgAAAAAAAAAABAAAAVwAAAAAAAAAA88kSfe3YpjWHzIRKFCUcJI9LHjimz/SIyu80eV+RGLkAAAAXSHY+sAAAAEQAAAACAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABXAAAAAGnIa4AAAAAAAAAAAQAAAAAAAAACAAAAAAAAAFcAAAAGAAAAAAAAAAGVCqcFWW7U4M4owlaEWAN0P8PlAvy8prHOATte+fhOdQAAABQAAAABAAAAEwAAAADXx0h7RTVTIUoJ/InlKw5pzkUI5otPgLSZVGxV3ZW2XQAAAAEAAAABAAAADwAAAAVvd25lcgAAAAAAABIAAAAAAAAAAPPJEn3t2KY1h8yEShQlHCSPSx44ps/0iMrvNHlfkRi5AAAAAAAAAAAAAABXAAAACYUAl8fMLuTtZs0YS5vLi4jx0EfgEnylvhcYnc9uQyXMAAHY1gAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAQAAABIAAAABlQqnBVlu1ODOKMJWhFgDdD/D5QL8vKaxzgE7Xvn4TnUAAAACAAAAAAAAAAAAAAAB15KLcsJwPM/q9+uf9O9NUEpVqLl5/JtFDqLIQrTRzmEAAAABAAAAAAAAAAIAAAAPAAAAA2ZlZQAAAAASAAAAAAAAAADzyRJ97dimNYfMhEoUJRwkj0seOKbP9IjK7zR5X5EYuQAAAAoAAAAAAAAAAAAAAAAAAG/KAAAAAgAAAAAAAAAB15KLcsJwPM/q9+uf9O9NUEpVqLl5/JtFDqLIQrTRzmEAAAABAAAAAAAAAAIAAAAPAAAAA2ZlZQAAAAASAAAAAAAAAADzyRJ97dimNYfMhEoUJRwkj0seOKbP9IjK7zR5X5EYuQAAAAr//////////////////+K6AAAAAAAAAAIAAAADAAAAVwAAAAAAAAAA88kSfe3YpjWHzIRKFCUcJI9LHjimz/SIyu80eV+RGLkAAAAXSHY+sAAAAEQAAAACAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAAABXAAAAAGnIa4AAAAAAAAAAAQAAAFcAAAAAAAAAAPPJEn3t2KY1h8yEShQlHCSPSx44ps/0iMrvNHlfkRi5AAAAF0h2W/YAAABEAAAAAgAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAAAAAAVwAAAABpyGuAAAAAAAAAAAAAAAAAAAAAAAAAndcAAAAA
```

</details>

<details>
<summary>Ledger 65 (base64), save as <code>internal/indexer/testdata/deploy_credit_sac.b64</code></summary>

```
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
```

</details>

<details>
<summary>Ledger 66 (base64), save as <code>internal/indexer/testdata/deploy_xlm_sac.b64</code></summary>

```
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
```

</details>

Test, saved as `internal/indexer/deploy_without_auth_entry_test.go`:

```go
package indexer

import (
	"context"
	"os"
	"strings"
	"testing"

	"github.com/alitto/pond/v2"
	"github.com/stellar/go-stellar-sdk/network"
	"github.com/stellar/go-stellar-sdk/xdr"
	"github.com/stretchr/testify/require"
)

func TestDeploysWithoutCreateRow(t *testing.T) {
	for _, name := range []string{"factory_deploy_with_constructor", "deploy_credit_sac", "deploy_xlm_sac"} {
		b64, err := os.ReadFile("testdata/" + name + ".b64")
		require.NoError(t, err)
		var lcm xdr.LedgerCloseMeta
		require.NoError(t, xdr.SafeUnmarshalBase64(strings.TrimSpace(string(b64)), &lcm))

		for _, tx := range lcm.TransactionEnvelopes() {
			for _, op := range tx.Operations() {
				ih := op.Body.MustInvokeHostFunctionOp()
				t.Logf("%s: host function %s, %d auth entries", name, ih.HostFunction.Type, len(ih.Auth))
			}
		}
		for i := 0; i < lcm.CountTransactions(); i++ {
			for _, opMeta := range lcm.TxApplyProcessing(i).MustV4().Operations {
				for _, c := range opMeta.Changes {
					if e, ok := c.GetCreated(); ok && e.Data.Type == xdr.LedgerEntryTypeContractData &&
						e.Data.MustContractData().Key.Type == xdr.ScValTypeScvLedgerKeyContractInstance {
						id, err := e.Data.MustContractData().Contract.String()
						require.NoError(t, err)
						t.Logf("%s: meta creates contract instance %s", name, id)
					}
				}
			}
		}

		idx, err := NewIndexer(network.TestNetworkPassphrase, pond.NewPool(1), nil, false)
		require.NoError(t, err)
		buf := NewIndexerBuffer()
		_, err = ProcessLedger(context.Background(), network.TestNetworkPassphrase, lcm, idx, buf)
		require.NoError(t, err)
		for _, sc := range buf.GetStateChanges() {
			t.Logf("%s: state change %s %s account=%s", name, sc.StateChangeCategory, sc.StateChangeReason, sc.AccountID)
		}
		for id := range buf.GetSACContracts() {
			t.Logf("%s: contract_tokens row %s", name, id)
		}
	}
}
```

`go test ./internal/indexer -run TestDeploysWithoutCreateRow -count=1 -v` at 3e22515a (go1.27.4):

```
=== RUN   TestDeploysWithoutCreateRow
    deploy_without_auth_entry_test.go:25: factory_deploy_with_constructor: host function HostFunctionTypeHostFunctionTypeInvokeContract, 0 auth entries
    deploy_without_auth_entry_test.go:35: factory_deploy_with_constructor: meta creates contract instance CCKQVJYFLFXNJYGOFDBFNBCYAN2D7Q7FAL6LZJVRZYATWXXZ7BHHK7XO
    deploy_without_auth_entry_test.go:47: factory_deploy_with_constructor: state change BALANCE DEBIT account=GDZ4SET55XMKMNMHZSCEUFBFDQSI6SY6HCTM75EIZLXTI6K7SEMLST4Q
    deploy_without_auth_entry_test.go:25: deploy_credit_sac: host function HostFunctionTypeHostFunctionTypeCreateContract, 0 auth entries
    deploy_without_auth_entry_test.go:35: deploy_credit_sac: meta creates contract instance CAZWR77LFFGRPUDQHUILEJGRQAACE6IGP6GIFMMAG465USHLJHCJI5T7
    deploy_without_auth_entry_test.go:47: deploy_credit_sac: state change BALANCE DEBIT account=GBRHNLJWCOTZPE45Z33CUBA335WGS25NNAUH5VVBFBGNTKVDA32LLZA5
    deploy_without_auth_entry_test.go:50: deploy_credit_sac: contract_tokens row CAZWR77LFFGRPUDQHUILEJGRQAACE6IGP6GIFMMAG465USHLJHCJI5T7
    deploy_without_auth_entry_test.go:25: deploy_xlm_sac: host function HostFunctionTypeHostFunctionTypeCreateContract, 0 auth entries
    deploy_without_auth_entry_test.go:35: deploy_xlm_sac: meta creates contract instance CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC
    deploy_without_auth_entry_test.go:47: deploy_xlm_sac: state change BALANCE DEBIT account=GDKMA2CZT4MP2PTP7GFMTUGDJEE7QBFRNJAYDKEYSEKJ7G4VO6OT5CZQ
--- PASS: TestDeploysWithoutCreateRow (0.00s)
```

Each meta creates a contract instance, and each ledger's only state change is the fee `BALANCE` `DEBIT`.

## Expected

An `ACCOUNT` `CREATE` row for every contract instance the operation meta creates, as state-changes.md describes: for the factory deploy, a row on `CCKQVJYFLFXNJYGOFDBFNBCYAN2D7Q7FAL6LZJVRZYATWXXZ7BHHK7XO` with the factory contract as creator. Reading deployments from the contract instance entries the meta creates covers both cases and every call depth; the auth tree or the preimage then only supplies the deployer address. For a SAC, whose preimage names no deployer address, either a row with no creator (or with the operation's source as creator) or a sentence in state-changes.md that asset contract deployments get no row tells consumers what to expect.

## Also found with shapekit

Found with the [shapekit](https://github.com/ErenTopaal/shapekit) replay harness, which compares what wallet-backend stores with the ledger meta. From a checkout of https://github.com/ErenTopaal/shapekit at tag v0.1.0 (Docker required; the adapter builds wallet-backend at the given commit and runs it against TimescaleDB):

```sh
cargo build --release --locked -p shapekit-cli
(cd adapters && go build -o bin/wb-adapter ./walletbackend/cmd/wb-adapter)
target/release/shapekit harness run --chain soroban.invoke-basics.p30 --chain cap73.sac-trust.p28 --network testnet \
  --adapter "adapters/bin/wb-adapter --commit 3e22515a44979fdc3a558fda3991ca2b48e38cef"
```

The corpus entries are [`soroban.invoke-basics.p30-contract-deploys-with-constructor-meta`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.invoke-basics/soroban.invoke-basics.p30-contract-deploys-with-constructor-meta/entry.json), [`cap73.sac-trust.p28-deploy-credit-sac-meta`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap73.sac-trust/cap73.sac-trust.p28-deploy-credit-sac-meta/entry.json) and [`cap73.sac-trust.p28-deploy-xlm-sac-meta`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap73.sac-trust/cap73.sac-trust.p28-deploy-xlm-sac-meta/entry.json). The harness reports the same missing row for 12 entries, on both the mainnet-passphrase and testnet-passphrase variants, among them the protocol 30 SAC deployments (`cap73.sac-trust.p30`) and a contract that deploys an instance of its own executable (`cap85.executable-tag.p30`).
