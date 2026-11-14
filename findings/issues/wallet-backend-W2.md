<!-- repo: stellar/wallet-backend -->
# Question: BALANCE rows for claimable balance and liquidity pool transfers do not record the balance or pool

Checked at `main` [3e22515a](https://github.com/stellar/wallet-backend/commit/3e22515a44979fdc3a558fda3991ca2b48e38cef) (no tagged release yet).

## What wallet-backend writes

For a transfer that has a claimable balance (`B...`) or a liquidity pool (`L...`) at one end, [`handleTransfer`](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/token_transfer.go#L258-L304) writes a row for the account end only:

- `CreateClaimableBalance` and `LiquidityPoolDeposit`: one `BALANCE` `DEBIT` on the sender ([lines 265-268](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/token_transfer.go#L265-L268)).
- `ClaimClaimableBalance` and `LiquidityPoolWithdraw`: one `BALANCE` `CREDIT` on the receiver ([lines 270-273](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/token_transfer.go#L270-L273)).
- A path payment through a pool: the account side only ([`handleTransfersWithLiquidityPool`](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/token_transfer.go#L306-L322)).
- `SetTrustLineFlags` and `AllowTrust`, whose revocation can redeem pool shares into claimable balances: no row ([lines 275-277](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/token_transfer.go#L275-L277)).

The code comment above `handleTransfer` and the [state changes table](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/docs/architecture/state-changes.md#L67-L68) describe the single row, so writing one row is intended. My question is about the other end of the transfer. The ledger names it: since protocol 23, CAP-67 emits `transfer` events whose `to` (create, deposit) or `from` (claim, withdraw) is the `B...` or `L...` address ([CAP-67, "Create Claimable Balance" and "Claim Claimable Balance"](https://github.com/stellar/stellar-protocol/blob/ced008c3727005ca4a775f430fe6fe346c30f58f/core/cap-0067.md#L380-L411)). The stored row keeps neither address. A `BALANCE` row has no claimable balance column, and `liquidity_pool_id` is set only on `TRUSTLINE` and `BALANCE_AUTHORIZATION` rows ([types.go lines 884-893](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/types/types.go#L884-L893)).

Where the cost shows most is `CreateClaimableBalance`. Core assigns the new balance's ID while it applies the operation. The ID is returned in the operation result, which wallet-backend does not keep: `operations` stores `operation_xdr` and `result_code` ([migration lines 20-21](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/db/migrations/2025-06-10.3-operations.sql#L20-L21)). So the database has no way to link the creator's `DEBIT` to the `CREDIT` written when the same balance is later claimed. For a deposit or withdrawal, the pool ID is still in `operation_xdr`. For a path payment through a pool, it is not.

## Reproduction

Two consecutive ledgers of a standalone stellar-core 30.0.0 network (protocol 30, testnet passphrase):

- ledger 83, transaction `88f64099f2161f9bcc78ae68cc8cbf29292ab543f38170d5fee04da0db21aa45`: `GBJL4ZBUVYPA2UGZXRN62YIL4QGTM4YUWBAOXDPKDVOHBE6ZGI35KVB3` creates a claimable balance of 20 `UNIF` for `GCOCPZVHUDKGHQSQUY2T6LEAGDDQUL5J6GG2HISAWEUVJNKKYGYK7E5P`. Its CAP-67 event is `transfer` from `GBJL4Z...` to `BAAEKXQNX55AA7WL5EAVRBQFYC4GPP44XOHBP27YTI7GMFJXPXP6U6WVWU`;
- ledger 84, transaction `eb794a34b20cd779848f00206033c0ee190c5c2aafd710d9681399e43bd611fb`: `GCOCPZ...` claims it. Its event is `transfer` from `BAAEKXQN...` to `GCOCPZ...`.

<details>
<summary>Ledger 83 (base64), save as <code>internal/indexer/testdata/create_claimable_balance.b64</code></summary>

```
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
```

</details>

<details>
<summary>Ledger 84 (base64), save as <code>internal/indexer/testdata/claim_claimable_balance.b64</code></summary>

```
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
```

</details>

Test, saved as `internal/indexer/claimable_balance_rows_test.go`:

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

func TestClaimableBalanceRowsCarryNoBalanceID(t *testing.T) {
	for _, name := range []string{"create_claimable_balance", "claim_claimable_balance"} {
		b64, err := os.ReadFile("testdata/" + name + ".b64")
		require.NoError(t, err)
		var lcm xdr.LedgerCloseMeta
		require.NoError(t, xdr.SafeUnmarshalBase64(strings.TrimSpace(string(b64)), &lcm))

		idx, err := NewIndexer(network.TestNetworkPassphrase, pond.NewPool(1), nil, false)
		require.NoError(t, err)
		buf := NewIndexerBuffer()
		_, err = ProcessLedger(context.Background(), network.TestNetworkPassphrase, lcm, idx, buf)
		require.NoError(t, err)
		for _, sc := range buf.GetStateChanges() {
			if sc.OperationID == 0 {
				continue // fee row
			}
			t.Logf("%s: %s %s account=%s amount=%s liquidity_pool_id=%q key_value=%v",
				name, sc.StateChangeCategory, sc.StateChangeReason, sc.AccountID, sc.Amount.String,
				sc.LiquidityPoolID.String, sc.KeyValue)
		}
	}
}
```

`go test ./internal/indexer -run TestClaimableBalanceRowsCarryNoBalanceID -count=1 -v` at 3e22515a (go1.27.4):

```
=== RUN   TestClaimableBalanceRowsCarryNoBalanceID
    claimable_balance_rows_test.go:31: create_claimable_balance: BALANCE DEBIT account=GBJL4ZBUVYPA2UGZXRN62YIL4QGTM4YUWBAOXDPKDVOHBE6ZGI35KVB3 amount=200000000 liquidity_pool_id="" key_value=map[]
    claimable_balance_rows_test.go:31: claim_claimable_balance: BALANCE CREDIT account=GCOCPZVHUDKGHQSQUY2T6LEAGDDQUL5J6GG2HISAWEUVJNKKYGYK7E5P amount=200000000 liquidity_pool_id="" key_value=map[]
--- PASS: TestClaimableBalanceRowsCarryNoBalanceID (0.00s)
```

The `B...` address that both events carry is in neither row, so nothing in the database says that the two rows belong to the same claimable balance.

## Questions

1. Is leaving the claimable balance or pool out of these rows intended, or only a consequence of `BALANCE` rows being keyed by account?
2. If a link is wanted, would a nullable counterparty column on `BALANCE` rows (holding the `B...` or `L...` address from the event) fit the schema? `liquidity_pool_id` already exists for pool IDs.

Affected CAPs: CAP-0023 (claimable balances), CAP-0038 (liquidity pools), CAP-0067 (the events that name both ends).

Found with the [shapekit](https://github.com/ErenTopaal/shapekit) replay harness, which compares what wallet-backend stores with the ledger meta. From a checkout of https://github.com/ErenTopaal/shapekit at tag v0.1.0 (Docker required; the adapter builds wallet-backend at the given commit and runs it against TimescaleDB):

```sh
cargo build --release --locked -p shapekit-cli
(cd adapters && go build -o bin/wb-adapter ./walletbackend/cmd/wb-adapter)
target/release/shapekit harness run --chain cap67.unified-events.p30 --network testnet \
  --adapter "adapters/bin/wb-adapter --commit 3e22515a44979fdc3a558fda3991ca2b48e38cef"
```

The same pattern shows up in the corpus chains `cap23.claimable-balances.p30`, `cap33.sponsorship.p30`, `cap35.clawback.p30` and `cap38.liquidity-pools.p30`, on both the testnet and mainnet passphrases.
