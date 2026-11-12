// Package store is imported by the fields test package. Its exported
// builder stores its argument in a db-tagged field, which IDX001 records in
// a fact for importers.
package store

// Row is a database row.
type Row struct {
	Signer  string `db:"signer_account_id"`
	Comment string `db:"-"`
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

// WithComment keeps a note that is never written to the database.
func (b *Builder) WithComment(c string) *Builder {
	b.row.Comment = c

	return b
}
