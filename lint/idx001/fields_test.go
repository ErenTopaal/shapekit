package idx001

import (
	"testing"

	"github.com/ErenTopaal/shapekit/lint/internal/fieldtable"
)

// the hand-written lists here name registry rows and accessors; if the
// registry renames one, these tests fail instead of the rule going quiet.
func TestTrustedPartialRowsExist(t *testing.T) {
	for id := range trustedPartial {
		f, ok := fieldtable.Lookup(id)
		if !ok {
			t.Errorf("trustedPartial names %s, which fields.toml does not have", id)

			continue
		}

		if f.ValidatedByCore != "partial" {
			t.Errorf("%s is %q in fields.toml, trustedPartial is for partial rows", id, f.ValidatedByCore)
		}
	}
}

func TestCarriersResolve(t *testing.T) {
	for c, via := range carriers {
		if _, ok := sourceRow[c]; !ok {
			t.Errorf("carrier %v: no selected row lists %v", c, via)
		}
	}

	if len(muxRows) == 0 {
		t.Error("no row lists MuxedAccount.GetId")
	}
}

func TestEveryNoRowHasAnAccessor(t *testing.T) {
	for _, f := range fieldtable.Fields {
		if selected(f) && len(f.Accessors) == 0 {
			t.Errorf("%s is selected but lists no go accessor, so IDX001 cannot see it", f.ID)
		}
	}
}
