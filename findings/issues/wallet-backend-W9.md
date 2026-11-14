<!-- repo: stellar/wallet-backend -->
# Question: CreateAccount writes a SIGNER ADD row for the new account's master key; intended for state changes?

Checked at `main` [3e22515a](https://github.com/stellar/wallet-backend/commit/3e22515a44979fdc3a558fda3991ca2b48e38cef) (no tagged release yet).

## What happens

[`addAccountCreatedEffects`](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/effects_horizon.go#L122-L133) emits a `signer_created` effect for the new account's own key with weight `keypair.DefaultSignerWeight`. The effects processor turns it into a `SIGNER` `ADD` state change on the new account, signer = the account's own key, weight null to 1.

This looks deliberate. The file says it was ported from Horizon's effects processing ([line 3](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/effects_horizon.go#L3)), and Horizon emits the same effect ([stellar-horizon `effects_processor.go`, `addAccountCreatedEffects`](https://github.com/stellar/stellar-horizon/blob/01ba44a988b3ebc2d68d3cb6747fa2c2733773cd/internal/ingest/processors/effects_processor.go#L460-L490)). It is also consistent with set options handling here. `addSetOptionsEffects` compares `SignerSummary()` before and after, and `SignerSummary()` includes the master key while its weight is above 0 ([go-stellar-sdk v0.8.0, `xdr/account_entry.go` lines 3-14](https://github.com/stellar/go-stellar-sdk/blob/v0.8.0/xdr/account_entry.go#L3-L14)). So a later change of master weight becomes a `SIGNER` `UPDATE` or `REMOVE` for the same key.

The question is about what a consumer of state changes sees. In the ledger, the new account has an empty `signers` list, and the master key's weight is byte 0 of its `thresholds`. The `SIGNER` `ADD` row for the master key has the same shape as a row for a signer added with set options. The [state changes table](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/docs/architecture/state-changes.md#L73) describes `SIGNER` `ADD` as "A signer is added", with no mention of the master key. A consumer that rebuilds an account's signer list from these rows, or counts signer additions, has to know the Horizon convention. To tell the two apart it has to compare the signer key with the account. Nothing in the row marks it as the master key.

## Reproduction

Ledger 65 of a standalone stellar-core 30.0.0 network (protocol 30, testnet passphrase), transaction `deaf08ab7a2f5b5baee797b4567f4eb2edc56d2397c31576d40ffa9a7b68dfed`. `GCADADAUU5HWGTXYQ4WPC5SH7A6AI5ZKY7ZLWZCSCXQJCO6FX5W4FA5A` creates `GD7H63KWIVUWQD73AP53NQMZBJYEHNYXCSPL22QNQE37MVS7PVQA3NE6` between `BeginSponsoringFutureReserves` and `EndSponsoringFutureReserves`. The sponsorship does not matter here: the row comes from the `CreateAccount` operation alone.

<details>
<summary>LedgerCloseMeta (base64), save as <code>internal/indexer/testdata/create_account.b64</code></summary>

```
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
```

</details>

Test, saved as `internal/indexer/master_key_signer_row_test.go`:

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

func TestCreateAccountMasterKeySignerRow(t *testing.T) {
	b64, err := os.ReadFile("testdata/create_account.b64")
	require.NoError(t, err)
	var lcm xdr.LedgerCloseMeta
	require.NoError(t, xdr.SafeUnmarshalBase64(strings.TrimSpace(string(b64)), &lcm))

	for _, opMeta := range lcm.TxApplyProcessing(0).MustV4().Operations {
		for _, c := range opMeta.Changes {
			if e, ok := c.GetCreated(); ok && e.Data.Type == xdr.LedgerEntryTypeAccount {
				acc := e.Data.MustAccount()
				t.Logf("meta creates %s: signers=%d thresholds=%v", acc.AccountId.Address(), len(acc.Signers), acc.Thresholds)
			}
		}
	}

	idx, err := NewIndexer(network.TestNetworkPassphrase, pond.NewPool(1), nil, false)
	require.NoError(t, err)
	buf := NewIndexerBuffer()
	_, err = ProcessLedger(context.Background(), network.TestNetworkPassphrase, lcm, idx, buf)
	require.NoError(t, err)
	for _, sc := range buf.GetStateChanges() {
		if sc.StateChangeCategory == "SIGNER" {
			t.Logf("state change %s %s account=%s signer=%s weight %v -> %v", sc.StateChangeCategory, sc.StateChangeReason,
				sc.AccountID, sc.SignerAccountID.SignerKeyBytea, sc.SignerWeightOld, sc.SignerWeightNew)
		}
	}
}
```

`go test ./internal/indexer -run TestCreateAccountMasterKeySignerRow -count=1 -v` at 3e22515a (go1.27.4):

```
=== RUN   TestCreateAccountMasterKeySignerRow
    master_key_signer_row_test.go:25: meta creates GD7H63KWIVUWQD73AP53NQMZBJYEHNYXCSPL22QNQE37MVS7PVQA3NE6: signers=0 thresholds=[1 0 0 0]
    master_key_signer_row_test.go:37: state change SIGNER ADD account=GD7H63KWIVUWQD73AP53NQMZBJYEHNYXCSPL22QNQE37MVS7PVQA3NE6 signer=GD7H63KWIVUWQD73AP53NQMZBJYEHNYXCSPL22QNQE37MVS7PVQA3NE6 weight {0 false} -> {1 true}
--- PASS: TestCreateAccountMasterKeySignerRow (0.00s)
```

## Questions

1. Is the master key meant to appear as a signer in state changes, as it does in Horizon's effects?
2. If so, would a marker on these rows or a note in the state changes docs be welcome, so consumers do not read them as entries of the account's signer list? If not, the master key's weight could be left to the account's thresholds, with no `SIGNER` row for it.

Found with the [shapekit](https://github.com/ErenTopaal/shapekit) replay harness, which compares what wallet-backend stores with the ledger meta. From a checkout of https://github.com/ErenTopaal/shapekit at tag v0.1.0 (Docker required; the adapter builds wallet-backend at the given commit and runs it against TimescaleDB):

```sh
cargo build --release --locked -p shapekit-cli
(cd adapters && go build -o bin/wb-adapter ./walletbackend/cmd/wb-adapter)
target/release/shapekit harness run --chain cap33.sponsorship.p30 --network testnet \
  --adapter "adapters/bin/wb-adapter --commit 3e22515a44979fdc3a558fda3991ca2b48e38cef"
```
