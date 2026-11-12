// Package fields holds seeded IDX001 cases and their clean counterparts.
package fields

import (
	"errors"
	"unicode/utf8"

	"example.com/idx001/src/store"

	"github.com/stellar/go-stellar-sdk/ingest"
	"github.com/stellar/go-stellar-sdk/xdr"
)

type stringSet map[string]struct{}

func (s stringSet) Add(v string) { s[v] = struct{}{} }

// memoIndex keys transactions by memo text, which can be any 28 bytes.
func memoIndex(txs []ingest.LedgerTransaction) map[string]int {
	byMemo := map[string]int{}

	for i, tx := range txs {
		if text, ok := tx.Envelope.Memo().GetText(); ok {
			byMemo[text] = i // want `IDX001: text is used as a map key without a check, but it carries tx.memo.text, which stellar-core validates: no`
		}
	}

	return byMemo
}

// memoIndexChecked validates the bytes first.
func memoIndexChecked(txs []ingest.LedgerTransaction) map[string]int {
	byMemo := map[string]int{}

	for i, tx := range txs {
		if text, ok := tx.Envelope.Memo().GetText(); ok && utf8.ValidString(text) {
			byMemo[text] = i
		}
	}

	return byMemo
}

type txRow struct {
	Hash   string `db:"hash"`
	Source string `db:"source_account"`
	MemoID int64  `db:"memo_id"`
}

// sourceRow persists the muxed source as is, mux id and all.
func sourceRow(tx ingest.LedgerTransaction) txRow {
	src := tx.Envelope.SourceAccount()

	return txRow{Source: src.Address()} // want `IDX001: src.Address\(\) is stored in db field Source without a check, but it carries tx.source_account.mux_id`
}

// sourceRowBase stores the base account.
func sourceRowBase(tx ingest.LedgerTransaction) txRow {
	src := tx.Envelope.SourceAccount().ToAccountId()

	return txRow{Source: src.Address()}
}

// sourceRowSwitch handles the muxed case explicitly.
func sourceRowSwitch(tx ingest.LedgerTransaction) txRow {
	src := tx.Envelope.SourceAccount()
	addr := src.Address()

	switch src.Type {
	case xdr.CryptoKeyTypeKeyTypeMuxedEd25519:
		return txRow{Source: src.ToAccountId().Address()}
	default:
		return txRow{Source: addr}
	}
}

// memoID converts an unsigned id that can exceed 2^63-1.
func memoID(tx ingest.LedgerTransaction) txRow {
	row := txRow{}
	if id, ok := tx.Envelope.Memo().GetId(); ok {
		row.MemoID = int64(id) // want `IDX001: id is converted to a signed integer without a check, but it carries tx.memo.id`
	}

	return row
}

// dataValue reads binary data as text.
func dataValue(op xdr.ManageDataOp) string {
	if op.DataValue == nil {
		return ""
	}

	return string(*op.DataValue) // want `IDX001: \*op.DataValue is converted to a string without a check, but it carries op.manage_data.data_value`
}

// participants keys a set by the muxed operation source (wallet-backend#753).
func participants(op xdr.Operation) stringSet {
	set := stringSet{}
	if op.SourceAccount != nil {
		set.Add(op.SourceAccount.Address()) // want `IDX001: op.SourceAccount.Address\(\) is used as a set key without a check, but it carries tx.source_account.mux_id`
	}

	return set
}

// signerRow stores a signer key through a builder in another package; for
// a signed payload signer the key is a P strkey of up to 165 characters.
func signerRow(op xdr.SetOptionsOp) *store.Builder {
	b := &store.Builder{}
	if op.Signer != nil {
		b.WithSigner(op.Signer.Key.Address()) // want `IDX001: op.Signer.Key.Address\(\) is stored through WithSigner without a check, but it carries signer_key.signed_payload.payload`
	}

	return b
}

// signerComment passes the key to a builder method that does not store it.
func signerComment(op xdr.SetOptionsOp) *store.Builder {
	b := &store.Builder{}
	if op.Signer != nil {
		b.WithComment(op.Signer.Key.Address())
	}

	return b
}

type authRow struct {
	Authorizer string `db:"authorizer"`
}

// declaredAuthorizers stores every address an auth entry names.
func declaredAuthorizers(op xdr.InvokeHostFunctionOp) ([]authRow, error) {
	var rows []authRow

	for _, entry := range op.Auth {
		if entry.Credentials.Type != xdr.SorobanCredentialsTypeSorobanCredentialsAddress {
			continue
		}

		addr, err := entry.Credentials.MustAddress().Address.String()
		if err != nil {
			return nil, err
		}

		rows = append(rows, authRow{Authorizer: addr}) // want `IDX001: addr is stored in db field Authorizer without a check, but it carries auth.credentials.address.address`
	}

	return rows, nil
}

var errNoMeta = errors.New("operation has no meta")

// confirmedAuthorizers keeps only the authorizers whose nonce the host
// wrote, so the declared address is checked against meta.
func confirmedAuthorizers(tx ingest.LedgerTransaction, i uint32, op xdr.InvokeHostFunctionOp) ([]authRow, error) {
	changes, err := tx.GetOperationChanges(i)
	if err != nil {
		return nil, errNoMeta
	}

	consumed := map[string]bool{}

	for _, c := range changes {
		if c.Post == nil || c.Type != xdr.LedgerEntryTypeContractData {
			continue
		}

		data := c.Post.Data.MustContractData()
		if data.Key.Type == xdr.ScValTypeScvLedgerKeyNonce {
			if id, err := data.Contract.String(); err == nil {
				consumed[id] = true
			}
		}
	}

	var rows []authRow

	for _, entry := range op.Auth {
		if entry.Credentials.Type != xdr.SorobanCredentialsTypeSorobanCredentialsAddress {
			continue
		}

		addr, err := entry.Credentials.MustAddress().Address.String()
		if err != nil {
			return nil, err
		}

		if consumed[addr] {
			rows = append(rows, authRow{Authorizer: addr})
		}
	}

	return rows, nil
}

// trustedMemo is suppressed on the line.
func trustedMemo(tx ingest.LedgerTransaction) map[string]bool {
	seen := map[string]bool{}
	if text, ok := tx.Envelope.Memo().GetText(); ok {
		//indexerlint:ignore IDX001 the key is only used for an in-memory dedupe within one ledger
		seen[text] = true
	}

	return seen
}

var _ = []any{
	memoIndex, memoIndexChecked, sourceRow, sourceRowBase, sourceRowSwitch, memoID, dataValue,
	participants, signerRow, signerComment, declaredAuthorizers, confirmedAuthorizers, trustedMemo,
}
