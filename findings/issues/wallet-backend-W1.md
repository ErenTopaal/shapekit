<!-- repo: stellar/wallet-backend -->
# Comment for #677: a ledger that reproduces it, and the documented participant rule it breaks

A concrete case for this issue, checked at `main` [3e22515a](https://github.com/stellar/wallet-backend/commit/3e22515a44979fdc3a558fda3991ca2b48e38cef) (wallet-backend has no tagged release yet). `participantsForMeta` is unchanged there ([participants.go#L66-L83](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/participants.go#L66-L83)).

**The documented rule that does not hold.** The participants table in [`docs/architecture/state-changes.md` line 121](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/docs/architecture/state-changes.md#L121) says that "Accounts whose entry changes in a successful operation's meta" are written to `transactions_accounts`. Because of this bug, that row of the table never applies.

**An account this drops in practice: the sponsor of a claimable balance.** When a sponsored claimable balance is claimed or clawed back, core releases the sponsor's reserve, so the operation meta updates the sponsor's account entry. The sponsor is not named in the operation, and no state change is written for it, so the meta is the only place wallet-backend could learn about it. As a result, the transaction does not appear in the sponsor's `Account.transactions`.

**Ledger.** Ledger 66 of a standalone stellar-core 30.0.0 network (protocol 30, testnet passphrase). Transaction `e21911e104f314d546bf3486b8c67dba96727af3153de08e99d311ae34c0d105` is a `ClaimClaimableBalance` by `GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU` of a balance sponsored by `GCNPGADCOMTNPTM2XR6V7IOCWQ2MLJNGTCGD3U7SVPHHS7PKN6HMDEAG`.

<details>
<summary>LedgerCloseMeta (base64), save as <code>internal/indexer/testdata/claim_sponsored_balance.b64</code></summary>

```
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
```

</details>

Test, saved as `internal/indexer/claimed_balance_sponsor_test.go`:

```go
package indexer

import (
	"context"
	"os"
	"sort"
	"strings"
	"testing"

	"github.com/alitto/pond/v2"
	"github.com/stellar/go-stellar-sdk/ingest"
	"github.com/stellar/go-stellar-sdk/network"
	"github.com/stellar/go-stellar-sdk/xdr"
	"github.com/stretchr/testify/require"
)

func TestClaimedBalanceSponsorIsNotAParticipant(t *testing.T) {
	b64, err := os.ReadFile("testdata/claim_sponsored_balance.b64")
	require.NoError(t, err)
	var lcm xdr.LedgerCloseMeta
	require.NoError(t, xdr.SafeUnmarshalBase64(strings.TrimSpace(string(b64)), &lcm))

	// Accounts whose entry the operation meta changes.
	rd, err := ingest.NewLedgerTransactionReaderFromLedgerCloseMeta(network.TestNetworkPassphrase, lcm)
	require.NoError(t, err)
	tx, err := rd.Read()
	require.NoError(t, err)
	changes, err := tx.GetOperationChanges(0)
	require.NoError(t, err)
	for _, c := range changes {
		if c.Type == xdr.LedgerEntryTypeAccount {
			t.Logf("operation meta changes account %s", c.Post.Data.MustAccount().AccountId.Address())
		}
	}

	// What the indexer writes to transactions_accounts.
	idx, err := NewIndexer(network.TestNetworkPassphrase, pond.NewPool(1), nil, false)
	require.NoError(t, err)
	buf := NewIndexerBuffer()
	_, err = ProcessLedger(context.Background(), network.TestNetworkPassphrase, lcm, idx, buf)
	require.NoError(t, err)
	for toID, accounts := range buf.GetTransactionsParticipants() {
		var list []string
		for a := range accounts {
			list = append(list, a)
		}
		sort.Strings(list)
		t.Logf("transactions_accounts for %d: %v", toID, list)
	}
}
```

`go test ./internal/indexer -run TestClaimedBalanceSponsorIsNotAParticipant -count=1 -v` at 3e22515a (go1.27.4):

```
=== RUN   TestClaimedBalanceSponsorIsNotAParticipant
    claimed_balance_sponsor_test.go:32: operation meta changes account GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU
    claimed_balance_sponsor_test.go:32: operation meta changes account GCNPGADCOMTNPTM2XR6V7IOCWQ2MLJNGTCGD3U7SVPHHS7PKN6HMDEAG
    claimed_balance_sponsor_test.go:48: transactions_accounts for 283467845632: [GAPTSOXOPLGTIL32WZTIP6O4RULS23ILXXG5Y3IPEULYMPVCFPO4OMPU]
--- PASS: TestClaimedBalanceSponsorIsNotAParticipant (0.00s)
```

The operation meta changes the sponsor's account, but `transactions_accounts` holds only the claimant.

The same happens when the issuer claws back a sponsored claimable balance: ledger 69 of a second network, transaction `45260cb10422d6d18fcf808e50f095d4007baaea2296b7b828dab5c9521b662e` (`ClawbackClaimableBalance`). The operation meta updates the sponsor `GACU5O4PFDMUNMQC6FWBU355EEHYU7J4VYDJE7HEXZ5VXMP4X2KPAEMW`, and only the issuer is a participant. Both ledgers are in the [shapekit](https://github.com/ErenTopaal/shapekit) corpus as Galexie files, if they are useful as regression fixtures for the fix: [claim, ledger 66](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/chains/cap23.claimable-balances.p30/testnet/ledgers/FFFFFFFF--0-63999/FFFFFFBD--66.xdr.zst) and [clawback, ledger 69](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/chains/cap35.clawback.p30/testnet/ledgers/FFFFFFFF--0-63999/FFFFFFBA--69.xdr.zst).
