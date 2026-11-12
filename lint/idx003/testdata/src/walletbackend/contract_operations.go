package walletbackend

import (
	"errors"
	"fmt"

	"github.com/stellar/go-stellar-sdk/xdr"
)

var errNotSorobanOperation = errors.New("not a soroban operation")

// participantsFromInvocationAndSubInvocations is reduced from
// internal/indexer/processors/contract_operations.go at commit
// f8e5786d22c04adad16318f2e126728beec5aac9. Every contract named in the
// declared auth tree becomes a participant.
func participantsFromInvocationAndSubInvocations(invocation xdr.SorobanAuthorizedInvocation) (stringSet, error) {
	participants := newSet()

	if contractFn, ok := invocation.Function.GetContractFn(); ok {
		contractID, err := contractFn.ContractAddress.String()
		if err != nil {
			return nil, fmt.Errorf("converting contract address to string: %w", err)
		}

		participants.Add(contractID) // want `IDX003: participantsFromInvocationAndSubInvocations puts an address from xdr.InvokeContractArgs.ContractAddress`
	}

	for _, sub := range invocation.SubInvocations {
		subParticipants, err := participantsFromInvocationAndSubInvocations(sub)
		if err != nil {
			return nil, err
		}

		participants = participants.Union(subParticipants)
	}

	return participants, nil
}

// participantsForAuthEntries is reduced from the same file and commit. The
// credential address of an auth entry is taken as a participant whether or
// not the host ever matched the entry.
func participantsForAuthEntries(authEntries []xdr.SorobanAuthorizationEntry) (stringSet, error) {
	participants := newSet()

	for _, authEntry := range authEntries {
		if authEntry.Credentials.Type == xdr.SorobanCredentialsTypeSorobanCredentialsAddress {
			participant, err := authEntry.Credentials.MustAddress().Address.String()
			if err != nil {
				return nil, fmt.Errorf("converting ScAddress to string: %w", err)
			}

			participants.Add(participant) // want `IDX003: participantsForAuthEntries puts an address from xdr.SorobanAddressCredentials.Address`
		}

		invocationParticipants, err := participantsFromInvocationAndSubInvocations(authEntry.RootInvocation)
		if err != nil {
			return nil, err
		}

		participants = participants.Union(invocationParticipants)
	}

	return participants, nil
}

// InvokeContractOpProcessor is reduced from the same file and commit.
type InvokeContractOpProcessor struct {
	op *TransactionOperationWrapper
}

// Participants seeds the set with the muxed source (the #753 halt) and
// adds the auth tree.
func (p *InvokeContractOpProcessor) Participants() (stringSet, error) { // want Participants:`envelope\(xdr.InvokeContractArgs.ContractAddress\)`
	invokeContractOp, ok := p.op.Operation.Body.MustInvokeHostFunctionOp().HostFunction.GetInvokeContract()
	if !ok {
		return nil, errNotSorobanOperation
	}

	source := p.op.SourceAccount()
	participants := newSet(source.Address())

	contractID, err := invokeContractOp.ContractAddress.String()
	if err != nil {
		return nil, err
	}

	participants.Add(contractID) // want `IDX003: \(\*InvokeContractOpProcessor\).Participants puts an address from xdr.InvokeContractArgs.ContractAddress`

	authParticipants, err := participantsForAuthEntries(p.op.Operation.Body.MustInvokeHostFunctionOp().Auth)
	if err != nil {
		return nil, err
	}

	return participants.Union(authParticipants), nil
}

// participantsForSorobanOp is reduced from the same file and commit.
func participantsForSorobanOp(op *TransactionOperationWrapper) (stringSet, error) {
	if !op.Transaction.IsSorobanTx() {
		return nil, errNotSorobanOperation
	}

	source := op.SourceAccount()
	participants := newSet(source.Address())

	if op.Operation.Body.Type == xdr.OperationTypeInvokeHostFunction {
		processor := InvokeContractOpProcessor{op: op}

		opParticipants, err := processor.Participants()
		if err != nil {
			return nil, err
		}

		participants = participants.Union(opParticipants) // want `IDX003: participantsForSorobanOp puts an address from xdr.InvokeContractArgs.ContractAddress`
	}

	return participants, nil
}
