// Package rules describes the indexerlint rules by their stable IDs.
//
// The IDs and wording match registry/lint-rules.toml so that the linter
// output and the indexer-hardening guide name each rule the same way.
package rules

import "fmt"

// Rule is the fixed description of one indexerlint rule.
type Rule struct {
	// ID is the stable identifier printed in findings and used in
	// //indexerlint:ignore comments, for example IDX001.
	ID string
	// Name is a short kebab-case label.
	Name string
	// Description is a one-line summary of what the rule reports.
	Description string
}

// all stays in this order because findings and the guide checklist list
// rules in ID order.
var all = []Rule{
	{
		ID:          "IDX001",
		Name:        "unvalidated-field-trusted",
		Description: "a transaction field that stellar-core does not validate is trusted without a check",
	},
	{
		ID:          "IDX002",
		Name:        "missing-depth-or-size-cap",
		Description: "a recursive or length-prefixed xdr structure is walked or decoded without a depth or size cap",
	},
	{
		ID:          "IDX003",
		Name:        "participants-from-envelope",
		Description: "participants are derived from the transaction envelope instead of meta changes or events",
	},
}

// All returns every rule in ID order. The slice is a copy, so callers
// cannot change the table.
func All() []Rule {
	out := make([]Rule, len(all))
	copy(out, all)

	return out
}

// Lookup returns the rule with the given ID.
func Lookup(id string) (Rule, error) {
	for _, r := range all {
		if r.ID == id {
			return r, nil
		}
	}

	return Rule{}, fmt.Errorf("unknown indexerlint rule %q", id)
}
