package idx003

import (
	"go/types"

	"github.com/ErenTopaal/shapekit/lint/internal/sdk"
)

type member struct {
	pkg, typ, name string
}

// envelopeAccessors are the fields and methods that hand out an address the
// transaction submitter wrote: source accounts, operation destinations and
// trustors, auth entry credentials, declared invocations and the footprint.
// None of them says that the account took part in what the ledger did.
var envelopeAccessors = map[member]bool{
	{sdk.XDR, "TransactionEnvelope", "SourceAccount"}:  true,
	{sdk.XDR, "TransactionEnvelope", "FeeBumpAccount"}: true,
	{sdk.XDR, "Transaction", "SourceAccount"}:          true,
	{sdk.XDR, "TransactionV0", "SourceAccountEd25519"}: true,
	{sdk.XDR, "FeeBumpTransaction", "FeeSource"}:       true,
	{sdk.XDR, "Operation", "SourceAccount"}:            true,

	{sdk.Ingest, "LedgerTransaction", "FeeAccount"}:        true,
	{sdk.Ingest, "LedgerTransaction", "FeeAccountAddress"}: true,
	{sdk.Ingest, "LedgerTransaction", "Account"}:           true,
	{sdk.Ingest, "LedgerTransaction", "AccountMuxed"}:      true,

	{sdk.XDR, "CreateAccountOp", "Destination"}:                 true,
	{sdk.XDR, "PaymentOp", "Destination"}:                       true,
	{sdk.XDR, "PathPaymentStrictReceiveOp", "Destination"}:      true,
	{sdk.XDR, "PathPaymentStrictSendOp", "Destination"}:         true,
	{sdk.XDR, "ClaimantV0", "Destination"}:                      true,
	{sdk.XDR, "OperationBody", "Destination"}:                   true,
	{sdk.XDR, "OperationBody", "MustDestination"}:               true,
	{sdk.XDR, "OperationBody", "GetDestination"}:                true,
	{sdk.XDR, "AllowTrustOp", "Trustor"}:                        true,
	{sdk.XDR, "SetTrustLineFlagsOp", "Trustor"}:                 true,
	{sdk.XDR, "BeginSponsoringFutureReservesOp", "SponsoredId"}: true,
	{sdk.XDR, "ClawbackOp", "From"}:                             true,
	{sdk.XDR, "RevokeSponsorshipOp", "LedgerKey"}:               true,
	{sdk.XDR, "RevokeSponsorshipOp", "Signer"}:                  true,
	{sdk.XDR, "RevokeSponsorshipOpSigner", "AccountId"}:         true,
	{sdk.XDR, "SetOptionsOp", "InflationDest"}:                  true,
	{sdk.XDR, "SetOptionsOp", "Signer"}:                         true,

	{sdk.XDR, "SorobanAddressCredentials", "Address"}:     true,
	{sdk.XDR, "SorobanDelegateSignature", "Address"}:      true,
	{sdk.XDR, "ContractIdPreimageFromAddress", "Address"}: true,
	{sdk.XDR, "InvokeContractArgs", "ContractAddress"}:    true,
	{sdk.XDR, "InvokeContractArgs", "Args"}:               true,
	{sdk.XDR, "LedgerFootprint", "ReadOnly"}:              true,
	{sdk.XDR, "LedgerFootprint", "ReadWrite"}:             true,
}

// preimageTypes are reached only through a declared host function or auth
// invocation, so any field or method of an xdr type that yields one is an
// envelope accessor. The contract ID computed from it is declared, not
// executed.
var preimageTypes = map[string]bool{
	"ContractIdPreimage":            true,
	"ContractIdPreimageFromAddress": true,
}

// accountTypes are the xdr types that name an account or address.
var accountTypes = map[string]bool{
	"AccountId":    true,
	"MuxedAccount": true,
	"ScAddress":    true,
	"PublicKey":    true,
	"SignerKey":    true,
}

// isAccount reports whether t can hold one account: a string (strkeys are
// strings) or an xdr account or address type.
func isAccount(t types.Type) bool {
	if t == nil {
		return false
	}

	t = sdk.Deref(t)

	if pkg, name, ok := sdk.NamedIn(t); ok && pkg == sdk.XDR && accountTypes[name] {
		return true
	}

	b, ok := t.Underlying().(*types.Basic)

	return ok && b.Kind() == types.String
}

// isAccountSet reports whether t is a collection of accounts: a slice,
// array or map with account keys or elements, or a generic named type such
// as set.Set[string] instantiated with an account type.
func isAccountSet(t types.Type) bool {
	for range 8 {
		if t == nil {
			return false
		}

		t = sdk.Deref(t)

		if n, ok := t.(*types.Named); ok && n.TypeArgs().Len() > 0 {
			for a := range n.TypeArgs().Types() {
				if isAccount(a) {
					return true
				}
			}
		}

		switch u := t.Underlying().(type) {
		case *types.Slice:
			if isAccount(u.Elem()) {
				return true
			}

			t = u.Elem()
		case *types.Array:
			if isAccount(u.Elem()) {
				return true
			}

			t = u.Elem()
		case *types.Map:
			return isAccount(u.Key()) || isAccount(u.Elem()) || isAccountSet(u.Elem())
		default:
			return false
		}
	}

	return false
}
