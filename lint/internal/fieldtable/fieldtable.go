// Package fieldtable is registry/fields.toml as Go data, so IDX001 and the
// indexer-hardening guide read the same list of fields stellar-core does
// not validate. table_gen.go is generated; run go generate after editing
// the registry. A test fails when the two have drifted.
package fieldtable

//go:generate go run ./gen -in ../../../registry/fields.toml -verdicts ../../../registry/generated/field-verdicts.json -out table_gen.go

// Field is one row of fields.toml.
type Field struct {
	ID              string
	XDRPath         string
	ValidatedByCore string // "yes", "partial" or "no"
	// Measured is what core was measured to do over the row's struct fields:
	// "validated", "not-validated" or "mixed".
	Measured string
	// SettledBy is the reading of core's source that settles a measurement the
	// row does not match, empty when none is needed.
	SettledBy string
	Risk      string
	Handling  string
	Accessors []Accessor
}

// Accessor is a field or method of a go-stellar-sdk xdr type, such as
// Memo.GetText.
type Accessor struct {
	Type   string
	Member string
}

// Lookup returns the row with the given id.
func Lookup(id string) (Field, bool) {
	for _, f := range Fields {
		if f.ID == id {
			return f, true
		}
	}

	return Field{}, false
}
