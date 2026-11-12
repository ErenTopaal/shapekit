// Package walletbackend holds reduced copies of stellar/wallet-backend code
// from before the IDX003 fixes, each of which the rule reports.
//
// Adapted from stellar/wallet-backend (Apache-2.0,
// https://github.com/stellar/wallet-backend), cut down to the shape the rule
// looks at. Each function names the upstream file and commit it came from.
// The upstream code uses github.com/deckarep/golang-set/v2; a small local
// set type stands in for it so the testdata needs only the sdk.
package walletbackend

import (
	"github.com/stellar/go-stellar-sdk/ingest"
	"github.com/stellar/go-stellar-sdk/xdr"
)

type stringSet map[string]struct{}

func newSet(items ...string) stringSet {
	s := stringSet{}
	for _, it := range items {
		s[it] = struct{}{}
	}

	return s
}

func (s stringSet) Add(v string) { s[v] = struct{}{} }

func (s stringSet) Union(o stringSet) stringSet {
	out := newSet()
	for k := range s {
		out[k] = struct{}{}
	}

	for k := range o {
		out[k] = struct{}{}
	}

	return out
}

// TransactionOperationWrapper is reduced from
// internal/indexer/processors/transaction_operation_wrapper.go.
type TransactionOperationWrapper struct {
	Index       uint32
	Transaction *ingest.LedgerTransaction
	Operation   xdr.Operation
	Network     string
}

// SourceAccount returns the operation's source account.
func (operation *TransactionOperationWrapper) SourceAccount() xdr.MuxedAccount { // want SourceAccount:`envelope\(xdr.Operation.SourceAccount\)`
	if sourceAccount := operation.Operation.SourceAccount; sourceAccount != nil {
		return *sourceAccount
	}

	return operation.Transaction.Envelope.SourceAccount()
}

// Changes returns the ledger entry changes the operation produced.
func (operation *TransactionOperationWrapper) Changes() ([]ingest.Change, error) {
	return operation.Transaction.GetOperationChanges(operation.Index)
}
