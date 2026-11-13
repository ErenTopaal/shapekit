// Package store is imported by the fields test package. Its exported
// builder stores its argument in a db-tagged field, which IDX001 records in
// a fact for importers.
package store

// Row is a database row.
type Row struct {
	Signer  string `db:"signer_account_id"`
	Comment string `db:"-"`
	MuxID   uint64 `db:"mux_id"`
}

// Builder fills a Row.
type Builder struct {
	row Row
}

// WithSigner stores the signer key.
func (b *Builder) WithSigner(signer string) *Builder { // want WithSigner:`persists\[true\]`
	b.row.Signer = signer

	return b
}

// WithMuxID stores a mux id.
func (b *Builder) WithMuxID(id uint64) *Builder { // want WithMuxID:`persists\[true\]`
	b.row.MuxID = id

	return b
}

// WithComment keeps a note that is never written to the database.
func (b *Builder) WithComment(c string) *Builder {
	b.row.Comment = c

	return b
}
