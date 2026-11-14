<!-- repo: stellar/wallet-backend -->
# Comment for #801: memo edge cases checked against this branch

Some context first. For a classic payment to a `G...` address, CAP-67 copies the transaction memo into the transfer event's `to_muxed_id`: `MEMO_ID` as `u64`, `MEMO_TEXT` as `string`, and `MEMO_HASH` and `MEMO_RETURN` as `bytes` ([CAP-67 lines 269-272](https://github.com/stellar/stellar-protocol/blob/ced008c3727005ca4a775f430fe6fe346c30f58f/core/cap-0067.md#L269-L272)). On `main` ([3e22515a](https://github.com/stellar/wallet-backend/blob/3e22515a44979fdc3a558fda3991ca2b48e38cef/internal/indexer/processors/token_transfer.go#L119-L133)), `setToMuxedID` keeps only the `u64` form, so a text, hash or return memo was stored nowhere. This PR closes that gap by storing every memo on the transaction. I ran this branch's ingestion on ledgers with unusual memos to check that.

**What I checked.** Head [a9bcdb6b](https://github.com/stellar/wallet-backend/commit/a9bcdb6b0f27199705cf1b014999379ccafffe3b). I ran `ProcessLedger` on 15 ledgers recorded on a standalone stellar-core 30.0.0 network (protocol 30, testnet passphrase), each holding one payment whose memo is an edge case:

- text memos that are not valid UTF-8: a lone `ff`, a lone continuation byte `80`, overlong `c0 80`, a surrogate `ed a0 80`, a code point above U+10FFFF `f4 90 80 80`, and a 28-byte memo cut in the middle of a multi-byte sequence;
- a 28-byte text memo (the maximum);
- hash and return memos of all zero bytes, all `ff` bytes, and a SHA-256 value;
- an ordinary text and hash memo.

In every case the indexed transaction (`types.Transaction`, which `BatchCopy` writes to `transactions`) carries the memo bytes from the envelope unchanged, with the right `MemoType`. The non-UTF-8 text memos come back from the API as the schema describes:

```
=== RUN   TestMemoBytes
    memo_bytes_test.go:47: ledger 70: memo bytes 73686170656b6974206d656d6f20637574206174206279746520e282, API memoType TEXT memo "shapekit memo cut at byte �"
    memo_bytes_test.go:47: ledger 69: memo bytes f4908080, API memoType TEXT memo "�"
    memo_bytes_test.go:47: ledger 68: memo bytes eda080, API memoType TEXT memo "�"
    memo_bytes_test.go:47: ledger 67: memo bytes c080, API memoType TEXT memo "�"
    memo_bytes_test.go:47: ledger 66: memo bytes 80, API memoType TEXT memo "�"
    memo_bytes_test.go:47: ledger 65: memo bytes ff, API memoType TEXT memo "�"
--- PASS: TestMemoBytes (0.00s)
```

**One consequence of the replacement.** Five different memos (`ff`, `80`, `c0 80`, `ed a0 80`, `f4 90 80 80`) all render as the same string, a single U+FFFD. The `TEXT` memo filter compares the stored bytes with `[]byte(filter.Value)` ([utils.go line 451](https://github.com/stellar/wallet-backend/blob/a9bcdb6b0f27199705cf1b014999379ccafffe3b/internal/serve/graphql/resolvers/utils.go#L451)). A GraphQL `String` is always valid UTF-8, so none of these memos can be found with the filter, not even by passing back the value the API returned. Core accepts such memos: the six payments with non-UTF-8 text memos above all succeeded. If that matters for deposits, one option is an extra field with the raw bytes (hex or base64) for `TEXT` memos that are not valid UTF-8, plus a filter that accepts it. If it does not matter, a sentence in the filter's description would cover it.

The test I used (place in `internal/indexer/`). The ledgers are Galexie files from the [shapekit](https://github.com/ErenTopaal/shapekit) corpus: [`classic.memo-bytes.p30`](https://github.com/ErenTopaal/shapekit/tree/v0.1.0/corpus/chains/classic.memo-bytes.p30/testnet/ledgers/FFFFFFFF--0-63999) for the output above, plus [`classic.memo-arms.p30`](https://github.com/ErenTopaal/shapekit/tree/v0.1.0/corpus/chains/classic.memo-arms.p30/testnet/ledgers/FFFFFFFF--0-63999), [`classic.max-length.p30`](https://github.com/ErenTopaal/shapekit/tree/v0.1.0/corpus/chains/classic.max-length.p30/testnet/ledgers/FFFFFFFF--0-63999) and [`cap67.unified-events.p30`](https://github.com/ErenTopaal/shapekit/tree/v0.1.0/corpus/chains/cap67.unified-events.p30/testnet/ledgers/FFFFFFFF--0-63999) for the other cases.

```go
package indexer

import (
	"context"
	"encoding/hex"
	"os"
	"path/filepath"
	"testing"

	"github.com/alitto/pond/v2"
	"github.com/klauspost/compress/zstd"
	"github.com/stellar/go-stellar-sdk/network"
	"github.com/stellar/go-stellar-sdk/xdr"
	"github.com/stretchr/testify/require"

	"github.com/stellar/wallet-backend/internal/utils"
)

// SHAPEKIT_LEDGERS lists Galexie ledger files (*.xdr.zst), for example
// corpus/chains/classic.memo-bytes.p30/testnet/ledgers/FFFFFFFF--0-63999/*.xdr.zst.
func TestMemoBytes(t *testing.T) {
	paths, err := filepath.Glob(os.Getenv("SHAPEKIT_LEDGERS"))
	require.NoError(t, err)
	dec, err := zstd.NewReader(nil)
	require.NoError(t, err)
	defer dec.Close()
	for _, path := range paths {
		raw, err := os.ReadFile(path)
		require.NoError(t, err)
		plain, err := dec.DecodeAll(raw, nil)
		require.NoError(t, err)
		var batch xdr.LedgerCloseMetaBatch
		require.NoError(t, batch.UnmarshalBinary(plain))
		lcm := batch.LedgerCloseMetas[0]

		idx, err := NewIndexer(network.TestNetworkPassphrase, pond.NewPool(1), nil, false)
		require.NoError(t, err)
		buf := NewIndexerBuffer()
		_, err = ProcessLedger(context.Background(), network.TestNetworkPassphrase, lcm, idx, buf)
		require.NoError(t, err)
		for _, tx := range buf.GetTransactions() {
			if *tx.MemoType == int16(xdr.MemoTypeMemoNone) {
				continue
			}
			name, value, err := utils.DecodeMemo(*tx.MemoType, tx.Memo)
			require.NoError(t, err)
			t.Logf("ledger %d: memo bytes %s, API memoType %s memo %q", lcm.LedgerSequence(), hex.EncodeToString(tx.Memo), name, *value)
		}
	}
}
```

```sh
SHAPEKIT_LEDGERS='/path/to/shapekit/corpus/chains/classic.memo-bytes.p30/testnet/ledgers/FFFFFFFF--0-63999/*.xdr.zst' \
  go test ./internal/indexer -run '^TestMemoBytes$' -count=1 -v
```
