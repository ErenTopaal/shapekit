<!-- repo: stellar/wallet-backend -->
# A text, hash or return memo of a payment to a G address is stored nowhere

Checked at `main` [3e22515a](https://github.com/stellar/wallet-backend/commit/3e22515a44979fdc3a558fda3991ca2b48e38cef) (no tagged release yet).

## What happens

For a classic operation that pays a plain `G...` account in a transaction with a memo, CAP-67 puts the memo into the transfer event's `to_muxed_id`: `MEMO_ID` as `u64`, `MEMO_TEXT` as `string`, `MEMO_HASH` and `MEMO_RETURN` as `bytes` ([CAP-67 lines 269-272](https://github.com/stellar/stellar-protocol/blob/ced008c3727005ca4a775f430fe6fe346c30f58f/core/cap-0067.md#L269-L272)).

wallet-backend keeps only the `u64` form. `setToMuxedID` returns early unless the event's content is `ttp.MuxedInfo_Id`, and its comment says "text and hash memos are not ids and are left out" ([token_transfer.go lines 119-133](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/token_transfer.go#L119-L133)). The GraphQL description of `toMuxedId` documents the same rule: "Null on debits, issuer MINT rows, fee rows and for text/hash memos" ([statechange.graphqls lines 69-79](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/serve/graphql/schema/statechange.graphqls#L69-L79)).

No other table keeps the memo. The [`transactions` table](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/db/migrations/2025-06-10.2-transactions.sql#L4-L13) has `to_id`, `hash`, `fee_charged`, `result_code`, `ledger_number`, `is_fee_bump`, `ingested_at` and `ledger_created_at`, and [`operations.operation_xdr`](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/db/migrations/2025-06-10.3-operations.sql#L20) holds the operation, not the transaction's memo. No migration under `internal/db/migrations` and no GraphQL schema file under `internal/serve/graphql/schema` has a memo field.

So when the recipient of a payment to a `G...` address tells payers apart by a text or hash memo, wallet-backend's data does not say which payer a payment came from. For a `MEMO_ID` payment or a payment to an `M...` address it does.

Severity: data-loss (the ledger carries a value that no stored row has). CAPs: CAP-0067.

## Reproduction

Two ledgers of a standalone stellar-core 30.0.0 network (protocol 30, testnet passphrase). In each, `GBJL4ZBUVYPA2UGZXRN62YIL4QGTM4YUWBAOXDPKDVOHBE6ZGI35KVB3` pays 1 XLM to `GCOCPZVHUDKGHQSQUY2T6LEAGDDQUL5J6GG2HISAWEUVJNKKYGYK7E5P`:

- ledger 77, transaction `c47a1a7e25f006a025ec5fd277394133e0c9c47bcc3081629bf181192f9f1c02`, `MEMO_TEXT` "unified events memo";
- ledger 78, transaction `8ec1408e2d5245c16447a358ab2113549de8e8b13656d4199db1fc0b674782aa`, `MEMO_HASH`.

<details>
<summary>Ledger 77 (base64), save as <code>internal/indexer/testdata/payment_memo_text.b64</code></summary>

```
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
```

</details>

<details>
<summary>Ledger 78 (base64), save as <code>internal/indexer/testdata/payment_memo_hash.b64</code></summary>

```
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
```

</details>

Test, saved as `internal/indexer/text_memo_to_muxed_id_test.go`:

```go
package indexer

import (
	"context"
	"os"
	"strings"
	"testing"

	"github.com/alitto/pond/v2"
	"github.com/stellar/go-stellar-sdk/network"
	ttp "github.com/stellar/go-stellar-sdk/processors/token_transfer"
	"github.com/stellar/go-stellar-sdk/xdr"
	"github.com/stretchr/testify/require"
)

func TestTextAndHashMemoToMuxedID(t *testing.T) {
	for _, name := range []string{"payment_memo_text", "payment_memo_hash"} {
		b64, err := os.ReadFile("testdata/" + name + ".b64")
		require.NoError(t, err)
		var lcm xdr.LedgerCloseMeta
		require.NoError(t, xdr.SafeUnmarshalBase64(strings.TrimSpace(string(b64)), &lcm))

		t.Logf("%s: transaction %s, envelope memo %s", name, lcm.TransactionHash(0).HexString(), lcm.TransactionEnvelopes()[0].Memo().Type)
		events, err := ttp.NewEventsProcessor(network.TestNetworkPassphrase).EventsFromLedger(lcm)
		require.NoError(t, err)
		for _, ev := range events {
			if tr := ev.GetTransfer(); tr != nil && ev.GetMeta().GetToMuxedInfo() != nil {
				switch c := ev.GetMeta().GetToMuxedInfo().GetContent().(type) {
				case *ttp.MuxedInfo_Text:
					t.Logf("%s: transfer event to %s, to_muxed_id text %q", name, tr.GetTo(), c.Text)
				case *ttp.MuxedInfo_Hash:
					t.Logf("%s: transfer event to %s, to_muxed_id hash %x", name, tr.GetTo(), c.Hash)
				case *ttp.MuxedInfo_Id:
					t.Logf("%s: transfer event to %s, to_muxed_id id %d", name, tr.GetTo(), c.Id)
				}
			}
		}

		idx, err := NewIndexer(network.TestNetworkPassphrase, pond.NewPool(1), nil, false)
		require.NoError(t, err)
		buf := NewIndexerBuffer()
		_, err = ProcessLedger(context.Background(), network.TestNetworkPassphrase, lcm, idx, buf)
		require.NoError(t, err)
		for _, sc := range buf.GetStateChanges() {
			t.Logf("%s: state change %s %s account=%s amount=%s to_muxed_id valid=%t %q", name, sc.StateChangeCategory,
				sc.StateChangeReason, sc.AccountID, sc.Amount.String, sc.ToMuxedID.Valid, sc.ToMuxedID.String)
		}
	}
}
```

`go test ./internal/indexer -run TestTextAndHashMemoToMuxedID -count=1 -v` at 3e22515a (go1.27.4):

```
=== RUN   TestTextAndHashMemoToMuxedID
    text_memo_to_muxed_id_test.go:23: payment_memo_text: transaction c47a1a7e25f006a025ec5fd277394133e0c9c47bcc3081629bf181192f9f1c02, envelope memo MemoTypeMemoText
    text_memo_to_muxed_id_test.go:30: payment_memo_text: transfer event to GCOCPZVHUDKGHQSQUY2T6LEAGDDQUL5J6GG2HISAWEUVJNKKYGYK7E5P, to_muxed_id text "unified events memo"
    text_memo_to_muxed_id_test.go:45: payment_memo_text: state change BALANCE DEBIT account=GBJL4ZBUVYPA2UGZXRN62YIL4QGTM4YUWBAOXDPKDVOHBE6ZGI35KVB3 amount=100 to_muxed_id valid=false ""
    text_memo_to_muxed_id_test.go:45: payment_memo_text: state change BALANCE DEBIT account=GBJL4ZBUVYPA2UGZXRN62YIL4QGTM4YUWBAOXDPKDVOHBE6ZGI35KVB3 amount=10000000 to_muxed_id valid=false ""
    text_memo_to_muxed_id_test.go:45: payment_memo_text: state change BALANCE CREDIT account=GCOCPZVHUDKGHQSQUY2T6LEAGDDQUL5J6GG2HISAWEUVJNKKYGYK7E5P amount=10000000 to_muxed_id valid=false ""
    text_memo_to_muxed_id_test.go:23: payment_memo_hash: transaction 8ec1408e2d5245c16447a358ab2113549de8e8b13656d4199db1fc0b674782aa, envelope memo MemoTypeMemoHash
    text_memo_to_muxed_id_test.go:32: payment_memo_hash: transfer event to GCOCPZVHUDKGHQSQUY2T6LEAGDDQUL5J6GG2HISAWEUVJNKKYGYK7E5P, to_muxed_id hash 908bd8b261204076aa5ca86004a619fca1aa4395916a7a0f06ae05247eb079da
    text_memo_to_muxed_id_test.go:45: payment_memo_hash: state change BALANCE DEBIT account=GBJL4ZBUVYPA2UGZXRN62YIL4QGTM4YUWBAOXDPKDVOHBE6ZGI35KVB3 amount=100 to_muxed_id valid=false ""
    text_memo_to_muxed_id_test.go:45: payment_memo_hash: state change BALANCE DEBIT account=GBJL4ZBUVYPA2UGZXRN62YIL4QGTM4YUWBAOXDPKDVOHBE6ZGI35KVB3 amount=10000000 to_muxed_id valid=false ""
    text_memo_to_muxed_id_test.go:45: payment_memo_hash: state change BALANCE CREDIT account=GCOCPZVHUDKGHQSQUY2T6LEAGDDQUL5J6GG2HISAWEUVJNKKYGYK7E5P amount=10000000 to_muxed_id valid=false ""
--- PASS: TestTextAndHashMemoToMuxedID (0.00s)
```

The transfer event carries the memo in `to_muxed_id` (as a string and as 32 bytes). The credit row of the recipient has `to_muxed_id` null in both cases, and no other row holds the memo.

## Expected

The memo of a payment to a `G...` address is stored and served. Two ways that keep the existing `toMuxedId` contract:

- store the transaction memo (type and raw bytes) on the transaction, so every operation of the transaction can be matched with it; or
- store `to_muxed_id` with its CAP-67 type (`u64`, `string` or `bytes`) on the credit row.

A `MEMO_TEXT` is up to 28 bytes and core does not check that it is valid UTF-8 (the shapekit corpus has successful payments with such memos), so storing the raw bytes and serving them alongside any decoded string keeps every memo distinguishable.

## Also found with shapekit

Found with the [shapekit](https://github.com/ErenTopaal/shapekit) replay harness, which compares what wallet-backend stores with the ledger meta. From a checkout of https://github.com/ErenTopaal/shapekit at tag v0.1.0 (Docker required; the adapter builds wallet-backend at the given commit and runs it against TimescaleDB):

```sh
cargo build --release --locked -p shapekit-cli
(cd adapters && go build -o bin/wb-adapter ./walletbackend/cmd/wb-adapter)
target/release/shapekit harness run --chain cap67.unified-events.p30 --network testnet \
  --adapter "adapters/bin/wb-adapter --commit 3e22515a44979fdc3a558fda3991ca2b48e38cef"
```

The corpus entries are [`cap67.unified-events.p30-payment-memo-text`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap67.unified-events/cap67.unified-events.p30-payment-memo-text/entry.json) and [`cap67.unified-events.p30-payment-memo-hash`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap67.unified-events/cap67.unified-events.p30-payment-memo-hash/entry.json). The harness reports the same null `to_muxed_id` for 17 entries, on both the mainnet-passphrase and testnet-passphrase variants, among them return memos (`classic.memo-arms.p30`), a 28-byte text memo (`classic.max-length.p30`) and text memos that are not valid UTF-8 (`classic.memo-bytes.p30`).
