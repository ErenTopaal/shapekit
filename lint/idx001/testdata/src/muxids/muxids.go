// Package muxids holds seeded IDX001 cases for counters keyed by an
// unvalidated field and for mux ids used as integer keys or stored as
// integers, with their clean counterparts.
package muxids

import (
	"unicode/utf8"

	"example.com/idx001/src/store"

	"github.com/stellar/go-stellar-sdk/ingest"
	"github.com/stellar/go-stellar-sdk/xdr"
)

// memoCounts counts transactions per memo text.
func memoCounts(txs []ingest.LedgerTransaction) map[string]int {
	counts := map[string]int{}

	for _, tx := range txs {
		if t, ok := tx.Envelope.Memo().GetText(); ok {
			counts[t]++ // want `IDX001: t is used as a map key without a check, but it carries tx.memo.text`
		}
	}

	return counts
}

// memoTotals sums fees per memo text with an op-assign.
func memoTotals(txs []ingest.LedgerTransaction) map[string]int64 {
	totals := map[string]int64{}

	for _, tx := range txs {
		if t, ok := tx.Envelope.Memo().GetText(); ok {
			totals[t] += int64(tx.Envelope.Fee()) // want `IDX001: t is used as a map key without a check, but it carries tx.memo.text`
		}
	}

	return totals
}

// memoCountsChecked validates the text first.
func memoCountsChecked(txs []ingest.LedgerTransaction) map[string]int {
	counts := map[string]int{}

	for _, tx := range txs {
		if t, ok := tx.Envelope.Memo().GetText(); ok && utf8.ValidString(t) {
			counts[t]++
		}
	}

	return counts
}

// byMuxID keys sources by the mux id alone, as if it named an account.
func byMuxID(tx ingest.LedgerTransaction) map[xdr.Uint64]bool {
	seen := map[xdr.Uint64]bool{}
	src := tx.Envelope.SourceAccount()

	if src.Med25519 != nil {
		seen[src.Med25519.Id] = true // want `IDX001: src.Med25519.Id is used as a map key without a check, but it carries tx.source_account.mux_id`
	}

	return seen
}

// muxCounts counts payments per destination mux id; MustMed25519 panics
// on an account that is not muxed, so nothing here looks at the type.
func muxCounts(ops []xdr.PaymentOp) map[uint64]int {
	counts := map[uint64]int{}

	for _, op := range ops {
		counts[uint64(op.Destination.MustMed25519().Id)]++ // want `IDX001: uint64\(op.Destination.MustMed25519\(\).Id\) is used as a map key without a check, but it carries tx.source_account.mux_id`
	}

	return counts
}

// muxCountsTyped branches on the account type before reading the id.
func muxCountsTyped(ops []xdr.PaymentOp) map[uint64]int {
	counts := map[uint64]int{}

	for _, op := range ops {
		if op.Destination.Type == xdr.CryptoKeyTypeKeyTypeMuxedEd25519 {
			counts[uint64(op.Destination.MustMed25519().Id)]++
		}
	}

	return counts
}

// muxCountsOk uses the ok result of GetMed25519.
func muxCountsOk(ops []xdr.PaymentOp) map[uint64]int {
	counts := map[uint64]int{}

	for _, op := range ops {
		if med, ok := op.Destination.GetMed25519(); ok {
			counts[uint64(med.Id)]++
		}
	}

	return counts
}

type paymentRow struct {
	To    string `db:"to_account"`
	MuxID uint64 `db:"to_mux_id"`
}

// paymentRowBlind ignores the error GetId returns for an account that is
// not muxed, so every plain G destination is stored with mux id 0.
func paymentRowBlind(op xdr.PaymentOp) paymentRow {
	id, _ := op.Destination.GetId()

	return paymentRow{
		To:    op.Destination.ToAccountId().Address(),
		MuxID: id, // want `IDX001: id is stored in db field MuxID without a check, but it carries tx.source_account.mux_id`
	}
}

// paymentRowChecked stores the id only when there is one.
func paymentRowChecked(op xdr.PaymentOp) paymentRow {
	row := paymentRow{To: op.Destination.ToAccountId().Address()}

	if id, err := op.Destination.GetId(); err == nil {
		row.MuxID = id
	}

	return row
}

// muxThroughBuilder stores the id through a builder in another package.
func muxThroughBuilder(op xdr.PaymentOp) *store.Builder {
	b := &store.Builder{}
	if op.Destination.Med25519 != nil {
		b.WithMuxID(uint64(op.Destination.Med25519.Id)) // want `IDX001: uint64\(op.Destination.Med25519.Id\) is stored through WithMuxID without a check, but it carries tx.source_account.mux_id`
	}

	return b
}

// muxAddressKey keys by the M strkey that GetAddress renders.
func muxAddressKey(op xdr.PaymentOp) (map[string]bool, error) {
	seen := map[string]bool{}

	addr, err := op.Destination.GetAddress()
	if err != nil {
		return nil, err
	}

	seen[addr] = true // want `IDX001: addr is used as a map key without a check, but it carries tx.source_account.mux_id`

	return seen, nil
}

// memoIDKey keys by the memo id: any 64-bit value, but only a lossy
// conversion of a memo id is reported, not its use as a key.
func memoIDKey(tx ingest.LedgerTransaction) map[uint64]bool {
	seen := map[uint64]bool{}
	if id, ok := tx.Envelope.Memo().GetId(); ok {
		seen[uint64(id)] = true
	}

	return seen
}

var _ = []any{
	memoCounts, memoTotals, memoCountsChecked, byMuxID, muxCounts, muxCountsTyped, muxCountsOk,
	paymentRowBlind, paymentRowChecked, muxThroughBuilder, muxAddressKey, memoIDKey,
}
