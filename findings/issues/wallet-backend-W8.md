<!-- repo: stellar/wallet-backend -->
# A pre-authorized transaction signer that core removes in txChangesBefore never gets a SIGNER REMOVE state change

Checked at `main` [3e22515a](https://github.com/stellar/wallet-backend/commit/3e22515a44979fdc3a558fda3991ca2b48e38cef) (no tagged release yet).

## What happens

Core removes a pre-authorized transaction signer (`T...`, the hash of a transaction) from the source accounts when that transaction is applied, in `txChangesBefore` and outside any operation. It does this in two places in stellar-core v30.0.0:

- `TransactionFrame::processSignatures` calls `removeOneTimeSignerFromAllSourceAccounts` before the operations run, so the signer is removed whether the transaction then succeeds or fails ([TransactionFrame.cpp lines 1532-1563](https://github.com/stellar/stellar-core/blob/v30.0.0/src/transactions/TransactionFrame.cpp#L1532-L1563));
- `FeeBumpTransactionFrame::apply` calls `removeOneTimeSignerKeyFromFeeSource` in its own `LedgerTxn` and pushes the result into `txChangesBefore` before the inner transaction runs, so a fee bump's own hash signer leaves the fee source also when the inner transaction succeeds ([FeeBumpTransactionFrame.cpp lines 167-198](https://github.com/stellar/stellar-core/blob/v30.0.0/src/transactions/FeeBumpTransactionFrame.cpp#L167-L198), and [lines 110-123](https://github.com/stellar/stellar-core/blob/v30.0.0/src/transactions/FeeBumpTransactionFrame.cpp#L110-L123) for the parallel Soroban path).

wallet-backend writes `SIGNER` rows only from operation effects ([effects.go lines 143-146](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/effects.go#L143-L146)). `Effects` returns nothing for a failed transaction ([effects_horizon.go lines 64-67](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/effects_horizon.go#L64-L67)), and `addSetOptionsEffects` ([line 135](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/effects_horizon.go#L135)) compares the signers in the operation's own changes only ([lines 177-196](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/effects_horizon.go#L177-L196)). `txChangesBefore` is read for contract data only ([indexer.go lines 844-876](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/indexer.go#L844-L876)). So neither removal is stored, and the account's history keeps a signer it no longer has.

Severity: data-loss (a removed signer with no state change). CAPs: CAP-0002, CAP-0015, CAP-0028.

## Reproduction

Two ledgers of standalone stellar-core 30.0.0 networks (protocol 30, testnet passphrase):

- ledger 67, transaction `44d93dbceaf43f09b5d08608600d1c6e50d0fbde60659efd7a1ce8f8814dee4e`: a pre-authorized transaction with no signatures from `GDD25X2BOFUU33TVAMOKUHP7UNWFO45G2YIB54KLIAJAJJZTRYXUIBES` fails (`txFAILED`) on a payment to an account that does not exist. Its source held the pre-auth signer `TBCNSPN4...` for this transaction.
- ledger 85, transaction `6411ddc2788dc797d4775332e3199098e2a3a2213e5566ab78a160a839bf8afd`: a fee bump whose fee source `GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX` is also the inner transaction's source and holds a pre-auth signer for the fee bump's hash. The inner payment succeeds (`txFEE_BUMP_INNER_SUCCESS`). `txChangesBefore` holds two groups for the account: the signer removal, then the inner transaction's sequence bump.

<details>
<summary>Ledger 67 (base64), save as <code>internal/indexer/testdata/failed_pre_auth_tx.b64</code></summary>

```
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
```

</details>

<details>
<summary>Ledger 85 (base64), save as <code>internal/indexer/testdata/fee_bump_pre_auth_signer.b64</code></summary>

```
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
```

</details>

Test, saved as `internal/indexer/pre_auth_signer_removal_test.go`:

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

func TestPreAuthSignerRemoval(t *testing.T) {
	for _, name := range []string{"failed_pre_auth_tx", "fee_bump_pre_auth_signer"} {
		b64, err := os.ReadFile("testdata/" + name + ".b64")
		require.NoError(t, err)
		var lcm xdr.LedgerCloseMeta
		require.NoError(t, xdr.SafeUnmarshalBase64(strings.TrimSpace(string(b64)), &lcm))

		hash, err := network.HashTransactionInEnvelope(lcm.TransactionEnvelopes()[0], network.TestNetworkPassphrase)
		require.NoError(t, err)
		t.Logf("%s: result %s, pre-auth key of the envelope hash %s", name,
			lcm.TransactionResultPair(0).Result.Result.Code, strkey.MustEncode(strkey.VersionByteHashTx, hash[:]))
		for _, c := range lcm.TxApplyProcessing(0).MustV4().TxChangesBefore {
			if e, ok := c.GetState(); ok && e.Data.Type == xdr.LedgerEntryTypeAccount {
				acc := e.Data.MustAccount()
				for _, s := range acc.Signers {
					t.Logf("%s: txChangesBefore STATE %s signer %s weight %d", name, acc.AccountId.Address(), s.Key.Address(), s.Weight)
				}
			}
			if e, ok := c.GetUpdated(); ok && e.Data.Type == xdr.LedgerEntryTypeAccount {
				acc := e.Data.MustAccount()
				t.Logf("%s: txChangesBefore UPDATED %s: %d signers, seq %d", name, acc.AccountId.Address(), len(acc.Signers), acc.SeqNum)
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
	}
}
```

`go test ./internal/indexer -run TestPreAuthSignerRemoval -count=1 -v` at 3e22515a (go1.27.4):

```
=== RUN   TestPreAuthSignerRemoval
    pre_auth_signer_removal_test.go:25: failed_pre_auth_tx: result TransactionResultCodeTxFailed, pre-auth key of the envelope hash TBCNSPN45L2D6CNV2CDAQYANDRXFBUH33ZQGLHX5PIOOR6EBJXXE4TJL
    pre_auth_signer_removal_test.go:31: failed_pre_auth_tx: txChangesBefore STATE GDD25X2BOFUU33TVAMOKUHP7UNWFO45G2YIB54KLIAJAJJZTRYXUIBES signer TBCNSPN45L2D6CNV2CDAQYANDRXFBUH33ZQGLHX5PIOOR6EBJXXE4TJL weight 1
    pre_auth_signer_removal_test.go:36: failed_pre_auth_tx: txChangesBefore UPDATED GDD25X2BOFUU33TVAMOKUHP7UNWFO45G2YIB54KLIAJAJJZTRYXUIBES: 0 signers, seq 274877906947
    pre_auth_signer_removal_test.go:46: failed_pre_auth_tx: state change BALANCE DEBIT account=GDD25X2BOFUU33TVAMOKUHP7UNWFO45G2YIB54KLIAJAJJZTRYXUIBES
    pre_auth_signer_removal_test.go:25: fee_bump_pre_auth_signer: result TransactionResultCodeTxFeeBumpInnerSuccess, pre-auth key of the envelope hash TBSBDXOCPCG4PF6UO5JTFYYZSCMOFI5CEE7FKZVLPCQWBKBZX6FP3CEA
    pre_auth_signer_removal_test.go:31: fee_bump_pre_auth_signer: txChangesBefore STATE GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX signer TBSBDXOCPCG4PF6UO5JTFYYZSCMOFI5CEE7FKZVLPCQWBKBZX6FP3CEA weight 1
    pre_auth_signer_removal_test.go:36: fee_bump_pre_auth_signer: txChangesBefore UPDATED GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX: 0 signers, seq 274877906949
    pre_auth_signer_removal_test.go:36: fee_bump_pre_auth_signer: txChangesBefore UPDATED GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX: 0 signers, seq 274877906950
    pre_auth_signer_removal_test.go:46: fee_bump_pre_auth_signer: state change BALANCE DEBIT account=GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX
    pre_auth_signer_removal_test.go:46: fee_bump_pre_auth_signer: state change BALANCE DEBIT account=GBFARHLQRKXBMLQHQ7RTH67BBVFKVXYP555TSUTSVPTCVLUELTH2ULAX
    pre_auth_signer_removal_test.go:46: fee_bump_pre_auth_signer: state change BALANCE CREDIT account=GAE24PMV3GAMPVGKAIEY6ZFAW5BSVQ4ML4R4FGU4DVAKOHHO2C6YVYOK
--- PASS: TestPreAuthSignerRemoval (0.00s)
```

In both ledgers the signer in `txChangesBefore` is the transaction's own pre-auth key, and the update leaves the account with no signers. The state changes hold fees and the payment only; there is no `SIGNER` `REMOVE`.

## Expected

A `SIGNER` `REMOVE` row for `GDD25X2B...` (signer `TBCNSPN4...`, old weight 1) and one for `GBFARHLQ...` (signer `TBSBDXOC...`, old weight 1). Reading signer changes from `txChangesBefore` as well as from operation effects, for failed transactions and fee bumps too, covers both. In the fee bump case the account appears in two `txChangesBefore` groups: comparing each `STATE`/`UPDATED` pair, or the first `STATE` with the last `UPDATED`, finds the removal; comparing only the last pair misses it.

## Also found with shapekit

Found with the [shapekit](https://github.com/ErenTopaal/shapekit) replay harness, which compares what wallet-backend stores with the ledger meta. From a checkout of https://github.com/ErenTopaal/shapekit at tag v0.1.0 (Docker required; the adapter builds wallet-backend at the given commit and runs it against TimescaleDB):

```sh
cargo build --release --locked -p shapekit-cli
(cd adapters && go build -o bin/wb-adapter ./walletbackend/cmd/wb-adapter)
target/release/shapekit harness run --chain classic.failed-tx.p30 --chain cap15.fee-bump.p30 --network testnet \
  --adapter "adapters/bin/wb-adapter --commit 3e22515a44979fdc3a558fda3991ca2b48e38cef"
```

The corpus entries are [`classic.failed-tx.p30-pre-auth-failed-removes-signer`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/classic.failed-tx/classic.failed-tx.p30-pre-auth-failed-removes-signer/entry.json) and [`cap15.fee-bump.p30-fee-source-is-inner-source-pre-auth-signer-meta`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap15.fee-bump/cap15.fee-bump.p30-fee-source-is-inner-source-pre-auth-signer-meta/entry.json). The mainnet-passphrase variants give the same result.
