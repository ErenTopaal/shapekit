<!-- repo: stellar/wallet-backend -->
# Comment for #805: the same removal happens for a successful fee bump

Core also removes a pre-auth signer in `txChangesBefore` when the transaction succeeds: a fee bump's pre-auth signer for its own hash is removed from the fee source. A fix scoped to failed transactions would not cover this case.

`FeeBumpTransactionFrame::apply` calls `removeOneTimeSignerKeyFromFeeSource` in its own `LedgerTxn` and pushes the result into `txChangesBefore`. It does this before the inner transaction runs, whatever the inner transaction's outcome ([stellar-core v30.0.0, FeeBumpTransactionFrame.cpp lines 167-198](https://github.com/stellar/stellar-core/blob/v30.0.0/src/transactions/FeeBumpTransactionFrame.cpp#L167-L198), and [lines 111-123](https://github.com/stellar/stellar-core/blob/v30.0.0/src/transactions/FeeBumpTransactionFrame.cpp#L111-L123) for the parallel Soroban path). The function removes a pre-auth signer for the fee bump's own hash from the fee source. On the wallet-backend side the effects processor only diffs the operation's own changes ([effects_horizon.go lines 177-196](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/effects_horizon.go#L177-L196)), so the removal is lost even though `Successful()` is true.

**Ledger.** Ledger 85 of a standalone stellar-core 30.0.0 network (protocol 30, testnet passphrase). Transaction `6411ddc2788dc797d4775332e3199098e2a3a2213e5566ab78a160a839bf8afd` is a fee bump whose fee source `GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX` is also the inner transaction's source. That account holds a pre-auth signer for the fee-bump hash, and the inner payment succeeds (`txFEE_BUMP_INNER_SUCCESS`). `txChangesBefore` holds two groups for the account: the signer removal, then the inner transaction's sequence bump.

<details>
<summary>LedgerCloseMeta (base64), save as <code>internal/indexer/testdata/fee_bump_pre_auth_signer.b64</code></summary>

```
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
```

</details>

```go
package indexer

import (
	"context"
	"os"
	"strings"
	"testing"

	"github.com/alitto/pond/v2"
	"github.com/stellar/go-stellar-sdk/network"
	"github.com/stellar/go-stellar-sdk/strkey"
	"github.com/stellar/go-stellar-sdk/xdr"
	"github.com/stretchr/testify/require"
)

func TestFeeBumpPreAuthSignerRemoval(t *testing.T) {
	b64, err := os.ReadFile("testdata/fee_bump_pre_auth_signer.b64")
	require.NoError(t, err)
	var lcm xdr.LedgerCloseMeta
	require.NoError(t, xdr.SafeUnmarshalBase64(strings.TrimSpace(string(b64)), &lcm))

	t.Logf("result: %s", lcm.TransactionResultPair(0).Result.Result.Code)
	for _, c := range lcm.TxApplyProcessing(0).MustV4().TxChangesBefore {
		if e, ok := c.GetState(); ok && e.Data.Type == xdr.LedgerEntryTypeAccount {
			acc := e.Data.MustAccount()
			for _, s := range acc.Signers {
				t.Logf("txChangesBefore STATE %s signer %s weight %d", acc.AccountId.Address(), s.Key.Address(), s.Weight)
			}
		}
		if e, ok := c.GetUpdated(); ok && e.Data.Type == xdr.LedgerEntryTypeAccount {
			acc := e.Data.MustAccount()
			t.Logf("txChangesBefore UPDATED %s: %d signers, seq %d", acc.AccountId.Address(), len(acc.Signers), acc.SeqNum)
		}
	}
	feeBumpHash, err := network.HashTransactionInEnvelope(lcm.TransactionEnvelopes()[0], network.TestNetworkPassphrase)
	require.NoError(t, err)
	t.Logf("pre-auth key of the fee-bump hash: %s", strkey.MustEncode(strkey.VersionByteHashTx, feeBumpHash[:]))

	idx, err := NewIndexer(network.TestNetworkPassphrase, pond.NewPool(1), nil, false)
	require.NoError(t, err)
	buf := NewIndexerBuffer()
	_, err = ProcessLedger(context.Background(), network.TestNetworkPassphrase, lcm, idx, buf)
	require.NoError(t, err)
	for _, sc := range buf.GetStateChanges() {
		t.Logf("state change %s %s account=%s", sc.StateChangeCategory, sc.StateChangeReason, sc.AccountID)
	}
}
```

`go test ./internal/indexer -run TestFeeBumpPreAuthSignerRemoval -count=1 -v` at `main` [3e22515a](https://github.com/stellar/wallet-backend/commit/3e22515a44979fdc3a558fda3991ca2b48e38cef) (go1.27.4):

```
=== RUN   TestFeeBumpPreAuthSignerRemoval
    fee_bump_pre_auth_signer_test.go:22: result: TransactionResultCodeTxFeeBumpInnerSuccess
    fee_bump_pre_auth_signer_test.go:27: txChangesBefore STATE GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX signer TBSBDXOCPCG4PF6UO5JTFYYZSCMOFI5CEE7FKZVLPCQWBKBZX6FP3CEA weight 1
    fee_bump_pre_auth_signer_test.go:32: txChangesBefore UPDATED GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX: 0 signers, seq 274877906949
    fee_bump_pre_auth_signer_test.go:32: txChangesBefore UPDATED GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX: 0 signers, seq 274877906950
    fee_bump_pre_auth_signer_test.go:37: pre-auth key of the fee-bump hash: TBSBDXOCPCG4PF6UO5JTFYYZSCMOFI5CEE7FKZVLPCQWBKBZX6FP3CEA
    fee_bump_pre_auth_signer_test.go:45: state change BALANCE DEBIT account=GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX
    fee_bump_pre_auth_signer_test.go:45: state change BALANCE DEBIT account=GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX
    fee_bump_pre_auth_signer_test.go:45: state change BALANCE CREDIT account=GAE24PMV3GAMPVGKAIEY6ZFAW5BSVQ4ML4R4FGU4DVAKOHHO2C6YVYOK
--- PASS: TestFeeBumpPreAuthSignerRemoval (0.00s)
```

The signer `TBSBDXOC...` is the fee bump's own pre-auth key, and the first `txChangesBefore` update leaves the account with no signers. There is no `SIGNER` `REMOVE` row. Expected: `SIGNER` `REMOVE` for `GBFARHLQ...`, signer `TBSBDXOC...`, old weight 1.

One detail for the fix. Because the account appears in two `txChangesBefore` groups here, comparing each `STATE`/`UPDATED` pair on its own works. Comparing the first `STATE` with the last `UPDATED` also works. Comparing only the last pair misses the removal.

The ledger is in the [shapekit](https://github.com/ErenTopaal/shapekit) corpus ([Galexie file](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/chains/cap15.fee-bump.p30/testnet/ledgers/FFFFFFFF--0-63999/FFFFFFAA--85.xdr.zst)), with a mainnet-passphrase variant that gives the same result.
