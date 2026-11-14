<!-- repo: stellar/wallet-backend -->
# A pool-share trustline removed by an authorization revocation gets no TRUSTLINE REMOVE state change

Checked at `main` [3e22515a](https://github.com/stellar/wallet-backend/commit/3e22515a44979fdc3a558fda3991ca2b48e38cef) (no tagged release yet).

## What happens

When an issuer revokes a holder's authorization with `SetTrustLineFlags` or `AllowTrust`, and the holder has pool shares in a pool that contains the issuer's asset, core redeems those shares and deletes the holder's pool-share trustline ([CAP-38, "SetTrustLineFlagsOp and AllowTrustOp"](https://github.com/stellar/stellar-protocol/blob/ced008c3727005ca4a775f430fe6fe346c30f58f/core/cap-0038.md#L886-L925)). The redeemed amounts go into claimable balances for the holder. The trustline removal is in the operation meta.

wallet-backend writes no `TRUSTLINE` `REMOVE` row for it. `TRUSTLINE` `REMOVE` rows come only from `addChangeTrustEffects` ([effects_horizon.go line 245](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/effects_horizon.go#L245)), which runs for `ChangeTrust` only ([lines 80-81](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/effects_horizon.go#L80-L81)). For `AllowTrust` and `SetTrustLineFlags`, [`addAllowTrustEffects`](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/effects_horizon.go#L307) and [`addSetTrustLineFlagsEffects`](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/effects_horizon.go#L383-L387) emit flag effects only.

The pool-share balance table does see the removal: `LiquidityPoolSharesProcessor` records a `REMOVE` for the holder's shares. So the current state is right, but the holder's state change history keeps the pool-share trustline that a `ChangeTrust` added and never shows it leaving. When the holder removes the same trustline with `ChangeTrust`, the history does get a `TRUSTLINE` `REMOVE` with `liquidity_pool_id` set ([effects_horizon.go lines 266-272](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/effects_horizon.go#L266-L272), [effects.go lines 258-265](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/effects.go#L258-L265)).

The redeemed amounts that move into claimable balances get no `BALANCE` rows either. That part is deliberate in [`handleTransfer`](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/token_transfer.go#L275-L277). It is mentioned here only because, together with the missing `REMOVE`, the history shows no sign of the holder's pool position ending.

Severity: data-loss (a ledger change with no state change). CAP: CAP-0038.

## Reproduction

Ledger 74 of a standalone stellar-core 30.0.0 network (protocol 30, testnet passphrase), transaction `17dc53f99a4ca34a64291886b4e2f7b9f8183bd7814412db2930217bf6ec510e`. Issuer `GAYL3ZJQZMNCRXA3HNX6WTDKUVXQDW3AS5HPZL45E4WPNILW2ARM3JBS` clears the authorization of holder `GCTWYC6XEKAC5HBLNUH6NS7VA7CZNZGBU2R7FUSM2TWLNGBR367THITH` on its `USD` with `SetTrustLineFlags`. The holder had shares in pool `79d95b12eb2e5c9bc625aa5e059360ec35a1be8a7a4a7fdb90a04c8e5f814ba4`. Core redeems them into two claimable balances for the holder (one per pool asset) and removes the pool-share trustline.

<details>
<summary>LedgerCloseMeta (base64), save as <code>internal/indexer/testdata/revoke_redeems_pool_shares.b64</code></summary>

```
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
```

</details>

Test, saved as `internal/indexer/revoke_pool_shares_test.go`:

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

func TestRevocationRemovesPoolShareTrustline(t *testing.T) {
	b64, err := os.ReadFile("testdata/revoke_redeems_pool_shares.b64")
	require.NoError(t, err)
	var lcm xdr.LedgerCloseMeta
	require.NoError(t, xdr.SafeUnmarshalBase64(strings.TrimSpace(string(b64)), &lcm))

	for _, opMeta := range lcm.TxApplyProcessing(0).MustV4().Operations {
		for _, c := range opMeta.Changes {
			if c.Type == xdr.LedgerEntryChangeTypeLedgerEntryRemoved && c.MustRemoved().Type == xdr.LedgerEntryTypeTrustline {
				tl := c.MustRemoved().MustTrustLine()
				t.Logf("meta removes trustline of %s, asset type %s", tl.AccountId.Address(), tl.Asset.Type)
			}
		}
	}

	idx, err := NewIndexer(network.TestNetworkPassphrase, pond.NewPool(1), nil, false)
	require.NoError(t, err)
	buf := NewIndexerBuffer()
	_, err = ProcessLedger(context.Background(), network.TestNetworkPassphrase, lcm, idx, buf)
	require.NoError(t, err)
	for _, sc := range buf.GetStateChanges() {
		t.Logf("state change %s %s account=%s token=%s liquidity_pool_id=%q", sc.StateChangeCategory,
			sc.StateChangeReason, sc.AccountID, sc.TokenID.AddressBytea, sc.LiquidityPoolID.String)
	}
	for _, c := range buf.GetLiquidityPoolShareChanges() {
		t.Logf("pool-share balance change %s %s pool=%s", c.Operation, c.AccountID, c.PoolID)
	}
}
```

`go test ./internal/indexer -run TestRevocationRemovesPoolShareTrustline -count=1 -v` at 3e22515a (go1.27.4):

```
=== RUN   TestRevocationRemovesPoolShareTrustline
    revoke_pool_shares_test.go:25: meta removes trustline of GCTWYC6XEKAC5HBLNUH6NS7VA7CZNZGBU2R7FUSM2TWLNGBR367THITH, asset type AssetTypeAssetTypePoolShare
    revoke_pool_shares_test.go:36: state change BALANCE_AUTHORIZATION CLEAR account=GCTWYC6XEKAC5HBLNUH6NS7VA7CZNZGBU2R7FUSM2TWLNGBR367THITH token=CDAHTVQFV6N6UQ2UWUCVC7PPJN5JS6FS2ZETX6F2I4M35J7BW6AMRB5S liquidity_pool_id=""
    revoke_pool_shares_test.go:36: state change BALANCE DEBIT account=GAYL3ZJQZMNCRXA3HNX6WTDKUVXQDW3AS5HPZL45E4WPNILW2ARM3JBS token=CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC liquidity_pool_id=""
    revoke_pool_shares_test.go:40: pool-share balance change REMOVE GCTWYC6XEKAC5HBLNUH6NS7VA7CZNZGBU2R7FUSM2TWLNGBR367THITH pool=79d95b12eb2e5c9bc625aa5e059360ec35a1be8a7a4a7fdb90a04c8e5f814ba4
--- PASS: TestRevocationRemovesPoolShareTrustline (0.00s)
```

The meta removes the pool-share trustline, and the pool-share balance table records the removal. The state changes hold the authorization clear and the fee only.

## Expected

A `TRUSTLINE` `REMOVE` row for `GCTWYC6X...` with `liquidity_pool_id` `79d95b12...`, the same row a `ChangeTrust` removal of that trustline produces. Deriving trustline removals from the trustline entries the operation meta removes would cover this case and any other operation that removes one.

## Also found with shapekit

Found with the [shapekit](https://github.com/ErenTopaal/shapekit) replay harness, which compares what wallet-backend stores with the ledger meta. From a checkout of https://github.com/ErenTopaal/shapekit at tag v0.1.0 (Docker required; the adapter builds wallet-backend at the given commit and runs it against TimescaleDB):

```sh
cargo build --release --locked -p shapekit-cli
(cd adapters && go build -o bin/wb-adapter ./walletbackend/cmd/wb-adapter)
target/release/shapekit harness run --chain cap38.liquidity-pools.p30 --network testnet \
  --adapter "adapters/bin/wb-adapter --commit 3e22515a44979fdc3a558fda3991ca2b48e38cef"
```

The corpus entry is [`cap38.liquidity-pools.p30-revoke-redeems-pool-shares`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap38.liquidity-pools/cap38.liquidity-pools.p30-revoke-redeems-pool-shares/entry.json). The mainnet-passphrase variant gives the same result.
