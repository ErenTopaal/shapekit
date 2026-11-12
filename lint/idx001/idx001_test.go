package idx001_test

import (
	"testing"

	"golang.org/x/tools/go/analysis/analysistest"

	"github.com/ErenTopaal/shapekit/lint/idx001"
	"github.com/ErenTopaal/shapekit/lint/internal/suppress"
	"github.com/ErenTopaal/shapekit/lint/internal/testmod"
)

func TestIDX001(t *testing.T) {
	dir := analysistest.TestData()
	testmod.Download(t, dir)

	results := analysistest.Run(t, dir, idx001.Analyzer, "./src/...")

	var suppressed []suppress.Record

	for _, r := range results {
		res, ok := r.Action.Result.(*suppress.Result)
		if !ok {
			t.Fatalf("%s: result is %T, want *suppress.Result", r.Action.Package.PkgPath, r.Action.Result)
		}

		suppressed = append(suppressed, res.Suppressed...)
	}

	if len(suppressed) != 1 || suppressed[0].Function != "trustedMemo" || suppressed[0].Reason == "" {
		t.Errorf("suppressed = %+v, want one finding in trustedMemo with a reason", suppressed)
	}
}
