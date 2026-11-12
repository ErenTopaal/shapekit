// Package walletbackend holds reduced copies of stellar/wallet-backend code
// for IDX002.
//
// Adapted from stellar/wallet-backend (Apache-2.0,
// https://github.com/stellar/wallet-backend), cut down to the shape the rule
// looks at. Each function names the upstream file and commit it came from.
package walletbackend

import (
	"github.com/stellar/go-stellar-sdk/xdr"
)

// participantsFromInvocationAndSubInvocations is reduced from
// internal/indexer/processors/contract_operations.go at commit
// f8e5786d22c04adad16318f2e126728beec5aac9 (the parent of PR #753's merge).
// It recurses over the declared auth tree with no depth limit.
func participantsFromInvocationAndSubInvocations(invocation xdr.SorobanAuthorizedInvocation) (map[string]struct{}, error) { // want `IDX002: participantsFromInvocationAndSubInvocations recurses over xdr.SorobanAuthorizedInvocation with no depth bound`
	participants := map[string]struct{}{}

	if contractFn, ok := invocation.Function.GetContractFn(); ok {
		contractID, err := contractFn.ContractAddress.String()
		if err != nil {
			return nil, err
		}

		participants[contractID] = struct{}{}
	}

	for _, sub := range invocation.SubInvocations {
		subParticipants, err := participantsFromInvocationAndSubInvocations(sub)
		if err != nil {
			return nil, err
		}

		for p := range subParticipants {
			participants[p] = struct{}{}
		}
	}

	return participants, nil
}

// processOperation is reduced from ProcessOperation in
// internal/indexer/processors/contract_deploy.go at commit
// e967ce8b7362b79c277e700e789181d76fbb5967. The walkInvocation closure
// recurses over the auth tree with no depth limit; the decoder's depth
// limit of 1500 is the only bound.
func processOperation(op xdr.InvokeHostFunctionOp, emit func(xdr.ContractIdPreimageFromAddress)) error {
	var walkInvocation func(inv xdr.SorobanAuthorizedInvocation) error
	walkInvocation = func(inv xdr.SorobanAuthorizedInvocation) error { // want `IDX002: walkInvocation \(closure in processOperation\) recurses over xdr.SorobanAuthorizedInvocation with no depth bound`
		var preimage *xdr.ContractIdPreimage

		switch inv.Function.Type {
		case xdr.SorobanAuthorizedFunctionTypeSorobanAuthorizedFunctionTypeCreateContractHostFn:
			cc := inv.Function.MustCreateContractHostFn()
			preimage = &cc.ContractIdPreimage
		case xdr.SorobanAuthorizedFunctionTypeSorobanAuthorizedFunctionTypeCreateContractV2HostFn:
			cc := inv.Function.MustCreateContractV2HostFn()
			preimage = &cc.ContractIdPreimage
		}

		if preimage != nil && preimage.Type == xdr.ContractIdPreimageTypeContractIdPreimageFromAddress {
			emit(preimage.MustFromAddress())
		}

		for _, sub := range inv.SubInvocations {
			if err := walkInvocation(sub); err != nil {
				return err
			}
		}

		return nil
	}

	for _, auth := range op.Auth {
		if err := walkInvocation(auth.RootInvocation); err != nil {
			return err
		}
	}

	return nil
}

var _ = []any{participantsFromInvocationAndSubInvocations, processOperation}
