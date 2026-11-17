package generate

import (
	"strings"
	"testing"
)

const fields = `schema_version = 1

[[field]]
id = "a.yes"
xdr_path = "A.x"
validated_by_core = "yes"
risk = "r"
handling = "h"
go_accessors = ["xdr.A.X"]

[[field]]
id = "b.no"
xdr_path = "B.y"
validated_by_core = "no"
risk = "r"
handling = "h"
go_accessors = []

[[field]]
id = "c.partial"
xdr_path = "C.z"
validated_by_core = "partial"
risk = "r"
handling = "h"
go_accessors = []
`

// verdictsWith returns a field-verdicts.json whose rows a.yes, b.no and
// c.partial are as given (each a json object body after the id).
func verdictsWith(a, b, c string) []byte {
	return []byte(`{"schema": 1, "source": "registry/generated/validation.json", "rows": [
		{"id": "a.yes", ` + a + `},
		{"id": "b.no", ` + b + `},
		{"id": "c.partial", ` + c + `}]}`)
}

const (
	validated    = `"measured": "validated", "sites": [{"field": "A.x", "carrier": "", "verdict": "validated"}], "settled_by_source": null`
	notValidated = `"measured": "not-validated", "sites": [{"field": "B.y", "carrier": "pay", "verdict": "not-validated"}], "settled_by_source": null`
	mixed        = `"measured": "mixed", "sites": [{"field": "C.z", "carrier": "", "verdict": "validated"}, {"field": "C.w", "carrier": "", "verdict": "not-validated"}], "settled_by_source": null`
	mixedSettled = `"measured": "mixed", "sites": [], "settled_by_source": {"verdict": "partial", "reading": "src/x.cpp:1-2"}`
)

func TestRowsThatAgreeGenerate(t *testing.T) {
	out, err := Generate([]byte(fields), verdictsWith(validated, notValidated, mixedSettled))
	if err != nil {
		t.Fatal(err)
	}

	src := string(out)
	for _, want := range []string{`Measured:        "validated"`, `Measured:        "mixed"`, `SettledBy:       "src/x.cpp:1-2"`} {
		if !strings.Contains(src, want) {
			t.Errorf("generated source lacks %s:\n%s", want, src)
		}
	}
}

func TestDisagreementsFail(t *testing.T) {
	cases := []struct {
		name, a, b, c, want string
	}{
		{"measured otherwise, unsettled", notValidated, notValidated, mixedSettled, `row a.yes: validated_by_core "yes", measured not-validated`},
		{"settled otherwise", validated, `"measured": "validated", "sites": [], "settled_by_source": {"verdict": "yes", "reading": "src/y.cpp:3"}`, mixedSettled, `row b.no: validated_by_core "no", the source reading src/y.cpp:3 settles "yes"`},
		{"mixed, unsettled", validated, notValidated, mixed, "row c.partial: core was measured both ways and no source reading settles it: C.z validated; C.w not-validated"},
	}
	for _, c := range cases {
		_, err := Generate([]byte(fields), verdictsWith(c.a, c.b, c.c))
		if err == nil || !strings.Contains(err.Error(), c.want) {
			t.Errorf("%s: got %v, want an error containing %q", c.name, err, c.want)
		}
	}
}

func TestEveryRowOnceAndSorted(t *testing.T) {
	missing := []byte(`{"schema": 1, "source": "s", "rows": [{"id": "a.yes", ` + validated + `}]}`)
	if _, err := Generate([]byte(fields), missing); err == nil || !strings.Contains(err.Error(), "row b.no: field-verdicts.json has no verdict") {
		t.Errorf("missing rows: %v", err)
	}

	extra := []byte(`{"schema": 1, "source": "s", "rows": [{"id": "a.yes", ` + validated + `}, {"id": "b.no", ` + notValidated + `}, {"id": "c.partial", ` + mixedSettled + `}, {"id": "d.gone", ` + validated + `}]}`)
	if _, err := Generate([]byte(fields), extra); err == nil || !strings.Contains(err.Error(), "d.gone, which fields.toml does not have") {
		t.Errorf("extra row: %v", err)
	}

	unsorted := []byte(`{"schema": 1, "source": "s", "rows": [{"id": "b.no", ` + notValidated + `}, {"id": "a.yes", ` + validated + `}, {"id": "c.partial", ` + mixedSettled + `}]}`)
	if _, err := Generate([]byte(fields), unsorted); err == nil || !strings.Contains(err.Error(), "is not after") {
		t.Errorf("unsorted rows: %v", err)
	}

	unknown := []byte(`{"schema": 1, "source": "s", "rows": [], "later": 1}`)
	if _, err := Generate([]byte(fields), unknown); err == nil {
		t.Error("an unknown member was accepted")
	}
}
