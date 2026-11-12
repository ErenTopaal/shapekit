// Package helpers is imported by the participants test package, so the
// facts IDX003 exports for it are what the importer sees.
package helpers

import (
	"github.com/stellar/go-stellar-sdk/ingest"
	"github.com/stellar/go-stellar-sdk/xdr"
)

// Destinations reads payment destinations from the envelope. Its name says
// nothing about participants, so it is not reported itself, but callers
// learn that its result is envelope-derived.
func Destinations(tx ingest.LedgerTransaction) []string { // want Destinations:`envelope\(xdr.PaymentOp.Destination\)`
	var out []string

	for _, op := range tx.Envelope.Operations() {
		if p, ok := op.Body.GetPaymentOp(); ok {
			out = append(out, p.Destination.ToAccountId().Address())
		}
	}

	return out
}

// Touched lists the accounts whose ledger entries the transaction changed.
func Touched(tx ingest.LedgerTransaction) ([]string, error) {
	changes, err := tx.GetChanges()
	if err != nil {
		return nil, err
	}

	var out []string

	for _, c := range changes {
		if c.Type != xdr.LedgerEntryTypeAccount {
			continue
		}

		if c.Post != nil {
			out = append(out, c.Post.Data.MustAccount().AccountId.Address())
		} else if c.Pre != nil {
			out = append(out, c.Pre.Data.MustAccount().AccountId.Address())
		}
	}

	return out, nil
}
