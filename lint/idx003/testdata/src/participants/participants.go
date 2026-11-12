// Package participants holds seeded IDX003 cases and their clean
// counterparts.
package participants

import (
	"example.com/idx003/src/helpers"

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

// operationParticipants trusts every address the operation names.
func operationParticipants(tx ingest.LedgerTransaction, i int) []xdr.AccountId {
	op := tx.Envelope.Operations()[i]
	out := []xdr.AccountId{tx.Envelope.SourceAccount().ToAccountId()}

	if op.SourceAccount != nil {
		out = append(out, op.SourceAccount.ToAccountId()) // want `IDX003: operationParticipants puts an address from xdr.Operation.SourceAccount into an account set but reads no transaction meta`
	}

	if p, ok := op.Body.GetPaymentOp(); ok {
		out = append(out, p.Destination.ToAccountId())
	}

	return out
}

// txParticipants also starts from the envelope, but adds the accounts the
// meta shows were changed, so the set reflects what happened.
func txParticipants(tx ingest.LedgerTransaction) (stringSet, error) {
	set := newSet(tx.Envelope.SourceAccount().ToAccountId().Address())

	if tx.Envelope.IsFeeBump() {
		set.Add(tx.Envelope.FeeBumpAccount().ToAccountId().Address())
	}

	for _, c := range tx.FeeChanges {
		if c.Type == xdr.LedgerEntryChangeTypeLedgerEntryUpdated {
			set.Add(c.MustUpdated().Data.MustAccount().AccountId.Address())
		}
	}

	return set, nil
}

// sponsorOf reads the operation's meta.
func sponsorOf(tx ingest.LedgerTransaction, i uint32) *xdr.AccountId {
	changes, err := tx.GetOperationChanges(i)
	if err != nil {
		return nil
	}

	for _, c := range changes {
		if c.Pre == nil && c.Post != nil {
			return c.Post.SponsoringID()
		}
	}

	return nil
}

// opParticipantsWithSponsor is clean because a callee reads meta.
func opParticipantsWithSponsor(tx ingest.LedgerTransaction, i uint32) []xdr.AccountId {
	op := tx.Envelope.Operations()[i]
	out := []xdr.AccountId{tx.Envelope.SourceAccount().ToAccountId()}

	if p, ok := op.Body.GetPaymentOp(); ok {
		out = append(out, p.Destination.ToAccountId())
	}

	if s := sponsorOf(tx, i); s != nil {
		out = append(out, *s)
	}

	return out
}

// accountsWithHelper is clean because a callee in another package reads
// meta; the analyzer learns that through a fact.
func accountsWithHelper(tx ingest.LedgerTransaction) []string {
	out := []string{tx.Envelope.SourceAccount().ToAccountId().Address()}

	touched, err := helpers.Touched(tx)
	if err != nil {
		return out
	}

	return append(out, touched...)
}

// destinationAccounts takes envelope addresses from another package.
func destinationAccounts(tx ingest.LedgerTransaction) []string {
	return helpers.Destinations(tx) // want `IDX003: destinationAccounts puts an address from xdr.PaymentOp.Destination`
}

// collect is not named for accounts, but the set it writes to is.
func collect(tx ingest.LedgerTransaction) int {
	accounts := newSet()
	for _, op := range tx.Envelope.Operations() {
		if op.SourceAccount != nil {
			accounts.Add(op.SourceAccount.Address()) // want `IDX003: collect puts an address from xdr.Operation.SourceAccount`
		}
	}

	return len(accounts)
}

type opRow struct {
	ID           int64
	Participants []string
}

// buildRow fills a participants field from the envelope.
func buildRow(op xdr.Operation, id int64) opRow {
	var src string
	if op.SourceAccount != nil {
		src = op.SourceAccount.Address()
	}

	return opRow{ID: id, Participants: []string{src}} // want `IDX003: buildRow puts an address from xdr.Operation.SourceAccount`
}

type record struct {
	account, creator string
}

func (r record) WithAccount(a string) record { r.account = a; return r }

func (r record) WithCreator(c string) record { r.creator = c; return r }

// deployRecords attributes a declared deployment to its declared deployer.
func deployRecords(op xdr.InvokeHostFunctionOp) ([]record, error) {
	var out []record

	for _, auth := range op.Auth {
		fn := auth.RootInvocation.Function
		if fn.Type != xdr.SorobanAuthorizedFunctionTypeSorobanAuthorizedFunctionTypeCreateContractHostFn {
			continue
		}

		preimage := fn.MustCreateContractHostFn().ContractIdPreimage
		if preimage.Type != xdr.ContractIdPreimageTypeContractIdPreimageFromAddress {
			continue
		}

		deployer, err := preimage.MustFromAddress().Address.String()
		if err != nil {
			return nil, err
		}

		out = append(out, record{}.WithCreator(deployer)) // want `IDX003: deployRecords puts an address from xdr.ContractIdPreimageFromAddress.Address`
	}

	return out, nil
}

// sourceAccount is a getter, not a set of participants.
func sourceAccount(tx ingest.LedgerTransaction) xdr.MuxedAccount {
	return tx.Envelope.SourceAccount()
}

// contractsToPrefetch collects contract addresses for a cache warm-up; it
// does not claim anyone took part, and its name does not say so.
func contractsToPrefetch(op xdr.InvokeHostFunctionOp) []string {
	var out []string

	if args, ok := op.HostFunction.GetInvokeContract(); ok {
		if id, err := args.ContractAddress.String(); err == nil {
			out = append(out, id)
		}
	}

	return out
}

// feeAccounts is suppressed for the whole function.
//
//indexerlint:ignore IDX003 the fee source always pays, so it always took part
func feeAccounts(tx ingest.LedgerTransaction) []string {
	return []string{tx.Envelope.SourceAccount().ToAccountId().Address()}
}

// sourceOnly is suppressed on the line.
func sourceOnly(tx ingest.LedgerTransaction) []string {
	var participants []string
	//indexerlint:ignore IDX003 the transaction source is charged the fee and bumps its sequence
	participants = append(participants, tx.Envelope.SourceAccount().ToAccountId().Address())

	return participants
}

var _ = []any{
	operationParticipants, txParticipants, opParticipantsWithSponsor, accountsWithHelper,
	destinationAccounts, collect, buildRow, deployRecords, sourceAccount, contractsToPrefetch,
	feeAccounts, sourceOnly,
}

// rebuild copies the source into a new xdr transaction, as code that
// hashes an envelope does; an xdr struct is not an account set.
func rebuild(tx ingest.LedgerTransaction) xdr.Transaction {
	out := xdr.Transaction{SourceAccount: tx.Envelope.SourceAccount()}
	out.SourceAccount = tx.Envelope.SourceAccount()

	return out
}

var _ = rebuild

// addDestination fills the caller's set from the envelope. It is not
// reported itself: whether that is right depends on the caller.
func addDestination(participants stringSet, op xdr.Operation) {
	if p, ok := op.Body.GetPaymentOp(); ok {
		participants.Add(p.Destination.ToAccountId().Address())
	}
}

// paymentParticipants calls the helper and reads no meta.
func paymentParticipants(op xdr.Operation) stringSet {
	participants := newSet()
	addDestination(participants, op) // want `IDX003: paymentParticipants puts an address from xdr.PaymentOp.Destination`

	return participants
}

// paymentParticipantsChecked calls the same helper but also reads meta.
func paymentParticipantsChecked(tx ingest.LedgerTransaction, i uint32) stringSet {
	participants := newSet()
	addDestination(participants, tx.Envelope.Operations()[i])

	if changes, err := tx.GetOperationChanges(i); err == nil {
		for _, c := range changes {
			if c.Post != nil && c.Type == xdr.LedgerEntryTypeAccount {
				participants.Add(c.Post.Data.MustAccount().AccountId.Address())
			}
		}
	}

	return participants
}

var _ = []any{paymentParticipants, paymentParticipantsChecked}
