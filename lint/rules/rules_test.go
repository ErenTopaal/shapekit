package rules

import (
	"regexp"
	"testing"
)

func TestAllInIDOrder(t *testing.T) {
	want := []string{"IDX001", "IDX002", "IDX003"}

	got := All()
	if len(got) != len(want) {
		t.Fatalf("got %d rules, want %d", len(got), len(want))
	}

	for i, r := range got {
		if r.ID != want[i] {
			t.Errorf("rule %d: got ID %s, want %s", i, r.ID, want[i])
		}
	}
}

func TestRulesWellFormed(t *testing.T) {
	name := regexp.MustCompile(`^[a-z0-9]+(-[a-z0-9]+)*$`)
	seen := map[string]bool{}

	for _, r := range All() {
		if seen[r.Name] {
			t.Errorf("%s: duplicate name %q", r.ID, r.Name)
		}

		seen[r.Name] = true

		if !name.MatchString(r.Name) {
			t.Errorf("%s: name %q is not kebab-case", r.ID, r.Name)
		}

		if r.Description == "" {
			t.Errorf("%s: empty description", r.ID)
		}
	}
}

func TestLookup(t *testing.T) {
	cases := map[string]string{
		"IDX001": "unvalidated-field-trusted",
		"IDX002": "missing-depth-or-size-cap",
		"IDX003": "participants-from-envelope",
	}

	for id, wantName := range cases {
		r, err := Lookup(id)
		if err != nil {
			t.Fatalf("Lookup(%s): %v", id, err)
		}

		if r.Name != wantName {
			t.Errorf("Lookup(%s).Name = %q, want %q", id, r.Name, wantName)
		}
	}
}

func TestLookupUnknown(t *testing.T) {
	for _, id := range []string{"", "IDX000", "IDX004", "idx001"} {
		if _, err := Lookup(id); err == nil {
			t.Errorf("Lookup(%q) returned no error", id)
		}
	}
}

func TestAllReturnsCopy(t *testing.T) {
	got := All()
	got[0].ID = "changed"

	if r, err := Lookup("IDX001"); err != nil || r.ID != "IDX001" {
		t.Fatalf("table changed through All(): %v %v", r, err)
	}
}
