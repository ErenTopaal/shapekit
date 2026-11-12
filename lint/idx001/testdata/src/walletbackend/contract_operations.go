// Package walletbackend holds a reduced copy of stellar/wallet-backend code
// from before the fix for wallet-backend#716 and #753.
//
// Adapted from stellar/wallet-backend (Apache-2.0,
// https://github.com/stellar/wallet-backend) at commit
// 79ea2acd1dc65a07166133cc04b12e76db314db0, file
// internal/indexer/processors/contract_operations.go, cut down to the shape
// the rule looks at. NewThreadUnsafeSet stands in for the
// github.com/deckarep/golang-set/v2 constructor of the same name.
package walletbackend

import (
	"github.com/stellar/go-stellar-sdk/ingest"
	"github.com/stellar/go-stellar-sdk/xdr"
)

type stringSet map[string]struct{}

// NewThreadUnsafeSet builds a set from items.
func NewThreadUnsafeSet(items ...string) stringSet {
	s := stringSet{}
	for _, it := range items {
		s[it] = struct{}{}
	}

	return s
}

// TransactionOperationWrapper is reduced from
// internal/indexer/processors/transaction_operation_wrapper.go.
type TransactionOperationWrapper struct {
	Transaction *ingest.LedgerTransaction
	Operation   xdr.Operation
}

// SourceAccount returns the operation's source account.
func (operation *TransactionOperationWrapper) SourceAccount() *xdr.MuxedAccount {
	if sourceAccount := operation.Operation.SourceAccount; sourceAccount != nil {
		return sourceAccount
	}

	src := operation.Transaction.Envelope.SourceAccount()

	return &src
}

// participantsForSorobanOp seeds the participants with the source as an M
// strkey when it is muxed. The database encoder cut it to 32 bytes (#716)
// and, next to the same account as G, produced a duplicate key (#753).
func participantsForSorobanOp(op *TransactionOperationWrapper) stringSet {
	return NewThreadUnsafeSet(op.SourceAccount().Address()) // want `IDX001: op.SourceAccount\(\).Address\(\) is used as a set key without a check, but it carries tx.source_account.mux_id`
}

// participantsForSorobanOpFixed is the shape after #753: the base account.
func participantsForSorobanOpFixed(op *TransactionOperationWrapper) stringSet {
	return NewThreadUnsafeSet(op.SourceAccount().ToAccountId().Address())
}

var _ = []any{participantsForSorobanOp, participantsForSorobanOpFixed}
