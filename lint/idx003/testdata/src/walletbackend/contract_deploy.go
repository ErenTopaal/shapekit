package walletbackend

import (
	"fmt"

	"github.com/stellar/go-stellar-sdk/xdr"
)

// StateChange is reduced from internal/indexer/types/types.go.
type StateChange struct {
	AccountID string
	Creator   string
}

// StateChangeBuilder is reduced from
// internal/indexer/processors/state_change_builder.go.
type StateChangeBuilder struct {
	sc StateChange
}

// WithAccount sets the account the state change is filed under.
func (b *StateChangeBuilder) WithAccount(accountID string) *StateChangeBuilder {
	b.sc.AccountID = accountID

	return b
}

// WithCreator sets the deployer of a created contract.
func (b *StateChangeBuilder) WithCreator(creator string) *StateChangeBuilder {
	b.sc.Creator = creator

	return b
}

// Build returns the state change.
func (b *StateChangeBuilder) Build() StateChange {
	return b.sc
}

// ProcessOperation is reduced from
// internal/indexer/processors/contract_deploy.go at commit
// 9538b3caeaebfeb3d034146c0ce9643c3066cf8a, the parent of 80121f3a, which
// fixed the behaviour reported in wallet-backend#746. Every deployment
// declared in the auth tree is recorded as a contract creation, executed
// or not.
func ProcessOperation(op *TransactionOperationWrapper) ([]StateChange, error) { // want ProcessOperation:`envelope\(xdr.ContractIdPreimageFromAddress.Address\)`
	invokeHostOp := op.Operation.Body.MustInvokeHostFunctionOp()

	var stateChanges []StateChange

	processCreate := func(fromAddr xdr.ContractIdPreimageFromAddress) error {
		deployerAddr, err := fromAddr.Address.String()
		if err != nil {
			return fmt.Errorf("deployer address to string: %w", err)
		}

		builder := &StateChangeBuilder{}
		stateChanges = append(stateChanges, builder.WithCreator(deployerAddr).Build()) // want `IDX003: ProcessOperation puts an address from xdr.ContractIdPreimageFromAddress.Address`

		return nil
	}

	var walkInvocation func(inv xdr.SorobanAuthorizedInvocation) error
	walkInvocation = func(inv xdr.SorobanAuthorizedInvocation) error {
		if inv.Function.Type == xdr.SorobanAuthorizedFunctionTypeSorobanAuthorizedFunctionTypeCreateContractHostFn {
			cc := inv.Function.MustCreateContractHostFn()
			if cc.ContractIdPreimage.Type == xdr.ContractIdPreimageTypeContractIdPreimageFromAddress {
				if err := processCreate(cc.ContractIdPreimage.MustFromAddress()); err != nil {
					return err
				}
			}
		}

		for _, sub := range inv.SubInvocations {
			if err := walkInvocation(sub); err != nil {
				return err
			}
		}

		return nil
	}

	for _, auth := range invokeHostOp.Auth {
		if err := walkInvocation(auth.RootInvocation); err != nil {
			return nil, err
		}
	}

	return stateChanges, nil
}
