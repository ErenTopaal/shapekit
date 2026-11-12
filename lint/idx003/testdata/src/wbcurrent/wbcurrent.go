// Package wbcurrent holds reduced copies of stellar/wallet-backend code
// after the IDX003 fixes. None of it is reported.
//
// Adapted from stellar/wallet-backend (Apache-2.0,
// https://github.com/stellar/wallet-backend) at commit
// e967ce8b7362b79c277e700e789181d76fbb5967, cut down to the shape the rule
// looks at. The upstream code uses github.com/deckarep/golang-set/v2; a
// small local set type stands in for it.
package wbcurrent

import (
	"fmt"

	"github.com/stellar/go-stellar-sdk/ingest"
	"github.com/stellar/go-stellar-sdk/xdr"
)

type stringSet map[string]struct{}

func (s stringSet) Add(v string) { s[v] = struct{}{} }

// TransactionOperationWrapper is reduced from
// internal/indexer/processors/transaction_operation_wrapper.go.
type TransactionOperationWrapper struct {
	Index       uint32
	Transaction *ingest.LedgerTransaction
	Operation   xdr.Operation
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

// participantsForSorobanOp is reduced from
// internal/indexer/processors/contract_operations.go. Authorizers come from
// the nonce entries the host writes after a signature verified.
func participantsForSorobanOp(op *TransactionOperationWrapper, participants stringSet) error {
	participants.Add(op.SourceAccount().ToAccountId().Address())

	if op.Operation.Body.Type != xdr.OperationTypeInvokeHostFunction {
		return nil
	}

	changes, err := op.Changes()
	if err != nil {
		return fmt.Errorf("getting operation changes: %w", err)
	}

	for _, change := range changes {
		if change.Type != xdr.LedgerEntryTypeContractData || change.ChangeType != xdr.LedgerEntryChangeTypeLedgerEntryCreated || change.Post == nil {
			continue
		}

		contractData := change.Post.Data.MustContractData()
		if contractData.Key.Type != xdr.ScValTypeScvLedgerKeyNonce {
			continue
		}

		authorizer, err := contractData.Contract.String()
		if err != nil {
			return err
		}

		participants.Add(authorizer)
	}

	return nil
}

// createdContractInstances is reduced from
// internal/indexer/processors/contract_deploy.go.
func createdContractInstances(op *TransactionOperationWrapper) (map[string]struct{}, error) {
	changes, err := op.Changes()
	if err != nil {
		return nil, err
	}

	created := map[string]struct{}{}

	for _, change := range changes {
		if change.Type != xdr.LedgerEntryTypeContractData || change.ChangeType != xdr.LedgerEntryChangeTypeLedgerEntryCreated || change.Post == nil {
			continue
		}

		contractData := change.Post.Data.MustContractData()
		if contractData.Key.Type != xdr.ScValTypeScvLedgerKeyContractInstance {
			continue
		}

		id, err := contractData.Contract.String()
		if err != nil {
			return nil, err
		}

		created[id] = struct{}{}
	}

	return created, nil
}

type stateChange struct {
	account, creator string
}

type builder struct {
	sc stateChange
}

func (b *builder) WithAccount(a string) *builder { b.sc.account = a; return b }

func (b *builder) WithCreator(c string) *builder { b.sc.creator = c; return b }

// processDeploys is reduced from ProcessOperation in
// internal/indexer/processors/contract_deploy.go: a declared deployment is
// recorded only when the meta holds the created contract instance.
func processDeploys(op *TransactionOperationWrapper, contractIDOf func(xdr.ContractIdPreimageFromAddress) string) ([]stateChange, error) {
	created, err := createdContractInstances(op)
	if err != nil {
		return nil, err
	}

	var out []stateChange

	for _, auth := range op.Operation.Body.MustInvokeHostFunctionOp().Auth {
		fn := auth.RootInvocation.Function
		if fn.Type != xdr.SorobanAuthorizedFunctionTypeSorobanAuthorizedFunctionTypeCreateContractHostFn {
			continue
		}

		fromAddr := fn.MustCreateContractHostFn().ContractIdPreimage.MustFromAddress()

		id := contractIDOf(fromAddr)
		if _, ok := created[id]; !ok {
			continue
		}

		deployer, err := fromAddr.Address.String()
		if err != nil {
			return nil, err
		}

		b := &builder{}
		out = append(out, b.WithAccount(id).WithCreator(deployer).sc)
	}

	return out, nil
}

var _ = []any{participantsForSorobanOp, processDeploys}
