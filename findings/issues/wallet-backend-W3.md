<!-- repo: stellar/wallet-backend -->
# Feature request: a state change reason for clawbacks, which today are written exactly like a payment to the issuer

Checked at `main` [3e22515a](https://github.com/stellar/wallet-backend/commit/3e22515a44979fdc3a558fda3991ca2b48e38cef) (no tagged release yet).

## Current behaviour

[`handleClawback`](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/token_transfer.go#L361-L375) sends a classic `Clawback` through `handleDefaultBurnOrClawback`, the same function that handles a payment back to the issuer. Both produce a `BALANCE` `BURN` on the issuer and a `BALANCE` `DEBIT` on the holder. A `ClawbackClaimableBalance` produces one `BURN` on the issuer, the same row as an issuer claiming its own asset from a claimable balance. This is documented: "Clawbacks have no reason of their own and appear as `BURN`" ([state-changes.md line 92](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/docs/architecture/state-changes.md#L92)).

The ledger does tell them apart. CAP-67 emits a `clawback` event for both clawback operations and a `burn` event for a payment to the issuer ([CAP-67, "Payment"](https://github.com/stellar/stellar-protocol/blob/ced008c3727005ca4a775f430fe6fe346c30f58f/core/cap-0067.md#L308-L325) for `burn`, ["Clawback / Clawback Claimable Balance"](https://github.com/stellar/stellar-protocol/blob/ced008c3727005ca4a775f430fe6fe346c30f58f/core/cap-0067.md#L473-L482) for `clawback`). The SDK's token transfer processor already delivers them as different event types (`*ttp.TokenTransferEvent_Clawback` and `*ttp.TokenTransferEvent_Burn`, [token_transfer.go lines 165-168](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/token_transfer.go#L165-L168)), and the distinction is dropped when the rows are written.

## Why it matters to a consumer

For a holder, the `DEBIT` of a clawback, where the issuer took the funds, looks the same as the `DEBIT` of a payment they sent to the issuer themselves. A wallet that wants to show "clawed back by the issuer" has to load each row's operation and check its type. A query on state change reason cannot do it: `BURN` returns both kinds, and the holder's rows are plain `DEBIT`s.

## Reproduction

Two ledgers of a standalone stellar-core 30.0.0 network (protocol 30, testnet passphrase), same asset (`UNIF` issued by `GC75L4OJYE25NGFKBO7L35X76K3OJCVTONXBC2L4XGBQNF7IGQV6GOZW`, asset contract `CBUMQDKWSQ4RVQSL6JE4PTUWKFRDW4PNMTSKRL3ECH4U7JJQ7POMW5D7`):

- ledger 80, transaction `a4f74302135b443f55451f826e459eec0af32ae65962b6fc4fc0a895229882c2`: the issuer claws back 5 `UNIF` from `GBJL4ZBUVYPA2UGZXRN62YIL4QGTM4YUWBAOXDPKDVOHBE6ZGI35KVB3`, and the meta carries a `clawback` event;
- ledger 73, transaction `4fe753eceb376b0de277a3601e59b3840d760b682551ce3bfb80607045bc9e46`: `GCOCPZVHUDKGHQSQUY2T6LEAGDDQUL5J6GG2HISAWEUVJNKKYGYK7E5P` pays 10 `UNIF` back to the issuer, and the meta carries a `burn` event.

<details>
<summary>Ledger 80 (base64), save as <code>internal/indexer/testdata/clawback.b64</code></summary>

```
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
```

</details>

<details>
<summary>Ledger 73 (base64), save as <code>internal/indexer/testdata/payment_to_issuer.b64</code></summary>

```
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
```

</details>

Test, saved as `internal/indexer/clawback_rows_test.go`:

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

func TestClawbackAndPaymentToIssuerRows(t *testing.T) {
	for _, name := range []string{"clawback", "payment_to_issuer"} {
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
			t.Logf("%s: %s %s account=%s token=%s amount=%s", name, sc.StateChangeCategory,
				sc.StateChangeReason, sc.AccountID, sc.TokenID.AddressBytea, sc.Amount.String)
		}
	}
}
```

`go test ./internal/indexer -run TestClawbackAndPaymentToIssuerRows -count=1 -v` at 3e22515a (go1.27.4):

```
=== RUN   TestClawbackAndPaymentToIssuerRows
    clawback_rows_test.go:31: clawback: BALANCE BURN account=GC75L4OJYE25NGFKBO7L35X76K3OJCVTONXBC2L4XGBQNF7IGQV6GOZW token=CBUMQDKWSQ4RVQSL6JE4PTUWKFRDW4PNMTSKRL3ECH4U7JJQ7POMW5D7 amount=50000000
    clawback_rows_test.go:31: clawback: BALANCE DEBIT account=GBJL4ZBUVYPA2UGZXRN62YIL4QGTM4YUWBAOXDPKDVOHBE6ZGI35KVB3 token=CBUMQDKWSQ4RVQSL6JE4PTUWKFRDW4PNMTSKRL3ECH4U7JJQ7POMW5D7 amount=50000000
    clawback_rows_test.go:31: payment_to_issuer: BALANCE BURN account=GC75L4OJYE25NGFKBO7L35X76K3OJCVTONXBC2L4XGBQNF7IGQV6GOZW token=CBUMQDKWSQ4RVQSL6JE4PTUWKFRDW4PNMTSKRL3ECH4U7JJQ7POMW5D7 amount=100000000
    clawback_rows_test.go:31: payment_to_issuer: BALANCE DEBIT account=GCOCPZVHUDKGHQSQUY2T6LEAGDDQUL5J6GG2HISAWEUVJNKKYGYK7E5P token=CBUMQDKWSQ4RVQSL6JE4PTUWKFRDW4PNMTSKRL3ECH4U7JJQ7POMW5D7 amount=100000000
--- PASS: TestClawbackAndPaymentToIssuerRows (0.00s)
```

Apart from the accounts and amounts, the two operations produce the same rows.

## Possible change

Add a `CLAWBACK` reason (`BALANCE` `CLAWBACK` on the holder, keeping the issuer row), or keep the reasons and record the CAP-67 event name on `BALANCE` rows. The operation type cannot always separate the two. A SAC `clawback` and a SAC `burn` are both `invoke_host_function` calls and take the same `handleDefaultBurnOrClawback` path. The SEP-41 path also writes `BURN` for both a SEP-41 `burn` and a `clawback` event ([state-changes.md line 70](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/docs/architecture/state-changes.md#L70)).

Affected CAPs: CAP-0035 (clawback), CAP-0067 (`clawback` and `burn` events).

Found with the [shapekit](https://github.com/ErenTopaal/shapekit) replay harness, which compares what wallet-backend stores with the ledger meta. From a checkout of https://github.com/ErenTopaal/shapekit at tag v0.1.0 (Docker required; the adapter builds wallet-backend at the given commit and runs it against TimescaleDB):

```sh
cargo build --release --locked -p shapekit-cli
(cd adapters && go build -o bin/wb-adapter ./walletbackend/cmd/wb-adapter)
target/release/shapekit harness run --chain cap67.unified-events.p30 --network testnet \
  --adapter "adapters/bin/wb-adapter --commit 3e22515a44979fdc3a558fda3991ca2b48e38cef"
```

The chains `cap35.clawback.p30` and `cap27.muxed-accounts.p30` show the same rows for `Clawback` and `ClawbackClaimableBalance`.
